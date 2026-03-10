// Core trait abstractions for WAL implementations
//
// These traits follow SOLID principles and enable multiple WAL backends
// (file-based, distributed, cloud-native) while maintaining consistent APIs.

use async_trait::async_trait;
use bytes::Bytes;
use std::ops::Range;
use std::sync::Arc;
use tokio::sync::oneshot;

use super::{LogSequenceNumber, WalEntry, WalError, WalResult};

/// Core WAL operations - must be implemented by all WAL backends
#[async_trait]
pub trait WriteAheadLog: Send + Sync + 'static {
    /// Append an entry to the WAL, returning its LSN
    async fn append(&self, entry: WalEntry) -> WalResult<LogSequenceNumber>;

    /// Batch append for higher throughput
    async fn append_batch(&self, entries: Vec<WalEntry>) -> WalResult<Vec<LogSequenceNumber>>;

    /// Sync WAL to persistent storage (fsync)
    async fn sync(&self) -> WalResult<()>;

    /// Mark entries as committed (can be compacted)
    async fn commit(&self, lsn: LogSequenceNumber) -> WalResult<()>;

    /// Batch commit for efficiency
    async fn commit_batch(&self, lsns: Vec<LogSequenceNumber>) -> WalResult<()>;

    /// Get current tail LSN
    async fn tail_lsn(&self) -> LogSequenceNumber;

    /// Get oldest uncommitted LSN (head of active log)
    async fn head_lsn(&self) -> LogSequenceNumber;

    /// Check WAL health (disk space, corruption, etc.)
    async fn is_healthy(&self) -> bool;

    /// Get metrics snapshot
    async fn metrics(&self) -> WalMetricsSnapshot;

    /// Truncate WAL up to LSN (remove committed entries)
    async fn truncate(&self, up_to_lsn: LogSequenceNumber) -> WalResult<()>;

    /// Close WAL gracefully
    async fn close(&self) -> WalResult<()>;
}

/// Read operations on WAL - separated for interface segregation
#[async_trait]
pub trait WalReader: Send + Sync {
    /// Read a specific entry by LSN
    async fn read(&self, lsn: LogSequenceNumber) -> WalResult<WalEntry>;

    /// Scan entries in LSN range
    async fn scan(&self, range: Range<LogSequenceNumber>) -> WalResult<Vec<WalEntry>>;

    /// Stream entries from LSN (for recovery/replay)
    async fn stream_from(&self, start_lsn: LogSequenceNumber) -> WalResult<WalEntryStream>;

    /// Find entries matching predicate (for debugging/analytics)
    async fn find<F>(&self, predicate: F) -> WalResult<Vec<WalEntry>>
    where
        F: Fn(&WalEntry) -> bool + Send + Sync + 'static;
}

/// Write operations - separated for write-only replicas
#[async_trait]
pub trait WalWriter: Send + Sync {
    /// Write with completion callback
    async fn write_with_callback(
        &self,
        entry: WalEntry,
        callback: oneshot::Sender<WalResult<LogSequenceNumber>>,
    );

    /// Pipeline writes for maximum throughput
    async fn pipeline_write(&self, entry: WalEntry) -> WalResult<PipelineHandle>;
}

/// Transactional WAL operations for coordinated commits
#[async_trait]
pub trait TransactionalWal: WriteAheadLog {
    /// Begin a new transaction
    async fn begin_transaction(&self) -> WalResult<TransactionId>;

    /// Add entry to transaction
    async fn add_to_transaction(
        &self,
        tx_id: TransactionId,
        entry: WalEntry,
    ) -> WalResult<LogSequenceNumber>;

    /// Prepare transaction for commit (2PC prepare phase)
    async fn prepare(&self, tx_id: TransactionId) -> WalResult<()>;

    /// Commit prepared transaction
    async fn commit_transaction(&self, tx_id: TransactionId) -> WalResult<()>;

    /// Abort transaction, rollback entries
    async fn abort_transaction(&self, tx_id: TransactionId) -> WalResult<()>;

    /// Get active transactions (for recovery)
    async fn active_transactions(&self) -> Vec<TransactionId>;
}

/// Recovery operations for crash recovery
#[async_trait]
pub trait Recoverable {
    /// Perform recovery, returning recovered entries
    async fn recover(&self) -> WalResult<RecoveryResult>;

    /// Validate WAL integrity
    async fn validate(&self) -> WalResult<ValidationReport>;

    /// Repair corrupted segments if possible
    async fn repair(&self) -> WalResult<RepairReport>;
}

/// Rotation and compaction operations
#[async_trait]
pub trait Rotatable {
    /// Check if rotation is needed
    async fn should_rotate(&self) -> bool;

    /// Rotate to new segment
    async fn rotate(&self) -> WalResult<()>;

    /// Compact old segments
    async fn compact(&self) -> WalResult<CompactionReport>;

    /// Archive old segments to cold storage
    async fn archive(&self, destination: &str) -> WalResult<()>;
}

// Support types

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TransactionId(pub u64);

#[derive(Debug)]
pub struct PipelineHandle {
    pub lsn_future: oneshot::Receiver<LogSequenceNumber>,
}

#[derive(Debug, Clone)]
pub struct WalMetricsSnapshot {
    pub total_writes: u64,
    pub total_bytes: u64,
    pub uncommitted_entries: u64,
    pub uncommitted_bytes: u64,
    pub active_transactions: usize,
    pub rotation_count: u64,
    pub compaction_count: u64,
    pub recovery_count: u64,
    pub corruption_events: u64,
    pub avg_write_latency_us: u64,
    pub p99_write_latency_us: u64,
    pub sync_count: u64,
    pub avg_sync_latency_ms: u64,
}

pub struct WalEntryStream {
    receiver: tokio::sync::mpsc::Receiver<WalResult<WalEntry>>,
}

impl WalEntryStream {
    /// Create a new WalEntryStream from a receiver
    pub fn new(receiver: tokio::sync::mpsc::Receiver<WalResult<WalEntry>>) -> Self {
        Self { receiver }
    }

    pub async fn next(&mut self) -> Option<WalResult<WalEntry>> {
        self.receiver.recv().await
    }
}

#[derive(Debug)]
pub struct RecoveryResult {
    pub recovered_entries: Vec<WalEntry>,
    pub last_valid_lsn: LogSequenceNumber,
    pub corrupted_entries: Vec<(LogSequenceNumber, WalError)>,
    pub recovery_time_ms: u64,
}

#[derive(Debug)]
pub struct ValidationReport {
    pub valid_segments: usize,
    pub corrupted_segments: Vec<String>,
    pub total_entries: u64,
    pub valid_entries: u64,
    pub checksum_failures: Vec<LogSequenceNumber>,
}

#[derive(Debug)]
pub struct RepairReport {
    pub repaired_segments: Vec<String>,
    pub unrecoverable_segments: Vec<String>,
    pub data_loss: bool,
    pub recovered_bytes: u64,
}

#[derive(Debug)]
pub struct CompactionReport {
    pub segments_compacted: usize,
    pub bytes_reclaimed: u64,
    pub entries_removed: u64,
    pub duration_ms: u64,
}
