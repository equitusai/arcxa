//! Ontology API request/response types

use graphica_core::catalog::{OntologyMetadata, ValidationStatus};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Request to register a new custom ontology
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct RegisterOntologyRequest {
    /// Unique identifier for this ontology
    pub id: String,

    /// Human-readable name
    pub name: String,

    /// Description of the ontology
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Ontology content in Turtle format
    pub content: String,

    /// Namespace URI (will be auto-detected if not provided)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,

    /// Version string
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    /// Author/organization
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,

    /// Tags for categorization
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Response after registering an ontology
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RegisterOntologyResponse {
    /// Metadata of the registered ontology
    pub metadata: OntologyMetadata,

    /// Validation status
    pub validation: ValidationStatus,

    /// Message
    pub message: String,
}

/// Request to update an existing ontology
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct UpdateOntologyRequest {
    /// Updated ontology content
    pub content: String,

    /// Updated name (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// Updated version (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    /// Updated active status (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active: Option<bool>,

    /// Updated description (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Updated tags (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

/// Response for get ontology
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OntologyResponse {
    /// Ontology metadata
    pub metadata: OntologyMetadata,

    /// Ontology content (Turtle format)
    pub content: String,

    /// Validation status
    pub validation: ValidationStatus,

    /// Statistics
    pub stats: OntologyStats,
}

/// Ontology statistics
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OntologyStats {
    pub class_count: usize,
    pub property_count: usize,
    pub individual_count: usize,
    pub size_bytes: usize,
    pub usage_count: u64,
}

/// Response for listing ontologies
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ListOntologiesResponse {
    /// List of ontologies
    pub ontologies: Vec<OntologyMetadata>,

    /// Total count
    pub total: usize,

    /// Whether to include only active ontologies
    pub active_only: bool,
}

/// Request to get merged ontology
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct GetMergedOntologyRequest {
    /// Specific ontology IDs to include (if empty, includes all active)
    #[serde(default)]
    pub ontology_ids: Vec<String>,

    /// Include base catalog ontology
    #[serde(default = "default_true")]
    pub include_base: bool,

    /// Include extended inference ontology
    #[serde(default = "default_true")]
    pub include_extensions: bool,
}

fn default_true() -> bool {
    true
}

/// Response for merged ontology
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MergedOntologyResponse {
    /// Merged ontology content (Turtle format)
    pub content: String,

    /// List of ontology IDs included in the merge
    pub included_ontologies: Vec<String>,

    /// Total size in bytes
    pub size_bytes: usize,
}

/// Request to validate ontology syntax
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct ValidateOntologyRequest {
    /// Ontology content to validate
    pub content: String,
}

/// Response for ontology validation
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ValidateOntologyResponse {
    /// Validation status
    pub status: ValidationStatus,

    /// Additional validation messages
    pub messages: Vec<String>,
}

/// Error response for ontology operations
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OntologyErrorResponse {
    /// Error code
    pub error: String,

    /// Human-readable error message
    pub message: String,

    /// Additional error details
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

// =============================================================================
// Tree Structure Types for Hierarchical Display
// =============================================================================

/// Request to get ontology as a tree structure
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct GetOntologyTreeRequest {
    /// Maximum depth to traverse (-1 for unlimited)
    #[serde(default = "default_max_depth")]
    pub max_depth: i32,

    /// Include property details
    #[serde(default = "default_true")]
    pub include_properties: bool,

    /// Include individual instances
    #[serde(default)]
    pub include_individuals: bool,
}

fn default_max_depth() -> i32 {
    -1 // Unlimited
}

/// Response with ontology tree structure
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OntologyTreeResponse {
    /// Ontology namespace
    pub namespace: String,

    /// Ontology metadata
    pub metadata: OntologyMetadata,

    /// Root classes (classes with no parent or owl:Thing as parent)
    pub root_classes: Vec<ClassNode>,

    /// Root properties (properties not subPropertyOf another)
    pub root_properties: Vec<PropertyNode>,

    /// Total counts
    pub stats: TreeStats,
}

/// Statistics about the tree structure
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TreeStats {
    pub total_classes: usize,
    pub total_properties: usize,
    pub total_individuals: usize,
    pub max_depth: usize,
}

/// A class node in the ontology tree
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ClassNode {
    /// URI of the class
    pub uri: String,

    /// Human-readable label
    pub label: String,

    /// Description/comment
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,

    /// Parent class URIs (rdfs:subClassOf)
    pub parent_classes: Vec<String>,

    /// Child classes (classes that have this as rdfs:subClassOf)
    pub subclasses: Vec<ClassNode>,

    /// Properties with this class as domain
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<Vec<PropertyNode>>,

    /// Example individuals of this class
    #[serde(skip_serializing_if = "Option::is_none")]
    pub individuals: Option<Vec<IndividualNode>>,

    /// Depth in hierarchy (0 = root)
    pub depth: usize,

    /// Whether this is deprecated
    #[serde(default)]
    pub deprecated: bool,
}

/// A property node in the ontology tree
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PropertyNode {
    /// URI of the property
    pub uri: String,

    /// Human-readable label
    pub label: String,

    /// Description/comment
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,

    /// Property type (ObjectProperty, DatatypeProperty, etc.)
    pub property_type: PropertyType,

    /// Domain classes (classes this property applies to)
    pub domain: Vec<String>,

    /// Range (target type for this property)
    pub range: Vec<String>,

    /// Parent property URIs (rdfs:subPropertyOf)
    pub parent_properties: Vec<String>,

    /// Sub-properties
    pub subproperties: Vec<PropertyNode>,

    /// Whether this is deprecated
    #[serde(default)]
    pub deprecated: bool,
}

/// Type of RDF property
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PropertyType {
    /// owl:ObjectProperty (relates two resources)
    ObjectProperty,

    /// owl:DatatypeProperty (relates resource to literal)
    DatatypeProperty,

    /// owl:AnnotationProperty (metadata property)
    AnnotationProperty,

    /// rdf:Property (generic property)
    RdfProperty,
}

/// An individual instance node
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct IndividualNode {
    /// URI of the individual
    pub uri: String,

    /// Human-readable label
    pub label: String,

    /// Description/comment
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,

    /// Classes this individual belongs to (rdf:type)
    pub types: Vec<String>,
}
