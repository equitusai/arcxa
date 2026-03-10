//! R2RML Turtle Serialization
//!
//! Converts R2RML mapping structures to W3C-compliant R2RML Turtle format.

use crate::mapping::semantic_mapping::rdf::r2rml_types::predicate_object_map::PredicateSpec;
use crate::mapping::semantic_mapping::rdf::r2rml_types::*;
use anyhow::Result;

/// R2RML Serializer
///
/// Converts R2RML types to Turtle format following the W3C R2RML specification.
pub struct R2rmlSerializer;

impl Default for R2rmlSerializer {
    fn default() -> Self {
        Self
    }
}

impl R2rmlSerializer {
    /// Create a new R2RML serializer
    pub fn new() -> Self {
        Self::default()
    }

    /// Serialize an R2RML mapping to Turtle format
    ///
    /// ## Arguments
    /// - `mapping`: R2RML mapping to serialize
    ///
    /// ## Returns
    /// R2RML Turtle string
    pub fn serialize(&self, mapping: &R2rmlMapping) -> Result<String> {
        let mut turtle = String::new();

        // Add prefixes
        turtle.push_str(&self.generate_prefixes());
        turtle.push('\n');

        // Add mapping metadata
        turtle.push_str(&self.serialize_mapping_metadata(mapping)?);
        turtle.push('\n');

        // Serialize each triples map
        for triples_map in &mapping.triples_maps {
            turtle.push_str(&self.serialize_triples_map(triples_map, &mapping.base_uri)?);
            turtle.push_str("\n\n");
        }

        Ok(turtle)
    }

    /// Generate R2RML Turtle prefixes
    fn generate_prefixes(&self) -> String {
        vec![
            "@prefix rr: <http://www.w3.org/ns/r2rml#> .",
            "@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .",
            "@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .",
            "@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .",
            "@prefix schema: <http://schema.org/> .",
            "@prefix gph: <http://graphica.io/ontology#> .",
            "@prefix dcterms: <http://purl.org/dc/terms/> .",
        ]
        .join("\n")
    }

    /// Serialize mapping metadata
    fn serialize_mapping_metadata(&self, mapping: &R2rmlMapping) -> Result<String> {
        let mut turtle = String::new();
        let mapping_uri = mapping.get_mapping_uri();

        turtle.push_str(&format!("<{}> a gph:R2RMLMapping ;\n", mapping_uri));
        turtle.push_str(&format!(
            "    dcterms:identifier \"{}\" ;\n",
            mapping.mapping_id
        ));
        turtle.push_str(&format!("    gph:baseUri \"{}\" ;\n", mapping.base_uri));
        turtle.push_str(&format!(
            "    gph:sourceDataset \"{}\" ;\n",
            mapping.source_dataset
        ));

        if let Some(description) = &mapping.description {
            turtle.push_str(&format!("    dcterms:description \"{}\" ;\n", description));
        }

        if let Some(target_graph) = &mapping.target_graph {
            turtle.push_str(&format!("    gph:targetGraph <{}> ;\n", target_graph));
        }

        turtle.push_str(&format!(
            "    dcterms:created \"{}\"^^xsd:dateTime ;\n",
            mapping.created_at.to_rfc3339()
        ));
        turtle.push_str(&format!(
            "    dcterms:modified \"{}\"^^xsd:dateTime .\n",
            mapping.updated_at.to_rfc3339()
        ));

        Ok(turtle)
    }

    /// Serialize a triples map
    fn serialize_triples_map(&self, tm: &TriplesMap, base_uri: &str) -> Result<String> {
        let mut turtle = String::new();
        let tm_uri = tm.get_uri(base_uri);

        turtle.push_str(&format!("<{}> a rr:TriplesMap ;\n", tm_uri));
        turtle.push_str(&format!("    rdfs:label \"{}\" ;\n", tm.name));

        // Logical table
        turtle.push_str(&self.serialize_logical_table(&tm.logical_table)?);

        // Subject map
        turtle.push_str(&self.serialize_subject_map(&tm.subject_map)?);

        // Predicate-object maps
        for (idx, pom) in tm.predicate_object_maps.iter().enumerate() {
            turtle.push_str(&self.serialize_predicate_object_map(pom)?);
            if idx < tm.predicate_object_maps.len() - 1 {
                turtle.push_str(" ;\n");
            } else {
                turtle.push_str(" .\n");
            }
        }

        Ok(turtle)
    }

    /// Serialize a logical table
    fn serialize_logical_table(&self, lt: &LogicalTable) -> Result<String> {
        let mut turtle = String::new();
        turtle.push_str("    rr:logicalTable [\n");

        match lt {
            LogicalTable::TableName { table_name } => {
                turtle.push_str(&format!("        rr:tableName \"{}\" ;\n", table_name));
            }
            LogicalTable::SqlQuery { query } => {
                turtle.push_str(&format!("        rr:sqlQuery \"\"\"{}\"\"\" ;\n", query));
            }
        }

        turtle.push_str("    ] ;\n");
        Ok(turtle)
    }

    /// Serialize a subject map
    fn serialize_subject_map(&self, sm: &SubjectMap) -> Result<String> {
        let mut turtle = String::new();
        turtle.push_str("    rr:subjectMap [\n");

        // Template, column, or constant
        if let Some(template) = &sm.template {
            turtle.push_str(&format!("        rr:template \"{}\" ;\n", template));
        } else if let Some(column) = &sm.column {
            turtle.push_str(&format!("        rr:column \"{}\" ;\n", column));
        } else if let Some(constant) = &sm.constant {
            turtle.push_str(&format!("        rr:constant <{}> ;\n", constant));
        }

        // Term type
        turtle.push_str(&format!("        rr:termType rr:{:?} ;\n", sm.term_type));

        // RDF classes
        if let Some(classes) = &sm.class {
            for class in classes {
                turtle.push_str(&format!("        rr:class <{}> ;\n", class));
            }
        }

        turtle.push_str("    ] ;\n");
        Ok(turtle)
    }

    /// Serialize a predicate-object map
    fn serialize_predicate_object_map(&self, pom: &PredicateObjectMap) -> Result<String> {
        let mut turtle = String::new();
        turtle.push_str("    rr:predicateObjectMap [\n");

        // Predicate
        match &pom.predicate {
            PredicateSpec::Constant(uri) => {
                turtle.push_str(&format!("        rr:predicate <{}> ;\n", uri));
            }
            PredicateSpec::Column(column) => {
                turtle.push_str(&format!(
                    "        rr:predicateMap [ rr:column \"{}\" ] ;\n",
                    column
                ));
            }
            PredicateSpec::Template(template) => {
                turtle.push_str(&format!(
                    "        rr:predicateMap [ rr:template \"{}\" ] ;\n",
                    template
                ));
            }
        }

        // Object map
        turtle.push_str(&self.serialize_object_map(&pom.object_map)?);

        turtle.push_str("    ]");
        Ok(turtle)
    }

    /// Serialize an object map
    fn serialize_object_map(&self, om: &ObjectMap) -> Result<String> {
        let mut turtle = String::new();
        turtle.push_str("        rr:objectMap [\n");

        match om {
            ObjectMap::Column {
                column,
                datatype,
                language,
            } => {
                turtle.push_str(&format!("            rr:column \"{}\" ;\n", column));
                if let Some(datatype) = datatype {
                    turtle.push_str(&format!("            rr:datatype <{}> ;\n", datatype));
                }
                if let Some(language) = language {
                    turtle.push_str(&format!("            rr:language \"{}\" ;\n", language));
                }
            }
            ObjectMap::Constant {
                value,
                datatype,
                language,
            } => {
                turtle.push_str(&format!("            rr:constant \"{}\" ;\n", value));
                if let Some(datatype) = datatype {
                    turtle.push_str(&format!("            rr:datatype <{}> ;\n", datatype));
                }
                if let Some(language) = language {
                    turtle.push_str(&format!("            rr:language \"{}\" ;\n", language));
                }
            }
            ObjectMap::Template { template, datatype } => {
                turtle.push_str(&format!("            rr:template \"{}\" ;\n", template));
                if let Some(datatype) = datatype {
                    turtle.push_str(&format!("            rr:datatype <{}> ;\n", datatype));
                }
            }
            ObjectMap::Reference {
                parent_triples_map,
                join_conditions,
            } => {
                turtle.push_str(&format!(
                    "            rr:parentTriplesMap <#{}> ;\n",
                    parent_triples_map
                ));
                for jc in join_conditions {
                    turtle.push_str("            rr:joinCondition [\n");
                    turtle.push_str(&format!("                rr:child \"{}\" ;\n", jc.child));
                    turtle.push_str(&format!("                rr:parent \"{}\" ;\n", jc.parent));
                    turtle.push_str("            ] ;\n");
                }
            }
        }

        turtle.push_str("        ] ;\n");
        Ok(turtle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::r2rml::types::*;

    #[test]
    fn test_r2rml_serialization() {
        let mut mapping = R2rmlMapping::new(
            "test-mapping".to_string(),
            "http://example.com/".to_string(),
            "customers.csv".to_string(),
        );

        let logical_table = LogicalTable::from_table_name("customers.csv".to_string());
        let subject_map =
            SubjectMap::from_template("http://example.com/customer/{customer_id}".to_string())
                .with_class("schema:Person".to_string());

        let mut triples_map =
            TriplesMap::new("CustomerMap".to_string(), logical_table, subject_map);

        triples_map.add_predicate_object_map(PredicateObjectMap::new(
            "schema:name".to_string(),
            ObjectMap::from_column("full_name".to_string()),
        ));

        triples_map.add_predicate_object_map(PredicateObjectMap::new(
            "schema:email".to_string(),
            ObjectMap::from_column("email".to_string()),
        ));

        mapping.add_triples_map(triples_map);

        let serializer = R2rmlSerializer::new();
        let turtle = serializer.serialize(&mapping).unwrap();

        // Verify key elements
        assert!(turtle.contains("@prefix rr:"));
        assert!(turtle.contains("rr:TriplesMap"));
        assert!(turtle.contains("rr:logicalTable"));
        assert!(turtle.contains("rr:subjectMap"));
        assert!(turtle.contains("rr:predicateObjectMap"));
        assert!(turtle.contains("schema:Person"));
        assert!(turtle.contains("schema:name"));
        assert!(turtle.contains("schema:email"));

        println!("Generated R2RML Turtle:\n{}", turtle);
    }

    #[test]
    fn test_r2rml_serialization_with_datatype() {
        let mut mapping = R2rmlMapping::new(
            "typed-mapping".to_string(),
            "http://example.com/".to_string(),
            "products.csv".to_string(),
        );

        let logical_table = LogicalTable::from_table_name("products.csv".to_string());
        let subject_map =
            SubjectMap::from_template("http://example.com/product/{product_id}".to_string());

        let mut triples_map = TriplesMap::new("ProductMap".to_string(), logical_table, subject_map);

        triples_map.add_predicate_object_map(PredicateObjectMap::new(
            "schema:price".to_string(),
            ObjectMap::from_column_typed("price".to_string(), "xsd:decimal".to_string()),
        ));

        mapping.add_triples_map(triples_map);

        let serializer = R2rmlSerializer::new();
        let turtle = serializer.serialize(&mapping).unwrap();

        assert!(turtle.contains("xsd:decimal"));
        assert!(turtle.contains("rr:datatype"));
    }
}
