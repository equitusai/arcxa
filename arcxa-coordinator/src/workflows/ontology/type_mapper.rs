//! XSD-to-SQL Type Mapping
//!
//! Converts XSD datatypes from ontologies to database-specific SQL types.

use anyhow::{anyhow, Result};
use std::sync::Arc;
use tracing::warn;

/// Trait for mapping XSD types to database-specific SQL types
pub trait TypeMapper: Send + Sync {
    /// Map XSD type to SQL type
    ///
    /// # Arguments
    /// * `xsd_type` - XSD type URI or short form (e.g., "xsd:string" or "string")
    ///
    /// # Returns
    /// Database-specific SQL type string
    fn map_type(&self, xsd_type: &str) -> Result<String>;

    /// Map XSD type with max length constraint (for strings)
    fn map_type_with_length(&self, xsd_type: &str, max_length: u32) -> Result<String>;

    /// Get the database name (for logging)
    fn database_type(&self) -> &str;
}

/// DB2-specific type mapper
#[derive(Debug, Clone)]
pub struct DB2TypeMapper;

impl DB2TypeMapper {
    /// Create a new DB2 type mapper
    pub fn new() -> Self {
        Self
    }

    /// Strip namespace prefix from XSD type
    fn normalize_xsd_type(xsd_type: &str) -> &str {
        if xsd_type.contains('#') {
            // Handle full URI: http://www.w3.org/2001/XMLSchema#string
            xsd_type.split('#').nth(1).unwrap_or(xsd_type)
        } else if xsd_type.contains(':') {
            xsd_type.split(':').nth(1).unwrap_or(xsd_type)
        } else {
            xsd_type
        }
    }

    /// Map XSD type to DB2 SQL type
    fn xsd_to_db2(&self, xsd_type: &str, max_length: Option<u32>) -> Result<String> {
        let base_type = Self::normalize_xsd_type(xsd_type);

        let sql_type = match base_type {
            // String types
            "string" | "normalizedString" | "token" => {
                let len = max_length.unwrap_or(255);
                format!("VARCHAR({})", len)
            }
            "anyURI" => "VARCHAR(2048)".to_string(),

            // Numeric types - integers
            "int" | "integer" => "INTEGER".to_string(),
            "long" => "BIGINT".to_string(),
            "short" => "SMALLINT".to_string(),
            "byte" => "SMALLINT".to_string(),
            "unsignedInt" => "BIGINT".to_string(),
            "unsignedLong" => "DECIMAL(20, 0)".to_string(),
            "unsignedShort" => "INTEGER".to_string(),
            "unsignedByte" => "SMALLINT".to_string(),

            // Numeric types - decimals
            "decimal" => "DECIMAL(19, 4)".to_string(),
            "float" => "REAL".to_string(),
            "double" => "DOUBLE".to_string(),

            // Boolean (DB2 11.1+ has native BOOLEAN, but use INTEGER for compatibility)
            "boolean" => "BOOLEAN".to_string(),

            // Date/Time types
            "date" => "DATE".to_string(),
            "time" => "TIME".to_string(),
            "dateTime" => "TIMESTAMP".to_string(),
            "dateTimeStamp" => "TIMESTAMP".to_string(),
            "gYear" => "SMALLINT".to_string(),
            "gYearMonth" => "VARCHAR(7)".to_string(), // Format: YYYY-MM
            "gMonth" => "VARCHAR(7)".to_string(),
            "gMonthDay" => "VARCHAR(10)".to_string(),
            "gDay" => "VARCHAR(5)".to_string(),
            "duration" => "VARCHAR(50)".to_string(),

            // Binary types
            "base64Binary" | "hexBinary" => "BLOB".to_string(),

            // Default fallback
            _ => {
                warn!(
                    "Unknown XSD type '{}', defaulting to VARCHAR(255)",
                    xsd_type
                );
                "VARCHAR(255)".to_string()
            }
        };

        Ok(sql_type)
    }
}

impl Default for DB2TypeMapper {
    fn default() -> Self {
        Self::new()
    }
}

impl TypeMapper for DB2TypeMapper {
    fn map_type(&self, xsd_type: &str) -> Result<String> {
        self.xsd_to_db2(xsd_type, None)
    }

    fn map_type_with_length(&self, xsd_type: &str, max_length: u32) -> Result<String> {
        self.xsd_to_db2(xsd_type, Some(max_length))
    }

    fn database_type(&self) -> &str {
        "DB2"
    }
}

/// PostgreSQL-specific type mapper
#[derive(Debug, Clone)]
pub struct PostgresTypeMapper;

impl PostgresTypeMapper {
    /// Create a new PostgreSQL type mapper
    pub fn new() -> Self {
        Self
    }

    /// Strip namespace prefix from XSD type
    fn normalize_xsd_type(xsd_type: &str) -> &str {
        if xsd_type.contains('#') {
            xsd_type.split('#').nth(1).unwrap_or(xsd_type)
        } else if xsd_type.contains(':') {
            xsd_type.split(':').nth(1).unwrap_or(xsd_type)
        } else {
            xsd_type
        }
    }

    /// Map XSD type to PostgreSQL SQL type
    fn xsd_to_postgres(&self, xsd_type: &str, max_length: Option<u32>) -> Result<String> {
        let base_type = Self::normalize_xsd_type(xsd_type);

        let sql_type = match base_type {
            // String types
            "string" | "normalizedString" | "token" => {
                let len = max_length.unwrap_or(255);
                if len > 10485760 {
                    // > 10MB, use TEXT
                    "TEXT".to_string()
                } else {
                    format!("VARCHAR({})", len)
                }
            }
            "anyURI" => "VARCHAR(2048)".to_string(),

            // Numeric types - integers
            "int" | "integer" => "INTEGER".to_string(),
            "long" => "BIGINT".to_string(),
            "short" => "SMALLINT".to_string(),
            "byte" => "SMALLINT".to_string(),
            "unsignedInt" => "BIGINT".to_string(),
            "unsignedLong" => "NUMERIC(20, 0)".to_string(),
            "unsignedShort" => "INTEGER".to_string(),
            "unsignedByte" => "SMALLINT".to_string(),

            // Numeric types - decimals
            "decimal" => "NUMERIC(19, 4)".to_string(),
            "float" => "REAL".to_string(),
            "double" => "DOUBLE PRECISION".to_string(),

            // Boolean
            "boolean" => "BOOLEAN".to_string(),

            // Date/Time types
            "date" => "DATE".to_string(),
            "time" => "TIME".to_string(),
            "dateTime" => "TIMESTAMP".to_string(),
            "dateTimeStamp" => "TIMESTAMP WITH TIME ZONE".to_string(),
            "gYear" => "SMALLINT".to_string(),
            "gYearMonth" => "VARCHAR(7)".to_string(),
            "gMonth" => "VARCHAR(7)".to_string(),
            "gMonthDay" => "VARCHAR(10)".to_string(),
            "gDay" => "VARCHAR(5)".to_string(),
            "duration" => "INTERVAL".to_string(),

            // Binary types
            "base64Binary" | "hexBinary" => "BYTEA".to_string(),

            // Default fallback
            _ => {
                warn!(
                    "Unknown XSD type '{}', defaulting to VARCHAR(255)",
                    xsd_type
                );
                "VARCHAR(255)".to_string()
            }
        };

        Ok(sql_type)
    }
}

impl Default for PostgresTypeMapper {
    fn default() -> Self {
        Self::new()
    }
}

impl TypeMapper for PostgresTypeMapper {
    fn map_type(&self, xsd_type: &str) -> Result<String> {
        self.xsd_to_postgres(xsd_type, None)
    }

    fn map_type_with_length(&self, xsd_type: &str, max_length: u32) -> Result<String> {
        self.xsd_to_postgres(xsd_type, Some(max_length))
    }

    fn database_type(&self) -> &str {
        "PostgreSQL"
    }
}

/// Factory function to create a type mapper by database type
pub fn create_type_mapper(database: &str) -> Result<Arc<dyn TypeMapper>> {
    match database.to_lowercase().as_str() {
        "db2" => Ok(Arc::new(DB2TypeMapper::new())),
        "postgresql" | "postgres" | "pg" => Ok(Arc::new(PostgresTypeMapper::new())),
        _ => Err(anyhow!("Unsupported database type: {}", database)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // =============================================================================
    // DB2 Type Mapper Tests
    // =============================================================================

    #[test]
    fn test_db2_string_types() {
        let mapper = DB2TypeMapper::new();

        assert_eq!(mapper.map_type("xsd:string").unwrap(), "VARCHAR(255)");
        assert_eq!(mapper.map_type("string").unwrap(), "VARCHAR(255)");
        assert_eq!(mapper.map_type("normalizedString").unwrap(), "VARCHAR(255)");
        assert_eq!(mapper.map_type("token").unwrap(), "VARCHAR(255)");
    }

    #[test]
    fn test_db2_string_with_custom_length() {
        let mapper = DB2TypeMapper::new();

        assert_eq!(
            mapper.map_type_with_length("xsd:string", 100).unwrap(),
            "VARCHAR(100)"
        );
        assert_eq!(
            mapper.map_type_with_length("string", 500).unwrap(),
            "VARCHAR(500)"
        );
    }

    #[test]
    fn test_db2_numeric_types() {
        let mapper = DB2TypeMapper::new();

        assert_eq!(mapper.map_type("xsd:int").unwrap(), "INTEGER");
        assert_eq!(mapper.map_type("xsd:integer").unwrap(), "INTEGER");
        assert_eq!(mapper.map_type("xsd:long").unwrap(), "BIGINT");
        assert_eq!(mapper.map_type("xsd:short").unwrap(), "SMALLINT");
        assert_eq!(mapper.map_type("xsd:decimal").unwrap(), "DECIMAL(19, 4)");
        assert_eq!(mapper.map_type("xsd:float").unwrap(), "REAL");
        assert_eq!(mapper.map_type("xsd:double").unwrap(), "DOUBLE");
    }

    #[test]
    fn test_db2_datetime_types() {
        let mapper = DB2TypeMapper::new();

        assert_eq!(mapper.map_type("xsd:date").unwrap(), "DATE");
        assert_eq!(mapper.map_type("xsd:time").unwrap(), "TIME");
        assert_eq!(mapper.map_type("xsd:dateTime").unwrap(), "TIMESTAMP");
    }

    #[test]
    fn test_db2_boolean_type() {
        let mapper = DB2TypeMapper::new();

        assert_eq!(mapper.map_type("xsd:boolean").unwrap(), "BOOLEAN");
    }

    #[test]
    fn test_db2_binary_types() {
        let mapper = DB2TypeMapper::new();

        assert_eq!(mapper.map_type("xsd:base64Binary").unwrap(), "BLOB");
        assert_eq!(mapper.map_type("xsd:hexBinary").unwrap(), "BLOB");
    }

    #[test]
    fn test_db2_full_uri_format() {
        let mapper = DB2TypeMapper::new();

        assert_eq!(
            mapper
                .map_type("http://www.w3.org/2001/XMLSchema#string")
                .unwrap(),
            "VARCHAR(255)"
        );
        assert_eq!(
            mapper
                .map_type("http://www.w3.org/2001/XMLSchema#integer")
                .unwrap(),
            "INTEGER"
        );
    }

    #[test]
    fn test_db2_unknown_type_fallback() {
        let mapper = DB2TypeMapper::new();

        assert_eq!(mapper.map_type("xsd:unknownType").unwrap(), "VARCHAR(255)");
    }

    #[test]
    fn test_db2_database_type() {
        let mapper = DB2TypeMapper::new();
        assert_eq!(mapper.database_type(), "DB2");
    }

    // =============================================================================
    // PostgreSQL Type Mapper Tests
    // =============================================================================

    #[test]
    fn test_postgres_string_types() {
        let mapper = PostgresTypeMapper::new();

        assert_eq!(mapper.map_type("xsd:string").unwrap(), "VARCHAR(255)");
        assert_eq!(mapper.map_type("string").unwrap(), "VARCHAR(255)");
        assert_eq!(mapper.map_type("normalizedString").unwrap(), "VARCHAR(255)");
    }

    #[test]
    fn test_postgres_string_with_custom_length() {
        let mapper = PostgresTypeMapper::new();

        assert_eq!(
            mapper.map_type_with_length("xsd:string", 100).unwrap(),
            "VARCHAR(100)"
        );
        assert_eq!(
            mapper.map_type_with_length("string", 500).unwrap(),
            "VARCHAR(500)"
        );
    }

    #[test]
    fn test_postgres_string_large_length() {
        let mapper = PostgresTypeMapper::new();

        // Very large strings should map to TEXT
        assert_eq!(
            mapper.map_type_with_length("string", 20_000_000).unwrap(),
            "TEXT"
        );
    }

    #[test]
    fn test_postgres_numeric_types() {
        let mapper = PostgresTypeMapper::new();

        assert_eq!(mapper.map_type("xsd:int").unwrap(), "INTEGER");
        assert_eq!(mapper.map_type("xsd:integer").unwrap(), "INTEGER");
        assert_eq!(mapper.map_type("xsd:long").unwrap(), "BIGINT");
        assert_eq!(mapper.map_type("xsd:short").unwrap(), "SMALLINT");
        assert_eq!(mapper.map_type("xsd:decimal").unwrap(), "NUMERIC(19, 4)");
        assert_eq!(mapper.map_type("xsd:float").unwrap(), "REAL");
        assert_eq!(mapper.map_type("xsd:double").unwrap(), "DOUBLE PRECISION");
    }

    #[test]
    fn test_postgres_datetime_types() {
        let mapper = PostgresTypeMapper::new();

        assert_eq!(mapper.map_type("xsd:date").unwrap(), "DATE");
        assert_eq!(mapper.map_type("xsd:time").unwrap(), "TIME");
        assert_eq!(mapper.map_type("xsd:dateTime").unwrap(), "TIMESTAMP");
        assert_eq!(
            mapper.map_type("xsd:dateTimeStamp").unwrap(),
            "TIMESTAMP WITH TIME ZONE"
        );
        assert_eq!(mapper.map_type("xsd:duration").unwrap(), "INTERVAL");
    }

    #[test]
    fn test_postgres_boolean_type() {
        let mapper = PostgresTypeMapper::new();

        assert_eq!(mapper.map_type("xsd:boolean").unwrap(), "BOOLEAN");
    }

    #[test]
    fn test_postgres_binary_types() {
        let mapper = PostgresTypeMapper::new();

        assert_eq!(mapper.map_type("xsd:base64Binary").unwrap(), "BYTEA");
        assert_eq!(mapper.map_type("xsd:hexBinary").unwrap(), "BYTEA");
    }

    #[test]
    fn test_postgres_full_uri_format() {
        let mapper = PostgresTypeMapper::new();

        assert_eq!(
            mapper
                .map_type("http://www.w3.org/2001/XMLSchema#string")
                .unwrap(),
            "VARCHAR(255)"
        );
        assert_eq!(
            mapper
                .map_type("http://www.w3.org/2001/XMLSchema#double")
                .unwrap(),
            "DOUBLE PRECISION"
        );
    }

    #[test]
    fn test_postgres_unknown_type_fallback() {
        let mapper = PostgresTypeMapper::new();

        assert_eq!(mapper.map_type("xsd:unknownType").unwrap(), "VARCHAR(255)");
    }

    #[test]
    fn test_postgres_database_type() {
        let mapper = PostgresTypeMapper::new();
        assert_eq!(mapper.database_type(), "PostgreSQL");
    }

    // =============================================================================
    // Factory Function Tests
    // =============================================================================

    #[test]
    fn test_create_type_mapper_db2() {
        let mapper = create_type_mapper("db2").unwrap();
        assert_eq!(mapper.database_type(), "DB2");
    }

    #[test]
    fn test_create_type_mapper_postgres() {
        let mapper = create_type_mapper("postgresql").unwrap();
        assert_eq!(mapper.database_type(), "PostgreSQL");

        let mapper = create_type_mapper("postgres").unwrap();
        assert_eq!(mapper.database_type(), "PostgreSQL");

        let mapper = create_type_mapper("pg").unwrap();
        assert_eq!(mapper.database_type(), "PostgreSQL");
    }

    #[test]
    fn test_create_type_mapper_invalid() {
        let result = create_type_mapper("mysql");
        assert!(result.is_err());
        let err = result.err().expect("expected create_type_mapper to fail");
        assert!(err.to_string().contains("Unsupported database type"));
    }

    // =============================================================================
    // Cross-Database Compatibility Tests
    // =============================================================================

    #[test]
    fn test_cross_database_string_mapping() {
        let db2 = DB2TypeMapper::new();
        let pg = PostgresTypeMapper::new();

        // Basic strings should map to VARCHAR(255) in both
        assert_eq!(db2.map_type("xsd:string").unwrap(), "VARCHAR(255)");
        assert_eq!(pg.map_type("xsd:string").unwrap(), "VARCHAR(255)");
    }

    #[test]
    fn test_cross_database_integer_mapping() {
        let db2 = DB2TypeMapper::new();
        let pg = PostgresTypeMapper::new();

        assert_eq!(db2.map_type("xsd:integer").unwrap(), "INTEGER");
        assert_eq!(pg.map_type("xsd:integer").unwrap(), "INTEGER");

        assert_eq!(db2.map_type("xsd:long").unwrap(), "BIGINT");
        assert_eq!(pg.map_type("xsd:long").unwrap(), "BIGINT");
    }

    #[test]
    fn test_cross_database_date_mapping() {
        let db2 = DB2TypeMapper::new();
        let pg = PostgresTypeMapper::new();

        assert_eq!(db2.map_type("xsd:date").unwrap(), "DATE");
        assert_eq!(pg.map_type("xsd:date").unwrap(), "DATE");

        assert_eq!(db2.map_type("xsd:dateTime").unwrap(), "TIMESTAMP");
        assert_eq!(pg.map_type("xsd:dateTime").unwrap(), "TIMESTAMP");
    }
}
