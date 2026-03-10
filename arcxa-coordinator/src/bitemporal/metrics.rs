//! # Prometheus Metrics for Temporal Indexes
//!
//! Exposes temporal index performance metrics for production monitoring:
//! - Lookup operations (by type, status)
//! - Lookup latency (p50, p95, p99)
//! - Cache hit/miss rates
//! - Index write operations
//! - Error tracking

use lazy_static::lazy_static;
use once_cell::sync::Lazy;
use prometheus::{
    register_counter_vec, register_gauge, register_histogram_vec, CounterVec, Gauge, HistogramVec,
    Opts, Registry,
};

// Test-safe metric registration: returns a dummy metric on AlreadyReg errors
fn register_counter_vec_safe(opts: Opts, label_names: &[&str]) -> CounterVec {
    match register_counter_vec!(opts.clone(), label_names) {
        Ok(metric) => metric,
        Err(prometheus::Error::AlreadyReg) => {
            // Metric already registered (happens when tests run in different module contexts)
            // Return a new local metric that won't be exported but will work for tests
            CounterVec::new(opts, label_names).expect("Failed to create local counter_vec")
        }
        Err(e) => panic!("Failed to register counter_vec: {:?}", e),
    }
}

fn register_histogram_vec_safe(
    name: &str,
    help: &str,
    label_names: &[&str],
    buckets: Vec<f64>,
) -> HistogramVec {
    match register_histogram_vec!(name, help, label_names, buckets.clone()) {
        Ok(metric) => metric,
        Err(prometheus::Error::AlreadyReg) => {
            // Return a new local metric for tests
            HistogramVec::new(
                prometheus::HistogramOpts::new(name, help).buckets(buckets),
                label_names,
            )
            .expect("Failed to create local histogram_vec")
        }
        Err(e) => panic!("Failed to register histogram_vec: {:?}", e),
    }
}

fn register_gauge_safe(name: &str, help: &str) -> Gauge {
    match register_gauge!(name, help) {
        Ok(metric) => metric,
        Err(prometheus::Error::AlreadyReg) => {
            // Return a new local metric for tests
            Gauge::new(name, help).expect("Failed to create local gauge")
        }
        Err(e) => panic!("Failed to register gauge: {:?}", e),
    }
}

lazy_static! {
    // ========================================
    // Lookup Operations
    // ========================================

    /// Total index lookups by type and status
    /// type: current_version, version_at, version_chain, version_by_id
    /// status: success, error
    pub static ref TEMPORAL_LOOKUPS: CounterVec = register_counter_vec_safe(
        Opts::new(
            "graphica_temporal_lookups_total",
            "Total temporal index lookup operations"
        ),
        &["lookup_type", "status"]
    );

    /// Lookup latency histogram (microseconds)
    pub static ref TEMPORAL_LOOKUP_DURATION: HistogramVec = register_histogram_vec_safe(
        "graphica_temporal_lookup_duration_microseconds",
        "Temporal index lookup latency in microseconds",
        &["lookup_type"],
        // Buckets: 10µs to 10ms
        vec![10.0, 25.0, 50.0, 100.0, 250.0, 500.0, 1000.0, 2500.0, 5000.0, 10000.0]
    );

    // ========================================
    // Cache Metrics
    // ========================================

    /// Cache hit rate counter
    pub static ref TEMPORAL_CACHE_HITS: CounterVec = register_counter_vec_safe(
        Opts::new(
            "graphica_temporal_cache_hits_total",
            "Total cache hits in temporal indexes"
        ),
        &["lookup_type"]
    );

    /// Cache miss rate counter
    pub static ref TEMPORAL_CACHE_MISSES: CounterVec = register_counter_vec_safe(
        Opts::new(
            "graphica_temporal_cache_misses_total",
            "Total cache misses in temporal indexes"
        ),
        &["lookup_type"]
    );

    /// Current cache size (gauge)
    pub static ref TEMPORAL_CACHE_SIZE: Gauge = register_gauge_safe(
        "graphica_temporal_cache_entries",
        "Current number of entries in LRU cache"
    );

    // ========================================
    // Write Operations
    // ========================================

    /// Index write operations (inserts, supersedes)
    pub static ref TEMPORAL_WRITES: CounterVec = register_counter_vec_safe(
        Opts::new(
            "graphica_temporal_writes_total",
            "Total temporal index write operations"
        ),
        &["operation", "status"]  // operation: index, supersede; status: success, error
    );

    /// Write operation latency (microseconds)
    pub static ref TEMPORAL_WRITE_DURATION: HistogramVec = register_histogram_vec_safe(
        "graphica_temporal_write_duration_microseconds",
        "Temporal index write latency in microseconds",
        &["operation"],
        vec![50.0, 100.0, 250.0, 500.0, 1000.0, 2500.0, 5000.0, 10000.0]
    );

    // ========================================
    // RocksDB Statistics
    // ========================================

    /// Total indexed versions
    pub static ref TEMPORAL_VERSIONS_INDEXED: CounterVec = register_counter_vec_safe(
        Opts::new(
            "graphica_temporal_versions_indexed_total",
            "Total versions indexed in temporal stores"
        ),
        &["column_family"]
    );

    /// Active version chains
    pub static ref TEMPORAL_VERSION_CHAINS: Gauge = register_gauge_safe(
        "graphica_temporal_version_chains_active",
        "Number of active version chains (unique subject+predicate pairs)"
    );

    // ========================================
    // Error Tracking
    // ========================================

    /// Errors by type
    pub static ref TEMPORAL_ERRORS: CounterVec = register_counter_vec_safe(
        Opts::new(
            "graphica_temporal_errors_total",
            "Total errors in temporal index operations"
        ),
        &["error_type"]  // missing_version, corrupted_metadata, rocksdb_error, etc.
    );
}

// ========================================
// Metric Recording Functions
// ========================================

/// Record a lookup operation
pub fn record_lookup(lookup_type: &str, success: bool, duration_micros: u64) {
    let status = if success { "success" } else { "error" };

    TEMPORAL_LOOKUPS
        .with_label_values(&[lookup_type, status])
        .inc();

    if success {
        TEMPORAL_LOOKUP_DURATION
            .with_label_values(&[lookup_type])
            .observe(duration_micros as f64);
    }
}

/// Record a cache hit or miss
pub fn record_cache_result(lookup_type: &str, is_hit: bool) {
    if is_hit {
        TEMPORAL_CACHE_HITS.with_label_values(&[lookup_type]).inc();
    } else {
        TEMPORAL_CACHE_MISSES
            .with_label_values(&[lookup_type])
            .inc();
    }
}

/// Update current cache size
pub fn update_cache_size(size: usize) {
    TEMPORAL_CACHE_SIZE.set(size as f64);
}

/// Record a write operation
pub fn record_write(operation: &str, success: bool, duration_micros: u64) {
    let status = if success { "success" } else { "error" };

    TEMPORAL_WRITES
        .with_label_values(&[operation, status])
        .inc();

    if success {
        TEMPORAL_WRITE_DURATION
            .with_label_values(&[operation])
            .observe(duration_micros as f64);
    }
}

/// Record a version being indexed
pub fn record_version_indexed(column_family: &str) {
    TEMPORAL_VERSIONS_INDEXED
        .with_label_values(&[column_family])
        .inc();
}

/// Update version chain count
pub fn update_version_chain_count(count: usize) {
    TEMPORAL_VERSION_CHAINS.set(count as f64);
}

/// Record an error
pub fn record_error(error_type: &str) {
    TEMPORAL_ERRORS.with_label_values(&[error_type]).inc();
}

// ========================================
// Computed Metrics
// ========================================

/// Get cache hit rate (0.0 to 1.0)
pub fn get_cache_hit_rate(lookup_type: &str) -> f64 {
    let hits = TEMPORAL_CACHE_HITS.with_label_values(&[lookup_type]).get();

    let misses = TEMPORAL_CACHE_MISSES
        .with_label_values(&[lookup_type])
        .get();

    let total = hits + misses;
    if total > 0.0 {
        hits / total
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    #[serial]
    fn test_record_lookup() {
        // Force lazy_static initialization by accessing the metric first
        let _ = &*TEMPORAL_LOOKUPS;

        record_lookup("current_version", true, 50);
        record_lookup("current_version", false, 0);

        let success_count = TEMPORAL_LOOKUPS
            .with_label_values(&["current_version", "success"])
            .get();

        assert!(success_count >= 1.0);
    }

    #[test]
    #[serial]
    fn test_cache_metrics() {
        // Force lazy_static initialization
        let _ = &*TEMPORAL_CACHE_HITS;
        let _ = &*TEMPORAL_CACHE_MISSES;
        let _ = &*TEMPORAL_CACHE_SIZE;

        record_cache_result("current_version", true);
        record_cache_result("current_version", false);

        let hit_rate = get_cache_hit_rate("current_version");
        assert!(hit_rate > 0.0 && hit_rate <= 1.0);

        update_cache_size(500);
        assert_eq!(TEMPORAL_CACHE_SIZE.get(), 500.0);
    }

    #[test]
    #[serial]
    fn test_write_metrics() {
        // Force lazy_static initialization
        let _ = &*TEMPORAL_WRITES;

        record_write("index", true, 100);
        record_write("supersede", true, 75);

        let index_count = TEMPORAL_WRITES
            .with_label_values(&["index", "success"])
            .get();

        assert!(index_count >= 1.0);
    }

    #[test]
    #[serial]
    fn test_error_tracking() {
        // Force lazy_static initialization
        let _ = &*TEMPORAL_ERRORS;

        record_error("missing_version");
        record_error("rocksdb_error");

        let error_count = TEMPORAL_ERRORS
            .with_label_values(&["missing_version"])
            .get();

        assert!(error_count >= 1.0);
    }
}
