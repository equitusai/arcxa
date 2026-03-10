//! Storage Layer Metrics
//!
//! Prometheus metrics for monitoring storage performance, write amplification,
//! and identifying bottlenecks in the lineage storage layer.

use once_cell::sync::Lazy;
use prometheus::{
    register_histogram_vec, register_int_counter_vec, register_int_gauge_vec, HistogramOpts,
    HistogramVec, IntCounterVec, IntGaugeVec,
};

/// Storage write latency histogram (microseconds)
/// Labels: storage_type (rocksdb, kafka, parquet), operation (write_event, update_index)
pub static STORAGE_WRITE_LATENCY_US: Lazy<HistogramVec> = Lazy::new(|| {
    register_histogram_vec!(
        "graphica_storage_write_latency_microseconds",
        "Storage write operation latency in microseconds",
        &["storage_type", "operation"],
        vec![
            100.0, 250.0, 500.0, 1_000.0, 2_500.0, 5_000.0, 10_000.0, 25_000.0, 50_000.0, 100_000.0
        ]
    )
    .unwrap()
});

/// Storage read latency histogram (microseconds)
/// Labels: storage_type, operation (get_by_record, get_by_model, get_by_time)
pub static STORAGE_READ_LATENCY_US: Lazy<HistogramVec> = Lazy::new(|| {
    register_histogram_vec!(
        "graphica_storage_read_latency_microseconds",
        "Storage read operation latency in microseconds",
        &["storage_type", "operation"],
        vec![50.0, 100.0, 250.0, 500.0, 1_000.0, 2_500.0, 5_000.0, 10_000.0, 25_000.0, 50_000.0]
    )
    .unwrap()
});

/// Total events written counter
/// Labels: storage_type, result (success, error)
pub static STORAGE_EVENTS_WRITTEN_TOTAL: Lazy<IntCounterVec> = Lazy::new(|| {
    register_int_counter_vec!(
        "graphica_storage_events_written_total",
        "Total number of lineage events written to storage",
        &["storage_type", "result"]
    )
    .unwrap()
});

/// Total events read counter
/// Labels: storage_type, operation
pub static STORAGE_EVENTS_READ_TOTAL: Lazy<IntCounterVec> = Lazy::new(|| {
    register_int_counter_vec!(
        "graphica_storage_events_read_total",
        "Total number of lineage events read from storage",
        &["storage_type", "operation"]
    )
    .unwrap()
});

/// RocksDB write amplification gauge
/// Tracks the ratio of actual disk writes to logical writes
/// Label: db_path
pub static ROCKSDB_WRITE_AMPLIFICATION: Lazy<IntGaugeVec> = Lazy::new(|| {
    register_int_gauge_vec!(
        "graphica_rocksdb_write_amplification_ratio",
        "RocksDB write amplification ratio (actual writes / logical writes)",
        &["db_path"]
    )
    .unwrap()
});

/// RocksDB operations counter
/// Labels: operation (put, get, delete, merge), column_family
pub static ROCKSDB_OPERATIONS_TOTAL: Lazy<IntCounterVec> = Lazy::new(|| {
    register_int_counter_vec!(
        "graphica_rocksdb_operations_total",
        "Total RocksDB operations by type",
        &["operation", "column_family"]
    )
    .unwrap()
});

/// Index update operations counter
/// Labels: index_name (by_record, by_model, by_time, by_dataset), operation (read, write)
pub static INDEX_UPDATE_OPERATIONS_TOTAL: Lazy<IntCounterVec> = Lazy::new(|| {
    register_int_counter_vec!(
        "graphica_index_update_operations_total",
        "Index update operations (includes read-modify-write)",
        &["index_name", "operation"]
    )
    .unwrap()
});

/// Index size gauge (number of keys)
/// Labels: index_name
pub static INDEX_SIZE_KEYS: Lazy<IntGaugeVec> = Lazy::new(|| {
    register_int_gauge_vec!(
        "graphica_index_size_keys",
        "Number of keys in each index",
        &["index_name"]
    )
    .unwrap()
});

/// Index entry size histogram (bytes)
/// Labels: index_name
pub static INDEX_ENTRY_SIZE_BYTES: Lazy<HistogramVec> = Lazy::new(|| {
    register_histogram_vec!(
        "graphica_index_entry_size_bytes",
        "Size of index entries in bytes",
        &["index_name"],
        vec![
            100.0,
            500.0,
            1_000.0,
            5_000.0,
            10_000.0,
            50_000.0,
            100_000.0,
            500_000.0,
            1_000_000.0
        ]
    )
    .unwrap()
});

/// Storage throughput gauge (events/second)
/// Labels: storage_type, window (1m, 5m, 15m)
pub static STORAGE_THROUGHPUT_EPS: Lazy<IntGaugeVec> = Lazy::new(|| {
    register_int_gauge_vec!(
        "graphica_storage_throughput_events_per_second",
        "Storage write throughput in events per second",
        &["storage_type", "window"]
    )
    .unwrap()
});

/// RocksDB compaction statistics
/// Labels: level
pub static ROCKSDB_COMPACTION_BYTES: Lazy<IntCounterVec> = Lazy::new(|| {
    register_int_counter_vec!(
        "graphica_rocksdb_compaction_bytes_total",
        "Bytes written during RocksDB compaction",
        &["level"]
    )
    .unwrap()
});

/// RocksDB block cache hit rate
/// Labels: db_path
pub static ROCKSDB_BLOCK_CACHE_HIT_RATE: Lazy<IntGaugeVec> = Lazy::new(|| {
    register_int_gauge_vec!(
        "graphica_rocksdb_block_cache_hit_rate_percent",
        "RocksDB block cache hit rate percentage",
        &["db_path"]
    )
    .unwrap()
});

/// Storage error counter
/// Labels: storage_type, error_type (timeout, disk_full, corruption, other)
pub static STORAGE_ERRORS_TOTAL: Lazy<IntCounterVec> = Lazy::new(|| {
    register_int_counter_vec!(
        "graphica_storage_errors_total",
        "Total storage errors by type",
        &["storage_type", "error_type"]
    )
    .unwrap()
});

/// Batch write size histogram (number of events)
/// Labels: storage_type
pub static BATCH_WRITE_SIZE_EVENTS: Lazy<HistogramVec> = Lazy::new(|| {
    register_histogram_vec!(
        "graphica_batch_write_size_events",
        "Number of events in batch write operations",
        &["storage_type"],
        vec![1.0, 5.0, 10.0, 25.0, 50.0, 100.0, 250.0, 500.0, 1000.0]
    )
    .unwrap()
});

/// Helper functions for recording metrics

/// Record a storage write operation
pub fn record_write(storage_type: &str, operation: &str, latency_us: u64, success: bool) {
    STORAGE_WRITE_LATENCY_US
        .with_label_values(&[storage_type, operation])
        .observe(latency_us as f64);

    STORAGE_EVENTS_WRITTEN_TOTAL
        .with_label_values(&[storage_type, if success { "success" } else { "error" }])
        .inc();
}

/// Record a storage read operation
pub fn record_read(storage_type: &str, operation: &str, latency_us: u64, events_returned: usize) {
    STORAGE_READ_LATENCY_US
        .with_label_values(&[storage_type, operation])
        .observe(latency_us as f64);

    STORAGE_EVENTS_READ_TOTAL
        .with_label_values(&[storage_type, operation])
        .inc_by(events_returned as u64);
}

/// Record a RocksDB operation
pub fn record_rocksdb_op(operation: &str, column_family: &str) {
    ROCKSDB_OPERATIONS_TOTAL
        .with_label_values(&[operation, column_family])
        .inc();
}

/// Record an index update operation (read-modify-write pattern)
pub fn record_index_update(index_name: &str, read_size_bytes: usize, write_size_bytes: usize) {
    // Track the read operation
    INDEX_UPDATE_OPERATIONS_TOTAL
        .with_label_values(&[index_name, "read"])
        .inc();

    // Track the write operation
    INDEX_UPDATE_OPERATIONS_TOTAL
        .with_label_values(&[index_name, "write"])
        .inc();

    // Record the size of the index entry
    INDEX_ENTRY_SIZE_BYTES
        .with_label_values(&[index_name])
        .observe(write_size_bytes as f64);
}

/// Update index size gauge
pub fn update_index_size(index_name: &str, num_keys: i64) {
    INDEX_SIZE_KEYS
        .with_label_values(&[index_name])
        .set(num_keys);
}

/// Record storage error
pub fn record_storage_error(storage_type: &str, error_type: &str) {
    STORAGE_ERRORS_TOTAL
        .with_label_values(&[storage_type, error_type])
        .inc();
}

/// Update RocksDB statistics from RocksDB properties
pub fn update_rocksdb_stats(db_path: &str, stats: &RocksDbStats) {
    ROCKSDB_WRITE_AMPLIFICATION
        .with_label_values(&[db_path])
        .set(stats.write_amplification as i64);

    ROCKSDB_BLOCK_CACHE_HIT_RATE
        .with_label_values(&[db_path])
        .set(stats.block_cache_hit_rate_percent as i64);

    for (level, bytes) in &stats.compaction_bytes_by_level {
        ROCKSDB_COMPACTION_BYTES
            .with_label_values(&[&level.to_string()])
            .inc_by(*bytes);
    }
}

/// RocksDB statistics snapshot
#[derive(Debug, Clone)]
pub struct RocksDbStats {
    pub write_amplification: f64,
    pub block_cache_hit_rate_percent: f64,
    pub compaction_bytes_by_level: Vec<(usize, u64)>,
    pub total_sst_files: usize,
    pub total_sst_bytes: u64,
}

impl Default for RocksDbStats {
    fn default() -> Self {
        Self {
            write_amplification: 1.0,
            block_cache_hit_rate_percent: 0.0,
            compaction_bytes_by_level: Vec::new(),
            total_sst_files: 0,
            total_sst_bytes: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_write_metrics() {
        record_write("rocksdb", "write_event", 5_000, true);
        record_write("rocksdb", "write_event", 3_000, false);

        // Verify metrics are recorded (just smoke test - actual values checked via Prometheus)
        let latency_metric = STORAGE_WRITE_LATENCY_US
            .get_metric_with_label_values(&["rocksdb", "write_event"])
            .unwrap();
        assert!(latency_metric.get_sample_count() >= 2);
    }

    #[test]
    fn test_record_index_update() {
        record_index_update("by_model", 10_000, 12_000);

        let read_count = INDEX_UPDATE_OPERATIONS_TOTAL
            .get_metric_with_label_values(&["by_model", "read"])
            .unwrap()
            .get();
        assert!(read_count >= 1);

        let write_count = INDEX_UPDATE_OPERATIONS_TOTAL
            .get_metric_with_label_values(&["by_model", "write"])
            .unwrap()
            .get();
        assert!(write_count >= 1);
    }

    #[test]
    fn test_rocksdb_stats_update() {
        let stats = RocksDbStats {
            write_amplification: 7.0,
            block_cache_hit_rate_percent: 85.0,
            compaction_bytes_by_level: vec![(0, 1_000_000), (1, 5_000_000)],
            total_sst_files: 42,
            total_sst_bytes: 50_000_000,
        };

        update_rocksdb_stats("/tmp/test_db", &stats);

        let write_amp = ROCKSDB_WRITE_AMPLIFICATION
            .get_metric_with_label_values(&["/tmp/test_db"])
            .unwrap()
            .get();
        assert_eq!(write_amp, 7);
    }
}
