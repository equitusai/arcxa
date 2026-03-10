//! PredicateObjectMap Type
//!
//! Defines how to generate RDF predicates and objects.

use super::ObjectMap;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// PredicateObjectMap (rr:predicateObjectMap)
///
/// Defines how to generate RDF predicate-object pairs for a subject.
///
/// ## W3C R2RML Spec
///
/// A predicate-object map consists of:
/// - One or more predicate maps (or predicate constants)
/// - One or more object maps (how to generate object values)
///
/// ## Example
///
/// ```turtle
/// rr:predicateObjectMap [
///     rr:predicate schema:name ;
///     rr:objectMap [ rr:column "full_name" ] ;
/// ] .
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PredicateObjectMap {
    /// Predicate URI (constant) or predicate map
    pub predicate: PredicateSpec,

    /// Object map (how to generate object values)
    pub object_map: ObjectMap,
}

impl PredicateObjectMap {
    /// Create a new predicate-object map with a constant predicate
    pub fn new(predicate_uri: String, object_map: ObjectMap) -> Self {
        Self {
            predicate: PredicateSpec::Constant(predicate_uri),
            object_map,
        }
    }

    /// Create a new predicate-object map with a column-based predicate
    pub fn with_predicate_column(column: String, object_map: ObjectMap) -> Self {
        Self {
            predicate: PredicateSpec::Column(column),
            object_map,
        }
    }

    /// Validate the predicate-object map
    pub fn validate(&self) -> Result<()> {
        self.predicate.validate()?;
        self.object_map.validate()?;
        Ok(())
    }

    /// Generate predicate-object pairs from a row of data
    ///
    /// ## Arguments
    /// - `row`: Map of column names to values
    ///
    /// ## Returns
    /// Generated predicate URI and object value
    pub fn generate_predicate_object(
        &self,
        row: &std::collections::HashMap<String, String>,
    ) -> Result<(String, String, Option<String>)> {
        let predicate = self.predicate.generate_predicate(row)?;
        let (object_value, datatype) = self.object_map.generate_object(row)?;
        Ok((predicate, object_value, datatype))
    }
}

/// PredicateSpec
///
/// Specifies how to generate an RDF predicate.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub enum PredicateSpec {
    /// Constant predicate URI (most common)
    Constant(String),

    /// Predicate from column value
    Column(String),

    /// Predicate from template
    Template(String),
}

impl PredicateSpec {
    /// Validate the predicate specification
    pub fn validate(&self) -> Result<()> {
        match self {
            PredicateSpec::Constant(uri) => {
                if uri.is_empty() {
                    anyhow::bail!("Predicate URI cannot be empty");
                }
            }
            PredicateSpec::Column(column) => {
                if column.is_empty() {
                    anyhow::bail!("Predicate column cannot be empty");
                }
            }
            PredicateSpec::Template(template) => {
                if !template.contains('{') || !template.contains('}') {
                    anyhow::bail!("Predicate template must contain placeholders");
                }
            }
        }
        Ok(())
    }

    /// Generate a predicate URI from a row of data
    pub fn generate_predicate(
        &self,
        row: &std::collections::HashMap<String, String>,
    ) -> Result<String> {
        match self {
            PredicateSpec::Constant(uri) => Ok(uri.clone()),
            PredicateSpec::Column(column) => row
                .get(column)
                .map(|v| v.clone())
                .ok_or_else(|| anyhow::anyhow!("Column not found: {}", column)),
            PredicateSpec::Template(template) => {
                let mut predicate = template.clone();
                for (key, value) in row {
                    let placeholder = format!("{{{}}}", key);
                    predicate = predicate.replace(&placeholder, value);
                }
                Ok(predicate)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_predicate_object_map_creation() {
        let pom = PredicateObjectMap::new(
            "schema:name".to_string(),
            ObjectMap::Column {
                column: "full_name".to_string(),
                datatype: None,
                language: None,
            },
        );

        assert!(matches!(pom.predicate, PredicateSpec::Constant(_)));
        assert!(pom.validate().is_ok());
    }

    #[test]
    fn test_predicate_generation() {
        let predicate = PredicateSpec::Constant("schema:name".to_string());
        let row = HashMap::new();
        assert_eq!(predicate.generate_predicate(&row).unwrap(), "schema:name");

        let predicate = PredicateSpec::Column("predicate_col".to_string());
        let mut row = HashMap::new();
        row.insert("predicate_col".to_string(), "schema:email".to_string());
        assert_eq!(predicate.generate_predicate(&row).unwrap(), "schema:email");
    }

    #[test]
    fn test_predicate_object_generation() {
        let pom = PredicateObjectMap::new(
            "schema:name".to_string(),
            ObjectMap::Column {
                column: "full_name".to_string(),
                datatype: None,
                language: None,
            },
        );

        let mut row = HashMap::new();
        row.insert("full_name".to_string(), "Alice Smith".to_string());

        let (predicate, object, datatype) = pom.generate_predicate_object(&row).unwrap();
        assert_eq!(predicate, "schema:name");
        assert_eq!(object, "Alice Smith");
        assert!(datatype.is_none());
    }
}
