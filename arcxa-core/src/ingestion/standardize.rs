//! # Data Standardization
//!
//! Normalize, clean, and standardize incoming data.

use crate::ingestion::Record;

/// Normalize a record according to standardization rules
pub fn normalize_record(mut record: Record) -> Record {
    // Trim string fields
    if let serde_json::Value::Object(ref mut map) = record.data {
        for (_key, value) in map.iter_mut() {
            if let serde_json::Value::String(s) = value {
                *value = serde_json::Value::String(s.trim().to_string());
            }
        }
    }

    // Convert field names to lowercase
    record.data = normalize_field_names(record.data);

    record
}

fn normalize_field_names(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let normalized: serde_json::Map<String, serde_json::Value> = map
                .into_iter()
                .map(|(k, v)| (k.to_lowercase(), normalize_field_names(v)))
                .collect();
            serde_json::Value::Object(normalized)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.into_iter().map(normalize_field_names).collect())
        }
        other => other,
    }
}

/// Deduplicate key generation
pub fn generate_dedup_key(record: &Record) -> String {
    // Simple hash-based key; in production use more sophisticated methods
    format!("{}:{}", record.dataset, record.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_field_names() {
        let input = serde_json::json!({
            "FirstName": "John",
            "LastName": "Doe",
            "AGE": 30
        });

        let normalized = normalize_field_names(input);
        assert!(normalized.get("firstname").is_some());
        assert!(normalized.get("lastname").is_some());
        assert!(normalized.get("age").is_some());
    }
}
