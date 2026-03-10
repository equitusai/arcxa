// Enterprise-grade error taxonomy for WAL operations
//
// Comprehensive error types with recovery strategies and observability hooks

use std::fmt;
use std::io;
use std::path::PathBuf;
use thiserror::Error;

use super::LogSequenceNumber;

pub type WalResult<T> = Result<T, WalError>;

/// Main WAL error type with detailed categorization
#[derive(Debug, Error)]
pub enum WalError {
    #[error("I/O error: {source} at {path:?}")]
    Io {
        #[source]
        source: io::Error,
        path: Option<PathBuf>,
    },

    #[error("Corruption detected at LSN {lsn}: {details}")]
    Corruption {
        lsn: LogSequenceNumber,
        details: String,
        recoverable: bool,
    },

    #[error("Checksum mismatch at LSN {lsn}: expected {expected:08x}, got {actual:08x}")]
    ChecksumMismatch {
        lsn: LogSequenceNumber,
        expected: u32,
        actual: u32,
    },

    #[error("Transaction error: {0}")]
    Transaction(#[from] TransactionError),

    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),

    #[error("Recovery error: {0}")]
    Recovery(#[from] RecoveryError),

    #[error("Rotation error: {0}")]
    Rotation(#[from] RotationError),

    #[error("Quota exceeded for tenant {tenant}: {details}")]
    QuotaExceeded { tenant: String, details: String },

    #[error("WAL is full: max segments ({max_segments}) reached")]
    WalFull { max_segments: usize },

    #[error("Insufficient disk space: {available_bytes} bytes available, {required_bytes} bytes required, {min_free_bytes} bytes minimum")]
    InsufficientDiskSpace {
        available_bytes: u64,
        required_bytes: u64,
        min_free_bytes: u64,
    },

    #[error("Invalid LSN: {lsn}")]
    InvalidLsn { lsn: LogSequenceNumber },

    #[error("Entry too large: {size} bytes exceeds max {max_size} bytes")]
    EntryTooLarge { size: usize, max_size: usize },

    #[error("Timeout waiting for {operation} after {timeout_ms}ms")]
    Timeout { operation: String, timeout_ms: u64 },

    #[error("WAL is closed")]
    Closed,

    #[error("Configuration error: {0}")]
    Configuration(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Unknown error: {0}")]
    Unknown(String),
}

impl WalError {
    /// Check if error is recoverable through retry
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::Io { source, .. } if source.kind() == io::ErrorKind::Interrupted
                || source.kind() == io::ErrorKind::WouldBlock
                || source.kind() == io::ErrorKind::TimedOut
        ) || matches!(self, Self::Timeout { .. })
    }

    /// Check if error indicates data loss
    pub fn is_data_loss(&self) -> bool {
        matches!(
            self,
            Self::Corruption {
                recoverable: false,
                ..
            } | Self::ChecksumMismatch { .. }
                | Self::Recovery(RecoveryError::UnrecoverableCorruption { .. })
        )
    }

    /// Get suggested recovery action
    pub fn recovery_action(&self) -> RecoveryAction {
        match self {
            Self::Io { source, .. } => match source.kind() {
                io::ErrorKind::PermissionDenied => RecoveryAction::FixPermissions,
                io::ErrorKind::NotFound => RecoveryAction::CreatePath,
                io::ErrorKind::OutOfMemory => RecoveryAction::IncreaseResources,
                _ => RecoveryAction::Retry,
            },
            Self::Corruption {
                recoverable: true, ..
            } => RecoveryAction::RunRecovery,
            Self::Corruption {
                recoverable: false, ..
            } => RecoveryAction::RestoreFromBackup,
            Self::ChecksumMismatch { .. } => RecoveryAction::ValidateAndRepair,
            Self::WalFull { .. } => RecoveryAction::RunCompaction,
            Self::QuotaExceeded { .. } => RecoveryAction::IncreaseQuota,
            Self::Closed => RecoveryAction::Restart,
            _ => RecoveryAction::Manual,
        }
    }

    /// Convert to metric labels for observability
    pub fn to_metrics_labels(&self) -> Vec<(&str, String)> {
        vec![
            ("error_type", self.error_type()),
            ("severity", self.severity().to_string()),
            ("retryable", self.is_retryable().to_string()),
            ("data_loss", self.is_data_loss().to_string()),
        ]
    }

    fn error_type(&self) -> String {
        match self {
            Self::Io { .. } => "io",
            Self::Corruption { .. } => "corruption",
            Self::ChecksumMismatch { .. } => "checksum",
            Self::Transaction(_) => "transaction",
            Self::Storage(_) => "storage",
            Self::Recovery(_) => "recovery",
            Self::Rotation(_) => "rotation",
            Self::QuotaExceeded { .. } => "quota",
            Self::WalFull { .. } => "full",
            Self::InsufficientDiskSpace { .. } => "disk_space",
            Self::InvalidLsn { .. } => "invalid_lsn",
            Self::EntryTooLarge { .. } => "oversized",
            Self::Timeout { .. } => "timeout",
            Self::Closed => "closed",
            Self::Configuration(_) => "config",
            Self::Serialization(_) => "serialization",
            Self::Unknown(_) => "unknown",
        }
        .to_string()
    }

    fn severity(&self) -> ErrorSeverity {
        match self {
            Self::Corruption {
                recoverable: false, ..
            }
            | Self::ChecksumMismatch { .. } => ErrorSeverity::Critical,

            Self::Corruption {
                recoverable: true, ..
            }
            | Self::Io { .. }
            | Self::Storage(_)
            | Self::Recovery(_)
            | Self::WalFull { .. }
            | Self::InsufficientDiskSpace { .. } => ErrorSeverity::High,

            Self::Transaction(_)
            | Self::Rotation(_)
            | Self::QuotaExceeded { .. }
            | Self::EntryTooLarge { .. } => ErrorSeverity::Medium,

            Self::Timeout { .. }
            | Self::InvalidLsn { .. }
            | Self::Closed
            | Self::Configuration(_)
            | Self::Serialization(_)
            | Self::Unknown(_) => ErrorSeverity::Low,
        }
    }
}

#[derive(Debug, Error)]
pub enum TransactionError {
    #[error("Transaction {tx_id} not found")]
    NotFound { tx_id: u64 },

    #[error("Transaction {tx_id} already {state}")]
    InvalidState { tx_id: u64, state: String },

    #[error("Deadlock detected involving transactions {tx_ids:?}")]
    Deadlock { tx_ids: Vec<u64> },

    #[error("Transaction {tx_id} timed out after {timeout_ms}ms")]
    Timeout { tx_id: u64, timeout_ms: u64 },

    #[error("Two-phase commit failed: {reason}")]
    TwoPhaseCommitFailed { reason: String },

    #[error("Participant {participant} failed: {reason}")]
    ParticipantFailed { participant: String, reason: String },
}

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("RocksDB error: {0}")]
    RocksDb(String),

    #[error("Kafka error: {0}")]
    Kafka(String),

    #[error("Parquet error: {0}")]
    Parquet(String),

    #[error("Archive error: {0}")]
    Archive(String),

    #[error("Multi-tier coordination failed: {0}")]
    CoordinationFailed(String),
}

#[derive(Debug, Error)]
pub enum RecoveryError {
    #[error("No valid checkpoint found")]
    NoCheckpoint,

    #[error("Unrecoverable corruption at LSN {lsn}")]
    UnrecoverableCorruption { lsn: LogSequenceNumber },

    #[error("Recovery incomplete: recovered {recovered} of {total} entries")]
    IncompleteRecovery { recovered: u64, total: u64 },

    #[error("Validation failed: {reason}")]
    ValidationFailed { reason: String },

    #[error("Repair failed: {reason}")]
    RepairFailed { reason: String },
}

#[derive(Debug, Error)]
pub enum RotationError {
    #[error("Cannot rotate: {reason}")]
    CannotRotate { reason: String },

    #[error("Compaction failed: {reason}")]
    CompactionFailed { reason: String },

    #[error("Archive failed: {reason}")]
    ArchiveFailed { reason: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorSeverity {
    Critical, // Data loss, corruption
    High,     // Service degradation
    Medium,   // Performance impact
    Low,      // Recoverable, minimal impact
}

impl fmt::Display for ErrorSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Critical => write!(f, "critical"),
            Self::High => write!(f, "high"),
            Self::Medium => write!(f, "medium"),
            Self::Low => write!(f, "low"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryAction {
    Retry,
    RunRecovery,
    RunCompaction,
    ValidateAndRepair,
    RestoreFromBackup,
    FixPermissions,
    CreatePath,
    IncreaseResources,
    IncreaseQuota,
    Restart,
    Manual,
}

/// Error context for detailed diagnostics
#[derive(Debug, Clone)]
pub struct ErrorContext {
    pub timestamp: u64,
    pub component: String,
    pub operation: String,
    pub lsn: Option<LogSequenceNumber>,
    pub tenant: Option<String>,
    pub additional_info: Vec<(String, String)>,
}

impl ErrorContext {
    pub fn new(component: &str, operation: &str) -> Self {
        Self {
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            component: component.to_string(),
            operation: operation.to_string(),
            lsn: None,
            tenant: None,
            additional_info: Vec::new(),
        }
    }

    pub fn with_lsn(mut self, lsn: LogSequenceNumber) -> Self {
        self.lsn = Some(lsn);
        self
    }

    pub fn with_tenant(mut self, tenant: String) -> Self {
        self.tenant = Some(tenant);
        self
    }

    pub fn with_info(mut self, key: String, value: String) -> Self {
        self.additional_info.push((key, value));
        self
    }
}

// Conversion helpers for external errors
impl From<io::Error> for WalError {
    fn from(err: io::Error) -> Self {
        Self::Io {
            source: err,
            path: None,
        }
    }
}

impl From<bincode::Error> for WalError {
    fn from(err: bincode::Error) -> Self {
        Self::Serialization(err.to_string())
    }
}
