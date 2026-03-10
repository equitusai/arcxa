//! # Async Brain Core Types
//!
//! Message types and shared state for async governance brain communication.
//! Enables batched materialization with channel-based coordination.

use crate::governance::async_config::AsyncGovernanceConfig;
use graphica_core::core::lineage::LineageEvent;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{oneshot, RwLock};

/// Results from a SPARQL query
pub type QueryResults = Vec<serde_json::Value>;

/// Messages for async channel communication between governance brain components
#[derive(Debug)]
pub enum GovernanceMessage {
    /// Single event to materialize
    MaterializeEvent(LineageEvent),

    /// Batch of events to process together
    ProcessBatch(Vec<LineageEvent>),

    /// SPARQL query with response channel
    Query {
        sparql: String,
        response: oneshot::Sender<anyhow::Result<QueryResults>>,
    },

    /// Get processor metrics
    GetMetrics {
        response: oneshot::Sender<ProcessorMetrics>,
    },

    /// Graceful shutdown signal
    Shutdown,
}

/// Metrics for monitoring batch processor performance
#[derive(Debug, Clone, Default)]
pub struct ProcessorMetrics {
    /// Total events processed successfully
    pub processed_events: u64,

    /// Events that failed to process
    pub failed_events: u64,

    /// Total batches processed
    pub batches_processed: u64,

    /// Average batch size
    pub avg_batch_size: f64,

    /// Last flush duration in milliseconds
    pub last_flush_ms: u64,
}

impl ProcessorMetrics {
    /// Create new metrics with zero values
    pub fn new() -> Self {
        Self::default()
    }

    /// Update metrics after processing a batch
    pub fn record_batch(&mut self, batch_size: usize, flush_duration: Duration, failed: usize) {
        let succeeded = batch_size.saturating_sub(failed);
        self.processed_events += succeeded as u64;
        self.failed_events += failed as u64;
        self.batches_processed += 1;
        self.last_flush_ms = flush_duration.as_millis() as u64;

        // Calculate running average batch size
        let total_events = self.processed_events + self.failed_events;
        self.avg_batch_size = if self.batches_processed > 0 {
            total_events as f64 / self.batches_processed as f64
        } else {
            0.0
        };
    }

    /// Get total events (processed + failed)
    pub fn total_events(&self) -> u64 {
        self.processed_events + self.failed_events
    }

    /// Get success rate as percentage
    pub fn success_rate(&self) -> f64 {
        let total = self.total_events();
        if total == 0 {
            0.0
        } else {
            (self.processed_events as f64 / total as f64) * 100.0
        }
    }
}

/// Accumulates events before batch processing
#[derive(Debug)]
pub struct EventBatch {
    /// Accumulated events
    events: Vec<LineageEvent>,

    /// When batch started accumulating
    created_at: Instant,
}

impl EventBatch {
    /// Create new empty batch
    pub fn new() -> Self {
        Self {
            events: Vec::new(),
            created_at: Instant::now(),
        }
    }

    /// Add event to batch
    pub fn add(&mut self, event: LineageEvent) {
        self.events.push(event);
    }

    /// Get number of events in batch
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Check if batch is empty
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Get time since batch creation
    pub fn age(&self) -> Duration {
        self.created_at.elapsed()
    }

    /// Take all events and reset batch
    pub fn drain(&mut self) -> Vec<LineageEvent> {
        let events = std::mem::take(&mut self.events);
        self.created_at = Instant::now();
        events
    }

    /// Check if batch should be flushed based on config
    pub fn should_flush(&self, config: &AsyncGovernanceConfig) -> bool {
        self.events.len() >= config.batch_size || self.age() >= config.batch_timeout
    }
}

impl Default for EventBatch {
    fn default() -> Self {
        Self::new()
    }
}

/// Shared state for async brain
#[derive(Debug, Clone)]
pub struct AsyncBrainState {
    /// Configuration for async processing
    pub config: AsyncGovernanceConfig,

    /// Processor metrics with async-safe access
    pub metrics: Arc<RwLock<ProcessorMetrics>>,
}

impl AsyncBrainState {
    /// Create new shared state with config
    pub fn new(config: AsyncGovernanceConfig) -> Self {
        Self {
            config,
            metrics: Arc::new(RwLock::new(ProcessorMetrics::new())),
        }
    }

    /// Get snapshot of current metrics
    pub async fn get_metrics(&self) -> ProcessorMetrics {
        self.metrics.read().await.clone()
    }

    /// Update metrics after batch processing
    pub async fn record_batch(&self, batch_size: usize, flush_duration: Duration, failed: usize) {
        let mut metrics = self.metrics.write().await;
        metrics.record_batch(batch_size, flush_duration, failed);
    }
}

impl Default for AsyncBrainState {
    fn default() -> Self {
        Self::new(AsyncGovernanceConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::collections::HashMap;
    use std::thread;
    use uuid::Uuid;

    fn create_test_event(record_id: &str) -> LineageEvent {
        use graphica_core::core::lineage::DataRef;

        LineageEvent {
            id: Uuid::new_v4(),
            dataset: "test_dataset".to_string(),
            record_id: record_id.to_string(),
            source_refs: vec![],
            transforms: vec![],
            model_refs: vec![],
            output_ref: DataRef {
                system: "test_system".to_string(),
                path: "test/path".to_string(),
                version: Some("v1".to_string()),
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
    fn test_processor_metrics_new() {
        let metrics = ProcessorMetrics::new();
        assert_eq!(metrics.processed_events, 0);
        assert_eq!(metrics.failed_events, 0);
        assert_eq!(metrics.batches_processed, 0);
        assert_eq!(metrics.avg_batch_size, 0.0);
        assert_eq!(metrics.last_flush_ms, 0);
    }

    #[test]
    fn test_processor_metrics_record_batch() {
        let mut metrics = ProcessorMetrics::new();

        // Record first batch: 100 events, 0 failed, 50ms
        metrics.record_batch(100, Duration::from_millis(50), 0);
        assert_eq!(metrics.processed_events, 100);
        assert_eq!(metrics.failed_events, 0);
        assert_eq!(metrics.batches_processed, 1);
        assert_eq!(metrics.avg_batch_size, 100.0);
        assert_eq!(metrics.last_flush_ms, 50);

        // Record second batch: 200 events, 10 failed, 75ms
        metrics.record_batch(200, Duration::from_millis(75), 10);
        assert_eq!(metrics.processed_events, 290); // 100 + 190
        assert_eq!(metrics.failed_events, 10);
        assert_eq!(metrics.batches_processed, 2);
        assert_eq!(metrics.avg_batch_size, 150.0); // (100 + 200) / 2
        assert_eq!(metrics.last_flush_ms, 75);
    }

    #[test]
    fn test_processor_metrics_success_rate() {
        let mut metrics = ProcessorMetrics::new();

        // No events yet
        assert_eq!(metrics.success_rate(), 0.0);

        // 80 success, 20 failed = 80% success rate
        metrics.record_batch(100, Duration::from_millis(50), 20);
        assert_eq!(metrics.success_rate(), 80.0);

        // Add more: total 180 success, 30 failed = ~85.7% success rate
        metrics.record_batch(110, Duration::from_millis(60), 10);
        assert!((metrics.success_rate() - 85.71428571428571).abs() < 0.0001);
    }

    #[test]
    fn test_event_batch_new() {
        let batch = EventBatch::new();
        assert_eq!(batch.len(), 0);
        assert!(batch.is_empty());
    }

    #[test]
    fn test_event_batch_add() {
        let mut batch = EventBatch::new();
        batch.add(create_test_event("rec1"));
        batch.add(create_test_event("rec2"));

        assert_eq!(batch.len(), 2);
        assert!(!batch.is_empty());
    }

    #[test]
    fn test_event_batch_age() {
        let batch = EventBatch::new();
        thread::sleep(Duration::from_millis(10));
        let age = batch.age();
        assert!(age >= Duration::from_millis(10));
        assert!(age < Duration::from_millis(100)); // Should be quick
    }

    #[test]
    fn test_event_batch_drain() {
        let mut batch = EventBatch::new();
        batch.add(create_test_event("rec1"));
        batch.add(create_test_event("rec2"));
        batch.add(create_test_event("rec3"));

        let events = batch.drain();
        assert_eq!(events.len(), 3);
        assert_eq!(batch.len(), 0);
        assert!(batch.is_empty());

        // Age should be reset after drain
        assert!(batch.age() < Duration::from_millis(10));
    }

    #[test]
    fn test_event_batch_should_flush_by_size() {
        let mut batch = EventBatch::new();
        let config = AsyncGovernanceConfig {
            batch_size: 3,
            batch_timeout: Duration::from_secs(10),
            ..Default::default()
        };

        assert!(!batch.should_flush(&config));

        batch.add(create_test_event("rec1"));
        batch.add(create_test_event("rec2"));
        assert!(!batch.should_flush(&config));

        batch.add(create_test_event("rec3"));
        assert!(batch.should_flush(&config)); // Reached batch_size
    }

    #[test]
    fn test_event_batch_should_flush_by_timeout() {
        let mut batch = EventBatch::new();
        let config = AsyncGovernanceConfig {
            batch_size: 100,
            batch_timeout: Duration::from_millis(10),
            ..Default::default()
        };

        batch.add(create_test_event("rec1"));
        assert!(!batch.should_flush(&config));

        thread::sleep(Duration::from_millis(15));
        assert!(batch.should_flush(&config)); // Exceeded timeout
    }

    #[test]
    fn test_async_brain_state_new() {
        let config = AsyncGovernanceConfig::default();
        let state = AsyncBrainState::new(config.clone());

        assert_eq!(state.config.batch_size, config.batch_size);
        assert_eq!(state.config.batch_timeout, config.batch_timeout);
    }

    #[tokio::test]
    async fn test_async_brain_state_metrics() {
        let state = AsyncBrainState::default();

        // Initial metrics
        let metrics = state.get_metrics().await;
        assert_eq!(metrics.processed_events, 0);

        // Record batch
        state.record_batch(50, Duration::from_millis(25), 5).await;

        // Check updated metrics
        let metrics = state.get_metrics().await;
        assert_eq!(metrics.processed_events, 45); // 50 - 5 failed
        assert_eq!(metrics.failed_events, 5);
        assert_eq!(metrics.batches_processed, 1);
        assert_eq!(metrics.last_flush_ms, 25);
    }

    #[tokio::test]
    async fn test_async_brain_state_concurrent_access() {
        let state = AsyncBrainState::default();
        let state2 = state.clone();

        // Spawn concurrent batch recordings
        let handle1 = tokio::spawn(async move {
            state.record_batch(100, Duration::from_millis(50), 10).await;
        });

        let handle2 = tokio::spawn(async move {
            state2
                .record_batch(150, Duration::from_millis(75), 15)
                .await;
        });

        handle1.await.unwrap();
        handle2.await.unwrap();
    }
}
