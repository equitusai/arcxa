//! DB Extract Callback Implementation
//!
//! Provides the callback function for workflow executor DB extract steps.
//! This allows the graphica-core workflow executor to extract data from databases
//! without creating a dependency from core to coordinator.

use anyhow::{anyhow, Context, Result};
use graphica_core::catalog::{api_types::SchemaDefinition, DataSourceCatalog};
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
            tracing::warn!("db_extract incremental filter ignored because query is provided");
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
            validated.push(validate_identifier_for_source(col, source_type)?.to_string());
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
                validate_identifier_for_source(incremental_column, source_type)?.to_string();
            if let Some(last_value) = &config.last_value {
                query.push_str(&format!(" WHERE {} > :last_value", incremental_column));
                parameters.insert("last_value".to_string(), last_value.clone());
            } else {
                tracing::warn!(
                    "db_extract incremental_column provided without last_value; skipping filter"
                );
            }
        }
    }

    Ok((query, parameters))
}

fn validate_table_identifier(identifier: &str, source_type: &str) -> Result<String> {
    let parts: Vec<&str> = identifier.split('.').collect();
    if parts.is_empty() {
        return Err(anyhow!("Table identifier cannot be empty"));
    }
    for part in &parts {
        validate_identifier_for_source(part, source_type)?;
    }
    Ok(identifier.to_string())
}

fn validate_identifier_for_source<'a>(identifier: &'a str, source_type: &str) -> Result<&'a str> {
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

        return Ok(identifier);
    }

    validate_identifier(identifier)
}

fn schema_definition_to_json(schema: &SchemaDefinition, table_name: &str) -> JsonValue {
    let normalized_table = table_name.split('.').last().unwrap_or(table_name);
    let table = schema
        .tables
        .iter()
        .find(|t| {
            t.name.eq_ignore_ascii_case(table_name) || t.name.eq_ignore_ascii_case(normalized_table)
        })
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
    } else {
        None
    }
}
