//! Ontology tree builder using SPARQL queries
//!
//! Extracts hierarchical structure from RDF ontologies by querying
//! rdfs:subClassOf, rdfs:subPropertyOf relationships and building a tree.

use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet};

use super::types::*;
use graphica_core::catalog::OntologyMetadata;

/// Parse Turtle content and extract hierarchical structure
pub struct OntologyTreeBuilder {
    /// The ontology content in Turtle format
    content: String,

    /// Maximum depth to traverse
    max_depth: i32,

    /// Include properties in class nodes
    include_properties: bool,

    /// Include individuals
    include_individuals: bool,

    /// Prefix mappings from @prefix declarations
    prefixes: HashMap<String, String>,
}

impl OntologyTreeBuilder {
    /// Create a new tree builder
    pub fn new(
        content: String,
        max_depth: i32,
        include_properties: bool,
        include_individuals: bool,
    ) -> Self {
        let prefixes = Self::parse_prefixes(&content);
        Self {
            content,
            max_depth,
            include_properties,
            include_individuals,
            prefixes,
        }
    }

    /// Build the tree structure
    pub fn build(self, metadata: OntologyMetadata) -> Result<OntologyTreeResponse> {
        // Parse the ontology content to extract:
        // 1. All classes and their subclass relationships
        // 2. All properties and their subproperty relationships
        // 3. Labels and comments

        let classes = self.extract_classes()?;
        let properties = self.extract_properties()?;
        let individuals = if self.include_individuals {
            self.extract_individuals()?
        } else {
            HashMap::new()
        };

        // Build class hierarchy
        let root_classes = self.build_class_hierarchy(&classes, &properties, &individuals)?;

        // Build property hierarchy
        let root_properties = if self.include_properties {
            self.build_property_hierarchy(&properties)?
        } else {
            Vec::new()
        };

        let stats = TreeStats {
            total_classes: classes.len(),
            total_properties: properties.len(),
            total_individuals: individuals.len(),
            max_depth: self.calculate_max_depth(&root_classes),
        };

        Ok(OntologyTreeResponse {
            namespace: metadata.namespace.clone(),
            metadata,
            root_classes,
            root_properties,
            stats,
        })
    }

    /// Extract all classes from ontology
    fn extract_classes(&self) -> Result<HashMap<String, ClassInfo>> {
        let mut classes = HashMap::new();

        // Parse lines looking for class definitions
        // Format: <uri> a rdfs:Class|owl:Class .
        // Format: <uri> rdfs:subClassOf <parent> .
        // Format: <uri> rdfs:label "Label" .
        // Format: <uri> rdfs:comment "Comment" .

        let mut current_uri: Option<String> = None;
        let mut class_info = ClassInfo::default();

        for line in self.content.lines() {
            let trimmed = line.trim();

            // Skip comments and empty lines
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            // Detect class declaration
            if trimmed.contains("a rdfs:Class") || trimmed.contains("a owl:Class") {
                if let Some(uri) = self.extract_uri(trimmed) {
                    if let Some(prev_uri) = current_uri.take() {
                        classes.insert(prev_uri, class_info);
                        class_info = ClassInfo::default();
                    }
                    current_uri = Some(uri.clone());
                    class_info.uri = uri;
                }
            }

            // rdfs:subClassOf
            if trimmed.contains("rdfs:subClassOf") {
                if let Some(parent) = self.extract_uri_from_object(trimmed) {
                    class_info.parent_classes.push(parent);
                }
            }

            // rdfs:label
            if trimmed.contains("rdfs:label") {
                if let Some(label) = Self::extract_literal(trimmed) {
                    class_info.label = label;
                }
            }

            // rdfs:comment
            if trimmed.contains("rdfs:comment") {
                if let Some(comment) = Self::extract_literal(trimmed) {
                    class_info.comment = Some(comment);
                }
            }

            // owl:deprecated
            if trimmed.contains("owl:deprecated") && trimmed.contains("true") {
                class_info.deprecated = true;
            }
        }

        // Don't forget the last class
        if let Some(uri) = current_uri {
            classes.insert(uri, class_info);
        }

        Ok(classes)
    }

    /// Extract all properties from ontology
    fn extract_properties(&self) -> Result<HashMap<String, PropertyInfo>> {
        let mut properties = HashMap::new();

        let mut current_uri: Option<String> = None;
        let mut prop_info = PropertyInfo::default();

        for line in self.content.lines() {
            let trimmed = line.trim();

            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            // Detect property declaration
            if trimmed.contains("a rdf:Property")
                || trimmed.contains("a owl:ObjectProperty")
                || trimmed.contains("a owl:DatatypeProperty")
                || trimmed.contains("a owl:AnnotationProperty")
            {
                if let Some(uri) = self.extract_uri(trimmed) {
                    if let Some(prev_uri) = current_uri.take() {
                        properties.insert(prev_uri, prop_info);
                        prop_info = PropertyInfo::default();
                    }
                    current_uri = Some(uri.clone());
                    prop_info.uri = uri;

                    // Determine property type
                    prop_info.property_type = if trimmed.contains("owl:ObjectProperty") {
                        PropertyType::ObjectProperty
                    } else if trimmed.contains("owl:DatatypeProperty") {
                        PropertyType::DatatypeProperty
                    } else if trimmed.contains("owl:AnnotationProperty") {
                        PropertyType::AnnotationProperty
                    } else {
                        PropertyType::RdfProperty
                    };
                }
            }

            // rdfs:domain
            if trimmed.contains("rdfs:domain") {
                if let Some(domain) = self.extract_uri_from_object(trimmed) {
                    prop_info.domain.push(domain);
                }
            }

            // rdfs:range
            if trimmed.contains("rdfs:range") {
                if let Some(range) = self.extract_uri_from_object(trimmed) {
                    prop_info.range.push(range);
                }
            }

            // rdfs:subPropertyOf
            if trimmed.contains("rdfs:subPropertyOf") {
                if let Some(parent) = self.extract_uri_from_object(trimmed) {
                    prop_info.parent_properties.push(parent);
                }
            }

            // rdfs:label
            if trimmed.contains("rdfs:label") {
                if let Some(label) = Self::extract_literal(trimmed) {
                    prop_info.label = label;
                }
            }

            // rdfs:comment
            if trimmed.contains("rdfs:comment") {
                if let Some(comment) = Self::extract_literal(trimmed) {
                    prop_info.comment = Some(comment);
                }
            }

            // owl:deprecated
            if trimmed.contains("owl:deprecated") && trimmed.contains("true") {
                prop_info.deprecated = true;
            }
        }

        // Don't forget the last property
        if let Some(uri) = current_uri {
            properties.insert(uri, prop_info);
        }

        Ok(properties)
    }

    /// Extract individuals (instances)
    fn extract_individuals(&self) -> Result<HashMap<String, IndividualInfo>> {
        let mut individuals = HashMap::new();

        // Look for statements like: <uri> a <ClassURI> .
        // where <ClassURI> is not rdfs:Class, owl:Class, rdf:Property, etc.

        for line in self.content.lines() {
            let trimmed = line.trim();

            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            // Skip meta-vocabulary (Class, Property definitions)
            if trimmed.contains("rdfs:Class")
                || trimmed.contains("owl:Class")
                || trimmed.contains("rdf:Property")
                || trimmed.contains("owl:ObjectProperty")
                || trimmed.contains("owl:DatatypeProperty")
            {
                continue;
            }

            // Look for: <uri> a <type> or prefix:name a prefix:Type
            if trimmed.contains(" a ") {
                if let Some(uri) = self.extract_uri(trimmed) {
                    if let Some(type_uri) = self.extract_uri_from_object(trimmed) {
                        let info =
                            individuals
                                .entry(uri.clone())
                                .or_insert_with(|| IndividualInfo {
                                    uri: uri.clone(),
                                    label: Self::extract_local_name(&uri),
                                    comment: None,
                                    types: Vec::new(),
                                });
                        info.types.push(type_uri);
                    }
                }
            }
        }

        Ok(individuals)
    }

    /// Build class hierarchy tree
    fn build_class_hierarchy(
        &self,
        classes: &HashMap<String, ClassInfo>,
        properties: &HashMap<String, PropertyInfo>,
        individuals: &HashMap<String, IndividualInfo>,
    ) -> Result<Vec<ClassNode>> {
        // Find root classes (those with no parents or owl:Thing as parent)
        let mut roots = Vec::new();
        let mut processed = HashSet::new();

        for (uri, info) in classes {
            if self.is_root_class(info) {
                let node = self.build_class_node(
                    uri,
                    info,
                    classes,
                    properties,
                    individuals,
                    0,
                    &mut processed,
                )?;
                roots.push(node);
            }
        }

        Ok(roots)
    }

    /// Check if class is a root (no parent or only owl:Thing)
    fn is_root_class(&self, info: &ClassInfo) -> bool {
        if info.parent_classes.is_empty() {
            return true;
        }

        // Check if all parents are owl:Thing or rdfs:Resource (top-level)
        info.parent_classes
            .iter()
            .all(|p| p.contains("owl#Thing") || p.contains("rdfs#Resource"))
    }

    /// Build a class node recursively
    fn build_class_node(
        &self,
        uri: &str,
        info: &ClassInfo,
        all_classes: &HashMap<String, ClassInfo>,
        all_properties: &HashMap<String, PropertyInfo>,
        all_individuals: &HashMap<String, IndividualInfo>,
        depth: usize,
        processed: &mut HashSet<String>,
    ) -> Result<ClassNode> {
        // Prevent infinite recursion
        if processed.contains(uri) {
            return Ok(ClassNode {
                uri: uri.to_string(),
                label: info.label.clone(),
                comment: info.comment.clone(),
                parent_classes: info.parent_classes.clone(),
                subclasses: Vec::new(),
                properties: None,
                individuals: None,
                depth,
                deprecated: info.deprecated,
            });
        }

        processed.insert(uri.to_string());

        // Check max depth
        if self.max_depth >= 0 && depth >= self.max_depth as usize {
            return Ok(ClassNode {
                uri: uri.to_string(),
                label: info.label.clone(),
                comment: info.comment.clone(),
                parent_classes: info.parent_classes.clone(),
                subclasses: Vec::new(),
                properties: None,
                individuals: None,
                depth,
                deprecated: info.deprecated,
            });
        }

        // Find subclasses
        let mut subclasses = Vec::new();
        for (child_uri, child_info) in all_classes {
            if child_info.parent_classes.contains(&uri.to_string()) {
                let child_node = self.build_class_node(
                    child_uri,
                    child_info,
                    all_classes,
                    all_properties,
                    all_individuals,
                    depth + 1,
                    processed,
                )?;
                subclasses.push(child_node);
            }
        }

        // Find properties with this class as domain
        let properties = if self.include_properties {
            let mut props = Vec::new();
            for (prop_uri, prop_info) in all_properties {
                if prop_info.domain.contains(&uri.to_string()) {
                    props.push(self.build_property_node(prop_uri, prop_info, all_properties, 0)?);
                }
            }
            if !props.is_empty() {
                Some(props)
            } else {
                None
            }
        } else {
            None
        };

        // Find individuals of this class
        let individuals = if self.include_individuals {
            let mut inds = Vec::new();
            for (ind_uri, ind_info) in all_individuals {
                if ind_info.types.contains(&uri.to_string()) {
                    inds.push(IndividualNode {
                        uri: ind_uri.clone(),
                        label: ind_info.label.clone(),
                        comment: ind_info.comment.clone(),
                        types: ind_info.types.clone(),
                    });
                }
            }
            if !inds.is_empty() {
                Some(inds)
            } else {
                None
            }
        } else {
            None
        };

        Ok(ClassNode {
            uri: uri.to_string(),
            label: info.label.clone(),
            comment: info.comment.clone(),
            parent_classes: info.parent_classes.clone(),
            subclasses,
            properties,
            individuals,
            depth,
            deprecated: info.deprecated,
        })
    }

    /// Build property hierarchy tree
    fn build_property_hierarchy(
        &self,
        properties: &HashMap<String, PropertyInfo>,
    ) -> Result<Vec<PropertyNode>> {
        let mut roots = Vec::new();

        for (uri, info) in properties {
            if info.parent_properties.is_empty() {
                let node = self.build_property_node(uri, info, properties, 0)?;
                roots.push(node);
            }
        }

        Ok(roots)
    }

    /// Build a property node recursively
    fn build_property_node(
        &self,
        uri: &str,
        info: &PropertyInfo,
        all_properties: &HashMap<String, PropertyInfo>,
        _depth: usize,
    ) -> Result<PropertyNode> {
        // Find sub-properties
        let mut subproperties = Vec::new();
        for (child_uri, child_info) in all_properties {
            if child_info.parent_properties.contains(&uri.to_string()) {
                let child_node =
                    self.build_property_node(child_uri, child_info, all_properties, _depth + 1)?;
                subproperties.push(child_node);
            }
        }

        Ok(PropertyNode {
            uri: uri.to_string(),
            label: info.label.clone(),
            comment: info.comment.clone(),
            property_type: info.property_type.clone(),
            domain: info.domain.clone(),
            range: info.range.clone(),
            parent_properties: info.parent_properties.clone(),
            subproperties,
            deprecated: info.deprecated,
        })
    }

    /// Calculate maximum depth of class hierarchy
    fn calculate_max_depth(&self, roots: &[ClassNode]) -> usize {
        roots
            .iter()
            .map(|node| self.node_max_depth(node))
            .max()
            .unwrap_or(0)
    }

    fn node_max_depth(&self, node: &ClassNode) -> usize {
        if node.subclasses.is_empty() {
            node.depth
        } else {
            node.subclasses
                .iter()
                .map(|child| self.node_max_depth(child))
                .max()
                .unwrap_or(node.depth)
        }
    }

    // ========================================================================
    // Parsing Utilities
    // ========================================================================

    /// Parse @prefix declarations from Turtle content
    fn parse_prefixes(content: &str) -> HashMap<String, String> {
        let mut prefixes = HashMap::new();

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("@prefix") {
                // Format: @prefix ex: <http://example.com#> .
                if let Some(rest) = trimmed.strip_prefix("@prefix") {
                    let parts: Vec<&str> = rest.trim().split_whitespace().collect();
                    if parts.len() >= 2 {
                        let prefix = parts[0].trim_end_matches(':');
                        if let Some(start) = parts[1].find('<') {
                            if let Some(end) = parts[1].find('>') {
                                let uri = &parts[1][start + 1..end];
                                prefixes.insert(prefix.to_string(), uri.to_string());
                            }
                        }
                    }
                }
            }
        }

        prefixes
    }

    /// Extract URI from subject position: <uri> or prefix:localName
    fn extract_uri(&self, line: &str) -> Option<String> {
        // Try full URI in angle brackets first
        if let Some(start) = line.find('<') {
            if let Some(end) = line[start..].find('>') {
                return Some(line[start + 1..start + end].to_string());
            }
        }

        // Try prefix:localName format
        // Look for pattern like "ex:Thing" at the start of the line
        let trimmed = line.trim();
        let words: Vec<&str> = trimmed.split_whitespace().collect();
        if let Some(first_word) = words.first() {
            if first_word.contains(':') && !first_word.starts_with('@') {
                let parts: Vec<&str> = first_word.split(':').collect();
                if parts.len() == 2 {
                    let prefix = parts[0];
                    let local_name = parts[1].trim_end_matches(';').trim_end_matches('.');
                    if let Some(namespace) = self.prefixes.get(prefix) {
                        return Some(format!("{}{}", namespace, local_name));
                    }
                }
            }
        }

        None
    }

    /// Extract URI from object position: ... <uri> or ... prefix:localName
    fn extract_uri_from_object(&self, line: &str) -> Option<String> {
        // Find the last occurrence of <...>
        let mut start_pos = None;
        let mut end_pos = None;

        for (i, ch) in line.char_indices() {
            if ch == '<' {
                start_pos = Some(i);
            } else if ch == '>' && start_pos.is_some() {
                end_pos = Some(i);
            }
        }

        if let (Some(start), Some(end)) = (start_pos, end_pos) {
            return Some(line[start + 1..end].to_string());
        }

        // Try prefix:localName format
        // Look for pattern after common predicates like rdfs:subClassOf, rdfs:domain, etc.
        let words: Vec<&str> = line.split_whitespace().collect();
        if words.len() >= 2 {
            // Get the last word (object position)
            let last_word = words
                .last()
                .unwrap()
                .trim_end_matches(';')
                .trim_end_matches('.');
            if last_word.contains(':') && !last_word.starts_with('@') {
                let parts: Vec<&str> = last_word.split(':').collect();
                if parts.len() == 2 {
                    let prefix = parts[0];
                    let local_name = parts[1];
                    if let Some(namespace) = self.prefixes.get(prefix) {
                        return Some(format!("{}{}", namespace, local_name));
                    }
                }
            }
        }

        None
    }

    /// Extract string literal: "value"
    fn extract_literal(line: &str) -> Option<String> {
        if let Some(start) = line.find('"') {
            if let Some(end) = line[start + 1..].find('"') {
                return Some(line[start + 1..start + 1 + end].to_string());
            }
        }
        None
    }

    /// Extract local name from URI (the part after # or /)
    fn extract_local_name(uri: &str) -> String {
        if let Some(pos) = uri.rfind('#') {
            return uri[pos + 1..].to_string();
        }
        if let Some(pos) = uri.rfind('/') {
            return uri[pos + 1..].to_string();
        }
        uri.to_string()
    }
}

// =============================================================================
// Internal Data Structures
// =============================================================================

#[derive(Debug, Clone, Default)]
struct ClassInfo {
    uri: String,
    label: String,
    comment: Option<String>,
    parent_classes: Vec<String>,
    deprecated: bool,
}

#[derive(Debug, Clone, Default)]
struct PropertyInfo {
    uri: String,
    label: String,
    comment: Option<String>,
    property_type: PropertyType,
    domain: Vec<String>,
    range: Vec<String>,
    parent_properties: Vec<String>,
    deprecated: bool,
}

impl Default for PropertyType {
    fn default() -> Self {
        PropertyType::RdfProperty
    }
}

#[derive(Debug, Clone)]
struct IndividualInfo {
    uri: String,
    label: String,
    comment: Option<String>,
    types: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_uri() {
        let content = "@prefix ex: <http://example.com#> .".to_string();
        let builder = OntologyTreeBuilder::new(content, -1, false, false);

        let line = "<http://example.com/Person> a rdfs:Class .";
        let uri = builder.extract_uri(line);
        assert_eq!(uri, Some("http://example.com/Person".to_string()));

        // Test prefix notation
        let line_prefix = "ex:Person a rdfs:Class .";
        let uri_prefix = builder.extract_uri(line_prefix);
        assert_eq!(uri_prefix, Some("http://example.com#Person".to_string()));
    }

    #[test]
    fn test_extract_literal() {
        let line = "rdfs:label \"Person\" .";
        let label = OntologyTreeBuilder::extract_literal(line);
        assert_eq!(label, Some("Person".to_string()));
    }

    #[test]
    fn test_extract_local_name() {
        assert_eq!(
            OntologyTreeBuilder::extract_local_name("http://example.com#Person"),
            "Person"
        );
        assert_eq!(
            OntologyTreeBuilder::extract_local_name("http://example.com/ontology/Person"),
            "Person"
        );
    }

    #[test]
    fn test_build_simple_hierarchy() {
        let content = r#"
@prefix ex: <http://example.com#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .

ex:Thing a rdfs:Class ;
    rdfs:label "Thing" .

ex:Person a rdfs:Class ;
    rdfs:subClassOf ex:Thing ;
    rdfs:label "Person" ;
    rdfs:comment "A person entity" .

ex:Employee a rdfs:Class ;
    rdfs:subClassOf ex:Person ;
    rdfs:label "Employee" .
        "#;

        let metadata = OntologyMetadata::new("test", "http://example.com#");
        let builder = OntologyTreeBuilder::new(content.to_string(), -1, false, false);
        let tree = builder.build(metadata).unwrap();

        assert_eq!(tree.stats.total_classes, 3);
        assert!(!tree.root_classes.is_empty());
    }
}
