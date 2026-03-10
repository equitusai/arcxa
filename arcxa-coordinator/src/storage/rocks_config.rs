//! # RocksDB Performance Configuration
//!
//! Optimized RocksDB configurations for high-throughput lineage storage.
//!
//! Provides tuned settings for different deployment scenarios:
//! - Development: Fast startup, low resource usage
//! - Production: Balanced performance and reliability
//! - HighThroughput: Maximum write throughput (target: 10K events/sec)

use rocksdb::{
    BlockBasedOptions, Cache, DBCompressionType, Options, SliceTransform, UniversalCompactOptions,
};

/// RocksDB configuration profile
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RocksProfile {
    /// Fast startup, minimal resource usage (for development/testing)
    Development,
    /// Balanced performance and reliability (default production)
    Production,
    /// Maximum write throughput (target: 10,000 events/sec)
    HighThroughput,
}

impl Default for RocksProfile {
    fn default() -> Self {
        RocksProfile::Production
    }
}

/// Create optimized RocksDB options based on profile
pub fn create_options(profile: RocksProfile) -> Options {
    match profile {
        RocksProfile::Development => create_development_options(),
        RocksProfile::Production => create_production_options(),
        RocksProfile::HighThroughput => create_high_throughput_options(),
    }
}

/// Development profile - Fast startup, low resource usage
fn create_development_options() -> Options {
    let mut opts = Options::default();
    opts.create_if_missing(true);
    opts.create_missing_column_families(true);

    // Basic settings
    opts.set_compression_type(DBCompressionType::Lz4);
    opts.set_max_open_files(100);
    opts.set_max_background_jobs(2);

    // Minimal memory usage
    opts.set_write_buffer_size(16 * 1024 * 1024); // 16 MB
    opts.set_max_write_buffer_number(2);

    // Block cache
    let cache = Cache::new_lru_cache(64 * 1024 * 1024); // 64 MB
    let mut block_opts = BlockBasedOptions::default();
    block_opts.set_block_cache(&cache);
    block_opts.set_block_size(4 * 1024); // 4 KB
    opts.set_block_based_table_factory(&block_opts);

    opts
}

/// Production profile - Balanced performance and reliability
fn create_production_options() -> Options {
    let mut opts = Options::default();
    opts.create_if_missing(true);
    opts.create_missing_column_families(true);

    // ===== WRITE PATH OPTIMIZATION =====

    // Write buffer (memtable) settings
    // Larger write buffers = fewer flushes = better write throughput
    opts.set_write_buffer_size(128 * 1024 * 1024); // 128 MB per memtable
    opts.set_max_write_buffer_number(4); // Allow 4 memtables (512 MB total)
    opts.set_min_write_buffer_number_to_merge(2); // Merge 2 before flushing

    // Allow writes to continue even when memtables are being flushed
    opts.set_max_write_buffer_number(6); // Total including those being flushed

    // ===== COMPACTION OPTIMIZATION =====

    // Background jobs (compaction + flushing)
    opts.set_max_background_jobs(8); // 8 concurrent background tasks
    opts.set_max_background_compactions(6);
    opts.set_max_background_flushes(2);

    // Level-based compaction with dynamic leveling
    opts.set_level_compaction_dynamic_level_bytes(true);

    // Target file sizes
    opts.set_target_file_size_base(128 * 1024 * 1024); // 128 MB
    opts.set_target_file_size_multiplier(2); // L0: 128MB, L1: 256MB, L2: 512MB, etc.

    // Level sizes
    opts.set_max_bytes_for_level_base(512 * 1024 * 1024); // 512 MB for L1
    opts.set_max_bytes_for_level_multiplier(10.0); // Each level 10× larger

    // Parallelism for compactions
    opts.set_max_subcompactions(4); // Parallelize single compactions

    // ===== READ PATH OPTIMIZATION =====

    // Block cache (hot data cache in memory)
    let cache = Cache::new_lru_cache(512 * 1024 * 1024); // 512 MB cache
    let mut block_opts = BlockBasedOptions::default();
    block_opts.set_block_cache(&cache);
    block_opts.set_block_size(16 * 1024); // 16 KB blocks

    // Bloom filter for faster point lookups
    block_opts.set_bloom_filter(10.0, false); // 10 bits per key, ~1% false positive rate

    // Index and filter blocks in cache
    block_opts.set_cache_index_and_filter_blocks(true);
    block_opts.set_pin_l0_filter_and_index_blocks_in_cache(true);

    opts.set_block_based_table_factory(&block_opts);

    // ===== COMPRESSION =====

    // Use Lz4 for speed (L0-L2), Zstd for better compression (L3+)
    opts.set_compression_type(DBCompressionType::Lz4);
    opts.set_compression_per_level(&[
        DBCompressionType::None, // L0: No compression (fresh writes)
        DBCompressionType::Lz4,  // L1: Fast compression
        DBCompressionType::Lz4,  // L2: Fast compression
        DBCompressionType::Zstd, // L3+: Better compression
    ]);

    // Zstd compression level (1 = fastest, 22 = best compression)
    opts.set_zstd_max_train_bytes(0); // Disable dictionary training for speed

    // ===== FILE MANAGEMENT =====

    opts.set_max_open_files(1000); // Keep up to 1000 SST files open

    // Delete obsolete files immediately
    opts.set_delete_obsolete_files_period_micros(60 * 1000000); // 60 seconds

    // ===== WRITE AHEAD LOG (WAL) =====

    opts.set_wal_bytes_per_sync(1024 * 1024); // Sync WAL every 1 MB
    opts.set_bytes_per_sync(1024 * 1024); // Sync data files every 1 MB

    // ===== STATISTICS & MONITORING =====

    opts.enable_statistics();
    opts.set_stats_dump_period_sec(300); // Dump stats every 5 minutes

    opts
}

/// High-throughput profile - Maximum write throughput
///
/// Optimized for 10,000+ events/sec sustained throughput
fn create_high_throughput_options() -> Options {
    let mut opts = Options::default();
    opts.create_if_missing(true);
    opts.create_missing_column_families(true);

    // ===== AGGRESSIVE WRITE OPTIMIZATION =====

    // HUGE write buffers for batching
    opts.set_write_buffer_size(256 * 1024 * 1024); // 256 MB per memtable
    opts.set_max_write_buffer_number(8); // 2 GB total buffering
    opts.set_min_write_buffer_number_to_merge(2);

    // Allow many pending memtables
    opts.set_max_write_buffer_number(12); // Including flushing

    // Disable WAL for maximum speed (CAREFUL: reduces durability)
    // Comment this out if you need durability guarantees
    // opts.set_manual_wal_flush(true);

    // ===== MASSIVE PARALLELISM =====

    // Maximum background jobs
    opts.set_max_background_jobs(16); // 16 concurrent background tasks
    opts.set_max_background_compactions(12);
    opts.set_max_background_flushes(4);

    // Parallel compactions
    opts.set_max_subcompactions(8); // Split compactions across 8 threads

    // ===== COMPACTION TUNING FOR WRITES =====

    // Universal compaction (better for write-heavy workloads)
    opts.set_compaction_style(rocksdb::DBCompactionStyle::Universal);

    let mut universal_opts = UniversalCompactOptions::default();
    universal_opts.set_size_ratio(1); // Compact when next level is same size
    universal_opts.set_min_merge_width(2);
    universal_opts.set_max_merge_width(10);
    universal_opts.set_max_size_amplification_percent(200); // Allow 2× amplification
    opts.set_universal_compaction_options(&universal_opts);

    // Larger SST files
    opts.set_target_file_size_base(256 * 1024 * 1024); // 256 MB
    opts.set_max_bytes_for_level_base(1024 * 1024 * 1024); // 1 GB

    // ===== READ OPTIMIZATION (SECONDARY) =====

    // Large block cache
    let cache = Cache::new_lru_cache(1024 * 1024 * 1024); // 1 GB cache
    let mut block_opts = BlockBasedOptions::default();
    block_opts.set_block_cache(&cache);
    block_opts.set_block_size(32 * 1024); // 32 KB blocks (larger for sequential reads)

    // Bloom filter
    block_opts.set_bloom_filter(10.0, true); // Full filters for all keys

    // Cache everything possible
    block_opts.set_cache_index_and_filter_blocks(true);
    block_opts.set_pin_l0_filter_and_index_blocks_in_cache(true);
    block_opts.set_pin_top_level_index_and_filter(true);

    opts.set_block_based_table_factory(&block_opts);

    // ===== COMPRESSION - OPTIMIZE FOR SPEED =====

    // Use only Lz4 (fast) or None
    opts.set_compression_type(DBCompressionType::Lz4);
    opts.set_compression_per_level(&[
        DBCompressionType::None, // L0: No compression
        DBCompressionType::None, // L1: No compression
        DBCompressionType::Lz4,  // L2+: Fast compression
        DBCompressionType::Lz4,
    ]);

    // ===== FILE MANAGEMENT =====

    opts.set_max_open_files(-1); // Keep all files open (requires ulimit tuning)

    // Aggressive deletion of obsolete files
    opts.set_delete_obsolete_files_period_micros(10 * 1000000); // 10 seconds

    // ===== WAL SETTINGS =====

    opts.set_wal_bytes_per_sync(4 * 1024 * 1024); // Sync every 4 MB
    opts.set_bytes_per_sync(4 * 1024 * 1024);

    // Large WAL size before rotating
    opts.set_max_total_wal_size(512 * 1024 * 1024); // 512 MB

    // ===== RATE LIMITING (OPTIONAL) =====

    // Uncomment to limit compaction I/O (prevents compaction from overwhelming disk)
    // opts.set_ratelimiter(RateLimiter::new(100 * 1024 * 1024, 10 * 1000, 10)); // 100 MB/s

    // ===== MONITORING =====

    opts.enable_statistics();
    opts.set_stats_dump_period_sec(60); // More frequent stats

    // ===== ADVANCED: PREFIX BLOOM FILTERS =====

    // For prefix scans (our inverted indexes use prefixes)
    opts.set_prefix_extractor(SliceTransform::create_fixed_prefix(8)); // 8-byte prefix

    opts
}

/// Get estimated memory usage for a profile
pub fn estimated_memory_usage(profile: RocksProfile) -> usize {
    match profile {
        RocksProfile::Development => {
            64 * 1024 * 1024 // 64 MB (cache)
            + 16 * 1024 * 1024 * 2 // 32 MB (write buffers)
        }
        RocksProfile::Production => {
            512 * 1024 * 1024 // 512 MB (cache)
            + 128 * 1024 * 1024 * 6 // 768 MB (write buffers)
        }
        RocksProfile::HighThroughput => {
            1024 * 1024 * 1024 // 1 GB (cache)
            + 256 * 1024 * 1024 * 12 // 3 GB (write buffers)
        }
    }
}

/// Print configuration summary
pub fn print_config_summary(profile: RocksProfile) {
    let mem_mb = estimated_memory_usage(profile) / (1024 * 1024);

    match profile {
        RocksProfile::Development => {
            tracing::info!("RocksDB Profile: Development");
            tracing::info!("  - Write buffer: 16 MB × 2 = 32 MB");
            tracing::info!("  - Block cache: 64 MB");
            tracing::info!("  - Background jobs: 2");
            tracing::info!("  - Estimated memory: {} MB", mem_mb);
        }
        RocksProfile::Production => {
            tracing::info!("RocksDB Profile: Production (Balanced)");
            tracing::info!("  - Write buffer: 128 MB × 6 = 768 MB");
            tracing::info!("  - Block cache: 512 MB");
            tracing::info!("  - Background jobs: 8 (6 compaction + 2 flush)");
            tracing::info!("  - Compression: Lz4 (L0-L2), Zstd (L3+)");
            tracing::info!("  - Bloom filters: enabled (10 bits/key)");
            tracing::info!("  - Estimated memory: {} MB", mem_mb);
        }
        RocksProfile::HighThroughput => {
            tracing::info!("RocksDB Profile: High Throughput");
            tracing::info!("  - Write buffer: 256 MB × 12 = 3 GB");
            tracing::info!("  - Block cache: 1 GB");
            tracing::info!("  - Background jobs: 16 (12 compaction + 4 flush)");
            tracing::info!("  - Compaction: Universal (optimized for writes)");
            tracing::info!("  - Compression: Lz4 only (speed over size)");
            tracing::info!("  - Parallelism: 8 subcompactions");
            tracing::info!("  - Estimated memory: {} MB", mem_mb);
            tracing::warn!("  ⚠️  Requires ~4 GB RAM and ulimit tuning");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_options_all_profiles() {
        // Should not panic
        let _ = create_options(RocksProfile::Development);
        let _ = create_options(RocksProfile::Production);
        let _ = create_options(RocksProfile::HighThroughput);
    }

    #[test]
    fn test_memory_estimates() {
        let dev_mem = estimated_memory_usage(RocksProfile::Development);
        let prod_mem = estimated_memory_usage(RocksProfile::Production);
        let high_mem = estimated_memory_usage(RocksProfile::HighThroughput);

        // Memory should increase with profile
        assert!(dev_mem < prod_mem);
        assert!(prod_mem < high_mem);

        // Sanity checks
        assert!(dev_mem < 200 * 1024 * 1024); // < 200 MB
        assert!(high_mem > 2 * 1024 * 1024 * 1024); // > 2 GB
    }

    #[test]
    fn test_print_config_summary() {
        // Should not panic
        print_config_summary(RocksProfile::Development);
        print_config_summary(RocksProfile::Production);
        print_config_summary(RocksProfile::HighThroughput);
    }
}
