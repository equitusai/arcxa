//! Custom ontology registry and management
//!
//! This module provides sophisticated ontology management allowing users to:
//! - Register custom domain-specific ontologies
//! - Validate ontology syntax and semantics
//! - Merge multiple ontologies with namespace isolation
//! - Query and retrieve ontology definitions
//!
//! ## Architecture
//!
//! The system supports a multi-tier ontology approach:
//! 1. **Base Ontology**: Core Graphica + DCAT vocabulary (immutable)
//! 2. **Extension Ontology**: Semantic types and inference metadata (immutable)
//! 3. **Custom Ontologies**: User-defined domain ontologies (mutable)
//!
//! ## Usage Example
//!
//! ```rust,no_run
//! use graphica_core::catalog::ontology_registry::*;
//!
//! # fn example() -> anyhow::Result<()> {
//! let mut registry = OntologyRegistry::new();
//!
//! // Register a custom retail domain ontology
//! let retail_ontology = r#"
//!     @prefix retail: <http://example.com/retail#> .
//!     retail:Product a rdfs:Class .
//!     retail:productSKU a rdf:Property .
//! "#;
//!
//! registry.register_custom_ontology(
//!     "retail_domain",
//!     retail_ontology,
//!     Some("http://example.com/retail#".to_string())
//! )?;
//!
//! // Get merged ontology for inference
//! let merged = registry.get_merged_ontology();
//! # Ok(())
//! # }
//! ```

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::ToSchema;

use super::ontology::CATALOG_ONTOLOGY;
use super::ontology_extensions::EXTENDED_CATALOG_ONTOLOGY;

/// Custom ontology metadata
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OntologyMetadata {
    /// Unique identifier for this ontology
    pub id: String,

    /// Human-readable name
    pub name: String,

    /// Description of this ontology's purpose
    pub description: Option<String>,

    /// Namespace URI for this ontology
    pub namespace: String,

    /// Version string
    pub version: String,

    /// Author/organization
    pub author: Option<String>,

    /// When this ontology was registered
    pub registered_at: DateTime<Utc>,

    /// When this ontology was last updated
    pub updated_at: DateTime<Utc>,

    /// Tags for categorization
    pub tags: Vec<String>,

    /// Whether this ontology is active
    pub active: bool,
}

impl OntologyMetadata {
    /// Create new ontology metadata
    pub fn new(id: impl Into<String>, namespace: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: id.into(),
            name: String::new(),
            description: None,
            namespace: namespace.into(),
            version: "1.0.0".to_string(),
            author: None,
            registered_at: now,
            updated_at: now,
            tags: Vec::new(),
            active: true,
        }
    }

    /// Set name
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Set description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Set version
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }

    /// Add tags
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }
}

/// Registered custom ontology
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisteredOntology {
    /// Metadata about this ontology
    pub metadata: OntologyMetadata,

    /// The ontology content (Turtle format)
    pub content: String,

    /// Validation status
    pub validation_status: ValidationStatus,

    /// Statistics about this ontology
    pub stats: OntologyStatistics,
}

/// Validation status for an ontology
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub enum ValidationStatus {
    /// Not yet validated
    Pending,

    /// Validation passed
    Valid,

    /// Validation failed
    Invalid { errors: Vec<String> },

    /// Validation warnings (valid but has issues)
    ValidWithWarnings { warnings: Vec<String> },
}

/// Statistics about an ontology
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct OntologyStatistics {
    /// Number of classes defined
    pub class_count: usize,

    /// Number of properties defined
    pub property_count: usize,

    /// Number of individuals/instances
    pub individual_count: usize,

    /// Size in bytes
    pub size_bytes: usize,

    /// Number of times this ontology has been used in queries
    pub usage_count: u64,
}

/// Ontology registry managing multiple ontologies
pub struct OntologyRegistry {
    /// Registered custom ontologies
    ontologies: HashMap<String, RegisteredOntology>,

    /// Namespace to ontology ID mapping
    namespace_index: HashMap<String, String>,

    /// Validation enabled
    validation_enabled: bool,
}

impl OntologyRegistry {
    /// Create a new ontology registry
    pub fn new() -> Self {
        Self {
            ontologies: HashMap::new(),
            namespace_index: HashMap::new(),
            validation_enabled: true,
        }
    }

    /// Register a custom ontology
    ///
    /// # Arguments
    /// * `id` - Unique identifier for this ontology
    /// * `content` - Ontology content in Turtle format
    /// * `namespace` - Optional namespace URI (will be extracted if not provided)
    pub fn register_custom_ontology(
        &mut self,
        id: impl Into<String>,
        content: impl Into<String>,
        namespace: Option<String>,
    ) -> Result<OntologyMetadata> {
        let id = id.into();
        let content = content.into();

        // Extract or use provided namespace
        let namespace = namespace.unwrap_or_else(|| {
            Self::extract_namespace(&content)
                .unwrap_or_else(|| format!("http://graphica.io/custom/{}", id))
        });

        // Check for namespace conflicts
        if let Some(existing_id) = self.namespace_index.get(&namespace) {
            if *existing_id != id {
                anyhow::bail!(
                    "Namespace {} already registered by ontology {}",
                    namespace,
                    existing_id
                );
            }
        }

        // Validate if enabled
        let validation_status = if self.validation_enabled {
            Self::validate_ontology(&content)?
        } else {
            ValidationStatus::Pending
        };

        // Calculate statistics
        let stats = Self::calculate_statistics(&content);

        // Create metadata
        let metadata = OntologyMetadata::new(&id, &namespace);

        let registered = RegisteredOntology {
            metadata: metadata.clone(),
            content,
            validation_status,
            stats,
        };

        // Store
        self.ontologies.insert(id.clone(), registered);
        self.namespace_index.insert(namespace, id);

        Ok(metadata)
    }

    /// Update an existing ontology
    pub fn update_ontology(&mut self, id: &str, new_content: impl Into<String>) -> Result<()> {
        let ontology = self.ontologies.get_mut(id).context("Ontology not found")?;

        let new_content = new_content.into();

        // Validate new content
        let validation_status = if self.validation_enabled {
            Self::validate_ontology(&new_content)?
        } else {
            ValidationStatus::Pending
        };

        // Update
        ontology.content = new_content;
        ontology.validation_status = validation_status;
        ontology.metadata.updated_at = Utc::now();
        ontology.stats = Self::calculate_statistics(&ontology.content);

        Ok(())
    }

    /// Update metadata fields of an existing ontology
    ///
    /// This allows updating metadata (name, version, active, description, tags)
    /// without changing the ontology content.
    pub fn update_metadata<F>(&mut self, id: &str, update_fn: F) -> Result<()>
    where
        F: FnOnce(&mut OntologyMetadata),
    {
        let ontology = self.ontologies.get_mut(id).context("Ontology not found")?;

        update_fn(&mut ontology.metadata);
        ontology.metadata.updated_at = Utc::now();

        Ok(())
    }

    /// Deactivate an ontology (soft delete)
    pub fn deactivate_ontology(&mut self, id: &str) -> Result<()> {
        let ontology = self.ontologies.get_mut(id).context("Ontology not found")?;

        ontology.metadata.active = false;
        Ok(())
    }

    /// Activate an ontology
    pub fn activate_ontology(&mut self, id: &str) -> Result<()> {
        let ontology = self.ontologies.get_mut(id).context("Ontology not found")?;

        ontology.metadata.active = true;
        Ok(())
    }

    /// Remove an ontology completely
    pub fn remove_ontology(&mut self, id: &str) -> Result<RegisteredOntology> {
        let ontology = self.ontologies.remove(id).context("Ontology not found")?;

        // Remove from namespace index
        self.namespace_index.remove(&ontology.metadata.namespace);

        Ok(ontology)
    }

    /// Get an ontology by ID
    pub fn get_ontology(&self, id: &str) -> Option<&RegisteredOntology> {
        self.ontologies.get(id)
    }

    /// Get ontology by namespace
    pub fn get_by_namespace(&self, namespace: &str) -> Option<&RegisteredOntology> {
        self.namespace_index
            .get(namespace)
            .and_then(|id| self.ontologies.get(id))
    }

    /// List all registered ontologies
    pub fn list_ontologies(&self) -> Vec<&OntologyMetadata> {
        self.ontologies.values().map(|o| &o.metadata).collect()
    }

    /// List active ontologies only
    pub fn list_active_ontologies(&self) -> Vec<&OntologyMetadata> {
        self.ontologies
            .values()
            .filter(|o| o.metadata.active)
            .map(|o| &o.metadata)
            .collect()
    }

    /// Get merged ontology combining base + extensions + all active custom ontologies
    ///
    /// This creates a single Turtle document with all ontologies merged.
    pub fn get_merged_ontology(&self) -> String {
        let mut merged = String::new();

        // Base catalog ontology
        merged.push_str("# ============= BASE CATALOG ONTOLOGY =============\n");
        merged.push_str(CATALOG_ONTOLOGY);
        merged.push_str("\n\n");

        // Extended inference ontology
        merged.push_str("# ============= EXTENDED INFERENCE ONTOLOGY =============\n");
        merged.push_str(EXTENDED_CATALOG_ONTOLOGY);
        merged.push_str("\n\n");

        // Custom ontologies (active only)
        for (id, ontology) in &self.ontologies {
            if !ontology.metadata.active {
                continue;
            }

            merged.push_str(&format!(
                "# ============= CUSTOM ONTOLOGY: {} =============\n",
                id
            ));
            merged.push_str(&ontology.content);
            merged.push_str("\n\n");
        }

        merged
    }

    /// Get merged ontology with specific ontologies
    pub fn get_merged_with_ontologies(&self, ontology_ids: &[String]) -> Result<String> {
        let mut merged = String::new();

        // Base
        merged.push_str(CATALOG_ONTOLOGY);
        merged.push_str("\n\n");

        // Extensions
        merged.push_str(EXTENDED_CATALOG_ONTOLOGY);
        merged.push_str("\n\n");

        // Selected custom ontologies
        for id in ontology_ids {
            let ontology = self
                .ontologies
                .get(id)
                .context(format!("Ontology {} not found", id))?;

            merged.push_str(&format!("# CUSTOM: {}\n", id));
            merged.push_str(&ontology.content);
            merged.push_str("\n\n");
        }

        Ok(merged)
    }

    /// Validate ontology content
    fn validate_ontology(content: &str) -> Result<ValidationStatus> {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        // Basic Turtle syntax validation
        if !content.contains("@prefix") && !content.contains("PREFIX") {
            warnings.push("No prefix declarations found".to_string());
        }

        // Check for basic RDF structure
        if content.trim().is_empty() {
            errors.push("Ontology content is empty".to_string());
        }

        // TODO: Use rio_turtle or similar for proper validation
        // For now, basic checks

        if !errors.is_empty() {
            Ok(ValidationStatus::Invalid { errors })
        } else if !warnings.is_empty() {
            Ok(ValidationStatus::ValidWithWarnings { warnings })
        } else {
            Ok(ValidationStatus::Valid)
        }
    }

    /// Extract namespace from Turtle content
    fn extract_namespace(content: &str) -> Option<String> {
        // Look for @prefix declarations
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("@prefix") {
                // Extract URI from: @prefix foo: <http://example.com#> .
                if let Some(start) = trimmed.find('<') {
                    if let Some(end) = trimmed.find('>') {
                        return Some(trimmed[start + 1..end].to_string());
                    }
                }
            }
        }
        None
    }

    /// Calculate statistics for an ontology
    fn calculate_statistics(content: &str) -> OntologyStatistics {
        let mut stats = OntologyStatistics::default();

        stats.size_bytes = content.len();

        // Simple heuristic counting
        for line in content.lines() {
            let trimmed = line.trim();

            if trimmed.contains("a rdfs:Class") || trimmed.contains("a owl:Class") {
                stats.class_count += 1;
            }

            if trimmed.contains("a rdf:Property")
                || trimmed.contains("a owl:ObjectProperty")
                || trimmed.contains("a owl:DatatypeProperty")
            {
                stats.property_count += 1;
            }
        }

        stats
    }
}

impl Default for OntologyRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_custom_ontology() {
        let mut registry = OntologyRegistry::new();

        let content = r#"
@prefix retail: <http://example.com/retail#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .

retail:Product a rdfs:Class .
retail:productSKU a rdf:Property .
        "#;

        let result = registry.register_custom_ontology(
            "retail",
            content,
            Some("http://example.com/retail#".to_string()),
        );

        assert!(result.is_ok());
        assert_eq!(registry.ontologies.len(), 1);
    }

    #[test]
    fn test_namespace_conflict() {
        let mut registry = OntologyRegistry::new();

        let content1 = r#"@prefix a: <http://example.com/ns#> ."#;
        registry
            .register_custom_ontology("ont1", content1, Some("http://example.com/ns#".to_string()))
            .unwrap();

        let content2 = r#"@prefix b: <http://example.com/ns#> ."#;
        let result = registry.register_custom_ontology(
            "ont2",
            content2,
            Some("http://example.com/ns#".to_string()),
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_get_merged_ontology() {
        let mut registry = OntologyRegistry::new();

        let custom = r#"@prefix custom: <http://example.com/custom#> ."#;
        registry
            .register_custom_ontology("custom1", custom, None)
            .unwrap();

        let merged = registry.get_merged_ontology();

        // Check for actual ontology content
        assert!(merged.contains("@prefix gph:"));
        assert!(merged.contains("@prefix gphi:"));
        assert!(merged.contains("custom1"));
        assert!(merged.contains("<http://example.com/custom#>"));
    }

    #[test]
    fn test_deactivate_ontology() {
        let mut registry = OntologyRegistry::new();

        registry
            .register_custom_ontology("test", "@prefix t: <http://test#> .", None)
            .unwrap();
        assert_eq!(registry.list_active_ontologies().len(), 1);

        registry.deactivate_ontology("test").unwrap();
        assert_eq!(registry.list_active_ontologies().len(), 0);
    }

    #[test]
    fn test_extract_namespace() {
        let content = r#"@prefix retail: <http://example.com/retail#> ."#;
        let ns = OntologyRegistry::extract_namespace(content);
        assert_eq!(ns, Some("http://example.com/retail#".to_string()));
    }

    #[test]
    fn test_calculate_statistics() {
        let content = r#"
@prefix ex: <http://example.com#> .
ex:Product a rdfs:Class .
ex:Customer a rdfs:Class .
ex:hasProduct a rdf:Property .
        "#;

        let stats = OntologyRegistry::calculate_statistics(content);
        assert_eq!(stats.class_count, 2);
        assert_eq!(stats.property_count, 1);
        assert!(stats.size_bytes > 0);
    }
}
