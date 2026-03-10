//! SHACL Parser
//!
//! Parse SHACL shapes from RDF triple store using SPARQL queries.

use super::types::{NodeKind, NodeShape, PropertyShape, SeverityLevel};
use anyhow::{Context, Result};
use std::collections::HashMap;
use tracing::warn;

/// SHACL Parser
///
/// Extracts SHACL shapes from RDF store using SPARQL queries.
pub struct ShaclParser;

impl ShaclParser {
    /// Create a new SHACL parser
    pub fn new() -> Self {
        Self
    }

    /// Parse a SHACL node shape from RDF using SPARQL
    ///
    /// # Arguments
    ///
    /// * `shape_uri` - URI of the SHACL node shape
    /// * `sparql_query_fn` - Function to execute SPARQL queries against RDF store
    ///
    /// # Returns
    ///
    /// The parsed `NodeShape` with all properties
    pub fn parse_node_shape<F>(&self, shape_uri: &str, sparql_query_fn: F) -> Result<NodeShape>
    where
        F: Fn(&str) -> Result<Vec<HashMap<String, String>>>,
    {
        // Query for node shape metadata
        let node_query = format!(
            r#"
            PREFIX sh: <http://www.w3.org/ns/shacl#>
            PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>

            SELECT ?targetClass ?label ?closed ?severity
            WHERE {{
                <{shape_uri}> a sh:NodeShape ;
                              sh:targetClass ?targetClass .
                OPTIONAL {{ <{shape_uri}> rdfs:label ?label }}
                OPTIONAL {{ <{shape_uri}> sh:closed ?closed }}
                OPTIONAL {{ <{shape_uri}> sh:severity ?severity }}
            }}
            "#,
            shape_uri = shape_uri
        );

        let node_results = sparql_query_fn(&node_query).context(format!(
            "Failed to query node shape metadata for shape: {}\nSPARQL query:\n{}",
            shape_uri, node_query
        ))?;

        if node_results.is_empty() {
            anyhow::bail!(
                "Node shape not found: {}\n\
                 Expected to find a sh:NodeShape with sh:targetClass.\n\
                 Verify that the shape exists in the RDF store and has the correct structure.",
                shape_uri
            );
        }

        let node_data = &node_results[0];
        let target_class = node_data
            .get("targetClass")
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Missing targetClass for node shape: {}\n\
                     A sh:NodeShape must have a sh:targetClass property.\n\
                     Found shape data: {:?}",
                    shape_uri,
                    node_data
                )
            })?
            .clone();

        let label = node_data.get("label").cloned();
        let closed = node_data
            .get("closed")
            .map(|s| s == "true")
            .unwrap_or(false);
        let severity = node_data.get("severity").and_then(|s| parse_severity(s));

        // Query for property shapes
        let props_query = format!(
            r#"
            PREFIX sh: <http://www.w3.org/ns/shacl#>
            PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>

            SELECT ?property ?path ?name ?datatype ?minCount ?maxCount
                   ?minLength ?maxLength ?pattern ?minInclusive ?maxInclusive
                   ?minExclusive ?maxExclusive
                   ?nodeKind ?class ?hasValue ?equals ?lessThan ?lessThanOrEquals
                   ?disjoint ?flags ?defaultValue ?description
            WHERE {{
                <{shape_uri}> sh:property ?property .
                ?property sh:path ?path .
                OPTIONAL {{ ?property sh:name ?name }}
                OPTIONAL {{ ?property sh:datatype ?datatype }}
                OPTIONAL {{ ?property sh:minCount ?minCount }}
                OPTIONAL {{ ?property sh:maxCount ?maxCount }}
                OPTIONAL {{ ?property sh:minLength ?minLength }}
                OPTIONAL {{ ?property sh:maxLength ?maxLength }}
                OPTIONAL {{ ?property sh:pattern ?pattern }}
                OPTIONAL {{ ?property sh:minInclusive ?minInclusive }}
                OPTIONAL {{ ?property sh:maxInclusive ?maxInclusive }}
                OPTIONAL {{ ?property sh:minExclusive ?minExclusive }}
                OPTIONAL {{ ?property sh:maxExclusive ?maxExclusive }}
                OPTIONAL {{ ?property sh:nodeKind ?nodeKind }}
                OPTIONAL {{ ?property sh:class ?class }}
                OPTIONAL {{ ?property sh:hasValue ?hasValue }}
                OPTIONAL {{ ?property sh:equals ?equals }}
                OPTIONAL {{ ?property sh:lessThan ?lessThan }}
                OPTIONAL {{ ?property sh:lessThanOrEquals ?lessThanOrEquals }}
                OPTIONAL {{ ?property sh:disjoint ?disjoint }}
                OPTIONAL {{ ?property sh:flags ?flags }}
                OPTIONAL {{ ?property sh:defaultValue ?defaultValue }}
                OPTIONAL {{ ?property sh:description ?description }}
            }}
            "#,
            shape_uri = shape_uri
        );

        let prop_results = sparql_query_fn(&props_query).context(format!(
            "Failed to query property shapes for node shape: {}\nSPARQL query:\n{}",
            shape_uri, props_query
        ))?;

        let mut properties = Vec::new();
        for (idx, prop_data) in prop_results.iter().enumerate() {
            let path = prop_data
                .get("path")
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "Missing path in property shape #{} for node shape: {}\n\
                         Every sh:PropertyShape must have a sh:path property.\n\
                         Found property data: {:?}",
                        idx + 1,
                        shape_uri,
                        prop_data
                    )
                })?
                .clone();

            let property_uri = prop_data.get("property").ok_or_else(|| {
                anyhow::anyhow!(
                    "Missing property URI for property shape #{} (path: {}) in node shape: {}\n\
                         The SPARQL query should return the property shape URI as ?property.\n\
                         Found property data: {:?}",
                    idx + 1,
                    path,
                    shape_uri,
                    prop_data
                )
            })?;

            // Query sh:in values separately (requires list traversal)
            let in_values = self.parse_in_constraint(property_uri, &sparql_query_fn)?;

            let property = PropertyShape {
                path,
                name: prop_data.get("name").cloned(),
                datatype: prop_data.get("datatype").cloned(),
                min_count: prop_data.get("minCount").and_then(|s| s.parse().ok()),
                max_count: prop_data.get("maxCount").and_then(|s| s.parse().ok()),
                min_length: prop_data.get("minLength").and_then(|s| s.parse().ok()),
                max_length: prop_data.get("maxLength").and_then(|s| s.parse().ok()),
                pattern: prop_data.get("pattern").cloned(),
                min_inclusive: prop_data.get("minInclusive").and_then(|s| s.parse().ok()),
                max_inclusive: prop_data.get("maxInclusive").and_then(|s| s.parse().ok()),
                min_exclusive: prop_data.get("minExclusive").and_then(|s| s.parse().ok()),
                max_exclusive: prop_data.get("maxExclusive").and_then(|s| s.parse().ok()),
                node_kind: prop_data.get("nodeKind").and_then(|s| parse_node_kind(s)),
                class: prop_data.get("class").cloned(),
                in_values,
                has_value: prop_data.get("hasValue").cloned(),
                equals: prop_data.get("equals").cloned(),
                less_than: prop_data.get("lessThan").cloned(),
                less_than_or_equals: prop_data.get("lessThanOrEquals").cloned(),
                disjoint: prop_data.get("disjoint").cloned(),
                pattern_flags: prop_data.get("flags").cloned(),
                default_value: prop_data.get("defaultValue").cloned(),
                description: prop_data.get("description").cloned(),
            };

            // Validate property constraints and log warnings
            let validation_warnings = self.validate_property_constraints(&property);
            for warning in validation_warnings {
                warn!(
                    shape_uri = %shape_uri,
                    property_path = %property.path,
                    "SHACL constraint validation warning: {}",
                    warning
                );
            }

            properties.push(property);
        }

        Ok(NodeShape {
            uri: shape_uri.to_string(),
            target_class,
            label,
            properties,
            closed,
            severity,
        })
    }

    /// Parse sh:in constraint (enumeration values)
    ///
    /// # Arguments
    ///
    /// * `property_uri` - URI of the property shape
    /// * `sparql_query_fn` - Function to execute SPARQL queries against RDF store
    ///
    /// # Returns
    ///
    /// Optional vector of allowed values
    fn parse_in_constraint<F>(
        &self,
        property_uri: &str,
        sparql_query_fn: F,
    ) -> Result<Option<Vec<String>>>
    where
        F: Fn(&str) -> Result<Vec<HashMap<String, String>>>,
    {
        // Query to traverse RDF list for sh:in values
        let in_query = format!(
            r#"
            PREFIX sh: <http://www.w3.org/ns/shacl#>
            PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>

            SELECT ?value
            WHERE {{
                <{property_uri}> sh:in ?list .
                ?list rdf:rest*/rdf:first ?value .
            }}
            ORDER BY ?value
            "#,
            property_uri = property_uri
        );

        let results = sparql_query_fn(&in_query).context(format!(
            "Failed to query sh:in constraint for property: {}\nSPARQL query:\n{}",
            property_uri, in_query
        ))?;

        if results.is_empty() {
            return Ok(None);
        }

        let values: Vec<String> = results
            .into_iter()
            .filter_map(|row| row.get("value").cloned())
            .collect();

        if values.is_empty() {
            Ok(None)
        } else {
            Ok(Some(values))
        }
    }

    /// Validate property shape for constraint conflicts
    ///
    /// Checks for common constraint conflicts and logs warnings.
    fn validate_property_constraints(&self, property: &PropertyShape) -> Vec<String> {
        let mut warnings = Vec::new();

        // Check for conflicting numeric bounds
        if let (Some(min_inc), Some(min_exc)) = (property.min_inclusive, property.min_exclusive) {
            warnings.push(format!(
                "Property '{}' has both sh:minInclusive ({}) and sh:minExclusive ({}). \
                 sh:minExclusive takes precedence.",
                property.get_column_name(),
                min_inc,
                min_exc
            ));
        }

        if let (Some(max_inc), Some(max_exc)) = (property.max_inclusive, property.max_exclusive) {
            warnings.push(format!(
                "Property '{}' has both sh:maxInclusive ({}) and sh:maxExclusive ({}). \
                 sh:maxExclusive takes precedence.",
                property.get_column_name(),
                max_inc,
                max_exc
            ));
        }

        // Check for impossible numeric ranges
        if let (Some(min), Some(max)) = (
            property.min_exclusive.or(property.min_inclusive),
            property.max_exclusive.or(property.max_inclusive),
        ) {
            if min >= max {
                warnings.push(format!(
                    "Property '{}' has invalid numeric range: min ({}) >= max ({})",
                    property.get_column_name(),
                    min,
                    max
                ));
            }
        }

        // Check for sh:in with numeric bounds (unusual combination)
        if property.in_values.is_some()
            && (property.min_inclusive.is_some()
                || property.max_inclusive.is_some()
                || property.min_exclusive.is_some()
                || property.max_exclusive.is_some())
        {
            warnings.push(format!(
                "Property '{}' has both sh:in (enumeration) and numeric bounds. \
                 This is unusual - sh:in already restricts the allowed values.",
                property.get_column_name()
            ));
        }

        // Check for sh:hasValue with sh:in (redundant)
        if property.has_value.is_some() && property.in_values.is_some() {
            warnings.push(format!(
                "Property '{}' has both sh:hasValue and sh:in. \
                 sh:hasValue forces a single value, making sh:in redundant.",
                property.get_column_name()
            ));
        }

        // Check for string constraints on non-string datatypes
        if let Some(datatype) = &property.datatype {
            if !datatype.contains("string")
                && !datatype.contains("String")
                && (property.min_length.is_some()
                    || property.max_length.is_some()
                    || property.pattern.is_some())
            {
                warnings.push(format!(
                    "Property '{}' has string constraints (minLength/maxLength/pattern) \
                     but datatype is '{}' (not a string type)",
                    property.get_column_name(),
                    datatype
                ));
            }
        }

        warnings
    }

    /// List all SHACL node shapes in the RDF store
    ///
    /// # Arguments
    ///
    /// * `sparql_query_fn` - Function to execute SPARQL queries against RDF store
    ///
    /// # Returns
    ///
    /// Vector of shape URIs
    pub fn list_node_shapes<F>(&self, sparql_query_fn: F) -> Result<Vec<String>>
    where
        F: Fn(&str) -> Result<Vec<HashMap<String, String>>>,
    {
        let query = r#"
            PREFIX sh: <http://www.w3.org/ns/shacl#>

            SELECT DISTINCT ?shape
            WHERE {
                ?shape a sh:NodeShape ;
                       sh:targetClass ?targetClass .
            }
            ORDER BY ?shape
        "#;

        let results = sparql_query_fn(query).context("Failed to list node shapes")?;

        Ok(results
            .into_iter()
            .filter_map(|row| row.get("shape").cloned())
            .collect())
    }
}

impl Default for ShaclParser {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse SHACL severity level from URI
fn parse_severity(uri: &str) -> Option<SeverityLevel> {
    match uri {
        "http://www.w3.org/ns/shacl#Info" => Some(SeverityLevel::Info),
        "http://www.w3.org/ns/shacl#Warning" => Some(SeverityLevel::Warning),
        "http://www.w3.org/ns/shacl#Violation" => Some(SeverityLevel::Violation),
        _ => None,
    }
}

/// Parse SHACL node kind from URI
fn parse_node_kind(uri: &str) -> Option<NodeKind> {
    match uri {
        "http://www.w3.org/ns/shacl#IRI" => Some(NodeKind::IRI),
        "http://www.w3.org/ns/shacl#BlankNode" => Some(NodeKind::BlankNode),
        "http://www.w3.org/ns/shacl#Literal" => Some(NodeKind::Literal),
        "http://www.w3.org/ns/shacl#BlankNodeOrIRI" => Some(NodeKind::BlankNodeOrIRI),
        "http://www.w3.org/ns/shacl#BlankNodeOrLiteral" => Some(NodeKind::BlankNodeOrLiteral),
        "http://www.w3.org/ns/shacl#IRIOrLiteral" => Some(NodeKind::IRIOrLiteral),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mock SPARQL query function for testing
    fn mock_sparql_customer_shape(query: &str) -> Result<Vec<HashMap<String, String>>> {
        if query.contains("SELECT ?targetClass") {
            // Node shape query
            let mut result = HashMap::new();
            result.insert(
                "targetClass".to_string(),
                "http://example.com/Customer".to_string(),
            );
            result.insert("label".to_string(), "Customer Shape".to_string());
            result.insert("closed".to_string(), "false".to_string());
            Ok(vec![result])
        } else if query.contains("SELECT ?property") {
            // Property shapes query
            let mut prop1 = HashMap::new();
            prop1.insert(
                "property".to_string(),
                "http://example.com/shape/Customer/prop/email".to_string(),
            );
            prop1.insert("path".to_string(), "http://schema.org/email".to_string());
            prop1.insert("name".to_string(), "email".to_string());
            prop1.insert(
                "datatype".to_string(),
                "http://www.w3.org/2001/XMLSchema#string".to_string(),
            );
            prop1.insert("minCount".to_string(), "1".to_string());
            prop1.insert("maxLength".to_string(), "255".to_string());

            let mut prop2 = HashMap::new();
            prop2.insert(
                "property".to_string(),
                "http://example.com/shape/Customer/prop/name".to_string(),
            );
            prop2.insert("path".to_string(), "http://schema.org/name".to_string());
            prop2.insert("name".to_string(), "name".to_string());
            prop2.insert(
                "datatype".to_string(),
                "http://www.w3.org/2001/XMLSchema#string".to_string(),
            );
            prop2.insert("maxLength".to_string(), "100".to_string());

            Ok(vec![prop1, prop2])
        } else {
            // sh:in query
            Ok(vec![])
        }
    }

    #[test]
    fn test_parse_node_shape() {
        let parser = ShaclParser::new();
        let shape = parser
            .parse_node_shape(
                "http://example.com/shape/Customer",
                mock_sparql_customer_shape,
            )
            .unwrap();

        assert_eq!(shape.uri, "http://example.com/shape/Customer");
        assert_eq!(shape.target_class, "http://example.com/Customer");
        assert_eq!(shape.label, Some("Customer Shape".to_string()));
        assert!(!shape.closed);
        assert_eq!(shape.properties.len(), 2);
    }

    #[test]
    fn test_parse_property_shapes() {
        let parser = ShaclParser::new();
        let shape = parser
            .parse_node_shape(
                "http://example.com/shape/Customer",
                mock_sparql_customer_shape,
            )
            .unwrap();

        let email_prop = &shape.properties[0];
        assert_eq!(email_prop.path, "http://schema.org/email");
        assert_eq!(email_prop.name, Some("email".to_string()));
        assert_eq!(
            email_prop.datatype,
            Some("http://www.w3.org/2001/XMLSchema#string".to_string())
        );
        assert_eq!(email_prop.min_count, Some(1));
        assert_eq!(email_prop.max_length, Some(255));

        let name_prop = &shape.properties[1];
        assert_eq!(name_prop.path, "http://schema.org/name");
        assert_eq!(name_prop.min_count, None);
    }

    #[test]
    fn test_list_node_shapes() {
        fn mock_list_shapes(_query: &str) -> Result<Vec<HashMap<String, String>>> {
            let mut shape1 = HashMap::new();
            shape1.insert(
                "shape".to_string(),
                "http://example.com/shape/Customer".to_string(),
            );

            let mut shape2 = HashMap::new();
            shape2.insert(
                "shape".to_string(),
                "http://example.com/shape/Order".to_string(),
            );

            Ok(vec![shape1, shape2])
        }

        let parser = ShaclParser::new();
        let shapes = parser.list_node_shapes(mock_list_shapes).unwrap();

        assert_eq!(shapes.len(), 2);
        assert!(shapes.contains(&"http://example.com/shape/Customer".to_string()));
        assert!(shapes.contains(&"http://example.com/shape/Order".to_string()));
    }

    #[test]
    fn test_parse_severity() {
        assert_eq!(
            parse_severity("http://www.w3.org/ns/shacl#Info"),
            Some(SeverityLevel::Info)
        );
        assert_eq!(
            parse_severity("http://www.w3.org/ns/shacl#Warning"),
            Some(SeverityLevel::Warning)
        );
        assert_eq!(
            parse_severity("http://www.w3.org/ns/shacl#Violation"),
            Some(SeverityLevel::Violation)
        );
        assert_eq!(parse_severity("http://example.com/unknown"), None);
    }

    #[test]
    fn test_parse_node_kind() {
        assert_eq!(
            parse_node_kind("http://www.w3.org/ns/shacl#IRI"),
            Some(NodeKind::IRI)
        );
        assert_eq!(
            parse_node_kind("http://www.w3.org/ns/shacl#Literal"),
            Some(NodeKind::Literal)
        );
        assert_eq!(parse_node_kind("http://example.com/unknown"), None);
    }

    // Test new constraint features

    #[test]
    fn test_parse_exclusive_bounds() {
        fn mock_sparql_exclusive_bounds(query: &str) -> Result<Vec<HashMap<String, String>>> {
            if query.contains("SELECT ?targetClass") {
                let mut result = HashMap::new();
                result.insert(
                    "targetClass".to_string(),
                    "http://example.com/Product".to_string(),
                );
                Ok(vec![result])
            } else if query.contains("SELECT ?property") {
                let mut prop = HashMap::new();
                prop.insert(
                    "property".to_string(),
                    "http://example.com/prop/price".to_string(),
                );
                prop.insert("path".to_string(), "http://schema.org/price".to_string());
                prop.insert("name".to_string(), "price".to_string());
                prop.insert("minExclusive".to_string(), "0".to_string());
                prop.insert("maxExclusive".to_string(), "10000".to_string());
                Ok(vec![prop])
            } else {
                Ok(vec![])
            }
        }

        let parser = ShaclParser::new();
        let shape = parser
            .parse_node_shape(
                "http://example.com/shape/Product",
                mock_sparql_exclusive_bounds,
            )
            .unwrap();

        let price_prop = &shape.properties[0];
        assert_eq!(price_prop.min_exclusive, Some(0.0));
        assert_eq!(price_prop.max_exclusive, Some(10000.0));
    }

    #[test]
    fn test_parse_property_comparison_constraints() {
        fn mock_sparql_comparisons(query: &str) -> Result<Vec<HashMap<String, String>>> {
            if query.contains("SELECT ?targetClass") {
                let mut result = HashMap::new();
                result.insert(
                    "targetClass".to_string(),
                    "http://example.com/TimeRange".to_string(),
                );
                Ok(vec![result])
            } else if query.contains("SELECT ?property") {
                let mut prop1 = HashMap::new();
                prop1.insert(
                    "property".to_string(),
                    "http://example.com/prop/start".to_string(),
                );
                prop1.insert(
                    "path".to_string(),
                    "http://schema.org/startDate".to_string(),
                );
                prop1.insert("name".to_string(), "start_date".to_string());
                prop1.insert(
                    "lessThan".to_string(),
                    "http://schema.org/endDate".to_string(),
                );

                let mut prop2 = HashMap::new();
                prop2.insert(
                    "property".to_string(),
                    "http://example.com/prop/end".to_string(),
                );
                prop2.insert("path".to_string(), "http://schema.org/endDate".to_string());
                prop2.insert("name".to_string(), "end_date".to_string());

                Ok(vec![prop1, prop2])
            } else {
                Ok(vec![])
            }
        }

        let parser = ShaclParser::new();
        let shape = parser
            .parse_node_shape(
                "http://example.com/shape/TimeRange",
                mock_sparql_comparisons,
            )
            .unwrap();

        let start_prop = &shape.properties[0];
        assert_eq!(
            start_prop.less_than,
            Some("http://schema.org/endDate".to_string())
        );
    }

    #[test]
    fn test_parse_equality_constraint() {
        fn mock_sparql_equality(query: &str) -> Result<Vec<HashMap<String, String>>> {
            if query.contains("SELECT ?targetClass") {
                let mut result = HashMap::new();
                result.insert(
                    "targetClass".to_string(),
                    "http://example.com/Registration".to_string(),
                );
                Ok(vec![result])
            } else if query.contains("SELECT ?property") {
                let mut prop1 = HashMap::new();
                prop1.insert(
                    "property".to_string(),
                    "http://example.com/prop/email".to_string(),
                );
                prop1.insert("path".to_string(), "http://schema.org/email".to_string());
                prop1.insert("name".to_string(), "email".to_string());

                let mut prop2 = HashMap::new();
                prop2.insert(
                    "property".to_string(),
                    "http://example.com/prop/confirm".to_string(),
                );
                prop2.insert(
                    "path".to_string(),
                    "http://schema.org/confirmEmail".to_string(),
                );
                prop2.insert("name".to_string(), "confirm_email".to_string());
                prop2.insert("equals".to_string(), "http://schema.org/email".to_string());

                Ok(vec![prop1, prop2])
            } else {
                Ok(vec![])
            }
        }

        let parser = ShaclParser::new();
        let shape = parser
            .parse_node_shape(
                "http://example.com/shape/Registration",
                mock_sparql_equality,
            )
            .unwrap();

        let confirm_prop = &shape.properties[1];
        assert_eq!(
            confirm_prop.equals,
            Some("http://schema.org/email".to_string())
        );
    }

    #[test]
    fn test_parse_has_value_constraint() {
        fn mock_sparql_has_value(query: &str) -> Result<Vec<HashMap<String, String>>> {
            if query.contains("SELECT ?targetClass") {
                let mut result = HashMap::new();
                result.insert(
                    "targetClass".to_string(),
                    "http://example.com/Transaction".to_string(),
                );
                Ok(vec![result])
            } else if query.contains("SELECT ?property") {
                let mut prop = HashMap::new();
                prop.insert(
                    "property".to_string(),
                    "http://example.com/prop/currency".to_string(),
                );
                prop.insert("path".to_string(), "http://schema.org/currency".to_string());
                prop.insert("name".to_string(), "currency".to_string());
                prop.insert("hasValue".to_string(), "USD".to_string());
                Ok(vec![prop])
            } else {
                Ok(vec![])
            }
        }

        let parser = ShaclParser::new();
        let shape = parser
            .parse_node_shape(
                "http://example.com/shape/Transaction",
                mock_sparql_has_value,
            )
            .unwrap();

        let currency_prop = &shape.properties[0];
        assert_eq!(currency_prop.has_value, Some("USD".to_string()));
    }

    #[test]
    fn test_parse_pattern_flags() {
        fn mock_sparql_flags(query: &str) -> Result<Vec<HashMap<String, String>>> {
            if query.contains("SELECT ?targetClass") {
                let mut result = HashMap::new();
                result.insert(
                    "targetClass".to_string(),
                    "http://example.com/User".to_string(),
                );
                Ok(vec![result])
            } else if query.contains("SELECT ?property") {
                let mut prop = HashMap::new();
                prop.insert(
                    "property".to_string(),
                    "http://example.com/prop/username".to_string(),
                );
                prop.insert("path".to_string(), "http://schema.org/username".to_string());
                prop.insert("name".to_string(), "username".to_string());
                prop.insert("pattern".to_string(), "^[A-Z][a-z]+$".to_string());
                prop.insert("flags".to_string(), "i".to_string());
                Ok(vec![prop])
            } else {
                Ok(vec![])
            }
        }

        let parser = ShaclParser::new();
        let shape = parser
            .parse_node_shape("http://example.com/shape/User", mock_sparql_flags)
            .unwrap();

        let username_prop = &shape.properties[0];
        assert_eq!(username_prop.pattern, Some("^[A-Z][a-z]+$".to_string()));
        assert_eq!(username_prop.pattern_flags, Some("i".to_string()));
    }

    #[test]
    fn test_parse_in_constraint_mock() {
        // Test parse_in_constraint method
        fn mock_sparql_in_values(query: &str) -> Result<Vec<HashMap<String, String>>> {
            if query.contains("sh:in") {
                let values = vec!["active", "inactive", "pending"];
                Ok(values
                    .into_iter()
                    .map(|v| {
                        let mut map = HashMap::new();
                        map.insert("value".to_string(), v.to_string());
                        map
                    })
                    .collect())
            } else if query.contains("SELECT ?targetClass") {
                let mut result = HashMap::new();
                result.insert(
                    "targetClass".to_string(),
                    "http://example.com/Account".to_string(),
                );
                Ok(vec![result])
            } else if query.contains("SELECT ?property") {
                let mut prop = HashMap::new();
                prop.insert(
                    "property".to_string(),
                    "http://example.com/prop/status".to_string(),
                );
                prop.insert("path".to_string(), "http://schema.org/status".to_string());
                prop.insert("name".to_string(), "status".to_string());
                Ok(vec![prop])
            } else {
                Ok(vec![])
            }
        }

        let parser = ShaclParser::new();
        let shape = parser
            .parse_node_shape("http://example.com/shape/Account", mock_sparql_in_values)
            .unwrap();

        let status_prop = &shape.properties[0];
        assert!(status_prop.in_values.is_some());
        let values = status_prop.in_values.as_ref().unwrap();
        assert_eq!(values.len(), 3);
        assert!(values.contains(&"active".to_string()));
        assert!(values.contains(&"inactive".to_string()));
        assert!(values.contains(&"pending".to_string()));
    }

    #[test]
    fn test_parse_disjoint_constraint() {
        fn mock_sparql_disjoint(query: &str) -> Result<Vec<HashMap<String, String>>> {
            if query.contains("SELECT ?targetClass") {
                let mut result = HashMap::new();
                result.insert(
                    "targetClass".to_string(),
                    "http://example.com/Person".to_string(),
                );
                Ok(vec![result])
            } else if query.contains("SELECT ?property") {
                let mut prop = HashMap::new();
                prop.insert(
                    "property".to_string(),
                    "http://example.com/prop/work_email".to_string(),
                );
                prop.insert(
                    "path".to_string(),
                    "http://schema.org/workEmail".to_string(),
                );
                prop.insert("name".to_string(), "work_email".to_string());
                prop.insert(
                    "disjoint".to_string(),
                    "http://schema.org/personalEmail".to_string(),
                );
                Ok(vec![prop])
            } else {
                Ok(vec![])
            }
        }

        let parser = ShaclParser::new();
        let shape = parser
            .parse_node_shape("http://example.com/shape/Person", mock_sparql_disjoint)
            .unwrap();

        let work_email_prop = &shape.properties[0];
        assert_eq!(
            work_email_prop.disjoint,
            Some("http://schema.org/personalEmail".to_string())
        );
    }

    // Edge case tests

    #[test]
    fn test_missing_node_shape() {
        fn mock_sparql_empty(_query: &str) -> Result<Vec<HashMap<String, String>>> {
            Ok(vec![])
        }

        let parser = ShaclParser::new();
        let result =
            parser.parse_node_shape("http://example.com/shape/NonExistent", mock_sparql_empty);

        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Node shape not found"));
        assert!(error_msg.contains("http://example.com/shape/NonExistent"));
    }

    #[test]
    fn test_missing_target_class() {
        fn mock_sparql_no_target_class(query: &str) -> Result<Vec<HashMap<String, String>>> {
            if query.contains("SELECT ?targetClass") {
                // Return shape but without targetClass
                let mut result = HashMap::new();
                result.insert("label".to_string(), "Some Shape".to_string());
                Ok(vec![result])
            } else {
                Ok(vec![])
            }
        }

        let parser = ShaclParser::new();
        let result = parser.parse_node_shape(
            "http://example.com/shape/Invalid",
            mock_sparql_no_target_class,
        );

        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Missing targetClass"));
    }

    #[test]
    fn test_missing_property_path() {
        fn mock_sparql_no_path(query: &str) -> Result<Vec<HashMap<String, String>>> {
            if query.contains("SELECT ?targetClass") {
                let mut result = HashMap::new();
                result.insert(
                    "targetClass".to_string(),
                    "http://example.com/Test".to_string(),
                );
                Ok(vec![result])
            } else if query.contains("SELECT ?property") {
                let mut prop = HashMap::new();
                prop.insert(
                    "property".to_string(),
                    "http://example.com/prop/test".to_string(),
                );
                // Missing "path" field
                prop.insert("name".to_string(), "test".to_string());
                Ok(vec![prop])
            } else {
                Ok(vec![])
            }
        }

        let parser = ShaclParser::new();
        let result = parser.parse_node_shape("http://example.com/shape/Test", mock_sparql_no_path);

        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Missing path in property shape"));
    }

    #[test]
    fn test_shape_with_no_properties() {
        fn mock_sparql_no_props(query: &str) -> Result<Vec<HashMap<String, String>>> {
            if query.contains("SELECT ?targetClass") {
                let mut result = HashMap::new();
                result.insert(
                    "targetClass".to_string(),
                    "http://example.com/Empty".to_string(),
                );
                Ok(vec![result])
            } else {
                // No properties
                Ok(vec![])
            }
        }

        let parser = ShaclParser::new();
        let shape = parser
            .parse_node_shape("http://example.com/shape/Empty", mock_sparql_no_props)
            .unwrap();

        assert_eq!(shape.properties.len(), 0);
        assert_eq!(shape.target_class, "http://example.com/Empty");
    }

    #[test]
    fn test_validation_conflicting_bounds() {
        let parser = ShaclParser::new();
        let mut property = PropertyShape::new("http://schema.org/test".to_string());
        property.name = Some("test".to_string());
        property.min_inclusive = Some(5.0);
        property.min_exclusive = Some(10.0);

        let warnings = parser.validate_property_constraints(&property);
        assert!(!warnings.is_empty());
        assert!(warnings[0].contains("minInclusive"));
        assert!(warnings[0].contains("minExclusive"));
    }

    #[test]
    fn test_validation_invalid_range() {
        let parser = ShaclParser::new();
        let mut property = PropertyShape::new("http://schema.org/test".to_string());
        property.name = Some("test".to_string());
        property.min_inclusive = Some(100.0);
        property.max_inclusive = Some(50.0);

        let warnings = parser.validate_property_constraints(&property);
        assert!(!warnings.is_empty());
        assert!(warnings[0].contains("invalid numeric range"));
    }

    #[test]
    fn test_validation_in_with_bounds() {
        let parser = ShaclParser::new();
        let mut property = PropertyShape::new("http://schema.org/test".to_string());
        property.name = Some("test".to_string());
        property.in_values = Some(vec!["a".to_string(), "b".to_string()]);
        property.min_inclusive = Some(0.0);

        let warnings = parser.validate_property_constraints(&property);
        assert!(!warnings.is_empty());
        assert!(warnings[0].contains("sh:in"));
        assert!(warnings[0].contains("numeric bounds"));
    }

    #[test]
    fn test_validation_has_value_with_in() {
        let parser = ShaclParser::new();
        let mut property = PropertyShape::new("http://schema.org/test".to_string());
        property.name = Some("test".to_string());
        property.has_value = Some("fixed".to_string());
        property.in_values = Some(vec!["a".to_string(), "b".to_string()]);

        let warnings = parser.validate_property_constraints(&property);
        assert!(!warnings.is_empty());
        assert!(warnings[0].contains("sh:hasValue"));
        assert!(warnings[0].contains("sh:in"));
    }

    #[test]
    fn test_validation_string_constraints_on_non_string() {
        let parser = ShaclParser::new();
        let mut property = PropertyShape::new("http://schema.org/test".to_string());
        property.name = Some("test".to_string());
        property.datatype = Some("http://www.w3.org/2001/XMLSchema#integer".to_string());
        property.min_length = Some(5);
        property.pattern = Some("^[A-Z]+$".to_string());

        let warnings = parser.validate_property_constraints(&property);
        assert!(!warnings.is_empty());
        assert!(warnings[0].contains("string constraints"));
        assert!(warnings[0].contains("integer"));
    }

    #[test]
    fn test_validation_no_warnings_for_valid_property() {
        let parser = ShaclParser::new();
        let mut property = PropertyShape::new("http://schema.org/test".to_string());
        property.name = Some("test".to_string());
        property.datatype = Some("http://www.w3.org/2001/XMLSchema#string".to_string());
        property.min_length = Some(5);
        property.max_length = Some(100);
        property.pattern = Some("^[A-Z]+$".to_string());

        let warnings = parser.validate_property_constraints(&property);
        assert!(warnings.is_empty());
    }
}
