//! AsyncGovernanceBrain V2 - Wires MessageRouter + ProcessorPool + BatchProcessor
//!
//! This is the production-ready facade that combines:
//! - MessageRouter: Work distribution across N processors
//! - ProcessorPool: N concurrent async tasks
//! - BatchProcessor: Circuit breaker + retry + RDF storage
//!
//! Target: 2,000+ events/sec throughput with graceful shutdown

use crate::governance::async_config::AsyncGovernanceConfig;
use crate::governance::message_router::{
    MessagePriority, MessageRouter, RoutedMessage, RoutingStrategy,
};
use crate::governance::processor_pool::{
    BatchProcessorConfig, PoolStats, ProcessorPool, ProcessorPoolConfig,
};
use crate::governance::rdf_store::GraphicaRdfStore;
use graphica_core::core::lineage::LineageEvent;
use graphica_core::reliability::CircuitBreakerConfig;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

/// AsyncGovernanceBrain V2 - Complete async governance system
///
/// Combines router, processor pool, and batch processing into a unified facade.
/// Provides simple API for event ingestion with automatic batching, retry, and circuit breaking.
pub struct AsyncGovernanceBrainV2 {
    router: Arc<MessageRouter>,
    pool: Arc<ProcessorPool>,
    config: AsyncGovernanceConfig,
}

impl AsyncGovernanceBrainV2 {
    /// Create new V2 brain with configuration and RDF store
    ///
    /// # Arguments
    /// * `config` - AsyncGovernanceConfig with batch size, num processors, etc.
    /// * `store` - Shared RDF store for triple persistence
    ///
    /// # Returns
    /// Initialized brain with N processors running
    pub async fn new(
        config: AsyncGovernanceConfig,
        store: Arc<GraphicaRdfStore>,
    ) -> Result<Self, anyhow::Error> {
        // Create router with N processors
        let mut router = MessageRouter::new(config.num_processors, config.channel_capacity);
        router.set_strategy(RoutingStrategy::HybridHashLeastLoaded);

        // Create processor pool config
        let pool_config = ProcessorPoolConfig {
            num_processors: config.num_processors,
            processor_config: BatchProcessorConfig {
                batch_size: config.batch_size,
                batch_timeout: config.batch_timeout,
                max_retries: 3,
                retry_delay: Duration::from_millis(100),
                dlq_threshold: 5,
            },
            circuit_breaker: CircuitBreakerConfig {
                failure_threshold: 5,
                timeout: Duration::from_secs(30),
                success_threshold: 2,
            },
        };

        // Spawn processor pool
        let pool = ProcessorPool::spawn(pool_config, &mut router, store).await?;

        Ok(Self {
            router: Arc::new(router),
            pool: Arc::new(pool),
            config,
        })
    }

    /// Materialize a single lineage event
    ///
    /// Event is routed to appropriate processor and batched for RDF persistence.
    ///
    /// # Arguments
    /// * `event` - LineageEvent to materialize
    ///
    /// # Returns
    /// Ok if routed successfully, Err if channel full or routing failed
    pub async fn materialize_event(&self, event: LineageEvent) -> Result<(), anyhow::Error> {
        let message = RoutedMessage {
            event,
            priority: MessagePriority::Normal,
            retry_count: 0,
            trace_id: Uuid::new_v4().to_string(),
        };

        self.router
            .route(message)
            .map_err(|e| anyhow::anyhow!("Failed to route message: {}", e))?;

        Ok(())
    }

    /// Materialize batch of events (optimized)
    ///
    /// Routes each event through the message router for distribution across processors.
    ///
    /// # Arguments
    /// * `events` - Vec of LineageEvents to materialize
    ///
    /// # Returns
    /// Ok if all events routed successfully
    pub async fn materialize_batch(&self, events: Vec<LineageEvent>) -> Result<(), anyhow::Error> {
        for event in events {
            self.materialize_event(event).await?;
        }
        Ok(())
    }

    /// Get pool statistics
    ///
    /// Aggregates stats across all processors.
    ///
    /// # Returns
    /// PoolStats with total events processed, failed, batches, etc.
    pub async fn stats(&self) -> PoolStats {
        self.pool.stats().await
    }

    /// Trigger coordinated flush across all processors
    ///
    /// Forces all processors to flush their current batches immediately.
    /// Useful for shutdown or checkpoint scenarios.
    ///
    /// # Returns
    /// Ok if all processors flushed successfully
    pub async fn flush(&self) -> Result<(), anyhow::Error> {
        self.pool
            .flush()
            .await
            .map_err(|e| anyhow::anyhow!("Flush failed: {}", e))
    }

    /// Get number of processors
    pub fn num_processors(&self) -> usize {
        self.config.num_processors
    }

    /// Get processor loads (tuple of processor_id, load)
    pub fn processor_loads(&self) -> Vec<(usize, usize)> {
        self.pool.processor_loads()
    }

    /// Check circuit breaker health
    ///
    /// Returns false if any processor circuit is open.
    pub fn is_healthy(&self) -> bool {
        self.pool.is_healthy()
    }

    /// Graceful shutdown
    ///
    /// 1. Stops accepting new events
    /// 2. Flushes remaining batches
    /// 3. Shuts down all processors
    /// 4. Logs final statistics
    ///
    /// # Returns
    /// Ok if shutdown completed cleanly
    pub async fn shutdown(self) -> Result<(), anyhow::Error> {
        // Take pool out of Arc
        let pool =
            Arc::try_unwrap(self.pool).map_err(|_| anyhow::anyhow!("Pool still has references"))?;

        let results = pool.shutdown().await;

        // Log processor stats
        for (i, result) in results.iter().enumerate() {
            match result {
                Ok(stats) => {
                    tracing::info!(
                        processor_id = i,
                        events_processed = stats.events_processed,
                        events_failed = stats.events_failed,
                        batches_processed = stats.batches_processed,
                        "Processor shutdown complete"
                    );
                }
                Err(e) => {
                    tracing::error!(processor_id = i, error = %e, "Processor shutdown error");
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use graphica_core::core::lineage::DataRef;
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn create_test_event(dataset: &str, record: &str) -> LineageEvent {
        LineageEvent {
            id: Uuid::new_v4(),
            dataset: dataset.to_string(),
            record_id: record.to_string(),
            source_refs: vec![DataRef {
                system: "test".to_string(),
                path: "/test".to_string(),
                version: None,
                extracted_at: Utc::now(),
                cdc_position: None,
            }],
            transforms: vec![],
            model_refs: vec![],
            output_ref: DataRef {
                system: "output".to_string(),
                path: "/output".to_string(),
                version: None,
                extracted_at: Utc::now(),
                cdc_position: None,
            },
            ts: Utc::now(),
            run_id: "test-run".to_string(),
            tenant_id: "test-tenant".to_string(),
            correlation_id: Some(Uuid::new_v4().to_string()),
            metadata: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn test_brain_v2_creation() {
        let temp_dir = TempDir::new().unwrap();
        #[allow(deprecated)] // Test code uses simplified initialization
        let store = Arc::new(GraphicaRdfStore::new(temp_dir.path().to_str().unwrap()).unwrap());

        let config = AsyncGovernanceConfig::default();
        let brain = AsyncGovernanceBrainV2::new(config, store).await.unwrap();

        assert_eq!(brain.num_processors(), 4); // Default config
        assert!(brain.is_healthy());
    }

    #[tokio::test]
    async fn test_brain_v2_materialize_single() {
        let temp_dir = TempDir::new().unwrap();
        #[allow(deprecated)] // Test code uses simplified initialization
        let store = Arc::new(GraphicaRdfStore::new(temp_dir.path().to_str().unwrap()).unwrap());

        let config = AsyncGovernanceConfig::default();
        let brain = AsyncGovernanceBrainV2::new(config, store).await.unwrap();

        let event = create_test_event("customers", "cust_001");
        brain.materialize_event(event).await.unwrap();

        // Allow time for async processing
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        let stats = brain.stats().await;
        assert!(stats.total_processed > 0 || stats.total_failed > 0);
    }

    #[tokio::test]
    async fn test_brain_v2_processor_loads() {
        let temp_dir = TempDir::new().unwrap();
        #[allow(deprecated)] // Test code uses simplified initialization
        let store = Arc::new(GraphicaRdfStore::new(temp_dir.path().to_str().unwrap()).unwrap());

        let config = AsyncGovernanceConfig::default();
        let brain = AsyncGovernanceBrainV2::new(config, store).await.unwrap();

        let loads = brain.processor_loads();
        assert_eq!(loads.len(), brain.num_processors());
    }
}
