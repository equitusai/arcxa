//! PostgreSQL Database Profiler
//!
//! Implements DataProfiler trait for PostgreSQL databases using existing inference logic

use super::profiler::*;
use super::semantic_detector::SemanticDetector;
use super::*;
use crate::inference::types::*;
use anyhow::{Context, Result};
use std::collections::HashMap;

/// PostgreSQL-specific profiler implementation
///
/// Note: This profiler converts pre-fetched TableMetadata to UnifiedSchema.
/// Actual database querying should be done by the connector layer, which can
/// use the PostgresStatsExtractor to gather statistics.
pub struct PostgresProfiler {
    /// Connection string or identifier (for reference only)
    pub connection_id: String,
    /// Semantic type detector
    pub semantic_detector: SemanticDetector,
}

impl PostgresProfiler {
    /// Create a new PostgreSQL profiler
    pub fn new(connection_id: String) -> Self {
        Self {
            connection_id,
            semantic_detector: SemanticDetector::new(),
        }
    }

    /// Convert TableMetadata to UnifiedSchema
    pub fn convert_table_metadata(
        &self,
        table_meta: TableMetadata,
        connection_id: &str,
    ) -> UnifiedSchema {
        let mut schema = UnifiedSchema::new(
            table_meta.name.clone(),
            SourceType::PostgreSQL,
            connection_id.to_string(),
        );

        // Convert columns to unified fields
        for (position, column) in table_meta.columns.iter().enumerate() {
            let mut unified_field = UnifiedField::new(
                column.name.clone(),
                convert_inference_data_type(&column.native_type),
            );

            unified_field.position = column.ordinal_position as usize;
            unified_field.nullable = column.nullable;

            // Set constraints
            unified_field.constraints.primary_key = column.is_primary_key;
            unified_field.constraints.not_null = !column.nullable;

            if let Some(default) = &column.default_value {
                unified_field.constraints.default_value = Some(default.clone());
            }

            // Convert column statistics if available
            if let Some(ref col_stats) = column.statistics {
                unified_field.profile = Some(convert_column_statistics(col_stats));
            }

            // Use semantic detector to automatically detect or enhance semantic types
            let sample_values: Vec<Option<String>> = column
                .statistics
                .as_ref()
                .and_then(|stats| stats.most_common_values.as_ref())
                .map(|mcv| mcv.iter().map(|vf| Some(vf.value.clone())).collect())
                .unwrap_or_default();

            if let Some(detection_result) =
                self.semantic_detector.detect(&column.name, &sample_values)
            {
                // Apply detected semantic type
                unified_field.semantic.semantic_type = Some(detection_result.semantic_type);

                // Apply suggested sensitivity level
                if let Some(sensitivity) = detection_result.suggested_sensitivity {
                    unified_field.semantic.sensitivity = Some(sensitivity);
                }

                // Update last classified timestamp
                unified_field.semantic.last_classified = Some(chrono::Utc::now());
            } else {
                // Fall back to existing semantic type if no automatic detection
                if let Some(ref semantic) = column.semantic_type {
                    unified_field.semantic.semantic_type =
                        Some(convert_inference_semantic_type(semantic));
                }

                // Set data classification
                if let Some(ref classification) = column.classification {
                    unified_field.semantic.sensitivity =
                        Some(convert_data_classification(classification));
                }
            }

            // Add comment as metadata
            if let Some(ref comment) = column.comment {
                unified_field.metadata.insert(
                    "comment".to_string(),
                    serde_json::Value::String(comment.clone()),
                );
            }

            schema.add_field(unified_field);
        }

        // Add table-level statistics
        if let Some(ref table_stats) = table_meta.statistics {
            schema.row_count = Some(table_stats.actual_row_count);
            schema.size_bytes = Some(table_stats.size_bytes);

            if let Some(last_mod) = table_stats.last_modified {
                schema.metadata.insert(
                    "last_modified".to_string(),
                    serde_json::Value::String(last_mod.to_rfc3339()),
                );
            }
        } else if let Some(estimated_rows) = table_meta.estimated_rows {
            schema.row_count = Some(estimated_rows);
        }

        // Add table metadata
        schema.metadata.insert(
            "schema_name".to_string(),
            serde_json::Value::String(table_meta.schema.clone()),
        );

        schema.metadata.insert(
            "table_type".to_string(),
            serde_json::Value::String(format!("{:?}", table_meta.table_type)),
        );

        schema.last_profiled = Some(chrono::Utc::now());

        schema
    }

    /// Extract relationships from TableMetadata
    pub fn extract_relationships(&self, table_meta: &TableMetadata) -> Vec<RelationshipInfo> {
        let mut relationships = Vec::new();

        if let Some(ref rel_meta) = table_meta.relationships {
            // Foreign keys from this table
            for fk in &rel_meta.foreign_keys {
                for (source_col, target_col) in fk.columns.iter().zip(&fk.referenced_columns) {
                    relationships.push(RelationshipInfo {
                        source: table_meta.name.clone(),
                        source_field: source_col.clone(),
                        target: fk.referenced_table.clone(),
                        target_field: target_col.clone(),
                        relationship_type: determine_relationship_type(fk),
                        confidence: 1.0, // High confidence for declared FKs
                    });
                }
            }

            // Reverse foreign keys (tables referencing this one)
            for fk in &rel_meta.referenced_by {
                for (source_col, target_col) in fk.columns.iter().zip(&fk.referenced_columns) {
                    relationships.push(RelationshipInfo {
                        source: fk.referenced_table.clone(),
                        source_field: target_col.clone(),
                        target: table_meta.name.clone(),
                        target_field: source_col.clone(),
                        relationship_type: determine_relationship_type(fk),
                        confidence: 1.0,
                    });
                }
            }
        }

        relationships
    }
}

/// Convert inference data type string to UniversalDataType
fn convert_inference_data_type(pg_type: &str) -> UniversalDataType {
    let pg_type_lower = pg_type.to_lowercase();

    // Handle common PostgreSQL types - check bigint/int8 first before generic int
    if pg_type_lower == "bigint" || pg_type_lower == "int8" || pg_type_lower == "bigserial" {
        UniversalDataType::Integer { bits: Some(64) }
    } else if pg_type_lower == "smallint" || pg_type_lower == "int2" {
        UniversalDataType::Integer { bits: Some(16) }
    } else if pg_type_lower.starts_with("int") || pg_type_lower == "serial" {
        UniversalDataType::Integer { bits: Some(32) }
    } else if pg_type_lower.starts_with("float")
        || pg_type_lower.starts_with("double")
        || pg_type_lower == "real"
    {
        if pg_type_lower.contains("4") || pg_type_lower == "real" {
            UniversalDataType::Float { bits: Some(32) }
        } else {
            UniversalDataType::Float { bits: Some(64) }
        }
    } else if pg_type_lower.starts_with("numeric") || pg_type_lower.starts_with("decimal") {
        // Parse numeric(precision, scale)
        UniversalDataType::Decimal {
            precision: 18, // Default precision
            scale: 2,      // Default scale
        }
    } else if pg_type_lower.starts_with("varchar") || pg_type_lower.starts_with("character varying")
    {
        UniversalDataType::String { max_length: None }
    } else if pg_type_lower.starts_with("char") && !pg_type_lower.contains("var") {
        UniversalDataType::Char { length: 1 }
    } else if pg_type_lower == "text" {
        UniversalDataType::Text
    } else if pg_type_lower == "date" {
        UniversalDataType::Date
    } else if pg_type_lower.starts_with("time") && !pg_type_lower.contains("stamp") {
        UniversalDataType::Time {
            with_timezone: pg_type_lower.contains("zone"),
        }
    } else if pg_type_lower.starts_with("timestamp") {
        UniversalDataType::DateTime {
            with_timezone: pg_type_lower.contains("zone"),
        }
    } else if pg_type_lower == "interval" {
        UniversalDataType::Interval
    } else if pg_type_lower == "boolean" || pg_type_lower == "bool" {
        UniversalDataType::Boolean
    } else if pg_type_lower == "bytea" {
        UniversalDataType::Binary { max_length: None }
    } else if pg_type_lower == "json" || pg_type_lower == "jsonb" {
        UniversalDataType::Json
    } else if pg_type_lower == "xml" {
        UniversalDataType::Xml
    } else if pg_type_lower == "uuid" {
        UniversalDataType::Uuid
    } else if pg_type_lower.ends_with("[]") {
        // Array type - extract element type
        let element_type = &pg_type_lower[..pg_type_lower.len() - 2];
        UniversalDataType::Array {
            element_type: Box::new(convert_inference_data_type(element_type)),
        }
    } else {
        UniversalDataType::Unknown
    }
}

/// Convert inference::types::ColumnStatistics to schema::profile::FieldProfile
fn convert_column_statistics(col_stats: &ColumnStatistics) -> FieldProfile {
    use crate::schema::profile::*;

    // Convert histogram
    let histogram_converted = col_stats.histogram.as_ref().map(|hist| {
        hist.buckets
            .iter()
            .map(|bucket| ValueFrequency {
                value: format!("{} - {}", bucket.lower_bound, bucket.upper_bound),
                count: bucket.frequency,
                percentage: 0.0, // Would need total to calculate
            })
            .collect::<Vec<_>>()
    });

    FieldProfile {
        distinct_count: col_stats.distinct_count.unwrap_or(0),
        total_rows: col_stats.sample_size.unwrap_or(0),
        null_count: col_stats.null_count,
        null_percentage: col_stats.null_percentage / 100.0, // Convert from percentage to ratio
        distribution: crate::schema::profile::ValueDistribution {
            min: col_stats.min_value.clone(),
            max: col_stats.max_value.clone(),
            mean: None,
            median: None,
            mode: None,
            stddev: None,
            variance: None,
            p01: None,
            p05: None,
            p25: None,
            p50: None,
            p75: None,
            p95: None,
            p99: None,
            sum: None,
            skewness: None,
            kurtosis: None,
        },
        samples: col_stats
            .most_common_values
            .as_ref()
            .map(|mcv| mcv.iter().take(10).map(|vf| vf.value.clone()).collect())
            .unwrap_or_default(),
        top_values: col_stats.most_common_values.as_ref().map(|mcv| {
            mcv.iter()
                .map(|vf| ValueFrequency {
                    value: vf.value.clone(),
                    count: vf.count,
                    percentage: vf.percentage,
                })
                .collect()
        }),
        patterns: None, // PostgreSQL doesn't provide pattern analysis
        quality: DataQualityMetrics {
            completeness: 1.0 - (col_stats.null_percentage / 100.0),
            uniqueness: col_stats
                .distinct_count
                .and_then(|dc| {
                    col_stats.sample_size.map(|total| {
                        if total > 0 {
                            dc as f64 / total as f64
                        } else {
                            0.0
                        }
                    })
                })
                .unwrap_or(0.0),
            validity: 1.0, // Would need constraint validation
            consistency: col_stats.correlation.map(|c| c.abs()).unwrap_or(1.0),
            overall_score: 1.0 - (col_stats.null_percentage / 100.0),
            issues: Vec::new(),
        },
    }
}

/// Convert inference::types::SemanticType to schema::field::SemanticType
fn convert_inference_semantic_type(
    semantic: &crate::inference::types::SemanticType,
) -> crate::schema::field::SemanticType {
    use crate::inference::types::SemanticType as InfSemantic;
    use crate::schema::field::SemanticType;

    match semantic {
        InfSemantic::Email => SemanticType::Email,
        InfSemantic::PhoneNumber => SemanticType::PhoneNumber,
        InfSemantic::PersonName => SemanticType::FullName,
        InfSemantic::OrganizationName => SemanticType::CompanyName,
        InfSemantic::Address => SemanticType::FullAddress,
        InfSemantic::City => SemanticType::City,
        InfSemantic::State => SemanticType::State,
        InfSemantic::PostalCode => SemanticType::PostalCode,
        InfSemantic::Country => SemanticType::Country,
        InfSemantic::CountryCode => SemanticType::Country,
        InfSemantic::Coordinates => SemanticType::GeoPoint,
        InfSemantic::IPAddress => SemanticType::IPAddress,
        InfSemantic::CreditCardNumber => SemanticType::CreditCardNumber,
        InfSemantic::BankAccountNumber => SemanticType::BankAccountNumber,
        InfSemantic::IBANNumber => SemanticType::BankAccountNumber,
        InfSemantic::CurrencyAmount => SemanticType::Currency,
        InfSemantic::CurrencyCode => SemanticType::Currency,
        InfSemantic::TaxIdentifier => SemanticType::BankAccountNumber, // Close mapping
        InfSemantic::SSN => SemanticType::SocialSecurityNumber,
        InfSemantic::MedicalRecordNumber => SemanticType::Custom("MedicalRecordNumber".to_string()),
        InfSemantic::HealthInsuranceNumber => {
            SemanticType::Custom("HealthInsuranceNumber".to_string())
        }
        InfSemantic::DrugCode => SemanticType::Custom("DrugCode".to_string()),
        InfSemantic::DiagnosisCode => SemanticType::Custom("DiagnosisCode".to_string()),
        InfSemantic::Timestamp => SemanticType::Custom("Timestamp".to_string()),
        InfSemantic::Date => SemanticType::Custom("Date".to_string()),
        InfSemantic::Time => SemanticType::Custom("Time".to_string()),
        InfSemantic::Duration => SemanticType::Custom("Duration".to_string()),
        InfSemantic::DateOfBirth => SemanticType::BirthDate,
        InfSemantic::URL => SemanticType::Custom("URL".to_string()),
        InfSemantic::URI => SemanticType::Custom("URI".to_string()),
        InfSemantic::UUID => SemanticType::UUID,
        InfSemantic::Hostname => SemanticType::Custom("Hostname".to_string()),
        InfSemantic::MACAddress => SemanticType::MACAddress,
        InfSemantic::FilePath => SemanticType::Custom("FilePath".to_string()),
        InfSemantic::MimeType => SemanticType::Custom("MimeType".to_string()),
        InfSemantic::ProductCode => SemanticType::ProductCode,
        InfSemantic::SKU => SemanticType::SKU,
        InfSemantic::OrderNumber => SemanticType::OrderNumber,
        InfSemantic::InvoiceNumber => SemanticType::InvoiceNumber,
        InfSemantic::AccountNumber => SemanticType::BankAccountNumber,
        InfSemantic::VIN => SemanticType::Custom("VIN".to_string()),
        InfSemantic::Enum => SemanticType::Custom("Enum".to_string()),
        InfSemantic::Boolean => SemanticType::Custom("Boolean".to_string()),
        InfSemantic::Flag => SemanticType::Custom("Flag".to_string()),
        InfSemantic::Status => SemanticType::Custom("Status".to_string()),
        InfSemantic::Category => SemanticType::Custom("Category".to_string()),
        InfSemantic::FreeText => SemanticType::Custom("FreeText".to_string()),
        InfSemantic::Description => SemanticType::Custom("Description".to_string()),
        InfSemantic::Comment => SemanticType::Custom("Comment".to_string()),
        InfSemantic::JsonBlob => SemanticType::Custom("JsonBlob".to_string()),
        InfSemantic::XMLBlob => SemanticType::Custom("XMLBlob".to_string()),
        InfSemantic::Quantity => SemanticType::Custom("Quantity".to_string()),
        InfSemantic::Percentage => SemanticType::Percentage,
        InfSemantic::Score => SemanticType::Custom("Score".to_string()),
        InfSemantic::Rating => SemanticType::Custom("Rating".to_string()),
        InfSemantic::Username => SemanticType::Custom("Username".to_string()),
        InfSemantic::UserId => SemanticType::CustomerId, // Close mapping
        InfSemantic::Custom(s) => SemanticType::Custom(s.clone()),
        InfSemantic::Unknown => SemanticType::Custom("Unknown".to_string()),
    }
}

/// Convert inference::types::DataClassification to schema::field::SensitivityLevel
fn convert_data_classification(
    classification: &DataClassification,
) -> crate::schema::field::SensitivityLevel {
    use crate::schema::field::SensitivityLevel;

    match classification {
        DataClassification::Public => SensitivityLevel::Public,
        DataClassification::Internal => SensitivityLevel::Internal,
        DataClassification::Confidential => SensitivityLevel::Confidential,
        DataClassification::Restricted => SensitivityLevel::Restricted,
        DataClassification::HighlyRestricted => SensitivityLevel::TopSecret,
    }
}

/// Determine relationship type from foreign key metadata
fn determine_relationship_type(fk: &ForeignKeyMetadata) -> RelationshipType {
    // Simple heuristic: if FK columns are all primary keys, likely 1:1
    // Otherwise, many:1 (most common case)
    // Would need cardinality analysis for accurate determination
    RelationshipType::ForeignKey
}

impl DataProfiler for PostgresProfiler {
    fn profile_source(
        &self,
        _source_ref: &str,
        _config: ProfileConfig,
    ) -> Result<Vec<UnifiedSchema>> {
        // Would need actual database connection to list all tables
        // This is a placeholder - actual implementation should:
        // 1. Connect to database
        // 2. Query information_schema for all tables
        // 3. Profile each table
        anyhow::bail!("PostgresProfiler::profile_source requires actual database connection. Use profile_table with TableMetadata instead.");
    }

    fn profile_table(
        &self,
        source_ref: &str,
        table_name: &str,
        _config: ProfileConfig,
    ) -> Result<UnifiedSchema> {
        // This method expects pre-fetched TableMetadata
        // Actual usage would be:
        // 1. Connector fetches TableMetadata using inference layer
        // 2. Pass TableMetadata to convert_table_metadata()

        // For now, return error with helpful message
        anyhow::bail!(
            "PostgresProfiler::profile_table for {}.{} requires pre-fetched TableMetadata. \
             Use convert_table_metadata() with TableMetadata from the inference layer.",
            source_ref,
            table_name
        )
    }

    fn get_sample_data(
        &self,
        _source_ref: &str,
        _table_name: Option<&str>,
        _limit: usize,
    ) -> Result<Vec<SampleRow>> {
        // Would require actual database connection
        anyhow::bail!("get_sample_data requires actual database connection")
    }

    fn detect_relationships(&self, _source_ref: &str) -> Result<Vec<RelationshipInfo>> {
        // Would require querying information_schema.table_constraints
        anyhow::bail!("detect_relationships requires actual database connection")
    }

    fn validate_quality(
        &self,
        _source_ref: &str,
        _table_name: Option<&str>,
    ) -> Result<QualityReport> {
        // Would require actual data access
        anyhow::bail!("validate_quality requires actual database connection")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_postgres_profiler_creation() {
        let profiler = PostgresProfiler::new("conn_123".to_string());
        assert_eq!(profiler.connection_id, "conn_123");
    }

    #[test]
    fn test_convert_inference_data_type() {
        assert_eq!(
            convert_inference_data_type("integer"),
            UniversalDataType::Integer { bits: Some(32) }
        );
        assert_eq!(
            convert_inference_data_type("bigint"),
            UniversalDataType::Integer { bits: Some(64) }
        );
        assert_eq!(
            convert_inference_data_type("varchar"),
            UniversalDataType::String { max_length: None }
        );
        assert_eq!(convert_inference_data_type("text"), UniversalDataType::Text);
        assert_eq!(
            convert_inference_data_type("boolean"),
            UniversalDataType::Boolean
        );
        assert_eq!(
            convert_inference_data_type("timestamp with time zone"),
            UniversalDataType::DateTime {
                with_timezone: true
            }
        );
        assert_eq!(convert_inference_data_type("uuid"), UniversalDataType::Uuid);
        assert_eq!(convert_inference_data_type("json"), UniversalDataType::Json);
    }

    #[test]
    fn test_convert_table_metadata() {
        let profiler = PostgresProfiler::new("test_conn".to_string());

        let table_meta = TableMetadata {
            name: "customers".to_string(),
            schema: "public".to_string(),
            table_type: TableType::BaseTable,
            columns: vec![
                ColumnMetadata {
                    name: "id".to_string(),
                    data_type: "integer".to_string(),
                    native_type: "integer".to_string(),
                    nullable: false,
                    is_primary_key: true,
                    ordinal_position: 1,
                    default_value: None,
                    comment: Some("Customer ID".to_string()),
                    statistics: None,
                    semantic_type: None,
                    semantic_confidence: None,
                    classification: None,
                    pii_detected: None,
                    value_profile: None,
                },
                ColumnMetadata {
                    name: "email".to_string(),
                    data_type: "text".to_string(),
                    native_type: "text".to_string(),
                    nullable: true,
                    is_primary_key: false,
                    ordinal_position: 2,
                    default_value: None,
                    comment: None,
                    statistics: None,
                    semantic_type: Some(crate::inference::types::SemanticType::Email),
                    semantic_confidence: Some(0.95),
                    classification: Some(DataClassification::Confidential),
                    pii_detected: None,
                    value_profile: None,
                },
            ],
            estimated_rows: Some(1000),
            relationships: None,
            indexes: vec![],
            constraints: vec![],
            statistics: None,
            partitioning: None,
            governance: None,
            profiling: None,
        };

        let unified_schema = profiler.convert_table_metadata(table_meta, "test_conn");

        assert_eq!(unified_schema.name, "customers");
        assert_eq!(unified_schema.source_type, SourceType::PostgreSQL);
        assert_eq!(unified_schema.fields.len(), 2);
        assert_eq!(unified_schema.row_count, Some(1000));

        // Check first field (id)
        let id_field = &unified_schema.fields[0];
        assert_eq!(id_field.name, "id");
        assert_eq!(
            id_field.data_type,
            UniversalDataType::Integer { bits: Some(32) }
        );
        assert!(!id_field.nullable);
        assert!(id_field.constraints.primary_key);

        // Check second field (email)
        let email_field = &unified_schema.fields[1];
        assert_eq!(email_field.name, "email");
        assert_eq!(email_field.data_type, UniversalDataType::Text);
        assert!(email_field.nullable);
        assert_eq!(
            email_field.semantic.semantic_type,
            Some(crate::schema::field::SemanticType::Email)
        );
        assert_eq!(
            email_field.semantic.sensitivity,
            Some(crate::schema::field::SensitivityLevel::Confidential)
        );
    }

    #[test]
    fn test_extract_relationships() {
        let profiler = PostgresProfiler::new("test_conn".to_string());

        let table_meta = TableMetadata {
            name: "orders".to_string(),
            schema: "public".to_string(),
            table_type: TableType::BaseTable,
            columns: vec![],
            estimated_rows: None,
            relationships: Some(RelationshipMetadata {
                foreign_keys: vec![ForeignKeyMetadata {
                    constraint_name: "fk_customer".to_string(),
                    columns: vec!["customer_id".to_string()],
                    referenced_schema: "public".to_string(),
                    referenced_table: "customers".to_string(),
                    referenced_columns: vec!["id".to_string()],
                    update_rule: crate::inference::types::ReferentialAction::Cascade,
                    delete_rule: crate::inference::types::ReferentialAction::Restrict,
                }],
                referenced_by: vec![],
            }),
            indexes: vec![],
            constraints: vec![],
            statistics: None,
            partitioning: None,
            governance: None,
            profiling: None,
        };

        let relationships = profiler.extract_relationships(&table_meta);

        assert_eq!(relationships.len(), 1);
        assert_eq!(relationships[0].source, "orders");
        assert_eq!(relationships[0].source_field, "customer_id");
        assert_eq!(relationships[0].target, "customers");
        assert_eq!(relationships[0].target_field, "id");
        assert_eq!(
            relationships[0].relationship_type,
            RelationshipType::ForeignKey
        );
        assert_eq!(relationships[0].confidence, 1.0);
    }
}
