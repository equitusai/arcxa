//! RDF Loader executor - Load entities into the triple store
//!
//! Batch loads entities as RDF triples with full lineage capture.
//! Converts JSON records to RDF using a simple property-based mapping.

use anyhow::{Context, Result};
use graphica_core::orchestration::workflow::RdfLoaderConfig;
use serde_json::{json, Value};
use std::sync::Arc;

use crate::governance::rdf_store::{GraphicaRdfStore, NamedGraph, RdfStore};

/// RDF Loader executor
pub struct RdfLoaderExecutor {
    config: RdfLoaderConfig,
    rdf_store: Option<Arc<GraphicaRdfStore>>,
}

impl RdfLoaderExecutor {
    pub fn new(config: RdfLoaderConfig) -> Self {
        Self {
            config,
            rdf_store: None,
        }
    }

    /// Create with RDF store
    pub fn with_rdf_store(config: RdfLoaderConfig, rdf_store: Arc<GraphicaRdfStore>) -> Self {
        Self {
            config,
            rdf_store: Some(rdf_store),
        }
    }

    /// Load entities into RDF store
    pub async fn load(&self, records: Vec<Value>) -> Result<u64> {
        if records.is_empty() {
            return Ok(0);
        }

        let rdf_store = self
            .rdf_store
            .as_ref()
            .context("RDF store not configured - use with_rdf_store() to provide store")?;

        // Determine target graph
        let graph = self
            .config
            .target_graph
            .as_ref()
            .map(|uri| NamedGraph::new(uri));

        // Process records in batches
        let mut total_loaded = 0u64;
        for batch in records.chunks(self.config.batch_size) {
            let triples = self.convert_records_to_triples(batch)?;

            // Insert batch into RDF store
            rdf_store
                .insert_triples(triples, graph.as_ref())
                .context("Failed to insert triples into RDF store")?;

            total_loaded += batch.len() as u64;

            tracing::debug!(
                "Loaded batch of {} entities into RDF store (graph: {:?})",
                batch.len(),
                graph.as_ref().map(|g| &g.uri)
            );
        }

        tracing::info!(
            "Loaded {} entities as {} triples into RDF store (entity_type: {}, graph: {:?})",
            total_loaded,
            total_loaded * 3, // Rough estimate: entity + type + properties
            self.config.entity_type,
            graph.as_ref().map(|g| &g.uri)
        );

        // Capture lineage if enabled
        if self.config.capture_lineage {
            self.capture_lineage(rdf_store, total_loaded, graph.as_ref())?;
        }

        Ok(total_loaded)
    }

    /// Convert JSON records to RDF triples
    fn convert_records_to_triples(
        &self,
        records: &[Value],
    ) -> Result<Vec<(String, String, String)>> {
        let mut triples = Vec::new();

        for record in records {
            if let Value::Object(obj) = record {
                // Extract entity ID
                let entity_id = obj
                    .get(&self.config.id_field)
                    .and_then(|v| v.as_str())
                    .context(format!(
                        "Missing or invalid ID field: {}",
                        self.config.id_field
                    ))?;

                // Create entity URI
                let entity_uri = format!(
                    "http://graphica.io/entity/{}/{}",
                    self.config.entity_type, entity_id
                );

                // Add type triple
                triples.push((
                    entity_uri.clone(),
                    "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_string(),
                    format!("http://graphica.io/type/{}", self.config.entity_type),
                ));

                // Add property triples for each field
                for (key, value) in obj.iter() {
                    if key == &self.config.id_field {
                        continue; // Skip ID field, already used for URI
                    }

                    let predicate = format!("http://graphica.io/property/{}", key);
                    let object = self.value_to_rdf_object(value);

                    triples.push((entity_uri.clone(), predicate, object));
                }
            } else {
                anyhow::bail!("Expected record to be a JSON object");
            }
        }

        Ok(triples)
    }

    /// Convert JSON value to RDF object (literal or URI)
    fn value_to_rdf_object(&self, value: &Value) -> String {
        match value {
            Value::Null => "".to_string(), // Empty string for null
            Value::Bool(b) => format!("\"{}\"^^<http://www.w3.org/2001/XMLSchema#boolean>", b),
            Value::Number(n) => {
                if n.is_i64() {
                    format!("\"{}\"^^<http://www.w3.org/2001/XMLSchema#integer>", n)
                } else {
                    format!("\"{}\"^^<http://www.w3.org/2001/XMLSchema#double>", n)
                }
            }
            Value::String(s) => {
                // Escape quotes in string literals
                let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
                format!("\"{}\"", escaped)
            }
            Value::Array(arr) => {
                // Serialize array as JSON string
                format!(
                    "\"{}\"",
                    serde_json::to_string(arr).unwrap_or_else(|_| "[]".to_string())
                )
            }
            Value::Object(obj) => {
                // Serialize object as JSON string
                format!(
                    "\"{}\"",
                    serde_json::to_string(obj).unwrap_or_else(|_| "{}".to_string())
                )
            }
        }
    }

    /// Capture lineage for the load operation
    fn capture_lineage(
        &self,
        rdf_store: &GraphicaRdfStore,
        entity_count: u64,
        graph: Option<&NamedGraph>,
    ) -> Result<()> {
        let now = chrono::Utc::now();
        let load_id = format!("load_{}", now.timestamp());
        let load_uri = format!("http://graphica.io/load/{}", load_id);

        // Create lineage triples (W3C PROV)
        let lineage_triples = vec![
            // Load activity type
            (
                load_uri.clone(),
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_string(),
                "http://www.w3.org/ns/prov#Activity".to_string(),
            ),
            // Entity type loaded
            (
                load_uri.clone(),
                "http://graphica.io/prov/entityType".to_string(),
                format!("\"{}\"", self.config.entity_type),
            ),
            // Entity count
            (
                load_uri.clone(),
                "http://graphica.io/prov/entityCount".to_string(),
                format!(
                    "\"{}\"^^<http://www.w3.org/2001/XMLSchema#integer>",
                    entity_count
                ),
            ),
            // Timestamp
            (
                load_uri.clone(),
                "http://www.w3.org/ns/prov#atTime".to_string(),
                format!(
                    "\"{}\"^^<http://www.w3.org/2001/XMLSchema#dateTime>",
                    now.to_rfc3339()
                ),
            ),
        ];

        rdf_store
            .insert_triples(lineage_triples, graph)
            .context("Failed to capture lineage")?;

        tracing::debug!("Captured lineage for RDF load: {}", load_uri);

        Ok(())
    }
}

#[async_trait::async_trait]
impl crate::etl::EtlExecutor for RdfLoaderExecutor {
    async fn execute(&self, input: Value) -> Result<Value> {
        let records = match &input {
            Value::Array(arr) => arr.clone(),
            Value::Object(obj) if obj.contains_key("records") => match &obj["records"] {
                Value::Array(arr) => arr.clone(),
                _ => anyhow::bail!("Expected 'records' to be an array"),
            },
            _ => anyhow::bail!("Expected array or object with 'records' field"),
        };

        let entities_loaded = self.load(records).await?;

        Ok(json!({
            "entities_loaded": entities_loaded,
            "entity_type": self.config.entity_type,
            "target_graph": self.config.target_graph,
            "lineage_captured": self.config.capture_lineage,
        }))
    }

    fn step_type(&self) -> &'static str {
        "rdf_loader"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_records_to_triples() {
        let config = RdfLoaderConfig {
            target_graph: None,
            entity_type: "Customer".to_string(),
            id_field: "id".to_string(),
            batch_size: 1000,
            capture_lineage: false,
        };

        let executor = RdfLoaderExecutor::new(config);

        let records = vec![
            json!({"id": "cust_001", "name": "Alice", "age": 30}),
            json!({"id": "cust_002", "name": "Bob", "email": "bob@example.com"}),
        ];

        let triples = executor.convert_records_to_triples(&records).unwrap();

        // Should have: 2 entities × (1 type + ~2-3 properties) = ~6-8 triples
        assert!(triples.len() >= 6);

        // Check that we have type triples
        let type_triples: Vec<_> = triples
            .iter()
            .filter(|(_, p, _)| p.contains("rdf-syntax-ns#type"))
            .collect();
        assert_eq!(type_triples.len(), 2);

        // Check that entity URIs are correct
        assert!(triples.iter().any(|(s, _, _)| s.contains("cust_001")));
        assert!(triples.iter().any(|(s, _, _)| s.contains("cust_002")));
    }

    #[test]
    fn test_value_to_rdf_object() {
        let config = RdfLoaderConfig {
            target_graph: None,
            entity_type: "Test".to_string(),
            id_field: "id".to_string(),
            batch_size: 1000,
            capture_lineage: false,
        };

        let executor = RdfLoaderExecutor::new(config);

        // Test string
        assert_eq!(executor.value_to_rdf_object(&json!("test")), "\"test\"");

        // Test integer
        assert!(executor.value_to_rdf_object(&json!(42)).contains("42"));
        assert!(executor.value_to_rdf_object(&json!(42)).contains("integer"));

        // Test boolean
        assert!(executor.value_to_rdf_object(&json!(true)).contains("true"));
        assert!(executor
            .value_to_rdf_object(&json!(true))
            .contains("boolean"));

        // Test null
        assert_eq!(executor.value_to_rdf_object(&Value::Null), "");
    }
}
