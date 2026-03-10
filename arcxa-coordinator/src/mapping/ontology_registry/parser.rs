//! # RDF Turtle Parser for Ontology Terms
//!
//! Low-level parsing utilities for extracting ontology terms from Turtle RDF content.
//!
//! ## Responsibilities
//!
//! - Parse Turtle format RDF to extract class and property definitions
//! - Extract metadata: labels, comments, parent classes, domains, ranges
//! - Convert RDF structures to `OntologyTerm` format for field matching
//!
//! ## Implementation Note
//!
//! This is a simple line-based parser optimized for the subset of Turtle
//! commonly used in ontology definitions. For full Turtle/RDF support,
//! consider integrating a proper RDF library like `rio_turtle`.

use crate::mapping::types::OntologyTerm;
use anyhow::Result;

/// Turtle RDF parser for ontology terms
pub struct TurtleParser;

impl TurtleParser {
    /// Parse ontology terms from Turtle RDF content
    ///
    /// Extracts classes and properties with their metadata:
    /// - `rdfs:Class` / `owl:Class` -> OntologyTerm
    /// - `rdfs:label` -> label
    /// - `rdfs:comment` -> description
    /// - `rdfs:subClassOf` -> parent_classes
    /// - Custom property annotations -> aliases, patterns, etc.
    ///
    /// # Arguments
    ///
    /// * `content` - Turtle RDF content as a string
    /// * `namespace` - Default namespace for expanding prefixed URIs
    ///
    /// # Returns
    ///
    /// Vector of extracted ontology terms
    pub fn parse(content: &str, namespace: &str) -> Result<Vec<OntologyTerm>> {
        let mut terms = Vec::new();

        // Simple Turtle parser - extract classes and properties
        // Format: <uri> a rdfs:Class ; rdfs:label "Label" ; rdfs:comment "Description" .

        let lines: Vec<&str> = content.lines().collect();
        let mut i = 0;

        while i < lines.len() {
            let line = lines[i].trim();

            // Skip comments and empty lines
            if line.starts_with('#') || line.is_empty() {
                i += 1;
                continue;
            }

            // Look for class or property declarations
            if line.contains("a rdfs:Class")
                || line.contains("a owl:Class")
                || line.contains("a rdf:Property")
                || line.contains("a owl:ObjectProperty")
                || line.contains("a owl:DatatypeProperty")
            {
                // Extract URI (before "a")
                let uri = if let Some(uri_end) = line.find(" a ") {
                    let uri_part = line[..uri_end].trim();

                    // Handle <http://...> or prefix:name format
                    if uri_part.starts_with('<') && uri_part.contains('>') {
                        let start = uri_part.find('<').unwrap() + 1;
                        let end = uri_part.find('>').unwrap();
                        uri_part[start..end].to_string()
                    } else {
                        // Prefix format - expand to full URI
                        if let Some(colon) = uri_part.find(':') {
                            let local_name = &uri_part[colon + 1..];
                            format!("{}{}", namespace, local_name)
                        } else {
                            i += 1;
                            continue;
                        }
                    }
                } else {
                    i += 1;
                    continue;
                };

                // For extraction, we need to find the original format in content (may be prefixed)
                // Extract the line that declared this resource to get its original format
                let uri_in_content = Self::find_resource_declaration(content, &uri, namespace)
                    .unwrap_or(uri.clone());

                // Extract label (rdfs:label)
                let label = Self::extract_literal(content, &uri_in_content, "rdfs:label")
                    .or_else(|| Self::extract_local_name(&uri));

                // Extract description (rdfs:comment)
                let description = Self::extract_literal(content, &uri_in_content, "rdfs:comment");

                // Extract parent classes (rdfs:subClassOf)
                let parent_classes =
                    Self::extract_references(content, &uri_in_content, "rdfs:subClassOf");

                // Extract aliases (skos:altLabel or custom graphica:alias)
                let mut aliases =
                    Self::extract_literals_multi(content, &uri_in_content, "skos:altLabel");
                aliases.extend(Self::extract_literals_multi(
                    content,
                    &uri_in_content,
                    "graphica:alias",
                ));

                // Extract examples (skos:example or custom graphica:example)
                let mut examples =
                    Self::extract_literals_multi(content, &uri_in_content, "skos:example");
                examples.extend(Self::extract_literals_multi(
                    content,
                    &uri_in_content,
                    "graphica:example",
                ));

                // Extract data type from rdfs:range
                let data_type = Self::extract_reference(content, &uri_in_content, "rdfs:range")
                    .and_then(|range_uri| Self::map_xsd_to_sql_type(&range_uri));

                // Extract value patterns (custom graphica:pattern)
                let value_patterns =
                    Self::extract_literals_multi(content, &uri_in_content, "graphica:pattern");

                // Create ontology term
                let term = OntologyTerm {
                    uri: uri.clone(),
                    label: label.unwrap_or_else(|| uri.clone()),
                    description,
                    parent_classes,
                    aliases,
                    examples,
                    data_type,
                    value_patterns,
                };

                terms.push(term);
            }

            i += 1;
        }

        Ok(terms)
    }

    /// Extract a literal value from Turtle content
    ///
    /// Searches for the property in lines following the resource declaration,
    /// then extracts the string value between quotes.
    ///
    /// # Arguments
    ///
    /// * `content` - Full Turtle content
    /// * `uri` - URI of the resource
    /// * `property` - Property name (e.g., "rdfs:label")
    pub fn extract_literal(content: &str, uri: &str, property: &str) -> Option<String> {
        let lines: Vec<&str> = content.lines().collect();

        // Find the line containing the resource URI
        let mut in_resource_block = false;
        for line in lines {
            let trimmed = line.trim();

            // Check if we're starting a new resource block
            if trimmed.contains(uri) && trimmed.contains(" a ") {
                in_resource_block = true;
                continue;
            }

            // If we're in the resource block, look for the property
            if in_resource_block {
                if trimmed.contains(property) {
                    // Extract string between quotes
                    if let Some(start) = trimmed.find('"') {
                        if let Some(end) = trimmed[start + 1..].find('"') {
                            return Some(trimmed[start + 1..start + 1 + end].to_string());
                        }
                    }
                }

                // End of resource block (indicated by . at end of line)
                if trimmed.ends_with('.') && !trimmed.contains(';') {
                    in_resource_block = false;
                }
            }
        }
        None
    }

    /// Find the original resource declaration format in content
    ///
    /// This finds how the resource URI appears in the Turtle content
    /// (may be prefixed like "ex:Person" or full URI like "<http://example.com#Person>")
    ///
    /// # Arguments
    ///
    /// * `content` - Full Turtle content
    /// * `expanded_uri` - The fully expanded URI
    /// * `namespace` - The namespace used for prefix expansion
    fn find_resource_declaration(
        content: &str,
        expanded_uri: &str,
        namespace: &str,
    ) -> Option<String> {
        // Extract the local name from the expanded URI
        let local_name = Self::extract_local_name(expanded_uri)?;

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.contains(" a ") {
                // Check if line contains the expanded URI in angle brackets
                if trimmed.contains(&format!("<{}>", expanded_uri)) {
                    return Some(format!("<{}>", expanded_uri));
                }

                // Check if line uses a prefix format
                if let Some(before_a) = trimmed.split(" a ").next() {
                    let resource_ref = before_a.trim();
                    // Check if this prefix:name expands to our target URI
                    if resource_ref.contains(':') && !resource_ref.starts_with('<') {
                        // Extract the local part after the colon
                        if let Some(colon_idx) = resource_ref.find(':') {
                            let ref_local = &resource_ref[colon_idx + 1..];
                            // Match if the local names are the same
                            if ref_local == local_name {
                                return Some(resource_ref.to_string());
                            }
                        }
                    }
                }
            }
        }
        None
    }

    /// Extract local name from URI (last part after # or /)
    ///
    /// Useful for generating default labels from URIs when rdfs:label is not present.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let local_name = TurtleParser::extract_local_name("http://schema.org/Person");
    /// assert_eq!(local_name, Some("Person".to_string()));
    /// ```
    pub fn extract_local_name(uri: &str) -> Option<String> {
        crate::mapping::uri_utils::extract_local_name(uri)
    }

    /// Extract multiple literal values for a property
    ///
    /// Used for properties that can have multiple values like aliases or examples.
    ///
    /// # Arguments
    ///
    /// * `content` - Full Turtle content
    /// * `uri` - URI of the resource
    /// * `property` - Property name (e.g., "skos:altLabel")
    pub fn extract_literals_multi(content: &str, uri: &str, property: &str) -> Vec<String> {
        let mut values = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        // Find the resource block
        let mut in_resource_block = false;
        for line in lines {
            let trimmed = line.trim();

            // Check if we're starting a new resource block
            if trimmed.contains(uri) && trimmed.contains(" a ") {
                in_resource_block = true;
                continue;
            }

            // If we're in the resource block, look for the property
            if in_resource_block {
                if trimmed.contains(property) {
                    // Extract all quoted strings in this line
                    let mut remaining = trimmed;
                    while let Some(start) = remaining.find('"') {
                        remaining = &remaining[start + 1..];
                        if let Some(end) = remaining.find('"') {
                            values.push(remaining[..end].to_string());
                            remaining = &remaining[end + 1..];
                        } else {
                            break;
                        }
                    }
                }

                // End of resource block
                if trimmed.ends_with('.') && !trimmed.contains(';') {
                    in_resource_block = false;
                }
            }
        }

        values
    }

    /// Extract a single URI reference for a property
    ///
    /// Used for properties that reference other resources like rdfs:range.
    ///
    /// # Arguments
    ///
    /// * `content` - Full Turtle content
    /// * `uri` - URI of the resource
    /// * `property` - Property name (e.g., "rdfs:range")
    pub fn extract_reference(content: &str, uri: &str, property: &str) -> Option<String> {
        let lines: Vec<&str> = content.lines().collect();

        // Find the resource block
        let mut in_resource_block = false;
        for line in lines {
            let trimmed = line.trim();

            // Check if we're starting a new resource block
            if trimmed.contains(uri) && trimmed.contains(" a ") {
                in_resource_block = true;
                continue;
            }

            // If we're in the resource block, look for the property
            if in_resource_block {
                if trimmed.contains(property) {
                    // Look for <uri> pattern or prefix:name pattern after the property
                    if let Some(prop_idx) = trimmed.find(property) {
                        let after_property = &trimmed[prop_idx + property.len()..];

                        // Try to extract <http://...> format
                        if let Some(start) = after_property.find('<') {
                            if let Some(end) = after_property[start + 1..].find('>') {
                                return Some(
                                    after_property[start + 1..start + 1 + end].to_string(),
                                );
                            }
                        }

                        // Try to extract prefix:name format
                        let trimmed_prop = after_property.trim();
                        if let Some(word_end) =
                            trimmed_prop.find(|c: char| c.is_whitespace() || c == ';' || c == '.')
                        {
                            let reference = &trimmed_prop[..word_end];
                            if reference.contains(':') && !reference.is_empty() {
                                return Some(reference.to_string());
                            }
                        }
                    }
                }

                // End of resource block
                if trimmed.ends_with('.') && !trimmed.contains(';') {
                    in_resource_block = false;
                }
            }
        }
        None
    }

    /// Extract multiple URI references for a property
    ///
    /// Used for properties that can have multiple resource references like rdfs:subClassOf.
    ///
    /// # Arguments
    ///
    /// * `content` - Full Turtle content
    /// * `uri` - URI of the resource
    /// * `property` - Property name (e.g., "rdfs:subClassOf")
    pub fn extract_references(content: &str, uri: &str, property: &str) -> Vec<String> {
        let mut references = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        // Find the resource block
        let mut in_resource_block = false;
        for line in lines {
            let trimmed = line.trim();

            // Check if we're starting a new resource block
            if trimmed.contains(uri) && trimmed.contains(" a ") {
                in_resource_block = true;
                continue;
            }

            // If we're in the resource block, look for the property
            if in_resource_block {
                if trimmed.contains(property) {
                    // Look for <uri> pattern or prefix:name pattern after the property
                    if let Some(prop_idx) = trimmed.find(property) {
                        let after_property = &trimmed[prop_idx + property.len()..];

                        // Try to extract <http://...> format
                        if let Some(start) = after_property.find('<') {
                            if let Some(end) = after_property[start + 1..].find('>') {
                                references
                                    .push(after_property[start + 1..start + 1 + end].to_string());
                                continue;
                            }
                        }

                        // Try to extract prefix:name format
                        let trimmed_prop = after_property.trim();
                        if let Some(word_end) =
                            trimmed_prop.find(|c: char| c.is_whitespace() || c == ';' || c == '.')
                        {
                            let reference = &trimmed_prop[..word_end];
                            if reference.contains(':') && !reference.is_empty() {
                                references.push(reference.to_string());
                            }
                        }
                    }
                }

                // End of resource block
                if trimmed.ends_with('.') && !trimmed.contains(';') {
                    in_resource_block = false;
                }
            }
        }

        references
    }

    /// Map XSD data types to SQL types
    ///
    /// Converts XSD type URIs to standard SQL type names.
    /// Handles both full URIs and prefixed formats.
    ///
    /// # Arguments
    ///
    /// * `xsd_type` - XSD type URI (e.g., "http://www.w3.org/2001/XMLSchema#string" or "xsd:string")
    pub fn map_xsd_to_sql_type(xsd_type: &str) -> Option<String> {
        // Handle prefixed format (e.g., "xsd:string")
        let type_name = if xsd_type.contains(':') && !xsd_type.starts_with("http") {
            // Extract the part after the colon
            xsd_type.split(':').last()?
        } else {
            // Extract from full URI
            xsd_type.rsplit(['#', '/']).next()?
        };

        let sql_type = match type_name {
            "string" | "normalizedString" | "token" => "VARCHAR",
            "integer" | "int" | "long" | "short" | "byte" => "INTEGER",
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
            TurtleParser::extract_local_name("http://schema.org/Person"),
            Some("Person".to_string())
        );
        assert_eq!(
            TurtleParser::extract_local_name("http://example.com#Product"),
            Some("Product".to_string())
        );
        assert_eq!(
            TurtleParser::extract_local_name("http://example.com/"),
            None
        );
    }

    #[test]
    fn test_extract_literal() {
        let content = r#"
            <http://example.com/Person> a rdfs:Class ;
                rdfs:label "Person" ;
                rdfs:comment "A human being" .
        "#;

        assert_eq!(
            TurtleParser::extract_literal(content, "http://example.com/Person", "rdfs:label"),
            Some("Person".to_string())
        );
        assert_eq!(
            TurtleParser::extract_literal(content, "http://example.com/Person", "rdfs:comment"),
            Some("A human being".to_string())
        );
    }

    #[test]
    fn test_parse_simple_ontology() {
        let content = r#"
@prefix ex: <http://example.com#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .

ex:Person a rdfs:Class ;
    rdfs:label "Person" ;
    rdfs:comment "A human being" .

ex:Product a rdfs:Class ;
    rdfs:label "Product" .
        "#;

        let terms = TurtleParser::parse(content, "http://example.com#").unwrap();

        assert_eq!(terms.len(), 2);
        assert_eq!(terms[0].label, "Person");
        assert_eq!(terms[0].description, Some("A human being".to_string()));
        assert_eq!(terms[1].label, "Product");
    }

    #[test]
    fn test_extract_literals_multi() {
        let content = r#"
            <http://example.com/email> a rdf:Property ;
                skos:altLabel "email" ;
                skos:altLabel "e-mail" ;
                graphica:alias "emailAddress" .
        "#;

        let aliases = TurtleParser::extract_literals_multi(
            content,
            "http://example.com/email",
            "skos:altLabel",
        );

        assert_eq!(aliases.len(), 2);
        assert!(aliases.contains(&"email".to_string()));
        assert!(aliases.contains(&"e-mail".to_string()));
    }

    #[test]
    fn test_extract_reference() {
        let content = r#"
            <http://example.com/name> a rdf:Property ;
                rdfs:range <http://www.w3.org/2001/XMLSchema#string> .
        "#;

        let range =
            TurtleParser::extract_reference(content, "http://example.com/name", "rdfs:range");

        assert_eq!(
            range,
            Some("http://www.w3.org/2001/XMLSchema#string".to_string())
        );
    }

    #[test]
    fn test_extract_references() {
        let content = r#"
            <http://example.com/Employee> a rdfs:Class ;
                rdfs:subClassOf <http://example.com/Person> .
        "#;

        let parent_classes = TurtleParser::extract_references(
            content,
            "http://example.com/Employee",
            "rdfs:subClassOf",
        );

        assert_eq!(parent_classes.len(), 1);
        assert_eq!(parent_classes[0], "http://example.com/Person");
    }

    #[test]
    fn test_map_xsd_to_sql_type() {
        assert_eq!(
            TurtleParser::map_xsd_to_sql_type("http://www.w3.org/2001/XMLSchema#string"),
            Some("VARCHAR".to_string())
        );
        assert_eq!(
            TurtleParser::map_xsd_to_sql_type("http://www.w3.org/2001/XMLSchema#integer"),
            Some("INTEGER".to_string())
        );
        assert_eq!(
            TurtleParser::map_xsd_to_sql_type("http://www.w3.org/2001/XMLSchema#dateTime"),
            Some("TIMESTAMP".to_string())
        );
        assert_eq!(
            TurtleParser::map_xsd_to_sql_type("http://www.w3.org/2001/XMLSchema#boolean"),
            Some("BOOLEAN".to_string())
        );
        assert_eq!(
            TurtleParser::map_xsd_to_sql_type("http://www.w3.org/2001/XMLSchema#unknownType"),
            None
        );
    }

    #[test]
    fn test_parse_complete_ontology_with_metadata() {
        let content = r#"
@prefix ex: <http://example.com#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
@prefix skos: <http://www.w3.org/2004/02/skos/core#> .
@prefix graphica: <http://graphica.io/ontology#> .

ex:email a rdf:Property ;
    rdfs:label "Email" ;
    rdfs:comment "Email address property" ;
    rdfs:range xsd:string ;
    skos:altLabel "email" ;
    skos:altLabel "e-mail" ;
    graphica:alias "emailAddress" ;
    graphica:example "user@example.com" ;
    graphica:pattern "^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\\.[a-zA-Z]{2,}$" .
        "#;

        let terms = TurtleParser::parse(content, "http://example.com#").unwrap();

        assert_eq!(terms.len(), 1);

        let email_term = &terms[0];
        assert_eq!(email_term.label, "Email");
        assert_eq!(
            email_term.description,
            Some("Email address property".to_string())
        );
        assert_eq!(email_term.data_type, Some("VARCHAR".to_string()));
        assert!(!email_term.aliases.is_empty());
        assert!(!email_term.examples.is_empty());
        assert!(!email_term.value_patterns.is_empty());
    }
}
