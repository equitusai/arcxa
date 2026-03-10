// Version manager for superseding old triples with new versions

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use std::sync::Arc;

use super::annotations::TransactionId;
use super::indexes::TemporalIndexes;
use crate::governance::rdf_star::AnnotatedTriple;
use crate::governance::rdf_store::{GraphicaRdfStore, RdfStore};

/// Manages version chains for bitemporal triples
///
/// When a new value for a triple arrives (same S,P but different O),
/// this manager:
/// 1. Checks if it supersedes an existing current version (via indexes - fast path)
/// 2. Closes the old version by setting tx_to
/// 3. Inserts the new version as current
pub struct VersionManager {
    store: Arc<GraphicaRdfStore>,
    /// Optional temporal indexes for O(1) lookups
    /// When present, uses indexes first before falling back to SPARQL
    indexes: Option<Arc<TemporalIndexes>>,
}

impl VersionManager {
    /// Create VersionManager with both RDF store and temporal indexes
    pub fn new(store: Arc<GraphicaRdfStore>, indexes: Arc<TemporalIndexes>) -> Self {
        Self {
            store,
            indexes: Some(indexes),
        }
    }

    /// Create VersionManager without indexes (backward compatible, uses SPARQL only)
    pub fn without_indexes(store: Arc<GraphicaRdfStore>) -> Self {
        Self {
            store,
            indexes: None,
        }
    }

    /// Check if a new triple supersedes an existing current version
    ///
    /// Returns the existing triple if it should be closed
    ///
    /// Performance: O(1) when indexes available, O(n) SPARQL fallback
    pub async fn check_supersedes(
        &self,
        new_triple: &AnnotatedTriple,
    ) -> Result<Option<ExistingVersion>> {
        // FAST PATH: Use indexes if available
        if let Some(ref indexes) = self.indexes {
            if let Some(current) =
                indexes.find_current_version(&new_triple.subject, &new_triple.predicate)?
            {
                // Check if value is different (requires superseding)
                if current.object != new_triple.object {
                    return Ok(Some(ExistingVersion {
                        subject: current.subject,
                        predicate: current.predicate,
                        object: current.object,
                        tx_id: current.tx_seq.to_string(),
                        tx_from: current.tx_from.to_rfc3339(),
                    }));
                } else {
                    // Same value, no superseding needed
                    return Ok(None);
                }
            } else {
                // No current version in indexes
                return Ok(None);
            }
        }

        // SLOW PATH: Fall back to SPARQL (for non-indexed data)
        self.check_supersedes_sparql(new_triple).await
    }

    /// SPARQL-based fallback for check_supersedes (slower but works without indexes)
    async fn check_supersedes_sparql(
        &self,
        new_triple: &AnnotatedTriple,
    ) -> Result<Option<ExistingVersion>> {
        // Simplified approach: Query all versions, filter in Rust
        // This is less efficient but more reliable with Oxigraph's RDF-star support
        let query = format!(
            r#"SELECT ?o WHERE {{
                <{subject}> <{predicate}> ?o .
            }}"#,
            subject = new_triple.subject,
            predicate = new_triple.predicate,
        );

        let results = self
            .store
            .query(&query)
            .context("Failed to query for existing versions")?;

        if results.is_empty() {
            return Ok(None);
        }

        // Get all object values
        let mut objects: Vec<String> = Vec::new();
        for result in &results {
            if let Some(val) = result.get("o") {
                if let Some(s) = val.as_str() {
                    objects.push(s.to_string());
                } else if let Some(obj) = val.as_object() {
                    if let Some(s) = obj.get("value").and_then(|v| v.as_str()) {
                        objects.push(s.to_string());
                    }
                }
            }
        }

        // Check if any existing value is different from new value
        let has_different_value = objects.iter().any(|obj| obj != &new_triple.object);

        if has_different_value && !objects.is_empty() {
            // Return the first existing version (simplified - in reality we'd want the current one)
            Ok(Some(ExistingVersion {
                subject: new_triple.subject.clone(),
                predicate: new_triple.predicate.clone(),
                object: objects[0].clone(),
                tx_id: "unknown".to_string(), // We don't have tx_id without proper query
                tx_from: chrono::Utc::now().to_rfc3339(), // Placeholder
            }))
        } else {
            Ok(None)
        }
    }

    /// Close an existing version by setting tx_to
    ///
    /// Updates both RDF store (via SPARQL UPDATE) and indexes
    pub async fn close_version(
        &self,
        existing: &ExistingVersion,
        closing_tx: &TransactionId,
    ) -> Result<()> {
        // 1. Update RDF store via SPARQL UPDATE
        let formatted_object = Self::format_object(&existing.object);
        let update = format!(
            r#"PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>
            DELETE {{
                << <{s}> <{p}> {o} >> <http://graphica.io/ontology#txTo> "MAX" .
            }}
            INSERT {{
                << <{s}> <{p}> {o} >> <http://graphica.io/ontology#txTo> "{tx_to}"^^xsd:dateTime .
            }}
            WHERE {{
                << <{s}> <{p}> {o} >> <http://graphica.io/ontology#txTo> "MAX" .
            }}"#,
            s = existing.subject,
            p = existing.predicate,
            o = formatted_object,
            tx_to = closing_tx.timestamp.to_rfc3339(),
        );

        self.store
            .update(&update)
            .context("Failed to close existing version in RDF store")?;

        // 2. Update indexes if available (supersede the version)
        if let Some(ref indexes) = self.indexes {
            // Find the version ID from the version chain
            let chain = indexes.get_version_chain(&existing.subject, &existing.predicate)?;

            // Find the version matching this tx_id and object
            if let Some(version) = chain
                .iter()
                .find(|v| v.tx_seq.to_string() == existing.tx_id && v.object == existing.object)
            {
                indexes
                    .supersede_version(&version.version_id, closing_tx.timestamp)
                    .context("Failed to supersede version in indexes")?;
            }
        }

        Ok(())
    }

    fn build_current_version_query(&self, subject: &str, predicate: &str) -> String {
        format!(
            r#"
            SELECT ?o ?txId ?txFrom WHERE {{
                <{subject}> <{predicate}> ?o .
                << <{subject}> <{predicate}> ?o >>
                    <http://graphica.io/ontology#txTo> "MAX" ;
                    <http://graphica.io/ontology#txId> ?txId ;
                    <http://graphica.io/ontology#txFrom> ?txFrom .
            }}
            LIMIT 1
            "#,
            subject = subject,
            predicate = predicate,
        )
    }

    fn parse_existing_version(
        &self,
        result: &serde_json::Value,
        new_triple: &AnnotatedTriple,
    ) -> Result<ExistingVersion> {
        // Handle both direct value and nested {"value": ...} formats
        let get_str = |key: &str| -> Result<String> {
            if let Some(val) = result.get(key) {
                if let Some(s) = val.as_str() {
                    return Ok(s.to_string());
                }
                if let Some(obj) = val.as_object() {
                    if let Some(s) = obj.get("value").and_then(|v| v.as_str()) {
                        return Ok(s.to_string());
                    }
                }
            }
            anyhow::bail!("Missing or invalid field: {}", key)
        };

        Ok(ExistingVersion {
            subject: new_triple.subject.clone(),
            predicate: new_triple.predicate.clone(),
            object: get_str("o")?,
            tx_id: get_str("txId")?,
            tx_from: get_str("txFrom")?,
        })
    }

    fn format_object(object: &str) -> String {
        // Simple formatting - wrap in quotes if it's a literal
        if object.starts_with("http://") || object.starts_with("https://") {
            format!("<{}>", object)
        } else {
            format!("\"{}\"", object.replace('"', "\\\""))
        }
    }
}

/// Represents an existing version found in the store
#[derive(Debug, Clone)]
pub struct ExistingVersion {
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub tx_id: String,
    pub tx_from: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_query_format() {
        let store = Arc::new(GraphicaRdfStore::new_in_memory().unwrap());
        let mgr = VersionManager::without_indexes(store);

        let query = mgr.build_current_version_query(
            "http://example.org/entity/1",
            "http://example.org/prop/name",
        );

        assert!(query.contains("txTo") && query.contains("MAX"));
        assert!(query.contains("LIMIT 1"));
    }

    #[test]
    fn test_object_formatting() {
        assert_eq!(
            VersionManager::format_object("http://example.org/entity"),
            "<http://example.org/entity>"
        );

        assert_eq!(VersionManager::format_object("John Doe"), "\"John Doe\"");

        assert_eq!(
            VersionManager::format_object("Test \"quoted\""),
            "\"Test \\\"quoted\\\"\""
        );
    }
}
