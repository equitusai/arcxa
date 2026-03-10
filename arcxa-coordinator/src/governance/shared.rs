//! # Simplified SharedGovernanceBrain
//!
//! Minimal wrapper for thread-safe access to GovernanceBrain.
//! This is the MVP implementation - we can add async/batching later.

use super::{converters::ToRdfTriples, GovernanceBrain};
use anyhow::Result;
use graphica_core::core::lineage::LineageEvent;
use std::sync::{Arc, Mutex};

/// Thread-safe wrapper for GovernanceBrain
///
/// Uses a simple Mutex for MVP - can be upgraded to RwLock or async later
#[derive(Clone)]
pub struct SharedGovernanceBrain {
    inner: Arc<Mutex<GovernanceBrain>>,
}

impl SharedGovernanceBrain {
    /// Create new shared governance brain
    pub fn new(brain: GovernanceBrain) -> Self {
        Self {
            inner: Arc::new(Mutex::new(brain)),
        }
    }

    /// Materialize a single lineage event to RDF (synchronous)
    pub fn materialize_event(&self, event: &LineageEvent) -> Result<()> {
        // Convert to RDF triples
        let triples = event.to_rdf_triples()?;

        // Lock and write
        let brain = self
            .inner
            .lock()
            .map_err(|e| anyhow::anyhow!("Failed to lock governance brain: {}", e))?;

        for (subject, predicate, object) in triples {
            brain.insert_lineage_triple(&subject, &predicate, &object)?;
        }

        tracing::debug!("Materialized lineage event {} to RDF", event.id);
        Ok(())
    }

    /// Execute SPARQL query
    pub fn query(&self, sparql: &str) -> Result<Vec<serde_json::Value>> {
        let brain = self
            .inner
            .lock()
            .map_err(|e| anyhow::anyhow!("Failed to lock governance brain: {}", e))?;
        brain.query(sparql)
    }

    /// Get triple count for monitoring
    pub fn triple_count(&self) -> Result<usize> {
        let brain = self
            .inner
            .lock()
            .map_err(|e| anyhow::anyhow!("Failed to lock governance brain: {}", e))?;
        brain.store().triple_count().map(|c| c as usize)
    }

    /// Validate SHACL shapes
    pub fn validate_shacl(&self, data_graph: &str, shapes_graph: &str) -> Result<bool> {
        let brain = self
            .inner
            .lock()
            .map_err(|e| anyhow::anyhow!("Failed to lock governance brain: {}", e))?;
        brain.validate_shacl(data_graph, shapes_graph)
    }

    /// Async wrapper for materialize_event (for compatibility with async code)
    ///
    /// Note: This is still synchronous under the hood but wrapped in async
    /// for compatibility with async storage writers. In production, this
    /// would be upgraded to true async with batching.
    pub async fn materialize_lineage_event(&self, event: &LineageEvent) -> Result<()> {
        // Clone to avoid holding references across await points
        let event_clone = event.clone();
        let self_clone = self.clone();

        // Run the synchronous operation in a blocking task
        // This prevents blocking the async runtime
        tokio::task::spawn_blocking(move || self_clone.materialize_event(&event_clone))
            .await
            .map_err(|e| anyhow::anyhow!("Task join error: {}", e))?
    }
}

/// Helper function to materialize a lineage event
///
/// This is the simplest way to get an event into RDF.
/// For production, consider using async batching.
pub fn materialize_lineage_event(brain: &SharedGovernanceBrain, event: LineageEvent) -> Result<()> {
    brain.materialize_event(&event)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use graphica_core::core::lineage::DataRef;
    use std::collections::HashMap;
    use uuid::Uuid;

    fn create_test_event() -> LineageEvent {
        LineageEvent {
            id: Uuid::new_v4(),
            dataset: "test_dataset".to_string(),
            record_id: "test_123".to_string(),
            source_refs: vec![DataRef {
                system: "kafka".to_string(),
                path: "test_topic".to_string(),
                version: None,
                extracted_at: Utc::now(),
                cdc_position: None,
            }],
            transforms: vec![],
            model_refs: vec![],
            output_ref: DataRef {
                system: "output".to_string(),
                path: "processed".to_string(),
                version: None,
                extracted_at: Utc::now(),
                cdc_position: None,
            },
            ts: Utc::now(),
            run_id: "test_run".to_string(),
            tenant_id: "test_tenant".to_string(),
            correlation_id: None,
            metadata: HashMap::new(),
        }
    }

    #[test]
    fn test_shared_brain_creation() {
        let brain = GovernanceBrain::new("./data/test_shared").unwrap();
        let shared = SharedGovernanceBrain::new(brain);

        // Should be able to clone
        let _cloned = shared.clone();
    }

    #[test]
    fn test_materialize_event() {
        let brain = GovernanceBrain::new("./data/test_materialize").unwrap();
        let shared = SharedGovernanceBrain::new(brain);

        let event = create_test_event();

        // Should materialize without error
        assert!(shared.materialize_event(&event).is_ok());
    }
}
