use crate::core::lineage::row_level::{DatabaseType, RowId};

use super::WorkflowExecutor;

pub(super) fn parse_row_id_key(row_id_str: &str) -> Option<RowId> {
    let parts: Vec<&str> = row_id_str.splitn(3, ':').collect();
    if parts.len() < 3 {
        return None;
    }

    let source_type = parts[0];
    let source_id = parts[1];
    let position = parts[2];

    match source_type {
        "csv" => position
            .parse::<u64>()
            .ok()
            .map(|row_num| RowId::csv(source_id, row_num)),
        "postgres" | "db2" | "oracle" | "hana" | "saphana" | "mysql" | "snowflake"
        | "databricks" => {
            let db_type = match source_type {
                "postgres" => DatabaseType::Postgres,
                "db2" => DatabaseType::DB2,
                "oracle" => DatabaseType::Oracle,
                "hana" | "saphana" => DatabaseType::SAPHANA,
                "mysql" => DatabaseType::MySQL,
                "snowflake" => DatabaseType::Snowflake,
                "databricks" => DatabaseType::Databricks,
                _ => return None,
            };

            let mut pk_map = std::collections::BTreeMap::new();
            for pair in position.split(',') {
                let mut kv = pair.splitn(2, '=');
                let key = kv.next()?.trim();
                let value = kv.next().unwrap_or("").trim();
                if !key.is_empty() {
                    pk_map.insert(key.to_string(), value.to_string());
                }
            }

            if pk_map.is_empty() {
                return None;
            }

            Some(RowId::database(db_type, source_id.to_string(), pk_map))
        }
        "kafka" => {
            let segments: Vec<&str> = position.split(':').collect();
            if segments.len() != 2 {
                return None;
            }
            let partition = segments[0].trim_start_matches('p').parse::<i32>().ok()?;
            let offset = segments[1].trim_start_matches('o').parse::<i64>().ok()?;
            Some(RowId::kafka(source_id.to_string(), partition, offset))
        }
        "s3" => {
            let row_idx = position.trim_start_matches('r').parse::<u64>().ok()?;
            let slash_pos = source_id.find('/')?;
            let bucket = &source_id[..slash_pos];
            let key = &source_id[slash_pos + 1..];
            Some(RowId::s3(bucket.to_string(), key.to_string(), row_idx))
        }
        _ => None,
    }
}

pub(super) fn extract_materializable_rows(
    output: &serde_json::Value,
) -> Option<Vec<serde_json::Value>> {
    output
        .get("_rows")
        .and_then(|value| value.as_array())
        .or_else(|| output.get("rows").and_then(|value| value.as_array()))
        .cloned()
}

impl WorkflowExecutor {
    /// Best-effort estimate of JSON heap usage for row-oriented payloads.
    pub(super) fn estimate_json_memory(value: &serde_json::Value) -> usize {
        match value {
            serde_json::Value::Null => 8,
            serde_json::Value::Bool(_) => 8,
            serde_json::Value::Number(_) => 16,
            serde_json::Value::String(text) => 24 + text.len(),
            serde_json::Value::Array(values) => {
                24 + values.iter().map(Self::estimate_json_memory).sum::<usize>()
            }
            serde_json::Value::Object(entries) => {
                24 + entries
                    .iter()
                    .map(|(key, value)| key.len() + Self::estimate_json_memory(value))
                    .sum::<usize>()
            }
        }
    }
}
