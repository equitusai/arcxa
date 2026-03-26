//! DB Extract Callback Implementation
//!
//! Provides the callback function for workflow executor DB extract steps.
//! This allows the graphica-core workflow executor to extract data from databases
//! without creating a dependency from core to coordinator.

use anyhow::{anyhow, Context, Result};
use graphica_core::catalog::{
    api_types::SchemaDefinition, connectors::databricks::DatabricksSqlClient, DataSourceCatalog,
};
use graphica_core::core::lineage::row_level::{DatabaseType, RowId};
use graphica_core::orchestration::workflow::definition::DbExtractConfig;
use graphica_core::orchestration::workflow::executor::{
    DbExtractCallback, DbExtractResult, ExecutionContext,
};
use graphica_core::security::validate_identifier;
use serde_json::Value as JsonValue;
use std::collections::{BTreeMap, HashMap};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::common::postgres::{
    quote_postgres_identifier_segment, quote_postgres_qualified_identifier,
};

/// Create a DB extract callback wired to the datasource catalog.
pub fn create_db_extract_callback(catalog: Arc<dyn DataSourceCatalog>) -> Arc<DbExtractCallback> {
    Arc::new(Box::new(
        move |config: &DbExtractConfig, context: &ExecutionContext| {
            let catalog = catalog.clone();
            let config = config.clone();
            let limits = context.resource_limits.clone();

            Box::pin(async move {
                let source_response = catalog
                    .get_source(&config.datasource_id)
                    .await
                    .with_context(|| {
                        format!(
                            "Failed to resolve datasource for db_extract: {}",
                            config.datasource_id
                        )
                    })?;

                let source_type = source_response.source.source_type.clone();
                let db_type = map_database_type(&source_type);

                let table_identifier = config
                    .table_name
                    .clone()
                    .or_else(|| config.schema_table.clone())
                    .unwrap_or_else(|| "query".to_string());

                let (query, parameters) = build_query(&config, &source_type)?;

                let limit = limits
                    .max_rows
                    .map(|max| std::cmp::min(max, config.batch_size))
                    .or_else(|| Some(config.batch_size))
                    .filter(|v| *v > 0);

                let result = catalog
                    .execute_query(&config.datasource_id, &query, parameters, limit)
                    .await
                    .with_context(|| {
                        format!(
                            "DB extract query failed for datasource {}",
                            config.datasource_id
                        )
                    })?;

                let mut rows = Vec::with_capacity(result.rows.len());
                for row in result.rows {
                    match row {
                        JsonValue::Object(map) => rows.push(map),
                        other => {
                            let mut map = serde_json::Map::new();
                            map.insert("value".to_string(), other);
                            rows.push(map);
                        }
                    }
                }

                let schema = if config.include_schema.unwrap_or(false) {
                    let schema_table = config
                        .schema_table
                        .as_deref()
                        .or(config.table_name.as_deref())
                        .ok_or_else(|| {
                            anyhow!(
                                "schema_table or table_name required when include_schema is true"
                            )
                        })?;

                    let sample_size = config.schema_sample_size.unwrap_or(1000);
                    let schema_def = catalog
                        .infer_schema(&config.datasource_id, Some(schema_table), sample_size)
                        .await
                        .with_context(|| {
                            format!(
                                "Schema inference failed for datasource {}",
                                config.datasource_id
                            )
                        })?;

                    let schema_json = schema_definition_to_json(&schema_def, schema_table);
                    Some(schema_json)
                } else {
                    None
                };

                if let Some(db_type) = db_type {
                    add_row_ids(&mut rows, db_type, &table_identifier, schema.as_ref());
                }

                Ok(DbExtractResult {
                    rows,
                    row_count: result.row_count,
                    schema,
                })
            }) as Pin<Box<dyn Future<Output = Result<DbExtractResult>> + Send>>
        },
    ))
}

fn build_query(
    config: &DbExtractConfig,
    source_type: &str,
) -> Result<(String, HashMap<String, JsonValue>)> {
    if let Some(query) = &config.query {
        if config.incremental.unwrap_or(false) {
            return Err(anyhow!(
                "incremental extraction is only supported in table mode, not custom query mode"
            ));
        }
        return Ok((query.clone(), HashMap::new()));
    }

    let table_name = config
        .table_name
        .as_deref()
        .ok_or_else(|| anyhow!("table_name required when query is not provided"))?;
    let table_name = validate_table_identifier(table_name, source_type)?;

    let select_columns = if let Some(columns) = &config.columns {
        let mut validated = Vec::with_capacity(columns.len());
        for col in columns {
            validated.push(validate_identifier_for_source(col, source_type)?);
        }
        validated.join(", ")
    } else {
        "*".to_string()
    };

    let mut query = format!("SELECT {} FROM {}", select_columns, table_name);
    let mut parameters = HashMap::new();

    if config.incremental.unwrap_or(false) {
        if let Some(incremental_column) = &config.incremental_column {
            let incremental_column =
                validate_identifier_for_source(incremental_column, source_type)?;
            if let Some(last_value) = &config.last_value {
                query.push_str(&format!(" WHERE {} > :last_value", incremental_column));
                parameters.insert("last_value".to_string(), last_value.clone());
            } else {
                return Err(anyhow!(
                    "last_value is required when incremental extraction is enabled"
                ));
            }
        }
    }

    Ok((query, parameters))
}

fn validate_table_identifier(identifier: &str, source_type: &str) -> Result<String> {
    if source_type.eq_ignore_ascii_case("Databricks") {
        return DatabricksSqlClient::quote_identifier(identifier).map_err(|error| anyhow!(error));
    }

    if source_type.eq_ignore_ascii_case("PostgreSQL") {
        return quote_postgres_qualified_identifier(identifier);
    }

    let parts: Vec<&str> = identifier.split('.').collect();
    if parts.is_empty() {
        return Err(anyhow!("Table identifier cannot be empty"));
    }
    for part in &parts {
        validate_identifier_for_source(part, source_type)?;
    }
    Ok(identifier.to_string())
}

fn validate_identifier_for_source(identifier: &str, source_type: &str) -> Result<String> {
    if source_type.eq_ignore_ascii_case("Databricks") {
        let segment = DatabricksSqlClient::sanitize_identifier_segment(identifier)
            .map_err(|error| anyhow!(error))?;
        return Ok(format!("`{segment}`"));
    }

    if source_type.eq_ignore_ascii_case("PostgreSQL") {
        return if identifier.contains('.') {
            quote_postgres_qualified_identifier(identifier)
        } else {
            quote_postgres_identifier_segment(identifier)
        };
    }

    if source_type.eq_ignore_ascii_case("Oracle") {
        if identifier.is_empty() {
            return Err(anyhow!("SQL identifier cannot be empty"));
        }

        let valid = identifier
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '$' | '#'));

        if !valid {
            return Err(anyhow!(
                "Invalid Oracle identifier '{}': must contain only alphanumeric, underscore, $, or #",
                identifier
            ));
        }

        return Ok(identifier.to_string());
    }

    Ok(validate_identifier(identifier)?.to_string())
}

fn schema_definition_to_json(schema: &SchemaDefinition, table_name: &str) -> JsonValue {
    let table = schema
        .tables
        .iter()
        .find(|t| table_identifiers_match(&t.name, table_name))
        .or_else(|| schema.tables.first());

    let fields = table
        .map(|table| {
            table
                .columns
                .iter()
                .map(|col| {
                    serde_json::json!({
                        "name": col.name,
                        "type": col.data_type,
                        "nullable": col.nullable,
                        "primary_key": col.primary_key,
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    serde_json::json!({
        "fields": fields,
    })
}

fn normalize_identifier_variants(value: &str) -> Vec<String> {
    let segments = value
        .split('.')
        .map(|segment| {
            segment
                .trim()
                .trim_matches('"')
                .trim_matches('`')
                .trim_matches('[')
                .trim_matches(']')
                .to_ascii_lowercase()
        })
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();

    if segments.is_empty() {
        return Vec::new();
    }

    let mut variants = Vec::with_capacity(segments.len());
    for start in 0..segments.len() {
        variants.push(segments[start..].join("."));
    }

    variants.sort();
    variants.dedup();
    variants
}

fn table_identifiers_match(left: &str, right: &str) -> bool {
    let left_variants = normalize_identifier_variants(left);
    let right_variants = normalize_identifier_variants(right);

    left_variants
        .iter()
        .any(|left_variant| right_variants.contains(left_variant))
}

fn add_row_ids(
    rows: &mut [serde_json::Map<String, JsonValue>],
    db_type: DatabaseType,
    table_identifier: &str,
    schema: Option<&JsonValue>,
) {
    let pk_columns = schema
        .and_then(|schema| schema.get("fields"))
        .and_then(|fields| fields.as_array())
        .map(|fields| {
            fields
                .iter()
                .filter(|field| field.get("primary_key").and_then(|v| v.as_bool()) == Some(true))
                .filter_map(|field| {
                    field
                        .get("name")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                })
                .collect::<Vec<String>>()
        })
        .unwrap_or_default();

    for (idx, row) in rows.iter_mut().enumerate() {
        let mut pk_map = BTreeMap::new();
        let mut has_all_pk = !pk_columns.is_empty();

        for col in &pk_columns {
            match row.get(col) {
                Some(value) if !value.is_null() => {
                    pk_map.insert(col.clone(), value_to_string(value));
                }
                _ => {
                    has_all_pk = false;
                    break;
                }
            }
        }

        if !has_all_pk {
            pk_map.clear();
        }

        if pk_map.is_empty() {
            pk_map.insert("_row_index".to_string(), (idx + 1).to_string());
        }

        let row_id = RowId::database(db_type, table_identifier.to_string(), pk_map);
        row.insert("_row_id".to_string(), JsonValue::String(row_id.to_key()));
    }
}

fn value_to_string(value: &JsonValue) -> String {
    match value {
        JsonValue::String(s) => s.clone(),
        JsonValue::Number(n) => n.to_string(),
        JsonValue::Bool(b) => b.to_string(),
        JsonValue::Null => "null".to_string(),
        JsonValue::Array(_) | JsonValue::Object(_) => value.to_string(),
    }
}

fn map_database_type(source_type: &str) -> Option<DatabaseType> {
    let lower = source_type.to_lowercase();
    if lower.contains("postgres") || lower.contains("edb") {
        Some(DatabaseType::Postgres)
    } else if lower.contains("db2") {
        Some(DatabaseType::DB2)
    } else if lower.contains("oracle") {
        Some(DatabaseType::Oracle)
    } else if lower.contains("hana") {
        Some(DatabaseType::SAPHANA)
    } else if lower.contains("mysql") {
        Some(DatabaseType::MySQL)
    } else if lower.contains("snowflake") {
        Some(DatabaseType::Snowflake)
    } else if lower.contains("databricks") {
        Some(DatabaseType::Databricks)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphica_core::catalog::api_types::{ColumnDefinition, TableDefinition};

    #[test]
    fn databricks_build_query_quotes_identifiers() {
        let config = DbExtractConfig {
            datasource_id: "dbx_source".to_string(),
            table_name: Some("main.bronze.customers".to_string()),
            schema_table: Some("main.bronze.customers".to_string()),
            query: None,
            incremental: Some(true),
            incremental_column: Some("updated_at".to_string()),
            last_value: Some(serde_json::json!("2026-01-01T00:00:00Z")),
            batch_size: 1_000,
            columns: Some(vec!["customer_id".to_string(), "email".to_string()]),
            include_schema: Some(true),
            schema_sample_size: Some(100),
        };

        let (query, parameters) = build_query(&config, "Databricks").unwrap();
        assert_eq!(
            query,
            "SELECT `customer_id`, `email` FROM `main`.`bronze`.`customers` WHERE `updated_at` > :last_value"
        );
        assert_eq!(
            parameters.get("last_value"),
            Some(&serde_json::json!("2026-01-01T00:00:00Z"))
        );
    }

    #[test]
    fn databricks_build_query_rejects_invalid_column_identifier() {
        let config = DbExtractConfig {
            datasource_id: "dbx_source".to_string(),
            table_name: Some("bronze.customers".to_string()),
            schema_table: None,
            query: None,
            incremental: Some(false),
            incremental_column: None,
            last_value: None,
            batch_size: 1_000,
            columns: Some(vec!["bad-column".to_string()]),
            include_schema: Some(false),
            schema_sample_size: Some(100),
        };

        let error = build_query(&config, "Databricks").unwrap_err();
        assert!(error
            .to_string()
            .contains("Invalid Databricks identifier segment"));
    }

    #[test]
    fn postgres_build_query_quotes_identifiers() {
        let config = DbExtractConfig {
            datasource_id: "pg_source".to_string(),
            table_name: Some("public.User".to_string()),
            schema_table: Some("public.User".to_string()),
            query: None,
            incremental: Some(true),
            incremental_column: Some("updatedAt".to_string()),
            last_value: Some(serde_json::json!("2026-01-01T00:00:00Z")),
            batch_size: 1_000,
            columns: Some(vec!["order".to_string(), "createdAt".to_string()]),
            include_schema: Some(false),
            schema_sample_size: Some(100),
        };

        let (query, parameters) = build_query(&config, "PostgreSQL").unwrap();
        assert_eq!(
            query,
            "SELECT \"order\", \"createdAt\" FROM \"public\".\"User\" WHERE \"updatedAt\" > :last_value"
        );
        assert_eq!(
            parameters.get("last_value"),
            Some(&serde_json::json!("2026-01-01T00:00:00Z"))
        );
    }

    #[test]
    fn build_query_rejects_incremental_custom_query_mode() {
        let config = DbExtractConfig {
            datasource_id: "pg_source".to_string(),
            table_name: None,
            schema_table: Some("public.users".to_string()),
            query: Some("SELECT * FROM users".to_string()),
            incremental: Some(true),
            incremental_column: Some("updated_at".to_string()),
            last_value: Some(serde_json::json!("2026-01-01T00:00:00Z")),
            batch_size: 1_000,
            columns: None,
            include_schema: Some(false),
            schema_sample_size: Some(100),
        };

        let error = build_query(&config, "PostgreSQL").unwrap_err().to_string();
        assert!(error.contains("table mode"));
    }

    #[test]
    fn schema_definition_to_json_matches_catalog_qualified_table_names() {
        let schema = SchemaDefinition {
            name: "main.bronze".to_string(),
            tables: vec![TableDefinition {
                name: "bronze.customers".to_string(),
                columns: vec![ColumnDefinition {
                    name: "customer_id".to_string(),
                    data_type: "INTEGER".to_string(),
                    nullable: false,
                    primary_key: true,
                    default_value: None,
                    semantic_type: None,
                    statistics: None,
                }],
                estimated_rows: None,
            }],
            relationships: Vec::new(),
            indexes: Vec::new(),
            inferred_at: chrono::Utc::now(),
        };

        let payload = schema_definition_to_json(&schema, "main.bronze.customers");
        let fields = payload["fields"].as_array().unwrap();
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0]["name"], "customer_id");
        assert_eq!(fields[0]["primary_key"], true);
    }
}
