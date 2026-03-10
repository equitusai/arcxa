// graphica-core/src/inference/mod.rs
//! Multi-tier schema inference framework for Graphica connectors.
//!
//! This module provides a progressive metadata discovery system that
//! adapts to different database capabilities and performance requirements.

pub mod csv_profiler; // NEW: CSV field profiling for mapping engine
pub mod db2_stats; // Phase 1: DB2 statistics extraction
pub mod detectors; // Legacy PII detector (will be deprecated)
pub mod mapping; // NEW: AI/ML field mapping and relationship discovery
pub mod orchestrator;
pub mod postgres_stats; // Phase 1: PostgreSQL statistics extraction
pub mod rdf_converter;
pub mod rdf_store; // Phase 2.1: RDF triple store for semantic metadata
pub mod semantic; // NEW: Sophisticated multi-strategy semantic type detection (Phase 1)
pub mod snowflake_stats;
pub mod traits;
pub mod types; // Phase 1: Snowflake statistics extraction with special features
               // TODO: Re-enable when sqlx dependency is added
               // pub mod postgres;
               // pub mod example;

pub use db2_stats::Db2StatsExtractor;
pub use orchestrator::SchemaInferenceOrchestrator;
pub use postgres_stats::PostgresStatsExtractor;
pub use snowflake_stats::SnowflakeStatsExtractor;
pub use traits::*;
pub use types::*;

// Re-export new detection system
pub use semantic::{ColumnNameDetector, DetectionContext, DetectionResult, DetectionStrategy};

// Re-export RDF store
pub use rdf_store::{RdfStore, RdfStoreConfig, RdfStoreStatistics};

// Re-export field mapping
pub use mapping::{
    DatasetSchema, FieldMapper, FieldMetadata, FieldProfile, FieldSimilarity, MappingSuggestions,
    RelationshipType, SimilarityScores,
};

// Re-export CSV profiler
pub use csv_profiler::{profile_csv_file, CsvFieldProfile, CsvProfilerConfig};
