//! SubjectMap Type
//!
//! Defines how to generate RDF subjects (URIs or blank nodes).

use anyhow::Result;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// SubjectMap (rr:subjectMap)
///
/// Defines how to generate RDF subjects from source data.
///
/// ## W3C R2RML Spec
///
/// A subject map can generate subjects using:
/// - **Template**: URI template with placeholders (e.g., `http://example.com/customer/{id}`)
/// - **Column**: Direct column value as URI
/// - **Constant**: Fixed URI for all rows
///
/// ## Example
///
/// ```turtle
/// rr:subjectMap [
///     rr:template "http://example.com/customer/{customer_id}" ;
///     rr:class schema:Person ;
///     rr:termType rr:IRI ;
/// ] .
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SubjectMap {
    /// URI template with placeholders (e.g., "http://example.com/customer/{id}")
    pub template: Option<String>,

    /// Column name to use as URI
    pub column: Option<String>,

    /// Constant URI for all rows
    pub constant: Option<String>,

    /// Term type (IRI or BlankNode)
    #[serde(default)]
    pub term_type: TermType,

    /// RDF classes for the subject (rdf:type statements)
    pub class: Option<Vec<String>>,
}

impl SubjectMap {
    /// Create a new subject map with a template
    pub fn from_template(template: String) -> Self {
        Self {
            template: Some(template),
            column: None,
            constant: None,
            term_type: TermType::IRI,
            class: None,
        }
    }

    /// Create a new subject map from a column
    pub fn from_column(column: String) -> Self {
        Self {
            template: None,
            column: Some(column),
            constant: None,
            term_type: TermType::IRI,
            class: None,
        }
    }

    /// Create a new subject map with a constant URI
    pub fn from_constant(constant: String) -> Self {
        Self {
            template: None,
            column: None,
            constant: Some(constant),
            term_type: TermType::IRI,
            class: None,
        }
    }

    /// Add an RDF class
    pub fn with_class(mut self, class: String) -> Self {
        self.class.get_or_insert_with(Vec::new).push(class);
        self
    }

    /// Add multiple RDF classes
    pub fn with_classes(mut self, classes: Vec<String>) -> Self {
        self.class = Some(classes);
        self
    }

    /// Set term type (IRI or BlankNode)
    pub fn with_term_type(mut self, term_type: TermType) -> Self {
        self.term_type = term_type;
        self
    }

    /// Validate the subject map
    pub fn validate(&self) -> Result<()> {
        // Exactly one of template, column, or constant must be set
        let set_count = [
            self.template.is_some(),
            self.column.is_some(),
            self.constant.is_some(),
        ]
        .iter()
        .filter(|&&x| x)
        .count();

        if set_count != 1 {
            anyhow::bail!("SubjectMap must have exactly one of: template, column, or constant");
        }

        // If template is set, validate it contains at least one placeholder
        if let Some(template) = &self.template {
            if !template.contains('{') || !template.contains('}') {
                anyhow::bail!("Template must contain at least one placeholder {{column}}");
            }
        }

        Ok(())
    }

    /// Generate a subject URI from a row of data
    ///
    /// ## Arguments
    /// - `row`: Map of column names to values
    ///
    /// ## Returns
    /// Generated subject URI
    pub fn generate_subject(
        &self,
        row: &std::collections::HashMap<String, String>,
    ) -> Result<String> {
        match (&self.template, &self.column, &self.constant) {
            (Some(template), None, None) => {
                // Template-based generation
                let mut subject = template.clone();
                for (key, value) in row {
                    let placeholder = format!("{{{}}}", key);
                    subject = subject.replace(&placeholder, value);
                }
                Ok(subject)
            }
            (None, Some(column), None) => {
                // Column-based generation
                row.get(column)
                    .map(|v| v.clone())
                    .ok_or_else(|| anyhow::anyhow!("Column not found: {}", column))
            }
            (None, None, Some(constant)) => {
                // Constant URI
                Ok(constant.clone())
            }
            _ => anyhow::bail!(
                "Invalid SubjectMap: exactly one of template, column, or constant must be set"
            ),
        }
    }
}

/// TermType (rr:termType)
///
/// Specifies the type of RDF term to generate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub enum TermType {
    /// IRI (default)
    #[serde(rename = "IRI")]
    IRI,
    /// Blank node
    #[serde(rename = "BlankNode")]
    BlankNode,
    /// Literal (for object maps only, not subject maps)
    #[serde(rename = "Literal")]
    Literal,
}

impl Default for TermType {
    fn default() -> Self {
        TermType::IRI
    }
}

impl TermType {
    /// Get the R2RML URI for this term type
    pub fn to_r2rml_uri(&self) -> &'static str {
        match self {
            TermType::IRI => "http://www.w3.org/ns/r2rml#IRI",
            TermType::BlankNode => "http://www.w3.org/ns/r2rml#BlankNode",
            TermType::Literal => "http://www.w3.org/ns/r2rml#Literal",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_subject_map_from_template() {
        let sm = SubjectMap::from_template("http://example.com/customer/{id}".to_string())
            .with_class("schema:Person".to_string());

        assert!(sm.template.is_some());
        assert_eq!(sm.term_type, TermType::IRI);
        assert_eq!(sm.class.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn test_subject_map_validation() {
        // Valid template
        let sm = SubjectMap::from_template("http://example.com/customer/{id}".to_string());
        assert!(sm.validate().is_ok());

        // Invalid - no placeholder
        let sm = SubjectMap::from_template("http://example.com/customer/".to_string());
        assert!(sm.validate().is_err());

        // Invalid - multiple sources
        let sm = SubjectMap {
            template: Some("http://example.com/customer/{id}".to_string()),
            column: Some("id".to_string()),
            constant: None,
            term_type: TermType::IRI,
            class: None,
        };
        assert!(sm.validate().is_err());
    }

    #[test]
    fn test_subject_generation() {
        let sm = SubjectMap::from_template("http://example.com/customer/{id}".to_string());

        let mut row = HashMap::new();
        row.insert("id".to_string(), "123".to_string());
        row.insert("name".to_string(), "Alice".to_string());

        let subject = sm.generate_subject(&row).unwrap();
        assert_eq!(subject, "http://example.com/customer/123");
    }

    #[test]
    fn test_subject_generation_from_column() {
        let sm = SubjectMap::from_column("customer_uri".to_string());

        let mut row = HashMap::new();
        row.insert(
            "customer_uri".to_string(),
            "http://example.com/customer/456".to_string(),
        );

        let subject = sm.generate_subject(&row).unwrap();
        assert_eq!(subject, "http://example.com/customer/456");
    }

    #[test]
    fn test_subject_generation_from_constant() {
        let sm = SubjectMap::from_constant("http://example.com/dataset".to_string());

        let row = HashMap::new();
        let subject = sm.generate_subject(&row).unwrap();
        assert_eq!(subject, "http://example.com/dataset");
    }
}
