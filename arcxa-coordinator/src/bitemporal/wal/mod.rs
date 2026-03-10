//! Write-Ahead Log (WAL) for Crash Recovery
//!
//! Provides atomicity guarantees for writes across RDF store and temporal indexes.
//! Prevents corruption if process crashes mid-operation.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────┐
//! │  1. Log Operation to WAL                │
//! │     (uncommitted)                       │
//! ├─────────────────────────────────────────┤
//! │  2. Execute RDF Store Write             │
//! ├─────────────────────────────────────────┤
//! │  3. Execute Temporal Index Update       │
//! ├─────────────────────────────────────────┤
//! │  4. Mark Operation as Committed         │
//! │     (atomically delete WAL entry)       │
//! └─────────────────────────────────────────┘
//!
//! On Restart:
//! - Scan WAL for uncommitted operations
//! - Replay uncommitted operations
//! - System guaranteed consistent
//! ```

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};
use uuid::Uuid;

/// WAL operation types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WalOperation {
    /// Insert RDF-star triple with temporal indexing
    InsertTriple {
        /// Serialized AnnotatedTriple (JSON)
        triple_json: String,
        /// Version ID for temporal index
        version_id: String,
        /// Transaction ID (serialized JSON)
        tx_id_json: String,
        /// Graph URI (if any)
        graph_uri: Option<String>,
    },
    /// Batch insert
    InsertBatch {
        /// Serialized vec of AnnotatedTriples
        triples_json: String,
        /// Graph URI (if any)
        graph_uri: Option<String>,
    },
}

/// WAL entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalEntry {
    /// Unique operation ID
    pub op_id: String,
    /// Timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Operation to execute
    pub operation: WalOperation,
    /// Committed flag (true = operation completed successfully)
    pub committed: bool,
}

/// Write-Ahead Log
pub struct WriteAheadLog {
    /// Path to WAL directory
    wal_dir: PathBuf,
}

impl WriteAheadLog {
    /// Create new WAL
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let wal_dir = path.as_ref().join("wal");

        // Create WAL directory if it doesn't exist
        fs::create_dir_all(&wal_dir).context("Failed to create WAL directory")?;

        info!("WAL initialized at: {}", wal_dir.display());

        Ok(Self { wal_dir })
    }

    /// Log an operation (before execution)
    ///
    /// Returns operation ID for later commit/rollback.
    pub fn log_operation(&self, operation: WalOperation) -> Result<String> {
        let op_id = Uuid::new_v4().to_string();

        let entry = WalEntry {
            op_id: op_id.clone(),
            timestamp: chrono::Utc::now(),
            operation,
            committed: false,
        };

        let entry_path = self.wal_dir.join(format!("{}.wal", op_id));
        let entry_json =
            serde_json::to_string_pretty(&entry).context("Failed to serialize WAL entry")?;

        // Atomic write: temp file + rename
        let temp_path = entry_path.with_extension("tmp");
        fs::write(&temp_path, entry_json).context("Failed to write WAL entry to temp file")?;
        fs::rename(&temp_path, &entry_path).context("Failed to rename WAL entry")?;

        debug!("Logged operation to WAL: {}", op_id);

        Ok(op_id)
    }

    /// Mark operation as committed (removes WAL entry)
    pub fn commit(&self, op_id: &str) -> Result<()> {
        let entry_path = self.wal_dir.join(format!("{}.wal", op_id));

        if entry_path.exists() {
            fs::remove_file(&entry_path).context("Failed to remove committed WAL entry")?;
            debug!("Committed operation: {}", op_id);
        }

        Ok(())
    }

    /// Get all uncommitted operations (for replay on startup)
    pub fn get_uncommitted_operations(&self) -> Result<Vec<WalEntry>> {
        let mut uncommitted = Vec::new();

        let entries = fs::read_dir(&self.wal_dir).context("Failed to read WAL directory")?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("wal") {
                let entry_json = fs::read_to_string(&path).context("Failed to read WAL entry")?;

                let wal_entry: WalEntry =
                    serde_json::from_str(&entry_json).context("Failed to deserialize WAL entry")?;

                if !wal_entry.committed {
                    uncommitted.push(wal_entry);
                }
            }
        }

        // Sort by timestamp (oldest first)
        uncommitted.sort_by_key(|e| e.timestamp);

        if !uncommitted.is_empty() {
            warn!("Found {} uncommitted WAL operations", uncommitted.len());
        }

        Ok(uncommitted)
    }

    /// Cleanup: Remove old committed entries (should already be deleted)
    pub fn cleanup(&self) -> Result<usize> {
        let mut cleaned = 0;

        let entries = fs::read_dir(&self.wal_dir).context("Failed to read WAL directory")?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("wal") {
                let entry_json = fs::read_to_string(&path)?;
                let wal_entry: WalEntry = serde_json::from_str(&entry_json)?;

                if wal_entry.committed {
                    fs::remove_file(&path)?;
                    cleaned += 1;
                }
            }
        }

        if cleaned > 0 {
            info!("Cleaned up {} committed WAL entries", cleaned);
        }

        Ok(cleaned)
    }

    /// Get WAL statistics
    pub fn statistics(&self) -> Result<WalStatistics> {
        let mut total_entries = 0;
        let mut uncommitted_entries = 0;

        let entries = fs::read_dir(&self.wal_dir).context("Failed to read WAL directory")?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("wal") {
                total_entries += 1;

                let entry_json = fs::read_to_string(&path)?;
                let wal_entry: WalEntry = serde_json::from_str(&entry_json)?;

                if !wal_entry.committed {
                    uncommitted_entries += 1;
                }
            }
        }

        Ok(WalStatistics {
            total_entries,
            uncommitted_entries,
        })
    }
}

/// WAL statistics
#[derive(Debug, Clone)]
pub struct WalStatistics {
    pub total_entries: usize,
    pub uncommitted_entries: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_wal_basic_lifecycle() {
        let temp_dir = TempDir::new().unwrap();
        let wal = WriteAheadLog::new(temp_dir.path()).unwrap();

        // Log operation
        let op = WalOperation::InsertTriple {
            triple_json: "{}".to_string(),
            version_id: "v1".to_string(),
            tx_id_json: "{}".to_string(),
            graph_uri: None,
        };

        let op_id = wal.log_operation(op).unwrap();

        // Verify uncommitted
        let uncommitted = wal.get_uncommitted_operations().unwrap();
        assert_eq!(uncommitted.len(), 1);
        assert_eq!(uncommitted[0].op_id, op_id);

        // Commit
        wal.commit(&op_id).unwrap();

        // Verify committed (removed)
        let uncommitted = wal.get_uncommitted_operations().unwrap();
        assert_eq!(uncommitted.len(), 0);
    }

    #[test]
    fn test_wal_crash_recovery() {
        let temp_dir = TempDir::new().unwrap();

        // Simulate crash: log operation but don't commit
        {
            let wal = WriteAheadLog::new(temp_dir.path()).unwrap();
            let op = WalOperation::InsertTriple {
                triple_json: "{}".to_string(),
                version_id: "v1".to_string(),
                tx_id_json: "{}".to_string(),
                graph_uri: None,
            };
            wal.log_operation(op).unwrap();
            // Drop WAL (simulate crash)
        }

        // Restart: Create new WAL and check for uncommitted
        {
            let wal = WriteAheadLog::new(temp_dir.path()).unwrap();
            let uncommitted = wal.get_uncommitted_operations().unwrap();
            assert_eq!(uncommitted.len(), 1, "Should recover uncommitted operation");
        }
    }
}
