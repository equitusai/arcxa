//! Cross-Source Field Mapping
//!
//! Enables field mapping between different datasource types using:
//! - Unified schema profiling
//! - Type conversion awareness
//! - Multi-dimensional similarity scoring

use anyhow::{Context, Result};
use std::collections::HashMap;

use crate::inference::mapping::{
    types::{DataType, DatasetSchema, FieldMetadata, FieldProfile, ValueDistribution},
    FieldMapper, MapperConfig, MappingSuggestions,
};
use crate::schema::{
    DataProfiler, ProfileConfig, SourceType, UnifiedField, UnifiedSchema, UniversalDataType,
};

/// Cross-source field mapper
///
/// Maps fields between different datasource types by:
/// 1. Profiling both sources using appropriate profilers
/// 2. Converting to common schema representation
/// 3. Using FieldMapper for similarity scoring
/// 4. Adding type conversion hints
pub struct CrossSourceMapper {
    /// Profiler registry: SourceType → DataProfiler
    profiler_registry: HashMap<SourceType, Box<dyn DataProfiler>>,

    /// Field mapper for similarity scoring
    field_mapper: FieldMapper,

    /// Mapper configuration
    config: MapperConfig,
}

impl CrossSourceMapper {
    /// Create a new cross-source mapper
    pub fn new() -> Self {
        Self::with_config(MapperConfig::default())
    }

    /// Create with custom configuration
    pub fn with_config(config: MapperConfig) -> Self {
        Self {
            profiler_registry: HashMap::new(),
            field_mapper: FieldMapper::with_config(config.clone()),
            config,
        }
    }

    /// Register a profiler for a specific source type
    pub fn register_profiler(&mut self, source_type: SourceType, profiler: Box<dyn DataProfiler>) {
        self.profiler_registry.insert(source_type, profiler);
    }

    /// Map fields between any two datasources using unified schemas
    ///
    /// This is the main entry point that accepts pre-profiled schemas.
    /// Use this when you already have UnifiedSchema objects.
    pub fn map_unified_schemas(
        &self,
        source_schema: &UnifiedSchema,
        target_schema: &UnifiedSchema,
    ) -> Result<CrossSourceMappingResult> {
        // Convert unified schemas to dataset schemas
        let source_dataset = self.unified_to_dataset(source_schema)?;
        let target_dataset = self.unified_to_dataset(target_schema)?;

        // Find mappings using field mapper
        let raw_mappings = self
            .field_mapper
            .find_mappings(&source_dataset, &target_dataset)
            .context("Failed to find field mappings")?;

        // Categorize into confidence tiers
        let mut suggestions = MappingSuggestions {
            joins: Vec::new(),
            auto_mapped: Vec::new(),
            recommended: Vec::new(),
            possible: Vec::new(),
        };

        // Add type conversion hints
        for mapping in raw_mappings {
            for candidate in &mapping.candidates {
                // Add conversion info if types differ
                let _conversion_info = if candidate.source.data_type != candidate.target.data_type {
                    Some(TypeConversionInfo {
                        source_type: self.universal_to_inference_type(
                            source_schema
                                .fields
                                .iter()
                                .find(|f| f.name == candidate.source.column_name)
                                .map(|f| &f.data_type),
                        ),
                        target_type: self.universal_to_inference_type(
                            target_schema
                                .fields
                                .iter()
                                .find(|f| f.name == candidate.target.column_name)
                                .map(|f| &f.data_type),
                        ),
                        is_lossy: self.is_lossy_conversion(
                            &candidate.source.data_type,
                            &candidate.target.data_type,
                        ),
                        conversion_function: self.suggest_conversion_function(
                            &candidate.source.data_type,
                            &candidate.target.data_type,
                        ),
                    })
                } else {
                    None
                };

                // Store candidates with conversion info
                suggestions.joins.push(candidate.clone());

                // Categorize by confidence
                if candidate.confidence >= self.config.auto_map_threshold {
                    suggestions.auto_mapped.push(candidate.clone());
                } else if candidate.confidence >= self.config.recommend_threshold {
                    suggestions.recommended.push(candidate.clone());
                } else if candidate.confidence >= self.config.min_confidence {
                    suggestions.possible.push(candidate.clone());
                }
            }
        }

        Ok(CrossSourceMappingResult {
            source_schema: source_schema.clone(),
            target_schema: target_schema.clone(),
            suggestions,
            type_conversions: Vec::new(), // TODO: Populate from conversion_info above
        })
    }

    /// Convert UnifiedSchema to DatasetSchema for mapping
    fn unified_to_dataset(&self, schema: &UnifiedSchema) -> Result<DatasetSchema> {
        let fields: Vec<FieldMetadata> = schema
            .fields
            .iter()
            .enumerate()
            .map(|(idx, field)| self.unified_field_to_metadata(field, &schema.source_ref, idx))
            .collect();

        Ok(DatasetSchema {
            dataset_id: schema.id.clone(),
            dataset_name: schema.name.clone(),
            fields,
        })
    }

    /// Convert UnifiedField to FieldMetadata
    fn unified_field_to_metadata(
        &self,
        field: &UnifiedField,
        source_ref: &str,
        position: usize,
    ) -> FieldMetadata {
        FieldMetadata {
            qualified_name: format!("{}.{}", source_ref, field.name),
            column_name: field.name.clone(),
            source_id: source_ref.to_string(),
            data_type: self.universal_to_inference_type(Some(&field.data_type)),
            profile: field
                .profile
                .as_ref()
                .map(|p| self.schema_profile_to_inference_profile(p))
                .unwrap_or_else(|| FieldProfile {
                    distinct_count: 0,
                    total_rows: 0,
                    null_percentage: 0.0,
                    distribution: ValueDistribution {
                        min: None,
                        max: None,
                        median: None,
                        p25: None,
                        p75: None,
                        p95: None,
                        p99: None,
                    },
                    samples: Vec::new(),
                }),
            semantic_type: field
                .semantic
                .semantic_type
                .as_ref()
                .map(|s| format!("{:?}", s)),
            position,
            neighbors: Vec::new(), // TODO: Extract from schema context
        }
    }

    /// Convert schema::FieldProfile to inference::mapping::FieldProfile
    fn schema_profile_to_inference_profile(
        &self,
        profile: &crate::schema::FieldProfile,
    ) -> FieldProfile {
        FieldProfile {
            distinct_count: profile.distinct_count,
            total_rows: profile.total_rows,
            null_percentage: profile.null_percentage,
            distribution: ValueDistribution {
                min: profile.distribution.min.clone(),
                max: profile.distribution.max.clone(),
                median: profile.distribution.median.clone(),
                p25: profile.distribution.p25.clone(),
                p75: profile.distribution.p75.clone(),
                p95: profile.distribution.p95.clone(),
                p99: profile.distribution.p99.clone(),
            },
            samples: profile.samples.clone(),
        }
    }

    /// Convert UniversalDataType to inference DataType
    fn universal_to_inference_type(&self, universal_type: Option<&UniversalDataType>) -> DataType {
        match universal_type {
            Some(UniversalDataType::Integer { .. }) => DataType::Integer,
            Some(UniversalDataType::Float { .. }) => DataType::Float,
            Some(UniversalDataType::Decimal { .. }) => DataType::Decimal {
                precision: 18,
                scale: 2,
            },
            Some(UniversalDataType::String { .. }) | Some(UniversalDataType::Text) => {
                DataType::String
            }
            Some(UniversalDataType::Char { .. }) => DataType::String,
            Some(UniversalDataType::Boolean) => DataType::Boolean,
            Some(UniversalDataType::Date) => DataType::Date,
            Some(UniversalDataType::DateTime { .. }) => DataType::DateTime,
            Some(UniversalDataType::Timestamp) => DataType::DateTime,
            Some(UniversalDataType::Time { .. }) => DataType::Time,
            Some(UniversalDataType::Binary { .. }) => DataType::Binary,
            Some(UniversalDataType::Json) | Some(UniversalDataType::Xml) => DataType::Json,
            Some(UniversalDataType::Uuid) => DataType::String,
            Some(UniversalDataType::Array { .. }) => DataType::Json,
            Some(UniversalDataType::Enum { .. }) => DataType::String,
            Some(UniversalDataType::Struct { .. }) => DataType::Json,
            Some(UniversalDataType::Interval) => DataType::String,
            Some(UniversalDataType::Unknown) | None => DataType::Unknown,
        }
    }

    /// Check if type conversion is lossy
    fn is_lossy_conversion(&self, source: &DataType, target: &DataType) -> bool {
        use DataType::*;

        match (source, target) {
            // Float to Integer is lossy
            (Float, Integer) => true,
            // Decimal to Integer is lossy
            (Decimal { .. }, Integer) => true,
            // DateTime to Date is lossy (loses time component)
            (DateTime, Date) => true,
            // DateTime to Time is lossy (loses date component)
            (DateTime, Time) => true,
            // Any to Unknown is lossy
            (_, Unknown) => true,
            // All other conversions are not lossy (for now)
            _ => false,
        }
    }

    /// Suggest conversion function for type mismatch
    fn suggest_conversion_function(&self, source: &DataType, target: &DataType) -> Option<String> {
        use DataType::*;

        match (source, target) {
            (Integer, String) => Some("CAST(x AS VARCHAR)".to_string()),
            (Float, String) => Some("CAST(x AS VARCHAR)".to_string()),
            (String, Integer) => Some("CAST(x AS INTEGER)".to_string()),
            (String, Float) => Some("CAST(x AS DOUBLE)".to_string()),
            (Date, DateTime) => Some("CAST(x AS TIMESTAMP)".to_string()),
            (DateTime, Date) => Some("CAST(x AS DATE)".to_string()),
            (Float, Integer) => Some("CAST(x AS INTEGER) -- WARNING: Lossy".to_string()),
            _ => None,
        }
    }
}

impl Default for CrossSourceMapper {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of cross-source mapping
#[derive(Debug, Clone)]
pub struct CrossSourceMappingResult {
    /// Source schema
    pub source_schema: UnifiedSchema,

    /// Target schema
    pub target_schema: UnifiedSchema,

    /// Mapping suggestions categorized by confidence
    pub suggestions: MappingSuggestions,

    /// Type conversions required
    pub type_conversions: Vec<TypeConversionInfo>,
}

/// Type conversion information
#[derive(Debug, Clone)]
pub struct TypeConversionInfo {
    /// Source data type
    pub source_type: DataType,

    /// Target data type
    pub target_type: DataType,

    /// Whether conversion is lossy
    pub is_lossy: bool,

    /// Suggested conversion function (e.g., "CAST(x AS VARCHAR)")
    pub conversion_function: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{SourceType, UniversalDataType};

    fn create_test_unified_schema(name: &str, source_type: SourceType) -> UnifiedSchema {
        let mut schema =
            UnifiedSchema::new(name.to_string(), source_type, format!("{}://test", name));

        // Add some test fields
        schema.add_field(UnifiedField::new(
            "id".to_string(),
            UniversalDataType::Integer { bits: Some(64) },
        ));

        schema.add_field(UnifiedField::new(
            "name".to_string(),
            UniversalDataType::String {
                max_length: Some(255),
            },
        ));

        schema.add_field(UnifiedField::new(
            "created_at".to_string(),
            UniversalDataType::DateTime {
                with_timezone: false,
            },
        ));

        schema
    }

    #[test]
    fn test_cross_source_mapper_creation() {
        let mapper = CrossSourceMapper::new();
        assert!(mapper.profiler_registry.is_empty());
    }

    #[test]
    fn test_unified_to_dataset_conversion() {
        let mapper = CrossSourceMapper::new();
        let schema = create_test_unified_schema("users", SourceType::PostgreSQL);

        let dataset = mapper.unified_to_dataset(&schema).unwrap();

        assert_eq!(dataset.dataset_name, "users");
        assert_eq!(dataset.fields.len(), 3);
        assert_eq!(dataset.fields[0].column_name, "id");
        assert_eq!(dataset.fields[1].column_name, "name");
        assert_eq!(dataset.fields[2].column_name, "created_at");
    }

    #[test]
    fn test_universal_to_inference_type_conversion() {
        let mapper = CrossSourceMapper::new();

        assert!(matches!(
            mapper
                .universal_to_inference_type(Some(&UniversalDataType::Integer { bits: Some(32) })),
            DataType::Integer
        ));

        assert!(matches!(
            mapper.universal_to_inference_type(Some(&UniversalDataType::String {
                max_length: Some(255)
            })),
            DataType::String
        ));

        assert!(matches!(
            mapper.universal_to_inference_type(Some(&UniversalDataType::DateTime {
                with_timezone: false
            })),
            DataType::DateTime
        ));

        assert!(matches!(
            mapper.universal_to_inference_type(None),
            DataType::Unknown
        ));
    }

    #[test]
    fn test_is_lossy_conversion() {
        let mapper = CrossSourceMapper::new();

        // Float to Integer is lossy
        assert!(mapper.is_lossy_conversion(&DataType::Float, &DataType::Integer));

        // DateTime to Date is lossy
        assert!(mapper.is_lossy_conversion(&DataType::DateTime, &DataType::Date));

        // Integer to String is not lossy
        assert!(!mapper.is_lossy_conversion(&DataType::Integer, &DataType::String));

        // Same type is not lossy
        assert!(!mapper.is_lossy_conversion(&DataType::Integer, &DataType::Integer));
    }

    #[test]
    fn test_suggest_conversion_function() {
        let mapper = CrossSourceMapper::new();

        // Integer to String
        let conv = mapper.suggest_conversion_function(&DataType::Integer, &DataType::String);
        assert!(conv.is_some());
        assert!(conv.unwrap().contains("VARCHAR"));

        // DateTime to Date
        let conv = mapper.suggest_conversion_function(&DataType::DateTime, &DataType::Date);
        assert!(conv.is_some());
        assert!(conv.unwrap().contains("DATE"));

        // Same type - no conversion needed
        let conv = mapper.suggest_conversion_function(&DataType::Integer, &DataType::Integer);
        assert!(conv.is_none());
    }

    #[test]
    fn test_map_unified_schemas() {
        let mapper = CrossSourceMapper::new();

        let source_schema = create_test_unified_schema("customers", SourceType::PostgreSQL);
        let target_schema = create_test_unified_schema("users", SourceType::CsvFile);

        let result = mapper
            .map_unified_schemas(&source_schema, &target_schema)
            .unwrap();

        // Should find mappings for all fields (id, name, created_at)
        assert!(!result.suggestions.joins.is_empty());
    }

    #[test]
    fn test_register_profiler() {
        let mut mapper = CrossSourceMapper::new();

        // Register CSV profiler
        use crate::schema::CsvProfiler;
        mapper.register_profiler(SourceType::CsvFile, Box::new(CsvProfiler::new()));

        assert_eq!(mapper.profiler_registry.len(), 1);
        assert!(mapper.profiler_registry.contains_key(&SourceType::CsvFile));
    }
}
