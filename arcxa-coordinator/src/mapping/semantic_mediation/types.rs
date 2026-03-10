//! Types for Semantic Mediation
//!
//! Core data structures for semantic-mediated vendor mappings.
//!
//! ## Design Decisions
//!
//! ### Why separate VendorOntology from SemanticOntology?
//!
//! - **Vendor ontologies** describe vendor-specific schemas (Oracle GL_JE_HEADERS table structure)
//! - **Semantic ontologies** describe universal business concepts (accounting:AccountingDocument)
//! - **Separation** allows versioning vendors independently of semantic layer
//!
//! ### Why use URIs for identifiers?
//!
//! - **Globally unique**: No collisions between vendors
//! - **Self-describing**: `http://graphica.io/ontology/oracle/ebs/r12.2#GL_JE_HEADERS` is clear
//! - **Linked data**: Can dereference to get schema documentation
//! - **RDF native**: Direct mapping to RDF triple subject/predicate/object
//!
//! ### Why confidence scores on mappings?
//!
//! - **Quality signal**: Pre-built mappings vary in accuracy (1.0 = exact, 0.8 = close approximation)
//! - **Conflict resolution**: When multiple mappings possible, choose highest confidence
//! - **User override**: UI can show low-confidence mappings for review
//! - **Audit trail**: Track which mappings are validated vs inferred

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Vendor ontology (describes vendor's schema)
///
/// Example: Oracle E-Business Suite R12.2 schema definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VendorOntology {
    /// Unique vendor identifier (e.g., "oracle_ebs_r12.2", "sap_s4hana_2023")
    pub vendor_id: String,

    /// Human-readable name
    pub display_name: String,

    /// Version string (e.g., "R12.2.11", "S/4HANA 2023 FPS02")
    pub version: String,

    /// Modules/schemas covered (e.g., ["GL", "AP", "AR"])
    pub modules: Vec<String>,

    /// Table count
    pub table_count: usize,

    /// Field/column count
    pub field_count: usize,

    /// Ontology content (Turtle or RDF/XML format)
    ///
    /// **Architecture decision**: Store as String instead of parsed RDF graph
    /// - **Pros**: Simpler serialization, easier to reload
    /// - **Cons**: Must parse on each access (mitigated by caching)
    /// - **Alternative**: Pre-parse to internal graph structure (more memory)
    pub ontology_content: String,

    /// Content hash (SHA256) for versioning
    pub content_hash: String,

    /// Created timestamp
    pub created_at: DateTime<Utc>,

    /// Last updated timestamp
    pub updated_at: DateTime<Utc>,
}

/// Vendor ontology metadata (lightweight version for listing)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VendorOntologyMetadata {
    pub vendor_id: String,
    pub display_name: String,
    pub version: String,
    pub modules: Vec<String>,
    pub table_count: usize,
    pub field_count: usize,
}

impl From<&VendorOntology> for VendorOntologyMetadata {
    fn from(ontology: &VendorOntology) -> Self {
        Self {
            vendor_id: ontology.vendor_id.clone(),
            display_name: ontology.display_name.clone(),
            version: ontology.version.clone(),
            modules: ontology.modules.clone(),
            table_count: ontology.table_count,
            field_count: ontology.field_count,
        }
    }
}

/// Vendor-to-semantic mapping
///
/// Maps a vendor table to a semantic concept
///
/// Example: Oracle GL_JE_HEADERS → accounting:AccountingDocument
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VendorToSemanticMapping {
    /// Unique mapping ID
    pub mapping_id: String,

    /// Source vendor identifier
    pub vendor_id: String,

    /// Source table name (vendor-specific)
    pub source_table: String,

    /// Target semantic concept URI (e.g., "http://graphica.io/ontology/accounting#AccountingDocument")
    pub semantic_concept: String,

    /// Mapping confidence (0.0 - 1.0)
    ///
    /// **Why f64 instead of u8 (0-100)?**
    /// - **Precision**: Can represent 0.85 vs 0.86 confidence
    /// - **Standard**: Common in ML/statistical systems
    /// - **Math**: Direct use in weighted scoring (no conversion)
    pub confidence: f64,

    /// Mapping type
    pub mapping_type: MappingType,

    /// Field-level mappings
    pub field_mappings: Vec<FieldMapping>,

    /// Notes/documentation
    pub notes: Option<String>,

    /// Created timestamp
    pub created_at: DateTime<Utc>,
}

/// Semantic-to-vendor mapping (reverse direction)
///
/// Maps a semantic concept to a vendor table
///
/// Example: accounting:AccountingDocument → SAP BKPF
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticToVendorMapping {
    /// Unique mapping ID
    pub mapping_id: String,

    /// Source semantic concept URI
    pub semantic_concept: String,

    /// Target vendor identifier
    pub vendor_id: String,

    /// Target table name (vendor-specific)
    pub target_table: String,

    /// Mapping confidence
    pub confidence: f64,

    /// Mapping type
    pub mapping_type: MappingType,

    /// Field-level mappings
    pub field_mappings: Vec<FieldMapping>,

    /// Notes/documentation
    pub notes: Option<String>,

    /// Created timestamp
    pub created_at: DateTime<Utc>,
}

/// Field-level mapping within table mapping
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldMapping {
    /// Source field name (vendor-specific or semantic property)
    pub source_field: String,

    /// Target field name (semantic property or vendor-specific)
    pub target_field: String,

    /// Mapping confidence
    pub confidence: f64,

    /// Optional transformation expression
    ///
    /// Examples:
    /// - `null` - Direct mapping, no transformation
    /// - `"COALESCE(ENTERED_DR, 0) - COALESCE(ENTERED_CR, 0)"` - Oracle DR/CR to signed amount
    /// - `"lookup(LEDGER_ID, 'oracle_ledger_to_company_code')"` - Requires lookup table
    pub transformation: Option<String>,

    /// Transformation type (for code generation)
    pub transformation_type: Option<TransformationType>,

    /// Notes explaining the mapping
    pub notes: Option<String>,
}

/// Type of table mapping
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MappingType {
    /// One source table maps to one semantic concept/target table
    OneToOne,

    /// One source table maps to multiple semantic concepts (e.g., header + lines)
    OneToMany,

    /// Multiple source tables map to one semantic concept (consolidation)
    ManyToOne,

    /// Complex mapping (custom logic required)
    Complex,
}

/// Type of field transformation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransformationType {
    /// No transformation (direct copy)
    Direct,

    /// SQL expression (evaluated in database)
    SqlExpression,

    /// Lookup table (requires join)
    Lookup,

    /// Value mapping (e.g., status codes)
    ValueMapping,

    /// Complex transformation (requires custom function)
    Custom,
}

/// Request to compose source→target mapping via semantic layer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposeMappingRequest {
    /// Source vendor ID
    pub source_vendor: String,

    /// Target vendor ID
    pub target_vendor: String,

    /// Optional module filter (e.g., ["GL", "AP"])
    pub modules: Vec<String>,

    /// Minimum confidence threshold (default: 0.7)
    pub min_confidence: Option<f64>,

    /// Whether to include unmapped fields (default: true)
    pub include_unmapped: Option<bool>,
}

/// Composed mapping result (source→semantic→target)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposedSemanticMapping {
    /// Unique session ID for this composition
    pub session_id: String,

    /// Source vendor
    pub source_vendor: String,

    /// Target vendor
    pub target_vendor: String,

    /// Modules covered
    pub modules: Vec<String>,

    /// Composed table mappings
    pub table_mappings: Vec<ComposedTableMapping>,

    /// Overall coverage statistics
    pub coverage_percent: f64,

    /// Semantic concepts covered
    pub semantic_concepts: Vec<String>,

    /// Created timestamp
    pub created_at: DateTime<Utc>,
}

/// Composed table mapping (source table → semantic concept → target table)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposedTableMapping {
    /// Source table name
    pub source_table: String,

    /// Semantic concept URI (mediator)
    pub semantic_concept: String,

    /// Target table name
    pub target_table: String,

    /// Overall confidence (min of source→semantic and semantic→target)
    ///
    /// **Architecture decision**: Use minimum instead of average
    /// - **Rationale**: Weakest link determines mapping quality
    /// - **Example**: Source→Semantic (0.95) × Semantic→Target (0.60) = 0.60 overall
    /// - **Alternative**: Average would give 0.775, overestimating quality
    pub confidence: f64,

    /// Field mappings composed through semantic layer
    pub field_mappings: Vec<ComposedFieldMapping>,

    /// Coverage: mapped fields / total fields
    pub coverage_percent: f64,

    /// Unmapped source fields (no semantic property match)
    pub unmapped_source_fields: Vec<String>,

    /// Unmapped target fields (no semantic property available)
    pub unmapped_target_fields: Vec<String>,
}

/// Composed field mapping (source field → semantic property → target field)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposedFieldMapping {
    /// Source field name (vendor-specific)
    pub source_field: String,

    /// Semantic property URI (mediator)
    pub semantic_property: String,

    /// Target field name (vendor-specific)
    pub target_field: String,

    /// Overall confidence (minimum of source→semantic and semantic→target)
    pub confidence: f64,

    /// Composed transformation (may chain source and target transformations)
    ///
    /// **Example**:
    /// ```
    /// Source→Semantic: COALESCE(ENTERED_DR, 0) - COALESCE(ENTERED_CR, 0)
    /// Semantic→Target: (no transformation, direct copy)
    /// Composed: COALESCE(ENTERED_DR, 0) - COALESCE(ENTERED_CR, 0)
    /// ```
    pub transformation: Option<String>,

    /// Transformation type
    pub transformation_type: Option<TransformationType>,

    /// Lineage: How this mapping was derived
    pub lineage: FieldMappingLineage,
}

/// Lineage for composed field mapping
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldMappingLineage {
    /// Source→semantic mapping ID
    pub source_to_semantic_id: String,

    /// Semantic→target mapping ID
    pub semantic_to_target_id: String,

    /// Source transformation applied
    pub source_transformation: Option<String>,

    /// Target transformation applied
    pub target_transformation: Option<String>,
}

/// Semantic coverage report for a vendor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticCoverageReport {
    /// Vendor ID
    pub vendor_id: String,

    /// Semantic concepts covered
    pub covered_concepts: Vec<SemanticConceptCoverage>,

    /// Overall coverage percentage
    pub overall_coverage_percent: f64,

    /// Total tables analyzed
    pub total_tables: usize,

    /// Mapped tables
    pub mapped_tables: usize,
}

/// Coverage for a single semantic concept
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticConceptCoverage {
    /// Semantic concept URI
    pub concept_uri: String,

    /// Concept label (human-readable)
    pub concept_label: String,

    /// Vendor tables mapping to this concept
    pub vendor_tables: Vec<String>,

    /// Properties coverage
    pub property_coverage: HashMap<String, PropertyCoverage>,

    /// Overall property coverage percentage
    pub coverage_percent: f64,
}

/// Coverage for a single semantic property
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertyCoverage {
    /// Property URI
    pub property_uri: String,

    /// Property label
    pub property_label: String,

    /// Is this property mapped?
    pub is_mapped: bool,

    /// Vendor field(s) mapping to this property
    pub vendor_fields: Vec<String>,

    /// Confidence of mapping
    pub confidence: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mapping_type_serialization() {
        let mapping_type = MappingType::OneToOne;
        let json = serde_json::to_string(&mapping_type).unwrap();
        assert_eq!(json, "\"one_to_one\"");

        let deserialized: MappingType = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, MappingType::OneToOne);
    }

    #[test]
    fn test_transformation_type_serialization() {
        let transform = TransformationType::SqlExpression;
        let json = serde_json::to_string(&transform).unwrap();
        assert_eq!(json, "\"sql_expression\"");
    }

    #[test]
    fn test_confidence_precision() {
        let mapping = FieldMapping {
            source_field: "STATUS".to_string(),
            target_field: "documentStatus".to_string(),
            confidence: 0.856789,
            transformation: None,
            transformation_type: None,
            notes: None,
        };

        // Ensure we can represent fine-grained confidence values
        assert!((mapping.confidence - 0.856789).abs() < 0.000001);
    }

    #[test]
    fn test_vendor_ontology_metadata_conversion() {
        let ontology = VendorOntology {
            vendor_id: "oracle_ebs_r12.2".to_string(),
            display_name: "Oracle E-Business Suite R12.2".to_string(),
            version: "R12.2.11".to_string(),
            modules: vec!["GL".to_string(), "AP".to_string()],
            table_count: 187,
            field_count: 5432,
            ontology_content: String::new(),
            content_hash: "abc123".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let metadata = VendorOntologyMetadata::from(&ontology);
        assert_eq!(metadata.vendor_id, "oracle_ebs_r12.2");
        assert_eq!(metadata.table_count, 187);
    }
}
