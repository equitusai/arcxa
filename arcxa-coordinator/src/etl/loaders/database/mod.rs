//! Database Loader Module
//!
//! Professional database loading infrastructure with support for:
//! - PostgreSQL (native via tokio-postgres)
//! - DB2 (deprecated - use workflows/engine/transformers/db2_load.rs instead)
//!
//! ## Architecture
//!
//! ```text
//! DatabaseLoader (trait)
//!       ↑
//!       └── PostgreSQLLoader (COPY, INSERT, UPSERT)
//! ```
//!
//! ## Features
//!
//! - **Async I/O**: All database operations are fully async
//! - **Connection Pooling**: Reusable connections across executions
//! - **Bulk Operations**: Optimized batch loading (50K-200K rows/sec)
//! - **Load Modes**: INSERT, UPSERT, REPLACE
//! - **Error Recovery**: Transaction management and rollback

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;

pub mod common;
pub mod postgres_pool;
pub mod postgresql;
// db2 module removed - use workflows/engine/transformers/db2_load.rs instead

pub use common::*;
pub use postgres_pool::{
    create_postgres_pool, get_pool_stats, PoolStats, PoolTimeouts, PostgresConfig, PostgresPool,
    PostgresPoolConfig,
};
pub use postgresql::PostgreSQLLoader;

/// Database loader trait
///
/// Abstraction over different database implementations for ETL loading.
#[async_trait]
pub trait DatabaseLoader: Send + Sync {
    /// Load records into target table
    ///
    /// # Arguments
    /// * `table_name` - Target table name
    /// * `records` - JSON records to load
    /// * `mode` - Load mode (INSERT, UPSERT, REPLACE)
    /// * `key_fields` - Key fields for UPSERT mode
    ///
    /// # Returns
    /// Number of rows successfully loaded
    async fn load(
        &self,
        table_name: &str,
        records: Vec<Value>,
        mode: LoadMode,
        key_fields: Option<&[String]>,
    ) -> Result<u64>;

    /// Test database connection
    async fn test_connection(&self) -> Result<()>;

    /// Get database type name
    fn database_type(&self) -> &'static str;

    /// Get connection info (for logging, without credentials)
    fn connection_info(&self) -> String;
}

/// Load mode for database operations (deprecated - use etl::traits::LoadMode)
///
/// This enum has been deprecated in favor of the canonical LoadMode in etl::traits.
/// The canonical version includes additional modes (Append, Merge) and is used
/// consistently across the entire ETL subsystem.
///
/// # Migration
/// ```rust
/// // Old
/// use graphica_coordinator::etl::loaders::database::LoadMode as LegacyLoadMode;
///
/// // New
/// use graphica_coordinator::etl::traits::LoadMode;
/// ```
#[deprecated(
    since = "2.1.0",
    note = "Use graphica_coordinator::etl::traits::LoadMode instead. This is now a type alias to the canonical LoadMode enum."
)]
pub use crate::etl::traits::LoadMode;

/// Database loader factory
///
/// Creates appropriate database loader based on datasource configuration.
pub struct DatabaseLoaderFactory;

impl DatabaseLoaderFactory {
    /// Create database loader from datasource configuration
    ///
    /// # Arguments
    /// * `datasource_type` - Database type (PostgreSQL, DB2, etc.)
    /// * `connection_string` - Database connection string
    /// * `batch_size` - Batch size for bulk operations
    ///
    /// # Returns
    /// Database loader implementation
    pub async fn create(
        datasource_type: &str,
        connection_string: &str,
        batch_size: usize,
    ) -> Result<Box<dyn DatabaseLoader>> {
        match datasource_type.to_lowercase().as_str() {
            "postgresql" | "postgres" => {
                let loader = PostgreSQLLoader::new(connection_string, batch_size).await?;
                Ok(Box::new(loader))
            }
            "db2" => {
                anyhow::bail!(
                    "ETL DB2Loader removed. Use Db2LoadTransformer from workflows/engine/transformers/db2_load.rs instead"
                )
            }
            _ => {
                anyhow::bail!("Unsupported database type: {}", datasource_type)
            }
        }
    }
}

/// Convert JSON records to row format for database loading
pub fn records_to_rows(
    records: &[Value],
) -> Result<(Vec<String>, Vec<HashMap<String, Option<String>>>)> {
    if records.is_empty() {
        return Ok((vec![], vec![]));
    }

    // Extract column names from first record
    let first_record = &records[0];
    let columns: Vec<String> = if let Value::Object(obj) = first_record {
        obj.keys().cloned().collect()
    } else {
        anyhow::bail!("Expected record to be a JSON object");
    };

    // Convert each record to HashMap
    let mut rows = Vec::new();
    for record in records {
        if let Value::Object(obj) = record {
            let mut row = HashMap::new();
            for column in &columns {
                let value = obj
                    .get(column)
                    .map(|v| match v {
                        Value::Null => None,
                        Value::String(s) => Some(s.clone()),
                        other => Some(other.to_string()),
                    })
                    .unwrap_or(None);
                row.insert(column.clone(), value);
            }
            rows.push(row);
        } else {
            anyhow::bail!("Expected record to be a JSON object");
        }
    }

    Ok((columns, rows))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_records_to_rows() {
        let records = vec![
            json!({"id": "1", "name": "Alice"}),
            json!({"id": "2", "name": "Bob"}),
        ];

        let (columns, rows) = records_to_rows(&records).unwrap();

        assert_eq!(columns.len(), 2);
        assert!(columns.contains(&"id".to_string()));
        assert!(columns.contains(&"name".to_string()));
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn test_load_mode_display() {
        assert_eq!(LoadMode::Insert.to_string(), "INSERT");
        assert_eq!(LoadMode::Upsert.to_string(), "UPSERT");
        assert_eq!(LoadMode::Replace.to_string(), "REPLACE");
    }
}
