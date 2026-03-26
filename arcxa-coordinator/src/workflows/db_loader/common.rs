use crate::etl::loaders::database::LoadMode as EtlLoadMode;
use serde_json::{Map, Value};

const WORKFLOW_ROW_METADATA_KEYS: &[&str] = &[
    "_row_id",
    "_row_index",
    "unmapped._row_id",
    "_modifications",
];

pub fn map_load_mode(mode: &str) -> EtlLoadMode {
    match mode.to_lowercase().as_str() {
        "insert" => EtlLoadMode::Insert,
        "upsert" => EtlLoadMode::Upsert,
        "replace" => EtlLoadMode::Replace,
        "append" => EtlLoadMode::Append,
        "merge" => EtlLoadMode::Merge,
        _ => EtlLoadMode::Insert,
    }
}

pub fn rows_to_records(rows: Vec<Map<String, Value>>) -> Vec<Value> {
    sanitize_rows_for_database_load(rows)
        .into_iter()
        .map(Value::Object)
        .collect()
}

pub fn batch_size_for_rows(row_count: usize) -> usize {
    if row_count > 0 {
        std::cmp::min(row_count, 10_000)
    } else {
        1_000
    }
}

pub fn sanitize_rows_for_database_load(rows: Vec<Map<String, Value>>) -> Vec<Map<String, Value>> {
    rows.into_iter()
        .map(|mut row| {
            row.retain(|key, _| !WORKFLOW_ROW_METADATA_KEYS.contains(&key.as_str()));
            row
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::sanitize_rows_for_database_load;

    #[test]
    fn strips_workflow_metadata_fields_before_database_load() {
        let rows = vec![serde_json::Map::from_iter([
            ("id".to_string(), serde_json::json!(1)),
            (
                "_row_id".to_string(),
                serde_json::json!("oracle:CRM.CUSTOMERS:customer_id=1"),
            ),
            ("_row_index".to_string(), serde_json::json!(1)),
            ("unmapped._row_id".to_string(), serde_json::json!("shadow")),
            (
                "_modifications".to_string(),
                serde_json::json!([{ "field": "name" }]),
            ),
        ])];

        let sanitized = sanitize_rows_for_database_load(rows);
        let row = sanitized.first().expect("sanitized row");

        assert_eq!(row.get("id"), Some(&serde_json::json!(1)));
        assert!(!row.contains_key("_row_id"));
        assert!(!row.contains_key("_row_index"));
        assert!(!row.contains_key("unmapped._row_id"));
        assert!(!row.contains_key("_modifications"));
    }
}
