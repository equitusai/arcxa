//! RocksDB Performance Monitor
//!
//! Collects runtime statistics and exports metrics for monitoring

use crate::workflows::storage::metrics::WorkflowStorageMetrics;
use anyhow::{Context, Result};
use rocksdb::{properties, DB};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::time::{interval, Duration};
use tracing::{debug, error, info, warn};

/// RocksDB performance monitor
pub struct RocksDbMonitor {
    db: Arc<DB>,
    metrics: Arc<WorkflowStorageMetrics>,
    collection_interval_secs: u64,
}

impl RocksDbMonitor {
    /// Create a new monitor
    pub fn new(db: Arc<DB>, metrics: Arc<WorkflowStorageMetrics>) -> Self {
        Self {
            db,
            metrics,
            collection_interval_secs: 10,
        }
    }

    /// Set collection interval
    pub fn with_interval(mut self, seconds: u64) -> Self {
        self.collection_interval_secs = seconds;
        self
    }

    /// Start monitoring in background
    pub async fn start_monitoring(self) {
        let mut interval = interval(Duration::from_secs(self.collection_interval_secs));

        info!(
            "Starting RocksDB monitoring with {}s interval",
            self.collection_interval_secs
        );

        loop {
            interval.tick().await;

            if let Err(e) = self.collect_statistics() {
                error!("Failed to collect RocksDB statistics: {}", e);
            }
        }
    }

    /// Collect statistics once
    pub fn collect_statistics(&self) -> Result<()> {
        // Collect global DB statistics
        self.collect_db_stats()?;

        // Collect per-column family statistics
        for cf_name in &["executions", "events", "checkpoints", "metadata"] {
            if let Some(cf) = self.db.cf_handle(cf_name) {
                self.collect_cf_stats(cf_name, cf)?;
            }
        }

        // Check for stalls and slowdowns
        self.check_write_stalls()?;

        // Update health status
        self.update_health_status();

        Ok(())
    }

    /// Collect database-wide statistics
    fn collect_db_stats(&self) -> Result<()> {
        // Memory usage
        if let Ok(mem_usage) = self.get_memory_usage() {
            debug!("RocksDB memory usage: {:?}", mem_usage);

            // Update metrics
            for (component, size) in mem_usage {
                self.metrics
                    .update_storage_size("rocksdb", &component, size as i64);
            }
        }

        // Compaction statistics
        if let Ok(Some(stats)) = self.db.property_value(properties::CFSTATS) {
            debug!("Compaction stats: {}", stats);
            // Parse and export relevant metrics
        }

        // Background errors
        if let Ok(Some(bg_error)) = self.db.property_int_value(properties::BACKGROUND_ERRORS) {
            if bg_error > 0 {
                warn!("RocksDB background errors: {}", bg_error);
                self.metrics
                    .record_error("background", "rocksdb", "background_error");
            }
        }

        Ok(())
    }

    /// Collect column family specific statistics
    fn collect_cf_stats(&self, cf_name: &str, cf: &rocksdb::ColumnFamily) -> Result<()> {
        // Estimate live data size
        if let Ok(Some(size)) = self
            .db
            .property_int_value_cf(cf, properties::ESTIMATE_LIVE_DATA_SIZE)
        {
            self.metrics
                .update_storage_size("rocksdb", &format!("{}_data", cf_name), size as i64);
        }

        // Number of keys
        if let Ok(Some(keys)) = self
            .db
            .property_int_value_cf(cf, properties::ESTIMATE_NUM_KEYS)
        {
            debug!("{} estimated keys: {}", cf_name, keys);
        }

        // Pending compaction bytes
        if let Ok(Some(pending)) = self
            .db
            .property_int_value_cf(cf, properties::ESTIMATE_PENDING_COMPACTION_BYTES)
        {
            if pending > 100 * 1024 * 1024 {
                // More than 100MB pending
                info!(
                    "{} has {} MB pending compaction",
                    cf_name,
                    pending / (1024 * 1024)
                );
            }
        }

        // Number of files per level
        // Note: NUM_FILES_AT_LEVEL_PREFIX not available in rocksdb v0.22
        // if let Ok(levels) = self.db.property_value_cf(cf, "rocksdb.num-files-at-level") {
        //     debug!("{} file distribution: {}", cf_name, levels);
        // }

        // Compression ratio
        // Note: COMPRESSION_RATIO_AT_LEVEL_PREFIX not available in rocksdb v0.22
        // if let Ok(ratio) = self.db.property_value_cf(cf, "rocksdb.compression-ratio-at-level") {
        //     debug!("{} compression ratios: {}", cf_name, ratio);
        // }

        // SST file count
        if let Ok(Some(total_sst)) = self
            .db
            .property_int_value_cf(cf, properties::TOTAL_SST_FILES_SIZE)
        {
            self.metrics.update_storage_size(
                "rocksdb",
                &format!("{}_sst", cf_name),
                total_sst as i64,
            );
        }

        Ok(())
    }

    /// Get memory usage breakdown
    fn get_memory_usage(&self) -> Result<HashMap<String, u64>> {
        let mut usage = HashMap::new();

        // Block cache usage
        if let Ok(Some(block_cache)) = self.db.property_int_value(properties::BLOCK_CACHE_USAGE) {
            usage.insert("block_cache".to_string(), block_cache);
        }

        // Block cache pinned usage
        if let Ok(Some(pinned)) = self
            .db
            .property_int_value(properties::BLOCK_CACHE_PINNED_USAGE)
        {
            usage.insert("block_cache_pinned".to_string(), pinned);
        }

        // Estimate table readers memory
        if let Ok(Some(table_readers)) = self
            .db
            .property_int_value(properties::ESTIMATE_TABLE_READERS_MEM)
        {
            usage.insert("table_readers".to_string(), table_readers);
        }

        // Current memtable size
        if let Ok(Some(memtable)) = self
            .db
            .property_int_value(properties::CUR_SIZE_ALL_MEM_TABLES)
        {
            usage.insert("memtables".to_string(), memtable);
        }

        Ok(usage)
    }

    /// Check for write stalls
    fn check_write_stalls(&self) -> Result<()> {
        // Check each column family for stalls
        for cf_name in &["executions", "events", "checkpoints", "metadata"] {
            if let Some(cf) = self.db.cf_handle(cf_name) {
                // Check if writes are being stalled
                if let Ok(Some(stall_micros)) = self
                    .db
                    .property_int_value_cf(cf, "rocksdb.cf-write-stall-micros")
                {
                    if stall_micros > 0 {
                        warn!(
                            "{} experienced write stalls: {} microseconds",
                            cf_name, stall_micros
                        );
                        self.metrics.record_lock_contention("rocksdb", cf_name);
                    }
                }

                // Check L0 file count (potential stall cause)
                if let Ok(Some(l0_files)) = self
                    .db
                    .property_int_value_cf(cf, "rocksdb.num-files-at-level0")
                {
                    if l0_files > 10 {
                        warn!(
                            "{} has {} L0 files (potential stall risk)",
                            cf_name, l0_files
                        );
                    }
                }
            }
        }

        Ok(())
    }

    /// Update health status based on current metrics
    fn update_health_status(&self) {
        let mut healthy = true;

        // Check for background errors
        if let Ok(Some(bg_errors)) = self.db.property_int_value(properties::BACKGROUND_ERRORS) {
            if bg_errors > 0 {
                healthy = false;
            }
        }

        // Check if DB is in read-only mode (indicates critical error)
        if let Ok(Some(is_write_stopped)) = self.db.property_int_value("rocksdb.is-write-stopped") {
            if is_write_stopped > 0 {
                error!("RocksDB writes are stopped!");
                healthy = false;
            }
        }

        self.metrics.set_backend_health("rocksdb", healthy);
    }

    /// Get compaction statistics summary
    pub fn get_compaction_summary(&self) -> Result<CompactionSummary> {
        let mut summary = CompactionSummary::default();

        // Collect compaction stats for all column families
        for cf_name in &["executions", "events", "checkpoints", "metadata"] {
            if let Some(cf) = self.db.cf_handle(cf_name) {
                if let Ok(Some(pending)) = self
                    .db
                    .property_int_value_cf(cf, properties::ESTIMATE_PENDING_COMPACTION_BYTES)
                {
                    summary.total_pending_bytes += pending;
                }

                if let Ok(Some(running)) = self
                    .db
                    .property_int_value_cf(cf, "rocksdb.num-running-compactions")
                {
                    summary.running_compactions += running as usize;
                }
            }
        }

        Ok(summary)
    }

    /// Force manual compaction (use sparingly)
    pub fn trigger_manual_compaction(&self, cf_name: &str) -> Result<()> {
        if let Some(cf) = self.db.cf_handle(cf_name) {
            info!("Triggering manual compaction for {}", cf_name);
            self.db.compact_range_cf(cf, None::<&[u8]>, None::<&[u8]>);
            Ok(())
        } else {
            anyhow::bail!("Column family {} not found", cf_name)
        }
    }
}

/// Compaction summary statistics
#[derive(Debug, Default)]
pub struct CompactionSummary {
    pub total_pending_bytes: u64,
    pub running_compactions: usize,
}

/// Performance recommendations based on statistics
pub struct PerformanceRecommendations {
    pub increase_write_buffer: bool,
    pub increase_l0_trigger: bool,
    pub enable_compression: bool,
    pub trigger_manual_compaction: Vec<String>,
}

impl RocksDbMonitor {
    /// Analyze performance and provide recommendations
    pub fn analyze_performance(&self) -> Result<PerformanceRecommendations> {
        let mut recommendations = PerformanceRecommendations {
            increase_write_buffer: false,
            increase_l0_trigger: false,
            enable_compression: false,
            trigger_manual_compaction: Vec::new(),
        };

        // Check each column family
        for cf_name in &["executions", "events", "checkpoints", "metadata"] {
            if let Some(cf) = self.db.cf_handle(cf_name) {
                // Check write stalls
                if let Ok(Some(stalls)) = self
                    .db
                    .property_int_value_cf(cf, "rocksdb.cf-write-stall-micros")
                {
                    if stalls > 1_000_000 {
                        // More than 1 second of stalls
                        recommendations.increase_write_buffer = true;
                    }
                }

                // Check L0 files
                if let Ok(Some(l0_files)) = self
                    .db
                    .property_int_value_cf(cf, "rocksdb.num-files-at-level0")
                {
                    if l0_files > 15 {
                        recommendations.increase_l0_trigger = true;
                    }
                }

                // Check pending compaction
                if let Ok(Some(pending)) = self
                    .db
                    .property_int_value_cf(cf, properties::ESTIMATE_PENDING_COMPACTION_BYTES)
                {
                    if pending > 1024 * 1024 * 1024 {
                        // More than 1GB pending
                        recommendations
                            .trigger_manual_compaction
                            .push(cf_name.to_string());
                    }
                }

                // Check compression effectiveness
                // Note: COMPRESSION_RATIO_AT_LEVEL_PREFIX not available in rocksdb v0.22
                // if let Ok(ratio_str) = self
                //     .db
                //     .property_value_cf(cf, "rocksdb.compression-ratio-at-level")
                // {
                //     // Parse compression ratio and recommend changes if needed
                //     if ratio_str.contains("1.00") && cf_name != "metadata" {
                //         recommendations.enable_compression = true;
                //     }
                // }
            }
        }

        Ok(recommendations)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_db() -> (Arc<DB>, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db = DB::open_default(temp_dir.path()).unwrap();
        (Arc::new(db), temp_dir)
    }

    #[test]
    fn test_monitor_creation() {
        let (db, _temp_dir) = create_test_db();
        let registry = prometheus::Registry::new();
        let metrics = Arc::new(WorkflowStorageMetrics::new(&registry).unwrap());

        let monitor = RocksDbMonitor::new(db, metrics);
        assert_eq!(monitor.collection_interval_secs, 10);
    }

    #[test]
    fn test_collect_statistics() {
        let (db, _temp_dir) = create_test_db();
        let registry = prometheus::Registry::new();
        let metrics = Arc::new(WorkflowStorageMetrics::new(&registry).unwrap());

        let monitor = RocksDbMonitor::new(db, metrics);
        let result = monitor.collect_statistics();
        assert!(result.is_ok());
    }
}
