//! LogicalTable Type
//!
//! Defines the source data for a triples map.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// LogicalTable (rr:logicalTable)
///
/// Defines the source data for generating RDF triples.
///
/// ## W3C R2RML Spec
///
/// A logical table can be:
/// - A table name (CSV file, Parquet file, database table)
/// - An SQL query (for relational databases)
///
/// For Graphica, we primarily use table names referring to CSV/Parquet files.
///
/// ## Examples
///
/// ### Table Name
/// ```turtle
/// rr:logicalTable [ rr:tableName "customers.csv" ] .
/// ```
///
/// ### SQL Query
/// ```turtle
/// rr:logicalTable [ rr:sqlQuery "SELECT * FROM customers WHERE active = true" ] .
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type")]
pub enum LogicalTable {
    /// Table name (CSV file, Parquet file, or database table)
    TableName { table_name: String },

    /// SQL query (for relational databases)
    SqlQuery { query: String },
}

impl LogicalTable {
    /// Create a logical table from a table name
    pub fn from_table_name(table_name: String) -> Self {
        LogicalTable::TableName { table_name }
    }

    /// Create a logical table from an SQL query
    pub fn from_sql_query(query: String) -> Self {
        LogicalTable::SqlQuery { query }
    }

    /// Validate the logical table
    pub fn validate(&self) -> Result<()> {
        match self {
            LogicalTable::TableName { table_name } => {
                if table_name.is_empty() {
                    anyhow::bail!("Table name cannot be empty");
                }
                Ok(())
            }
            LogicalTable::SqlQuery { query } => {
                if query.is_empty() {
                    anyhow::bail!("SQL query cannot be empty");
                }
                // Basic SQL validation
                let query_lower = query.to_lowercase();
                if !query_lower.contains("select") {
                    anyhow::bail!("SQL query must contain SELECT statement");
                }
                Ok(())
            }
        }
    }

    /// Get the table name or query string
    pub fn get_source(&self) -> &str {
        match self {
            LogicalTable::TableName { table_name } => table_name,
            LogicalTable::SqlQuery { query } => query,
        }
    }

    /// Check if this is a CSV file
    pub fn is_csv(&self) -> bool {
        match self {
            LogicalTable::TableName { table_name } => {
                table_name.ends_with(".csv") || table_name.ends_with(".CSV")
            }
            _ => false,
        }
    }

    /// Check if this is a Parquet file
    pub fn is_parquet(&self) -> bool {
        match self {
            LogicalTable::TableName { table_name } => {
                table_name.ends_with(".parquet") || table_name.ends_with(".PARQUET")
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_logical_table_from_table_name() {
        let lt = LogicalTable::from_table_name("customers.csv".to_string());
        assert!(matches!(lt, LogicalTable::TableName { .. }));
        assert!(lt.validate().is_ok());
        assert_eq!(lt.get_source(), "customers.csv");
        assert!(lt.is_csv());
        assert!(!lt.is_parquet());
    }

    #[test]
    fn test_logical_table_from_sql_query() {
        let lt = LogicalTable::from_sql_query("SELECT * FROM customers".to_string());
        assert!(matches!(lt, LogicalTable::SqlQuery { .. }));
        assert!(lt.validate().is_ok());
    }

    #[test]
    fn test_logical_table_validation() {
        // Empty table name
        let lt = LogicalTable::from_table_name("".to_string());
        assert!(lt.validate().is_err());

        // Empty SQL query
        let lt = LogicalTable::from_sql_query("".to_string());
        assert!(lt.validate().is_err());

        // Invalid SQL query (no SELECT)
        let lt = LogicalTable::from_sql_query("DELETE FROM customers".to_string());
        assert!(lt.validate().is_err());
    }

    #[test]
    fn test_file_type_detection() {
        let csv_lt = LogicalTable::from_table_name("data.csv".to_string());
        assert!(csv_lt.is_csv());
        assert!(!csv_lt.is_parquet());

        let parquet_lt = LogicalTable::from_table_name("data.parquet".to_string());
        assert!(!parquet_lt.is_csv());
        assert!(parquet_lt.is_parquet());
    }
}
