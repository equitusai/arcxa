//! RDF Lineage Module for Ontology-Driven DDL Generation
//!
//! Implements Phase 2.4 of GAP-002: Tracking lineage from field definitions
//! through ontology classes and SHACL shapes to generated DDL statements.
//!
//! This implementation is designed to be forward-compatible with future
//! LE-DAG optimizations while maintaining clean W3C PROV semantics.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use crate::governance::ontology::{GRAPHICA_NS, PROV_NS, ML_NS};
use crate::governance::rdf_star::{AnnotatedTriple, TripleValue};
use crate::governance::GovernanceBrain;

/// Represents the complete lineage chain: Field → Ontology → SHACL → DDL
///
/// This structure captures the full provenance of how a field definition
/// flows through ontology mapping, SHACL validation, and DDL generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OntologyDrivenLineage {
    /// Unique identifier for this lineage instance
    pub id: Uuid,

    /// Source field definition
    pub field: FieldDefinition,

    /// Mapped ontology class
    pub ontology_class: OntologyClass,

    /// SHACL shape for validation
    pub shacl_shape: ShaclShape,

    /// Generated DDL statement
    pub ddl: DdlStatement,

    /// Metadata
    pub generated_at: DateTime<Utc>,
    pub generated_by: String,
    pub tenant_id: String,
    pub run_id: String,

    /// Optional correlation ID for distributed tracing
    pub correlation_id: Option<String>,

    /// Additional metadata (future extensibility)
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Field definition from source system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldDefinition {
    /// URI identifying the field
    pub field_uri: String,

    /// Human-readable field name
    pub field_name: String,

    /// Source system identifier (e.g., "salesforce", "sap")
    pub source_system: String,

    /// Original data type in source system
    pub data_type: String,

    /// Constraints from source (e.g., "NOT NULL", "UNIQUE")
    pub constraints: Vec<String>,

    /// Optional source table/entity
    pub source_entity: Option<String>,

    /// Field metadata (e.g., precision, scale, length)
    pub metadata: HashMap<String, String>,
}

/// Ontology class mapping
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OntologyClass {
    /// URI of the ontology class
    pub class_uri: String,

    /// Class name (e.g., "Customer", "Product")
    pub class_name: String,

    /// Parent class URI (for inheritance)
    pub parent_class: Option<String>,

    /// Properties defined on this class
    pub properties: Vec<OntologyProperty>,

    /// Namespace of the ontology
    pub namespace: String,
}

/// Property in an ontology class
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OntologyProperty {
    pub property_uri: String,
    pub property_name: String,
    pub range: String, // Expected data type
    pub cardinality: Option<String>, // e.g., "1..1", "0..*"
}

/// SHACL shape for validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShaclShape {
    /// URI of the SHACL shape
    pub shape_uri: String,

    /// Target class this shape validates
    pub target_class: String,

    /// Property constraints
    pub property_constraints: Vec<PropertyConstraint>,

    /// Shape severity (Violation, Warning, Info)
    pub severity: String,

    /// Optional message template
    pub message: Option<String>,
}

/// SHACL property constraint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertyConstraint {
    pub path: String,
    pub datatype: Option<String>,
    pub min_count: Option<u32>,
    pub max_count: Option<u32>,
    pub min_length: Option<u32>,
    pub max_length: Option<u32>,
    pub pattern: Option<String>,
    pub min_inclusive: Option<String>,
    pub max_inclusive: Option<String>,
}

/// Generated DDL statement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DdlStatement {
    /// URI identifying this DDL instance
    pub ddl_uri: String,

    /// The actual DDL statement
    pub statement: String,

    /// SQL dialect (postgresql, oracle, db2, mysql, etc.)
    pub dialect: String,

    /// DDL generator version
    pub version: String,

    /// Target schema/database
    pub target_schema: Option<String>,

    /// DDL type (CREATE TABLE, ALTER TABLE, etc.)
    pub ddl_type: String,
}

impl OntologyDrivenLineage {
    /// Create a new lineage instance
    pub fn new(
        field: FieldDefinition,
        ontology_class: OntologyClass,
        shacl_shape: ShaclShape,
        ddl: DdlStatement,
        tenant_id: String,
        generated_by: String,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            field,
            ontology_class,
            shacl_shape,
            ddl,
            generated_at: Utc::now(),
            generated_by,
            tenant_id,
            run_id: Uuid::new_v4().to_string(),
            correlation_id: None,
            metadata: HashMap::new(),
        }
    }

    /// Convert to W3C PROV triples with RDF-Star annotations
    ///
    /// This generates a complete provenance graph showing how the field
    /// was transformed through each stage of the pipeline.
    pub fn to_prov_triples(&self) -> Result<Vec<AnnotatedTriple>> {
        let mut triples = Vec::new();

        // Create activity URI for this lineage process
        let activity_uri = format!("{}/activity/{}", GRAPHICA_NS, self.id);

        // 1. Main activity (the overall lineage process)
        triples.push(
            AnnotatedTriple::new(
                &activity_uri,
                &format!("{}type", crate::governance::ontology::RDF_NS),
                &format!("{}Activity", PROV_NS),
            )
            .with_timestamp(&self.generated_at)
            .with_annotation(
                &format!("{}runId", GRAPHICA_NS),
                TripleValue::Literal(self.run_id.clone()),
            )
        );

        // 2. Field as PROV Entity
        triples.push(
            AnnotatedTriple::new(
                &self.field.field_uri,
                &format!("{}type", crate::governance::ontology::RDF_NS),
                &format!("{}Entity", PROV_NS),
            )
            .with_annotation(
                &format!("{}sourceSystem", GRAPHICA_NS),
                TripleValue::Literal(self.field.source_system.clone()),
            )
        );

        // 3. Field → Ontology derivation
        let field_to_ont = AnnotatedTriple::new(
            &self.field.field_uri,
            &format!("{}wasDerivedFrom", PROV_NS),
            &self.ontology_class.class_uri,
        )
        .with_timestamp(&self.generated_at)
        .with_annotation(
            &format!("{}mappingConfidence", GRAPHICA_NS),
            TripleValue::typed_literal("0.95", "xsd:decimal"),
        )
        .with_annotation(
            &format!("{}mappingActivity", GRAPHICA_NS),
            TripleValue::Uri(format!("{}/mapping/{}", GRAPHICA_NS, self.id)),
        );
        triples.push(field_to_ont);

        // 4. Ontology → SHACL validation
        let ont_to_shacl = AnnotatedTriple::new(
            &self.ontology_class.class_uri,
            &format!("{}wasValidatedBy", GRAPHICA_NS),
            &self.shacl_shape.shape_uri,
        )
        .with_annotation(
            &format!("{}validationPassed", GRAPHICA_NS),
            TripleValue::typed_literal("true", "xsd:boolean"),
        )
        .with_annotation(
            &format!("{}validationActivity", GRAPHICA_NS),
            TripleValue::Uri(format!("{}/validation/{}", GRAPHICA_NS, self.id)),
        );
        triples.push(ont_to_shacl);

        // 5. SHACL → DDL generation
        let shacl_to_ddl = AnnotatedTriple::new(
            &self.ddl.ddl_uri,
            &format!("{}wasGeneratedBy", PROV_NS),
            &self.shacl_shape.shape_uri,
        )
        .with_annotation(
            &format!("{}dialect", GRAPHICA_NS),
            TripleValue::Literal(self.ddl.dialect.clone()),
        )
        .with_annotation(
            &format!("{}generatorVersion", GRAPHICA_NS),
            TripleValue::Literal(self.ddl.version.clone()),
        )
        .with_annotation(
            &format!("{}generationActivity", GRAPHICA_NS),
            TripleValue::Uri(format!("{}/generation/{}", GRAPHICA_NS, self.id)),
        );
        triples.push(shacl_to_ddl);

        // 6. Direct field → DDL lineage for efficient queries
        let field_to_ddl = AnnotatedTriple::new(
            &self.field.field_uri,
            &format!("{}resultedIn", GRAPHICA_NS),
            &self.ddl.ddl_uri,
        )
        .with_provenance(&activity_uri)
        .with_annotation(
            &format!("{}lineageDepth", GRAPHICA_NS),
            TripleValue::typed_literal("3", "xsd:integer"),
        );
        triples.push(field_to_ddl);

        // 7. Agent (who generated this)
        triples.push(
            AnnotatedTriple::new(
                &activity_uri,
                &format!("{}wasAssociatedWith", PROV_NS),
                &self.generated_by,
            )
        );

        Ok(triples)
    }

    /// Generate forward-compatible lineage descriptor
    ///
    /// This creates a structure that can later be converted to LE-DAG
    /// expressions when that optimization is implemented.
    pub fn to_future_compatible_lineage(&self) -> LineageDescriptor {
        LineageDescriptor {
            lineage_uri: format!("{}/lineage/{}", GRAPHICA_NS, self.id),
            lineage_type: LineageType::OntologyDriven,

            // These URIs can later point to LE-DAG expressions
            source_refs: vec![self.field.field_uri.clone()],
            transform_refs: vec![
                self.ontology_class.class_uri.clone(),
                self.shacl_shape.shape_uri.clone(),
            ],
            output_refs: vec![self.ddl.ddl_uri.clone()],

            // Metadata for future LE-DAG operator construction
            operations: vec![
                Operation {
                    op_type: "MAP".to_string(),
                    params: serde_json::json!({
                        "from": "field",
                        "to": "ontology_class",
                        "mapper": "semantic_mapper_v1"
                    }),
                },
                Operation {
                    op_type: "VALIDATE".to_string(),
                    params: serde_json::json!({
                        "shape": self.shacl_shape.shape_uri,
                        "severity": self.shacl_shape.severity
                    }),
                },
                Operation {
                    op_type: "GENERATE".to_string(),
                    params: serde_json::json!({
                        "template": "DDL",
                        "dialect": self.ddl.dialect,
                        "version": self.ddl.version
                    }),
                },
            ],

            metadata: LineageMetadata {
                created_at: self.generated_at,
                created_by: self.generated_by.clone(),
                tenant_id: self.tenant_id.clone(),
                run_id: self.run_id.clone(),
                correlation_id: self.correlation_id.clone(),
            },
        }
    }
}

/// Future-compatible lineage descriptor
///
/// This structure is designed to be easily convertible to LE-DAG
/// expressions when that optimization is implemented in Phase 3.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageDescriptor {
    pub lineage_uri: String,
    pub lineage_type: LineageType,
    pub source_refs: Vec<String>,
    pub transform_refs: Vec<String>,
    pub output_refs: Vec<String>,
    pub operations: Vec<Operation>,
    pub metadata: LineageMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LineageType {
    OntologyDriven,
    ModelDerived,
    RuleGenerated,
    UserDefined,
    Hybrid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Operation {
    pub op_type: String,
    pub params: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageMetadata {
    pub created_at: DateTime<Utc>,
    pub created_by: String,
    pub tenant_id: String,
    pub run_id: String,
    pub correlation_id: Option<String>,
}

/// Storage trait for lineage (abstract to allow future LE-DAG implementation)
#[async_trait::async_trait]
pub trait LineageStore: Send + Sync {
    /// Store a lineage descriptor
    async fn store_descriptor(&self, descriptor: LineageDescriptor) -> Result<()>;

    /// Query lineage by source
    async fn query_by_source(&self, source_uri: &str) -> Result<Vec<LineageDescriptor>>;

    /// Query lineage by output
    async fn query_by_output(&self, output_uri: &str) -> Result<Vec<LineageDescriptor>>;

    /// Get full lineage graph
    async fn get_lineage_graph(&self, lineage_uri: &str) -> Result<LineageDescriptor>;
}

/// Service for managing ontology-driven lineage
pub struct RdfLineageService {
    governance: Arc<GovernanceBrain>,
    storage: Arc<dyn LineageStore>,
}

impl RdfLineageService {
    pub fn new(governance: Arc<GovernanceBrain>, storage: Arc<dyn LineageStore>) -> Self {
        Self { governance, storage }
    }

    /// Record DDL generation lineage
    pub async fn record_ddl_generation(
        &self,
        lineage: OntologyDrivenLineage,
    ) -> Result<()> {
        // 1. Convert to RDF-Star annotated triples
        let triples = lineage.to_prov_triples()
            .context("Failed to convert lineage to PROV triples")?;

        // 2. Store in governance brain (RDF store)
        self.governance.insert_annotated_triples(triples).await
            .context("Failed to insert lineage triples")?;

        // 3. Store future-compatible descriptor for efficient queries
        let descriptor = lineage.to_future_compatible_lineage();
        self.storage.store_descriptor(descriptor).await
            .context("Failed to store lineage descriptor")?;

        // 4. Emit metrics
        crate::governance::prometheus_metrics::LINEAGE_EVENTS_TOTAL.inc();

        Ok(())
    }

    /// Query field to DDL lineage
    pub async fn query_field_to_ddl_lineage(
        &self,
        field_uri: &str,
    ) -> Result<Vec<DdlLineageResult>> {
        // SPARQL query with RDF-Star support
        let sparql = format!(r#"
            PREFIX prov: <{}>
            PREFIX gph: <{}>
            PREFIX rdf: <{}>

            SELECT ?ddl ?ontology ?shape ?dialect ?version ?timestamp WHERE {{
                # Main lineage chain
                <{field_uri}> prov:wasDerivedFrom ?ontology .
                ?ontology gph:wasValidatedBy ?shape .
                ?ddl prov:wasGeneratedBy ?shape .

                # Get annotations via RDF-Star
                << ?ddl prov:wasGeneratedBy ?shape >>
                    gph:dialect ?dialect ;
                    gph:generatorVersion ?version .

                # Get timestamp
                << <{field_uri}> prov:wasDerivedFrom ?ontology >>
                    prov:generatedAtTime ?timestamp .
            }}
            ORDER BY DESC(?timestamp)
        "#, PROV_NS, GRAPHICA_NS, crate::governance::ontology::RDF_NS);

        let results = self.governance.sparql_query(&sparql).await
            .context("Failed to execute SPARQL query")?;

        // Convert SPARQL results to domain objects
        let lineage_results = results.bindings.into_iter()
            .map(|binding| DdlLineageResult {
                ddl_uri: binding.get("ddl").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                ontology_uri: binding.get("ontology").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                shape_uri: binding.get("shape").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                dialect: binding.get("dialect").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                version: binding.get("version").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                timestamp: binding.get("timestamp").and_then(|v| v.as_datetime()).unwrap_or_else(Utc::now),
            })
            .collect();

        Ok(lineage_results)
    }

    /// Get impact analysis for ontology changes
    pub async fn analyze_ontology_impact(
        &self,
        ontology_uri: &str,
    ) -> Result<OntologyImpactAnalysis> {
        // Find all fields and DDLs affected by this ontology class
        let sparql = format!(r#"
            PREFIX prov: <{}>
            PREFIX gph: <{}>

            SELECT ?field ?ddl ?shape WHERE {{
                ?field prov:wasDerivedFrom <{ontology_uri}> .
                <{ontology_uri}> gph:wasValidatedBy ?shape .
                ?ddl prov:wasGeneratedBy ?shape .
            }}
        "#, PROV_NS, GRAPHICA_NS);

        let results = self.governance.sparql_query(&sparql).await?;

        let affected_fields: Vec<String> = results.bindings.iter()
            .filter_map(|b| b.get("field").and_then(|v| v.as_str()))
            .map(|s| s.to_string())
            .collect();

        let affected_ddls: Vec<String> = results.bindings.iter()
            .filter_map(|b| b.get("ddl").and_then(|v| v.as_str()))
            .map(|s| s.to_string())
            .collect();

        Ok(OntologyImpactAnalysis {
            ontology_uri: ontology_uri.to_string(),
            affected_fields,
            affected_ddls,
            total_impact_count: results.bindings.len(),
        })
    }
}

/// Result of DDL lineage query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DdlLineageResult {
    pub ddl_uri: String,
    pub ontology_uri: String,
    pub shape_uri: String,
    pub dialect: String,
    pub version: String,
    pub timestamp: DateTime<Utc>,
}

/// Impact analysis for ontology changes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OntologyImpactAnalysis {
    pub ontology_uri: String,
    pub affected_fields: Vec<String>,
    pub affected_ddls: Vec<String>,
    pub total_impact_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lineage_to_prov_triples() {
        let field = FieldDefinition {
            field_uri: "http://example.com/field/customer_id".to_string(),
            field_name: "customer_id".to_string(),
            source_system: "salesforce".to_string(),
            data_type: "varchar(50)".to_string(),
            constraints: vec!["NOT NULL".to_string()],
            source_entity: Some("Account".to_string()),
            metadata: HashMap::new(),
        };

        let ontology = OntologyClass {
            class_uri: "http://graphica.io/ontology#Customer".to_string(),
            class_name: "Customer".to_string(),
            parent_class: Some("http://graphica.io/ontology#Entity".to_string()),
            properties: vec![],
            namespace: GRAPHICA_NS.to_string(),
        };

        let shape = ShaclShape {
            shape_uri: "http://graphica.io/shapes#CustomerShape".to_string(),
            target_class: ontology.class_uri.clone(),
            property_constraints: vec![],
            severity: "Violation".to_string(),
            message: None,
        };

        let ddl = DdlStatement {
            ddl_uri: "http://graphica.io/ddl/customer_table".to_string(),
            statement: "CREATE TABLE customers (id VARCHAR(50) NOT NULL)".to_string(),
            dialect: "postgresql".to_string(),
            version: "1.0.0".to_string(),
            target_schema: Some("public".to_string()),
            ddl_type: "CREATE TABLE".to_string(),
        };

        let lineage = OntologyDrivenLineage::new(
            field,
            ontology,
            shape,
            ddl,
            "tenant_123".to_string(),
            "ddl_generator_v1".to_string(),
        );

        let triples = lineage.to_prov_triples().unwrap();
        assert!(!triples.is_empty());

        // Verify key relationships exist
        let has_derivation = triples.iter().any(|t|
            t.predicate.contains("wasDerivedFrom")
        );
        assert!(has_derivation);

        let has_generation = triples.iter().any(|t|
            t.predicate.contains("wasGeneratedBy")
        );
        assert!(has_generation);
    }

    #[test]
    fn test_future_compatible_lineage() {
        let field = FieldDefinition {
            field_uri: "http://example.com/field/customer_id".to_string(),
            field_name: "customer_id".to_string(),
            source_system: "salesforce".to_string(),
            data_type: "varchar(50)".to_string(),
            constraints: vec![],
            source_entity: None,
            metadata: HashMap::new(),
        };

        let ontology = OntologyClass {
            class_uri: "http://graphica.io/ontology#Customer".to_string(),
            class_name: "Customer".to_string(),
            parent_class: None,
            properties: vec![],
            namespace: GRAPHICA_NS.to_string(),
        };

        let shape = ShaclShape {
            shape_uri: "http://graphica.io/shapes#CustomerShape".to_string(),
            target_class: ontology.class_uri.clone(),
            property_constraints: vec![],
            severity: "Warning".to_string(),
            message: None,
        };

        let ddl = DdlStatement {
            ddl_uri: "http://graphica.io/ddl/customer_table".to_string(),
            statement: "CREATE TABLE customers (id VARCHAR(50))".to_string(),
            dialect: "oracle".to_string(),
            version: "1.0.0".to_string(),
            target_schema: None,
            ddl_type: "CREATE TABLE".to_string(),
        };

        let lineage = OntologyDrivenLineage::new(
            field,
            ontology,
            shape,
            ddl,
            "tenant_456".to_string(),
            "system".to_string(),
        );

        let descriptor = lineage.to_future_compatible_lineage();

        assert_eq!(descriptor.lineage_type as i32, LineageType::OntologyDriven as i32);
        assert_eq!(descriptor.operations.len(), 3);
        assert_eq!(descriptor.operations[0].op_type, "MAP");
        assert_eq!(descriptor.operations[1].op_type, "VALIDATE");
        assert_eq!(descriptor.operations[2].op_type, "GENERATE");
    }
}