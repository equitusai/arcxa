//! # RDF/XML Parser for Ontology Terms
//!
//! Parser for extracting ontology terms from RDF/XML format content.
//!
//! ## Responsibilities
//!
//! - Parse RDF/XML format to extract class and property definitions
//! - Extract metadata: labels, comments, parent classes, domains, ranges
//! - Convert RDF/XML structures to `OntologyTerm` format for field matching
//!
//! ## Implementation Note
//!
//! This is a simple XML-based parser for the subset of RDF/XML commonly
//! used in ontology definitions. It handles owl:Class, rdfs:Class, and
//! various property types.

use crate::mapping::types::OntologyTerm;
use anyhow::Result;

/// RDF/XML parser for ontology terms
pub struct RdfXmlParser;

impl RdfXmlParser {
    /// Parse ontology terms from RDF/XML content
    ///
    /// Extracts classes and properties with their metadata:
    /// - `owl:Class` / `rdfs:Class` -> OntologyTerm
    /// - `rdfs:label` -> label
    /// - `rdfs:comment` -> description
    /// - `rdfs:subClassOf` -> parent_classes
    /// - Property annotations -> aliases, patterns, etc.
    ///
    /// # Arguments
    ///
    /// * `content` - RDF/XML content as a string
    /// * `namespace` - Default namespace for the ontology
    ///
    /// # Returns
    ///
    /// Vector of extracted ontology terms
    pub fn parse(content: &str, namespace: &str) -> Result<Vec<OntologyTerm>> {
        let mut terms = Vec::new();

        // Find all owl:Class and rdfs:Class definitions
        let class_terms = Self::extract_classes(content, namespace)?;
        terms.extend(class_terms);

        // Find all property definitions
        let property_terms = Self::extract_properties(content, namespace)?;
        terms.extend(property_terms);

        Ok(terms)
    }

    /// Extract class definitions from RDF/XML
    fn extract_classes(content: &str, namespace: &str) -> Result<Vec<OntologyTerm>> {
        let mut terms = Vec::new();

        // Pattern: <owl:Class rdf:about="URI">
        let class_pattern = regex::Regex::new(
            r#"<(?:owl|rdfs):Class\s+rdf:about="([^"]+)"[^>]*>([\s\S]*?)</(?:owl|rdfs):Class>"#,
        )?;

        for cap in class_pattern.captures_iter(content) {
            if let (Some(uri_match), Some(content_match)) = (cap.get(1), cap.get(2)) {
                let uri = uri_match.as_str().to_string();
                let class_content = content_match.as_str();

                // Extract label
                let label = Self::extract_element_text(class_content, "rdfs:label")
                    .or_else(|| Self::extract_local_name(&uri))
                    .unwrap_or_else(|| uri.clone());

                // Extract description
                let description = Self::extract_element_text(class_content, "rdfs:comment");

                // Extract parent classes
                let parent_classes = Self::extract_resources(class_content, "rdfs:subClassOf");

                // Create ontology term
                let term = OntologyTerm {
                    uri: uri.clone(),
                    label,
                    description,
                    parent_classes,
                    aliases: Vec::new(),
                    examples: Vec::new(),
                    data_type: None,
                    value_patterns: Vec::new(),
                };

                terms.push(term);
            }
        }

        Ok(terms)
    }

    /// Extract property definitions from RDF/XML
    fn extract_properties(content: &str, namespace: &str) -> Result<Vec<OntologyTerm>> {
        let mut terms = Vec::new();

        // Pattern for ObjectProperty and DatatypeProperty
        let property_pattern = regex::Regex::new(
            r#"<owl:(?:ObjectProperty|DatatypeProperty)\s+rdf:about="([^"]+)"[^>]*>([\s\S]*?)</owl:(?:ObjectProperty|DatatypeProperty)>"#,
        )?;

        for cap in property_pattern.captures_iter(content) {
            if let (Some(uri_match), Some(content_match)) = (cap.get(1), cap.get(2)) {
                let uri = uri_match.as_str().to_string();
                let prop_content = content_match.as_str();

                // Extract label
                let label = Self::extract_element_text(prop_content, "rdfs:label")
                    .or_else(|| Self::extract_local_name(&uri))
                    .unwrap_or_else(|| uri.clone());

                // Extract description
                let description = Self::extract_element_text(prop_content, "rdfs:comment");

                // Extract domain (used as context)
                let parent_classes = Self::extract_resources(prop_content, "rdfs:domain");

                // Extract range (for data type mapping)
                // For properties without a range or non-XSD range (ObjectProperties), use "OBJECT" as marker
                let range_resources = Self::extract_resources(prop_content, "rdfs:range");
                let data_type = if let Some(range_uri) = range_resources.first() {
                    Self::map_xsd_to_sql_type(range_uri).or(Some("OBJECT".to_string()))
                } else {
                    // Property without range - mark as OBJECT (typical for ObjectProperty)
                    Some("OBJECT".to_string())
                };

                // Create ontology term
                let term = OntologyTerm {
                    uri: uri.clone(),
                    label,
                    description,
                    parent_classes,
                    aliases: Vec::new(),
                    examples: Vec::new(),
                    data_type,
                    value_patterns: Vec::new(),
                };

                terms.push(term);
            }
        }

        Ok(terms)
    }

    /// Extract text content from an XML element
    fn extract_element_text(content: &str, element_name: &str) -> Option<String> {
        let pattern_str = format!(
            r#"<{}[^>]*>([^<]+)</{}>|<{}>([^<]+)</{}>|<{} xml:lang="[^"]*">([^<]+)</{}"#,
            element_name, element_name, element_name, element_name, element_name, element_name
        );

        if let Ok(pattern) = regex::Regex::new(&pattern_str) {
            if let Some(cap) = pattern.captures(content) {
                // Try each capture group (different XML formats)
                for i in 1..=3 {
                    if let Some(text) = cap.get(i) {
                        let trimmed = text.as_str().trim();
                        if !trimmed.is_empty() {
                            return Some(trimmed.to_string());
                        }
                    }
                }
            }
        }

        None
    }

    /// Extract resource URIs from elements (like rdfs:subClassOf)
    fn extract_resources(content: &str, element_name: &str) -> Vec<String> {
        let mut resources = Vec::new();

        // Pattern: <element rdf:resource="URI"/>
        let pattern_str = format!(r#"<{}\s+rdf:resource="([^"]+)""#, element_name);
        if let Ok(pattern) = regex::Regex::new(&pattern_str) {
            for cap in pattern.captures_iter(content) {
                if let Some(uri) = cap.get(1) {
                    resources.push(uri.as_str().to_string());
                }
            }
        }

        resources
    }

    /// Extract local name from URI (last part after # or /)
    pub fn extract_local_name(uri: &str) -> Option<String> {
        crate::mapping::uri_utils::extract_local_name(uri)
    }

    /// Map XSD data types to SQL types
    pub fn map_xsd_to_sql_type(xsd_type: &str) -> Option<String> {
        // Handle both full URIs and prefixed formats
        let type_name = if xsd_type.contains(':') && !xsd_type.starts_with("http") {
            xsd_type.split(':').last()?
        } else {
            xsd_type.rsplit(['#', '/']).next()?
        };

        let sql_type = match type_name {
            "string" | "normalizedString" | "token" => "VARCHAR",
            "integer" | "int" | "long" | "short" | "byte" | "nonNegativeInteger"
            | "positiveInteger" => "INTEGER",
            "decimal" | "double" | "float" => "DECIMAL",
            "boolean" => "BOOLEAN",
            "date" => "DATE",
            "dateTime" | "dateTimeStamp" => "TIMESTAMP",
            "time" => "TIME",
            "anyURI" => "VARCHAR",
            _ => return None,
        };

        Some(sql_type.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_local_name() {
        assert_eq!(
            RdfXmlParser::extract_local_name("http://example.com/ontology#Customer"),
            Some("Customer".to_string())
        );
        assert_eq!(
            RdfXmlParser::extract_local_name("http://example.com/ontology/Product"),
            Some("Product".to_string())
        );
    }

    #[test]
    fn test_map_xsd_to_sql_type() {
        assert_eq!(
            RdfXmlParser::map_xsd_to_sql_type("http://www.w3.org/2001/XMLSchema#string"),
            Some("VARCHAR".to_string())
        );
        assert_eq!(
            RdfXmlParser::map_xsd_to_sql_type("http://www.w3.org/2001/XMLSchema#integer"),
            Some("INTEGER".to_string())
        );
        assert_eq!(
            RdfXmlParser::map_xsd_to_sql_type("xsd:dateTime"),
            Some("TIMESTAMP".to_string())
        );
    }

    #[test]
    fn test_parse_simple_rdfxml_class() {
        let content = r#"
            <owl:Class rdf:about="http://example.com#Customer">
                <rdfs:label>Customer</rdfs:label>
                <rdfs:comment>A person who purchases products</rdfs:comment>
            </owl:Class>
        "#;

        let terms = RdfXmlParser::parse(content, "http://example.com#").unwrap();
        assert_eq!(terms.len(), 1);
        assert_eq!(terms[0].label, "Customer");
        assert_eq!(
            terms[0].description,
            Some("A person who purchases products".to_string())
        );
    }

    #[test]
    fn test_parse_rdfxml_with_subclass() {
        let content = r#"
            <owl:Class rdf:about="http://example.com#IndividualCustomer">
                <rdfs:label>Individual Customer</rdfs:label>
                <rdfs:subClassOf rdf:resource="http://example.com#Customer"/>
            </owl:Class>
        "#;

        let terms = RdfXmlParser::parse(content, "http://example.com#").unwrap();
        assert_eq!(terms.len(), 1);
        assert_eq!(terms[0].parent_classes.len(), 1);
        assert_eq!(terms[0].parent_classes[0], "http://example.com#Customer");
    }

    #[test]
    fn test_parse_rdfxml_property() {
        let content = r#"
            <owl:DatatypeProperty rdf:about="http://example.com#email">
                <rdfs:label>email</rdfs:label>
                <rdfs:range rdf:resource="http://www.w3.org/2001/XMLSchema#string"/>
            </owl:DatatypeProperty>
        "#;

        let terms = RdfXmlParser::parse(content, "http://example.com#").unwrap();
        assert_eq!(terms.len(), 1);
        assert_eq!(terms[0].label, "email");
        assert_eq!(terms[0].data_type, Some("VARCHAR".to_string()));
    }
}
