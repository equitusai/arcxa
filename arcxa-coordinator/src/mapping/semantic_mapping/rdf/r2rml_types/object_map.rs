//! ObjectMap Type
//!
//! Defines how to generate RDF objects (literals, URIs, or references).

use anyhow::Result;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// ObjectMap (rr:objectMap)
///
/// Defines how to generate RDF objects from source data.
///
/// ## W3C R2RML Spec
///
/// An object map can generate objects using:
/// - **Column**: Direct column value
/// - **Constant**: Fixed value for all rows
/// - **Template**: Template with placeholders
/// - **Reference**: Reference to another triples map (for joins)
///
/// ## Examples
///
/// ### Column-based Object
/// ```turtle
/// rr:objectMap [ rr:column "full_name" ] .
/// ```
///
/// ### Constant Object
/// ```turtle
/// rr:objectMap [ rr:constant "Active" ] .
/// ```
///
/// ### Template Object
/// ```turtle
/// rr:objectMap [ rr:template "http://example.com/country/{country_code}" ] .
/// ```
///
/// ### Reference Object (Foreign Key)
/// ```turtle
/// rr:objectMap [
///     rr:parentTriplesMap <#CountryMap> ;
///     rr:joinCondition [
///         rr:child "country_code" ;
///         rr:parent "code" ;
///     ] ;
/// ] .
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type")]
pub enum ObjectMap {
    /// Object from column value (most common)
    Column {
        column: String,
        datatype: Option<String>,
        language: Option<String>,
    },

    /// Constant object value
    Constant {
        value: String,
        datatype: Option<String>,
        language: Option<String>,
    },

    /// Object from URI template
    Template {
        template: String,
        datatype: Option<String>,
    },

    /// Reference to another triples map (foreign key relationship)
    Reference {
        parent_triples_map: String,
        join_conditions: Vec<JoinCondition>,
    },
}

impl ObjectMap {
    /// Create a column-based object map
    pub fn from_column(column: String) -> Self {
        ObjectMap::Column {
            column,
            datatype: None,
            language: None,
        }
    }

    /// Create a column-based object map with datatype
    pub fn from_column_typed(column: String, datatype: String) -> Self {
        ObjectMap::Column {
            column,
            datatype: Some(datatype),
            language: None,
        }
    }

    /// Create a constant object map
    pub fn from_constant(value: String) -> Self {
        ObjectMap::Constant {
            value,
            datatype: None,
            language: None,
        }
    }

    /// Create a template-based object map
    pub fn from_template(template: String) -> Self {
        ObjectMap::Template {
            template,
            datatype: None,
        }
    }

    /// Create a reference object map (foreign key)
    pub fn from_reference(parent_triples_map: String, join_conditions: Vec<JoinCondition>) -> Self {
        ObjectMap::Reference {
            parent_triples_map,
            join_conditions,
        }
    }

    /// Validate the object map
    pub fn validate(&self) -> Result<()> {
        match self {
            ObjectMap::Column { column, .. } => {
                if column.is_empty() {
                    anyhow::bail!("Object column cannot be empty");
                }
            }
            ObjectMap::Constant { value, .. } => {
                if value.is_empty() {
                    anyhow::bail!("Object constant cannot be empty");
                }
            }
            ObjectMap::Template { template, .. } => {
                if !template.contains('{') || !template.contains('}') {
                    anyhow::bail!("Object template must contain placeholders");
                }
            }
            ObjectMap::Reference {
                parent_triples_map,
                join_conditions,
            } => {
                if parent_triples_map.is_empty() {
                    anyhow::bail!("Parent triples map cannot be empty");
                }
                if join_conditions.is_empty() {
                    anyhow::bail!("Reference object map must have at least one join condition");
                }
                for jc in join_conditions {
                    jc.validate()?;
                }
            }
        }
        Ok(())
    }

    /// Generate an object value from a row of data
    ///
    /// ## Arguments
    /// - `row`: Map of column names to values
    ///
    /// ## Returns
    /// Tuple of (object_value, optional_datatype)
    pub fn generate_object(
        &self,
        row: &std::collections::HashMap<String, String>,
    ) -> Result<(String, Option<String>)> {
        match self {
            ObjectMap::Column {
                column,
                datatype,
                language: _,
            } => {
                let value = row
                    .get(column)
                    .ok_or_else(|| anyhow::anyhow!("Column not found: {}", column))?
                    .clone();
                Ok((value, datatype.clone()))
            }
            ObjectMap::Constant {
                value,
                datatype,
                language: _,
            } => Ok((value.clone(), datatype.clone())),
            ObjectMap::Template { template, datatype } => {
                let mut object = template.clone();
                for (key, value) in row {
                    let placeholder = format!("{{{}}}", key);
                    object = object.replace(&placeholder, value);
                }
                Ok((object, datatype.clone()))
            }
            ObjectMap::Reference { .. } => {
                // References require joining with another triples map
                // This will be handled by the executor
                anyhow::bail!("Reference object maps require executor to resolve");
            }
        }
    }

    /// Get the datatype URI if specified
    pub fn get_datatype(&self) -> Option<&str> {
        match self {
            ObjectMap::Column { datatype, .. } => datatype.as_deref(),
            ObjectMap::Constant { datatype, .. } => datatype.as_deref(),
            ObjectMap::Template { datatype, .. } => datatype.as_deref(),
            ObjectMap::Reference { .. } => None,
        }
    }

    /// Get the language tag if specified
    pub fn get_language(&self) -> Option<&str> {
        match self {
            ObjectMap::Column { language, .. } => language.as_deref(),
            ObjectMap::Constant { language, .. } => language.as_deref(),
            _ => None,
        }
    }
}

/// JoinCondition (rr:joinCondition)
///
/// Defines how to join a child table with a parent table for reference object maps.
///
/// ## Example
///
/// ```turtle
/// rr:joinCondition [
///     rr:child "country_code" ;
///     rr:parent "code" ;
/// ] .
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct JoinCondition {
    /// Child column name (in the source table)
    pub child: String,

    /// Parent column name (in the referenced table)
    pub parent: String,
}

impl JoinCondition {
    /// Create a new join condition
    pub fn new(child: String, parent: String) -> Self {
        Self { child, parent }
    }

    /// Validate the join condition
    pub fn validate(&self) -> Result<()> {
        if self.child.is_empty() {
            anyhow::bail!("Join condition child column cannot be empty");
        }
        if self.parent.is_empty() {
            anyhow::bail!("Join condition parent column cannot be empty");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_object_map_from_column() {
        let om = ObjectMap::from_column("full_name".to_string());
        assert!(om.validate().is_ok());

        let mut row = HashMap::new();
        row.insert("full_name".to_string(), "Alice Smith".to_string());

        let (value, datatype) = om.generate_object(&row).unwrap();
        assert_eq!(value, "Alice Smith");
        assert!(datatype.is_none());
    }

    #[test]
    fn test_object_map_from_column_typed() {
        let om = ObjectMap::from_column_typed("age".to_string(), "xsd:integer".to_string());
        assert!(om.validate().is_ok());

        let mut row = HashMap::new();
        row.insert("age".to_string(), "30".to_string());

        let (value, datatype) = om.generate_object(&row).unwrap();
        assert_eq!(value, "30");
        assert_eq!(datatype.as_deref(), Some("xsd:integer"));
    }

    #[test]
    fn test_object_map_from_constant() {
        let om = ObjectMap::from_constant("Active".to_string());
        assert!(om.validate().is_ok());

        let row = HashMap::new();
        let (value, _) = om.generate_object(&row).unwrap();
        assert_eq!(value, "Active");
    }

    #[test]
    fn test_object_map_from_template() {
        let om = ObjectMap::from_template("http://example.com/country/{country_code}".to_string());
        assert!(om.validate().is_ok());

        let mut row = HashMap::new();
        row.insert("country_code".to_string(), "US".to_string());

        let (value, _) = om.generate_object(&row).unwrap();
        assert_eq!(value, "http://example.com/country/US");
    }

    #[test]
    fn test_object_map_from_reference() {
        let om = ObjectMap::from_reference(
            "CountryMap".to_string(),
            vec![JoinCondition::new(
                "country_code".to_string(),
                "code".to_string(),
            )],
        );
        assert!(om.validate().is_ok());
    }

    #[test]
    fn test_join_condition_validation() {
        let jc = JoinCondition::new("child_col".to_string(), "parent_col".to_string());
        assert!(jc.validate().is_ok());

        let jc = JoinCondition::new("".to_string(), "parent_col".to_string());
        assert!(jc.validate().is_err());
    }
}
