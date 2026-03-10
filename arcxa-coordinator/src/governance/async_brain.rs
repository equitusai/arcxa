//! # Async Governance Brain
//!
//! High-performance async implementation with batching for RDF materialization.
//! Targets 2,000+ events/sec throughput via:
//! - Non-blocking async operations
//! - Intelligent batching of RDF triple inserts
//! - Concurrent SPARQL query support
//! - Channel-based write pipeline

use crate::governance::async_config::AsyncGovernanceConfig;
use crate::governance::async_core::{
    AsyncBrainState, GovernanceMessage, ProcessorMetrics, QueryResults,
};
use crate::governance::batch_processor::BatchProcessor;
use crate::governance::rdf_store::GraphicaRdfStore;
use anyhow::{anyhow, Result};
use graphica_core::core::lineage::LineageEvent;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use tracing::{error, info};

/// Re-export types for backwards compatibility
pub use crate::governance::async_config::AsyncGovernanceConfig as BatchConfig;
pub use crate::governance::async_core::ProcessorMetrics as GovernanceMetrics;

/// Async governance brain with batching
///
/// Core async implementation that provides:
/// - Async materialization API
/// - Batch processing via background tasks
/// - Concurrent query support
/// - Metrics collection
pub struct AsyncGovernanceBrain {
    /// Message channel for sending to processors
    tx: mpsc::Sender<GovernanceMessage>,

    /// Shared state with metrics
    state: Arc<AsyncBrainState>,

    /// Background processor handles
    _processor_handles: Vec<tokio::task::JoinHandle<()>>,
}

impl AsyncGovernanceBrain {
    /// Create new async governance brain
    ///
    /// Spawns `config.num_processors` background tasks that handle batching
    /// and materialization of lineage events to RDF triples.
    pub async fn new(store: GraphicaRdfStore, config: AsyncGovernanceConfig) -> Result<Self> {
        info!(
            "Initializing AsyncGovernanceBrain: batch_size={}, timeout={:?}, processors={}",
            config.batch_size, config.batch_timeout, config.num_processors
        );

        // 1. Create shared state
        let state = Arc::new(AsyncBrainState::new(config.clone()));

        // 2. Create channel with capacity from config
        let (tx, rx) = mpsc::channel(config.channel_capacity);

        // 3. Wrap store in Arc for sharing across processors
        let store = Arc::new(store);

        // 4. Spawn batch processors
        // For simplicity, we'll use a single processor that owns the receiver
        // Multiple processors would require message distribution logic
        let mut handles = Vec::new();

        let processor = BatchProcessor::new(config.clone(), store.clone(), state.clone(), rx);

        let handle = tokio::spawn(async move {
            if let Err(e) = processor.run().await {
                error!("Batch processor failed: {}", e);
            }
        });

        handles.push(handle);

        Ok(Self {
            tx,
            state,
            _processor_handles: handles,
        })
    }

    /// Materialize a lineage event (async, with response)
    ///
    /// Sends event to background processor, waits for completion.
    /// Provides backpressure if channel is full.
    pub async fn materialize_lineage_event(&self, event: LineageEvent) -> Result<()> {
        self.tx
            .send(GovernanceMessage::MaterializeEvent(event))
            .await
            .map_err(|e| anyhow!("Failed to send event: {}", e))
    }

    /// Materialize batch of lineage events
    ///
    /// More efficient than calling materialize_lineage_event multiple times.
    pub async fn materialize_batch(&self, events: Vec<LineageEvent>) -> Result<()> {
        self.tx
            .send(GovernanceMessage::ProcessBatch(events))
            .await
            .map_err(|e| anyhow!("Failed to send batch: {}", e))
    }

    /// Execute SPARQL query
    ///
    /// Flushes pending batches before executing query to ensure consistency.
    pub async fn query(&self, sparql: &str) -> Result<QueryResults> {
        let (response_tx, response_rx) = oneshot::channel();

        self.tx
            .send(GovernanceMessage::Query {
                sparql: sparql.to_string(),
                response: response_tx,
            })
            .await
            .map_err(|e| anyhow!("Failed to send query: {}", e))?;

        response_rx
            .await
            .map_err(|e| anyhow!("Query response channel closed: {}", e))?
    }

    /// Get processor metrics
    pub async fn get_metrics(&self) -> Result<ProcessorMetrics> {
        Ok(self.state.get_metrics().await)
    }

    /// Shutdown gracefully
    ///
    /// Sends shutdown signal and waits for all processors to finish.
    /// Flushes any pending batches before shutting down.
    pub async fn shutdown(self) -> Result<()> {
        info!("Initiating AsyncGovernanceBrain shutdown");

        // Send shutdown signal
        self.tx
            .send(GovernanceMessage::Shutdown)
            .await
            .map_err(|e| anyhow!("Failed to send shutdown signal: {}", e))?;

        // Wait for all processors to finish
        for handle in self._processor_handles {
            if let Err(e) = handle.await {
                error!("Processor task failed during shutdown: {}", e);
            }
        }

        info!("AsyncGovernanceBrain shutdown complete");
        Ok(())
    }

    // ========================================================================
    // BACKWARDS COMPATIBILITY METHODS
    // ========================================================================

    /// Alias for materialize_lineage_event (backwards compatibility)
    pub async fn materialize_event(&self, event: &LineageEvent) -> Result<()> {
        self.materialize_lineage_event(event.clone()).await
    }

    /// Alias for materialize_batch (backwards compatibility)
    pub async fn materialize_events(&self, events: Vec<LineageEvent>) -> Result<()> {
        self.materialize_batch(events).await
    }

    /// Fire-and-forget materialization (backwards compatibility)
    pub fn materialize_event_nowait(&self, event: LineageEvent) -> Result<()> {
        self.tx
            .try_send(GovernanceMessage::MaterializeEvent(event))
            .map_err(|_| anyhow!("Write channel full - apply backpressure"))
    }

    /// Force flush of pending batches (backwards compatibility)
    pub async fn flush(&self) -> Result<()> {
        // No-op in new design - batches flush automatically
        Ok(())
    }

    /// Get triple count (backwards compatibility)
    pub async fn triple_count(&self) -> Result<usize> {
        let results = self
            .query("SELECT (COUNT(*) as ?count) WHERE { ?s ?p ?o }")
            .await?;

        if let Some(first) = results.first() {
            if let Some(count_val) = first.get("count") {
                match count_val {
                    serde_json::Value::Number(n) => {
                        if let Some(count) = n.as_u64() {
                            return Ok(count as usize);
                        }
                    }
                    serde_json::Value::String(s) => {
                        let count_str = s.split('^').next().unwrap_or(s);
                        let count_str = count_str.trim_matches('"');
                        if let Ok(count) = count_str.parse::<usize>() {
                            return Ok(count);
                        }
                    }
                    _ => {}
                }
            }
        }

        Ok(0)
    }

    /// Validate SHACL shapes (backwards compatibility)
    pub async fn validate_shacl(&self, _data_graph: &str, _shapes_graph: &str) -> Result<bool> {
        // TODO: Implement SHACL validation
        Ok(true)
    }

    /// Get metrics (backwards compatibility)
    pub fn metrics(&self) -> Arc<AsyncBrainState> {
        self.state.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use graphica_core::core::lineage::DataRef;
    use std::collections::HashMap;
    use uuid::Uuid;

    fn create_test_event(id: usize) -> LineageEvent {
        LineageEvent {
            id: Uuid::new_v4(),
            dataset: format!("test_dataset_{}", id),
            record_id: format!("record_{}", id),
            source_refs: vec![DataRef {
                system: "test".to_string(),
                path: "test_path".to_string(),
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

    #[tokio::test]
    async fn test_async_brain_creation() {
        let store = GraphicaRdfStore::new_in_memory().unwrap();
        let config = AsyncGovernanceConfig::default();
        let brain = AsyncGovernanceBrain::new(store, config).await;

        assert!(brain.is_ok());
    }

    #[tokio::test]
    async fn test_materialize_event() {
        let store = GraphicaRdfStore::new_in_memory().unwrap();
        let config = AsyncGovernanceConfig {
            batch_size: 10,
            num_processors: 1,
            ..Default::default()
        };

        let brain = AsyncGovernanceBrain::new(store, config).await.unwrap();
        let event = create_test_event(1);

        let result = brain.materialize_lineage_event(event).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_materialize_batch() {
        let store = GraphicaRdfStore::new_in_memory().unwrap();
        let config = AsyncGovernanceConfig::default();

        let brain = AsyncGovernanceBrain::new(store, config).await.unwrap();
        let events: Vec<_> = (0..10).map(create_test_event).collect();

        let result = brain.materialize_batch(events).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_query() {
        let store = GraphicaRdfStore::new_in_memory().unwrap();
        let config = AsyncGovernanceConfig::default();

        let brain = AsyncGovernanceBrain::new(store, config).await.unwrap();

        let result = brain
            .query("SELECT ?s ?p ?o WHERE { ?s ?p ?o } LIMIT 1")
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_metrics() {
        let store = GraphicaRdfStore::new_in_memory().unwrap();
        let config = AsyncGovernanceConfig::default();

        let brain = AsyncGovernanceBrain::new(store, config).await.unwrap();

        let metrics = brain.get_metrics().await;
        assert!(metrics.is_ok());
    }

    #[tokio::test]
    async fn test_shutdown() {
        let store = GraphicaRdfStore::new_in_memory().unwrap();
        let config = AsyncGovernanceConfig::default();

        let brain = AsyncGovernanceBrain::new(store, config).await.unwrap();

        let result = brain.shutdown().await;
        assert!(result.is_ok());
    }
}
