//! Source-specific schema helpers
//!
//! Utilities for working with schemas from specific datasource types.

use super::types::{SourceType, UniversalDataType};
use super::UnifiedField;

/// Convert database-specific type strings to UniversalDataType
pub fn database_type_to_universal(db_type: &str, source: SourceType) -> UniversalDataType {
    match source {
        SourceType::PostgreSQL => postgres_type_to_universal(db_type),
        SourceType::MySQL => mysql_type_to_universal(db_type),
        SourceType::Oracle => oracle_type_to_universal(db_type),
        SourceType::DB2 => db2_type_to_universal(db_type),
        SourceType::SQLServer => sqlserver_type_to_universal(db_type),
        SourceType::Snowflake => snowflake_type_to_universal(db_type),
        _ => UniversalDataType::Unknown,
    }
}

/// Convert PostgreSQL types to UniversalDataType
pub fn postgres_type_to_universal(pg_type: &str) -> UniversalDataType {
    match pg_type.to_lowercase().as_str() {
        // Numeric types
        "smallint" | "int2" => UniversalDataType::Integer { bits: Some(16) },
        "integer" | "int" | "int4" => UniversalDataType::Integer { bits: Some(32) },
        "bigint" | "int8" => UniversalDataType::Integer { bits: Some(64) },
        "real" | "float4" => UniversalDataType::Float { bits: Some(32) },
        "double precision" | "float8" => UniversalDataType::Float { bits: Some(64) },
        s if s.starts_with("numeric") || s.starts_with("decimal") => parse_decimal_type(s)
            .unwrap_or(UniversalDataType::Decimal {
                precision: 38,
                scale: 10,
            }),

        // String types
        s if s.starts_with("character varying") || s.starts_with("varchar") => {
            parse_varchar_type(s).unwrap_or(UniversalDataType::String { max_length: None })
        }
        s if s.starts_with("character") || s.starts_with("char") => {
            parse_char_type(s).unwrap_or(UniversalDataType::Char { length: 1 })
        }
        "text" => UniversalDataType::Text,

        // Temporal types
        "date" => UniversalDataType::Date,
        "time" | "time without time zone" => UniversalDataType::Time {
            with_timezone: false,
        },
        "time with time zone" | "timetz" => UniversalDataType::Time {
            with_timezone: true,
        },
        "timestamp" | "timestamp without time zone" => UniversalDataType::DateTime {
            with_timezone: false,
        },
        "timestamp with time zone" | "timestamptz" => UniversalDataType::DateTime {
            with_timezone: true,
        },
        "interval" => UniversalDataType::Interval,

        // Boolean
        "boolean" | "bool" => UniversalDataType::Boolean,

        // Binary
        "bytea" => UniversalDataType::Binary { max_length: None },

        // JSON
        "json" | "jsonb" => UniversalDataType::Json,

        // UUID
        "uuid" => UniversalDataType::Uuid,

        // Array types
        s if s.ends_with("[]") => {
            let base_type = &s[..s.len() - 2];
            UniversalDataType::Array {
                element_type: Box::new(postgres_type_to_universal(base_type)),
            }
        }

        _ => UniversalDataType::Unknown,
    }
}

/// Convert MySQL types to UniversalDataType
pub fn mysql_type_to_universal(mysql_type: &str) -> UniversalDataType {
    match mysql_type.to_lowercase().as_str() {
        // Numeric types
        "tinyint" => UniversalDataType::Integer { bits: Some(8) },
        "smallint" => UniversalDataType::Integer { bits: Some(16) },
        "mediumint" => UniversalDataType::Integer { bits: Some(24) },
        "int" | "integer" => UniversalDataType::Integer { bits: Some(32) },
        "bigint" => UniversalDataType::Integer { bits: Some(64) },
        "float" => UniversalDataType::Float { bits: Some(32) },
        "double" | "double precision" | "real" => UniversalDataType::Float { bits: Some(64) },
        s if s.starts_with("decimal") || s.starts_with("numeric") => parse_decimal_type(s)
            .unwrap_or(UniversalDataType::Decimal {
                precision: 65,
                scale: 30,
            }),

        // String types
        s if s.starts_with("varchar") => {
            parse_varchar_type(s).unwrap_or(UniversalDataType::String { max_length: None })
        }
        s if s.starts_with("char") => {
            parse_char_type(s).unwrap_or(UniversalDataType::Char { length: 1 })
        }
        "tinytext" => UniversalDataType::String {
            max_length: Some(255),
        },
        "text" => UniversalDataType::String {
            max_length: Some(65535),
        },
        "mediumtext" => UniversalDataType::Text,
        "longtext" => UniversalDataType::Text,

        // Temporal types
        "date" => UniversalDataType::Date,
        "time" => UniversalDataType::Time {
            with_timezone: false,
        },
        "datetime" => UniversalDataType::DateTime {
            with_timezone: false,
        },
        "timestamp" => UniversalDataType::DateTime {
            with_timezone: false,
        },
        "year" => UniversalDataType::Integer { bits: Some(16) },

        // Boolean (MySQL uses TINYINT(1))
        "boolean" | "bool" => UniversalDataType::Boolean,

        // Binary
        "binary" => UniversalDataType::Binary {
            max_length: Some(255),
        },
        "varbinary" => UniversalDataType::Binary { max_length: None },
        "tinyblob" => UniversalDataType::Binary {
            max_length: Some(255),
        },
        "blob" => UniversalDataType::Binary {
            max_length: Some(65535),
        },
        "mediumblob" | "longblob" => UniversalDataType::Binary { max_length: None },

        // JSON
        "json" => UniversalDataType::Json,

        _ => UniversalDataType::Unknown,
    }
}

/// Convert Oracle types to UniversalDataType
pub fn oracle_type_to_universal(oracle_type: &str) -> UniversalDataType {
    match oracle_type.to_uppercase().as_str() {
        // Numeric types
        "NUMBER" => UniversalDataType::Decimal {
            precision: 38,
            scale: 10,
        },
        s if s.starts_with("NUMBER") => {
            parse_oracle_number(s).unwrap_or(UniversalDataType::Decimal {
                precision: 38,
                scale: 10,
            })
        }
        "BINARY_FLOAT" => UniversalDataType::Float { bits: Some(32) },
        "BINARY_DOUBLE" => UniversalDataType::Float { bits: Some(64) },

        // String types
        s if s.starts_with("VARCHAR2") => {
            parse_varchar2_type(s).unwrap_or(UniversalDataType::String {
                max_length: Some(4000),
            })
        }
        s if s.starts_with("CHAR") => {
            parse_char_type(s).unwrap_or(UniversalDataType::Char { length: 1 })
        }
        "CLOB" => UniversalDataType::Text,
        "NCLOB" => UniversalDataType::Text,

        // Temporal types
        "DATE" => UniversalDataType::DateTime {
            with_timezone: false,
        },
        s if s.starts_with("TIMESTAMP") && s.contains("TIME ZONE") => UniversalDataType::DateTime {
            with_timezone: true,
        },
        s if s.starts_with("TIMESTAMP") => UniversalDataType::DateTime {
            with_timezone: false,
        },
        s if s.starts_with("INTERVAL") => UniversalDataType::Interval,

        // Binary
        "RAW" => UniversalDataType::Binary {
            max_length: Some(2000),
        },
        "LONG RAW" => UniversalDataType::Binary { max_length: None },
        "BLOB" => UniversalDataType::Binary { max_length: None },

        // Other
        "XMLTYPE" => UniversalDataType::Xml,

        _ => UniversalDataType::Unknown,
    }
}

/// Convert DB2 types to UniversalDataType
pub fn db2_type_to_universal(db2_type: &str) -> UniversalDataType {
    match db2_type.to_uppercase().as_str() {
        // Numeric types
        "SMALLINT" => UniversalDataType::Integer { bits: Some(16) },
        "INTEGER" | "INT" => UniversalDataType::Integer { bits: Some(32) },
        "BIGINT" => UniversalDataType::Integer { bits: Some(64) },
        "REAL" => UniversalDataType::Float { bits: Some(32) },
        "DOUBLE" | "DOUBLE PRECISION" | "FLOAT" => UniversalDataType::Float { bits: Some(64) },
        s if s.starts_with("DECIMAL") || s.starts_with("NUMERIC") || s.starts_with("DEC") => {
            parse_decimal_type(s).unwrap_or(UniversalDataType::Decimal {
                precision: 31,
                scale: 10,
            })
        }

        // String types
        s if s.starts_with("VARCHAR") => {
            parse_varchar_type(s).unwrap_or(UniversalDataType::String {
                max_length: Some(32672),
            })
        }
        s if s.starts_with("CHAR") => {
            parse_char_type(s).unwrap_or(UniversalDataType::Char { length: 1 })
        }
        "CLOB" => UniversalDataType::Text,

        // Temporal types
        "DATE" => UniversalDataType::Date,
        "TIME" => UniversalDataType::Time {
            with_timezone: false,
        },
        "TIMESTAMP" => UniversalDataType::DateTime {
            with_timezone: false,
        },

        // Boolean (DB2 uses CHAR(1) or SMALLINT for boolean)
        "BOOLEAN" => UniversalDataType::Boolean,

        // Binary
        s if s.starts_with("BINARY") => UniversalDataType::Binary {
            max_length: Some(255),
        },
        s if s.starts_with("VARBINARY") => UniversalDataType::Binary { max_length: None },
        "BLOB" => UniversalDataType::Binary { max_length: None },

        // XML
        "XML" => UniversalDataType::Xml,

        _ => UniversalDataType::Unknown,
    }
}

/// Convert SQL Server types to UniversalDataType
pub fn sqlserver_type_to_universal(sql_type: &str) -> UniversalDataType {
    match sql_type.to_lowercase().as_str() {
        // Numeric types
        "bit" => UniversalDataType::Boolean,
        "tinyint" => UniversalDataType::Integer { bits: Some(8) },
        "smallint" => UniversalDataType::Integer { bits: Some(16) },
        "int" => UniversalDataType::Integer { bits: Some(32) },
        "bigint" => UniversalDataType::Integer { bits: Some(64) },
        "real" => UniversalDataType::Float { bits: Some(32) },
        "float" => UniversalDataType::Float { bits: Some(64) },
        s if s.starts_with("decimal") || s.starts_with("numeric") => parse_decimal_type(s)
            .unwrap_or(UniversalDataType::Decimal {
                precision: 38,
                scale: 10,
            }),

        // String types
        s if s.starts_with("varchar") => {
            parse_varchar_type(s).unwrap_or(UniversalDataType::String { max_length: None })
        }
        s if s.starts_with("nvarchar") => {
            parse_varchar_type(s).unwrap_or(UniversalDataType::String { max_length: None })
        }
        s if s.starts_with("char") => {
            parse_char_type(s).unwrap_or(UniversalDataType::Char { length: 1 })
        }
        s if s.starts_with("nchar") => {
            parse_char_type(s).unwrap_or(UniversalDataType::Char { length: 1 })
        }
        "text" | "ntext" => UniversalDataType::Text,

        // Temporal types
        "date" => UniversalDataType::Date,
        "time" => UniversalDataType::Time {
            with_timezone: false,
        },
        "datetime" | "datetime2" => UniversalDataType::DateTime {
            with_timezone: false,
        },
        "datetimeoffset" => UniversalDataType::DateTime {
            with_timezone: true,
        },

        // Binary
        s if s.starts_with("binary") => UniversalDataType::Binary {
            max_length: Some(8000),
        },
        s if s.starts_with("varbinary") => UniversalDataType::Binary { max_length: None },
        "image" => UniversalDataType::Binary { max_length: None },

        // Other
        "uniqueidentifier" => UniversalDataType::Uuid,
        "xml" => UniversalDataType::Xml,

        _ => UniversalDataType::Unknown,
    }
}

/// Convert Snowflake types to UniversalDataType
pub fn snowflake_type_to_universal(sf_type: &str) -> UniversalDataType {
    match sf_type.to_uppercase().as_str() {
        // Numeric types
        "NUMBER" | "NUMERIC" | "DECIMAL" => UniversalDataType::Decimal {
            precision: 38,
            scale: 10,
        },
        "INT" | "INTEGER" | "BIGINT" | "SMALLINT" | "TINYINT" | "BYTEINT" => {
            UniversalDataType::Integer { bits: Some(64) }
        }
        "FLOAT" | "FLOAT4" | "FLOAT8" | "DOUBLE" | "DOUBLE PRECISION" | "REAL" => {
            UniversalDataType::Float { bits: Some(64) }
        }

        // String types
        s if s.starts_with("VARCHAR") || s.starts_with("STRING") || s.starts_with("TEXT") => {
            UniversalDataType::String { max_length: None }
        }
        s if s.starts_with("CHAR") || s.starts_with("CHARACTER") => {
            parse_char_type(s).unwrap_or(UniversalDataType::Char { length: 1 })
        }

        // Temporal types
        "DATE" => UniversalDataType::Date,
        "TIME" => UniversalDataType::Time {
            with_timezone: false,
        },
        "DATETIME" | "TIMESTAMP" | "TIMESTAMP_NTZ" => UniversalDataType::DateTime {
            with_timezone: false,
        },
        "TIMESTAMP_LTZ" | "TIMESTAMP_TZ" => UniversalDataType::DateTime {
            with_timezone: true,
        },

        // Boolean
        "BOOLEAN" => UniversalDataType::Boolean,

        // Binary
        "BINARY" | "VARBINARY" => UniversalDataType::Binary { max_length: None },

        // Semi-structured
        "VARIANT" | "OBJECT" | "ARRAY" => UniversalDataType::Json,

        _ => UniversalDataType::Unknown,
    }
}

// Helper functions for parsing type parameters

fn parse_decimal_type(type_str: &str) -> Option<UniversalDataType> {
    let re =
        regex::Regex::new(r"(?i)(?:decimal|numeric|dec|number)\s*\(\s*(\d+)\s*(?:,\s*(\d+))?\s*\)")
            .ok()?;

    if let Some(captures) = re.captures(type_str) {
        let precision: u32 = captures.get(1)?.as_str().parse().ok()?;
        let scale: u32 = captures
            .get(2)
            .map(|m| m.as_str().parse().ok())
            .flatten()
            .unwrap_or(0);

        Some(UniversalDataType::Decimal { precision, scale })
    } else {
        None
    }
}

fn parse_varchar_type(type_str: &str) -> Option<UniversalDataType> {
    let re = regex::Regex::new(r"(?i)varchar(?:2)?\s*\(\s*(\d+|max)\s*\)").ok()?;

    if let Some(captures) = re.captures(type_str) {
        let length_str = captures.get(1)?.as_str();
        let max_length = if length_str.eq_ignore_ascii_case("max") {
            None
        } else {
            Some(length_str.parse().ok()?)
        };

        Some(UniversalDataType::String { max_length })
    } else {
        None
    }
}

fn parse_varchar2_type(type_str: &str) -> Option<UniversalDataType> {
    let re = regex::Regex::new(r"(?i)varchar2\s*\(\s*(\d+)\s*(?:byte|char)?\s*\)").ok()?;

    if let Some(captures) = re.captures(type_str) {
        let max_length: usize = captures.get(1)?.as_str().parse().ok()?;
        Some(UniversalDataType::String {
            max_length: Some(max_length),
        })
    } else {
        None
    }
}

fn parse_char_type(type_str: &str) -> Option<UniversalDataType> {
    let re = regex::Regex::new(r"(?i)n?char(?:acter)?\s*\(\s*(\d+)\s*\)").ok()?;

    if let Some(captures) = re.captures(type_str) {
        let length: usize = captures.get(1)?.as_str().parse().ok()?;
        Some(UniversalDataType::Char { length })
    } else {
        None
    }
}

fn parse_oracle_number(type_str: &str) -> Option<UniversalDataType> {
    let re = regex::Regex::new(r"(?i)number\s*\(\s*(\d+)\s*(?:,\s*(\d+))?\s*\)").ok()?;

    if let Some(captures) = re.captures(type_str) {
        let precision: u32 = captures.get(1)?.as_str().parse().ok()?;
        let scale: u32 = captures
            .get(2)
            .map(|m| m.as_str().parse().ok())
            .flatten()
            .unwrap_or(0);

        // Oracle NUMBER with no scale is often used as integer
        if scale == 0 && precision <= 38 {
            // Map to appropriate integer size based on precision
            let bits = if precision <= 5 {
                Some(16)
            } else if precision <= 10 {
                Some(32)
            } else {
                Some(64)
            };
            Some(UniversalDataType::Integer { bits })
        } else {
            Some(UniversalDataType::Decimal { precision, scale })
        }
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_postgres_type_conversion() {
        assert_eq!(
            postgres_type_to_universal("integer"),
            UniversalDataType::Integer { bits: Some(32) }
        );
        assert_eq!(
            postgres_type_to_universal("varchar(255)"),
            UniversalDataType::String {
                max_length: Some(255)
            }
        );
        assert_eq!(
            postgres_type_to_universal("timestamp with time zone"),
            UniversalDataType::DateTime {
                with_timezone: true
            }
        );
        assert_eq!(postgres_type_to_universal("jsonb"), UniversalDataType::Json);
    }

    #[test]
    fn test_mysql_type_conversion() {
        assert_eq!(
            mysql_type_to_universal("int"),
            UniversalDataType::Integer { bits: Some(32) }
        );
        assert_eq!(
            mysql_type_to_universal("text"),
            UniversalDataType::String {
                max_length: Some(65535)
            }
        );
        assert_eq!(
            mysql_type_to_universal("datetime"),
            UniversalDataType::DateTime {
                with_timezone: false
            }
        );
        assert_eq!(mysql_type_to_universal("json"), UniversalDataType::Json);
    }

    #[test]
    fn test_oracle_type_conversion() {
        assert_eq!(
            oracle_type_to_universal("NUMBER(10,2)"),
            UniversalDataType::Decimal {
                precision: 10,
                scale: 2
            }
        );
        assert_eq!(
            oracle_type_to_universal("VARCHAR2(100)"),
            UniversalDataType::String {
                max_length: Some(100)
            }
        );
        assert_eq!(oracle_type_to_universal("CLOB"), UniversalDataType::Text);
    }

    #[test]
    fn test_decimal_parsing() {
        assert_eq!(
            parse_decimal_type("DECIMAL(10,2)"),
            Some(UniversalDataType::Decimal {
                precision: 10,
                scale: 2
            })
        );
        assert_eq!(
            parse_decimal_type("numeric(5)"),
            Some(UniversalDataType::Decimal {
                precision: 5,
                scale: 0
            })
        );
    }
}
