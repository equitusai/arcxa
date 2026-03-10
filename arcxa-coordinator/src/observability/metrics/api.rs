//! HTTP API metrics
//!
//! Tracks HTTP request/response metrics:
//! - Request counts by endpoint and status code
//! - Request latency distribution
//! - In-flight request gauge
//! - Request body sizes

use anyhow::Result;
use prometheus::{
    exponential_buckets, HistogramOpts, HistogramVec, IntCounterVec, IntGauge, Opts, Registry,
};

/// HTTP API metrics
///
/// Monitors REST API performance and usage patterns.
pub struct ApiMetrics {
    requests_total: IntCounterVec,
    request_duration_seconds: HistogramVec,
    requests_in_flight: IntGauge,
    request_size_bytes: HistogramVec,
}

impl ApiMetrics {
    /// Create and register API metrics
    pub fn new(registry: &Registry) -> Result<Self> {
        let requests_total = IntCounterVec::new(
            Opts::new(
                "graphica_http_requests_total",
                "Total HTTP requests by endpoint and status code",
            ),
            &["method", "endpoint", "status"],
        )?;

        let request_duration_seconds = HistogramVec::new(
            HistogramOpts::new(
                "graphica_http_request_duration_seconds",
                "HTTP request latency in seconds",
            )
            .buckets(exponential_buckets(0.001, 2.0, 10)?), // 1ms to ~1s
            &["method", "endpoint"],
        )?;

        let requests_in_flight = IntGauge::new(
            "graphica_http_requests_in_flight",
            "Current number of HTTP requests being processed",
        )?;

        let request_size_bytes = HistogramVec::new(
            HistogramOpts::new(
                "graphica_http_request_size_bytes",
                "HTTP request body size in bytes",
            )
            .buckets(exponential_buckets(100.0, 10.0, 7)?), // 100B to ~100MB
            &["method", "endpoint"],
        )?;

        registry.register(Box::new(requests_total.clone()))?;
        registry.register(Box::new(request_duration_seconds.clone()))?;
        registry.register(Box::new(requests_in_flight.clone()))?;
        registry.register(Box::new(request_size_bytes.clone()))?;

        Ok(Self {
            requests_total,
            request_duration_seconds,
            requests_in_flight,
            request_size_bytes,
        })
    }

    /// Record request completion
    pub fn record_request(&self, method: &str, endpoint: &str, status: u16, duration_secs: f64) {
        self.requests_total
            .with_label_values(&[method, endpoint, &status.to_string()])
            .inc();

        self.request_duration_seconds
            .with_label_values(&[method, endpoint])
            .observe(duration_secs);
    }

    /// Increment in-flight request counter
    pub fn request_started(&self) {
        self.requests_in_flight.inc();
    }

    /// Decrement in-flight request counter
    pub fn request_finished(&self) {
        self.requests_in_flight.dec();
    }

    /// Record request body size
    pub fn record_request_size(&self, method: &str, endpoint: &str, size_bytes: usize) {
        self.request_size_bytes
            .with_label_values(&[method, endpoint])
            .observe(size_bytes as f64);
    }
}
