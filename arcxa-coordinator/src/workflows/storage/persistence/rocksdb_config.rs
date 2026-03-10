//! RocksDB Configuration Module
//!
//! Production-grade configuration for workflow execution storage
//! Optimized for:
//! - 10K+ workflow executions per minute
//! - 100GB+ of execution history
//! - Sub-millisecond read latency
//! - 99.99% durability

use anyhow::{Context, Result};
use rocksdb::{
    BlockBasedOptions, Cache, ColumnFamilyDescriptor, DBCompactionStyle, DBCompressionType,
    Options, SliceTransform,
};
use serde::{Deserialize, Serialize};
use std::env;
use std::path::Path;
use std::sync::Arc;

/// RocksDB configuration for workflow storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RocksDbConfig {
    /// Database-wide settings
    pub db_config: DatabaseConfig,

    /// Column family specific settings
    pub cf_configs: ColumnFamilyConfigs,

    /// Memory management settings
    pub memory_config: MemoryConfig,

    /// Compaction settings
    pub compaction_config: CompactionConfig,

    /// Performance tuning
    pub performance_config: PerformanceConfig,

    /// TTL settings for automatic data expiry
    pub ttl_config: TtlConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    /// Max number of concurrent background jobs
    pub max_background_jobs: i32,

    /// Number of background threads
    pub max_background_compactions: i32,
    pub max_background_flushes: i32,

    /// WAL settings
    pub wal_dir: Option<String>,
    pub wal_ttl_seconds: u64,
    pub wal_size_limit_mb: u64,

    /// Stats dump period
    pub stats_dump_period_sec: u32,

    /// Keep log files for debugging
    pub keep_log_file_num: usize,
    pub max_log_file_size: usize,

    /// Enable statistics collection
    pub enable_statistics: bool,
}

/// Column family configurations optimized for different data patterns
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnFamilyConfigs {
    pub executions: CfConfig,
    pub events: CfConfig,
    pub checkpoints: CfConfig,
    pub metadata: CfConfig,
    pub progress: CfConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CfConfig {
    /// Write buffer size (per column family)
    pub write_buffer_size: usize,

    /// Number of write buffers
    pub max_write_buffer_number: i32,

    /// Minimum buffers to merge
    pub min_write_buffer_number_to_merge: i32,

    /// Compression algorithm
    pub compression: CompressionAlgorithm,

    /// Bottom-most level compression (better ratio)
    pub bottommost_compression: CompressionAlgorithm,

    /// Block size for SST files
    pub block_size: usize,

    /// Bloom filter bits per key
    pub bloom_filter_bits_per_key: i32,

    /// Use prefix extractor for range queries
    pub prefix_extractor: Option<usize>,

    /// Target file size for compaction
    pub target_file_size_base: u64,
    pub target_file_size_multiplier: i32,

    /// Level0 compaction triggers
    pub level0_file_num_compaction_trigger: i32,
    pub level0_slowdown_writes_trigger: i32,
    pub level0_stop_writes_trigger: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CompressionAlgorithm {
    None,
    Snappy,
    Lz4,
    Lz4hc(i32),
    Zlib(i32),
    Zstd(i32),
    BZip2(i32),
}

impl CompressionAlgorithm {
    pub fn to_rocksdb_type(&self) -> DBCompressionType {
        match self {
            Self::None => DBCompressionType::None,
            Self::Snappy => DBCompressionType::Snappy,
            Self::Lz4 => DBCompressionType::Lz4,
            Self::Lz4hc(_) => DBCompressionType::Lz4hc,
            Self::Zlib(_) => DBCompressionType::Zlib,
            Self::Zstd(_) => DBCompressionType::Zstd,
            Self::BZip2(_) => DBCompressionType::Bz2,
        }
    }

    pub fn compression_level(&self) -> Option<i32> {
        match self {
            Self::Lz4hc(level) | Self::Zlib(level) | Self::Zstd(level) | Self::BZip2(level) => {
                Some(*level)
            }
            _ => None,
        }
    }
}

/// Memory configuration for optimal performance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    /// Total memory budget for RocksDB (bytes)
    pub total_memory_budget: usize,

    /// Block cache size (shared across CFs)
    pub block_cache_size: usize,

    /// Write buffer manager budget
    pub write_buffer_budget: usize,

    /// Use direct I/O for reads
    pub use_direct_reads: bool,

    /// Use direct I/O for writes
    pub use_direct_io_for_flush_and_compaction: bool,

    /// Allow mmap reads
    pub allow_mmap_reads: bool,

    /// Cache index and filter blocks
    pub cache_index_and_filter_blocks: bool,

    /// Pin L0 filter and index blocks
    pub pin_l0_filter_and_index_blocks: bool,

    /// High priority pool ratio for index/filter blocks
    pub high_pri_pool_ratio: f64,
}

/// Compaction configuration for optimal performance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionConfig {
    pub style: CompactionStyleConfig,
    pub max_bytes_for_level_base: u64,
    pub max_bytes_for_level_multiplier: f64,
    pub level_compaction_dynamic_level_bytes: bool,
    pub periodic_compaction_seconds: u64,
    pub ttl_seconds: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CompactionStyleConfig {
    Level,
    Universal,
    Fifo,
}

impl CompactionStyleConfig {
    pub fn to_rocksdb_style(&self) -> DBCompactionStyle {
        match self {
            Self::Level => DBCompactionStyle::Level,
            Self::Universal => DBCompactionStyle::Universal,
            Self::Fifo => DBCompactionStyle::Fifo,
        }
    }
}

/// Performance tuning configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    /// Parallelism for flush and compaction
    pub max_subcompactions: u32,

    /// Optimize filters for hits
    pub optimize_filters_for_hits: bool,

    /// Bytes per sync for WAL/flush
    pub bytes_per_sync: u64,
    pub wal_bytes_per_sync: u64,

    /// Rate limiting for compaction (bytes/sec, 0 = unlimited)
    pub rate_limiter_bytes_per_sec: i64,

    /// Readahead size for compaction
    pub compaction_readahead_size: usize,

    /// Number of threads for flush and compaction
    pub max_background_threads: i32,

    /// Unordered write (faster but no ordering guarantees)
    pub unordered_write: bool,
}

/// TTL configuration for automatic data expiry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtlConfig {
    /// TTL for execution records (seconds)
    pub execution_ttl_seconds: Option<u64>,

    /// TTL for event logs (seconds)
    pub event_ttl_seconds: Option<u64>,

    /// TTL for checkpoints (seconds)
    pub checkpoint_ttl_seconds: Option<u64>,

    /// Enable periodic TTL compaction
    pub periodic_compaction_seconds: u64,
}

impl Default for RocksDbConfig {
    fn default() -> Self {
        Self::production()
    }
}

impl RocksDbConfig {
    /// Production configuration optimized for high-throughput workflow storage
    pub fn production() -> Self {
        Self {
            db_config: DatabaseConfig {
                max_background_jobs: 16,
                max_background_compactions: 8,
                max_background_flushes: 4,
                wal_dir: None,
                wal_ttl_seconds: 3600,      // 1 hour
                wal_size_limit_mb: 4096,    // 4GB
                stats_dump_period_sec: 600, // 10 minutes
                keep_log_file_num: 10,
                max_log_file_size: 20 * 1024 * 1024, // 20MB
                enable_statistics: true,
            },
            cf_configs: ColumnFamilyConfigs::production(),
            memory_config: MemoryConfig::production(),
            compaction_config: CompactionConfig::production(),
            performance_config: PerformanceConfig::production(),
            ttl_config: TtlConfig::production(),
        }
    }

    /// Development configuration with lower resource usage
    pub fn development() -> Self {
        let mut config = Self::production();

        // Reduce memory usage
        config.memory_config.total_memory_budget = 1024 * 1024 * 1024; // 1GB
        config.memory_config.block_cache_size = 256 * 1024 * 1024; // 256MB
        config.memory_config.write_buffer_budget = 128 * 1024 * 1024; // 128MB

        // Reduce parallelism
        config.db_config.max_background_jobs = 4;
        config.db_config.max_background_compactions = 2;
        config.db_config.max_background_flushes = 2;

        // Smaller write buffers
        config.cf_configs.executions.write_buffer_size = 32 * 1024 * 1024; // 32MB
        config.cf_configs.events.write_buffer_size = 16 * 1024 * 1024; // 16MB

        config
    }

    /// Load configuration from environment variables
    pub fn from_env() -> Result<Self> {
        let mut config = Self::production();

        // Override with env vars
        if let Ok(val) = env::var("ROCKSDB_MAX_BACKGROUND_JOBS") {
            config.db_config.max_background_jobs =
                val.parse().context("Invalid ROCKSDB_MAX_BACKGROUND_JOBS")?;
        }

        if let Ok(val) = env::var("ROCKSDB_BLOCK_CACHE_SIZE_MB") {
            config.memory_config.block_cache_size = val
                .parse::<usize>()
                .context("Invalid ROCKSDB_BLOCK_CACHE_SIZE_MB")?
                * 1024
                * 1024;
        }

        if let Ok(val) = env::var("ROCKSDB_WRITE_BUFFER_SIZE_MB") {
            config.memory_config.write_buffer_budget = val
                .parse::<usize>()
                .context("Invalid ROCKSDB_WRITE_BUFFER_SIZE_MB")?
                * 1024
                * 1024;
        }

        if let Ok(val) = env::var("ROCKSDB_EXECUTION_TTL_DAYS") {
            config.ttl_config.execution_ttl_seconds = Some(
                val.parse::<u64>()
                    .context("Invalid ROCKSDB_EXECUTION_TTL_DAYS")?
                    * 24
                    * 3600,
            );
        }

        if let Ok(val) = env::var("ROCKSDB_COMPRESSION") {
            let compression = match val.as_str() {
                "none" => CompressionAlgorithm::None,
                "snappy" => CompressionAlgorithm::Snappy,
                "lz4" => CompressionAlgorithm::Lz4,
                "zstd" => CompressionAlgorithm::Zstd(3),
                _ => {
                    tracing::warn!("Unknown compression type: {}, using LZ4", val);
                    CompressionAlgorithm::Lz4
                }
            };
            config.cf_configs.executions.compression = compression.clone();
            config.cf_configs.events.compression = compression;
        }

        Ok(config)
    }

    /// Load from YAML configuration file
    pub fn from_file(path: &Path) -> Result<Self> {
        let contents = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {:?}", path))?;
        let config: Self = serde_yaml::from_str(&contents)
            .with_context(|| format!("Failed to parse config file: {:?}", path))?;
        Ok(config)
    }

    /// Build Options for database opening
    pub fn build_db_options(&self) -> Options {
        let mut opts = Options::default();

        // Basic settings
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);

        // Parallelism
        opts.set_max_background_jobs(self.db_config.max_background_jobs);
        opts.increase_parallelism(num_cpus::get() as i32);

        // WAL settings
        if let Some(ref wal_dir) = self.db_config.wal_dir {
            opts.set_wal_dir(wal_dir);
        }
        opts.set_wal_ttl_seconds(self.db_config.wal_ttl_seconds);
        opts.set_wal_size_limit_mb(self.db_config.wal_size_limit_mb);

        // Statistics
        if self.db_config.enable_statistics {
            opts.enable_statistics();
            opts.set_stats_dump_period_sec(self.db_config.stats_dump_period_sec);
        }

        // Performance settings
        opts.set_bytes_per_sync(self.performance_config.bytes_per_sync);
        opts.set_wal_bytes_per_sync(self.performance_config.wal_bytes_per_sync);

        // Note: enable_pipelined_write not available in rocksdb v0.22

        if self.performance_config.unordered_write {
            opts.set_unordered_write(true);
        }

        // Direct I/O settings
        opts.set_use_direct_reads(self.memory_config.use_direct_reads);
        opts.set_use_direct_io_for_flush_and_compaction(
            self.memory_config.use_direct_io_for_flush_and_compaction,
        );

        opts
    }

    /// Build column family descriptors
    pub fn build_column_families(&self) -> Vec<ColumnFamilyDescriptor> {
        vec![
            self.build_cf("executions", &self.cf_configs.executions),
            self.build_cf("events", &self.cf_configs.events),
            self.build_cf("checkpoints", &self.cf_configs.checkpoints),
            self.build_cf("metadata", &self.cf_configs.metadata),
            self.build_cf("progress", &self.cf_configs.progress),
        ]
    }

    fn build_cf(&self, name: &str, config: &CfConfig) -> ColumnFamilyDescriptor {
        let mut opts = Options::default();

        // Write buffer settings
        opts.set_write_buffer_size(config.write_buffer_size);
        opts.set_max_write_buffer_number(config.max_write_buffer_number);
        opts.set_min_write_buffer_number_to_merge(config.min_write_buffer_number_to_merge);

        // Compression
        opts.set_compression_type(config.compression.to_rocksdb_type());
        if let Some(level) = config.compression.compression_level() {
            opts.set_compression_options(level, level, 0, 0);
        }

        opts.set_bottommost_compression_type(config.bottommost_compression.to_rocksdb_type());
        if let Some(level) = config.bottommost_compression.compression_level() {
            opts.set_bottommost_compression_options(level, level, 0, 0, true);
        }

        // Compaction settings
        opts.set_compaction_style(self.compaction_config.style.to_rocksdb_style());
        opts.set_level_compaction_dynamic_level_bytes(
            self.compaction_config.level_compaction_dynamic_level_bytes,
        );
        opts.set_max_bytes_for_level_base(self.compaction_config.max_bytes_for_level_base);
        opts.set_max_bytes_for_level_multiplier(
            self.compaction_config.max_bytes_for_level_multiplier,
        );

        // Level0 triggers
        opts.set_level_zero_file_num_compaction_trigger(config.level0_file_num_compaction_trigger);
        opts.set_level_zero_slowdown_writes_trigger(config.level0_slowdown_writes_trigger);
        opts.set_level_zero_stop_writes_trigger(config.level0_stop_writes_trigger);

        // Target file sizes
        opts.set_target_file_size_base(config.target_file_size_base);
        opts.set_target_file_size_multiplier(config.target_file_size_multiplier);

        // Periodic compaction for TTL
        if self.compaction_config.periodic_compaction_seconds > 0 {
            opts.set_periodic_compaction_seconds(
                self.compaction_config.periodic_compaction_seconds,
            );
        }

        // Block-based table options
        let mut block_opts = BlockBasedOptions::default();
        block_opts.set_block_size(config.block_size);

        // Bloom filter
        if config.bloom_filter_bits_per_key > 0 {
            block_opts.set_bloom_filter(config.bloom_filter_bits_per_key as f64, false);
        }

        // Cache configuration
        let cache = Arc::new(Cache::new_lru_cache(self.memory_config.block_cache_size));
        block_opts.set_block_cache(&cache);

        if self.memory_config.cache_index_and_filter_blocks {
            block_opts.set_cache_index_and_filter_blocks(true);
            block_opts.set_pin_l0_filter_and_index_blocks_in_cache(
                self.memory_config.pin_l0_filter_and_index_blocks,
            );
        }

        // Data block index type (binary search vs hash)
        block_opts.set_data_block_index_type(rocksdb::DataBlockIndexType::BinaryAndHash);

        opts.set_block_based_table_factory(&block_opts);

        // Prefix extractor for efficient range queries
        if let Some(prefix_len) = config.prefix_extractor {
            opts.set_prefix_extractor(SliceTransform::create_fixed_prefix(prefix_len));
            opts.set_memtable_prefix_bloom_ratio(0.2);
        }

        // Optimize filters for point lookups
        if self.performance_config.optimize_filters_for_hits {
            opts.set_optimize_filters_for_hits(true);
        }

        ColumnFamilyDescriptor::new(name, opts)
    }

    /// Create block cache
    pub fn create_block_cache(&self) -> Arc<Cache> {
        let cache = Cache::new_lru_cache(self.memory_config.block_cache_size);

        // Set high priority pool for index/filter blocks
        if self.memory_config.high_pri_pool_ratio > 0.0 {
            // Note: This would require the cache to support high priority pool
            // which is available in newer RocksDB versions
        }

        Arc::new(cache)
    }

    /// Create write buffer manager
    /// Note: WriteBufferManager is not available in rocksdb v0.22
    /// Memory management is handled through Options configuration instead
    #[allow(dead_code)]
    fn write_buffer_note(&self) {
        // Placeholder for write buffer manager functionality
        // In v0.22, we configure write buffers directly through Options
    }
}

impl ColumnFamilyConfigs {
    pub fn production() -> Self {
        Self {
            // Executions: Large JSON documents, frequent reads/writes
            executions: CfConfig {
                write_buffer_size: 128 * 1024 * 1024, // 128MB
                max_write_buffer_number: 4,
                min_write_buffer_number_to_merge: 2,
                compression: CompressionAlgorithm::Lz4,
                bottommost_compression: CompressionAlgorithm::Zstd(3),
                block_size: 32 * 1024, // 32KB blocks
                bloom_filter_bits_per_key: 10,
                prefix_extractor: Some(16), // First 16 bytes of key
                target_file_size_base: 256 * 1024 * 1024, // 256MB
                target_file_size_multiplier: 2,
                level0_file_num_compaction_trigger: 4,
                level0_slowdown_writes_trigger: 20,
                level0_stop_writes_trigger: 36,
            },

            // Events: Small, append-only, time-series data
            events: CfConfig {
                write_buffer_size: 64 * 1024 * 1024, // 64MB
                max_write_buffer_number: 6,
                min_write_buffer_number_to_merge: 3,
                compression: CompressionAlgorithm::Snappy,
                bottommost_compression: CompressionAlgorithm::Zstd(6),
                block_size: 16 * 1024, // 16KB blocks
                bloom_filter_bits_per_key: 10,
                prefix_extractor: Some(24), // execution_id:timestamp prefix
                target_file_size_base: 128 * 1024 * 1024, // 128MB
                target_file_size_multiplier: 2,
                level0_file_num_compaction_trigger: 8,
                level0_slowdown_writes_trigger: 32,
                level0_stop_writes_trigger: 64,
            },

            // Checkpoints: Infrequent writes, large documents
            checkpoints: CfConfig {
                write_buffer_size: 32 * 1024 * 1024, // 32MB
                max_write_buffer_number: 2,
                min_write_buffer_number_to_merge: 1,
                compression: CompressionAlgorithm::Zstd(6),
                bottommost_compression: CompressionAlgorithm::Zstd(9),
                block_size: 64 * 1024, // 64KB blocks
                bloom_filter_bits_per_key: 10,
                prefix_extractor: None,
                target_file_size_base: 512 * 1024 * 1024, // 512MB
                target_file_size_multiplier: 1,
                level0_file_num_compaction_trigger: 2,
                level0_slowdown_writes_trigger: 8,
                level0_stop_writes_trigger: 12,
            },

            // Metadata: Small, frequently accessed
            metadata: CfConfig {
                write_buffer_size: 16 * 1024 * 1024, // 16MB
                max_write_buffer_number: 3,
                min_write_buffer_number_to_merge: 1,
                compression: CompressionAlgorithm::None, // No compression for speed
                bottommost_compression: CompressionAlgorithm::Lz4,
                block_size: 8 * 1024, // 8KB blocks
                bloom_filter_bits_per_key: 10,
                prefix_extractor: None,
                target_file_size_base: 64 * 1024 * 1024, // 64MB
                target_file_size_multiplier: 1,
                level0_file_num_compaction_trigger: 4,
                level0_slowdown_writes_trigger: 12,
                level0_stop_writes_trigger: 20,
            },

            // Progress: Frequent updates, moderate size JSON documents
            progress: CfConfig {
                write_buffer_size: 64 * 1024 * 1024, // 64MB
                max_write_buffer_number: 4,
                min_write_buffer_number_to_merge: 2,
                compression: CompressionAlgorithm::Lz4, // Fast compression for real-time updates
                bottommost_compression: CompressionAlgorithm::Zstd(3),
                block_size: 16 * 1024, // 16KB blocks
                bloom_filter_bits_per_key: 10,
                prefix_extractor: Some(16), // execution_id prefix
                target_file_size_base: 128 * 1024 * 1024, // 128MB
                target_file_size_multiplier: 2,
                level0_file_num_compaction_trigger: 6,
                level0_slowdown_writes_trigger: 24,
                level0_stop_writes_trigger: 48,
            },
        }
    }
}

impl MemoryConfig {
    pub fn production() -> Self {
        Self {
            total_memory_budget: 8 * 1024 * 1024 * 1024, // 8GB
            block_cache_size: 4 * 1024 * 1024 * 1024,    // 4GB
            write_buffer_budget: 2 * 1024 * 1024 * 1024, // 2GB
            use_direct_reads: false,                     // Better with page cache
            use_direct_io_for_flush_and_compaction: false,
            allow_mmap_reads: false, // More predictable performance
            cache_index_and_filter_blocks: true,
            pin_l0_filter_and_index_blocks: true,
            high_pri_pool_ratio: 0.5, // 50% for index/filter blocks
        }
    }
}

impl CompactionConfig {
    pub fn production() -> Self {
        Self {
            style: CompactionStyleConfig::Level,
            max_bytes_for_level_base: 512 * 1024 * 1024, // 512MB
            max_bytes_for_level_multiplier: 10.0,
            level_compaction_dynamic_level_bytes: true,
            periodic_compaction_seconds: 30 * 24 * 3600, // 30 days
            ttl_seconds: None,
        }
    }
}

impl PerformanceConfig {
    pub fn production() -> Self {
        Self {
            max_subcompactions: 4,
            optimize_filters_for_hits: true,
            bytes_per_sync: 1024 * 1024,                // 1MB
            wal_bytes_per_sync: 1024 * 1024,            // 1MB
            rate_limiter_bytes_per_sec: 0,              // Unlimited
            compaction_readahead_size: 2 * 1024 * 1024, // 2MB
            max_background_threads: num_cpus::get() as i32,
            unordered_write: false, // Keep ordering for consistency
        }
    }
}

impl TtlConfig {
    pub fn production() -> Self {
        Self {
            execution_ttl_seconds: Some(90 * 24 * 3600), // 90 days
            event_ttl_seconds: Some(30 * 24 * 3600),     // 30 days
            checkpoint_ttl_seconds: Some(7 * 24 * 3600), // 7 days
            periodic_compaction_seconds: 24 * 3600,      // Daily
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_production_config() {
        let config = RocksDbConfig::production();
        assert_eq!(config.db_config.max_background_jobs, 16);
        assert_eq!(
            config.memory_config.block_cache_size,
            4 * 1024 * 1024 * 1024
        );
    }

    #[test]
    fn test_development_config() {
        let config = RocksDbConfig::development();
        assert_eq!(config.memory_config.total_memory_budget, 1024 * 1024 * 1024);
        assert_eq!(config.db_config.max_background_jobs, 4);
    }

    #[test]
    fn test_compression_algorithm() {
        assert_eq!(
            CompressionAlgorithm::Lz4.to_rocksdb_type(),
            DBCompressionType::Lz4
        );
        assert_eq!(CompressionAlgorithm::Zstd(3).compression_level(), Some(3));
    }

    #[test]
    fn test_build_options() {
        let config = RocksDbConfig::production();
        let _opts = config.build_db_options();
        // Options built successfully
    }

    #[test]
    fn test_build_column_families() {
        let config = RocksDbConfig::production();
        let cfs = config.build_column_families();
        assert_eq!(cfs.len(), 5); // executions, events, checkpoints, metadata, progress
    }
}
