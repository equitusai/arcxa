//! Shard Operational Metrics
//!
//! Tracks shard-specific operational metrics including query counts, latencies,
//! and error rates.

use parking_lot::RwLock;
use std::sync::Arc;
use std::time::Instant;

/// Shard operational metrics
#[derive(Debug, Clone)]
pub struct ShardMetrics {
    /// Total queries processed
    pub queries_processed: u64,

    /// Total inserts processed
    pub inserts_processed: u64,

    /// Total deletes processed
    pub deletes_processed: u64,

    /// Number of query errors
    pub query_errors: u64,

    /// Number of insert errors
    pub insert_errors: u64,

    /// 50th percentile query latency (ms)
    pub p50_latency_ms: f64,

    /// 95th percentile query latency (ms)
    pub p95_latency_ms: f64,

    /// 99th percentile query latency (ms)
    pub p99_latency_ms: f64,

    /// Replication lag in milliseconds
    pub replication_lag_ms: u64,
}

/// Internal metrics state
#[derive(Debug)]
struct MetricsState {
    queries_processed: u64,
    inserts_processed: u64,
    deletes_processed: u64,
    query_errors: u64,
    insert_errors: u64,

    /// Recent query latencies (circular buffer)
    recent_latencies: Vec<f64>,
    latency_index: usize,
    latency_capacity: usize,

    replication_lag_ms: u64,

    /// Timestamp tracking for throughput calculation (time window: 60 seconds)
    /// Stores (timestamp, query_count, insert_count) tuples
    recent_operations: Vec<(Instant, u64, u64)>,
    operations_window_secs: u64,
    last_snapshot_time: Instant,
    last_snapshot_queries: u64,
    last_snapshot_inserts: u64,
}

/// Collector for shard operational metrics
#[derive(Clone)]
pub struct ShardMetricsCollector {
    state: Arc<RwLock<MetricsState>>,
}

impl ShardMetricsCollector {
    /// Create a new shard metrics collector
    pub fn new() -> Self {
        Self::with_capacity(1000)
    }

    /// Create a new collector with specified latency tracking capacity
    pub fn with_capacity(capacity: usize) -> Self {
        let now = Instant::now();
        Self {
            state: Arc::new(RwLock::new(MetricsState {
                queries_processed: 0,
                inserts_processed: 0,
                deletes_processed: 0,
                query_errors: 0,
                insert_errors: 0,
                recent_latencies: Vec::with_capacity(capacity),
                latency_index: 0,
                latency_capacity: capacity,
                replication_lag_ms: 0,
                recent_operations: Vec::new(),
                operations_window_secs: 60,
                last_snapshot_time: now,
                last_snapshot_queries: 0,
                last_snapshot_inserts: 0,
            })),
        }
    }

    /// Calculate queries per second from recent window
    pub fn queries_per_second(&self) -> f64 {
        let state = self.state.read();
        state.calculate_queries_per_second()
    }

    /// Calculate inserts per second from recent window
    pub fn inserts_per_second(&self) -> f64 {
        let state = self.state.read();
        state.calculate_inserts_per_second()
    }

    /// Record a successful query
    pub fn record_query_success(&self, latency_ms: f64) {
        let mut state = self.state.write();
        state.queries_processed += 1;
        state.add_latency(latency_ms);
    }

    /// Record a query error
    pub fn record_query_error(&self) {
        let mut state = self.state.write();
        state.queries_processed += 1;
        state.query_errors += 1;
    }

    /// Record a successful insert
    pub fn record_insert_success(&self) {
        let mut state = self.state.write();
        state.inserts_processed += 1;
    }

    /// Record an insert error
    pub fn record_insert_error(&self) {
        let mut state = self.state.write();
        state.inserts_processed += 1;
        state.insert_errors += 1;
    }

    /// Record a successful delete
    pub fn record_delete_success(&self) {
        let mut state = self.state.write();
        state.deletes_processed += 1;
    }

    /// Update replication lag
    pub fn update_replication_lag(&self, lag_ms: u64) {
        let mut state = self.state.write();
        state.replication_lag_ms = lag_ms;
    }

    /// Get current metrics snapshot
    pub fn snapshot(&self) -> ShardMetrics {
        let state = self.state.read();

        // Calculate percentiles from recent latencies
        let (p50, p95, p99) = if state.recent_latencies.is_empty() {
            (0.0, 0.0, 0.0)
        } else {
            let mut sorted = state.recent_latencies.clone();
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

            let len = sorted.len();
            let p50_idx = (len as f64 * 0.50) as usize;
            let p95_idx = (len as f64 * 0.95) as usize;
            let p99_idx = (len as f64 * 0.99) as usize;

            (
                sorted[p50_idx.min(len - 1)],
                sorted[p95_idx.min(len - 1)],
                sorted[p99_idx.min(len - 1)],
            )
        };

        ShardMetrics {
            queries_processed: state.queries_processed,
            inserts_processed: state.inserts_processed,
            deletes_processed: state.deletes_processed,
            query_errors: state.query_errors,
            insert_errors: state.insert_errors,
            p50_latency_ms: p50,
            p95_latency_ms: p95,
            p99_latency_ms: p99,
            replication_lag_ms: state.replication_lag_ms,
        }
    }

    /// Reset all counters (useful for testing)
    pub fn reset(&self) {
        let now = Instant::now();
        let mut state = self.state.write();
        state.queries_processed = 0;
        state.inserts_processed = 0;
        state.deletes_processed = 0;
        state.query_errors = 0;
        state.insert_errors = 0;
        state.recent_latencies.clear();
        state.latency_index = 0;
        state.replication_lag_ms = 0;
        state.recent_operations.clear();
        state.last_snapshot_time = now;
        state.last_snapshot_queries = 0;
        state.last_snapshot_inserts = 0;
    }
}

impl Default for ShardMetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricsState {
    /// Add a latency measurement to the circular buffer
    fn add_latency(&mut self, latency_ms: f64) {
        if self.recent_latencies.len() < self.latency_capacity {
            // Still filling up
            self.recent_latencies.push(latency_ms);
        } else {
            // Circular buffer - overwrite oldest
            self.recent_latencies[self.latency_index] = latency_ms;
            self.latency_index = (self.latency_index + 1) % self.latency_capacity;
        }
    }

    /// Calculate queries per second over the configured time window
    fn calculate_queries_per_second(&self) -> f64 {
        let now = Instant::now();
        let elapsed_secs = now.duration_since(self.last_snapshot_time).as_secs_f64();

        // Avoid division by zero
        if elapsed_secs < 0.001 {
            return 0.0;
        }

        // Calculate rate based on counter difference
        let queries_delta = self.queries_processed.saturating_sub(self.last_snapshot_queries);
        queries_delta as f64 / elapsed_secs
    }

    /// Calculate inserts per second over the configured time window
    fn calculate_inserts_per_second(&self) -> f64 {
        let now = Instant::now();
        let elapsed_secs = now.duration_since(self.last_snapshot_time).as_secs_f64();

        // Avoid division by zero
        if elapsed_secs < 0.001 {
            return 0.0;
        }

        // Calculate rate based on counter difference
        let inserts_delta = self.inserts_processed.saturating_sub(self.last_snapshot_inserts);
        inserts_delta as f64 / elapsed_secs
    }

    /// Update snapshot baseline for rate calculations (called periodically)
    fn update_snapshot_baseline(&mut self) {
        let now = Instant::now();

        // Clean up old operations beyond the window
        let cutoff = now - std::time::Duration::from_secs(self.operations_window_secs);
        self.recent_operations.retain(|(ts, _, _)| *ts > cutoff);

        // Update snapshot
        self.last_snapshot_time = now;
        self.last_snapshot_queries = self.queries_processed;
        self.last_snapshot_inserts = self.inserts_processed;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_collector() {
        let collector = ShardMetricsCollector::new();
        let metrics = collector.snapshot();

        assert_eq!(metrics.queries_processed, 0);
        assert_eq!(metrics.inserts_processed, 0);
        assert_eq!(metrics.deletes_processed, 0);
        assert_eq!(metrics.query_errors, 0);
        assert_eq!(metrics.insert_errors, 0);
        assert_eq!(metrics.p50_latency_ms, 0.0);
        assert_eq!(metrics.replication_lag_ms, 0);
    }

    #[test]
    fn test_record_query_success() {
        let collector = ShardMetricsCollector::new();

        collector.record_query_success(10.5);
        collector.record_query_success(20.3);
        collector.record_query_success(15.7);

        let metrics = collector.snapshot();
        assert_eq!(metrics.queries_processed, 3);
        assert_eq!(metrics.query_errors, 0);
        assert!(metrics.p50_latency_ms > 0.0);
    }

    #[test]
    fn test_record_query_error() {
        let collector = ShardMetricsCollector::new();

        collector.record_query_success(10.0);
        collector.record_query_error();
        collector.record_query_error();

        let metrics = collector.snapshot();
        assert_eq!(metrics.queries_processed, 3);
        assert_eq!(metrics.query_errors, 2);
    }

    #[test]
    fn test_record_inserts() {
        let collector = ShardMetricsCollector::new();

        collector.record_insert_success();
        collector.record_insert_success();
        collector.record_insert_error();

        let metrics = collector.snapshot();
        assert_eq!(metrics.inserts_processed, 3);
        assert_eq!(metrics.insert_errors, 1);
    }

    #[test]
    fn test_record_deletes() {
        let collector = ShardMetricsCollector::new();

        collector.record_delete_success();
        collector.record_delete_success();

        let metrics = collector.snapshot();
        assert_eq!(metrics.deletes_processed, 2);
    }

    #[test]
    fn test_percentile_calculation() {
        let collector = ShardMetricsCollector::with_capacity(100);

        // Record latencies: 1, 2, 3, ..., 100
        for i in 1..=100 {
            collector.record_query_success(i as f64);
        }

        let metrics = collector.snapshot();

        // P50 should be around 50
        assert!((metrics.p50_latency_ms - 50.0).abs() < 5.0);

        // P95 should be around 95
        assert!((metrics.p95_latency_ms - 95.0).abs() < 5.0);

        // P99 should be around 99
        assert!((metrics.p99_latency_ms - 99.0).abs() < 5.0);
    }

    #[test]
    fn test_circular_buffer_overflow() {
        let collector = ShardMetricsCollector::with_capacity(5);

        // Add more latencies than capacity
        for i in 1..=10 {
            collector.record_query_success(i as f64);
        }

        let metrics = collector.snapshot();

        // Should have processed 10 queries
        assert_eq!(metrics.queries_processed, 10);

        // But latency buffer should only have most recent 5 values (6-10)
        // P50 should be around 8
        assert!((metrics.p50_latency_ms - 8.0).abs() < 2.0);
    }

    #[test]
    fn test_update_replication_lag() {
        let collector = ShardMetricsCollector::new();

        collector.update_replication_lag(1500);

        let metrics = collector.snapshot();
        assert_eq!(metrics.replication_lag_ms, 1500);

        // Update again
        collector.update_replication_lag(2000);

        let metrics = collector.snapshot();
        assert_eq!(metrics.replication_lag_ms, 2000);
    }

    #[test]
    fn test_reset() {
        let collector = ShardMetricsCollector::new();

        // Add some data
        collector.record_query_success(10.0);
        collector.record_insert_success();
        collector.update_replication_lag(500);

        // Verify data exists
        let metrics_before = collector.snapshot();
        assert_eq!(metrics_before.queries_processed, 1);
        assert_eq!(metrics_before.inserts_processed, 1);

        // Reset
        collector.reset();

        // Verify reset
        let metrics_after = collector.snapshot();
        assert_eq!(metrics_after.queries_processed, 0);
        assert_eq!(metrics_after.inserts_processed, 0);
        assert_eq!(metrics_after.replication_lag_ms, 0);
    }

    #[test]
    fn test_concurrent_access() {
        use std::sync::Arc;
        use std::thread;

        let collector = Arc::new(ShardMetricsCollector::new());
        let mut handles = vec![];

        // Spawn multiple threads recording metrics concurrently
        for i in 0..10 {
            let collector_clone = collector.clone();
            let handle = thread::spawn(move || {
                for _ in 0..100 {
                    collector_clone.record_query_success(i as f64);
                }
            });
            handles.push(handle);
        }

        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }

        let metrics = collector.snapshot();
        assert_eq!(metrics.queries_processed, 1000);
    }

    #[test]
    fn test_empty_percentiles() {
        let collector = ShardMetricsCollector::new();

        // No latencies recorded
        let metrics = collector.snapshot();

        assert_eq!(metrics.p50_latency_ms, 0.0);
        assert_eq!(metrics.p95_latency_ms, 0.0);
        assert_eq!(metrics.p99_latency_ms, 0.0);
    }

    #[test]
    fn test_single_latency() {
        let collector = ShardMetricsCollector::new();

        collector.record_query_success(42.0);

        let metrics = collector.snapshot();

        // All percentiles should equal the single value
        assert_eq!(metrics.p50_latency_ms, 42.0);
        assert_eq!(metrics.p95_latency_ms, 42.0);
        assert_eq!(metrics.p99_latency_ms, 42.0);
    }
}
