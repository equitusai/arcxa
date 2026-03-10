//! Universal Type System
//!
//! Defines a universal data type system that can represent types from any datasource.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Universal data type representation
///
/// This enum can represent data types from various sources:
/// - Databases (PostgreSQL, MySQL, Oracle, DB2, etc.)
/// - File formats (CSV, Parquet, JSON, etc.)
/// - Semi-structured data (JSON, XML)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum UniversalDataType {
    // ========== Numeric Types ==========
    /// Integer with optional bit size (8, 16, 32, 64)
    Integer {
        bits: Option<u8>,
    },

    /// Floating point with optional bit size (32, 64)
    Float {
        bits: Option<u8>,
    },

    /// Fixed-precision decimal
    Decimal {
        precision: u32,
        scale: u32,
    },

    // ========== String Types ==========
    /// Variable-length string with optional max length
    String {
        max_length: Option<usize>,
    },

    /// Unlimited text
    Text,

    /// Fixed-length string
    Char {
        length: usize,
    },

    // ========== Temporal Types ==========
    /// Date without time
    Date,

    /// Time without date
    Time {
        with_timezone: bool,
    },

    /// Date and time
    DateTime {
        with_timezone: bool,
    },

    /// Time interval/duration
    Interval,

    /// Unix timestamp
    Timestamp,

    // ========== Boolean ==========
    Boolean,

    // ========== Binary ==========
    /// Binary data with optional max length
    Binary {
        max_length: Option<usize>,
    },

    // ========== Semi-Structured ==========
    /// JSON data
    Json,

    /// XML data
    Xml,

    // ========== Special Types ==========
    /// UUID/GUID
    Uuid,

    /// Array of elements
    Array {
        element_type: Box<UniversalDataType>,
    },

    /// Enumeration with possible values
    Enum {
        values: Vec<String>,
    },

    /// Composite/Struct type
    Struct {
        fields: Vec<StructField>,
    },

    /// Unknown or unsupported type
    Unknown,
}

/// Field within a struct type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct StructField {
    pub name: String,
    pub data_type: UniversalDataType,
    pub nullable: bool,
}

impl UniversalDataType {
    /// Check if this type is numeric
    pub fn is_numeric(&self) -> bool {
        matches!(
            self,
            UniversalDataType::Integer { .. }
                | UniversalDataType::Float { .. }
                | UniversalDataType::Decimal { .. }
        )
    }

    /// Check if this type is string-like
    pub fn is_string(&self) -> bool {
        matches!(
            self,
            UniversalDataType::String { .. }
                | UniversalDataType::Text
                | UniversalDataType::Char { .. }
        )
    }

    /// Check if this type is temporal
    pub fn is_temporal(&self) -> bool {
        matches!(
            self,
            UniversalDataType::Date
                | UniversalDataType::Time { .. }
                | UniversalDataType::DateTime { .. }
                | UniversalDataType::Interval
                | UniversalDataType::Timestamp
        )
    }

    /// Check if types are compatible for assignment
    pub fn is_compatible_with(&self, other: &UniversalDataType) -> bool {
        match (self, other) {
            // Same types are always compatible
            (a, b) if a == b => true,

            // Integer promotions
            (
                UniversalDataType::Integer { bits: Some(a) },
                UniversalDataType::Integer { bits: Some(b) },
            ) => {
                a <= b // Can assign smaller int to larger
            }

            // Float promotions
            (UniversalDataType::Integer { .. }, UniversalDataType::Float { .. }) => true,
            (
                UniversalDataType::Float { bits: Some(32) },
                UniversalDataType::Float { bits: Some(64) },
            ) => true,

            // String conversions
            (UniversalDataType::Char { .. }, UniversalDataType::String { .. }) => true,
            (UniversalDataType::Char { .. }, UniversalDataType::Text) => true,
            (UniversalDataType::String { .. }, UniversalDataType::Text) => true,

            // Temporal conversions
            (UniversalDataType::Date, UniversalDataType::DateTime { .. }) => true,
            (UniversalDataType::Time { .. }, UniversalDataType::DateTime { .. }) => true,

            // Unknown can be assigned from anything
            (_, UniversalDataType::Unknown) => true,

            _ => false,
        }
    }

    /// Get the storage size estimate in bytes
    pub fn storage_size(&self) -> Option<usize> {
        match self {
            UniversalDataType::Integer { bits: Some(b) } => Some(*b as usize / 8),
            UniversalDataType::Float { bits: Some(b) } => Some(*b as usize / 8),
            UniversalDataType::Boolean => Some(1),
            UniversalDataType::Date => Some(4),
            UniversalDataType::Time { .. } => Some(8),
            UniversalDataType::DateTime { .. } => Some(8),
            UniversalDataType::Timestamp => Some(8),
            UniversalDataType::Uuid => Some(16),
            UniversalDataType::Char { length } => Some(*length),
            UniversalDataType::Decimal { precision, .. } => Some((precision + 1) as usize / 2),
            _ => None, // Variable length or unknown
        }
    }
}

impl fmt::Display for UniversalDataType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UniversalDataType::Integer { bits: Some(b) } => write!(f, "INT{}", b),
            UniversalDataType::Integer { bits: None } => write!(f, "INTEGER"),
            UniversalDataType::Float { bits: Some(b) } => write!(f, "FLOAT{}", b),
            UniversalDataType::Float { bits: None } => write!(f, "FLOAT"),
            UniversalDataType::Decimal { precision, scale } => {
                write!(f, "DECIMAL({},{})", precision, scale)
            }
            UniversalDataType::String {
                max_length: Some(len),
            } => {
                write!(f, "VARCHAR({})", len)
            }
            UniversalDataType::String { max_length: None } => write!(f, "VARCHAR"),
            UniversalDataType::Text => write!(f, "TEXT"),
            UniversalDataType::Char { length } => write!(f, "CHAR({})", length),
            UniversalDataType::Date => write!(f, "DATE"),
            UniversalDataType::Time {
                with_timezone: true,
            } => write!(f, "TIME WITH TIMEZONE"),
            UniversalDataType::Time {
                with_timezone: false,
            } => write!(f, "TIME"),
            UniversalDataType::DateTime {
                with_timezone: true,
            } => {
                write!(f, "TIMESTAMP WITH TIMEZONE")
            }
            UniversalDataType::DateTime {
                with_timezone: false,
            } => write!(f, "TIMESTAMP"),
            UniversalDataType::Interval => write!(f, "INTERVAL"),
            UniversalDataType::Timestamp => write!(f, "UNIX_TIMESTAMP"),
            UniversalDataType::Boolean => write!(f, "BOOLEAN"),
            UniversalDataType::Binary {
                max_length: Some(len),
            } => {
                write!(f, "BINARY({})", len)
            }
            UniversalDataType::Binary { max_length: None } => write!(f, "BLOB"),
            UniversalDataType::Json => write!(f, "JSON"),
            UniversalDataType::Xml => write!(f, "XML"),
            UniversalDataType::Uuid => write!(f, "UUID"),
            UniversalDataType::Array { element_type } => write!(f, "ARRAY<{}>", element_type),
            UniversalDataType::Enum { values } => {
                write!(f, "ENUM({})", values.join(", "))
            }
            UniversalDataType::Struct { fields } => {
                let field_strs: Vec<_> = fields
                    .iter()
                    .map(|f| format!("{}: {}", f.name, f.data_type))
                    .collect();
                write!(f, "STRUCT<{}>", field_strs.join(", "))
            }
            UniversalDataType::Unknown => write!(f, "UNKNOWN"),
        }
    }
}

/// Source type enumeration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum SourceType {
    // File formats
    CsvFile,
    ExcelFile,
    ParquetFile,
    JsonFile,
    XmlFile,
    AvroFile,

    // Databases
    PostgreSQL,
    MySQL,
    Oracle,
    DB2,
    SQLServer,
    SQLite,
    Snowflake,
    Redshift,
    BigQuery,

    // Cloud storage
    S3Parquet,
    AzureBlob,
    GoogleCloudStorage,

    // Semantic formats
    RdfTriples,
    GraphQL,

    // Streaming
    Kafka,
    Kinesis,

    // Custom
    Custom(String),
}

impl fmt::Display for SourceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SourceType::Custom(name) => write!(f, "{}", name),
            other => write!(f, "{:?}", other),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_compatibility() {
        let int32 = UniversalDataType::Integer { bits: Some(32) };
        let int64 = UniversalDataType::Integer { bits: Some(64) };
        let float32 = UniversalDataType::Float { bits: Some(32) };

        // Integer promotion
        assert!(int32.is_compatible_with(&int64));
        assert!(!int64.is_compatible_with(&int32)); // Can't assign larger to smaller

        // Int to float
        assert!(int32.is_compatible_with(&float32));

        // Same type
        assert!(int32.is_compatible_with(&int32));
    }

    #[test]
    fn test_type_categories() {
        let int_type = UniversalDataType::Integer { bits: Some(32) };
        let string_type = UniversalDataType::String { max_length: None };
        let date_type = UniversalDataType::Date;

        assert!(int_type.is_numeric());
        assert!(!int_type.is_string());
        assert!(!int_type.is_temporal());

        assert!(!string_type.is_numeric());
        assert!(string_type.is_string());
        assert!(!string_type.is_temporal());

        assert!(!date_type.is_numeric());
        assert!(!date_type.is_string());
        assert!(date_type.is_temporal());
    }

    #[test]
    fn test_storage_size() {
        assert_eq!(
            UniversalDataType::Integer { bits: Some(32) }.storage_size(),
            Some(4)
        );
        assert_eq!(
            UniversalDataType::Float { bits: Some(64) }.storage_size(),
            Some(8)
        );
        assert_eq!(UniversalDataType::Boolean.storage_size(), Some(1));
        assert_eq!(UniversalDataType::Uuid.storage_size(), Some(16));
        assert_eq!(UniversalDataType::Text.storage_size(), None); // Variable
    }

    #[test]
    fn test_display() {
        assert_eq!(
            UniversalDataType::Integer { bits: Some(32) }.to_string(),
            "INT32"
        );
        assert_eq!(
            UniversalDataType::Decimal {
                precision: 10,
                scale: 2
            }
            .to_string(),
            "DECIMAL(10,2)"
        );
        assert_eq!(
            UniversalDataType::String {
                max_length: Some(255)
            }
            .to_string(),
            "VARCHAR(255)"
        );
    }
}
