// System health metrics
//!
//! Tracks overall system health and build information:
//! - System uptime
//! - Build version and metadata
//! - Process resource usage

use anyhow::Result;
use prometheus::{IntCounterVec, IntGauge, Opts, Registry};
use std::time::Instant;

/// System health metrics
///
/// Monitors overall coordinator health and metadata.
pub struct SystemMetrics {
    up: IntGauge,
    build_info: IntCounterVec,
    start_time: Instant,
}

impl SystemMetrics {
    /// Create and register system metrics
    pub fn new(registry: &Registry) -> Result<Self> {
        let up = IntGauge::new("graphica_up", "Always 1 if coordinator is running")?;

        let build_info = IntCounterVec::new(
            Opts::new(
                "graphica_build_info",
                "Build information (version, git commit)",
            ),
            &["version", "git_commit", "rust_version"],
        )?;

        registry.register(Box::new(up.clone()))?;
        registry.register(Box::new(build_info.clone()))?;

        // Set up gauge to 1 immediately
        up.set(1);

        // Record build info
        let version = env!("CARGO_PKG_VERSION");
        let git_commit = option_env!("GIT_COMMIT").unwrap_or("unknown");
        let rust_version = option_env!("RUSTC_VERSION").unwrap_or("unknown");

        build_info
            .with_label_values(&[version, git_commit, rust_version])
            .inc();

        Ok(Self {
            up,
            build_info,
            start_time: Instant::now(),
        })
    }

    /// Get uptime in seconds
    pub fn uptime_seconds(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }

    /// Mark system as up
    pub fn mark_up(&self) {
        self.up.set(1);
    }

    /// Mark system as down
    pub fn mark_down(&self) {
        self.up.set(0);
    }
}
