//! Compatibility Layer for Existing Type Systems
//!
//! Provides From/Into trait implementations for automatic conversion
//! between legacy type enums and the unified UniversalDataType.

use super::types::UniversalDataType;

// ========== Inference Mapping DataType ==========

use crate::inference::mapping::DataType as MappingDataType;

impl From<MappingDataType> for UniversalDataType {
    fn from(dt: MappingDataType) -> Self {
        match dt {
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
            MappingDataType::Decimal { precision, scale } => {
                UniversalDataType::Decimal { precision, scale }
            }
            MappingDataType::Binary => UniversalDataType::Binary { max_length: None },
            MappingDataType::Json => UniversalDataType::Json,
            MappingDataType::Unknown => UniversalDataType::Unknown,
        }
    }
}

impl From<UniversalDataType> for MappingDataType {
    fn from(udt: UniversalDataType) -> Self {
        match udt {
            UniversalDataType::Integer { .. } => MappingDataType::Integer,
            UniversalDataType::Float { .. } => MappingDataType::Float,
            UniversalDataType::Decimal { precision, scale } => {
                MappingDataType::Decimal { precision, scale }
            }
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
}

// ========== Profiling DataType ==========

use crate::profiling::DataType as ProfilingDataType;

impl From<ProfilingDataType> for UniversalDataType {
    fn from(dt: ProfilingDataType) -> Self {
        match dt {
            ProfilingDataType::Integer => UniversalDataType::Integer { bits: Some(64) },
            ProfilingDataType::Float => UniversalDataType::Float { bits: Some(64) },
            ProfilingDataType::String => UniversalDataType::String { max_length: None },
            ProfilingDataType::Boolean => UniversalDataType::Boolean,
            ProfilingDataType::DateTime => UniversalDataType::DateTime {
                with_timezone: false,
            },
            ProfilingDataType::Json => UniversalDataType::Json,
            ProfilingDataType::Unknown => UniversalDataType::Unknown,
        }
    }
}

impl From<UniversalDataType> for ProfilingDataType {
    fn from(udt: UniversalDataType) -> Self {
        match udt {
            UniversalDataType::Integer { .. } => ProfilingDataType::Integer,
            UniversalDataType::Float { .. } | UniversalDataType::Decimal { .. } => {
                ProfilingDataType::Float
            }
            UniversalDataType::String { .. }
            | UniversalDataType::Text
            | UniversalDataType::Char { .. } => ProfilingDataType::String,
            UniversalDataType::Boolean => ProfilingDataType::Boolean,
            UniversalDataType::DateTime { .. }
            | UniversalDataType::Time { .. }
            | UniversalDataType::Date
            | UniversalDataType::Timestamp => ProfilingDataType::DateTime,
            UniversalDataType::Json | UniversalDataType::Xml => ProfilingDataType::Json,
            _ => ProfilingDataType::Unknown,
        }
    }
}

// ========== Catalog FieldType ==========
// Note: FieldType in catalog/connectors is for connector configuration fields,
// not data types. These are semantic types like URL, Hostname, Port, etc.

use crate::catalog::connectors::FieldType as CatalogFieldType;

impl From<CatalogFieldType> for UniversalDataType {
    fn from(ft: CatalogFieldType) -> Self {
        match ft {
            CatalogFieldType::String => UniversalDataType::String { max_length: None },
            CatalogFieldType::Integer => UniversalDataType::Integer { bits: Some(32) },
            CatalogFieldType::Boolean => UniversalDataType::Boolean,
            CatalogFieldType::Url => UniversalDataType::String {
                max_length: Some(2048),
            },
            CatalogFieldType::Hostname => UniversalDataType::String {
                max_length: Some(255),
            },
            CatalogFieldType::Port => UniversalDataType::Integer { bits: Some(16) },
            CatalogFieldType::FilePath => UniversalDataType::String {
                max_length: Some(4096),
            },
        }
    }
}

impl From<UniversalDataType> for CatalogFieldType {
    fn from(udt: UniversalDataType) -> Self {
        match udt {
            UniversalDataType::Boolean => CatalogFieldType::Boolean,
            UniversalDataType::Integer { bits } if bits == Some(16) => CatalogFieldType::Port,
            UniversalDataType::Integer { .. } => CatalogFieldType::Integer,
            // Default all string-like types to String
            _ => CatalogFieldType::String,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mapping_datatype_conversion() {
        let mapping_int = MappingDataType::Integer;
        let universal: UniversalDataType = mapping_int.into();
        assert_eq!(universal, UniversalDataType::Integer { bits: None });

        let back: MappingDataType = universal.into();
        assert_eq!(back, MappingDataType::Integer);
    }

    #[test]
    fn test_profiling_datatype_conversion() {
        // Test conversion works (ProfilingDataType doesn't derive PartialEq)
        let profiling_str = ProfilingDataType::String;
        let universal: UniversalDataType = profiling_str.into();
        assert_eq!(universal, UniversalDataType::String { max_length: None });

        // Test roundtrip (just verify it doesn't panic)
        let _back: ProfilingDataType = universal.into();
    }

    #[test]
    fn test_catalog_fieldtype_conversion() {
        let catalog_port = CatalogFieldType::Port;
        let universal: UniversalDataType = catalog_port.into();
        assert_eq!(universal, UniversalDataType::Integer { bits: Some(16) });

        let back: CatalogFieldType = universal.into();
        assert_eq!(back, CatalogFieldType::Port);
    }

    #[test]
    fn test_decimal_conversion() {
        let mapping_decimal = MappingDataType::Decimal {
            precision: 10,
            scale: 2,
        };
        let universal: UniversalDataType = mapping_decimal.into();
        assert_eq!(
            universal,
            UniversalDataType::Decimal {
                precision: 10,
                scale: 2
            }
        );

        let back: MappingDataType = universal.into();
        assert_eq!(
            back,
            MappingDataType::Decimal {
                precision: 10,
                scale: 2
            }
        );
    }

    #[test]
    fn test_roundtrip_conversions() {
        // Test that conversions are idempotent where possible
        let types = vec![
            MappingDataType::Integer,
            MappingDataType::Float,
            MappingDataType::String,
            MappingDataType::Boolean,
            MappingDataType::Date,
            MappingDataType::DateTime,
        ];

        for original in types {
            let universal: UniversalDataType = original.clone().into();
            let back: MappingDataType = universal.into();
            assert_eq!(
                original, back,
                "Roundtrip conversion failed for {:?}",
                original
            );
        }
    }
}
