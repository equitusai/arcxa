//! Type Conversion Utilities
//!
//! Provides conversion between different type representations in the system.

use std::convert::TryFrom;
use thiserror::Error;

use super::field::UnifiedField;
use super::types::{SourceType, UniversalDataType};

// Import existing types from different modules (for conversion)
use crate::catalog::api_types::ColumnDefinition;
use crate::inference::mapping::DataType as MappingDataType;

/// Result type for conversions
pub type ConversionResult<T> = Result<T, ConversionError>;

/// Errors that can occur during type conversion
#[derive(Debug, Error)]
pub enum ConversionError {
    #[error("Unsupported type conversion from {from} to {to}")]
    UnsupportedConversion { from: String, to: String },

    #[error("Loss of precision converting from {from} to {to}")]
    PrecisionLoss { from: String, to: String },

    #[error("Invalid type string: {0}")]
    InvalidTypeString(String),

    #[error("Conversion would result in data loss")]
    DataLoss,
}

/// Type converter for managing conversions between different type systems
pub struct TypeConverter;

impl TypeConverter {
    /// Convert from mapping DataType to UniversalDataType
    pub fn from_mapping_type(mapping_type: &MappingDataType) -> UniversalDataType {
        match mapping_type {
            MappingDataType::Integer => UniversalDataType::Integer { bits: None },
            MappingDataType::Float => UniversalDataType::Float { bits: None },
            MappingDataType::String => UniversalDataType::String { max_length: None },
            MappingDataType::Boolean => UniversalDataType::Boolean,
            MappingDataType::Date => UniversalDataType::Date,
            MappingDataType::DateTime => UniversalDataType::DateTime {
                with_timezone: false,
            },
            MappingDataType::Time => UniversalDataType::Time {
                with_timezone: false,
            },
            MappingDataType::Decimal { precision, scale } => UniversalDataType::Decimal {
                precision: *precision,
                scale: *scale,
            },
            MappingDataType::Binary => UniversalDataType::Binary { max_length: None },
            MappingDataType::Json => UniversalDataType::Json,
            MappingDataType::Unknown => UniversalDataType::Unknown,
        }
    }

    /// Convert from UniversalDataType to mapping DataType
    pub fn to_mapping_type(universal_type: &UniversalDataType) -> MappingDataType {
        match universal_type {
            UniversalDataType::Integer { .. } => MappingDataType::Integer,
            UniversalDataType::Float { .. } => MappingDataType::Float,
            UniversalDataType::Decimal { precision, scale } => MappingDataType::Decimal {
                precision: *precision,
                scale: *scale,
            },
            UniversalDataType::String { .. }
            | UniversalDataType::Text
            | UniversalDataType::Char { .. } => MappingDataType::String,
            UniversalDataType::Boolean => MappingDataType::Boolean,
            UniversalDataType::Date => MappingDataType::Date,
            UniversalDataType::DateTime { .. } | UniversalDataType::Timestamp => {
                MappingDataType::DateTime
            }
            UniversalDataType::Time { .. } => MappingDataType::Time,
            UniversalDataType::Binary { .. } => MappingDataType::Binary,
            UniversalDataType::Json | UniversalDataType::Xml => MappingDataType::Json,
            _ => MappingDataType::Unknown,
        }
    }

    /// Convert ColumnDefinition to UnifiedField
    pub fn column_to_unified_field(column: &ColumnDefinition, source_ref: String) -> UnifiedField {
        use super::source::database_type_to_universal;

        // Parse the data_type string based on common patterns
        let universal_type = if let Ok(source_type) = detect_source_type(&column.data_type) {
            database_type_to_universal(&column.data_type, source_type)
        } else {
            // Fallback to basic parsing
            parse_generic_type(&column.data_type)
        };

        let mut field = UnifiedField::new(column.name.clone(), universal_type)
            .with_nullable(column.nullable)
            .with_primary_key(column.primary_key);

        field.source_ref = source_ref;

        if let Some(default) = &column.default_value {
            field.constraints.default_value = Some(default.clone());
        }

        if let Some(semantic) = &column.semantic_type {
            // Convert SemanticType if available
            field.semantic.semantic_type = Some(convert_semantic_type(semantic));
        }

        field
    }

    /// Check if conversion between types is safe (no data loss)
    pub fn is_safe_conversion(from: &UniversalDataType, to: &UniversalDataType) -> bool {
        match (from, to) {
            // Same type is always safe
            (a, b) if a == b => true,

            // Integer promotions (smaller to larger)
            (
                UniversalDataType::Integer {
                    bits: Some(from_bits),
                },
                UniversalDataType::Integer {
                    bits: Some(to_bits),
                },
            ) => from_bits <= to_bits,

            // Integer to float (generally safe, though precision may be lost for very large integers)
            (UniversalDataType::Integer { .. }, UniversalDataType::Float { .. }) => true,

            // Float promotions
            (
                UniversalDataType::Float { bits: Some(32) },
                UniversalDataType::Float { bits: Some(64) },
            ) => true,

            // Integer/Float to Decimal
            (UniversalDataType::Integer { .. }, UniversalDataType::Decimal { .. }) => true,
            (UniversalDataType::Float { .. }, UniversalDataType::Decimal { .. }) => true,

            // String promotions
            (
                UniversalDataType::Char { length: from_len },
                UniversalDataType::Char { length: to_len },
            ) => from_len <= to_len,
            (UniversalDataType::Char { .. }, UniversalDataType::String { .. }) => true,
            (UniversalDataType::Char { .. }, UniversalDataType::Text) => true,
            (
                UniversalDataType::String {
                    max_length: Some(from_len),
                },
                UniversalDataType::String {
                    max_length: Some(to_len),
                },
            ) => from_len <= to_len,
            (
                UniversalDataType::String {
                    max_length: Some(_),
                },
                UniversalDataType::String { max_length: None },
            ) => true,
            (UniversalDataType::String { .. }, UniversalDataType::Text) => true,

            // Date/Time promotions
            (UniversalDataType::Date, UniversalDataType::DateTime { .. }) => true,
            (UniversalDataType::Time { .. }, UniversalDataType::DateTime { .. }) => true,

            // Binary promotions
            (
                UniversalDataType::Binary {
                    max_length: Some(from_len),
                },
                UniversalDataType::Binary {
                    max_length: Some(to_len),
                },
            ) => from_len <= to_len,
            (
                UniversalDataType::Binary {
                    max_length: Some(_),
                },
                UniversalDataType::Binary { max_length: None },
            ) => true,

            _ => false,
        }
    }

    /// Get conversion hints for incompatible types
    pub fn get_conversion_hint(from: &UniversalDataType, to: &UniversalDataType) -> String {
        match (from, to) {
            // String to numeric
            (f, t) if f.is_string() && t.is_numeric() => {
                format!(
                    "Use CAST({} AS {}) - ensure string contains valid numeric values",
                    from, to
                )
            }

            // Numeric to string
            (f, t) if f.is_numeric() && t.is_string() => {
                format!("Use TO_STRING({}) or CAST({} AS {})", from, from, to)
            }

            // Date/Time conversions
            (UniversalDataType::String { .. }, t) if t.is_temporal() => {
                format!("Use TO_DATE/TO_TIMESTAMP with appropriate format string")
            }
            (f, UniversalDataType::String { .. }) if f.is_temporal() => {
                format!("Use DATE_FORMAT/TO_CHAR with format string")
            }

            // Precision loss warnings
            (UniversalDataType::Float { .. }, UniversalDataType::Integer { .. }) => {
                "Warning: Converting float to integer will truncate decimal places".to_string()
            }
            (UniversalDataType::Decimal { .. }, UniversalDataType::Integer { .. }) => {
                "Warning: Converting decimal to integer will truncate decimal places".to_string()
            }

            // Default
            _ => format!(
                "Conversion from {} to {} may require custom transformation",
                from, to
            ),
        }
    }
}

// Helper functions

fn detect_source_type(type_str: &str) -> Result<SourceType, ConversionError> {
    // Try to detect database type from common patterns
    let lower = type_str.to_lowercase();

    if lower.contains("varchar2") || lower.contains("number") || lower.contains("clob") {
        Ok(SourceType::Oracle)
    } else if lower.contains("serial") || lower.contains("jsonb") {
        Ok(SourceType::PostgreSQL)
    } else if lower.contains("tinyint") || lower.contains("mediumtext") {
        Ok(SourceType::MySQL)
    } else if lower.contains("nvarchar") || lower.contains("uniqueidentifier") {
        Ok(SourceType::SQLServer)
    } else if lower.contains("variant") {
        Ok(SourceType::Snowflake)
    } else {
        Err(ConversionError::InvalidTypeString(type_str.to_string()))
    }
}

fn parse_generic_type(type_str: &str) -> UniversalDataType {
    let lower = type_str.to_lowercase();

    if lower.contains("int") {
        UniversalDataType::Integer { bits: None }
    } else if lower.contains("float") || lower.contains("double") || lower.contains("real") {
        UniversalDataType::Float { bits: None }
    } else if lower.contains("decimal") || lower.contains("numeric") {
        UniversalDataType::Decimal {
            precision: 38,
            scale: 10,
        }
    } else if lower.contains("char") || lower.contains("text") || lower.contains("string") {
        UniversalDataType::String { max_length: None }
    } else if lower.contains("bool") {
        UniversalDataType::Boolean
    } else if lower.contains("date") && !lower.contains("time") {
        UniversalDataType::Date
    } else if lower.contains("time") && !lower.contains("stamp") {
        UniversalDataType::Time {
            with_timezone: false,
        }
    } else if lower.contains("timestamp") || lower.contains("datetime") {
        UniversalDataType::DateTime {
            with_timezone: false,
        }
    } else if lower.contains("binary") || lower.contains("blob") {
        UniversalDataType::Binary { max_length: None }
    } else if lower.contains("json") {
        UniversalDataType::Json
    } else if lower.contains("xml") {
        UniversalDataType::Xml
    } else if lower.contains("uuid") || lower.contains("guid") {
        UniversalDataType::Uuid
    } else {
        UniversalDataType::Unknown
    }
}

fn convert_semantic_type(
    semantic: &crate::inference::types::SemanticType,
) -> super::field::SemanticType {
    use super::field::SemanticType as FieldType;
    use crate::inference::types::SemanticType as InferenceType;

    match semantic {
        InferenceType::Email => FieldType::Email,
        InferenceType::PhoneNumber => FieldType::PhoneNumber,
        InferenceType::URL => FieldType::URL,
        InferenceType::IPAddress => FieldType::IPAddress,
        InferenceType::UUID => FieldType::UUID,
        InferenceType::CreditCardNumber => FieldType::CreditCardNumber,
        InferenceType::SSN => FieldType::SocialSecurityNumber,
        InferenceType::PostalCode => FieldType::PostalCode,
        InferenceType::Country => FieldType::Country,
        InferenceType::CurrencyAmount => FieldType::Currency,
        InferenceType::CurrencyCode => FieldType::Currency,
        InferenceType::Date => FieldType::BirthDate, // Approximation
        InferenceType::PersonName => FieldType::FullName,
        InferenceType::OrganizationName => FieldType::CompanyName,
        InferenceType::Address => FieldType::FullAddress,
        InferenceType::Custom(s) => FieldType::Custom(s.clone()),
        _ => FieldType::Custom("Unknown".to_string()), // Catch-all for any unmapped types
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_conversions() {
        let int32 = UniversalDataType::Integer { bits: Some(32) };
        let int64 = UniversalDataType::Integer { bits: Some(64) };
        let float32 = UniversalDataType::Float { bits: Some(32) };
        let varchar100 = UniversalDataType::String {
            max_length: Some(100),
        };
        let text = UniversalDataType::Text;

        assert!(TypeConverter::is_safe_conversion(&int32, &int64));
        assert!(!TypeConverter::is_safe_conversion(&int64, &int32));
        assert!(TypeConverter::is_safe_conversion(&int32, &float32));
        assert!(TypeConverter::is_safe_conversion(&varchar100, &text));
    }

    #[test]
    fn test_conversion_hints() {
        let string_type = UniversalDataType::String { max_length: None };
        let int_type = UniversalDataType::Integer { bits: Some(32) };
        let float_type = UniversalDataType::Float { bits: Some(64) };

        let hint = TypeConverter::get_conversion_hint(&string_type, &int_type);
        assert!(hint.contains("CAST"));

        let hint = TypeConverter::get_conversion_hint(&float_type, &int_type);
        assert!(hint.contains("truncate"));
    }

    #[test]
    fn test_mapping_type_conversion() {
        let mapping_int = MappingDataType::Integer;
        let universal = TypeConverter::from_mapping_type(&mapping_int);
        assert_eq!(universal, UniversalDataType::Integer { bits: None });

        let back = TypeConverter::to_mapping_type(&universal);
        assert_eq!(back, MappingDataType::Integer);
    }

    #[test]
    fn test_generic_type_parsing() {
        assert_eq!(
            parse_generic_type("integer"),
            UniversalDataType::Integer { bits: None }
        );
        assert_eq!(
            parse_generic_type("varchar"),
            UniversalDataType::String { max_length: None }
        );
        assert_eq!(parse_generic_type("boolean"), UniversalDataType::Boolean);
        assert_eq!(
            parse_generic_type("timestamp"),
            UniversalDataType::DateTime {
                with_timezone: false
            }
        );
    }
}
