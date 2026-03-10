//! Tiered Dead Letter Queue with Memory Fallback
//!
//! Implements Architecture Decision #4: Secondary memory-based DLQ to prevent cascading failures.
//!
//! ## Problem
//! If primary DLQ disk is full, DLQ writes fail and system crashes.
//!
//! ## Solution
//! Three-tier failure handling:
//! 1. Primary DLQ (disk-based, durable)
//! 2. Secondary DLQ (memory-based, bounded)
//! 3. Final fallback (drop + log critical error + metric)

use crate::core::lineage::LineageEvent;
use crate::ingestion::dlq::{DeadLetterQueue, DlqRecord};
use anyhow::Result;
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::Arc;

/// Capacity limits for tiered DLQ
#[derive(Debug, Clone)]
pub struct TieredDlqConfig {
    /// Maximum events in memory buffer before dropping
    pub memory_capacity: usize,

    /// Warning threshold (percentage) for memory buffer
    pub memory_warn_threshold: f64,
}

impl Default for TieredDlqConfig {
    fn default() -> Self {
        Self {
            memory_capacity: 10_000,    // 10K events max in memory
            memory_warn_threshold: 0.7, // Warn at 70% full
        }
    }
}

/// Tiered DLQ with memory fallback for cascading failure prevention
pub struct TieredDeadLetterQueue {
    primary: Arc<DeadLetterQueue>,
    secondary: Arc<Mutex<VecDeque<DlqRecord>>>,
    config: TieredDlqConfig,
}

impl TieredDeadLetterQueue {
    pub fn new(primary: DeadLetterQueue, config: TieredDlqConfig) -> Self {
        Self {
            primary: Arc::new(primary),
            secondary: Arc::new(Mutex::new(VecDeque::with_capacity(config.memory_capacity))),
            config,
        }
    }

    pub fn with_defaults(primary: DeadLetterQueue) -> Self {
        Self::new(primary, TieredDlqConfig::default())
    }

    /// Write event to tiered DLQ with cascading failure prevention
    ///
    /// ## Failure Cascade
    /// 1. Try primary DLQ (disk) - if full/error, go to step 2
    /// 2. Try secondary DLQ (memory) - if full, go to step 3
    /// 3. Drop event + log CRITICAL error + increment DATA_LOSS metric
    ///
    /// ## Enterprise Requirements
    /// - No panics or crashes
    /// - All failures logged with context
    /// - Metrics track each tier usage
    /// - System continues operating
    pub fn write(&self, event: LineageEvent, error: &str, retry_count: u32) -> Result<()> {
        let dlq_record = DlqRecord {
            original_timestamp: chrono::Utc::now().timestamp_millis(),
            event: event.clone(),
            error: error.to_string(),
            failed_at: chrono::Utc::now(),
            retry_count,
        };

        // Tier 1: Try primary DLQ (disk-based, durable)
        match self.primary.write(event.clone(), error, retry_count) {
            Ok(_) => {
                // Success - primary DLQ accepted event
                crate::ingestion::metrics::DLQ_WRITES
                    .with_label_values(&["primary_success"])
                    .inc();
                return Ok(());
            }
            Err(primary_error) => {
                tracing::error!(
                    "Primary DLQ write failed: {}. Falling back to memory DLQ",
                    primary_error
                );
                crate::ingestion::metrics::DLQ_WRITES
                    .with_label_values(&["primary_failure"])
                    .inc();

                // Tier 2: Try secondary DLQ (memory-based, bounded)
                let mut secondary = self.secondary.lock();

                if secondary.len() < self.config.memory_capacity {
                    secondary.push_back(dlq_record);

                    let usage = secondary.len() as f64 / self.config.memory_capacity as f64;

                    if usage >= self.config.memory_warn_threshold {
                        tracing::warn!(
                            "Secondary DLQ at {:.1}% capacity ({}/{})",
                            usage * 100.0,
                            secondary.len(),
                            self.config.memory_capacity
                        );
                    } else {
                        tracing::info!(
                            "Event buffered in secondary DLQ ({}/{})",
                            secondary.len(),
                            self.config.memory_capacity
                        );
                    }

                    crate::ingestion::metrics::DLQ_WRITES
                        .with_label_values(&["secondary_success"])
                        .inc();
                    crate::ingestion::metrics::DLQ_MEMORY_SIZE.set(secondary.len() as f64);

                    return Ok(());
                } else {
                    // Tier 3: Memory DLQ full - PERMANENT DATA LOSS
                    tracing::error!(
                        "CRITICAL: DATA LOSS - Both primary and secondary DLQ full. Dropping event: {}",
                        event.id
                    );
                    tracing::error!("  Primary DLQ error: {}", primary_error);
                    tracing::error!(
                        "  Secondary DLQ capacity: {}/{}",
                        secondary.len(),
                        self.config.memory_capacity
                    );

                    crate::ingestion::metrics::DATA_LOSS_TOTAL.inc();
                    crate::ingestion::metrics::DLQ_WRITES
                        .with_label_values(&["data_loss"])
                        .inc();

                    // Return error to caller (but system continues)
                    return Err(anyhow::anyhow!(
                        "Both primary and secondary DLQ full. Event dropped: {}",
                        event.id
                    ));
                }
            }
        }
    }

    /// Drain secondary DLQ back to primary (for recovery/background task)
    ///
    /// Call this periodically from a background task when primary DLQ recovers.
    /// Drains memory buffer back to disk for durability.
    pub fn drain_secondary_to_primary(&self) -> Result<usize> {
        let mut secondary = self.secondary.lock();
        let mut drained = 0usize;

        while let Some(record) = secondary.pop_front() {
            match self
                .primary
                .write(record.event.clone(), &record.error, record.retry_count)
            {
                Ok(_) => {
                    drained += 1;
                    crate::ingestion::metrics::DLQ_WRITES
                        .with_label_values(&["drain_success"])
                        .inc();
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to drain secondary DLQ to primary: {}. Stopping drain (will retry later)",
                        e
                    );
                    // Put record back
                    secondary.push_front(record);
                    break;
                }
            }
        }

        if drained > 0 {
            tracing::info!(
                "Drained {} events from secondary DLQ to primary. Remaining: {}",
                drained,
                secondary.len()
            );
            crate::ingestion::metrics::DLQ_MEMORY_SIZE.set(secondary.len() as f64);
        }

        Ok(drained)
    }

    /// Get statistics for both tiers
    pub fn stats(&self) -> Result<TieredDlqStats> {
        let primary_stats = self.primary.stats()?;
        let secondary_size = self.secondary.lock().len();

        Ok(TieredDlqStats {
            primary_records: primary_stats.total_records,
            primary_size_bytes: primary_stats.total_size_bytes,
            secondary_records: secondary_size as u64,
            secondary_capacity: self.config.memory_capacity,
            secondary_usage_pct: (secondary_size as f64 / self.config.memory_capacity as f64)
                * 100.0,
        })
    }

    /// Start background drain task (runs every 60 seconds)
    ///
    /// Automatically drains secondary DLQ back to primary when primary recovers.
    /// Returns JoinHandle that can be aborted on shutdown.
    pub fn start_drain_task(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));

            loop {
                interval.tick().await;

                // Only drain if secondary has events
                let secondary_size = self.secondary.lock().len();
                if secondary_size > 0 {
                    tracing::info!(
                        "Background drain: {} events in secondary DLQ",
                        secondary_size
                    );

                    match self.drain_secondary_to_primary() {
                        Ok(drained) => {
                            if drained > 0 {
                                tracing::info!(
                                    "Background drain: successfully drained {} events",
                                    drained
                                );
                            }
                        }
                        Err(e) => {
                            tracing::warn!("Background drain failed: {}", e);
                        }
                    }
                }
            }
        })
    }
}

// Implement DlqWriter trait for integration with RetryExecutor
impl crate::reliability::retry_strategy::DlqWriter for TieredDeadLetterQueue {
    fn write(
        &self,
        event: crate::core::lineage::LineageEvent,
        error: &str,
        retries: u32,
    ) -> Result<()> {
        self.write(event, error, retries)
    }
}

/// Statistics for tiered DLQ
#[derive(Debug, Clone)]
pub struct TieredDlqStats {
    pub primary_records: u64,
    pub primary_size_bytes: u64,
    pub secondary_records: u64,
    pub secondary_capacity: usize,
    pub secondary_usage_pct: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::lineage::{DataRef, LineageEvent};
    use std::collections::HashMap;
    use tempfile::tempdir;

    fn create_test_event(id: &str) -> LineageEvent {
        LineageEvent {
            id: uuid::Uuid::new_v4(),
            record_id: id.to_string(),
            dataset: "test".to_string(),
            source_refs: vec![DataRef {
                system: "test".to_string(),
                path: "test".to_string(),
                version: Some("1".to_string()),
                extracted_at: chrono::Utc::now(),
                cdc_position: None,
            }],
            transforms: vec![],
            model_refs: vec![],
            output_ref: DataRef {
                system: "test".to_string(),
                path: "output".to_string(),
                version: None,
                extracted_at: chrono::Utc::now(),
                cdc_position: None,
            },
            ts: chrono::Utc::now(),
            run_id: "test_run".to_string(),
            tenant_id: "test_tenant".to_string(),
            correlation_id: Some(uuid::Uuid::new_v4().to_string()),
            metadata: HashMap::new(),
        }
    }

    #[test]
    fn test_tiered_dlq_primary_success() {
        let dir = tempdir().unwrap();
        let primary = DeadLetterQueue::new(dir.path()).unwrap();
        let tiered = TieredDeadLetterQueue::with_defaults(primary);

        let event = create_test_event("test1");
        tiered.write(event, "test error", 0).unwrap();

        let stats = tiered.stats().unwrap();
        assert_eq!(stats.primary_records, 1);
        assert_eq!(stats.secondary_records, 0);
    }

    #[test]
    #[ignore = "TODO: Requires mock DLQ to test fallback behavior (running as root allows all writes)"]
    fn test_tiered_dlq_memory_fallback() {
        // Create DLQ with path that will fail on write (permission denied)
        // NOTE: This test requires running as non-root or mocking the DLQ
        let primary = DeadLetterQueue::new("/graphica_test_dlq_fallback").unwrap();
        let config = TieredDlqConfig {
            memory_capacity: 10,
            memory_warn_threshold: 0.7,
        };
        let tiered = TieredDeadLetterQueue::new(primary, config);

        // Write should fall back to memory (write will fail due to permissions)
        let event = create_test_event("test1");
        tiered.write(event, "test error", 0).unwrap();

        let stats = tiered.stats().unwrap();
        assert_eq!(stats.secondary_records, 1);
    }

    #[test]
    #[ignore = "TODO: Requires mock DLQ to test data loss behavior (running as root allows all writes)"]
    fn test_tiered_dlq_data_loss() {
        // Create DLQ with path that will fail on write and small memory capacity
        // NOTE: This test requires running as non-root or mocking the DLQ
        let primary = DeadLetterQueue::new("/graphica_test_dlq_data_loss").unwrap();
        let config = TieredDlqConfig {
            memory_capacity: 2,
            memory_warn_threshold: 0.5,
        };
        let tiered = TieredDeadLetterQueue::new(primary, config);

        // Fill memory buffer (writes fail on primary, go to secondary)
        tiered
            .write(create_test_event("test1"), "error", 0)
            .unwrap();
        tiered
            .write(create_test_event("test2"), "error", 0)
            .unwrap();

        // Third write should fail (data loss - secondary full)
        let result = tiered.write(create_test_event("test3"), "error", 0);
        assert!(result.is_err());

        let stats = tiered.stats().unwrap();
        assert_eq!(stats.secondary_records, 2);
    }

    #[tokio::test]
    async fn test_drain_secondary_to_primary() {
        let dir = tempdir().unwrap();
        let primary = DeadLetterQueue::new(dir.path()).unwrap();
        let config = TieredDlqConfig {
            memory_capacity: 10,
            memory_warn_threshold: 0.7,
        };
        let tiered = TieredDeadLetterQueue::new(primary, config);

        // Manually add to secondary
        {
            let mut secondary = tiered.secondary.lock();
            secondary.push_back(DlqRecord {
                original_timestamp: chrono::Utc::now().timestamp_millis(),
                event: create_test_event("test1"),
                error: "test".to_string(),
                failed_at: chrono::Utc::now(),
                retry_count: 0,
            });
        }

        // Drain to primary
        let drained = tiered.drain_secondary_to_primary().unwrap();
        assert_eq!(drained, 1);

        let stats = tiered.stats().unwrap();
        assert_eq!(stats.secondary_records, 0);
        assert_eq!(stats.primary_records, 1);
    }
}
