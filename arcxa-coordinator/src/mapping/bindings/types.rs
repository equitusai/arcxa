//! Domain types for ontology-to-physical bindings.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Lifecycle status for a binding version.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BindingStatus {
    Active,
    Deprecated,
    Stale,
}

/// Provenance metadata for auditing and governance.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BindingProvenance {
    pub workflow_id: Option<String>,
    pub session_id: Option<String>,
    pub approved_by: Option<String>,
    pub approval_reason: Option<String>,
    pub observed_schema_hash: Option<String>,
}

/// Persisted ontology-to-physical binding record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OntologyPhysicalBinding {
    pub id: String,
    pub source_id: String,
    pub entity_uri: String,
    pub ontology_uri: String,
    pub table: String,
    pub column: String,
    pub dialect: String,
    pub confidence: f64,
    pub status: BindingStatus,
    pub version: u32,
    pub binding_hash: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub created_by: String,
    pub updated_by: String,
    pub provenance: BindingProvenance,
}

impl OntologyPhysicalBinding {
    /// Deterministic lookup key for current binding resolution.
    pub fn lookup_key(source_id: &str, entity_uri: &str, ontology_uri: &str) -> String {
        format!("{}|{}|{}", source_id, entity_uri, ontology_uri)
    }

    /// Deterministic hash for semantic version bump detection.
    pub fn compute_hash(table: &str, column: &str, dialect: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(table.as_bytes());
        hasher.update(b"|");
        hasher.update(column.as_bytes());
        hasher.update(b"|");
        hasher.update(dialect.as_bytes());
        format!("{:x}", hasher.finalize())
    }
}

/// Upsert command for creating/updating a binding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertBindingRequest {
    pub source_id: String,
    pub entity_uri: String,
    pub ontology_uri: String,
    pub table: String,
    pub column: String,
    pub dialect: String,
    pub confidence: f64,
    pub updated_by: String,
    pub provenance: BindingProvenance,
}

/// Diff of ontology requirements against current binding availability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BindingCoverageDiff {
    pub required_properties: Vec<String>,
    pub covered_properties: Vec<String>,
    pub missing_properties: Vec<String>,
    pub stale_properties: Vec<String>,
    pub unmapped_properties: Vec<String>,
    pub coverage_ratio: f64,
}
