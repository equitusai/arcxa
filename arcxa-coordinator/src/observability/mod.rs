//! Observability module
//!
//! Production-grade observability for Graphica coordinator:
//! - Prometheus metrics export
//! - Request tracing with correlation IDs
//! - Structured logging integration
//! - HTTP metrics middleware
//!
//! Architecture:
//! - `metrics`: Metric definitions and registry
//! - `middleware`: HTTP middleware for metrics collection
//! - `export`: Prometheus exporter endpoint

pub mod export;
pub mod metrics;
pub mod middleware;

pub use export::metrics_handler;
pub use metrics::{
    ApiMetrics, ErrorMetrics, MetricsRegistry, RdfMetrics, ShardMetrics, SystemMetrics,
};
pub use middleware::{MetricsLayer, RequestIdLayer};

use anyhow::Result;

/// Initialize observability subsystem
///
/// Sets up metrics registry and returns components for wiring into application.
pub fn initialize() -> Result<MetricsRegistry> {
    let registry = MetricsRegistry::new()?;
    tracing::info!("Observability initialized with Prometheus metrics");
    Ok(registry)
}
