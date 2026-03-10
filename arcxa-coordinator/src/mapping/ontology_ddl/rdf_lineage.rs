//! RDF Lineage Generation for Ontology-Driven DDL
//!
//! Generates W3C PROV-compliant RDF triples tracking the complete lineage chain:
//! Field → Ontology Mapping → SHACL Shape → DDL Statement
//!
//! This implementation uses standard RDF triples for Phase 2.4. It is designed to be
//! forward-compatible with future LE-DAG optimizations (Phase 3.0) without requiring
//! API changes.

use anyhow::Result;
use std::collections::HashMap;

use super::types::{FieldOntologyMapping, OntologyDdlResult};
use crate::mapping::ddl::shacl::types::NodeShape;
use crate::mapping::discovery::types::DiscoveredTable;

/// RDF triple (subject, predicate, object)
pub type RdfTriple = (String, String, String);

/// RDF lineage generator
///
/// Generates W3C PROV-compliant RDF triples documenting the complete
/// ontology-driven DDL generation process.
pub struct RdfLineageGenerator {
    /// Base URI for generated resources
    base_uri: String,

    /// Current timestamp (Unix epoch seconds)
    timestamp: i64,

    /// Agent identifier (e.g., "graphica-coordinator:v1.0.0")
    agent_id: String,

    /// Run identifier for grouping related lineage
    run_id: String,
}

impl RdfLineageGenerator {
    /// Create a new RDF lineage generator
    pub fn new(base_uri: String, agent_id: String, run_id: String) -> Self {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        Self {
            base_uri,
            timestamp,
            agent_id,
            run_id,
        }
    }

    /// Generate complete lineage triples for ontology-driven DDL generation
    ///
    /// Returns RDF triples in (subject, predicate, object) format following W3C PROV.
    ///
    /// # Lineage Chain
    /// 1. Field (prov:Entity) - Source field from discovery
    /// 2. Mapping Activity (prov:Activity) - Ontology mapping process
    /// 3. Ontology Class (prov:Entity) - Mapped semantic class
    /// 4. Shape Activity (prov:Activity) - SHACL shape generation
    /// 5. SHACL Shape (prov:Entity) - Generated shape
    /// 6. DDL Activity (prov:Activity) - DDL generation
    /// 7. DDL Statement (prov:Entity) - Final SQL DDL
    pub fn generate_lineage(
        &self,
        discovered: &DiscoveredTable,
        mappings: &[FieldOntologyMapping],
        shacl_shape: &NodeShape,
        ddl_statements: &[String],
    ) -> Result<Vec<RdfTriple>> {
        let mut triples = Vec::new();

        // Generate run-level metadata
        triples.extend(self.generate_run_metadata());

        // Generate lineage for each field
        for mapping in mappings {
            triples.extend(self.generate_field_lineage(&discovered.name, mapping, shacl_shape)?);
        }

        // Generate DDL activity and statement lineage
        triples.extend(self.generate_ddl_lineage(&discovered.name, shacl_shape, ddl_statements)?);

        Ok(triples)
    }

    /// Generate run metadata triples
    fn generate_run_metadata(&self) -> Vec<RdfTriple> {
        let run_uri = format!("{}run/{}", self.base_uri, self.run_id);
        let xsd = "http://www.w3.org/2001/XMLSchema#";

        vec![
            // Run is an Activity
            (
                run_uri.clone(),
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_string(),
                "http://www.w3.org/ns/prov#Activity".to_string(),
            ),
            // Run start time
            (
                run_uri.clone(),
                "http://www.w3.org/ns/prov#startedAtTime".to_string(),
                format!("\"{}\"^^{}dateTime", self.format_timestamp(), xsd),
            ),
            // Run was associated with agent
            (
                run_uri.clone(),
                "http://www.w3.org/ns/prov#wasAssociatedWith".to_string(),
                format!("{}agent/{}", self.base_uri, self.agent_id),
            ),
        ]
    }

    /// Generate lineage for a single field: Field → Ontology → SHACL
    fn generate_field_lineage(
        &self,
        table_name: &str,
        mapping: &FieldOntologyMapping,
        shacl_shape: &NodeShape,
    ) -> Result<Vec<RdfTriple>> {
        let mut triples = Vec::new();

        // URIs
        let field_uri = format!(
            "{}field/{}/{}",
            self.base_uri, table_name, mapping.field_name
        );
        let mapping_activity_uri =
            format!("{}activity/mapping/{}", self.base_uri, mapping.field_id);
        let ontology_uri = mapping.ontology_uri.clone();
        let shape_activity_uri = format!(
            "{}activity/shape/{}/{}",
            self.base_uri, table_name, mapping.field_name
        );
        let property_shape_uri = format!(
            "{}shape/{}/property/{}",
            self.base_uri, table_name, mapping.field_name
        );

        let rdf_type = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
        let prov = "http://www.w3.org/ns/prov#";
        let gph = "http://graphica.io/ontology#";
        let xsd = "http://www.w3.org/2001/XMLSchema#";

        // 1. Field as prov:Entity
        triples.push((
            field_uri.clone(),
            rdf_type.to_string(),
            format!("{}Entity", prov),
        ));
        triples.push((
            field_uri.clone(),
            format!("{}fieldName", gph),
            format!("\"{}\"", mapping.field_name),
        ));
        triples.push((
            field_uri.clone(),
            format!("{}tableName", gph),
            format!("\"{}\"", table_name),
        ));

        // 2. Mapping Activity
        triples.push((
            mapping_activity_uri.clone(),
            rdf_type.to_string(),
            format!("{}Activity", prov),
        ));
        triples.push((
            mapping_activity_uri.clone(),
            format!("{}used", prov),
            field_uri.clone(),
        ));
        triples.push((
            mapping_activity_uri.clone(),
            format!("{}generated", prov),
            ontology_uri.clone(),
        ));
        triples.push((
            mapping_activity_uri.clone(),
            format!("{}mappingConfidence", gph),
            format!("\"{}\"^^{}double", mapping.confidence, xsd),
        ));
        triples.push((
            mapping_activity_uri.clone(),
            format!("{}mappingMethod", gph),
            format!("\"{}\"", format!("{:?}", mapping.mapping_method)),
        ));
        triples.push((
            mapping_activity_uri.clone(),
            format!("{}wasPartOf", prov),
            format!("{}run/{}", self.base_uri, self.run_id),
        ));

        // 3. Ontology Class (already exists, just link)
        triples.push((
            ontology_uri.clone(),
            format!("{}wasGeneratedBy", prov),
            mapping_activity_uri.clone(),
        ));

        // 4. SHACL Shape Generation Activity
        triples.push((
            shape_activity_uri.clone(),
            rdf_type.to_string(),
            format!("{}Activity", prov),
        ));
        triples.push((
            shape_activity_uri.clone(),
            format!("{}used", prov),
            ontology_uri.clone(),
        ));
        triples.push((
            shape_activity_uri.clone(),
            format!("{}used", prov),
            field_uri.clone(),
        ));
        triples.push((
            shape_activity_uri.clone(),
            format!("{}generated", prov),
            property_shape_uri.clone(),
        ));
        triples.push((
            shape_activity_uri.clone(),
            format!("{}wasPartOf", prov),
            format!("{}run/{}", self.base_uri, self.run_id),
        ));

        // 5. SHACL PropertyShape
        triples.push((
            property_shape_uri.clone(),
            rdf_type.to_string(),
            "http://www.w3.org/ns/shacl#PropertyShape".to_string(),
        ));
        triples.push((
            property_shape_uri.clone(),
            format!("{}wasDerivedFrom", prov),
            ontology_uri.clone(),
        ));
        triples.push((
            property_shape_uri.clone(),
            format!("{}wasDerivedFrom", prov),
            field_uri.clone(),
        ));
        triples.push((
            property_shape_uri.clone(),
            format!("{}wasGeneratedBy", prov),
            shape_activity_uri.clone(),
        ));

        Ok(triples)
    }

    /// Generate DDL lineage: SHACL Shape → DDL Statement
    fn generate_ddl_lineage(
        &self,
        table_name: &str,
        shacl_shape: &NodeShape,
        ddl_statements: &[String],
    ) -> Result<Vec<RdfTriple>> {
        let mut triples = Vec::new();

        // URIs
        let shape_uri = format!("{}shape/{}", self.base_uri, table_name);
        let ddl_activity_uri = format!("{}activity/ddl/{}", self.base_uri, table_name);
        let ddl_statement_uri = format!("{}ddl/{}", self.base_uri, table_name);

        let rdf_type = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
        let prov = "http://www.w3.org/ns/prov#";
        let gph = "http://graphica.io/ontology#";

        // 1. SHACL NodeShape
        triples.push((
            shape_uri.clone(),
            rdf_type.to_string(),
            format!("{}Entity", prov),
        ));
        triples.push((
            shape_uri.clone(),
            rdf_type.to_string(),
            "http://www.w3.org/ns/shacl#NodeShape".to_string(),
        ));
        triples.push((
            shape_uri.clone(),
            "http://www.w3.org/ns/shacl#targetClass".to_string(),
            shacl_shape.target_class.clone(),
        ));

        // 2. DDL Generation Activity
        triples.push((
            ddl_activity_uri.clone(),
            rdf_type.to_string(),
            format!("{}Activity", prov),
        ));
        triples.push((
            ddl_activity_uri.clone(),
            format!("{}used", prov),
            shape_uri.clone(),
        ));
        triples.push((
            ddl_activity_uri.clone(),
            format!("{}generated", prov),
            ddl_statement_uri.clone(),
        ));
        triples.push((
            ddl_activity_uri.clone(),
            format!("{}wasPartOf", prov),
            format!("{}run/{}", self.base_uri, self.run_id),
        ));

        // 3. DDL Statement
        triples.push((
            ddl_statement_uri.clone(),
            rdf_type.to_string(),
            format!("{}Entity", prov),
        ));
        triples.push((
            ddl_statement_uri.clone(),
            format!("{}ddlStatement", gph),
            format!("\"{}\"", ddl_statements.join(";\n")),
        ));
        triples.push((
            ddl_statement_uri.clone(),
            format!("{}wasGeneratedBy", prov),
            ddl_activity_uri.clone(),
        ));
        triples.push((
            ddl_statement_uri.clone(),
            format!("{}wasDerivedFrom", prov),
            shape_uri.clone(),
        ));

        Ok(triples)
    }

    /// Format timestamp as ISO 8601
    fn format_timestamp(&self) -> String {
        use chrono::{DateTime, TimeZone, Utc};
        let dt: DateTime<Utc> = Utc.timestamp_opt(self.timestamp, 0).unwrap();
        dt.to_rfc3339()
    }

    /// Generate lineage summary statistics
    pub fn generate_lineage_summary(&self, triples: &[RdfTriple]) -> LineageSummary {
        let entities = triples
            .iter()
            .filter(|(_, p, o)| p.ends_with("type") && o.contains("Entity"))
            .count();

        let activities = triples
            .iter()
            .filter(|(_, p, o)| p.ends_with("type") && o.contains("Activity"))
            .count();

        let derivations = triples
            .iter()
            .filter(|(_, p, _)| p.contains("wasDerivedFrom") || p.contains("wasGeneratedBy"))
            .count();

        LineageSummary {
            total_triples: triples.len(),
            entity_count: entities,
            activity_count: activities,
            derivation_count: derivations,
            run_id: self.run_id.clone(),
            timestamp: self.timestamp,
        }
    }
}

/// Summary of generated lineage
#[derive(Debug, Clone)]
pub struct LineageSummary {
    pub total_triples: usize,
    pub entity_count: usize,
    pub activity_count: usize,
    pub derivation_count: usize,
    pub run_id: String,
    pub timestamp: i64,
}

/// Helper to add RDF lineage to OntologyDdlResult
pub fn add_lineage_to_result(
    result: &mut OntologyDdlResult,
    discovered: &DiscoveredTable,
    base_uri: String,
    agent_id: String,
    run_id: String,
) -> Result<()> {
    let generator = RdfLineageGenerator::new(base_uri, agent_id, run_id);

    let triples = generator.generate_lineage(
        discovered,
        &result.ontology_mappings,
        &result.shacl_shape,
        &result.ddl_statements,
    )?;

    result.rdf_triples = Some(triples);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::ddl::shacl::types::{NodeShape, PropertyShape};
    use crate::mapping::discovery::types::{ColumnStatistics, DiscoveredColumn, DiscoveredTable};
    use crate::mapping::ontology_ddl::types::MappingMethod;

    fn create_test_table() -> DiscoveredTable {
        DiscoveredTable {
            name: "customers".to_string(),
            columns: vec![DiscoveredColumn {
                name: "email".to_string(),
                data_type: "VARCHAR(255)".to_string(),
                nullable: false,
                primary_key: false,
                semantic_type: None,
                confidence: 0.95,
                patterns: vec![],
                statistics: ColumnStatistics::default(),
                sample_values: vec![],
            }],
            row_count: Some(1000),
        }
    }

    fn create_test_mapping() -> FieldOntologyMapping {
        FieldOntologyMapping {
            field_id: "customers_email".to_string(),
            field_name: "email".to_string(),
            table_name: "customers".to_string(),
            ontology_uri: "http://schema.org/email".to_string(),
            confidence: 0.95,
            mapping_method: MappingMethod::PatternInference,
            mapped_at: 1234567890,
        }
    }

    fn create_test_shape() -> NodeShape {
        let mut shape = NodeShape::new(
            "http://graphica.io/shape/customers".to_string(),
            "http://graphica.io/class/customers".to_string(),
        );

        let mut prop = PropertyShape::new("http://schema.org/email".to_string());
        prop.name = Some("email".to_string());
        prop.datatype = Some("http://www.w3.org/2001/XMLSchema#string".to_string());
        prop.max_length = Some(255);
        prop.min_count = Some(1);

        shape.add_property(prop);
        shape
    }

    #[test]
    fn test_generate_lineage_basic() {
        let generator = RdfLineageGenerator::new(
            "http://graphica.io/".to_string(),
            "test-agent".to_string(),
            "run-001".to_string(),
        );

        let table = create_test_table();
        let mappings = vec![create_test_mapping()];
        let shape = create_test_shape();
        let ddl = vec!["CREATE TABLE customers (email VARCHAR(255) NOT NULL);".to_string()];

        let triples = generator
            .generate_lineage(&table, &mappings, &shape, &ddl)
            .unwrap();

        // Should have run metadata + field lineage + DDL lineage
        assert!(triples.len() > 10);

        // Check for key PROV patterns
        let has_entity = triples
            .iter()
            .any(|(_, p, o)| p.ends_with("type") && o.contains("Entity"));
        assert!(has_entity, "Should have prov:Entity");

        let has_activity = triples
            .iter()
            .any(|(_, p, o)| p.ends_with("type") && o.contains("Activity"));
        assert!(has_activity, "Should have prov:Activity");

        let has_derivation = triples.iter().any(|(_, p, _)| p.contains("wasDerivedFrom"));
        assert!(has_derivation, "Should have prov:wasDerivedFrom");
    }

    #[test]
    fn test_run_metadata() {
        let generator = RdfLineageGenerator::new(
            "http://graphica.io/".to_string(),
            "test-agent".to_string(),
            "run-001".to_string(),
        );

        let metadata = generator.generate_run_metadata();

        assert!(metadata.len() >= 3);

        // Check run URI
        let has_run_type = metadata.iter().any(|(s, p, o)| {
            s.contains("run-001") && p.ends_with("type") && o.contains("Activity")
        });
        assert!(has_run_type, "Run should be an Activity");

        // Check agent association
        let has_agent = metadata
            .iter()
            .any(|(_, p, o)| p.contains("wasAssociatedWith") && o.contains("test-agent"));
        assert!(has_agent, "Run should be associated with agent");
    }

    #[test]
    fn test_field_lineage_chain() {
        let generator = RdfLineageGenerator::new(
            "http://graphica.io/".to_string(),
            "test-agent".to_string(),
            "run-001".to_string(),
        );

        let mapping = create_test_mapping();
        let shape = create_test_shape();

        let triples = generator
            .generate_field_lineage("customers", &mapping, &shape)
            .unwrap();

        // Should have: Field → Mapping Activity → Ontology → Shape Activity → PropertyShape
        // Check for field entity
        let has_field = triples.iter().any(|(s, p, o)| {
            s.contains("field/customers/email") && p.ends_with("type") && o.contains("Entity")
        });
        assert!(has_field, "Should have field entity");

        // Check for mapping activity
        let has_mapping = triples.iter().any(|(s, p, o)| {
            s.contains("activity/mapping") && p.ends_with("type") && o.contains("Activity")
        });
        assert!(has_mapping, "Should have mapping activity");

        // Check for ontology usage
        let has_ontology = triples.iter().any(|(s, p, o)| {
            s.contains("activity/mapping") && p.contains("generated") && o.contains("schema.org")
        });
        assert!(has_ontology, "Should generate ontology reference");

        // Check for SHACL shape
        let has_shape = triples.iter().any(|(s, p, o)| {
            s.contains("shape/customers") && p.ends_with("type") && o.contains("PropertyShape")
        });
        assert!(has_shape, "Should have PropertyShape");
    }

    #[test]
    fn test_ddl_lineage() {
        let generator = RdfLineageGenerator::new(
            "http://graphica.io/".to_string(),
            "test-agent".to_string(),
            "run-001".to_string(),
        );

        let shape = create_test_shape();
        let ddl = vec!["CREATE TABLE customers (email VARCHAR(255) NOT NULL);".to_string()];

        let triples = generator
            .generate_ddl_lineage("customers", &shape, &ddl)
            .unwrap();

        // Should have: NodeShape → DDL Activity → DDL Statement
        let has_node_shape = triples.iter().any(|(s, p, o)| {
            s.contains("shape/customers") && p.ends_with("type") && o.contains("NodeShape")
        });
        assert!(has_node_shape, "Should have NodeShape");

        let has_ddl_activity = triples.iter().any(|(s, p, o)| {
            s.contains("activity/ddl") && p.ends_with("type") && o.contains("Activity")
        });
        assert!(has_ddl_activity, "Should have DDL activity");

        let has_ddl_statement = triples
            .iter()
            .any(|(s, p, _)| s.contains("ddl/customers") && p.contains("ddlStatement"));
        assert!(has_ddl_statement, "Should have DDL statement");
    }

    #[test]
    fn test_confidence_tracking() {
        let generator = RdfLineageGenerator::new(
            "http://graphica.io/".to_string(),
            "test-agent".to_string(),
            "run-001".to_string(),
        );

        let mapping = create_test_mapping();
        let shape = create_test_shape();

        let triples = generator
            .generate_field_lineage("customers", &mapping, &shape)
            .unwrap();

        // Check for confidence score
        let has_confidence = triples
            .iter()
            .any(|(_, p, o)| p.contains("mappingConfidence") && o.contains("0.95"));
        assert!(has_confidence, "Should track mapping confidence");
    }

    #[test]
    fn test_lineage_summary() {
        let generator = RdfLineageGenerator::new(
            "http://graphica.io/".to_string(),
            "test-agent".to_string(),
            "run-001".to_string(),
        );

        let table = create_test_table();
        let mappings = vec![create_test_mapping()];
        let shape = create_test_shape();
        let ddl = vec!["CREATE TABLE customers (email VARCHAR(255) NOT NULL);".to_string()];

        let triples = generator
            .generate_lineage(&table, &mappings, &shape, &ddl)
            .unwrap();
        let summary = generator.generate_lineage_summary(&triples);

        assert!(summary.total_triples > 0);
        assert!(summary.entity_count > 0);
        assert!(summary.activity_count > 0);
        assert!(summary.derivation_count > 0);
        assert_eq!(summary.run_id, "run-001");
    }

    #[test]
    fn test_multiple_fields_lineage() {
        let generator = RdfLineageGenerator::new(
            "http://graphica.io/".to_string(),
            "test-agent".to_string(),
            "run-001".to_string(),
        );

        let mut table = create_test_table();
        table.columns.push(DiscoveredColumn {
            name: "age".to_string(),
            data_type: "INTEGER".to_string(),
            nullable: true,
            primary_key: false,
            semantic_type: None,
            confidence: 0.88,
            patterns: vec![],
            statistics: ColumnStatistics::default(),
            sample_values: vec![],
        });

        let mappings = vec![
            create_test_mapping(),
            FieldOntologyMapping {
                field_id: "customers_age".to_string(),
                field_name: "age".to_string(),
                table_name: "customers".to_string(),
                ontology_uri: "http://schema.org/age".to_string(),
                confidence: 0.88,
                mapping_method: MappingMethod::PatternInference,
                mapped_at: 1234567890,
            },
        ];

        let shape = create_test_shape();
        let ddl =
            vec!["CREATE TABLE customers (email VARCHAR(255) NOT NULL, age INTEGER);".to_string()];

        let triples = generator
            .generate_lineage(&table, &mappings, &shape, &ddl)
            .unwrap();

        // Should have lineage for both fields
        let email_field = triples
            .iter()
            .any(|(s, _, _)| s.contains("field/customers/email"));
        let age_field = triples
            .iter()
            .any(|(s, _, _)| s.contains("field/customers/age"));

        assert!(email_field, "Should have email field lineage");
        assert!(age_field, "Should have age field lineage");
    }

    #[test]
    fn test_w3c_prov_compliance() {
        let generator = RdfLineageGenerator::new(
            "http://graphica.io/".to_string(),
            "test-agent".to_string(),
            "run-001".to_string(),
        );

        let table = create_test_table();
        let mappings = vec![create_test_mapping()];
        let shape = create_test_shape();
        let ddl = vec!["CREATE TABLE customers (email VARCHAR(255) NOT NULL);".to_string()];

        let triples = generator
            .generate_lineage(&table, &mappings, &shape, &ddl)
            .unwrap();

        // Check for W3C PROV namespace usage
        let prov_namespace = "http://www.w3.org/ns/prov#";

        let uses_prov = triples.iter().any(|(_, p, _)| p.contains(prov_namespace));
        assert!(uses_prov, "Should use W3C PROV namespace");

        // Check for key PROV relationships
        let has_used = triples.iter().any(|(_, p, _)| p.contains("used"));
        let has_generated = triples.iter().any(|(_, p, _)| p.contains("generated"));
        let has_was_derived = triples.iter().any(|(_, p, _)| p.contains("wasDerivedFrom"));

        assert!(has_used, "Should have prov:used");
        assert!(has_generated, "Should have prov:generated");
        assert!(has_was_derived, "Should have prov:wasDerivedFrom");
    }
}
