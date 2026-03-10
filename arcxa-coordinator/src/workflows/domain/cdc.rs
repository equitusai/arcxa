//! CDC (Change Data Capture) Event Parsing
//!
//! Parses Debezium CDC events from Kafka and converts them to workflow input data.
//! Supports multiple database connectors (PostgreSQL, MySQL, Oracle, DB2).

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;

/// Debezium CDC operation type
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CdcOperation {
    /// Create/Insert operation (c)
    #[serde(rename = "c")]
    Create,
    /// Update operation (u)
    #[serde(rename = "u")]
    Update,
    /// Delete operation (d)
    #[serde(rename = "d")]
    Delete,
    /// Read operation (r) - initial snapshot
    #[serde(rename = "r")]
    Read,
}

/// Debezium CDC event source metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CdcSource {
    /// Database version
    pub version: Option<String>,
    /// Connector name
    pub connector: Option<String>,
    /// Database name
    #[serde(rename = "db")]
    pub database: Option<String>,
    /// Schema name
    pub schema: Option<String>,
    /// Table name
    pub table: String,
    /// Transaction ID
    #[serde(rename = "txId")]
    pub tx_id: Option<i64>,
    /// Log sequence number (LSN) for PostgreSQL, or equivalent for other DBs
    pub lsn: Option<i64>,
    /// Timestamp in milliseconds since epoch
    #[serde(rename = "ts_ms")]
    pub timestamp_ms: Option<i64>,
}

/// Debezium CDC envelope
///
/// Standard Debezium event structure:
/// - `before`: Row state before the change (null for inserts)
/// - `after`: Row state after the change (null for deletes)
/// - `op`: Operation type (c/u/d/r)
/// - `source`: Source metadata (database, table, LSN, etc.)
/// - `ts_ms`: Event timestamp in milliseconds
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebeziumEvent {
    /// Row state before the change (null for creates/reads)
    pub before: Option<JsonValue>,

    /// Row state after the change (null for deletes)
    pub after: Option<JsonValue>,

    /// Operation type
    pub op: CdcOperation,

    /// Source metadata
    pub source: CdcSource,

    /// Event timestamp in milliseconds since epoch
    #[serde(rename = "ts_ms")]
    pub timestamp_ms: Option<i64>,

    /// Transaction metadata (optional)
    pub transaction: Option<JsonValue>,
}

impl DebeziumEvent {
    /// Parse a Debezium CDC event from JSON bytes
    pub fn from_json_bytes(bytes: &[u8]) -> Result<Self> {
        serde_json::from_slice(bytes).context("Failed to parse Debezium CDC event from JSON")
    }

    /// Parse a Debezium CDC event from JSON value
    pub fn from_json_value(value: &JsonValue) -> Result<Self> {
        serde_json::from_value(value.clone())
            .context("Failed to parse Debezium CDC event from JSON value")
    }

    /// Convert CDC event to workflow input data
    ///
    /// Creates a unified JSON object with:
    /// - `operation`: The CDC operation type (create/update/delete/read)
    /// - `before`: Previous row state (for updates/deletes)
    /// - `after`: Current row state (for creates/updates/reads)
    /// - `source`: Source metadata (database, table, etc.)
    /// - `timestamp`: Event timestamp
    /// - `changed_fields`: List of fields that changed (for updates)
    pub fn to_workflow_input(&self) -> JsonValue {
        let mut input = serde_json::json!({
            "operation": match self.op {
                CdcOperation::Create => "create",
                CdcOperation::Update => "update",
                CdcOperation::Delete => "delete",
                CdcOperation::Read => "read",
            },
            "source": {
                "database": self.source.database.as_deref().unwrap_or("unknown"),
                "schema": self.source.schema.as_deref().unwrap_or("public"),
                "table": &self.source.table,
            }
        });

        // Add before state if present
        if let Some(ref before) = self.before {
            input["before"] = before.clone();
        }

        // Add after state if present
        if let Some(ref after) = self.after {
            input["after"] = after.clone();

            // For creates/reads/updates, merge 'after' fields into top level for easy routing
            if let Some(obj) = after.as_object() {
                for (key, value) in obj {
                    input[key] = value.clone();
                }
            }
        }

        // Add timestamp
        if let Some(ts_ms) = self.timestamp_ms {
            input["timestamp_ms"] = JsonValue::Number(ts_ms.into());
        }

        // For updates, calculate changed fields
        if self.op == CdcOperation::Update {
            if let (Some(before), Some(after)) = (&self.before, &self.after) {
                let changed_fields = self.calculate_changed_fields(before, after);
                if !changed_fields.is_empty() {
                    input["changed_fields"] = JsonValue::Array(
                        changed_fields.into_iter().map(JsonValue::String).collect(),
                    );
                }
            }
        }

        input
    }

    /// Calculate which fields changed between before and after states
    fn calculate_changed_fields(&self, before: &JsonValue, after: &JsonValue) -> Vec<String> {
        let mut changed = Vec::new();

        if let (Some(before_obj), Some(after_obj)) = (before.as_object(), after.as_object()) {
            // Check all fields in 'after' state
            for (key, after_value) in after_obj {
                if let Some(before_value) = before_obj.get(key) {
                    if before_value != after_value {
                        changed.push(key.clone());
                    }
                } else {
                    // Field was added
                    changed.push(key.clone());
                }
            }

            // Check for fields that were removed
            for key in before_obj.keys() {
                if !after_obj.contains_key(key) {
                    changed.push(key.clone());
                }
            }
        }

        changed
    }

    /// Get CDC position for lineage tracking
    ///
    /// Returns a map with connector-specific position information:
    /// - PostgreSQL: LSN (log sequence number)
    /// - MySQL: binlog file + position
    /// - Oracle: SCN (system change number)
    /// - DB2: LSN
    pub fn get_cdc_position(&self) -> HashMap<String, String> {
        let mut position = HashMap::new();

        if let Some(lsn) = self.source.lsn {
            position.insert("lsn".to_string(), lsn.to_string());
        }

        if let Some(ref connector) = self.source.connector {
            position.insert("connector".to_string(), connector.clone());
        }

        if let Some(tx_id) = self.source.tx_id {
            position.insert("tx_id".to_string(), tx_id.to_string());
        }

        position.insert("table".to_string(), self.source.table.clone());

        if let Some(ref database) = self.source.database {
            position.insert("database".to_string(), database.clone());
        }

        position
    }

    /// Get fully qualified table name (database.schema.table)
    pub fn get_qualified_table_name(&self) -> String {
        let mut parts = Vec::new();

        if let Some(ref db) = self.source.database {
            parts.push(db.as_str());
        }

        if let Some(ref schema) = self.source.schema {
            parts.push(schema.as_str());
        }

        parts.push(&self.source.table);

        parts.join(".")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_create_event() {
        let json = serde_json::json!({
            "before": null,
            "after": {
                "id": 123,
                "name": "John Doe",
                "email": "john@example.com"
            },
            "op": "c",
            "source": {
                "version": "2.1.0",
                "connector": "postgresql",
                "db": "testdb",
                "schema": "public",
                "table": "customers",
                "txId": 567,
                "lsn": 98765,
                "ts_ms": 1698765432000i64
            },
            "ts_ms": 1698765432000i64
        });

        let event = DebeziumEvent::from_json_value(&json).unwrap();

        assert_eq!(event.op, CdcOperation::Create);
        assert!(event.before.is_none());
        assert!(event.after.is_some());
        assert_eq!(event.source.table, "customers");
        assert_eq!(event.source.database, Some("testdb".to_string()));
    }

    #[test]
    fn test_parse_update_event() {
        let json = serde_json::json!({
            "before": {
                "id": 123,
                "name": "John Doe",
                "email": "john@example.com"
            },
            "after": {
                "id": 123,
                "name": "John Doe",
                "email": "john.doe@example.com"
            },
            "op": "u",
            "source": {
                "table": "customers"
            },
            "ts_ms": 1698765432000i64
        });

        let event = DebeziumEvent::from_json_value(&json).unwrap();

        assert_eq!(event.op, CdcOperation::Update);
        assert!(event.before.is_some());
        assert!(event.after.is_some());
    }

    #[test]
    fn test_to_workflow_input_create() {
        let event = DebeziumEvent {
            before: None,
            after: Some(serde_json::json!({
                "id": 123,
                "name": "John Doe",
                "email": "john@example.com"
            })),
            op: CdcOperation::Create,
            source: CdcSource {
                version: None,
                connector: Some("postgresql".to_string()),
                database: Some("testdb".to_string()),
                schema: Some("public".to_string()),
                table: "customers".to_string(),
                tx_id: None,
                lsn: Some(12345),
                timestamp_ms: Some(1698765432000i64),
            },
            timestamp_ms: Some(1698765432000i64),
            transaction: None,
        };

        let input = event.to_workflow_input();

        assert_eq!(input["operation"], "create");
        assert_eq!(input["id"], 123);
        assert_eq!(input["name"], "John Doe");
        assert_eq!(input["email"], "john@example.com");
        assert_eq!(input["source"]["table"], "customers");
        assert_eq!(input["source"]["database"], "testdb");
    }

    #[test]
    fn test_calculate_changed_fields() {
        let event = DebeziumEvent {
            before: Some(serde_json::json!({
                "id": 123,
                "name": "John Doe",
                "email": "john@example.com",
                "age": 30
            })),
            after: Some(serde_json::json!({
                "id": 123,
                "name": "John Doe",
                "email": "john.doe@example.com",
                "age": 31
            })),
            op: CdcOperation::Update,
            source: CdcSource {
                version: None,
                connector: None,
                database: None,
                schema: None,
                table: "customers".to_string(),
                tx_id: None,
                lsn: None,
                timestamp_ms: None,
            },
            timestamp_ms: None,
            transaction: None,
        };

        let input = event.to_workflow_input();
        let changed_fields = input["changed_fields"].as_array().unwrap();

        assert_eq!(changed_fields.len(), 2);
        assert!(changed_fields.contains(&JsonValue::String("email".to_string())));
        assert!(changed_fields.contains(&JsonValue::String("age".to_string())));
    }

    #[test]
    fn test_get_qualified_table_name() {
        let event = DebeziumEvent {
            before: None,
            after: None,
            op: CdcOperation::Read,
            source: CdcSource {
                version: None,
                connector: None,
                database: Some("mydb".to_string()),
                schema: Some("public".to_string()),
                table: "users".to_string(),
                tx_id: None,
                lsn: None,
                timestamp_ms: None,
            },
            timestamp_ms: None,
            transaction: None,
        };

        assert_eq!(event.get_qualified_table_name(), "mydb.public.users");
    }

    #[test]
    fn test_get_cdc_position() {
        let event = DebeziumEvent {
            before: None,
            after: None,
            op: CdcOperation::Read,
            source: CdcSource {
                version: None,
                connector: Some("postgresql".to_string()),
                database: Some("testdb".to_string()),
                schema: None,
                table: "orders".to_string(),
                tx_id: Some(789),
                lsn: Some(45678),
                timestamp_ms: None,
            },
            timestamp_ms: None,
            transaction: None,
        };

        let position = event.get_cdc_position();

        assert_eq!(position.get("lsn"), Some(&"45678".to_string()));
        assert_eq!(position.get("connector"), Some(&"postgresql".to_string()));
        assert_eq!(position.get("tx_id"), Some(&"789".to_string()));
        assert_eq!(position.get("table"), Some(&"orders".to_string()));
    }
}
