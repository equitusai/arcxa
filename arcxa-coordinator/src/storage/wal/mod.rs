// Enterprise-grade Write-Ahead Log (WAL) for Graphica
//
// This module provides transactional durability guarantees for the data governance platform,
// ensuring zero data loss on crashes while maintaining high throughput (100K+ writes/sec).
//
// Architecture:
// - Trait-based abstractions for extensibility
// - File-based implementation with checksums and rotation
// - Coordinated transactions across RocksDB, Kafka, and Parquet
// - Crash recovery with corruption detection
// - Production observability and metrics

mod config;
mod coordinator;
mod entry;
mod errors;
mod file_wal;
mod metrics;
mod recovery;
mod rotation;
mod traits;

pub use config::{
    CompactionPolicy, CompressionCodec, CorruptionTolerance, FsyncMode, GroupCommitConfig,
    RecoveryMode, RotationPolicy, WalConfig,
};
pub use coordinator::{TransactionHandle, WalCoordinator};
pub use entry::{
    EntryPayload, EntryType, IsolationLevel, LogSequenceNumber, RdfOperation, RdfTripleEntry,
    StorageCheckpoint, StorageType, TransactionContext, TransactionOp, WalEntry,
};
pub use errors::{
    RecoveryError, RotationError, StorageError, TransactionError, WalError, WalResult,
};
pub use file_wal::FileWal;
pub use metrics::{WalMetrics, WalMetricsCollector};
pub use recovery::{RecoveryManager, RecoveryReport, RecoveryStrategy};
pub use rotation::{CompactionStrategy, RotationManager};
pub use traits::{
    CompactionReport, RecoveryResult, RepairReport, TransactionId, TransactionalWal,
    ValidationReport, WalEntryStream, WalMetricsSnapshot, WalReader, WalWriter, WriteAheadLog,
};

use std::sync::Arc;
use tokio::sync::RwLock;

/// Factory for creating WAL instances with appropriate configuration
pub struct WalFactory;

impl WalFactory {
    /// Create a file-based WAL with production defaults
    pub async fn create_file_wal(config: WalConfig) -> WalResult<Arc<dyn WriteAheadLog>> {
        let metrics = Arc::new(WalMetricsCollector::new(&config.metrics_prefix));
        let file_wal = FileWal::new(config, metrics.clone()).await?;

        Ok(Arc::new(file_wal) as Arc<dyn WriteAheadLog>)
    }

    /// Create a WAL with coordinator for multi-tier storage
    pub async fn create_coordinated_wal(config: WalConfig) -> WalResult<Arc<WalCoordinator>> {
        let wal = Self::create_file_wal(config.clone()).await?;
        let coordinator = WalCoordinator::new(wal, config).await?;

        Ok(Arc::new(coordinator))
    }
}

use once_cell::sync::OnceCell;

/// Global WAL instance for the application
pub struct GlobalWal {
    inner: OnceCell<Arc<WalCoordinator>>,
}

impl GlobalWal {
    pub const fn new() -> Self {
        Self {
            inner: OnceCell::new(),
        }
    }

    pub async fn initialize(&self, config: WalConfig) -> WalResult<()> {
        let coordinator = WalFactory::create_coordinated_wal(config).await?;
        self.inner.set(coordinator).map_err(|_| WalError::Io {
            source: std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "WAL already initialized",
            ),
            path: None,
        })?;
        Ok(())
    }

    pub fn get(&self) -> Option<Arc<WalCoordinator>> {
        self.inner.get().cloned()
    }
}

// Global singleton for WAL access
pub static GLOBAL_WAL: GlobalWal = GlobalWal::new();

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_wal_factory() {
        let tmp_dir = TempDir::new().unwrap();
        let config = WalConfig::default()
            .with_path(tmp_dir.path().join("wal"))
            .with_max_file_size(1024 * 1024);

        let wal = WalFactory::create_file_wal(config).await.unwrap();
        assert!(wal.is_healthy().await);
    }
}
