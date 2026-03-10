//! Replay manager for recovering unacknowledged Kafka events on startup
//!
//! When the coordinator starts, the ReplayManager scans the WAL for any
//! lineage events that were written but never acknowledged by Kafka.
//! These events are then replayed in batches with rate limiting.
//!
//! # Recovery Process
//!
//! 1. Scan WAL for unacknowledged lineage events
//! 2. Batch events (default: 500 per batch)
//! 3. Replay each batch with retry logic
//! 4. Rate limit between batches (default: 1 second)
//! 5. Report recovery statistics

use anyhow::{Context, Result};
use graphica_core::core::lineage::{LineageEvent, LineageSink};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

use crate::storage::wal::WriteAheadLog;

use super::acknowledgment_tracker::AcknowledgmentTracker;
use super::durable_sink::DurableKafkaLineageSink;

/// Configuration for replay behavior
#[derive(Debug, Clone)]
pub struct ReplayConfig {
    /// Number of events per replay batch
    pub batch_size: usize,

    /// Delay between batches (rate limiting)
    pub batch_interval: Duration,

    /// Maximum retry attempts for failed batches
    pub max_retry_attempts: u32,

    /// Initial backoff for retries
    pub retry_backoff: Duration,

    /// Maximum backoff for retries
    pub max_backoff: Duration,
}

impl Default for ReplayConfig {
    fn default() -> Self {
        Self {
            batch_size: 500,
            batch_interval: Duration::from_secs(1),
            max_retry_attempts: 3,
            retry_backoff: Duration::from_secs(1),
            max_backoff: Duration::from_secs(60),
        }
    }
}

impl ReplayConfig {
    /// Aggressive replay (faster recovery)
    pub fn aggressive() -> Self {
        Self {
            batch_size: 1000,
            batch_interval: Duration::from_millis(100),
            max_retry_attempts: 5,
            retry_backoff: Duration::from_millis(500),
            max_backoff: Duration::from_secs(30),
        }
    }

    /// Conservative replay (lower load)
    pub fn conservative() -> Self {
        Self {
            batch_size: 100,
            batch_interval: Duration::from_secs(5),
            max_retry_attempts: 3,
            retry_backoff: Duration::from_secs(2),
            max_backoff: Duration::from_secs(120),
        }
    }
}

/// Statistics from a recovery operation
#[derive(Debug, Clone)]
pub struct RecoveryReport {
    /// Total events found in WAL
    pub total_events: usize,

    /// Successfully replayed events
    pub replayed_events: usize,

    /// Failed events (after all retries)
    pub failed_events: usize,

    /// Error messages for failed batches
    pub failures: Vec<String>,

    /// Total recovery duration
    pub duration: Duration,

    /// Number of batches processed
    pub batches_processed: usize,

    /// Number of retry attempts
    pub retry_attempts: usize,
}

impl RecoveryReport {
    /// Check if recovery was fully successful
    pub fn is_successful(&self) -> bool {
        self.failed_events == 0
    }

    /// Get success rate
    pub fn success_rate(&self) -> f64 {
        if self.total_events == 0 {
            1.0
        } else {
            self.replayed_events as f64 / self.total_events as f64
        }
    }
}

/// Manages replay of unacknowledged events from WAL
pub struct ReplayManager {
    /// WAL for reading unacknowledged events
    wal: Arc<dyn WriteAheadLog>,

    /// Kafka sink for replaying events
    kafka_sink: Arc<DurableKafkaLineageSink>,

    /// Acknowledgment tracker to check what's already acknowledged
    ack_tracker: Arc<AcknowledgmentTracker>,

    /// Configuration
    config: ReplayConfig,
}

impl ReplayManager {
    /// Create new replay manager
    pub fn new(
        wal: Arc<dyn WriteAheadLog>,
        kafka_sink: Arc<DurableKafkaLineageSink>,
        ack_tracker: Arc<AcknowledgmentTracker>,
        config: ReplayConfig,
    ) -> Self {
        info!(
            "Creating replay manager: batch_size={}, interval={:?}",
            config.batch_size, config.batch_interval
        );

        Self {
            wal,
            kafka_sink,
            ack_tracker,
            config,
        }
    }

    /// Recover unacknowledged events on startup
    pub async fn recover_on_startup(&self) -> Result<RecoveryReport> {
        info!("Starting Kafka recovery process");
        let start = Instant::now();

        // Step 1: Find unacknowledged events in WAL
        let unacked_events = self.find_unacknowledged_events().await?;

        info!(
            "Found {} unacknowledged events to replay",
            unacked_events.len()
        );

        if unacked_events.is_empty() {
            return Ok(RecoveryReport {
                total_events: 0,
                replayed_events: 0,
                failed_events: 0,
                failures: Vec::new(),
                duration: start.elapsed(),
                batches_processed: 0,
                retry_attempts: 0,
            });
        }

        // Step 2: Replay in batches with rate limiting
        let mut total_replayed = 0;
        let mut total_failed = 0;
        let mut failures = Vec::new();
        let mut batches_processed = 0;
        let mut retry_attempts = 0;

        for (batch_idx, chunk) in unacked_events.chunks(self.config.batch_size).enumerate() {
            debug!(
                "Processing batch {}/{} ({} events)",
                batch_idx + 1,
                (unacked_events.len() + self.config.batch_size - 1) / self.config.batch_size,
                chunk.len()
            );

            match self.replay_batch_with_retry(chunk).await {
                Ok((replayed, retries)) => {
                    total_replayed += replayed;
                    retry_attempts += retries;
                    batches_processed += 1;

                    info!(
                        "Batch {} replayed successfully: {}/{} events",
                        batch_idx + 1,
                        replayed,
                        chunk.len()
                    );
                }
                Err(e) => {
                    total_failed += chunk.len();
                    let error_msg = format!("Batch {} failed: {}", batch_idx + 1, e);
                    error!("{}", error_msg);
                    failures.push(error_msg);
                }
            }

            // Rate limiting between batches
            if batch_idx
                < (unacked_events.len() + self.config.batch_size - 1) / self.config.batch_size - 1
            {
                tokio::time::sleep(self.config.batch_interval).await;
            }
        }

        let report = RecoveryReport {
            total_events: unacked_events.len(),
            replayed_events: total_replayed,
            failed_events: total_failed,
            failures,
            duration: start.elapsed(),
            batches_processed,
            retry_attempts,
        };

        info!(
            "Recovery complete: {}/{} events replayed in {:?} ({:.1}% success rate)",
            total_replayed,
            unacked_events.len(),
            report.duration,
            report.success_rate() * 100.0
        );

        Ok(report)
    }

    /// Count unacknowledged events (faster than find_unacknowledged_events)
    pub async fn count_unacknowledged_events(&self) -> Result<usize> {
        Ok(self.ack_tracker.pending_count())
    }

    /// Find unacknowledged lineage events from pending acknowledgments
    async fn find_unacknowledged_events(&self) -> Result<Vec<LineageEvent>> {
        // Get all pending acknowledgments (which include the events)
        let pending = self.ack_tracker.get_pending();

        info!("Found {} pending acknowledgments for replay", pending.len());

        // Extract the lineage events from pending acknowledgments
        let unacked_events: Vec<LineageEvent> = pending
            .into_iter()
            .map(|pending_ack| {
                debug!(
                    "Unacknowledged event at LSN {}: record_id={}",
                    pending_ack.lsn.0, pending_ack.event.record_id
                );
                pending_ack.event
            })
            .collect();

        Ok(unacked_events)
    }

    /// Replay a batch of events with retry logic
    async fn replay_batch_with_retry(&self, batch: &[LineageEvent]) -> Result<(usize, usize)> {
        let mut attempt = 0;
        let mut backoff = self.config.retry_backoff;
        let mut retry_count = 0;

        loop {
            match self.replay_batch(batch).await {
                Ok(count) => {
                    return Ok((count, retry_count));
                }
                Err(e) if attempt < self.config.max_retry_attempts => {
                    retry_count += 1;
                    warn!(
                        "Replay attempt {} failed: {} (retrying in {:?})",
                        attempt + 1,
                        e,
                        backoff
                    );

                    tokio::time::sleep(backoff).await;

                    // Exponential backoff
                    backoff = std::cmp::min(backoff * 2, self.config.max_backoff);
                    attempt += 1;
                }
                Err(e) => {
                    error!(
                        "Replay failed after {} attempts: {}",
                        self.config.max_retry_attempts, e
                    );
                    return Err(e);
                }
            }
        }
    }

    /// Replay a batch of events to Kafka
    async fn replay_batch(&self, batch: &[LineageEvent]) -> Result<usize> {
        let mut replayed = 0;

        for event in batch {
            // Use the durable sink to replay (goes through WAL + circuit breaker)
            // Note: This will write to WAL again, but that's OK for idempotency
            // The acknowledgment tracker will deduplicate
            match self.kafka_sink.write(event.clone()) {
                Ok(_) => {
                    replayed += 1;
                }
                Err(e) => {
                    warn!("Failed to replay event {}: {}", event.record_id, e);
                    // Continue with other events in batch
                }
            }
        }

        if replayed == 0 {
            return Err(anyhow::anyhow!("No events replayed in batch"));
        }

        Ok(replayed)
    }

    /// Get configuration
    pub fn config(&self) -> &ReplayConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::kafka::{AcknowledgmentTracker, DurableKafkaLineageSink, KafkaConfig};
    use crate::storage::wal::{FileWal, WalConfig, WalMetricsCollector};
    use graphica_core::core::lineage::DataRef;
    use std::collections::HashMap;
    use tempfile::TempDir;
    use uuid::Uuid;

    async fn create_test_replay_manager() -> (ReplayManager, TempDir) {
        use std::sync::atomic::{AtomicU64, Ordering};
        static TEST_COUNTER: AtomicU64 = AtomicU64::new(5000);
        let test_id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);

        let temp_dir = TempDir::new().unwrap();
        let wal_config = WalConfig::default().with_path(temp_dir.path().to_path_buf());
        let metrics = Arc::new(WalMetricsCollector::new(&format!(
            "test_replay_{}",
            test_id
        )));
        let wal: Arc<dyn WriteAheadLog> =
            Arc::new(FileWal::new(wal_config.clone(), metrics).await.unwrap());

        // Note: Kafka sink creation will fail without Kafka, but that's OK for testing
        let kafka_config = KafkaConfig::default();
        let ack_tracker = Arc::new(AcknowledgmentTracker::new(
            Arc::clone(&wal),
            kafka_config.ack_tracking.clone(),
        ));

        // Create a mock sink (won't actually connect to Kafka)
        let sink_result =
            DurableKafkaLineageSink::new("localhost:9092", Arc::clone(&wal), kafka_config).await;

        let sink = match sink_result {
            Ok(s) => Arc::new(s),
            Err(_) => {
                // Create mock for testing (Kafka not available)
                // In real tests, we'd use a mock Kafka or testcontainers
                panic!("Cannot create sink without Kafka - skipping test");
            }
        };

        let config = ReplayConfig::default();
        let manager = ReplayManager::new(wal, sink, ack_tracker, config);

        (manager, temp_dir)
    }

    fn create_test_event(record_id: &str) -> LineageEvent {
        use chrono::Utc;

        LineageEvent {
            id: Uuid::new_v4(),
            dataset: "test_dataset".to_string(),
            record_id: record_id.to_string(),
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
                system: "output_system".to_string(),
                path: "output/path".to_string(),
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
    #[ignore] // Requires Kafka
    async fn test_replay_manager_creation() {
        let (manager, _dir) = create_test_replay_manager().await;
        assert_eq!(manager.config().batch_size, 500);
    }

    #[tokio::test]
    #[ignore] // Requires Kafka
    async fn test_empty_recovery() {
        let (manager, _dir) = create_test_replay_manager().await;

        let report = manager.recover_on_startup().await.unwrap();

        assert_eq!(report.total_events, 0);
        assert_eq!(report.replayed_events, 0);
        assert!(report.is_successful());
        assert_eq!(report.success_rate(), 1.0);
    }

    #[test]
    fn test_replay_config_presets() {
        let default = ReplayConfig::default();
        assert_eq!(default.batch_size, 500);
        assert_eq!(default.batch_interval, Duration::from_secs(1));

        let aggressive = ReplayConfig::aggressive();
        assert_eq!(aggressive.batch_size, 1000);
        assert_eq!(aggressive.batch_interval, Duration::from_millis(100));

        let conservative = ReplayConfig::conservative();
        assert_eq!(conservative.batch_size, 100);
        assert_eq!(conservative.batch_interval, Duration::from_secs(5));
    }

    #[test]
    fn test_recovery_report_success_rate() {
        let report = RecoveryReport {
            total_events: 100,
            replayed_events: 95,
            failed_events: 5,
            failures: vec![],
            duration: Duration::from_secs(10),
            batches_processed: 2,
            retry_attempts: 3,
        };

        assert_eq!(report.success_rate(), 0.95);
        assert!(!report.is_successful());
    }

    #[test]
    fn test_recovery_report_full_success() {
        let report = RecoveryReport {
            total_events: 100,
            replayed_events: 100,
            failed_events: 0,
            failures: vec![],
            duration: Duration::from_secs(10),
            batches_processed: 2,
            retry_attempts: 0,
        };

        assert_eq!(report.success_rate(), 1.0);
        assert!(report.is_successful());
    }
}
