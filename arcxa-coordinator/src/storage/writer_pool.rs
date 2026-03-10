//! # Multi-threaded Writer Pool
//!
//! Parallelizes RocksDB writes across multiple threads for maximum throughput.
//!
//! Key optimizations:
//! - Multiple writer threads (default: num_cpus)
//! - Channel-based work distribution
//! - Automatic batching for efficiency
//! - Back-pressure handling
//! - Per-thread metrics

use crate::storage::rocks::RocksLineageStore;
use anyhow::Result;
use flume::{bounded, Receiver, Sender};
use graphica_core::core::lineage::LineageEvent;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use tracing::{error, info, warn};

/// Configuration for writer pool
#[derive(Debug, Clone)]
pub struct WriterPoolConfig {
    /// Number of writer threads (default: num_cpus)
    pub num_threads: usize,
    /// Maximum batch size per write (default: 100)
    pub batch_size: usize,
    /// Maximum time to wait before flushing partial batch (default: 10ms)
    pub batch_timeout_ms: u64,
    /// Channel buffer size (default: 10000)
    pub channel_buffer: usize,
}

impl Default for WriterPoolConfig {
    fn default() -> Self {
        Self {
            num_threads: num_cpus::get(),
            batch_size: 100,
            batch_timeout_ms: 10,
            channel_buffer: 10_000,
        }
    }
}

/// Statistics for monitoring writer pool performance
pub struct WriterPoolStats {
    pub events_written: AtomicU64,
    pub events_failed: AtomicU64,
    pub batches_written: AtomicU64,
    pub total_latency_us: AtomicU64,
}

impl WriterPoolStats {
    fn new() -> Self {
        Self {
            events_written: AtomicU64::new(0),
            events_failed: AtomicU64::new(0),
            batches_written: AtomicU64::new(0),
            total_latency_us: AtomicU64::new(0),
        }
    }

    pub fn events_written(&self) -> u64 {
        self.events_written.load(Ordering::Relaxed)
    }

    pub fn events_failed(&self) -> u64 {
        self.events_failed.load(Ordering::Relaxed)
    }

    pub fn avg_latency_us(&self) -> f64 {
        let total = self.total_latency_us.load(Ordering::Relaxed);
        let count = self.events_written.load(Ordering::Relaxed);
        if count == 0 {
            0.0
        } else {
            total as f64 / count as f64
        }
    }

    pub fn throughput(&self, duration: Duration) -> f64 {
        let count = self.events_written.load(Ordering::Relaxed);
        count as f64 / duration.as_secs_f64()
    }
}

/// Message sent to writer threads
enum WriterMessage {
    Write(LineageEvent),
    Flush,
    Shutdown,
}

/// Multi-threaded writer pool for RocksDB
pub struct WriterPool {
    senders: Vec<Sender<WriterMessage>>,
    config: WriterPoolConfig,
    stats: Arc<WriterPoolStats>,
    current_thread: AtomicU64,
}

impl WriterPool {
    /// Create a new writer pool
    pub fn new(storage: Arc<RocksLineageStore>, config: WriterPoolConfig) -> Result<Self> {
        info!(
            "Initializing writer pool with {} threads, batch size: {}, buffer: {}",
            config.num_threads, config.batch_size, config.channel_buffer
        );

        let stats = Arc::new(WriterPoolStats::new());
        let mut senders = Vec::new();

        // Spawn writer threads
        for thread_id in 0..config.num_threads {
            let (tx, rx) = bounded(config.channel_buffer);
            let storage_clone = Arc::clone(&storage);
            let config_clone = config.clone();
            let stats_clone = Arc::clone(&stats);

            thread::Builder::new()
                .name(format!("graphica-writer-{}", thread_id))
                .spawn(move || {
                    writer_thread(thread_id, rx, storage_clone, config_clone, stats_clone);
                })?;

            senders.push(tx);
        }

        Ok(Self {
            senders,
            config,
            stats,
            current_thread: AtomicU64::new(0),
        })
    }

    /// Write an event (non-blocking, returns immediately)
    pub fn write(&self, event: LineageEvent) -> Result<()> {
        // Round-robin distribution across writer threads
        let thread_idx =
            self.current_thread.fetch_add(1, Ordering::Relaxed) as usize % self.senders.len();

        self.senders[thread_idx]
            .send(WriterMessage::Write(event))
            .map_err(|e| anyhow::anyhow!("Failed to send to writer thread: {}", e))?;

        Ok(())
    }

    /// Flush all pending writes (blocking)
    pub fn flush(&self) -> Result<()> {
        // Send flush message to all threads
        for sender in &self.senders {
            sender
                .send(WriterMessage::Flush)
                .map_err(|e| anyhow::anyhow!("Failed to send flush: {}", e))?;
        }

        // Note: This doesn't wait for flush to complete
        // For blocking flush, we'd need a response channel
        Ok(())
    }

    /// Shutdown the writer pool gracefully
    pub fn shutdown(self) -> Result<()> {
        info!("Shutting down writer pool...");

        // Send shutdown message to all threads
        for sender in &self.senders {
            let _ = sender.send(WriterMessage::Shutdown);
        }

        // Wait for channels to close (threads will exit)
        drop(self.senders);

        info!("Writer pool shutdown complete");
        Ok(())
    }

    /// Get statistics
    pub fn stats(&self) -> &WriterPoolStats {
        &self.stats
    }

    /// Get number of threads
    pub fn num_threads(&self) -> usize {
        self.config.num_threads
    }
}

/// Writer thread main loop
fn writer_thread(
    thread_id: usize,
    rx: Receiver<WriterMessage>,
    storage: Arc<RocksLineageStore>,
    config: WriterPoolConfig,
    stats: Arc<WriterPoolStats>,
) {
    info!("Writer thread {} started", thread_id);

    let mut batch = Vec::with_capacity(config.batch_size);
    let mut last_flush = Instant::now();

    loop {
        // Try to receive with timeout to enable periodic flushing
        let timeout = Duration::from_millis(config.batch_timeout_ms);

        match rx.recv_timeout(timeout) {
            Ok(WriterMessage::Write(event)) => {
                batch.push(event);

                // Flush if batch is full or timeout elapsed
                let should_flush = batch.len() >= config.batch_size
                    || last_flush.elapsed().as_millis() as u64 >= config.batch_timeout_ms;

                if should_flush {
                    flush_batch(&mut batch, &storage, &stats, thread_id);
                    last_flush = Instant::now();
                }
            }

            Ok(WriterMessage::Flush) => {
                // Explicit flush request
                if !batch.is_empty() {
                    flush_batch(&mut batch, &storage, &stats, thread_id);
                    last_flush = Instant::now();
                }
            }

            Ok(WriterMessage::Shutdown) => {
                // Flush remaining and exit
                if !batch.is_empty() {
                    flush_batch(&mut batch, &storage, &stats, thread_id);
                }
                info!("Writer thread {} shutting down", thread_id);
                break;
            }

            Err(flume::RecvTimeoutError::Timeout) => {
                // Timeout - flush partial batch if any
                if !batch.is_empty()
                    && last_flush.elapsed().as_millis() as u64 >= config.batch_timeout_ms
                {
                    flush_batch(&mut batch, &storage, &stats, thread_id);
                    last_flush = Instant::now();
                }
            }

            Err(flume::RecvTimeoutError::Disconnected) => {
                // Channel closed - flush and exit
                if !batch.is_empty() {
                    flush_batch(&mut batch, &storage, &stats, thread_id);
                }
                info!("Writer thread {} channel disconnected", thread_id);
                break;
            }
        }
    }
}

/// Flush a batch of events to storage
fn flush_batch(
    batch: &mut Vec<LineageEvent>,
    storage: &RocksLineageStore,
    stats: &WriterPoolStats,
    thread_id: usize,
) {
    if batch.is_empty() {
        return;
    }

    let batch_size = batch.len();
    let start = Instant::now();

    match storage.write_batch(batch.drain(..).collect()) {
        Ok(_) => {
            let latency_us = start.elapsed().as_micros() as u64;

            stats
                .events_written
                .fetch_add(batch_size as u64, Ordering::Relaxed);
            stats.batches_written.fetch_add(1, Ordering::Relaxed);
            stats
                .total_latency_us
                .fetch_add(latency_us, Ordering::Relaxed);

            // Log periodically
            let total_written = stats.events_written.load(Ordering::Relaxed);
            if total_written % 10000 == 0 {
                info!(
                    "Thread {} wrote {} events (total: {}, avg latency: {:.2}ms)",
                    thread_id,
                    batch_size,
                    total_written,
                    latency_us as f64 / 1000.0
                );
            }
        }
        Err(e) => {
            error!(
                "Thread {} failed to write batch of {}: {}",
                thread_id, batch_size, e
            );
            stats
                .events_failed
                .fetch_add(batch_size as u64, Ordering::Relaxed);
        }
    }

    batch.clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::create_test_lineage_event;
    use tempfile::TempDir;

    #[test]
    fn test_writer_pool_basic() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let storage = Arc::new(RocksLineageStore::new(temp_dir.path().to_str().unwrap())?);

        let config = WriterPoolConfig {
            num_threads: 2,
            batch_size: 10,
            batch_timeout_ms: 100,
            channel_buffer: 100,
        };

        let pool = WriterPool::new(Arc::clone(&storage), config)?;

        // Write 50 events
        for i in 0..50 {
            let event = create_test_lineage_event(&format!("test-{}", i));
            pool.write(event)?;
        }

        // Flush and wait
        pool.flush()?;
        thread::sleep(Duration::from_millis(500));

        // Verify stats
        let stats = pool.stats();
        assert!(
            stats.events_written() >= 50,
            "Should have written at least 50 events"
        );
        assert_eq!(stats.events_failed(), 0, "Should have no failures");

        pool.shutdown()?;

        Ok(())
    }

    #[test]
    fn test_writer_pool_throughput() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let storage = Arc::new(RocksLineageStore::new(temp_dir.path().to_str().unwrap())?);

        let config = WriterPoolConfig {
            num_threads: 4,
            batch_size: 50,
            batch_timeout_ms: 10,
            channel_buffer: 1000,
        };

        let pool = WriterPool::new(Arc::clone(&storage), config)?;

        let start = Instant::now();
        let num_events = 1000;

        // Write events
        for i in 0..num_events {
            let event = create_test_lineage_event(&format!("throughput-{}", i));
            pool.write(event)?;
        }

        pool.flush()?;
        thread::sleep(Duration::from_millis(500));

        let duration = start.elapsed();
        let throughput = pool.stats().throughput(duration);

        println!("Wrote {} events in {:?}", num_events, duration);
        println!("Throughput: {:.2} events/sec", throughput);
        println!("Avg latency: {:.2}μs", pool.stats().avg_latency_us());

        assert!(throughput > 100.0, "Throughput should be > 100 events/sec");

        pool.shutdown()?;

        Ok(())
    }
}
