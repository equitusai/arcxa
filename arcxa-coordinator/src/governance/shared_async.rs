//! # Async-Compatible SharedGovernanceBrain
//!
//! Updated wrapper that maintains backwards compatibility while enabling
//! high-performance async operations internally.
//!
//! Migration path:
//! 1. Use this module alongside existing shared.rs
//! 2. Switch via feature flag or runtime config
//! 3. Gradually migrate callers to async API
//! 4. Remove old implementation

use super::{
    async_brain::{AsyncGovernanceBrain, BatchConfig},
    GovernanceBrain,
};
use anyhow::Result;
use graphica_core::core::lineage::LineageEvent;
use std::sync::Arc;

/// Configuration for SharedGovernanceBrain
#[derive(Debug, Clone)]
pub struct GovernanceConfig {
    /// Use async implementation (default: true)
    pub use_async: bool,

    /// Batch configuration for async mode
    pub batch_config: Option<BatchConfig>,

    /// Enable metrics collection
    pub enable_metrics: bool,
}

impl Default for GovernanceConfig {
    fn default() -> Self {
        Self {
            use_async: true,
            batch_config: Some(BatchConfig::default()),
            enable_metrics: true,
        }
    }
}

/// Thread-safe wrapper for GovernanceBrain with async support
///
/// Provides backwards-compatible API while enabling high-performance
/// async operations internally.
#[derive(Clone)]
pub struct SharedGovernanceBrain {
    // Internal implementation (async or sync)
    inner: GovernanceImpl,

    // Runtime handle for blocking contexts
    runtime_handle: tokio::runtime::Handle,

    // Configuration
    config: GovernanceConfig,
}

/// Internal implementation variants
#[derive(Clone)]
enum GovernanceImpl {
    /// Legacy sync implementation
    Sync(Arc<std::sync::Mutex<GovernanceBrain>>),

    /// New async implementation
    Async(Arc<AsyncGovernanceBrain>),
}

impl SharedGovernanceBrain {
    /// Create new shared governance brain with configuration
    pub fn new_with_config(brain: GovernanceBrain, config: GovernanceConfig) -> Result<Self> {
        let runtime_handle = tokio::runtime::Handle::try_current().unwrap_or_else(|_| {
            // Create a runtime if not in async context
            tokio::runtime::Runtime::new()
                .expect("Failed to create Tokio runtime")
                .handle()
                .clone()
        });

        let inner = if config.use_async {
            // Convert to async implementation
            // Clone the store (shares the underlying Arc<Store>)
            let store = brain.store().clone();
            let batch_config = config.batch_config.clone().unwrap_or_default();

            // Create async brain - check if we're already in a runtime
            let async_brain = if tokio::runtime::Handle::try_current().is_ok() {
                // Already in a runtime, use block_in_place to allow blocking
                let handle = runtime_handle.clone();
                tokio::task::block_in_place(move || {
                    handle.block_on(
                        async move { AsyncGovernanceBrain::new(store, batch_config).await },
                    )
                })?
            } else {
                // Not in a runtime, safe to block_on directly
                runtime_handle
                    .block_on(async { AsyncGovernanceBrain::new(store, batch_config).await })?
            };

            GovernanceImpl::Async(Arc::new(async_brain))
        } else {
            // Use legacy sync implementation
            GovernanceImpl::Sync(Arc::new(std::sync::Mutex::new(brain)))
        };

        Ok(Self {
            inner,
            runtime_handle,
            config,
        })
    }

    /// Create new shared governance brain (uses async by default)
    pub fn new(brain: GovernanceBrain) -> Self {
        Self::new_with_config(brain, GovernanceConfig::default())
            .expect("Failed to create SharedGovernanceBrain")
    }

    /// Create with sync implementation (for compatibility)
    pub fn new_sync(brain: GovernanceBrain) -> Self {
        let config = GovernanceConfig {
            use_async: false,
            ..Default::default()
        };
        Self::new_with_config(brain, config).expect("Failed to create SharedGovernanceBrain")
    }

    // ========================================================================
    // SYNCHRONOUS API (for backwards compatibility)
    // ========================================================================

    /// Materialize a single lineage event (blocking)
    pub fn materialize_event(&self, event: &LineageEvent) -> Result<()> {
        match &self.inner {
            GovernanceImpl::Sync(brain) => {
                // Original sync implementation
                use super::converters::ToRdfTriples;

                let triples = event.to_rdf_triples()?;
                let brain = brain
                    .lock()
                    .map_err(|e| anyhow::anyhow!("Failed to lock governance brain: {}", e))?;

                for (subject, predicate, object) in triples {
                    brain.insert_lineage_triple(&subject, &predicate, &object)?;
                }

                tracing::debug!("Materialized lineage event {} to RDF", event.id);
                Ok(())
            }
            GovernanceImpl::Async(brain) => {
                // Block on async implementation - check if we're already in a runtime
                if tokio::runtime::Handle::try_current().is_ok() {
                    // Already in a runtime, use block_in_place to allow blocking
                    let brain = brain.clone();
                    let event = event.clone();
                    tokio::task::block_in_place(move || {
                        tokio::runtime::Handle::current().block_on(brain.materialize_event(&event))
                    })
                } else {
                    // Not in a runtime, safe to block_on directly
                    self.runtime_handle.block_on(brain.materialize_event(event))
                }
            }
        }
    }

    /// Execute SPARQL query (blocking)
    pub fn query(&self, sparql: &str) -> Result<Vec<serde_json::Value>> {
        match &self.inner {
            GovernanceImpl::Sync(brain) => {
                let brain = brain
                    .lock()
                    .map_err(|e| anyhow::anyhow!("Failed to lock governance brain: {}", e))?;
                brain.query(sparql)
            }
            GovernanceImpl::Async(brain) => self.runtime_handle.block_on(brain.query(sparql)),
        }
    }

    /// Get triple count (blocking)
    pub fn triple_count(&self) -> Result<usize> {
        match &self.inner {
            GovernanceImpl::Sync(brain) => {
                let brain = brain
                    .lock()
                    .map_err(|e| anyhow::anyhow!("Failed to lock governance brain: {}", e))?;
                brain.store().triple_count().map(|c| c as usize)
            }
            GovernanceImpl::Async(brain) => self.runtime_handle.block_on(brain.triple_count()),
        }
    }

    /// Validate SHACL shapes (blocking)
    pub fn validate_shacl(&self, data_graph: &str, shapes_graph: &str) -> Result<bool> {
        match &self.inner {
            GovernanceImpl::Sync(brain) => {
                let brain = brain
                    .lock()
                    .map_err(|e| anyhow::anyhow!("Failed to lock governance brain: {}", e))?;
                brain.validate_shacl(data_graph, shapes_graph)
            }
            GovernanceImpl::Async(brain) => self
                .runtime_handle
                .block_on(brain.validate_shacl(data_graph, shapes_graph)),
        }
    }

    // ========================================================================
    // ASYNC API (primary interface)
    // ========================================================================

    /// Materialize a lineage event (async)
    pub async fn materialize_lineage_event(&self, event: &LineageEvent) -> Result<()> {
        match &self.inner {
            GovernanceImpl::Sync(brain) => {
                // Fall back to spawning blocking task for sync implementation
                let event_clone = event.clone();
                let brain_clone = brain.clone();

                tokio::task::spawn_blocking(move || {
                    use super::converters::ToRdfTriples;

                    let triples = event_clone.to_rdf_triples()?;
                    let brain = brain_clone
                        .lock()
                        .map_err(|e| anyhow::anyhow!("Failed to lock governance brain: {}", e))?;

                    for (subject, predicate, object) in triples {
                        brain.insert_lineage_triple(&subject, &predicate, &object)?;
                    }

                    Ok(())
                })
                .await
                .map_err(|e| anyhow::anyhow!("Task join error: {}", e))?
            }
            GovernanceImpl::Async(brain) => {
                // Use native async implementation
                brain.materialize_event(event).await
            }
        }
    }

    /// Materialize multiple events efficiently (async batch API)
    pub async fn materialize_batch(&self, events: Vec<LineageEvent>) -> Result<()> {
        match &self.inner {
            GovernanceImpl::Sync(_) => {
                // Fall back to sequential processing for sync impl
                for event in events {
                    self.materialize_lineage_event(&event).await?;
                }
                Ok(())
            }
            GovernanceImpl::Async(brain) => {
                // Use optimized batch API
                brain.materialize_events(events).await
            }
        }
    }

    /// Fire-and-forget materialization (maximum throughput)
    ///
    /// Only available with async implementation
    pub fn materialize_nowait(&self, event: LineageEvent) -> Result<()> {
        match &self.inner {
            GovernanceImpl::Sync(_) => Err(anyhow::anyhow!(
                "Fire-and-forget not supported with sync implementation"
            )),
            GovernanceImpl::Async(brain) => brain.materialize_event_nowait(event),
        }
    }

    /// Execute SPARQL query (async)
    pub async fn query_async(&self, sparql: &str) -> Result<Vec<serde_json::Value>> {
        match &self.inner {
            GovernanceImpl::Sync(brain) => {
                let brain_clone = brain.clone();
                let sparql = sparql.to_string();

                tokio::task::spawn_blocking(move || {
                    let brain = brain_clone
                        .lock()
                        .map_err(|e| anyhow::anyhow!("Failed to lock governance brain: {}", e))?;
                    brain.query(&sparql)
                })
                .await
                .map_err(|e| anyhow::anyhow!("Task join error: {}", e))?
            }
            GovernanceImpl::Async(brain) => brain.query(sparql).await,
        }
    }

    /// Force flush of pending batches (async only)
    pub async fn flush(&self) -> Result<()> {
        match &self.inner {
            GovernanceImpl::Sync(_) => {
                // No-op for sync implementation
                Ok(())
            }
            GovernanceImpl::Async(brain) => brain.flush().await,
        }
    }

    // ========================================================================
    // MONITORING & DIAGNOSTICS
    // ========================================================================

    /// Check if using async implementation
    pub fn is_async(&self) -> bool {
        matches!(self.inner, GovernanceImpl::Async(_))
    }

    /// Get metrics (only available with async implementation)
    pub async fn metrics(&self) -> Option<super::async_brain::GovernanceMetrics> {
        match &self.inner {
            GovernanceImpl::Sync(_) => None,
            GovernanceImpl::Async(brain) => brain.get_metrics().await.ok(),
        }
    }

    /// Get configuration
    pub fn config(&self) -> &GovernanceConfig {
        &self.config
    }

    /// Shutdown gracefully
    ///
    /// Note: Cannot actually shutdown Arc-wrapped brain, only signals intent
    pub async fn shutdown(&self) -> Result<()> {
        match &self.inner {
            GovernanceImpl::Sync(_) => Ok(()),
            GovernanceImpl::Async(_brain) => {
                // Cannot move out of Arc, so we can't call shutdown()
                // This is a limitation of the Arc-wrapped design
                tracing::warn!("SharedGovernanceBrain cannot actually shutdown Arc-wrapped AsyncGovernanceBrain");
                Ok(())
            }
        }
    }

    /// Start background metrics exporter for Prometheus
    ///
    /// Periodically exports ProcessorMetrics to Prometheus metrics.
    /// Only works for async implementation, no-op for sync.
    pub fn start_metrics_exporter(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        use super::prometheus_metrics;
        use std::time::Duration;

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(10));

            loop {
                interval.tick().await;

                // Export mode gauge (1=async, 0=sync)
                prometheus_metrics::set_governance_mode(self.is_async(), "graphica");

                // Export async metrics if available
                if let Some(metrics) = self.metrics().await {
                    prometheus_metrics::update_from_processor_metrics(&metrics, "processor_0");

                    tracing::debug!(
                        "Exported governance metrics: processed={}, failed={}, batches={}",
                        metrics.processed_events,
                        metrics.failed_events,
                        metrics.batches_processed
                    );
                }
            }
        })
    }
}

/// Helper function to materialize a lineage event (compatibility)
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
    fn test_sync_compatibility() {
        let brain = GovernanceBrain::new("./data/test_sync_compat").unwrap();
        let shared = SharedGovernanceBrain::new_sync(brain);

        assert!(!shared.is_async());

        let event = create_test_event();
        assert!(shared.materialize_event(&event).is_ok());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_async_implementation() {
        let brain = GovernanceBrain::new("./data/test_async_impl").unwrap();
        let shared = SharedGovernanceBrain::new(brain);

        assert!(shared.is_async());

        let event = create_test_event();

        // Test async API
        assert!(shared.materialize_lineage_event(&event).await.is_ok());

        // Test sync API (should still work)
        assert!(shared.materialize_event(&event).is_ok());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_batch_api() {
        let brain = GovernanceBrain::new("./data/test_batch_api").unwrap();
        let shared = SharedGovernanceBrain::new(brain);

        let events: Vec<_> = (0..10).map(|_| create_test_event()).collect();

        // Batch API should work with async impl
        assert!(shared.materialize_batch(events).await.is_ok());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_metrics_availability() {
        let temp_dir = tempfile::tempdir().unwrap();

        // Async implementation should have metrics
        let brain_async =
            GovernanceBrain::new(temp_dir.path().join("async").to_str().unwrap()).unwrap();
        let async_shared = SharedGovernanceBrain::new(brain_async);
        assert!(async_shared.metrics().await.is_some());

        // Sync implementation should not have metrics
        let brain_sync =
            GovernanceBrain::new(temp_dir.path().join("sync").to_str().unwrap()).unwrap();
        let sync_shared = SharedGovernanceBrain::new_sync(brain_sync);
        assert!(sync_shared.metrics().await.is_none());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_concurrent_access() {
        let temp_dir = tempfile::tempdir().unwrap();
        let brain = GovernanceBrain::new(temp_dir.path().to_str().unwrap()).unwrap();
        let shared = Arc::new(SharedGovernanceBrain::new(brain));

        let mut handles = vec![];

        // Spawn concurrent tasks
        for i in 0..10 {
            let shared_clone = shared.clone();
            let handle = tokio::spawn(async move {
                let mut event = create_test_event();
                event.record_id = format!("concurrent_{}", i);
                shared_clone.materialize_lineage_event(&event).await
            });
            handles.push(handle);
        }

        // All should succeed
        for handle in handles {
            assert!(handle.await.unwrap().is_ok());
        }
    }
}
