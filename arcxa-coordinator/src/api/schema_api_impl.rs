//! Schema API Implementation
//!
//! Helper functions and implementations for schema API handlers.

use anyhow::{anyhow, Result};
use graphica_core::schema::{ConversionRulesEngine, SourceType, SqlDialect, UniversalDataType};

/// Parse type string to UniversalDataType
///
/// Supports common type names used in API requests.
pub fn parse_type_string(type_str: &str) -> Result<UniversalDataType> {
    let normalized = type_str.trim().to_lowercase();

    match normalized.as_str() {
        // Integer types
        "integer" | "int" | "int32" => Ok(UniversalDataType::Integer { bits: Some(32) }),
        "bigint" | "int64" | "long" => Ok(UniversalDataType::Integer { bits: Some(64) }),
        "smallint" | "int16" | "short" => Ok(UniversalDataType::Integer { bits: Some(16) }),
        "tinyint" | "int8" => Ok(UniversalDataType::Integer { bits: Some(8) }),

        // Float types
        "float" | "float32" | "real" => Ok(UniversalDataType::Float { bits: Some(32) }),
        "double" | "float64" | "double precision" => Ok(UniversalDataType::Float { bits: Some(64) }),

        // Decimal
        "decimal" | "numeric" => Ok(UniversalDataType::Decimal { precision: 18, scale: 2 }),

        // String types
        "string" | "varchar" | "text" => Ok(UniversalDataType::String { max_length: None }),
        "char" => Ok(UniversalDataType::Char { length: 1 }),

        // Boolean
        "boolean" | "bool" => Ok(UniversalDataType::Boolean),

        // Temporal types
        "date" => Ok(UniversalDataType::Date),
        "time" => Ok(UniversalDataType::Time { with_timezone: false }),
        "datetime" | "timestamp" => Ok(UniversalDataType::DateTime { with_timezone: false }),
        "timestamptz" | "timestamp with time zone" => Ok(UniversalDataType::DateTime { with_timezone: true }),

        // Semi-structured
        "json" | "jsonb" => Ok(UniversalDataType::Json),
        "xml" => Ok(UniversalDataType::Xml),

        // Binary
        "binary" | "bytea" | "blob" => Ok(UniversalDataType::Binary { max_length: None }),

        // Special
        "uuid" => Ok(UniversalDataType::Uuid),

        _ => Err(anyhow!("Unknown type: {}. Supported types: Integer, BigInt, Float, Double, Decimal, String, Char, Boolean, Date, Time, DateTime, TimestampTz, Json, Xml, Binary, UUID", type_str))
    }
}

/// Parse dialect string to SqlDialect
pub fn parse_dialect_string(dialect_str: &str) -> Result<SqlDialect> {
    let normalized = dialect_str.trim();

    match normalized {
        "PostgreSQL" => Ok(SqlDialect::PostgreSQL),
        "MySQL" => Ok(SqlDialect::MySQL),
        "Oracle" => Ok(SqlDialect::Oracle),
        "DB2" => Ok(SqlDialect::DB2),
        "SQLServer" | "SQL Server" => Ok(SqlDialect::SQLServer),
        "Snowflake" => Ok(SqlDialect::Snowflake),
        "Generic" => Ok(SqlDialect::Generic),
        _ => Err(anyhow!("Unknown dialect: {}. Supported: PostgreSQL, MySQL, Oracle, DB2, SQLServer, Snowflake, Generic", dialect_str))
    }
}

/// Convert UniversalDataType to display string
pub fn type_to_string(data_type: &UniversalDataType) -> String {
    match data_type {
        UniversalDataType::Integer { bits: Some(8) } => "TinyInt".to_string(),
        UniversalDataType::Integer { bits: Some(16) } => "SmallInt".to_string(),
        UniversalDataType::Integer { bits: Some(32) } => "Integer".to_string(),
        UniversalDataType::Integer { bits: Some(64) } => "BigInt".to_string(),
        UniversalDataType::Integer { bits: None } => "Integer".to_string(),
        UniversalDataType::Float { bits: Some(32) } => "Float".to_string(),
        UniversalDataType::Float { bits: Some(64) } => "Double".to_string(),
        UniversalDataType::Float { bits: None } => "Float".to_string(),
        UniversalDataType::Decimal { precision, scale } => {
            format!("Decimal({}, {})", precision, scale)
        }
        UniversalDataType::String {
            max_length: Some(len),
        } => format!("String({})", len),
        UniversalDataType::String { max_length: None } => "String".to_string(),
        UniversalDataType::Text => "Text".to_string(),
        UniversalDataType::Char { length } => format!("Char({})", length),
        UniversalDataType::Boolean => "Boolean".to_string(),
        UniversalDataType::Date => "Date".to_string(),
        UniversalDataType::Time {
            with_timezone: true,
        } => "Time with timezone".to_string(),
        UniversalDataType::Time {
            with_timezone: false,
        } => "Time".to_string(),
        UniversalDataType::DateTime {
            with_timezone: true,
        } => "DateTime with timezone".to_string(),
        UniversalDataType::DateTime {
            with_timezone: false,
        } => "DateTime".to_string(),
        UniversalDataType::Timestamp => "Timestamp".to_string(),
        UniversalDataType::Interval => "Interval".to_string(),
        UniversalDataType::Binary {
            max_length: Some(len),
        } => format!("Binary({})", len),
        UniversalDataType::Binary { max_length: None } => "Binary".to_string(),
        UniversalDataType::Json => "JSON".to_string(),
        UniversalDataType::Xml => "XML".to_string(),
        UniversalDataType::Uuid => "UUID".to_string(),
        UniversalDataType::Array { element_type } => {
            format!("Array<{}>", type_to_string(element_type))
        }
        UniversalDataType::Enum { values } => format!("Enum({})", values.join(", ")),
        UniversalDataType::Struct { fields } => format!("Struct({} fields)", fields.len()),
        UniversalDataType::Unknown => "Unknown".to_string(),
        _ => format!("{:?}", data_type),
    }
}

/// Convert SourceType to string
pub fn source_type_to_string(source_type: &SourceType) -> String {
    let s = match source_type {
        SourceType::CsvFile => "CsvFile",
        SourceType::ExcelFile => "ExcelFile",
        SourceType::ParquetFile => "ParquetFile",
        SourceType::PostgreSQL => "PostgreSQL",
        SourceType::MySQL => "MySQL",
        SourceType::Oracle => "Oracle",
        SourceType::DB2 => "DB2",
        SourceType::Snowflake => "Snowflake",
        SourceType::S3Parquet => "S3Parquet",
        SourceType::RdfTriples => "RdfTriples",
        _ => return format!("{:?}", source_type), // Handle any other variants
    };
    s.to_string()
}

/// Convert source type string to String (pass-through for catalog source_type which is already a String)
pub fn source_type_string_passthrough(source_type_str: &str) -> String {
    source_type_str.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_integer_types() {
        assert!(matches!(
            parse_type_string("integer").unwrap(),
            UniversalDataType::Integer { bits: Some(32) }
        ));
        assert!(matches!(
            parse_type_string("INT").unwrap(),
            UniversalDataType::Integer { bits: Some(32) }
        ));
        assert!(matches!(
            parse_type_string("bigint").unwrap(),
            UniversalDataType::Integer { bits: Some(64) }
        ));
    }

    #[test]
    fn test_parse_float_types() {
        assert!(matches!(
            parse_type_string("float").unwrap(),
            UniversalDataType::Float { bits: Some(32) }
        ));
        assert!(matches!(
            parse_type_string("double").unwrap(),
            UniversalDataType::Float { bits: Some(64) }
        ));
    }

    #[test]
    fn test_parse_string_types() {
        assert!(matches!(
            parse_type_string("string").unwrap(),
            UniversalDataType::String { max_length: None }
        ));
        assert!(matches!(
            parse_type_string("VARCHAR").unwrap(),
            UniversalDataType::String { max_length: None }
        ));
    }

    #[test]
    fn test_parse_temporal_types() {
        assert!(matches!(
            parse_type_string("date").unwrap(),
            UniversalDataType::Date
        ));
        assert!(matches!(
            parse_type_string("datetime").unwrap(),
            UniversalDataType::DateTime {
                with_timezone: false
            }
        ));
    }

    #[test]
    fn test_parse_invalid_type() {
        assert!(parse_type_string("InvalidType").is_err());
    }

    #[test]
    fn test_parse_dialects() {
        assert!(matches!(
            parse_dialect_string("PostgreSQL").unwrap(),
            SqlDialect::PostgreSQL
        ));
        assert!(matches!(
            parse_dialect_string("MySQL").unwrap(),
            SqlDialect::MySQL
        ));
        assert!(matches!(
            parse_dialect_string("Oracle").unwrap(),
            SqlDialect::Oracle
        ));
    }

    #[test]
    fn test_parse_invalid_dialect() {
        assert!(parse_dialect_string("InvalidDialect").is_err());
    }

    #[test]
    fn test_type_to_string() {
        let int_type = UniversalDataType::Integer { bits: Some(32) };
        assert_eq!(type_to_string(&int_type), "Integer");

        let str_type = UniversalDataType::String {
            max_length: Some(255),
        };
        assert_eq!(type_to_string(&str_type), "String(255)");

        let decimal_type = UniversalDataType::Decimal {
            precision: 18,
            scale: 2,
        };
        assert_eq!(type_to_string(&decimal_type), "Decimal(18, 2)");
    }
}
