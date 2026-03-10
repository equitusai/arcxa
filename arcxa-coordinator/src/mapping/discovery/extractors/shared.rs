//! Shared helpers for connector-backed discovery extractors.

use super::super::types::*;
use graphica_core::catalog::api_types::{QueryResult, SchemaDefinition};
use serde_json::Value;
use std::collections::HashMap;

/// Convert catalog `SchemaDefinition` into discovery `SchemaMetadata`.
pub fn schema_definition_to_metadata(schema: SchemaDefinition) -> SchemaMetadata {
    let tables = schema
        .tables
        .into_iter()
        .map(|table| TableMetadata {
            name: table.name,
            columns: table
                .columns
                .into_iter()
                .map(|column| ColumnMetadata {
                    name: column.name,
                    data_type: column.data_type,
                    nullable: column.nullable,
                    default_value: column.default_value,
                    primary_key: column.primary_key,
                })
                .collect(),
            estimated_rows: table.estimated_rows,
        })
        .collect();

    let relationships = schema
        .relationships
        .into_iter()
        .map(|rel| TableRelationshipMetadata {
            name: rel.name,
            source_table: rel.source_table,
            source_columns: rel.source_columns,
            target_table: rel.target_table,
            target_columns: rel.target_columns,
        })
        .collect();

    SchemaMetadata {
        schema_name: schema.name,
        tables,
        relationships,
    }
}

/// Convert query-result rows into discovery sample rows.
pub fn query_result_to_sample_rows(result: QueryResult) -> Vec<SampleRow> {
    result
        .rows
        .into_iter()
        .map(json_value_to_sample_row)
        .collect()
}

/// Convert a single JSON row into a sample row.
pub fn json_value_to_sample_row(row: Value) -> SampleRow {
    let mut values = HashMap::new();

    if let Value::Object(object) = row {
        for (key, value) in object {
            values.insert(key, value_to_string(value));
        }
    }

    SampleRow { values }
}

/// Parse `ColumnStats` from a `QueryResult` row with `distinct_count` and `null_fraction`.
pub fn parse_column_stats_from_query(result: &QueryResult) -> ColumnStats {
    let Some(first) = result.rows.first() else {
        return ColumnStats::default();
    };

    let Value::Object(obj) = first else {
        return ColumnStats::default();
    };

    let distinct_count = obj
        .get("distinct_count")
        .or_else(|| obj.get("DISTINCT_COUNT"))
        .and_then(parse_i64)
        .unwrap_or(0);

    let null_fraction = obj
        .get("null_fraction")
        .or_else(|| obj.get("NULL_FRACTION"))
        .and_then(parse_f64)
        .unwrap_or(0.0);

    let most_common_values = obj
        .get("most_common_values")
        .or_else(|| obj.get("MOST_COMMON_VALUES"))
        .and_then(|v| v.as_str().map(|s| s.to_string()));

    ColumnStats {
        distinct_count,
        null_fraction,
        most_common_values,
    }
}

/// Parse a fraction from `NULL_COUNT` and `TOTAL_COUNT` fallback columns.
pub fn parse_null_fraction_from_counts(result: &QueryResult) -> Option<f64> {
    let first = result.rows.first()?;
    let Value::Object(obj) = first else {
        return None;
    };

    let null_count = obj
        .get("null_count")
        .or_else(|| obj.get("NULL_COUNT"))
        .and_then(parse_f64)?;
    let total_count = obj
        .get("total_count")
        .or_else(|| obj.get("TOTAL_COUNT"))
        .and_then(parse_f64)?;

    if total_count <= 0.0 {
        return None;
    }

    Some(null_count / total_count)
}

fn parse_i64(value: &Value) -> Option<i64> {
    match value {
        Value::Number(n) => n.as_i64(),
        Value::String(s) => s.parse::<i64>().ok(),
        _ => None,
    }
}

fn parse_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse::<f64>().ok(),
        _ => None,
    }
}

fn value_to_string(value: Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(s) => s,
        other => other.to_string(),
    }
}
