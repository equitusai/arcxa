//! IBM DB2 Database Profiler
//!
//! Implements DataProfiler trait for DB2 databases using existing inference logic

use super::profiler::*;
use super::semantic_detector::SemanticDetector;
use super::*;
use crate::inference::types::ReferentialAction;
use crate::inference::types::*;
use anyhow::Result;

/// DB2-specific profiler implementation
///
/// Note: This profiler converts pre-fetched TableMetadata to UnifiedSchema.
/// Actual database querying should be done by the connector layer, which can
/// use DB2 SYSCAT metadata queries to gather statistics.
pub struct DB2Profiler {
    /// Connection string or identifier (for reference only)
    pub connection_id: String,
    /// Semantic type detector
    pub semantic_detector: SemanticDetector,
}

impl DB2Profiler {
    /// Create a new DB2 profiler
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
            SourceType::DB2,
            connection_id.to_string(),
        );

        // Convert columns to unified fields
        for (position, column) in table_meta.columns.iter().enumerate() {
            let mut unified_field = UnifiedField::new(
                column.name.clone(),
                convert_db2_data_type(&column.native_type),
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

/// Convert DB2 data type string to UniversalDataType
///
/// DB2 has specific type names that differ from PostgreSQL:
/// - INTEGER, SMALLINT, BIGINT for integer types
/// - DECIMAL, NUMERIC for fixed-point types
/// - REAL, DOUBLE, FLOAT for floating-point types
/// - VARCHAR, CHAR, CLOB for character types
/// - DATE, TIME, TIMESTAMP for temporal types
/// - BLOB for binary data
fn convert_db2_data_type(db2_type: &str) -> UniversalDataType {
    let db2_type_upper = db2_type.to_uppercase();

    // Handle DB2-specific types
    if db2_type_upper == "BIGINT" {
        UniversalDataType::Integer { bits: Some(64) }
    } else if db2_type_upper == "INTEGER" || db2_type_upper == "INT" {
        UniversalDataType::Integer { bits: Some(32) }
    } else if db2_type_upper == "SMALLINT" {
        UniversalDataType::Integer { bits: Some(16) }
    } else if db2_type_upper == "REAL" {
        UniversalDataType::Float { bits: Some(32) }
    } else if db2_type_upper.starts_with("DOUBLE") || db2_type_upper.starts_with("FLOAT") {
        UniversalDataType::Float { bits: Some(64) }
    } else if db2_type_upper.starts_with("DECIMAL") || db2_type_upper.starts_with("NUMERIC") {
        // Parse DECIMAL(p, s) or NUMERIC(p, s)
        UniversalDataType::Decimal {
            precision: 18, // DB2 default precision
            scale: 2,      // DB2 default scale
        }
    } else if db2_type_upper.starts_with("VARCHAR")
        || db2_type_upper.starts_with("CHARACTER VARYING")
    {
        UniversalDataType::String { max_length: None }
    } else if db2_type_upper.starts_with("CHAR") && !db2_type_upper.contains("VAR") {
        UniversalDataType::Char { length: 1 }
    } else if db2_type_upper == "CLOB" || db2_type_upper == "DBCLOB" {
        UniversalDataType::Text
    } else if db2_type_upper == "DATE" {
        UniversalDataType::Date
    } else if db2_type_upper.starts_with("TIME") && !db2_type_upper.contains("STAMP") {
        UniversalDataType::Time {
            with_timezone: false, // DB2 TIME doesn't have timezone
        }
    } else if db2_type_upper.starts_with("TIMESTAMP") {
        UniversalDataType::DateTime {
            with_timezone: db2_type_upper.contains("ZONE"),
        }
    } else if db2_type_upper == "BOOLEAN" {
        UniversalDataType::Boolean
    } else if db2_type_upper == "BLOB"
        || db2_type_upper == "BINARY"
        || db2_type_upper == "VARBINARY"
    {
        UniversalDataType::Binary { max_length: None }
    } else if db2_type_upper == "XML" {
        UniversalDataType::Xml
    } else if db2_type_upper == "GRAPHIC" || db2_type_upper == "VARGRAPHIC" {
        // DB2 graphic types for DBCS (double-byte character set)
        UniversalDataType::String { max_length: None }
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
        patterns: None, // DB2 doesn't provide pattern analysis
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

impl DataProfiler for DB2Profiler {
    fn profile_source(
        &self,
        _source_ref: &str,
        _config: ProfileConfig,
    ) -> Result<Vec<UnifiedSchema>> {
        // Would need actual database connection to list all tables
        // This is a placeholder - actual implementation should:
        // 1. Connect to database
        // 2. Query SYSCAT.TABLES for all tables
        // 3. Profile each table
        anyhow::bail!("DB2Profiler::profile_source requires actual database connection. Use profile_table with TableMetadata instead.");
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
            "DB2Profiler::profile_table for {}.{} requires pre-fetched TableMetadata. \
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
        // Would require querying SYSCAT.REFERENCES
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
    fn test_db2_profiler_creation() {
        let profiler = DB2Profiler::new("db2_conn_123".to_string());
        assert_eq!(profiler.connection_id, "db2_conn_123");
    }

    #[test]
    fn test_convert_db2_data_type() {
        assert_eq!(
            convert_db2_data_type("INTEGER"),
            UniversalDataType::Integer { bits: Some(32) }
        );
        assert_eq!(
            convert_db2_data_type("BIGINT"),
            UniversalDataType::Integer { bits: Some(64) }
        );
        assert_eq!(
            convert_db2_data_type("SMALLINT"),
            UniversalDataType::Integer { bits: Some(16) }
        );
        assert_eq!(
            convert_db2_data_type("VARCHAR"),
            UniversalDataType::String { max_length: None }
        );
        assert_eq!(convert_db2_data_type("CLOB"), UniversalDataType::Text);
        assert_eq!(
            convert_db2_data_type("DOUBLE"),
            UniversalDataType::Float { bits: Some(64) }
        );
        assert_eq!(
            convert_db2_data_type("REAL"),
            UniversalDataType::Float { bits: Some(32) }
        );
        assert_eq!(convert_db2_data_type("DATE"), UniversalDataType::Date);
        assert_eq!(
            convert_db2_data_type("TIMESTAMP"),
            UniversalDataType::DateTime {
                with_timezone: false
            }
        );
        assert_eq!(
            convert_db2_data_type("BLOB"),
            UniversalDataType::Binary { max_length: None }
        );
        assert_eq!(convert_db2_data_type("XML"), UniversalDataType::Xml);
        assert_eq!(convert_db2_data_type("BOOLEAN"), UniversalDataType::Boolean);
    }

    #[test]
    fn test_convert_table_metadata() {
        let profiler = DB2Profiler::new("test_conn".to_string());

        let table_meta = TableMetadata {
            name: "EMPLOYEE".to_string(),
            schema: "DB2INST1".to_string(),
            table_type: TableType::BaseTable,
            columns: vec![
                ColumnMetadata {
                    name: "EMPNO".to_string(),
                    data_type: "INTEGER".to_string(),
                    native_type: "INTEGER".to_string(),
                    nullable: false,
                    is_primary_key: true,
                    ordinal_position: 1,
                    default_value: None,
                    comment: Some("Employee Number".to_string()),
                    statistics: None,
                    semantic_type: None,
                    semantic_confidence: None,
                    classification: None,
                    pii_detected: None,
                    value_profile: None,
                },
                ColumnMetadata {
                    name: "FIRSTNME".to_string(),
                    data_type: "VARCHAR".to_string(),
                    native_type: "VARCHAR".to_string(),
                    nullable: false,
                    is_primary_key: false,
                    ordinal_position: 2,
                    default_value: None,
                    comment: None,
                    statistics: None,
                    semantic_type: Some(crate::inference::types::SemanticType::PersonName),
                    semantic_confidence: Some(0.95),
                    classification: Some(DataClassification::Internal),
                    pii_detected: None,
                    value_profile: None,
                },
            ],
            estimated_rows: Some(2500),
            relationships: None,
            indexes: vec![],
            constraints: vec![],
            statistics: None,
            partitioning: None,
            governance: None,
            profiling: None,
        };

        let unified_schema = profiler.convert_table_metadata(table_meta, "test_conn");

        assert_eq!(unified_schema.name, "EMPLOYEE");
        assert_eq!(unified_schema.source_type, SourceType::DB2);
        assert_eq!(unified_schema.fields.len(), 2);
        assert_eq!(unified_schema.row_count, Some(2500));

        // Check first field (EMPNO)
        let empno_field = &unified_schema.fields[0];
        assert_eq!(empno_field.name, "EMPNO");
        assert_eq!(
            empno_field.data_type,
            UniversalDataType::Integer { bits: Some(32) }
        );
        assert!(!empno_field.nullable);
        assert!(empno_field.constraints.primary_key);

        // Check second field (FIRSTNME)
        let name_field = &unified_schema.fields[1];
        assert_eq!(name_field.name, "FIRSTNME");
        assert_eq!(
            name_field.data_type,
            UniversalDataType::String { max_length: None }
        );
        assert!(!name_field.nullable);
        assert_eq!(
            name_field.semantic.semantic_type,
            Some(crate::schema::field::SemanticType::FullName)
        );
        assert_eq!(
            name_field.semantic.sensitivity,
            Some(crate::schema::field::SensitivityLevel::Internal)
        );
    }

    #[test]
    fn test_extract_relationships() {
        let profiler = DB2Profiler::new("test_conn".to_string());

        let table_meta = TableMetadata {
            name: "PROJECT".to_string(),
            schema: "DB2INST1".to_string(),
            table_type: TableType::BaseTable,
            columns: vec![],
            estimated_rows: None,
            relationships: Some(RelationshipMetadata {
                foreign_keys: vec![ForeignKeyMetadata {
                    constraint_name: "FK_RESPEMP".to_string(),
                    columns: vec!["RESPEMP".to_string()],
                    referenced_schema: "DB2INST1".to_string(),
                    referenced_table: "EMPLOYEE".to_string(),
                    referenced_columns: vec!["EMPNO".to_string()],
                    update_rule: ReferentialAction::Cascade,
                    delete_rule: ReferentialAction::Restrict,
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
        assert_eq!(relationships[0].source, "PROJECT");
        assert_eq!(relationships[0].source_field, "RESPEMP");
        assert_eq!(relationships[0].target, "EMPLOYEE");
        assert_eq!(relationships[0].target_field, "EMPNO");
        assert_eq!(
            relationships[0].relationship_type,
            RelationshipType::ForeignKey
        );
        assert_eq!(relationships[0].confidence, 1.0);
    }
}
