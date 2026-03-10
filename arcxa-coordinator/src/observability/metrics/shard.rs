//! Shard coordination metrics
//!
//! Tracks distributed shard operations:
//! - Shard request counts by shard and operation
//! - Shard request latency
//! - Shard health status
//! - Connection pool usage

use anyhow::Result;
use prometheus::{
    exponential_buckets, HistogramOpts, HistogramVec, IntCounterVec, IntGaugeVec, Opts, Registry,
};

/// Shard coordination metrics
///
/// Monitors distributed shard communication and health.
pub struct ShardMetrics {
    shard_requests_total: IntCounterVec,
    shard_request_duration_seconds: HistogramVec,
    shard_health: IntGaugeVec,
    shard_connections_active: IntGaugeVec,
}

impl ShardMetrics {
    /// Create and register shard metrics
    pub fn new(registry: &Registry) -> Result<Self> {
        let shard_requests_total = IntCounterVec::new(
            Opts::new(
                "graphica_shard_requests_total",
                "Total requests to shards by shard ID and operation",
            ),
            &["shard_id", "operation"],
        )?;

        let shard_request_duration_seconds = HistogramVec::new(
            HistogramOpts::new(
                "graphica_shard_request_duration_seconds",
                "Shard request latency in seconds",
            )
            .buckets(exponential_buckets(0.001, 2.0, 10)?),
            &["shard_id", "operation"],
        )?;

        let shard_health = IntGaugeVec::new(
            Opts::new(
                "graphica_shard_health",
                "Shard health status (1 = healthy, 0 = unhealthy)",
            ),
            &["shard_id"],
        )?;

        let shard_connections_active = IntGaugeVec::new(
            Opts::new(
                "graphica_shard_connections_active",
                "Active connections to shard",
            ),
            &["shard_id"],
        )?;

        registry.register(Box::new(shard_requests_total.clone()))?;
        registry.register(Box::new(shard_request_duration_seconds.clone()))?;
        registry.register(Box::new(shard_health.clone()))?;
        registry.register(Box::new(shard_connections_active.clone()))?;

        Ok(Self {
            shard_requests_total,
            shard_request_duration_seconds,
            shard_health,
            shard_connections_active,
        })
    }

    /// Record shard request
    pub fn record_request(&self, shard_id: u32, operation: &str, duration_secs: f64) {
        let shard_label = shard_id.to_string();

        self.shard_requests_total
            .with_label_values(&[shard_label.as_str(), operation])
            .inc();

        self.shard_request_duration_seconds
            .with_label_values(&[shard_label.as_str(), operation])
            .observe(duration_secs);
    }

    /// Set shard health status
    pub fn set_health(&self, shard_id: u32, healthy: bool) {
        let shard_label = shard_id.to_string();
        self.shard_health
            .with_label_values(&[shard_label.as_str()])
            .set(if healthy { 1 } else { 0 });
    }

    /// Set active connection count
    pub fn set_active_connections(&self, shard_id: u32, count: i64) {
        let shard_label = shard_id.to_string();
        self.shard_connections_active
            .with_label_values(&[shard_label.as_str()])
            .set(count);
    }
}
