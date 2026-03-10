//! RocksDB Statistics Collector
//!
//! Detailed statistics collection and analysis for performance optimization
//!
//! Note: This module is designed for RocksDB v0.22 which has limited statistics API.
//! For more detailed statistics, upgrade to a newer RocksDB version with full Statistics support.

use anyhow::{Context, Result};
use rocksdb::DB;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Statistics collector for RocksDB
pub struct StatsCollector {
    db: Arc<DB>,
}

/// Collected statistics snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatsSnapshot {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub block_cache_stats: BlockCacheStats,
    pub compaction_stats: CompactionStats,
    pub write_stats: WriteStats,
    pub read_stats: ReadStats,
    pub stall_stats: StallStats,
    pub cf_stats: HashMap<String, ColumnFamilyStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockCacheStats {
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub cache_hit_rate: f64,
    pub index_hits: u64,
    pub index_misses: u64,
    pub filter_hits: u64,
    pub filter_misses: u64,
    pub data_hits: u64,
    pub data_misses: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionStats {
    pub total_compactions: u64,
    pub total_compaction_time_micros: u64,
    pub compaction_bytes_read: u64,
    pub compaction_bytes_written: u64,
    pub write_amplification: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteStats {
    pub total_writes: u64,
    pub total_bytes_written: u64,
    pub wal_writes: u64,
    pub memtable_hits: u64,
    pub memtable_misses: u64,
    pub write_stall_micros: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadStats {
    pub total_reads: u64,
    pub total_bytes_read: u64,
    pub multiget_bytes_read: u64,
    pub iterator_bytes_read: u64,
    pub block_cache_bytes_read: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StallStats {
    pub write_stall_count: u64,
    pub write_stall_duration_micros: u64,
    pub l0_slowdown_count: u64,
    pub memtable_compaction_count: u64,
    pub l0_num_files_stall_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnFamilyStats {
    pub num_immutable_mem_tables: u64,
    pub mem_table_flush_pending: u64,
    pub compaction_pending: u64,
    pub num_running_compactions: u64,
    pub num_running_flushes: u64,
    pub estimate_pending_compaction_bytes: u64,
    pub num_entries_active_mem_table: u64,
    pub num_entries_imm_mem_tables: u64,
    pub estimate_num_keys: u64,
    pub estimate_table_readers_mem: u64,
    pub live_sst_files_size: u64,
}

impl StatsCollector {
    /// Create a new stats collector
    ///
    /// Note: RocksDB v0.22 has limited statistics API. This collector uses property_value()
    /// to read available statistics. For more detailed stats, upgrade to newer RocksDB.
    pub fn new(db: Arc<DB>) -> Self {
        warn!("StatsCollector running in v0.22 compatibility mode with limited statistics");
        Self { db }
    }

    /// Collect current statistics snapshot
    pub fn collect(&self) -> Result<StatsSnapshot> {
        debug!("Collecting RocksDB statistics (v0.22 limited mode)");

        Ok(StatsSnapshot {
            timestamp: chrono::Utc::now(),
            block_cache_stats: self.collect_block_cache_stats()?,
            compaction_stats: self.collect_compaction_stats()?,
            write_stats: self.collect_write_stats()?,
            read_stats: self.collect_read_stats()?,
            stall_stats: self.collect_stall_stats()?,
            cf_stats: self.collect_cf_stats()?,
        })
    }

    /// Collect block cache statistics
    ///
    /// Note: v0.22 doesn't expose detailed cache hit/miss counters via Statistics API.
    /// Returns default values as placeholders.
    fn collect_block_cache_stats(&self) -> Result<BlockCacheStats> {
        // In v0.22, we don't have access to detailed cache statistics
        // These would be available in newer versions via Statistics::get_ticker_count()
        Ok(BlockCacheStats {
            cache_hits: 0,
            cache_misses: 0,
            cache_hit_rate: 0.0,
            index_hits: 0,
            index_misses: 0,
            filter_hits: 0,
            filter_misses: 0,
            data_hits: 0,
            data_misses: 0,
        })
    }

    /// Collect compaction statistics
    fn collect_compaction_stats(&self) -> Result<CompactionStats> {
        // Basic compaction stats available via property_value
        let bytes_written = self
            .get_int_property("rocksdb.total-sst-files-size")
            .unwrap_or(0);

        Ok(CompactionStats {
            total_compactions: 0, // Not available in v0.22
            total_compaction_time_micros: 0,
            compaction_bytes_read: 0,
            compaction_bytes_written: bytes_written,
            write_amplification: 0.0,
        })
    }

    /// Collect write statistics
    fn collect_write_stats(&self) -> Result<WriteStats> {
        Ok(WriteStats {
            total_writes: 0, // Not available in v0.22 without Statistics
            total_bytes_written: self
                .get_int_property("rocksdb.total-sst-files-size")
                .unwrap_or(0),
            wal_writes: 0,
            memtable_hits: 0,
            memtable_misses: 0,
            write_stall_micros: 0,
        })
    }

    /// Collect read statistics
    fn collect_read_stats(&self) -> Result<ReadStats> {
        Ok(ReadStats {
            total_reads: 0, // Not available in v0.22 without Statistics
            total_bytes_read: 0,
            multiget_bytes_read: 0,
            iterator_bytes_read: 0,
            block_cache_bytes_read: 0,
        })
    }

    /// Collect write stall statistics
    fn collect_stall_stats(&self) -> Result<StallStats> {
        Ok(StallStats {
            write_stall_count: 0,
            write_stall_duration_micros: 0,
            l0_slowdown_count: 0,
            memtable_compaction_count: 0,
            l0_num_files_stall_count: 0,
        })
    }

    /// Collect per-column-family statistics
    fn collect_cf_stats(&self) -> Result<HashMap<String, ColumnFamilyStats>> {
        let mut cf_stats = HashMap::new();

        // Get stats for default CF (v0.22 has limited CF introspection)
        let stats = ColumnFamilyStats {
            num_immutable_mem_tables: self
                .get_cf_int_property("rocksdb.num-immutable-mem-table")
                .unwrap_or(0),
            mem_table_flush_pending: self
                .get_cf_int_property("rocksdb.mem-table-flush-pending")
                .unwrap_or(0),
            compaction_pending: self
                .get_cf_int_property("rocksdb.compaction-pending")
                .unwrap_or(0),
            num_running_compactions: self
                .get_cf_int_property("rocksdb.num-running-compactions")
                .unwrap_or(0),
            num_running_flushes: self
                .get_cf_int_property("rocksdb.num-running-flushes")
                .unwrap_or(0),
            estimate_pending_compaction_bytes: self
                .get_cf_int_property("rocksdb.estimate-pending-compaction-bytes")
                .unwrap_or(0),
            num_entries_active_mem_table: self
                .get_cf_int_property("rocksdb.num-entries-active-mem-table")
                .unwrap_or(0),
            num_entries_imm_mem_tables: self
                .get_cf_int_property("rocksdb.num-entries-imm-mem-tables")
                .unwrap_or(0),
            estimate_num_keys: self
                .get_cf_int_property("rocksdb.estimate-num-keys")
                .unwrap_or(0),
            estimate_table_readers_mem: self
                .get_cf_int_property("rocksdb.estimate-table-readers-mem")
                .unwrap_or(0),
            live_sst_files_size: self
                .get_cf_int_property("rocksdb.live-sst-files-size")
                .unwrap_or(0),
        };

        cf_stats.insert("default".to_string(), stats);
        Ok(cf_stats)
    }

    /// Helper to get integer property
    fn get_int_property(&self, property: &str) -> Option<u64> {
        self.db
            .property_value(property)
            .ok()
            .flatten()
            .and_then(|v| v.parse::<u64>().ok())
    }

    /// Helper to get CF integer property
    fn get_cf_int_property(&self, property: &str) -> Option<u64> {
        // In v0.22, we just use the default CF
        self.get_int_property(property)
    }

    /// Get human-readable summary
    pub fn get_summary(&self) -> Result<String> {
        let snapshot = self.collect()?;

        let summary = format!(
            "RocksDB Statistics Summary (v0.22 limited mode)\n\
             Timestamp: {}\n\
             \n\
             Compaction:\n\
               Bytes Written: {} MB\n\
             \n\
             Column Family (default):\n\
               Immutable Memtables: {}\n\
               Flush Pending: {}\n\
               Compaction Pending: {}\n\
               Running Compactions: {}\n\
               Running Flushes: {}\n\
               Estimated Keys: {}\n\
               Live SST Size: {} MB\n\
             \n\
             Note: Detailed statistics (cache hits, ticker counts) require RocksDB > v0.22",
            snapshot.timestamp,
            snapshot.compaction_stats.compaction_bytes_written / 1024 / 1024,
            snapshot
                .cf_stats
                .get("default")
                .map(|s| s.num_immutable_mem_tables)
                .unwrap_or(0),
            snapshot
                .cf_stats
                .get("default")
                .map(|s| s.mem_table_flush_pending)
                .unwrap_or(0),
            snapshot
                .cf_stats
                .get("default")
                .map(|s| s.compaction_pending)
                .unwrap_or(0),
            snapshot
                .cf_stats
                .get("default")
                .map(|s| s.num_running_compactions)
                .unwrap_or(0),
            snapshot
                .cf_stats
                .get("default")
                .map(|s| s.num_running_flushes)
                .unwrap_or(0),
            snapshot
                .cf_stats
                .get("default")
                .map(|s| s.estimate_num_keys)
                .unwrap_or(0),
            snapshot
                .cf_stats
                .get("default")
                .map(|s| s.live_sst_files_size)
                .unwrap_or(0)
                / 1024
                / 1024,
        );

        Ok(summary)
    }

    /// Print statistics to console
    pub fn print_stats(&self) -> Result<()> {
        info!("\n{}", self.get_summary()?);
        Ok(())
    }

    /// Calculate and log performance recommendations
    pub fn analyze_performance(&self) -> Result<Vec<String>> {
        let snapshot = self.collect()?;
        let mut recommendations = Vec::new();

        // Check for pending compactions
        if let Some(cf) = snapshot.cf_stats.get("default") {
            if cf.compaction_pending > 0 {
                recommendations.push(format!(
                    "Compaction pending detected. Consider increasing max_background_compactions"
                ));
            }

            if cf.mem_table_flush_pending > 0 {
                recommendations.push(format!(
                    "Memtable flush pending. Consider increasing max_write_buffer_number"
                ));
            }

            if cf.num_immutable_mem_tables > 2 {
                recommendations.push(format!(
                    "Multiple immutable memtables ({}). May indicate write pressure",
                    cf.num_immutable_mem_tables
                ));
            }
        }

        if recommendations.is_empty() {
            recommendations
                .push("No performance issues detected in v0.22 limited monitoring".to_string());
        }

        for rec in &recommendations {
            info!("Performance recommendation: {}", rec);
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
    fn test_stats_collector_creation() {
        let (db, _temp_dir) = create_test_db();
        let collector = StatsCollector::new(db);
        assert!(collector.collect().is_ok());
    }

    #[test]
    fn test_stats_snapshot() {
        let (db, _temp_dir) = create_test_db();
        let collector = StatsCollector::new(db);
        let snapshot = collector.collect().unwrap();

        // Basic sanity checks
        assert_eq!(snapshot.cf_stats.len(), 1);
        assert!(snapshot.cf_stats.contains_key("default"));
    }

    #[test]
    fn test_summary_generation() {
        let (db, _temp_dir) = create_test_db();
        let collector = StatsCollector::new(db);
        let summary = collector.get_summary().unwrap();

        assert!(summary.contains("RocksDB Statistics Summary"));
        assert!(summary.contains("v0.22 limited mode"));
    }

    #[test]
    fn test_performance_analysis() {
        let (db, _temp_dir) = create_test_db();
        let collector = StatsCollector::new(db);
        let recommendations = collector.analyze_performance().unwrap();

        assert!(!recommendations.is_empty());
    }
}
