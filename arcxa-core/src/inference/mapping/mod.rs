// graphica-core/src/inference/mapping/mod.rs
//! Field mapping and relationship discovery engine
//!
//! This module provides intelligent field mapping capabilities using multi-dimensional
//! similarity analysis. It helps automatically discover relationships between datasets
//! by analyzing:
//!
//! - **Lexical similarity**: String distance algorithms (Levenshtein, Jaro-Winkler, token overlap)
//! - **Statistical similarity**: Value distribution comparison (cardinality, range overlap)
//! - **Schema context**: Field position and neighboring fields
//! - **Semantic similarity** (Phase 2): NLP embeddings and domain vocabulary
//! - **Domain knowledge** (Phase 5): Entity ontology and common patterns
//! - **ML prediction** (Phase 4): Trained classifier for relationship types
//!
//! ## Example Usage
//!
//! ```rust
//! use graphica_core::inference::mapping::{FieldMapper, DatasetSchema, FieldMetadata};
//!
//! # fn example() -> anyhow::Result<()> {
//! // Create mapper with default configuration
//! let mapper = FieldMapper::new();
//!
//! // Define your dataset schemas
//! let customers_schema = DatasetSchema {
//!     dataset_id: "customers".to_string(),
//!     dataset_name: "Customers".to_string(),
//!     fields: vec![
//!         // ... field metadata
//!     ],
//! };
//!
//! let orders_schema = DatasetSchema {
//!     dataset_id: "orders".to_string(),
//!     dataset_name: "Orders".to_string(),
//!     fields: vec![
//!         // ... field metadata
//!     ],
//! };
//!
//! // Find mappings
//! let mappings = mapper.find_mappings(&customers_schema, &orders_schema)?;
//!
//! // Examine results
//! for mapping in mappings {
//!     println!("Source field: {}", mapping.source_field.column_name);
//!     for candidate in mapping.candidates {
//!         println!("  → {} (confidence: {:.2})",
//!             candidate.target.column_name,
//!             candidate.confidence
//!         );
//!     }
//! }
//! # Ok(())
//! # }
//! ```

pub mod lexical;
pub mod mapper;
pub mod statistical;
pub mod types;
pub mod vocabulary;

// Re-export main types
pub use types::{
    Cardinality, DataType, DatasetSchema, EvidenceType, FieldMapping, FieldMetadata, FieldProfile,
    FieldSimilarity, JoinDirection, MapperConfig, MappingEvidence, MappingSuggestions,
    RelationshipType, ScoreWeights, SimilarityScores, ValueDistribution,
};

pub use lexical::LexicalSimilarity;
pub use mapper::FieldMapper;
pub use statistical::{CardinalityEstimate, StatisticalSimilarity};
pub use vocabulary::DomainVocabulary;
