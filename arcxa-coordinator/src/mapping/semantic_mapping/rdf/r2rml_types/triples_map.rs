//! TriplesMap Type
//!
//! Defines how to generate RDF triples from a logical table.

use super::{LogicalTable, PredicateObjectMap, SubjectMap};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// TriplesMap (rr:TriplesMap)
///
/// A triples map defines how to generate RDF triples from a logical table (CSV/Parquet).
///
/// ## W3C R2RML Spec
///
/// A triples map consists of:
/// - A logical table (the source data)
/// - A subject map (how to generate subjects)
/// - Zero or more predicate-object maps (how to generate predicates and objects)
///
/// ## Example
///
/// ```turtle
/// <#CustomerMap> a rr:TriplesMap ;
///     rr:logicalTable [ rr:tableName "customers.csv" ] ;
///     rr:subjectMap [
///         rr:template "http://example.com/customer/{customer_id}" ;
///         rr:class schema:Person ;
///     ] ;
///     rr:predicateObjectMap [
///         rr:predicate schema:name ;
///         rr:objectMap [ rr:column "full_name" ] ;
///     ] .
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TriplesMap {
    /// Unique name for this triples map
    pub name: String,

    /// Logical table (source data)
    pub logical_table: LogicalTable,

    /// Subject map (how to generate RDF subjects)
    pub subject_map: SubjectMap,

    /// Predicate-object maps (how to generate predicates and objects)
    pub predicate_object_maps: Vec<PredicateObjectMap>,

    /// Optional graph map (named graph for these triples)
    pub graph_map: Option<GraphMap>,
}

impl TriplesMap {
    /// Create a new triples map
    pub fn new(name: String, logical_table: LogicalTable, subject_map: SubjectMap) -> Self {
        Self {
            name,
            logical_table,
            subject_map,
            predicate_object_maps: vec![],
            graph_map: None,
        }
    }

    /// Add a predicate-object map
    pub fn add_predicate_object_map(&mut self, pom: PredicateObjectMap) {
        self.predicate_object_maps.push(pom);
    }

    /// Validate the triples map structure
    pub fn validate(&self) -> Result<()> {
        if self.name.is_empty() {
            anyhow::bail!("TriplesMap name cannot be empty");
        }

        self.logical_table.validate()?;
        self.subject_map.validate()?;

        if self.predicate_object_maps.is_empty() {
            anyhow::bail!("TriplesMap must have at least one PredicateObjectMap");
        }

        for pom in &self.predicate_object_maps {
            pom.validate()?;
        }

        Ok(())
    }

    /// Get URI for this triples map
    pub fn get_uri(&self, base_uri: &str) -> String {
        format!("{}#TriplesMap/{}", base_uri, self.name)
    }
}

/// GraphMap (rr:graphMap)
///
/// Specifies a named graph for the generated triples.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub enum GraphMap {
    /// Constant graph URI
    Constant { graph_uri: String },
    /// Graph URI from column value
    Column { column: String },
    /// Graph URI from template
    Template { template: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::semantic_mapping::rdf::r2rml_types::{SubjectMap, TermType};

    #[test]
    fn test_triples_map_creation() {
        let logical_table = LogicalTable::TableName {
            table_name: "customers.csv".to_string(),
        };
        let subject_map = SubjectMap {
            template: Some("http://example.com/customer/{id}".to_string()),
            column: None,
            constant: None,
            term_type: TermType::IRI,
            class: Some(vec!["schema:Person".to_string()]),
        };

        let tm = TriplesMap::new("CustomerMap".to_string(), logical_table, subject_map);

        assert_eq!(tm.name, "CustomerMap");
        assert!(tm.predicate_object_maps.is_empty());
    }

    #[test]
    fn test_triples_map_validation_requires_pom() {
        let logical_table = LogicalTable::TableName {
            table_name: "customers.csv".to_string(),
        };
        let subject_map = SubjectMap {
            template: Some("http://example.com/customer/{id}".to_string()),
            column: None,
            constant: None,
            term_type: TermType::IRI,
            class: Some(vec!["schema:Person".to_string()]),
        };

        let tm = TriplesMap::new("CustomerMap".to_string(), logical_table, subject_map);

        // Should fail - no predicate-object maps
        assert!(tm.validate().is_err());
    }
}
