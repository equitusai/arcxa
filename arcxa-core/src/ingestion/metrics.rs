//! Metrics stubs for graphica-core
//!
//! This module provides no-op metric stubs to allow core business logic to compile
//! without depending on prometheus. The actual metrics implementation (using prometheus)
//! is in graphica-coordinator.

use std::sync::atomic::{AtomicU64, Ordering};

/// No-op counter metric
pub struct Counter {
    value: AtomicU64,
}

impl Counter {
    const fn new() -> Self {
        Self {
            value: AtomicU64::new(0),
        }
    }

    #[inline]
    pub fn inc(&self) {
        // No-op in core, implemented in coordinator
        self.value.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn inc_by(&self, v: f64) {
        // No-op in core, implemented in coordinator
        self.value.fetch_add(v as u64, Ordering::Relaxed);
    }

    #[inline]
    pub fn with_label_values(&self, _labels: &[&str]) -> &Self {
        // No-op in core, implemented in coordinator
        self
    }
}

/// No-op gauge metric
pub struct Gauge {
    value: AtomicU64,
}

impl Gauge {
    const fn new() -> Self {
        Self {
            value: AtomicU64::new(0),
        }
    }

    #[inline]
    pub fn set(&self, _v: f64) {
        // No-op in core, implemented in coordinator
    }

    #[inline]
    pub fn inc(&self) {
        // No-op in core, implemented in coordinator
        self.value.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn dec(&self) {
        // No-op in core, implemented in coordinator
        self.value.fetch_sub(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn with_label_values(&self, _labels: &[&str]) -> &Self {
        // No-op in core, implemented in coordinator
        self
    }
}

/// No-op histogram metric
pub struct Histogram;

impl Histogram {
    const fn new() -> Self {
        Self
    }

    #[inline]
    pub fn observe(&self, _v: f64) {
        // No-op in core, implemented in coordinator
    }

    #[inline]
    pub fn start_timer(&self) -> HistogramTimer {
        HistogramTimer
    }

    #[inline]
    pub fn with_label_values(&self, _labels: &[&str]) -> &Self {
        // No-op in core, implemented in coordinator
        self
    }
}

/// No-op histogram timer
pub struct HistogramTimer;

impl HistogramTimer {
    #[inline]
    pub fn observe_duration(self) {
        // No-op in core, implemented in coordinator
    }
}

impl Drop for HistogramTimer {
    fn drop(&mut self) {
        // No-op in core, implemented in coordinator
    }
}

// Ingestion metrics
pub static RECORDS_PROCESSED: Counter = Counter::new();
pub static RECORDS_DROPPED: Counter = Counter::new();
pub static DEDUP_HITS: Counter = Counter::new();
pub static DEDUP_MAP_SIZE: Gauge = Gauge::new();
pub static PROCESSING_LATENCY: Histogram = Histogram::new();
pub static DEDUP_LATENCY: Histogram = Histogram::new();

// DLQ metrics
pub static DLQ_WRITES: Counter = Counter::new();
pub static DLQ_MEMORY_SIZE: Gauge = Gauge::new();
pub static DATA_LOSS_TOTAL: Counter = Counter::new();

// Checkpoint metrics
pub static CHECKPOINT_SIZE_BYTES: Gauge = Gauge::new();
pub static CHECKPOINT_WRITE_DURATION_MS: Histogram = Histogram::new();
pub static CHECKPOINT_ERRORS: Counter = Counter::new();

// Lineage metrics
pub static LINEAGE_EVENTS_SENT: Counter = Counter::new();
pub static LINEAGE_EVENTS_FAILED: Counter = Counter::new();
pub static LINEAGE_BATCH_SIZE: Histogram = Histogram::new();
pub static LINEAGE_SEND_LATENCY: Histogram = Histogram::new();
pub static LINEAGE_FLUSH_LATENCY: Histogram = Histogram::new();
pub static LINEAGE_RETRY_ATTEMPTS: Counter = Counter::new();
pub static LINEAGE_CHANNEL_DROPS: Counter = Counter::new();
pub static LINEAGE_BACKPRESSURE_EVENTS: Counter = Counter::new();

// Circuit breaker metrics
pub static CIRCUIT_BREAKER_OPEN: Gauge = Gauge::new();
pub static CIRCUIT_BREAKER_CLOSED: Gauge = Gauge::new();
pub static CIRCUIT_BREAKER_OPENED: Counter = Counter::new();

// Storage metrics
pub static STORAGE_RETRIES: Counter = Counter::new();
pub static MEMORY_USAGE_BYTES: Gauge = Gauge::new();
pub static STORAGE_WRITE_LATENCY: Histogram = Histogram::new();
pub static STORAGE_BATCH_SIZE: Histogram = Histogram::new();
