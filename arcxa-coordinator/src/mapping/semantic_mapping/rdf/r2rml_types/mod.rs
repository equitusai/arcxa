//! R2RML Type Definitions (Phase 2 migration)
//!
//! W3C R2RML specification types, migrated from `r2rml` module as part of
//! the semantic mapping consolidation.

pub mod logical_table;
pub mod object_map;
pub mod predicate_object_map;
pub mod subject_map;
pub mod triples_map;

pub use logical_table::LogicalTable;
pub use object_map::{JoinCondition, ObjectMap};
pub use predicate_object_map::{PredicateObjectMap, PredicateSpec};
pub use subject_map::{SubjectMap, TermType};
pub use triples_map::{GraphMap, TriplesMap};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// R2RML Mapping Document
///
/// Top-level structure containing one or more triples maps.
///
/// Migrated from `r2rml::types` module as part of Phase 2 consolidation.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct R2rmlMapping {
    /// Mapping identifier (UUID or human-readable name)
    pub mapping_id: String,

    /// Base URI for all generated URIs
    pub base_uri: String,

    /// Human-readable description
    pub description: Option<String>,

    /// Source dataset this mapping applies to
    pub source_dataset: String,

    /// Target graph URI (where triples will be inserted)
    pub target_graph: Option<String>,

    /// Collection of triples maps
    pub triples_maps: Vec<TriplesMap>,

    /// Metadata
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub created_by: Option<String>,
}

impl R2rmlMapping {
    /// Create a new R2RML mapping
    pub fn new(mapping_id: String, base_uri: String, source_dataset: String) -> Self {
        let now = chrono::Utc::now();
        Self {
            mapping_id,
            base_uri,
            description: None,
            source_dataset,
            target_graph: None,
            triples_maps: vec![],
            created_at: now,
            updated_at: now,
            created_by: None,
        }
    }

    /// Add a triples map
    pub fn add_triples_map(&mut self, triples_map: TriplesMap) {
        self.triples_maps.push(triples_map);
        self.updated_at = chrono::Utc::now();
    }

    /// Validate the mapping structure
    pub fn validate(&self) -> Result<()> {
        if self.triples_maps.is_empty() {
            anyhow::bail!("Mapping must contain at least one TriplesMap");
        }

        for tm in &self.triples_maps {
            tm.validate()?;
        }

        Ok(())
    }

    /// Get URI for this mapping in RDF store
    pub fn get_mapping_uri(&self) -> String {
        format!("{}mapping/{}", self.base_uri, self.mapping_id)
    }
}
