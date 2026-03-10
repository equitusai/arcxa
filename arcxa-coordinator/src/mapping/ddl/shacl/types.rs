//! SHACL Types
//!
//! Data structures representing SHACL shapes and constraints.

use serde::{Deserialize, Serialize};

/// SHACL Node Shape
///
/// Represents a SHACL node shape, which defines constraints for a class of RDF resources.
/// In DDL generation, a NodeShape typically maps to a SQL table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeShape {
    /// Shape URI (e.g., "http://example.com/shape/Customer")
    pub uri: String,

    /// Target class URI (e.g., "http://example.com/Customer")
    pub target_class: String,

    /// Human-readable label for the shape
    pub label: Option<String>,

    /// Property shapes (columns)
    pub properties: Vec<PropertyShape>,

    /// Whether the shape is closed (sh:closed)
    /// If true, only declared properties are allowed
    pub closed: bool,

    /// Severity level (sh:severity)
    pub severity: Option<SeverityLevel>,
}

/// SHACL Property Shape
///
/// Represents a SHACL property shape, which defines constraints for a property.
/// In DDL generation, a PropertyShape typically maps to a SQL column.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertyShape {
    /// Property path (RDF property URI)
    pub path: String,

    /// Suggested column name (derived from property)
    pub name: Option<String>,

    /// XSD datatype (e.g., "http://www.w3.org/2001/XMLSchema#string")
    pub datatype: Option<String>,

    /// Minimum cardinality (sh:minCount)
    /// If 1 or more, translates to NOT NULL
    pub min_count: Option<u32>,

    /// Maximum cardinality (sh:maxCount)
    /// If 1, may translate to UNIQUE constraint
    pub max_count: Option<u32>,

    /// Minimum string length (sh:minLength)
    pub min_length: Option<u32>,

    /// Maximum string length (sh:maxLength)
    /// Used for VARCHAR sizing
    pub max_length: Option<u32>,

    /// Regex pattern (sh:pattern)
    /// Translates to CHECK constraint
    pub pattern: Option<String>,

    /// Minimum numeric value (sh:minInclusive)
    pub min_inclusive: Option<f64>,

    /// Maximum numeric value (sh:maxInclusive)
    pub max_inclusive: Option<f64>,

    /// Minimum numeric value - exclusive (sh:minExclusive)
    pub min_exclusive: Option<f64>,

    /// Maximum numeric value - exclusive (sh:maxExclusive)
    pub max_exclusive: Option<f64>,

    /// Node kind constraint (sh:nodeKind)
    pub node_kind: Option<NodeKind>,

    /// Class constraint (sh:class)
    /// Used for foreign key relationships
    pub class: Option<String>,

    /// Enumeration constraint (sh:in)
    /// Translates to ENUM or CHECK constraint with IN clause
    pub in_values: Option<Vec<String>>,

    /// Fixed value constraint (sh:hasValue)
    /// Translates to DEFAULT or CHECK constraint
    pub has_value: Option<String>,

    /// Equality constraint (sh:equals)
    /// Property must equal another property
    pub equals: Option<String>,

    /// Less than constraint (sh:lessThan)
    /// Property must be less than another property
    pub less_than: Option<String>,

    /// Less than or equals constraint (sh:lessThanOrEquals)
    /// Property must be less than or equal to another property
    pub less_than_or_equals: Option<String>,

    /// Disjoint constraint (sh:disjoint)
    /// Property must not equal another property
    pub disjoint: Option<String>,

    /// Regex flags (sh:flags)
    /// Modifiers for pattern constraint (i for case-insensitive, etc.)
    pub pattern_flags: Option<String>,

    /// Default value (sh:defaultValue)
    pub default_value: Option<String>,

    /// Property description
    pub description: Option<String>,
}

/// Node kind enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum NodeKind {
    /// sh:IRI
    IRI,

    /// sh:BlankNode
    BlankNode,

    /// sh:Literal
    Literal,

    /// sh:BlankNodeOrIRI
    BlankNodeOrIRI,

    /// sh:BlankNodeOrLiteral
    BlankNodeOrLiteral,

    /// sh:IRIOrLiteral
    IRIOrLiteral,
}

/// Severity level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SeverityLevel {
    /// sh:Info
    Info,

    /// sh:Warning
    Warning,

    /// sh:Violation
    Violation,
}

impl NodeShape {
    /// Create a new node shape
    pub fn new(uri: String, target_class: String) -> Self {
        Self {
            uri,
            target_class,
            label: None,
            properties: Vec::new(),
            closed: false,
            severity: None,
        }
    }

    /// Add a property shape
    pub fn add_property(&mut self, property: PropertyShape) {
        self.properties.push(property);
    }

    /// Get suggested table name from target class
    pub fn get_table_name(&self) -> String {
        // Extract local name from URI
        self.target_class
            .split(['/', '#'])
            .last()
            .unwrap_or("UNKNOWN")
            .to_uppercase()
    }

    /// Get primary key properties (properties with maxCount=1 and high cardinality)
    pub fn get_primary_key_properties(&self) -> Vec<&PropertyShape> {
        self.properties
            .iter()
            .filter(|p| {
                p.max_count == Some(1)
                    && p.min_count == Some(1)
                    && p.name
                        .as_ref()
                        .map(|n| n.to_lowercase().contains("id"))
                        .unwrap_or(false)
            })
            .collect()
    }

    /// Get foreign key properties (properties with sh:class constraint)
    pub fn get_foreign_key_properties(&self) -> Vec<&PropertyShape> {
        self.properties
            .iter()
            .filter(|p| p.class.is_some())
            .collect()
    }
}

impl PropertyShape {
    /// Create a new property shape
    pub fn new(path: String) -> Self {
        Self {
            path,
            name: None,
            datatype: None,
            min_count: None,
            max_count: None,
            min_length: None,
            max_length: None,
            pattern: None,
            min_inclusive: None,
            max_inclusive: None,
            min_exclusive: None,
            max_exclusive: None,
            node_kind: None,
            class: None,
            in_values: None,
            has_value: None,
            equals: None,
            less_than: None,
            less_than_or_equals: None,
            disjoint: None,
            pattern_flags: None,
            default_value: None,
            description: None,
        }
    }

    /// Check if property is required (NOT NULL)
    pub fn is_required(&self) -> bool {
        self.min_count.unwrap_or(0) > 0
    }

    /// Check if property should be unique
    pub fn is_unique(&self) -> bool {
        self.max_count == Some(1) && self.is_required()
    }

    /// Get column name (derived from property path)
    pub fn get_column_name(&self) -> String {
        if let Some(name) = &self.name {
            return name.clone();
        }

        // Extract local name from property URI
        self.path
            .split(['/', '#'])
            .last()
            .unwrap_or("unknown")
            .to_lowercase()
    }

    /// Get SQL type hint based on constraints
    pub fn get_type_hint(&self) -> Option<&str> {
        if let Some(datatype) = &self.datatype {
            return Some(datatype.as_str());
        }

        // Infer from constraints
        if self.pattern.is_some() {
            return Some("xsd:string");
        }

        if self.min_inclusive.is_some() || self.max_inclusive.is_some() {
            return Some("xsd:decimal");
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_shape_creation() {
        let shape = NodeShape::new(
            "http://example.com/shape/Customer".to_string(),
            "http://example.com/Customer".to_string(),
        );

        assert_eq!(shape.uri, "http://example.com/shape/Customer");
        assert_eq!(shape.target_class, "http://example.com/Customer");
        assert!(shape.properties.is_empty());
        assert!(!shape.closed);
    }

    #[test]
    fn test_table_name_derivation() {
        let shape = NodeShape::new(
            "http://example.com/shape/Customer".to_string(),
            "http://example.com/Customer".to_string(),
        );

        assert_eq!(shape.get_table_name(), "CUSTOMER");
    }

    #[test]
    fn test_property_shape_required() {
        let mut prop = PropertyShape::new("http://schema.org/email".to_string());
        assert!(!prop.is_required());

        prop.min_count = Some(1);
        assert!(prop.is_required());
    }

    #[test]
    fn test_property_shape_unique() {
        let mut prop = PropertyShape::new("http://schema.org/id".to_string());
        prop.min_count = Some(1);
        prop.max_count = Some(1);

        assert!(prop.is_unique());
    }

    #[test]
    fn test_column_name_derivation() {
        let prop = PropertyShape::new("http://schema.org/emailAddress".to_string());
        assert_eq!(prop.get_column_name(), "emailaddress");
    }

    #[test]
    fn test_property_with_explicit_name() {
        let mut prop = PropertyShape::new("http://schema.org/email".to_string());
        prop.name = Some("email_address".to_string());

        assert_eq!(prop.get_column_name(), "email_address");
    }
}
