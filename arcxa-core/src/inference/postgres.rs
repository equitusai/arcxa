// graphica-core/src/inference/postgres.rs
//! PostgreSQL-specific schema inference implementation.
//!
//! Leverages pg_catalog, information_schema, and pg_stats for rich metadata.

use async_trait::async_trait;
use anyhow::{Result, Context};
use sqlx::{PgPool, Row};
use crate::inference::{types::*, traits::*};
use chrono::{DateTime, Utc};
use std::collections::HashMap;

pub struct PostgresInferrer {
    pool: PgPool,
    source_id: String,
}

impl PostgresInferrer {
    pub fn new(pool: PgPool, source_id: String) -> Self {
        Self { pool, source_id }
    }

    /// Helper: map PostgreSQL type to standard type
    fn normalize_type(pg_type: &str) -> String {
        match pg_type {
            "integer" | "int4" => "INTEGER",
            "bigint" | "int8" => "BIGINT",
            "smallint" | "int2" => "SMALLINT",
            "numeric" | "decimal" => "DECIMAL",
            "real" | "float4" => "FLOAT",
            "double precision" | "float8" => "DOUBLE",
            "character varying" | "varchar" => "VARCHAR",
            "character" | "char" => "CHAR",
            "text" => "TEXT",
            "boolean" | "bool" => "BOOLEAN",
            "timestamp without time zone" => "TIMESTAMP",
            "timestamp with time zone" | "timestamptz" => "TIMESTAMPTZ",
            "date" => "DATE",
            "jsonb" => "JSONB",
            "json" => "JSON",
            "uuid" => "UUID",
            _ => "STRING",
        }
        .to_string()
    }
}

#[async_trait]
impl BasicInference for PostgresInferrer {
    async fn list_schemas(&self) -> Result<Vec<String>> {
        let rows = sqlx::query(
            r#"
            SELECT schema_name
            FROM information_schema.schemata
            WHERE schema_name NOT IN ('pg_catalog', 'information_schema', 'pg_toast')
            ORDER BY schema_name
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.iter().map(|r| r.get(0)).collect())
    }

    async fn infer_basic_structure(&self, schema: &str) -> Result<Vec<TableMetadata>> {
        let query = r#"
            SELECT
                t.table_name,
                t.table_type,
                COALESCE(c.reltuples, 0)::bigint as est_rows,
                obj_description(c.oid) as table_comment
            FROM information_schema.tables t
            LEFT JOIN pg_class c ON c.relname = t.table_name
            LEFT JOIN pg_namespace n ON n.oid = c.relnamespace AND n.nspname = t.table_schema
            WHERE t.table_schema = $1
            ORDER BY t.table_name
        "#;

        let rows = sqlx::query(query)
            .bind(schema)
            .fetch_all(&self.pool)
            .await
            .context("Failed to fetch table list")?;

        let mut tables = Vec::new();
        for row in rows {
            let table_name: String = row.get("table_name");
            let table_type_str: String = row.get("table_type");
            let est_rows: i64 = row.get("est_rows");

            let table_type = match table_type_str.as_str() {
                "BASE TABLE" => TableType::BaseTable,
                "VIEW" => TableType::View,
                _ => TableType::BaseTable,
            };

            let columns = self.infer_columns(schema, &table_name).await?;

            tables.push(TableMetadata {
                name: table_name,
                schema: schema.to_string(),
                table_type,
                columns,
                estimated_rows: Some(est_rows as u64),
                relationships: None,
                indexes: vec![],
                constraints: vec![],
                statistics: None,
                partitioning: None,
                governance: None,
                profiling: None,
            });
        }

        Ok(tables)
    }

    async fn infer_columns(&self, schema: &str, table: &str) -> Result<Vec<ColumnMetadata>> {
        let query = r#"
            SELECT
                c.column_name,
                c.data_type,
                c.udt_name as native_type,
                c.is_nullable::boolean,
                c.ordinal_position,
                c.column_default,
                pgd.description as comment,
                CASE WHEN pk.column_name IS NOT NULL THEN true ELSE false END as is_pk
            FROM information_schema.columns c
            LEFT JOIN pg_catalog.pg_description pgd
                ON pgd.objoid = (
                    SELECT oid FROM pg_class WHERE relname = c.table_name AND relnamespace = (
                        SELECT oid FROM pg_namespace WHERE nspname = c.table_schema
                    )
                )
                AND pgd.objsubid = c.ordinal_position
            LEFT JOIN (
                SELECT ku.column_name
                FROM information_schema.table_constraints tc
                JOIN information_schema.key_column_usage ku
                    ON tc.constraint_name = ku.constraint_name
                    AND tc.table_schema = ku.table_schema
                WHERE tc.constraint_type = 'PRIMARY KEY'
                    AND tc.table_schema = $1
                    AND tc.table_name = $2
            ) pk ON pk.column_name = c.column_name
            WHERE c.table_schema = $1 AND c.table_name = $2
            ORDER BY c.ordinal_position
        "#;

        let rows = sqlx::query(query)
            .bind(schema)
            .bind(table)
            .fetch_all(&self.pool)
            .await?;

        let mut columns = Vec::new();
        for row in rows {
            let data_type: String = row.get("data_type");
            let native_type: String = row.get("native_type");
            let is_nullable_str: String = row.get("is_nullable");

            columns.push(ColumnMetadata {
                name: row.get("column_name"),
                data_type: Self::normalize_type(&data_type),
                native_type,
                nullable: is_nullable_str == "YES",
                is_primary_key: row.get("is_pk"),
                ordinal_position: row.get("ordinal_position"),
                default_value: row.get("column_default"),
                comment: row.get("comment"),
                statistics: None,
                classification: None,
                pii_detected: None,
                value_profile: None,
            });
        }

        Ok(columns)
    }

    async fn estimate_row_count(&self, schema: &str, table: &str) -> Result<u64> {
        let query = r#"
            SELECT reltuples::bigint
            FROM pg_class c
            JOIN pg_namespace n ON n.oid = c.relnamespace
            WHERE n.nspname = $1 AND c.relname = $2
        "#;

        let row = sqlx::query(query)
            .bind(schema)
            .bind(table)
            .fetch_one(&self.pool)
            .await?;

        Ok(row.get::<i64, _>(0) as u64)
    }
}

#[async_trait]
impl RelationshipInference for PostgresInferrer {
    async fn infer_foreign_keys(
        &self,
        schema: &str,
        table: &str,
    ) -> Result<Vec<ForeignKeyMetadata>> {
        let query = r#"
            SELECT
                tc.constraint_name,
                kcu.column_name,
                ccu.table_schema AS foreign_schema,
                ccu.table_name AS foreign_table,
                ccu.column_name AS foreign_column,
                rc.update_rule,
                rc.delete_rule
            FROM information_schema.table_constraints AS tc
            JOIN information_schema.key_column_usage AS kcu
                ON tc.constraint_name = kcu.constraint_name
                AND tc.table_schema = kcu.table_schema
            JOIN information_schema.constraint_column_usage AS ccu
                ON ccu.constraint_name = tc.constraint_name
                AND ccu.table_schema = tc.table_schema
            JOIN information_schema.referential_constraints AS rc
                ON rc.constraint_name = tc.constraint_name
            WHERE tc.constraint_type = 'FOREIGN KEY'
                AND tc.table_schema = $1
                AND tc.table_name = $2
            ORDER BY tc.constraint_name, kcu.ordinal_position
        "#;

        let rows = sqlx::query(query)
            .bind(schema)
            .bind(table)
            .fetch_all(&self.pool)
            .await?;

        let mut fk_map: HashMap<String, ForeignKeyMetadata> = HashMap::new();

        for row in rows {
            let constraint_name: String = row.get("constraint_name");
            let column: String = row.get("column_name");
            let foreign_column: String = row.get("foreign_column");

            let fk = fk_map.entry(constraint_name.clone()).or_insert_with(|| {
                ForeignKeyMetadata {
                    constraint_name: constraint_name.clone(),
                    columns: vec![],
                    referenced_schema: row.get("foreign_schema"),
                    referenced_table: row.get("foreign_table"),
                    referenced_columns: vec![],
                    update_rule: Self::map_referential_action(row.get("update_rule")),
                    delete_rule: Self::map_referential_action(row.get("delete_rule")),
                }
            });

            fk.columns.push(column);
            fk.referenced_columns.push(foreign_column);
        }

        Ok(fk_map.into_values().collect())
    }

    async fn infer_reverse_foreign_keys(
        &self,
        schema: &str,
        table: &str,
    ) -> Result<Vec<ForeignKeyMetadata>> {
        let query = r#"
            SELECT
                tc.constraint_name,
                kcu.column_name,
                kcu.table_schema,
                kcu.table_name,
                rc.update_rule,
                rc.delete_rule
            FROM information_schema.constraint_column_usage AS ccu
            JOIN information_schema.table_constraints AS tc
                ON ccu.constraint_name = tc.constraint_name
            JOIN information_schema.key_column_usage AS kcu
                ON tc.constraint_name = kcu.constraint_name
            JOIN information_schema.referential_constraints AS rc
                ON rc.constraint_name = tc.constraint_name
            WHERE tc.constraint_type = 'FOREIGN KEY'
                AND ccu.table_schema = $1
                AND ccu.table_name = $2
            ORDER BY tc.constraint_name
        "#;

        let rows = sqlx::query(query)
            .bind(schema)
            .bind(table)
            .fetch_all(&self.pool)
            .await?;

        let mut fk_map: HashMap<String, ForeignKeyMetadata> = HashMap::new();

        for row in rows {
            let constraint_name: String = row.get("constraint_name");
            let column: String = row.get("column_name");

            let fk = fk_map.entry(constraint_name.clone()).or_insert_with(|| {
                ForeignKeyMetadata {
                    constraint_name: constraint_name.clone(),
                    columns: vec![],
                    referenced_schema: row.get("table_schema"),
                    referenced_table: row.get("table_name"),
                    referenced_columns: vec![],
                    update_rule: Self::map_referential_action(row.get("update_rule")),
                    delete_rule: Self::map_referential_action(row.get("delete_rule")),
                }
            });

            fk.referenced_columns.push(column);
        }

        Ok(fk_map.into_values().collect())
    }

    async fn infer_indexes(&self, schema: &str, table: &str) -> Result<Vec<IndexMetadata>> {
        let query = r#"
            SELECT
                i.relname as index_name,
                am.amname as index_type,
                ix.indisunique as is_unique,
                ix.indisprimary as is_primary,
                pg_get_expr(ix.indpred, ix.indrelid) as filter_condition,
                pg_relation_size(i.oid) as size_bytes,
                ARRAY(
                    SELECT a.attname
                    FROM pg_attribute a
                    WHERE a.attrelid = t.oid
                        AND a.attnum = ANY(ix.indkey)
                    ORDER BY a.attnum
                ) as columns
            FROM pg_class t
            JOIN pg_namespace n ON n.oid = t.relnamespace
            JOIN pg_index ix ON t.oid = ix.indrelid
            JOIN pg_class i ON i.oid = ix.indexrelid
            JOIN pg_am am ON i.relam = am.oid
            WHERE n.nspname = $1 AND t.relname = $2
            ORDER BY i.relname
        "#;

        let rows = sqlx::query(query)
            .bind(schema)
            .bind(table)
            .fetch_all(&self.pool)
            .await?;

        let mut indexes = Vec::new();
        for row in rows {
            let columns: Vec<String> = row.get("columns");
            let index_columns: Vec<IndexColumn> = columns
                .into_iter()
                .enumerate()
                .map(|(i, name)| IndexColumn {
                    name,
                    ordinal: i as i32,
                    is_descending: false,
                })
                .collect();

            let index_type_str: String = row.get("index_type");
            let index_type = match index_type_str.as_str() {
                "btree" => IndexType::BTree,
                "hash" => IndexType::Hash,
                "gist" => IndexType::GiST,
                "gin" => IndexType::GIN,
                "brin" => IndexType::BRIN,
                other => IndexType::Other(other.to_string()),
            };

            indexes.push(IndexMetadata {
                name: row.get("index_name"),
                index_type,
                columns: index_columns,
                is_unique: row.get("is_unique"),
                is_primary: row.get("is_primary"),
                filter_condition: row.get("filter_condition"),
                size_bytes: row.get::<Option<i64>, _>("size_bytes").map(|v| v as u64),
            });
        }

        Ok(indexes)
    }

    async fn infer_constraints(
        &self,
        schema: &str,
        table: &str,
    ) -> Result<Vec<ConstraintMetadata>> {
        let query = r#"
            SELECT
                tc.constraint_name,
                tc.constraint_type,
                ARRAY_AGG(kcu.column_name) as columns,
                pg_get_constraintdef(
                    (SELECT oid FROM pg_constraint WHERE conname = tc.constraint_name LIMIT 1)
                ) as definition
            FROM information_schema.table_constraints tc
            LEFT JOIN information_schema.key_column_usage kcu
                ON tc.constraint_name = kcu.constraint_name
                AND tc.table_schema = kcu.table_schema
            WHERE tc.table_schema = $1 AND tc.table_name = $2
            GROUP BY tc.constraint_name, tc.constraint_type
            ORDER BY tc.constraint_name
        "#;

        let rows = sqlx::query(query)
            .bind(schema)
            .bind(table)
            .fetch_all(&self.pool)
            .await?;

        let mut constraints = Vec::new();
        for row in rows {
            let constraint_type_str: String = row.get("constraint_type");
            let constraint_type = match constraint_type_str.as_str() {
                "PRIMARY KEY" => ConstraintType::PrimaryKey,
                "FOREIGN KEY" => ConstraintType::ForeignKey,
                "UNIQUE" => ConstraintType::Unique,
                "CHECK" => ConstraintType::Check,
                _ => continue,
            };

            constraints.push(ConstraintMetadata {
                name: row.get("constraint_name"),
                constraint_type,
                columns: row.get::<Vec<String>, _>("columns"),
                definition: row.get::<Option<String>, _>("definition").unwrap_or_default(),
            });
        }

        Ok(constraints)
    }

    async fn infer_view_dependencies(&self, schema: &str, view: &str) -> Result<Vec<String>> {
        let query = r#"
            SELECT DISTINCT
                source_ns.nspname || '.' || source_table.relname as dependency
            FROM pg_depend
            JOIN pg_rewrite ON pg_depend.objid = pg_rewrite.oid
            JOIN pg_class as dependent_view ON pg_rewrite.ev_class = dependent_view.oid
            JOIN pg_class as source_table ON pg_depend.refobjid = source_table.oid
            JOIN pg_namespace dependent_ns ON dependent_ns.oid = dependent_view.relnamespace
            JOIN pg_namespace source_ns ON source_ns.oid = source_table.relnamespace
            WHERE dependent_ns.nspname = $1
                AND dependent_view.relname = $2
                AND source_ns.nspname NOT IN ('pg_catalog', 'information_schema')
            ORDER BY dependency
        "#;

        let rows = sqlx::query(query)
            .bind(schema)
            .bind(view)
            .fetch_all(&self.pool)
            .await?;

        Ok(rows.iter().map(|r| r.get(0)).collect())
    }
}

impl PostgresInferrer {
    fn map_referential_action(action: &str) -> ReferentialAction {
        match action {
            "NO ACTION" => ReferentialAction::NoAction,
            "RESTRICT" => ReferentialAction::Restrict,
            "CASCADE" => ReferentialAction::Cascade,
            "SET NULL" => ReferentialAction::SetNull,
            "SET DEFAULT" => ReferentialAction::SetDefault,
            _ => ReferentialAction::NoAction,
        }
    }
}

#[async_trait]
impl StatisticalInference for PostgresInferrer {
    async fn get_exact_row_count(&self, schema: &str, table: &str) -> Result<u64> {
        let query = format!("SELECT COUNT(*) FROM \"{}\".\"{}\"", schema, table);
        let row = sqlx::query(&query).fetch_one(&self.pool).await?;
        Ok(row.get::<i64, _>(0) as u64)
    }

    async fn infer_table_statistics(
        &self,
        schema: &str,
        table: &str,
    ) -> Result<TableStatistics> {
        let query = r#"
            SELECT
                c.reltuples::bigint as row_count,
                pg_total_relation_size(c.oid) as total_size,
                pg_relation_size(c.oid) as table_size,
                pg_indexes_size(c.oid) as index_size,
                pg_stat_get_last_analyze_time(c.oid) as last_analyzed,
                pg_stat_get_last_data_changed_time(c.oid) as last_modified
            FROM pg_class c
            JOIN pg_namespace n ON n.oid = c.relnamespace
            WHERE n.nspname = $1 AND c.relname = $2
        "#;

        let row = sqlx::query(query)
            .bind(schema)
            .bind(table)
            .fetch_one(&self.pool)
            .await?;

        let table_size: i64 = row.get("table_size");
        let index_size: i64 = row.get("index_size");

        Ok(TableStatistics {
            actual_row_count: row.get::<i64, _>("row_count") as u64,
            size_bytes: table_size as u64,
            index_size_bytes: index_size as u64,
            compression_ratio: None, // PostgreSQL doesn't track this by default
            last_analyzed: row.get("last_analyzed"),
            last_modified: row.get("last_modified"),
            read_count_daily: None,
            write_count_daily: None,
        })
    }

    async fn infer_column_statistics(
        &self,
        schema: &str,
        table: &str,
        column: &str,
    ) -> Result<ColumnStatistics> {
        // Use pg_stats view
        let query = r#"
            SELECT
                n_distinct,
                null_frac,
                avg_width
            FROM pg_stats
            WHERE schemaname = $1 AND tablename = $2 AND attname = $3
        "#;

        let row = sqlx::query(query)
            .bind(schema)
            .bind(table)
            .bind(column)
            .fetch_optional(&self.pool)
            .await?;

        if let Some(row) = row {
            let n_distinct: f32 = row.get("n_distinct");
            let null_frac: f32 = row.get("null_frac");
            let avg_width: i32 = row.get("avg_width");

            // Get total rows for null count
            let total_rows = self.estimate_row_count(schema, table).await?;
            let null_count = (total_rows as f64 * null_frac as f64) as u64;

            Ok(ColumnStatistics {
                distinct_count: if n_distinct > 0.0 {
                    Some(n_distinct as u64)
                } else {
                    None
                },
                null_count,
                null_percentage: (null_frac * 100.0) as f64,
                min_value: None, // Requires actual query
                max_value: None,
                avg_length: Some(avg_width as f64),
                histogram: None,
            })
        } else {
            Ok(ColumnStatistics {
                distinct_count: None,
                null_count: 0,
                null_percentage: 0.0,
                min_value: None,
                max_value: None,
                avg_length: None,
                histogram: None,
            })
        }
    }

    async fn infer_histogram(
        &self,
        schema: &str,
        table: &str,
        column: &str,
    ) -> Result<Option<Histogram>> {
        // PostgreSQL stores histograms in pg_stats.histogram_bounds
        // This is a simplified version - full implementation would parse the array
        Ok(None)
    }

    async fn infer_partitioning(
        &self,
        schema: &str,
        table: &str,
    ) -> Result<Option<PartitioningMetadata>> {
        let query = r#"
            SELECT
                pt.partstrat,
                ARRAY_AGG(a.attname) as partition_columns
            FROM pg_partitioned_table pt
            JOIN pg_class c ON c.oid = pt.partrelid
            JOIN pg_namespace n ON n.oid = c.relnamespace
            JOIN pg_attribute a ON a.attrelid = c.oid AND a.attnum = ANY(pt.partattrs)
            WHERE n.nspname = $1 AND c.relname = $2
            GROUP BY pt.partstrat
        "#;

        let row = sqlx::query(query)
            .bind(schema)
            .bind(table)
            .fetch_optional(&self.pool)
            .await?;

        if let Some(row) = row {
            let strat: String = row.get("partstrat");
            let strategy = match strat.as_str() {
                "r" => PartitioningStrategy::Range,
                "l" => PartitioningStrategy::List,
                "h" => PartitioningStrategy::Hash,
                _ => PartitioningStrategy::Range,
            };

            Ok(Some(PartitioningMetadata {
                strategy,
                columns: row.get("partition_columns"),
                partitions: vec![], // Would need separate query for partition details
            }))
        } else {
            Ok(None)
        }
    }

    async fn infer_storage_metrics(
        &self,
        schema: &str,
        table: &str,
    ) -> Result<(u64, u64, Option<f64>)> {
        let query = r#"
            SELECT
                pg_relation_size(c.oid) as data_size,
                pg_indexes_size(c.oid) as index_size
            FROM pg_class c
            JOIN pg_namespace n ON n.oid = c.relnamespace
            WHERE n.nspname = $1 AND c.relname = $2
        "#;

        let row = sqlx::query(query)
            .bind(schema)
            .bind(table)
            .fetch_one(&self.pool)
            .await?;

        let data_size: i64 = row.get("data_size");
        let index_size: i64 = row.get("index_size");

        Ok((data_size as u64, index_size as u64, None))
    }
}

// Tier 3 & 4 implementations would follow similar pattern
// Omitted for brevity - see next section for detector modules
