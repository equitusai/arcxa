//! # Automated Maintenance for Temporal Indexes
//!
//! This module provides automated maintenance tasks for temporal indexes:
//! - Scheduled compaction to optimize disk usage and read performance
//! - Archive coordination to move old versions to cold storage
//! - Statistics collection for monitoring and alerting
//!
//! ## Usage
//!
//! ```ignore
//! use graphica::governance::bitemporal::{TemporalIndexes, maintenance::MaintenanceScheduler};
//! use std::sync::Arc;
//! use std::time::Duration;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let indexes = Arc::new(TemporalIndexes::new("/path/to/indexes")?);
//!
//! // Create scheduler with default configuration
//! let scheduler = MaintenanceScheduler::new(indexes.clone())
//!     .with_compaction_interval(Duration::from_secs(3600 * 24))  // Daily
//!     .with_archive_threshold(730)  // 2 years
//!     .with_stats_collection_interval(Duration::from_secs(300));  // 5 minutes
//!
//! // Start background tasks
//! scheduler.start().await?;
//!
//! // Scheduler runs until dropped or explicitly stopped
//! # Ok(())
//! # }
//! ```ignore

use anyhow::{Context, Result};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::{interval, sleep};
use tracing::{debug, error, info, warn};

use super::indexes::TemporalIndexes;
use super::metrics;

/// Configuration for maintenance scheduler
#[derive(Debug, Clone)]
pub struct MaintenanceConfig {
    /// Interval between compaction runs (default: 24 hours)
    pub compaction_interval: Duration,

    /// Interval between statistics collection (default: 5 minutes)
    pub stats_interval: Duration,

    /// Archive versions older than this many days (default: 730 = 2 years)
    pub archive_threshold_days: usize,

    /// Interval between archive checks (default: 12 hours)
    pub archive_interval: Duration,

    /// Maximum versions per chain before triggering archive (default: 1000)
    pub max_versions_per_chain: usize,

    /// Enable automatic compaction (default: true)
    pub enable_compaction: bool,

    /// Enable automatic archival (default: false, requires manual configuration)
    pub enable_archival: bool,

    /// Enable statistics collection (default: true)
    pub enable_stats_collection: bool,
}

impl Default for MaintenanceConfig {
    fn default() -> Self {
        Self {
            compaction_interval: Duration::from_secs(3600 * 24), // Daily
            stats_interval: Duration::from_secs(300),            // 5 minutes
            archive_threshold_days: 730,                         // 2 years
            archive_interval: Duration::from_secs(3600 * 12),    // Twice daily
            max_versions_per_chain: 1000,
            enable_compaction: true,
            enable_archival: false, // Disabled by default
            enable_stats_collection: true,
        }
    }
}

/// Automated maintenance scheduler for temporal indexes
pub struct MaintenanceScheduler {
    indexes: Arc<TemporalIndexes>,
    config: MaintenanceConfig,
    shutdown_tx: Option<tokio::sync::broadcast::Sender<()>>,
}

impl MaintenanceScheduler {
    /// Create new maintenance scheduler with default configuration
    pub fn new(indexes: Arc<TemporalIndexes>) -> Self {
        Self {
            indexes,
            config: MaintenanceConfig::default(),
            shutdown_tx: None,
        }
    }

    /// Create with custom configuration
    pub fn with_config(indexes: Arc<TemporalIndexes>, config: MaintenanceConfig) -> Self {
        Self {
            indexes,
            config,
            shutdown_tx: None,
        }
    }

    /// Set compaction interval
    pub fn with_compaction_interval(mut self, interval: Duration) -> Self {
        self.config.compaction_interval = interval;
        self
    }

    /// Set archive threshold in days
    pub fn with_archive_threshold(mut self, days: usize) -> Self {
        self.config.archive_threshold_days = days;
        self
    }

    /// Set statistics collection interval
    pub fn with_stats_collection_interval(mut self, interval: Duration) -> Self {
        self.config.stats_interval = interval;
        self
    }

    /// Enable/disable automatic compaction
    pub fn with_compaction_enabled(mut self, enabled: bool) -> Self {
        self.config.enable_compaction = enabled;
        self
    }

    /// Enable/disable automatic archival
    pub fn with_archival_enabled(mut self, enabled: bool) -> Self {
        self.config.enable_archival = enabled;
        self
    }

    /// Start all maintenance tasks
    ///
    /// Spawns background tasks for:
    /// - Compaction (if enabled)
    /// - Archival (if enabled)
    /// - Statistics collection (if enabled)
    ///
    /// Tasks run until the scheduler is dropped or `stop()` is called.
    pub async fn start(&mut self) -> Result<()> {
        info!("Starting temporal index maintenance scheduler");
        info!(
            "  Compaction: {} (interval: {:?})",
            self.config.enable_compaction, self.config.compaction_interval
        );
        info!(
            "  Archival: {} (threshold: {} days)",
            self.config.enable_archival, self.config.archive_threshold_days
        );
        info!(
            "  Statistics: {} (interval: {:?})",
            self.config.enable_stats_collection, self.config.stats_interval
        );

        let (shutdown_tx, _) = tokio::sync::broadcast::channel(1);
        self.shutdown_tx = Some(shutdown_tx.clone());

        // Spawn compaction task
        if self.config.enable_compaction {
            let indexes = self.indexes.clone();
            let interval_duration = self.config.compaction_interval;
            let mut shutdown_rx = shutdown_tx.subscribe();

            tokio::spawn(async move {
                let mut ticker = interval(interval_duration);
                ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

                loop {
                    tokio::select! {
                        _ = ticker.tick() => {
                            if let Err(e) = run_compaction(&indexes).await {
                                error!("Compaction task failed: {}", e);
                                metrics::record_write("compaction", false, 0);
                            }
                        }
                        _ = shutdown_rx.recv() => {
                            info!("Compaction task shutting down");
                            break;
                        }
                    }
                }
            });
        }

        // Spawn archival task
        if self.config.enable_archival {
            let indexes = self.indexes.clone();
            let threshold = self.config.archive_threshold_days;
            let max_versions = self.config.max_versions_per_chain;
            let interval_duration = self.config.archive_interval;
            let mut shutdown_rx = shutdown_tx.subscribe();

            tokio::spawn(async move {
                let mut ticker = interval(interval_duration);
                ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

                loop {
                    tokio::select! {
                        _ = ticker.tick() => {
                            if let Err(e) = run_archival(&indexes, threshold, max_versions).await {
                                error!("Archival task failed: {}", e);
                            }
                        }
                        _ = shutdown_rx.recv() => {
                            info!("Archival task shutting down");
                            break;
                        }
                    }
                }
            });
        }

        // Spawn statistics collection task
        if self.config.enable_stats_collection {
            let indexes = self.indexes.clone();
            let interval_duration = self.config.stats_interval;
            let mut shutdown_rx = shutdown_tx.subscribe();

            tokio::spawn(async move {
                let mut ticker = interval(interval_duration);
                ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

                loop {
                    tokio::select! {
                        _ = ticker.tick() => {
                            if let Err(e) = collect_statistics(&indexes).await {
                                error!("Statistics collection failed: {}", e);
                            }
                        }
                        _ = shutdown_rx.recv() => {
                            info!("Statistics collection task shutting down");
                            break;
                        }
                    }
                }
            });
        }

        info!("Maintenance scheduler started successfully");
        Ok(())
    }

    /// Stop all maintenance tasks gracefully
    pub async fn stop(&self) -> Result<()> {
        if let Some(ref shutdown_tx) = self.shutdown_tx {
            info!("Stopping maintenance scheduler");
            let _ = shutdown_tx.send(());

            // Give tasks time to finish current work
            sleep(Duration::from_secs(2)).await;

            info!("Maintenance scheduler stopped");
        }
        Ok(())
    }

    /// Get reference to current configuration
    pub fn config(&self) -> &MaintenanceConfig {
        &self.config
    }
}

/// Run compaction task
async fn run_compaction(indexes: &Arc<TemporalIndexes>) -> Result<()> {
    info!("Starting scheduled compaction");
    let start = std::time::Instant::now();

    // Run compaction in blocking task to avoid blocking async runtime
    let indexes_clone = indexes.clone();
    tokio::task::spawn_blocking(move || indexes_clone.compact_database()).await??;

    let duration_ms = start.elapsed().as_millis() as u64;
    info!("Scheduled compaction completed in {}ms", duration_ms);
    metrics::record_write("scheduled_compaction", true, duration_ms * 1000);

    Ok(())
}

/// Run archival task
async fn run_archival(
    indexes: &Arc<TemporalIndexes>,
    threshold_days: usize,
    max_versions: usize,
) -> Result<()> {
    info!(
        "Starting archival check (threshold: {} days, max: {} versions)",
        threshold_days, max_versions
    );

    // Analyze version chains to find candidates for archival
    let indexes_clone = indexes.clone();
    let analysis =
        tokio::task::spawn_blocking(move || indexes_clone.analyze_version_chains(max_versions))
            .await??;

    if analysis.long_chains.is_empty() {
        debug!("No version chains require archival");
        return Ok(());
    }

    info!(
        "Found {} version chains exceeding {} versions",
        analysis.long_chains.len(),
        max_versions
    );

    // Log chains that need archival (actual archival would move to cold storage)
    for chain in &analysis.long_chains {
        warn!(
            "Version chain '{}' has {} versions (exceeds threshold of {})",
            chain.sp_key, chain.version_count, max_versions
        );
    }

    // TODO: Implement actual archival to cold storage (S3/Parquet)
    // For now, we just identify candidates
    info!(
        "Archival check complete. {} chains need attention",
        analysis.long_chains.len()
    );

    Ok(())
}

/// Collect and log statistics
async fn collect_statistics(indexes: &Arc<TemporalIndexes>) -> Result<()> {
    debug!("Collecting temporal index statistics");

    let indexes_clone = indexes.clone();
    let stats = tokio::task::spawn_blocking(move || indexes_clone.get_statistics()).await??;

    // Log statistics for monitoring
    debug!(
        "Temporal index stats: {} versions, cache {}/{}, disk {} bytes",
        stats.total_versions, stats.cache_size, stats.cache_capacity, stats.disk_usage_bytes
    );

    // Calculate cache hit rate (would need to store previous values for rate calculation)
    let cache_utilization = if stats.cache_capacity > 0 {
        (stats.cache_size as f64 / stats.cache_capacity as f64) * 100.0
    } else {
        0.0
    };

    if cache_utilization > 90.0 {
        warn!(
            "Cache utilization high: {:.1}% - consider increasing cache size",
            cache_utilization
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_maintenance_config_defaults() {
        let config = MaintenanceConfig::default();
        assert_eq!(config.compaction_interval, Duration::from_secs(3600 * 24));
        assert_eq!(config.archive_threshold_days, 730);
        assert!(config.enable_compaction);
        assert!(!config.enable_archival);
        assert!(config.enable_stats_collection);
    }

    #[tokio::test]
    async fn test_maintenance_scheduler_creation() {
        let temp_dir = TempDir::new().unwrap();
        let indexes = Arc::new(TemporalIndexes::new(temp_dir.path().join("indexes")).unwrap());

        let scheduler = MaintenanceScheduler::new(indexes.clone())
            .with_compaction_interval(Duration::from_secs(60))
            .with_archive_threshold(365)
            .with_stats_collection_interval(Duration::from_secs(30));

        assert_eq!(
            scheduler.config.compaction_interval,
            Duration::from_secs(60)
        );
        assert_eq!(scheduler.config.archive_threshold_days, 365);
        assert_eq!(scheduler.config.stats_interval, Duration::from_secs(30));
    }

    #[tokio::test]
    async fn test_compaction_task() {
        let temp_dir = TempDir::new().unwrap();
        let indexes = Arc::new(TemporalIndexes::new(temp_dir.path().join("indexes")).unwrap());

        // Run compaction task once
        let result = run_compaction(&indexes).await;
        assert!(result.is_ok(), "Compaction should succeed");
    }

    #[tokio::test]
    async fn test_statistics_collection() {
        let temp_dir = TempDir::new().unwrap();
        let indexes = Arc::new(TemporalIndexes::new(temp_dir.path().join("indexes")).unwrap());

        // Collect statistics
        let result = collect_statistics(&indexes).await;
        assert!(result.is_ok(), "Statistics collection should succeed");
    }
}
