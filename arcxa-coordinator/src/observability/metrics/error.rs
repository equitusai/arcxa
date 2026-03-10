//! Error tracking metrics
//!
//! Categorizes and counts errors by:
//! - Component (API, RDF, shard coordinator)
//! - Error type
//! - Severity

use anyhow::Result;
use prometheus::{IntCounterVec, Opts, Registry};

/// Error tracking metrics
///
/// Monitors error rates across all subsystems.
pub struct ErrorMetrics {
    errors_total: IntCounterVec,
}

impl ErrorMetrics {
    /// Create and register error metrics
    pub fn new(registry: &Registry) -> Result<Self> {
        let errors_total = IntCounterVec::new(
            Opts::new(
                "graphica_errors_total",
                "Total errors by component and error type",
            ),
            &["component", "error_type", "severity"],
        )?;

        registry.register(Box::new(errors_total.clone()))?;

        Ok(Self { errors_total })
    }

    /// Record error occurrence
    pub fn record_error(&self, component: &str, error_type: &str, severity: &str) {
        self.errors_total
            .with_label_values(&[component, error_type, severity])
            .inc();
    }
}

/// Error severity levels
pub mod severity {
    pub const DEBUG: &str = "debug";
    pub const INFO: &str = "info";
    pub const WARNING: &str = "warning";
    pub const ERROR: &str = "error";
    pub const CRITICAL: &str = "critical";
}

/// Component names for error tracking
pub mod component {
    pub const API: &str = "api";
    pub const RDF: &str = "rdf";
    pub const SHARD: &str = "shard";
    pub const AUTH: &str = "auth";
    pub const STORAGE: &str = "storage";
}
