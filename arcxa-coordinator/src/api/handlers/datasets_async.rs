//! Async Dataset Import Handlers
//!
//! Background job processing for large dataset imports with profiling support.

use axum::{
    extract::{Extension, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use std::sync::Arc;
use tracing::{error, info, warn};

use crate::api::auth::Claims;
use crate::api::dto::datasets::*;
use crate::api::import_jobs::{ImportJobManager, ImportJobRequest};
use crate::api::ApiState;
use anyhow::Context;
use graphica_core::catalog::api_types::QueryResult;
use graphica_core::profiling::IncrementalProfiler;
use graphica_core::security::validate_identifier;

use super::datasets::{create_error, store_datasource_lineage, write_query_result_to_parquet};

/// Execute datasource import in background
pub async fn execute_async_import(
    state: Arc<ApiState>,
    import_id: String,
    request: ImportJobRequest,
    user_id: String,
) {
    info!("🚀 Starting async import job: {}", import_id);

    let job_manager = &state.import_job_manager;

    // Get catalog
    let catalog = match state.datasource_catalog.as_ref() {
        Some(c) => c,
        None => {
            error!("❌ Datasource catalog not available");
            job_manager.fail_job(
                &import_id,
                ImportError {
                    row: None,
                    column: None,
                    message: "Datasource catalog not configured".to_string(),
                    code: "CATALOG_UNAVAILABLE".to_string(),
                },
            );
            return;
        }
    };

    // Get datasource
    let datasource = match catalog.get_source(&request.source_id).await {
        Ok(ds) => ds,
        Err(e) => {
            error!("❌ Failed to get datasource: {}", e);
            job_manager.fail_job(
                &import_id,
                ImportError {
                    row: None,
                    column: None,
                    message: format!("Datasource not found: {}", e),
                    code: "DATASOURCE_NOT_FOUND".to_string(),
                },
            );
            return;
        }
    };

    job_manager.update_progress(&import_id, 10, 0);

    // Validate table name to prevent SQL injection
    let validated_table = match validate_identifier(&request.table) {
        Ok(table) => table,
        Err(e) => {
            error!("❌ Invalid table name '{}': {}", request.table, e);
            job_manager.fail_job(&import_id, ImportError {
                row: None,
                column: None,
                message: format!("Invalid table name '{}': {}. Table names must be alphanumeric with underscores only.", request.table, e),
                code: "INVALID_TABLE_NAME".to_string(),
            });
            return;
        }
    };

    // Validate column names to prevent SQL injection
    let validated_columns: Result<Vec<&str>, _> = request
        .columns
        .iter()
        .map(|col| validate_identifier(col))
        .collect();

    let validated_columns = match validated_columns {
        Ok(cols) => cols,
        Err(e) => {
            error!("❌ Invalid column name: {}", e);
            job_manager.fail_job(&import_id, ImportError {
                row: None,
                column: None,
                message: format!("Invalid column name: {}. Column names must be alphanumeric with underscores only.", e),
                code: "INVALID_COLUMN_NAME".to_string(),
            });
            return;
        }
    };

    // Build SQL query with validated identifiers
    let columns_clause = if validated_columns.is_empty() {
        "*".to_string()
    } else {
        validated_columns.join(", ")
    };

    // WHERE clause is temporarily disabled due to SQL injection risk
    // TODO: Implement parameterized queries or SQL parser before re-enabling
    if request.where_clause.is_some() {
        error!("❌ WHERE clause not yet supported (SQL injection risk)");
        job_manager.fail_job(&import_id, ImportError {
            row: None,
            column: None,
            message: "WHERE clauses are not yet supported due to SQL injection risk. Please use the limit parameter for now.".to_string(),
            code: "WHERE_CLAUSE_NOT_SUPPORTED".to_string(),
        });
        return;
    }

    let mut query = format!("SELECT {} FROM {}", columns_clause, validated_table);

    if let Some(limit) = request.limit {
        query.push_str(&format!(" LIMIT {}", limit));
    }

    info!("🔍 Executing query: {}", query);
    job_manager.update_progress(&import_id, 20, 0);

    // Execute query
    let query_result = match catalog
        .execute_query(
            &request.source_id,
            &query,
            std::collections::HashMap::new(),
            request.limit,
        )
        .await
    {
        Ok(result) => result,
        Err(e) => {
            error!("❌ Query execution failed: {}", e);
            job_manager.fail_job(
                &import_id,
                ImportError {
                    row: None,
                    column: None,
                    message: format!("Query execution failed: {}", e),
                    code: "QUERY_FAILED".to_string(),
                },
            );
            return;
        }
    };

    info!("✅ Query returned {} rows", query_result.row_count);
    job_manager.update_progress(&import_id, 50, query_result.row_count as u64);

    // Generate dataset ID
    let dataset_id = format!("ds_datasource_{}", generate_id());

    // Prepare storage
    let storage_path =
        std::env::var("PARQUET_PATH").unwrap_or_else(|_| "./data/parquet".to_string());

    if let Err(e) = std::fs::create_dir_all(&storage_path) {
        error!("❌ Failed to create storage directory: {}", e);
        job_manager.fail_job(
            &import_id,
            ImportError {
                row: None,
                column: None,
                message: format!("Storage error: {}", e),
                code: "STORAGE_ERROR".to_string(),
            },
        );
        return;
    }

    let parquet_path = format!("{}/{}.parquet", storage_path, dataset_id);
    job_manager.update_progress(&import_id, 60, query_result.row_count as u64);

    // Write to Parquet
    let file_size = match write_query_result_to_parquet(&query_result, &parquet_path) {
        Ok(size) => size,
        Err(e) => {
            error!("❌ Parquet write failed: {:?}", e);
            job_manager.fail_job(
                &import_id,
                ImportError {
                    row: None,
                    column: None,
                    message: "Failed to write Parquet file".to_string(),
                    code: "PARQUET_WRITE_ERROR".to_string(),
                },
            );
            return;
        }
    };

    job_manager.update_progress(&import_id, 80, query_result.row_count as u64);

    // Profile data if requested
    let profile = if request.profile {
        info!("📊 Profiling dataset with {} rows", query_result.row_count);
        Some(profile_query_result(&query_result))
    } else {
        None
    };

    job_manager.update_progress(&import_id, 90, query_result.row_count as u64);

    // Store lineage
    let default_name = format!("{}_{}", datasource.source.title, request.table);
    let dataset_name = request.name.unwrap_or(default_name);

    let lineage = ImportLineage {
        import_method: "datasource_query".to_string(),
        source_file: format!("{}:{}", request.source_id, request.table),
        imported_by: user_id.clone(),
        imported_at: Utc::now().to_rfc3339(),
        import_id: import_id.clone(),
    };

    let schema = SchemaDefinition {
        primary_key: None,
        columns: query_result
            .columns
            .as_ref()
            .map(|cols| {
                cols.iter()
                    .map(|col| ColumnDefinition {
                        name: col.name.clone(),
                        data_type: col.data_type.clone(),
                        nullable: col.nullable,
                    })
                    .collect()
            })
            .unwrap_or_else(Vec::new),
    };

    if let Err(e) = store_datasource_lineage(
        &state,
        &dataset_id,
        &import_id,
        &dataset_name,
        query_result.row_count as u64,
        &lineage,
        &schema,
        &request.source_id,
        &request.table,
        request.where_clause.as_deref(),
        "parquet",
        &parquet_path,
        file_size,
    )
    .await
    {
        warn!("⚠️  Failed to store lineage: {}", e);
    }

    // Complete job
    job_manager.complete_job(
        &import_id,
        dataset_id.clone(),
        query_result.row_count as u64,
        profile,
    );

    info!(
        "🎉 Async import complete: import_id={}, dataset_id={}",
        import_id, dataset_id
    );
}

/// Profile query result for quality metrics
fn profile_query_result(query_result: &QueryResult) -> crate::api::import_jobs::ImportProfile {
    use crate::api::import_jobs::{ColumnProfileSummary, ImportProfile};

    let columns = query_result
        .columns
        .as_ref()
        .map(|cols| cols.as_slice())
        .unwrap_or(&[]);

    let mut column_profiles = Vec::new();

    for column in columns {
        let mut profiler = IncrementalProfiler::new();

        // Profile column values
        for row in &query_result.rows {
            if let Some(value) = row.get(&column.name) {
                profiler.observe(value);
            }
        }

        let (null_count, distinct_count, top_values) = profiler.finalize();

        column_profiles.push(ColumnProfileSummary {
            name: column.name.clone(),
            null_count,
            distinct_count,
            top_values,
        });
    }

    // Calculate quality scores
    let total_records = query_result.row_count as f64;
    let completeness = if total_records > 0.0 {
        let total_nulls: u64 = column_profiles.iter().map(|c| c.null_count).sum();
        let total_cells = total_records * columns.len() as f64;
        ((total_cells - total_nulls as f64) / total_cells * 100.0) as u8
    } else {
        100
    };

    let uniqueness = if !column_profiles.is_empty() {
        let avg_distinct_ratio: f64 = column_profiles
            .iter()
            .map(|c| c.distinct_count as f64 / total_records)
            .sum::<f64>()
            / column_profiles.len() as f64;
        (avg_distinct_ratio * 100.0) as u8
    } else {
        100
    };

    let quality_score = (completeness as u16 + uniqueness as u16) / 2;

    ImportProfile {
        quality_score: quality_score as u8,
        completeness,
        validity: 100, // TODO: Add validation rules
        uniqueness,
        column_profiles,
    }
}

/// Generate unique ID
fn generate_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    format!("{}", timestamp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphica_core::catalog::api_types::ColumnDefinition as CatalogColumnDef;
    use serde_json::json;

    #[test]
    fn test_generate_id() {
        let id1 = generate_id();
        // Sleep for 2ms to ensure timestamp changes
        std::thread::sleep(std::time::Duration::from_millis(2));
        let id2 = generate_id();

        assert!(!id1.is_empty());
        assert!(!id2.is_empty());
        // IDs should be different (timestamp-based)
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_profile_query_result_empty() {
        let query_result = QueryResult {
            rows: vec![],
            row_count: 0,
            execution_time_ms: 0,
            truncated: false,
            columns: Some(vec![]),
        };

        let profile = profile_query_result(&query_result);

        assert_eq!(profile.completeness, 100); // No cells, so 100% complete
        assert_eq!(profile.uniqueness, 100); // Default for empty
        assert_eq!(profile.validity, 100);
        assert_eq!(profile.column_profiles.len(), 0);
    }

    #[test]
    fn test_profile_query_result_single_column() {
        let query_result = QueryResult {
            rows: vec![
                json!({"name": "Alice"}),
                json!({"name": "Bob"}),
                json!({"name": "Charlie"}),
                json!({"name": null}),
            ],
            row_count: 4,
            execution_time_ms: 10,
            truncated: false,
            columns: Some(vec![CatalogColumnDef {
                name: "name".to_string(),
                data_type: "VARCHAR".to_string(),
                nullable: true,
                primary_key: false,
                default_value: None,
                semantic_type: None,
                statistics: None,
            }]),
        };

        let profile = profile_query_result(&query_result);

        assert_eq!(profile.column_profiles.len(), 1);
        assert_eq!(profile.column_profiles[0].name, "name");
        assert_eq!(profile.column_profiles[0].null_count, 1);

        // Completeness: (4 cells - 1 null) / 4 cells = 75%
        assert_eq!(profile.completeness, 75);
    }

    #[test]
    fn test_profile_query_result_multiple_columns() {
        let query_result = QueryResult {
            rows: vec![
                json!({"id": 1, "name": "Alice", "email": "alice@example.com"}),
                json!({"id": 2, "name": "Bob", "email": "bob@example.com"}),
                json!({"id": 3, "name": "Charlie", "email": null}),
            ],
            row_count: 3,
            execution_time_ms: 15,
            truncated: false,
            columns: Some(vec![
                CatalogColumnDef {
                    name: "id".to_string(),
                    data_type: "INTEGER".to_string(),
                    nullable: false,
                    primary_key: true,
                    default_value: None,
                    semantic_type: None,
                    statistics: None,
                },
                CatalogColumnDef {
                    name: "name".to_string(),
                    data_type: "VARCHAR".to_string(),
                    nullable: false,
                    primary_key: false,
                    default_value: None,
                    semantic_type: None,
                    statistics: None,
                },
                CatalogColumnDef {
                    name: "email".to_string(),
                    data_type: "VARCHAR".to_string(),
                    nullable: true,
                    primary_key: false,
                    default_value: None,
                    semantic_type: None,
                    statistics: None,
                },
            ]),
        };

        let profile = profile_query_result(&query_result);

        assert_eq!(profile.column_profiles.len(), 3);

        // Find email column profile
        let email_profile = profile
            .column_profiles
            .iter()
            .find(|c| c.name == "email")
            .unwrap();
        assert_eq!(email_profile.null_count, 1);

        // Completeness: (9 cells - 1 null) / 9 cells = 88.8% -> 88
        assert!(profile.completeness >= 88);
        assert!(profile.completeness <= 89);

        // Uniqueness should reflect distinct values
        assert!(profile.uniqueness > 0);
        assert_eq!(profile.validity, 100);
    }

    #[test]
    fn test_profile_query_result_all_nulls() {
        let query_result = QueryResult {
            rows: vec![
                json!({"value": null}),
                json!({"value": null}),
                json!({"value": null}),
            ],
            row_count: 3,
            execution_time_ms: 5,
            truncated: false,
            columns: Some(vec![CatalogColumnDef {
                name: "value".to_string(),
                data_type: "VARCHAR".to_string(),
                nullable: true,
                primary_key: false,
                default_value: None,
                semantic_type: None,
                statistics: None,
            }]),
        };

        let profile = profile_query_result(&query_result);

        assert_eq!(profile.column_profiles.len(), 1);
        assert_eq!(profile.column_profiles[0].null_count, 3);
        assert_eq!(profile.completeness, 0); // All nulls = 0% complete
    }

    #[test]
    fn test_profile_query_result_no_nulls() {
        let query_result = QueryResult {
            rows: vec![json!({"id": 1}), json!({"id": 2}), json!({"id": 3})],
            row_count: 3,
            execution_time_ms: 5,
            truncated: false,
            columns: Some(vec![CatalogColumnDef {
                name: "id".to_string(),
                data_type: "INTEGER".to_string(),
                nullable: false,
                primary_key: true,
                default_value: None,
                semantic_type: None,
                statistics: None,
            }]),
        };

        let profile = profile_query_result(&query_result);

        assert_eq!(profile.column_profiles.len(), 1);
        assert_eq!(profile.column_profiles[0].null_count, 0);
        assert_eq!(profile.completeness, 100); // No nulls = 100% complete
    }

    #[test]
    fn test_profile_query_result_distinct_counts() {
        let query_result = QueryResult {
            rows: vec![
                json!({"status": "active"}),
                json!({"status": "active"}),
                json!({"status": "inactive"}),
                json!({"status": "active"}),
            ],
            row_count: 4,
            execution_time_ms: 5,
            truncated: false,
            columns: Some(vec![CatalogColumnDef {
                name: "status".to_string(),
                data_type: "VARCHAR".to_string(),
                nullable: false,
                primary_key: false,
                default_value: None,
                semantic_type: None,
                statistics: None,
            }]),
        };

        let profile = profile_query_result(&query_result);

        assert_eq!(profile.column_profiles.len(), 1);

        // Should detect 2 distinct values (active, inactive)
        // Note: IncrementalProfiler uses HyperLogLog, which is approximate
        let distinct_count = profile.column_profiles[0].distinct_count;
        assert!(
            distinct_count >= 2 && distinct_count <= 3,
            "Distinct count should be close to 2, got {}",
            distinct_count
        );

        // Uniqueness: distinct / total = 2/4 = 50%
        // May vary slightly due to HyperLogLog approximation
        assert!(profile.uniqueness >= 40 && profile.uniqueness <= 60);
    }

    #[test]
    fn test_profile_query_result_quality_score() {
        let query_result = QueryResult {
            rows: vec![
                json!({"id": 1, "value": "A"}),
                json!({"id": 2, "value": "B"}),
                json!({"id": 3, "value": null}),
            ],
            row_count: 3,
            execution_time_ms: 5,
            truncated: false,
            columns: Some(vec![
                CatalogColumnDef {
                    name: "id".to_string(),
                    data_type: "INTEGER".to_string(),
                    nullable: false,
                    primary_key: true,
                    default_value: None,
                    semantic_type: None,
                    statistics: None,
                },
                CatalogColumnDef {
                    name: "value".to_string(),
                    data_type: "VARCHAR".to_string(),
                    nullable: true,
                    primary_key: false,
                    default_value: None,
                    semantic_type: None,
                    statistics: None,
                },
            ]),
        };

        let profile = profile_query_result(&query_result);

        // Quality score is average of completeness and uniqueness
        // Completeness: (6 cells - 1 null) / 6 cells = 83%
        // Uniqueness will vary based on distinct counts
        // Quality score = (completeness + uniqueness) / 2

        assert!(profile.quality_score > 0);
        assert!(profile.quality_score <= 100);
        assert_eq!(profile.validity, 100); // Not yet implemented, returns 100
    }

    #[test]
    fn test_profile_query_result_large_dataset() {
        // Create a large dataset to test performance
        let rows: Vec<serde_json::Value> = (0..1000)
            .map(|i| json!({"id": i, "category": format!("cat_{}", i % 10)}))
            .collect();

        let query_result = QueryResult {
            rows,
            row_count: 1000,
            execution_time_ms: 100,
            truncated: false,
            columns: Some(vec![
                CatalogColumnDef {
                    name: "id".to_string(),
                    data_type: "INTEGER".to_string(),
                    nullable: false,
                    primary_key: true,
                    default_value: None,
                    semantic_type: None,
                    statistics: None,
                },
                CatalogColumnDef {
                    name: "category".to_string(),
                    data_type: "VARCHAR".to_string(),
                    nullable: false,
                    primary_key: false,
                    default_value: None,
                    semantic_type: None,
                    statistics: None,
                },
            ]),
        };

        let profile = profile_query_result(&query_result);

        assert_eq!(profile.column_profiles.len(), 2);
        assert_eq!(profile.completeness, 100); // No nulls

        // ID column should have high distinct count (1000 unique values)
        let id_profile = profile
            .column_profiles
            .iter()
            .find(|c| c.name == "id")
            .unwrap();
        assert!(
            id_profile.distinct_count > 900,
            "ID distinct count should be high"
        );

        // Category column should have ~10 distinct values
        let cat_profile = profile
            .column_profiles
            .iter()
            .find(|c| c.name == "category")
            .unwrap();
        assert!(
            cat_profile.distinct_count >= 8 && cat_profile.distinct_count <= 12,
            "Category distinct count should be ~10, got {}",
            cat_profile.distinct_count
        );
    }
}
