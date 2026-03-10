//! # Async Storage Writer
//!
//! High-throughput async storage writer with concurrent batching.
//!
//! Replaces the sync storage writer thread with an async task that enables:
//! - Concurrent batch writes
//! - Non-blocking I/O
//! - Better resource utilization

use crate::governance::SharedGovernanceBrain;
use anyhow::Result;
use graphica_core::core::lineage::{AsyncLineageSink, LineageEvent};
use graphica_core::ingestion::dlq_tiered::TieredDeadLetterQueue;
use graphica_core::ingestion::metrics;
use graphica_core::reliability::{CircuitBreaker, CircuitBreakerConfig};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::time::sleep;

/// Configuration for async storage writer
#[derive(Debug, Clone)]
pub struct AsyncStorageWriterConfig {
    /// Minimum batch size before flushing
    pub min_batch_size: usize,
    /// Maximum batch size (flush immediately when reached)
    pub max_batch_size: usize,
    /// Maximum age of oldest event before flushing (milliseconds)
    pub max_wait_ms: u64,
    /// Channel buffer size
    pub channel_buffer: usize,
    /// Enable circuit breaker
    pub enable_circuit_breaker: bool,
}

impl Default for AsyncStorageWriterConfig {
    fn default() -> Self {
        Self {
            min_batch_size: 100,
            max_batch_size: 1000,
            max_wait_ms: 250,
            channel_buffer: 10_000,
            enable_circuit_breaker: true,
        }
    }
}

/// Async storage writer with concurrent batching
pub struct AsyncStorageWriter {
    tx: mpsc::Sender<LineageEvent>,
    config: AsyncStorageWriterConfig,
}

impl AsyncStorageWriter {
    /// Create new async storage writer
    ///
    /// # Arguments
    /// * `store` - Async storage implementation
    /// * `dlq` - Dead letter queue for failed events
    /// * `config` - Writer configuration
    /// * `governance` - Optional governance brain for RDF materialization
    ///
    /// # Returns
    /// Writer instance and background task handle
    pub fn new<S>(
        store: Arc<S>,
        dlq: Arc<TieredDeadLetterQueue>,
        config: AsyncStorageWriterConfig,
        governance: Option<SharedGovernanceBrain>,
    ) -> (Self, tokio::task::JoinHandle<()>)
    where
        S: AsyncLineageSink + Send + Sync + 'static,
    {
        let (tx, rx) = mpsc::channel(config.channel_buffer);

        // Clone config for writer instance
        let config_clone = config.clone();

        // Spawn background writer task
        let task = tokio::spawn(async move {
            run_async_writer(store, dlq, rx, config, governance).await;
        });

        (
            Self {
                tx,
                config: config_clone,
            },
            task,
        )
    }

    /// Send event to storage writer (non-blocking)
    pub async fn write(&self, event: LineageEvent) -> Result<()> {
        self.tx
            .send(event)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send event to storage writer: {}", e))
    }

    /// Try to send event without blocking (returns error if channel full)
    pub fn try_write(&self, event: LineageEvent) -> Result<()> {
        self.tx
            .try_send(event)
            .map_err(|e| anyhow::anyhow!("Failed to send event to storage writer: {}", e))
    }

    /// Get configuration
    pub fn config(&self) -> &AsyncStorageWriterConfig {
        &self.config
    }
}

/// Background async writer task
async fn run_async_writer<S>(
    store: Arc<S>,
    dlq: Arc<TieredDeadLetterQueue>,
    mut rx: mpsc::Receiver<LineageEvent>,
    config: AsyncStorageWriterConfig,
    governance: Option<SharedGovernanceBrain>,
) where
    S: AsyncLineageSink + Send + Sync + 'static,
{
    tracing::info!("Async storage writer started with config: {:?}", config);

    // Circuit breaker for storage writes
    let circuit_breaker = if config.enable_circuit_breaker {
        Some(CircuitBreaker::new(
            "async_storage",
            CircuitBreakerConfig {
                failure_threshold: 5,
                timeout: Duration::from_secs(30),
                success_threshold: 2,
            },
        ))
    } else {
        None
    };

    let mut batch = Vec::with_capacity(config.max_batch_size);
    let mut batch_start = Instant::now();
    let mut total_written = 0u64;
    let mut total_failed = 0u64;

    loop {
        // Receive with timeout to enable periodic batch flushing
        let timeout_duration = Duration::from_millis(config.max_wait_ms);

        match tokio::time::timeout(timeout_duration, rx.recv()).await {
            Ok(Some(event)) => {
                batch.push(event);

                // Check if we should flush
                let batch_age_ms = batch_start.elapsed().as_millis() as u64;
                let should_flush = batch.len() >= config.max_batch_size
                    || (batch.len() >= config.min_batch_size && batch_age_ms >= config.max_wait_ms);

                if should_flush {
                    let batch_size = batch.len();
                    let written = flush_batch(
                        &store,
                        &dlq,
                        &mut batch,
                        circuit_breaker.as_ref(),
                        batch_age_ms,
                        governance.as_ref(),
                    )
                    .await;

                    total_written += written;
                    total_failed += (batch_size as u64) - written;

                    batch_start = Instant::now();
                }
            }
            Ok(None) => {
                // Channel closed - flush remaining batch and exit
                tracing::info!("Storage writer channel closed, flushing final batch");

                if !batch.is_empty() {
                    let batch_age_ms = batch_start.elapsed().as_millis() as u64;
                    let written = flush_batch(
                        &store,
                        &dlq,
                        &mut batch,
                        circuit_breaker.as_ref(),
                        batch_age_ms,
                        governance.as_ref(),
                    )
                    .await;

                    total_written += written;
                    total_failed += (batch.len() as u64) - written;
                }

                tracing::info!(
                    "Async storage writer shutdown complete (written: {}, failed: {})",
                    total_written,
                    total_failed
                );
                break;
            }
            Err(_) => {
                // Timeout - flush aged batch
                if !batch.is_empty() {
                    let batch_age_ms = batch_start.elapsed().as_millis() as u64;

                    if batch_age_ms >= config.max_wait_ms {
                        let batch_size = batch.len(); // FIX: Save size before flush

                        tracing::debug!(
                            "Flushing aged batch of {} events (age: {}ms)",
                            batch_size,
                            batch_age_ms
                        );

                        let written = flush_batch(
                            &store,
                            &dlq,
                            &mut batch,
                            circuit_breaker.as_ref(),
                            batch_age_ms,
                            governance.as_ref(),
                        )
                        .await;

                        total_written += written;
                        total_failed += (batch_size as u64) - written;

                        batch_start = Instant::now();
                    }
                }
            }
        }
    }
}

/// Flush batch of events to storage with concurrent writes
async fn flush_batch<S>(
    store: &Arc<S>,
    dlq: &Arc<TieredDeadLetterQueue>,
    batch: &mut Vec<LineageEvent>,
    circuit_breaker: Option<&CircuitBreaker>,
    batch_age_ms: u64,
    governance: Option<&SharedGovernanceBrain>,
) -> u64
where
    S: AsyncLineageSink + Send + Sync,
{
    if batch.is_empty() {
        return 0;
    }

    // Check circuit breaker before attempting write
    if let Some(cb) = circuit_breaker {
        if cb.is_open() {
            tracing::warn!(
                "Circuit breaker open - rejecting batch of {} events",
                batch.len()
            );

            // Send all events to DLQ since circuit is open
            let events_to_dlq: Vec<LineageEvent> = batch.drain(..).collect();
            for event in events_to_dlq {
                if let Err(e) = dlq.write(event, "Circuit breaker open", 1) {
                    tracing::error!("Failed to write to DLQ during circuit breaker open: {}", e);
                }
            }

            return 0;
        }
    }

    let batch_size = batch.len();
    let flush_start = Instant::now();

    tracing::debug!(
        "Flushing batch of {} events (age: {}ms)",
        batch_size,
        batch_age_ms
    );

    // PERFORMANCE OPTIMIZATION: Write entire batch concurrently
    // Instead of sequential writes, use write_batch for better throughput
    let events: Vec<LineageEvent> = batch.drain(..).collect();

    // Write with circuit breaker tracking
    let write_result = store.write_batch(events.clone()).await;

    let written = match write_result {
        Ok(_) => {
            // Success - record with circuit breaker
            if let Some(cb) = circuit_breaker {
                cb.record_success();
            }

            // Success - all events written
            let flush_latency = flush_start.elapsed().as_millis() as f64;

            tracing::debug!(
                "Batch of {} events written successfully ({:.2}ms)",
                batch_size,
                flush_latency
            );

            // Metrics
            metrics::STORAGE_BATCH_SIZE
                .with_label_values(&["async_rocks"])
                .observe(batch_size as f64);
            metrics::STORAGE_WRITE_LATENCY
                .with_label_values(&["async_rocks"])
                .observe(flush_latency);

            // RDF Materialization - async, non-blocking, best-effort
            if let Some(gov) = governance {
                let gov_clone = gov.clone();
                let events_for_rdf = events.clone();

                // Spawn async task for RDF materialization
                // This runs in parallel without blocking storage writes
                tokio::spawn(async move {
                    let rdf_start = Instant::now();
                    let mut materialized_count = 0usize;
                    let mut failed_count = 0usize;

                    for event in events_for_rdf {
                        match gov_clone.materialize_lineage_event(&event).await {
                            Ok(_) => {
                                materialized_count += 1;
                                tracing::trace!("RDF materialized for event: {}", event.id);
                            }
                            Err(e) => {
                                failed_count += 1;
                                // Log error but don't fail the entire batch
                                // RDF materialization is best-effort
                                tracing::warn!(
                                    "RDF materialization failed for event {}: {}",
                                    event.id,
                                    e
                                );

                                // Track RDF materialization failures
                                metrics::RECORDS_DROPPED
                                    .with_label_values(&["rdf", "materialization_failed"])
                                    .inc();
                            }
                        }
                    }

                    let rdf_latency = rdf_start.elapsed();

                    if materialized_count > 0 {
                        tracing::debug!(
                            "RDF materialization complete: {}/{} events in {:?}",
                            materialized_count,
                            materialized_count + failed_count,
                            rdf_latency
                        );

                        // RDF materialization metrics
                        metrics::STORAGE_BATCH_SIZE
                            .with_label_values(&["rdf_materialization"])
                            .observe(materialized_count as f64);

                        metrics::STORAGE_WRITE_LATENCY
                            .with_label_values(&["rdf_materialization"])
                            .observe(rdf_latency.as_millis() as f64);
                    }

                    if failed_count > 0 {
                        tracing::warn!(
                            "RDF materialization had {} failures out of {} total events",
                            failed_count,
                            materialized_count + failed_count
                        );
                    }
                });

                tracing::trace!("RDF materialization task spawned for {} events", batch_size);
            }

            batch_size as u64
        }
        Err(e) => {
            // Record failure with circuit breaker
            if let Some(cb) = circuit_breaker {
                cb.record_failure();
            }

            tracing::error!("Batch write failed: {}", e);

            // Fallback: Try individual writes with retry
            let mut success_count = 0u64;

            for event in events {
                // Retry with exponential backoff
                let result = retry_with_backoff(&store, event.clone(), 3).await;

                match result {
                    Ok(_) => {
                        success_count += 1;
                    }
                    Err(retry_err) => {
                        tracing::error!("Event write failed after retries: {}", retry_err);

                        // Send to DLQ
                        let error_msg = format!("Write failed after retries: {}", retry_err);
                        if let Err(dlq_err) = dlq.write(event, &error_msg, 3) {
                            tracing::error!("DLQ write failed: {}", dlq_err);
                        }
                    }
                }
            }

            success_count
        }
    };

    written
}

/// Retry write with exponential backoff
async fn retry_with_backoff<S>(store: &Arc<S>, event: LineageEvent, max_retries: u32) -> Result<()>
where
    S: AsyncLineageSink + Send + Sync,
{
    let mut attempt = 0;
    let mut delay_ms = 100;

    loop {
        match store.write(event.clone()).await {
            Ok(_) => return Ok(()),
            Err(e) => {
                attempt += 1;

                if attempt >= max_retries {
                    return Err(e);
                }

                tracing::warn!(
                    "Write failed (attempt {}/{}), retrying in {}ms: {}",
                    attempt,
                    max_retries,
                    delay_ms,
                    e
                );

                sleep(Duration::from_millis(delay_ms)).await;

                // Exponential backoff
                delay_ms *= 2;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::AsyncRocksLineageStore;
    use crate::storage::RocksLineageStore;
    use graphica_core::core::lineage::{DataRef, LineageEvent};
    use graphica_core::ingestion::dlq::DeadLetterQueue;
    use std::collections::HashMap;
    use tokio;
    use uuid::Uuid;

    fn create_test_event(record_id: &str) -> LineageEvent {
        LineageEvent {
            id: Uuid::new_v4(),
            dataset: "test".to_string(),
            record_id: record_id.to_string(),
            source_refs: vec![],
            transforms: vec![],
            model_refs: vec![],
            output_ref: DataRef {
                system: "test".to_string(),
                path: "test".to_string(),
                version: None,
                extracted_at: chrono::Utc::now(),
                cdc_position: None,
            },
            ts: chrono::Utc::now(),
            run_id: "test-run".to_string(),
            tenant_id: "test-tenant".to_string(),
            correlation_id: None,
            metadata: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn test_async_writer_basic() {
        let temp_dir = tempfile::tempdir().unwrap();
        let sync_store =
            Arc::new(RocksLineageStore::new(temp_dir.path().to_str().unwrap()).unwrap());
        let async_store = Arc::new(AsyncRocksLineageStore::new(sync_store));

        let primary_dlq = DeadLetterQueue::new(temp_dir.path().to_str().unwrap()).unwrap();
        let dlq = Arc::new(TieredDeadLetterQueue::with_defaults(primary_dlq));

        let config = AsyncStorageWriterConfig {
            min_batch_size: 10,
            max_batch_size: 100,
            max_wait_ms: 100,
            channel_buffer: 1000,
            enable_circuit_breaker: false,
        };

        let (writer, _task) = AsyncStorageWriter::new(async_store.clone(), dlq, config, None);

        // Write some events
        for i in 0..50 {
            let event = create_test_event(&format!("rec-{}", i));
            writer.write(event).await.unwrap();
        }

        // Wait for flush
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Verify events written
        for i in 0..50 {
            let events = async_store
                .get_record_lineage(&format!("rec-{}", i))
                .await
                .unwrap();
            assert_eq!(events.len(), 1);
        }
    }

    #[tokio::test]
    async fn test_async_writer_concurrent() {
        let temp_dir = tempfile::tempdir().unwrap();
        let sync_store =
            Arc::new(RocksLineageStore::new(temp_dir.path().to_str().unwrap()).unwrap());
        let async_store = Arc::new(AsyncRocksLineageStore::new(sync_store));

        let primary_dlq = DeadLetterQueue::new(temp_dir.path().to_str().unwrap()).unwrap();
        let dlq = Arc::new(TieredDeadLetterQueue::with_defaults(primary_dlq));

        let config = AsyncStorageWriterConfig::default();

        let (writer, _task) = AsyncStorageWriter::new(async_store.clone(), dlq, config, None);
        let writer = Arc::new(writer);

        // Spawn 10 concurrent writers
        let mut handles = vec![];
        for worker_id in 0..10 {
            let writer_clone = writer.clone();
            let handle = tokio::spawn(async move {
                for i in 0..100 {
                    let event = create_test_event(&format!("rec-worker{}-{}", worker_id, i));
                    writer_clone.write(event).await.unwrap();
                }
            });
            handles.push(handle);
        }

        // Wait for all writers
        for handle in handles {
            handle.await.unwrap();
        }

        // Wait for flush
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Verify all written
        for worker_id in 0..10 {
            for i in 0..100 {
                let events = async_store
                    .get_record_lineage(&format!("rec-worker{}-{}", worker_id, i))
                    .await
                    .unwrap();
                assert_eq!(events.len(), 1);
            }
        }
    }
}
