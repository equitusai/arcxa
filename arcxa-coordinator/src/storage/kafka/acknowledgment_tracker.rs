//! Acknowledgment tracking for Kafka delivery confirmations
//!
//! Tracks which lineage events have been successfully delivered to Kafka
//! and manages cleanup of acknowledged entries from the WAL.

use dashmap::DashMap;
use graphica_core::core::lineage::LineageEvent;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::storage::wal::{LogSequenceNumber, WriteAheadLog};

use super::config::AckTrackingConfig;

/// Status of a pending acknowledgment
#[derive(Debug, Clone)]
pub struct PendingAck {
    /// Unique event ID (for idempotency)
    pub event_id: Uuid,

    /// WAL LSN for this event
    pub lsn: LogSequenceNumber,

    /// The lineage event itself (for replay)
    pub event: LineageEvent,

    /// When was this event written
    pub written_at: Instant,

    /// Kafka partition assigned
    pub partition: Option<i32>,

    /// Kafka offset assigned
    pub offset: Option<i64>,

    /// Number of retry attempts
    pub retry_count: u32,
}

impl PendingAck {
    pub fn new(event_id: Uuid, lsn: LogSequenceNumber, event: LineageEvent) -> Self {
        Self {
            event_id,
            lsn,
            event,
            written_at: Instant::now(),
            partition: None,
            offset: None,
            retry_count: 0,
        }
    }

    pub fn with_kafka_location(mut self, partition: i32, offset: i64) -> Self {
        self.partition = Some(partition);
        self.offset = Some(offset);
        self
    }

    pub fn age(&self) -> Duration {
        self.written_at.elapsed()
    }

    pub fn increment_retry(&mut self) {
        self.retry_count += 1;
    }
}

/// Tracks acknowledgments for in-flight Kafka sends
pub struct AcknowledgmentTracker {
    /// Pending acknowledgments: event_id -> PendingAck
    pending: Arc<DashMap<Uuid, PendingAck>>,

    /// Acknowledged events (for deduplication): event_id -> (acked_at, LSN)
    acknowledged: Arc<DashMap<Uuid, (Instant, LogSequenceNumber)>>,

    /// WAL for committing acknowledged entries
    wal: Arc<dyn WriteAheadLog>,

    /// Configuration
    config: AckTrackingConfig,

    /// Metrics
    total_acknowledged: Arc<std::sync::atomic::AtomicU64>,
    total_pending: Arc<std::sync::atomic::AtomicU64>,
}

impl AcknowledgmentTracker {
    /// Create new acknowledgment tracker
    pub fn new(wal: Arc<dyn WriteAheadLog>, config: AckTrackingConfig) -> Self {
        Self {
            pending: Arc::new(DashMap::new()),
            acknowledged: Arc::new(DashMap::new()),
            wal,
            config,
            total_acknowledged: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            total_pending: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    /// Track a new pending acknowledgment
    pub fn track_pending(&self, event_id: Uuid, lsn: LogSequenceNumber, event: LineageEvent) {
        let ack = PendingAck::new(event_id, lsn, event);
        self.pending.insert(event_id, ack);
        self.total_pending
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        debug!(
            "Tracking pending ack for event {} at LSN {}",
            event_id, lsn.0
        );
    }

    /// Mark an event as acknowledged
    pub async fn mark_acknowledged(
        &self,
        event_id: Uuid,
        partition: i32,
        offset: i64,
    ) -> anyhow::Result<()> {
        if let Some((_, mut pending)) = self.pending.remove(&event_id) {
            pending.partition = Some(partition);
            pending.offset = Some(offset);

            // Store in acknowledged map for deduplication
            self.acknowledged
                .insert(event_id, (Instant::now(), pending.lsn));

            // Commit to WAL (entry can be compacted)
            self.wal.commit(pending.lsn).await?;

            self.total_acknowledged
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            self.total_pending
                .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);

            debug!(
                "Acknowledged event {} (partition={}, offset={}, age={:?})",
                event_id,
                partition,
                offset,
                pending.age()
            );

            Ok(())
        } else {
            warn!("Received ack for unknown event: {}", event_id);
            Ok(())
        }
    }

    /// Mark an event as failed (will retry)
    pub fn mark_failed(&self, event_id: Uuid) {
        if let Some(mut entry) = self.pending.get_mut(&event_id) {
            entry.increment_retry();
            warn!("Event {} failed (retry #{})", event_id, entry.retry_count);
        }
    }

    /// Check if event is already acknowledged (deduplication)
    pub fn is_acknowledged(&self, event_id: &Uuid) -> bool {
        self.acknowledged.contains_key(event_id)
    }

    /// Get all pending acknowledgments (for recovery)
    pub fn get_pending(&self) -> Vec<PendingAck> {
        self.pending
            .iter()
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// Get pending count
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Get acknowledged count
    pub fn acknowledged_count(&self) -> usize {
        self.acknowledged.len()
    }

    /// Get oldest pending event age
    pub fn oldest_pending_age(&self) -> Option<Duration> {
        self.pending.iter().map(|entry| entry.value().age()).max()
    }

    /// Cleanup old acknowledged entries
    pub async fn cleanup_acknowledged(&self) -> anyhow::Result<usize> {
        let cutoff = Instant::now() - self.config.ack_retention;
        let mut cleaned = 0;

        self.acknowledged.retain(|event_id, (acked_at, _lsn)| {
            if *acked_at < cutoff {
                debug!("Cleaning up old ack for event {}", event_id);
                cleaned += 1;
                false
            } else {
                true
            }
        });

        if cleaned > 0 {
            info!("Cleaned up {} old acknowledgments", cleaned);
        }

        Ok(cleaned)
    }

    /// Start background cleanup task
    pub fn start_cleanup_task(self: Arc<Self>) {
        let tracker = Arc::clone(&self);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tracker.config.cleanup_interval);

            loop {
                interval.tick().await;
                if let Err(e) = tracker.cleanup_acknowledged().await {
                    warn!("Cleanup task failed: {}", e);
                }
            }
        });
    }

    /// Check if we're at capacity (backpressure)
    pub fn is_at_capacity(&self) -> bool {
        self.pending_count() >= self.config.max_pending
    }

    /// Get metrics snapshot
    pub fn metrics_snapshot(&self) -> AckMetrics {
        AckMetrics {
            pending_count: self.pending_count(),
            acknowledged_count: self.acknowledged_count(),
            total_acknowledged: self
                .total_acknowledged
                .load(std::sync::atomic::Ordering::Relaxed),
            total_pending: self
                .total_pending
                .load(std::sync::atomic::Ordering::Relaxed),
            oldest_pending_age: self.oldest_pending_age(),
        }
    }
}

/// Metrics snapshot for acknowledgment tracker
#[derive(Debug, Clone)]
pub struct AckMetrics {
    pub pending_count: usize,
    pub acknowledged_count: usize,
    pub total_acknowledged: u64,
    pub total_pending: u64,
    pub oldest_pending_age: Option<Duration>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::wal::{FileWal, WalConfig, WalMetricsCollector};
    use tempfile::TempDir;

    async fn create_test_tracker() -> (AcknowledgmentTracker, TempDir) {
        use std::sync::atomic::{AtomicU64, Ordering};
        static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

        let test_id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        let temp_dir = TempDir::new().unwrap();
        let wal_config = WalConfig::default().with_path(temp_dir.path().to_path_buf());
        let metrics = Arc::new(WalMetricsCollector::new(&format!(
            "test_ack_tracker_{}",
            test_id
        )));
        let wal: Arc<dyn WriteAheadLog> =
            Arc::new(FileWal::new(wal_config.clone(), metrics).await.unwrap());

        let config = AckTrackingConfig::default();
        let tracker = AcknowledgmentTracker::new(wal, config);

        (tracker, temp_dir)
    }

    fn create_test_event() -> LineageEvent {
        use chrono::Utc;
        use graphica_core::core::lineage::DataRef;
        use std::collections::HashMap;

        LineageEvent {
            id: Uuid::new_v4(),
            dataset: "test".to_string(),
            record_id: "test_record".to_string(),
            source_refs: vec![DataRef {
                system: "test_system".to_string(),
                path: "test/path".to_string(),
                version: None,
                extracted_at: Utc::now(),
                cdc_position: None,
            }],
            transforms: vec![],
            model_refs: vec![],
            output_ref: DataRef {
                system: "output".to_string(),
                path: "out".to_string(),
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
    async fn test_track_pending() {
        let (tracker, _dir) = create_test_tracker().await;

        let event_id = Uuid::new_v4();
        let lsn = LogSequenceNumber(42);
        let event = create_test_event();

        tracker.track_pending(event_id, lsn, event);

        assert_eq!(tracker.pending_count(), 1);
        assert!(!tracker.is_acknowledged(&event_id));
    }

    #[tokio::test]
    async fn test_mark_acknowledged() {
        let (tracker, _dir) = create_test_tracker().await;

        let event_id = Uuid::new_v4();
        let lsn = LogSequenceNumber(42);
        let event = create_test_event();

        tracker.track_pending(event_id, lsn, event);
        tracker.mark_acknowledged(event_id, 0, 100).await.unwrap();

        assert_eq!(tracker.pending_count(), 0);
        assert!(tracker.is_acknowledged(&event_id));
        assert_eq!(tracker.acknowledged_count(), 1);
    }

    #[tokio::test]
    async fn test_mark_failed() {
        let (tracker, _dir) = create_test_tracker().await;

        let event_id = Uuid::new_v4();
        let lsn = LogSequenceNumber(42);
        let event = create_test_event();

        tracker.track_pending(event_id, lsn, event);
        tracker.mark_failed(event_id);

        let pending = tracker.get_pending();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].retry_count, 1);
    }

    #[tokio::test]
    async fn test_deduplication() {
        let (tracker, _dir) = create_test_tracker().await;

        let event_id = Uuid::new_v4();
        let lsn = LogSequenceNumber(42);
        let event = create_test_event();

        tracker.track_pending(event_id, lsn, event);
        tracker.mark_acknowledged(event_id, 0, 100).await.unwrap();

        // Should be marked as already acknowledged
        assert!(tracker.is_acknowledged(&event_id));
    }

    #[tokio::test]
    async fn test_cleanup_acknowledged() {
        let (tracker, _dir) = create_test_tracker().await;

        let event_id = Uuid::new_v4();
        let lsn = LogSequenceNumber(42);
        let event = create_test_event();

        tracker.track_pending(event_id, lsn, event);
        tracker.mark_acknowledged(event_id, 0, 100).await.unwrap();

        // Immediate cleanup shouldn't remove (within retention window)
        let cleaned = tracker.cleanup_acknowledged().await.unwrap();
        assert_eq!(cleaned, 0);
        assert_eq!(tracker.acknowledged_count(), 1);
    }

    #[tokio::test]
    async fn test_oldest_pending_age() {
        let (tracker, _dir) = create_test_tracker().await;

        let event_id = Uuid::new_v4();
        let lsn = LogSequenceNumber(42);
        let event = create_test_event();

        tracker.track_pending(event_id, lsn, event);

        tokio::time::sleep(Duration::from_millis(100)).await;

        let age = tracker.oldest_pending_age().unwrap();
        assert!(age >= Duration::from_millis(100));
    }

    #[tokio::test]
    async fn test_backpressure_at_capacity() {
        use std::sync::atomic::{AtomicU64, Ordering};
        static TEST_COUNTER: AtomicU64 = AtomicU64::new(1000);
        let test_id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);

        let temp_dir = TempDir::new().unwrap();
        let wal_config = WalConfig::default().with_path(temp_dir.path().to_path_buf());
        let metrics = Arc::new(WalMetricsCollector::new(&format!(
            "test_backpressure_{}",
            test_id
        )));
        let wal: Arc<dyn WriteAheadLog> =
            Arc::new(FileWal::new(wal_config.clone(), metrics).await.unwrap());

        let mut config = AckTrackingConfig::default();
        config.max_pending = 2;

        let tracker = AcknowledgmentTracker::new(wal, config);

        tracker.track_pending(Uuid::new_v4(), LogSequenceNumber(1), create_test_event());
        tracker.track_pending(Uuid::new_v4(), LogSequenceNumber(2), create_test_event());

        assert!(tracker.is_at_capacity());
    }

    #[tokio::test]
    async fn test_metrics_snapshot() {
        let (tracker, _dir) = create_test_tracker().await;

        let event_id1 = Uuid::new_v4();
        let event_id2 = Uuid::new_v4();

        tracker.track_pending(event_id1, LogSequenceNumber(1), create_test_event());
        tracker.track_pending(event_id2, LogSequenceNumber(2), create_test_event());
        tracker.mark_acknowledged(event_id1, 0, 100).await.unwrap();

        let metrics = tracker.metrics_snapshot();
        assert_eq!(metrics.pending_count, 1);
        assert_eq!(metrics.acknowledged_count, 1);
        assert_eq!(metrics.total_acknowledged, 1);
    }
}
