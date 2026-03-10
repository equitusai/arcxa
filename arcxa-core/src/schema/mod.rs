//! Unified Schema Module
//!
//! Provides a universal schema representation for all datasource types in Graphica.
//! This module defines the core types that unify file schemas, database table definitions,
//! and field metadata across the entire system.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Re-export for convenience
pub use embeddings::{
    cosine_similarity, EmbeddingConfig, EmbeddingGenerator, PretrainedEmbeddings,
};
pub use field::*;
pub use ontology_alignment::{
    AlignmentMethod, AlignmentResult, ConceptType, ModelServiceConfig, OntologyAligner,
    OntologyConcept,
};
pub use profile::*;
pub use profile::{IssueSeverity, QualityIssue};
pub use profile_cache::{CacheStats, ProfileCache, ProfileCacheConfig};
pub use profiler::*;
pub use relationship_detector::{RelationshipDetector, RelationshipDetectorConfig};
pub use semantic_detector::{DetectionMethod, SemanticDetectionResult, SemanticDetector};
pub use similarity_index::{CachedSimilarityIndex, IndexConfig, SimilarityMatch};
pub use source::*;
pub use types::*;

pub mod compat; // Public for external use of From trait implementations
mod conversion;
pub mod conversion_rules; // Phase 2: Type conversion rules engine
pub mod cross_source_mapper; // Phase 2: Cross-source field mapping
pub mod embeddings; // Lightweight text embeddings (TF-IDF + character n-grams)
mod field;
pub mod ontology_alignment; // ML-based ontology alignment
mod profile;
pub mod profile_cache; // Caching layer for profiled schemas
pub mod profiler; // Public profiler trait and types
pub mod profiler_csv; // CSV profiler implementation
pub mod profiler_db2; // IBM DB2 profiler implementation
pub mod profiler_postgres; // PostgreSQL profiler implementation
pub mod relationship_detector; // Cross-source relationship detection
pub mod semantic_detector; // Automatic semantic type detection
pub mod similarity_index;
mod source;
mod types; // Fast similarity search with brute-force and HNSW options

pub use conversion::{ConversionError, ConversionResult, TypeConverter};

// Re-export profilers
pub use profiler_csv::CsvProfiler;
pub use profiler_db2::DB2Profiler;
pub use profiler_postgres::PostgresProfiler;

// Re-export cross-source mapper
pub use cross_source_mapper::{CrossSourceMapper, CrossSourceMappingResult, TypeConversionInfo};

// Re-export conversion rules
pub use conversion_rules::{ConversionRule, ConversionRulesEngine, ConversionSafety, SqlDialect};

/// Unified schema definition for any datasource
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedSchema {
    /// Unique identifier for this schema
    pub id: String,

    /// Human-readable name
    pub name: String,

    /// Type of datasource
    pub source_type: SourceType,

    /// Reference to the source (file path, connection ID, etc.)
    pub source_ref: String,

    /// Fields/columns in this schema
    pub fields: Vec<UnifiedField>,

    /// Optional row count (if known)
    pub row_count: Option<u64>,

    /// Optional size in bytes (if applicable)
    pub size_bytes: Option<u64>,

    /// When this schema was last profiled
    pub last_profiled: Option<DateTime<Utc>>,

    /// Creation timestamp
    pub created_at: DateTime<Utc>,

    /// Last update timestamp
    pub updated_at: DateTime<Utc>,

    /// Additional metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

impl UnifiedSchema {
    /// Create a new unified schema
    pub fn new(name: String, source_type: SourceType, source_ref: String) -> Self {
        let id = format!("schema_{}", uuid::Uuid::new_v4());
        Self {
            id,
            name,
            source_type,
            source_ref,
            fields: Vec::new(),
            row_count: None,
            size_bytes: None,
            last_profiled: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            metadata: HashMap::new(),
        }
    }

    /// Add a field to the schema
    pub fn add_field(&mut self, field: UnifiedField) {
        self.fields.push(field);
        self.updated_at = Utc::now();
    }

    /// Find a field by name
    pub fn find_field(&self, name: &str) -> Option<&UnifiedField> {
        self.fields.iter().find(|f| f.name == name)
    }

    /// Get primary key fields
    pub fn primary_keys(&self) -> Vec<&UnifiedField> {
        self.fields
            .iter()
            .filter(|f| f.constraints.primary_key)
            .collect()
    }

    /// Check if schemas are compatible for mapping
    pub fn is_compatible_with(&self, other: &UnifiedSchema) -> bool {
        // Basic compatibility check - can be extended
        !self.fields.is_empty() && !other.fields.is_empty()
    }

    /// Generate a fingerprint for change detection
    pub fn fingerprint(&self) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();

        hasher.update(&self.name);
        hasher.update(&self.source_ref);

        for field in &self.fields {
            hasher.update(&field.name);
            hasher.update(field.data_type.to_string());
        }

        format!("{:x}", hasher.finalize())
    }
}

/// Result type for schema operations
pub type SchemaResult<T> = Result<T, SchemaError>;

/// Errors that can occur in schema operations
#[derive(Debug, thiserror::Error)]
pub enum SchemaError {
    #[error("Field not found: {0}")]
    FieldNotFound(String),

    #[error("Invalid schema: {0}")]
    InvalidSchema(String),

    #[error("Type conversion error: {0}")]
    TypeConversion(String),

    #[error("Profiling error: {0}")]
    ProfilingError(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Other error: {0}")]
    Other(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unified_schema_creation() {
        let mut schema = UnifiedSchema::new(
            "test_table".to_string(),
            SourceType::PostgreSQL,
            "postgres://localhost/test".to_string(),
        );

        assert_eq!(schema.name, "test_table");
        assert_eq!(schema.source_type, SourceType::PostgreSQL);
        assert!(schema.fields.is_empty());

        // Add a field
        let field = UnifiedField::new(
            "id".to_string(),
            UniversalDataType::Integer { bits: Some(64) },
        );
        schema.add_field(field);

        assert_eq!(schema.fields.len(), 1);
        assert!(schema.find_field("id").is_some());
    }

    #[test]
    fn test_schema_fingerprint() {
        let mut schema1 = UnifiedSchema::new(
            "users".to_string(),
            SourceType::CsvFile,
            "users.csv".to_string(),
        );

        schema1.add_field(UnifiedField::new(
            "id".to_string(),
            UniversalDataType::Integer { bits: Some(32) },
        ));

        let mut schema2 = schema1.clone();

        // Same schemas should have same fingerprint
        assert_eq!(schema1.fingerprint(), schema2.fingerprint());

        // Different field should change fingerprint
        schema2.add_field(UnifiedField::new(
            "name".to_string(),
            UniversalDataType::String {
                max_length: Some(255),
            },
        ));

        assert_ne!(schema1.fingerprint(), schema2.fingerprint());
    }
}
