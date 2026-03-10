// WAL Configuration with enterprise defaults
//
// Production-ready configuration with sensible defaults for high-throughput
// transactional workloads in data governance environments.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

/// Main WAL configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalConfig {
    // Storage configuration
    pub path: PathBuf,
    pub max_file_size: u64,
    pub max_segments: usize,
    pub preallocate: bool,
    pub direct_io: bool,
    pub min_free_disk_space: u64, // Minimum free disk space in bytes (prevents writes if below)

    // Durability configuration
    pub fsync_mode: FsyncMode,
    pub sync_interval: Duration,
    pub group_commit: GroupCommitConfig,

    // Rotation configuration
    pub rotation_policy: RotationPolicy,
    pub compaction_policy: CompactionPolicy,

    // Performance tuning
    pub write_buffer_size: usize,
    pub max_batch_size: usize,
    pub pipeline_depth: usize,
    pub compression: Option<CompressionCodec>,

    // Recovery configuration
    pub recovery_mode: RecoveryMode,
    pub corruption_tolerance: CorruptionTolerance,
    pub checkpoint_interval: Duration,

    // Multi-tenant configuration
    pub tenant_isolation: bool,
    pub quota_per_tenant: Option<QuotaConfig>,

    // Observability
    pub metrics_enabled: bool,
    pub metrics_prefix: String,
    pub slow_write_threshold: Duration,
    pub enable_tracing: bool,

    // I/O timeout configuration
    pub io_timeout: Option<Duration>, // Timeout for sync operations (None = no timeout)
}

impl WalConfig {
    /// Production defaults optimized for throughput and durability
    pub fn production() -> Self {
        Self {
            path: PathBuf::from("/var/lib/graphica/wal"),
            max_file_size: 1024 * 1024 * 1024, // 1GB segments
            max_segments: 10,
            preallocate: true,
            direct_io: false, // Direct I/O requires aligned buffers
            min_free_disk_space: 10 * 1024 * 1024 * 1024, // 10GB minimum free space

            fsync_mode: FsyncMode::BatchSync,
            sync_interval: Duration::from_millis(10),
            group_commit: GroupCommitConfig::default(),

            rotation_policy: RotationPolicy::SizeAndTime {
                max_size: 1024 * 1024 * 1024,
                max_age: Duration::from_secs(3600),
            },
            compaction_policy: CompactionPolicy::default(),

            write_buffer_size: 64 * 1024, // 64KB
            max_batch_size: 1000,
            pipeline_depth: 100,
            compression: Some(CompressionCodec::Lz4),

            recovery_mode: RecoveryMode::BestEffort,
            corruption_tolerance: CorruptionTolerance::SkipCorrupted,
            checkpoint_interval: Duration::from_secs(60),

            tenant_isolation: false,
            quota_per_tenant: None,

            metrics_enabled: true,
            metrics_prefix: "graphica_wal".to_string(),
            slow_write_threshold: Duration::from_millis(100),
            enable_tracing: true,

            io_timeout: Some(Duration::from_secs(30)), // 30 second timeout for I/O
        }
    }

    /// High-durability configuration for financial/compliance workloads
    pub fn high_durability() -> Self {
        Self {
            fsync_mode: FsyncMode::EveryWrite,
            sync_interval: Duration::from_millis(1),
            group_commit: GroupCommitConfig {
                enabled: false,
                max_wait: Duration::from_millis(0),
                max_batch: 1,
            },
            recovery_mode: RecoveryMode::Strict,
            corruption_tolerance: CorruptionTolerance::FailOnCorruption,
            ..Self::production()
        }
    }

    /// High-throughput configuration for analytics workloads
    pub fn high_throughput() -> Self {
        Self {
            fsync_mode: FsyncMode::Periodic,
            sync_interval: Duration::from_secs(1),
            group_commit: GroupCommitConfig {
                enabled: true,
                max_wait: Duration::from_millis(100),
                max_batch: 10000,
            },
            write_buffer_size: 1024 * 1024, // 1MB
            max_batch_size: 10000,
            pipeline_depth: 1000,
            ..Self::production()
        }
    }

    /// Builder pattern for custom configuration
    pub fn with_path(mut self, path: PathBuf) -> Self {
        self.path = path;
        self
    }

    pub fn with_max_file_size(mut self, size: u64) -> Self {
        self.max_file_size = size;
        self
    }

    pub fn with_fsync_mode(mut self, mode: FsyncMode) -> Self {
        self.fsync_mode = mode;
        self
    }

    pub fn with_compression(mut self, codec: CompressionCodec) -> Self {
        self.compression = Some(codec);
        self
    }

    pub fn with_tenant_isolation(mut self, quota: QuotaConfig) -> Self {
        self.tenant_isolation = true;
        self.quota_per_tenant = Some(quota);
        self
    }
}

impl Default for WalConfig {
    fn default() -> Self {
        Self::production()
    }
}

/// Fsync modes for durability vs performance tradeoff
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FsyncMode {
    /// Fsync after every write (highest durability, lowest throughput)
    EveryWrite,

    /// Batch multiple writes before fsync (balanced)
    BatchSync,

    /// Fsync on explicit sync() calls only
    OnDemand,

    /// Periodic fsync based on time interval
    Periodic,

    /// No fsync (OS page cache only - testing only!)
    #[cfg(test)]
    NoSync,
}

/// Group commit configuration for batching writes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupCommitConfig {
    pub enabled: bool,
    pub max_wait: Duration,
    pub max_batch: usize,
}

impl Default for GroupCommitConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_wait: Duration::from_millis(10),
            max_batch: 100,
        }
    }
}

/// Rotation policy for creating new segments
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RotationPolicy {
    /// Rotate based on size only
    Size { max_size: u64 },

    /// Rotate based on time only
    Time { max_age: Duration },

    /// Rotate based on size OR time (whichever comes first)
    SizeAndTime { max_size: u64, max_age: Duration },

    /// Rotate based on number of entries
    EntryCount { max_entries: u64 },

    /// Custom rotation logic
    Custom(String),
}

/// Compaction policy for removing committed entries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionPolicy {
    pub enabled: bool,
    pub min_segments: usize,
    pub max_segments: usize,
    pub compaction_threshold: f64, // % of dead entries
    pub compaction_interval: Duration,
    pub archive_before_compact: bool,
}

impl Default for CompactionPolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            min_segments: 2,
            max_segments: 10,
            compaction_threshold: 0.5,
            compaction_interval: Duration::from_secs(3600),
            archive_before_compact: true,
        }
    }
}

/// Compression codecs for WAL entries
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompressionCodec {
    None,
    Lz4,
    Zstd(i32), // Compression level
    Snappy,
}

/// Recovery modes for handling corruption
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecoveryMode {
    /// Strict mode - fail on any corruption
    Strict,

    /// Best effort - recover what's possible
    BestEffort,

    /// Fast recovery - skip validation for speed
    Fast,
}

/// How to handle corrupted entries during recovery
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CorruptionTolerance {
    /// Fail immediately on corruption
    FailOnCorruption,

    /// Skip corrupted entries, continue recovery
    SkipCorrupted,

    /// Try to repair corruption
    AttemptRepair,

    /// Truncate at first corruption
    TruncateAtCorruption,
}

/// Multi-tenant quota configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaConfig {
    pub max_bytes_per_tenant: u64,
    pub max_writes_per_sec: u64,
    pub max_entries: u64,
    pub enforcement_mode: QuotaEnforcement,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuotaEnforcement {
    /// Block writes when quota exceeded
    Hard,

    /// Log warning but allow writes
    Soft,

    /// Throttle writes to stay within quota
    Throttle,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_builders() {
        let config = WalConfig::production()
            .with_path(PathBuf::from("/tmp/wal"))
            .with_max_file_size(512 * 1024 * 1024)
            .with_fsync_mode(FsyncMode::BatchSync);

        assert_eq!(config.path, PathBuf::from("/tmp/wal"));
        assert_eq!(config.max_file_size, 512 * 1024 * 1024);
        assert_eq!(config.fsync_mode, FsyncMode::BatchSync);
    }

    #[test]
    fn test_config_presets() {
        let prod = WalConfig::production();
        assert_eq!(prod.fsync_mode, FsyncMode::BatchSync);

        let durability = WalConfig::high_durability();
        assert_eq!(durability.fsync_mode, FsyncMode::EveryWrite);

        let throughput = WalConfig::high_throughput();
        assert_eq!(throughput.fsync_mode, FsyncMode::Periodic);
    }
}
