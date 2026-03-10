//! Metrics registry
//!
//! Central registry for all Prometheus metrics.
//! Owns and manages metric instances for all subsystems.

use anyhow::Result;
use prometheus::{Encoder, Registry, TextEncoder};

use super::{
    ApiMetrics, ErrorMetrics, LoaderMetrics, RdfMetrics, ShardMetrics, SystemMetrics,
    WorkflowMetrics,
};

/// Central metrics registry
///
/// Aggregates all subsystem metrics into a single registry for export.
/// Each subsystem has its own metrics struct for clear separation.
pub struct MetricsRegistry {
    registry: Registry,
    pub api: ApiMetrics,
    pub rdf: RdfMetrics,
    pub shard: ShardMetrics,
    pub system: SystemMetrics,
    pub error: ErrorMetrics,
    pub loader: LoaderMetrics,
    pub workflow: WorkflowMetrics,
}

impl MetricsRegistry {
    /// Create new metrics registry
    ///
    /// Initializes all subsystem metrics and registers them with Prometheus.
    pub fn new() -> Result<Self> {
        let registry = Registry::new();

        let api = ApiMetrics::new(&registry)?;
        let rdf = RdfMetrics::new(&registry)?;
        let shard = ShardMetrics::new(&registry)?;
        let system = SystemMetrics::new(&registry)?;
        let error = ErrorMetrics::new(&registry)?;
        let loader = LoaderMetrics::new(&registry)?;
        let workflow = WorkflowMetrics::new(&registry)?;

        Ok(Self {
            registry,
            api,
            rdf,
            shard,
            system,
            error,
            loader,
            workflow,
        })
    }

    /// Gather metrics in Prometheus text format
    ///
    /// Returns serialized metrics suitable for HTTP export.
    pub fn gather(&self) -> Result<Vec<u8>> {
        let encoder = TextEncoder::new();
        let metric_families = self.registry.gather();
        let mut buffer = Vec::new();
        encoder.encode(&metric_families, &mut buffer)?;
        Ok(buffer)
    }

    /// Get reference to underlying registry
    pub fn registry(&self) -> &Registry {
        &self.registry
    }
}
