//! Transaction Coordinator for Batch Jobs
//!
//! Manages transaction boundaries and commit/rollback operations for batch job executions.
//!
//! ## Transaction Modes
//!
//! ### PerFile (Default)
//! Each CSV file is processed in its own transaction. If a file fails, only that file is
//! rolled back. Other files remain committed.
//!
//! ```text
//! File 1: [BEGIN → INSERT → COMMIT] ✓
//! File 2: [BEGIN → INSERT → ERROR → ROLLBACK] ✗
//! File 3: [BEGIN → INSERT → COMMIT] ✓
//! Result: Files 1 and 3 committed, File 2 rolled back
//! ```
//!
//! ### AllOrNothing
//! All files are processed in a single transaction. If any file fails, the entire batch
//! is rolled back.
//!
//! ```text
//! [BEGIN]
//!   File 1: INSERT ✓
//!   File 2: INSERT → ERROR ✗
//! [ROLLBACK]
//! Result: Nothing committed, all changes rolled back
//! ```
//!
//! ### Batched
//! Files are grouped into batches (e.g., 5 files per batch). Each batch is a separate
//! transaction. If a file fails, only that batch is rolled back.
//!
//! ```text
//! Batch 1: [BEGIN → File1 → File2 → File3 → COMMIT] ✓
//! Batch 2: [BEGIN → File4 → File5 → ERROR → ROLLBACK] ✗
//! Batch 3: [BEGIN → File6 → File7 → COMMIT] ✓
//! Result: Batches 1 and 3 committed, Batch 2 rolled back
//! ```

use crate::workflows::domain::{BatchJob, TransactionMode};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, error, info, warn};

/// Transaction state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransactionState {
    /// Transaction not started
    NotStarted,

    /// Transaction active
    Active,

    /// Transaction committed
    Committed,

    /// Transaction rolled back
    RolledBack,
}

/// Transaction info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionInfo {
    /// Transaction ID
    pub transaction_id: String,

    /// State
    pub state: TransactionState,

    /// File IDs in this transaction
    pub file_ids: Vec<String>,

    /// Started timestamp
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,

    /// Committed/rolled back timestamp
    pub ended_at: Option<chrono::DateTime<chrono::Utc>>,

    /// Error message if rolled back
    pub error: Option<String>,
}

impl TransactionInfo {
    pub fn new(transaction_id: String) -> Self {
        Self {
            transaction_id,
            state: TransactionState::NotStarted,
            file_ids: Vec::new(),
            started_at: None,
            ended_at: None,
            error: None,
        }
    }

    pub fn begin(&mut self) {
        self.state = TransactionState::Active;
        self.started_at = Some(chrono::Utc::now());
    }

    pub fn add_file(&mut self, file_id: String) {
        self.file_ids.push(file_id);
    }

    pub fn commit(&mut self) {
        self.state = TransactionState::Committed;
        self.ended_at = Some(chrono::Utc::now());
    }

    pub fn rollback(&mut self, error: String) {
        self.state = TransactionState::RolledBack;
        self.ended_at = Some(chrono::Utc::now());
        self.error = Some(error);
    }
}

/// Transaction coordinator
pub struct TransactionCoordinator {
    /// Batch job being coordinated
    batch_job: BatchJob,

    /// Active transactions
    transactions: HashMap<String, TransactionInfo>,

    /// Current transaction ID (for AllOrNothing mode)
    current_transaction_id: Option<String>,

    /// Batch counter (for Batched mode)
    batch_counter: usize,

    /// Files in current batch (for Batched mode)
    current_batch_files: Vec<String>,
}

impl TransactionCoordinator {
    /// Create a new transaction coordinator
    pub fn new(batch_job: BatchJob) -> Self {
        Self {
            batch_job,
            transactions: HashMap::new(),
            current_transaction_id: None,
            batch_counter: 0,
            current_batch_files: Vec::new(),
        }
    }

    /// Begin transaction for a file or batch
    ///
    /// Returns the transaction ID that should be used for this operation.
    pub fn begin_transaction(&mut self, file_id: &str) -> Result<String> {
        let transaction_id = match self.batch_job.config.transaction_mode {
            TransactionMode::PerFile => {
                // Each file gets its own transaction
                let txn_id = format!("txn_{}_{}", self.batch_job.job_id, file_id);
                let mut txn = TransactionInfo::new(txn_id.clone());
                txn.begin();
                txn.add_file(file_id.to_string());
                self.transactions.insert(txn_id.clone(), txn);

                info!(
                    "Started PerFile transaction {} for file {}",
                    txn_id, file_id
                );

                txn_id
            }

            TransactionMode::AllOrNothing => {
                // Single transaction for entire batch
                if let Some(txn_id) = &self.current_transaction_id {
                    // Transaction already exists, add this file to it
                    if let Some(txn) = self.transactions.get_mut(txn_id) {
                        txn.add_file(file_id.to_string());
                    }
                    txn_id.clone()
                } else {
                    // Start the batch-wide transaction
                    let txn_id = format!("txn_{}_all", self.batch_job.job_id);
                    let mut txn = TransactionInfo::new(txn_id.clone());
                    txn.begin();
                    txn.add_file(file_id.to_string());
                    self.transactions.insert(txn_id.clone(), txn);
                    self.current_transaction_id = Some(txn_id.clone());

                    info!("Started AllOrNothing transaction {} (batch-wide)", txn_id);

                    txn_id
                }
            }

            TransactionMode::Batched { batch_size } => {
                // Check if we need to start a new batch transaction
                if self.current_batch_files.len() >= batch_size {
                    // Commit previous batch and start new one
                    if let Some(txn_id) = self.current_transaction_id.clone() {
                        self.commit_transaction_internal(&txn_id)?;
                    }
                    self.current_batch_files.clear();
                    self.batch_counter += 1;
                    self.current_transaction_id = None;
                }

                // Get or create batch transaction
                if let Some(txn_id) = &self.current_transaction_id {
                    // Add to existing batch
                    if let Some(txn) = self.transactions.get_mut(txn_id) {
                        txn.add_file(file_id.to_string());
                    }
                    self.current_batch_files.push(file_id.to_string());
                    txn_id.clone()
                } else {
                    // Start new batch transaction
                    let txn_id =
                        format!("txn_{}_batch_{}", self.batch_job.job_id, self.batch_counter);
                    let mut txn = TransactionInfo::new(txn_id.clone());
                    txn.begin();
                    txn.add_file(file_id.to_string());
                    self.transactions.insert(txn_id.clone(), txn);
                    self.current_transaction_id = Some(txn_id.clone());
                    self.current_batch_files.push(file_id.to_string());

                    info!(
                        "Started Batched transaction {} (batch {}, size {})",
                        txn_id, self.batch_counter, batch_size
                    );

                    txn_id
                }
            }
        };

        Ok(transaction_id)
    }

    /// Commit a transaction
    pub fn commit_transaction(&mut self, transaction_id: &str) -> Result<()> {
        match self.batch_job.config.transaction_mode {
            TransactionMode::PerFile => {
                // Commit immediately for PerFile mode
                self.commit_transaction_internal(transaction_id)?;
                info!("Committed PerFile transaction {}", transaction_id);
            }

            TransactionMode::AllOrNothing => {
                // Don't commit yet - wait until all files are processed
                debug!(
                    "Deferring commit for AllOrNothing transaction {}",
                    transaction_id
                );
            }

            TransactionMode::Batched { .. } => {
                // Don't commit yet - wait until batch is full or all files processed
                debug!(
                    "Deferring commit for Batched transaction {}",
                    transaction_id
                );
            }
        }

        Ok(())
    }

    /// Internal commit (actually commits the transaction)
    fn commit_transaction_internal(&mut self, transaction_id: &str) -> Result<()> {
        if let Some(txn) = self.transactions.get_mut(transaction_id) {
            if txn.state == TransactionState::Active {
                txn.commit();
                info!(
                    "Committed transaction {} ({} files)",
                    transaction_id,
                    txn.file_ids.len()
                );
            }
            Ok(())
        } else {
            Err(anyhow!("Transaction not found: {}", transaction_id))
        }
    }

    /// Rollback a transaction
    pub fn rollback_transaction(&mut self, transaction_id: &str, error: String) -> Result<()> {
        if let Some(txn) = self.transactions.get_mut(transaction_id) {
            if txn.state == TransactionState::Active {
                txn.rollback(error.clone());
                error!(
                    "Rolled back transaction {} ({} files): {}",
                    transaction_id,
                    txn.file_ids.len(),
                    error
                );
            }
            Ok(())
        } else {
            Err(anyhow!("Transaction not found: {}", transaction_id))
        }
    }

    /// Finalize all transactions (commit or rollback remaining)
    pub fn finalize(&mut self, overall_success: bool) -> Result<TransactionSummary> {
        info!(
            "Finalizing transactions (mode: {:?}, overall_success: {})",
            self.batch_job.config.transaction_mode, overall_success
        );

        match self.batch_job.config.transaction_mode {
            TransactionMode::PerFile => {
                // All transactions already committed or rolled back individually
                Ok(self.get_summary())
            }

            TransactionMode::AllOrNothing => {
                // Commit or rollback the single batch-wide transaction
                if let Some(txn_id) = &self.current_transaction_id.clone() {
                    if overall_success {
                        self.commit_transaction_internal(txn_id)?;
                        info!("Committed AllOrNothing transaction (all files succeeded)");
                    } else {
                        self.rollback_transaction(
                            txn_id,
                            "Batch job had failures - rolling back all changes".to_string(),
                        )?;
                        warn!("Rolled back AllOrNothing transaction (batch had failures)");
                    }
                }
                Ok(self.get_summary())
            }

            TransactionMode::Batched { .. } => {
                // Commit the final batch if it exists and was successful
                if let Some(txn_id) = &self.current_transaction_id.clone() {
                    if overall_success {
                        self.commit_transaction_internal(txn_id)?;
                        info!("Committed final Batched transaction");
                    } else {
                        self.rollback_transaction(txn_id, "Final batch had failures".to_string())?;
                        warn!("Rolled back final Batched transaction");
                    }
                }
                Ok(self.get_summary())
            }
        }
    }

    /// Get transaction summary
    pub fn get_summary(&self) -> TransactionSummary {
        let mut summary = TransactionSummary {
            transaction_mode: self.batch_job.config.transaction_mode,
            total_transactions: self.transactions.len(),
            committed: 0,
            rolled_back: 0,
            transactions: self.transactions.values().cloned().collect(),
        };

        for txn in self.transactions.values() {
            match txn.state {
                TransactionState::Committed => summary.committed += 1,
                TransactionState::RolledBack => summary.rolled_back += 1,
                _ => {}
            }
        }

        summary
    }

    /// Get transaction info by ID
    pub fn get_transaction(&self, transaction_id: &str) -> Option<&TransactionInfo> {
        self.transactions.get(transaction_id)
    }
}

/// Transaction summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionSummary {
    pub transaction_mode: TransactionMode,
    pub total_transactions: usize,
    pub committed: usize,
    pub rolled_back: usize,
    pub transactions: Vec<TransactionInfo>,
}

impl TransactionSummary {
    /// Check if all transactions were successful
    pub fn all_committed(&self) -> bool {
        self.total_transactions > 0 && self.committed == self.total_transactions
    }

    /// Check if any transactions were rolled back
    pub fn has_rollbacks(&self) -> bool {
        self.rolled_back > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflows::domain::{BatchJobConfig, ResourceLimits};

    fn create_test_batch_job(mode: TransactionMode) -> BatchJob {
        let mut config = BatchJobConfig::default();
        config.transaction_mode = mode;

        BatchJob::new(
            "Test Batch".to_string(),
            "workflow_123".to_string(),
            config,
            "user_1".to_string(),
        )
    }

    #[test]
    fn test_per_file_transaction_mode() {
        let batch_job = create_test_batch_job(TransactionMode::PerFile);
        let mut coordinator = TransactionCoordinator::new(batch_job);

        // Begin transaction for file 1
        let txn1 = coordinator.begin_transaction("file_1").unwrap();
        assert!(txn1.contains("file_1"));

        // Begin transaction for file 2
        let txn2 = coordinator.begin_transaction("file_2").unwrap();
        assert!(txn2.contains("file_2"));

        // Each file should have its own transaction
        assert_ne!(txn1, txn2);

        // Commit file 1
        coordinator.commit_transaction(&txn1).unwrap();

        // Rollback file 2
        coordinator
            .rollback_transaction(&txn2, "Test error".to_string())
            .unwrap();

        let summary = coordinator.finalize(true).unwrap();
        assert_eq!(summary.total_transactions, 2);
        assert_eq!(summary.committed, 1);
        assert_eq!(summary.rolled_back, 1);
    }

    #[test]
    fn test_all_or_nothing_success() {
        let batch_job = create_test_batch_job(TransactionMode::AllOrNothing);
        let mut coordinator = TransactionCoordinator::new(batch_job);

        // Begin transactions for multiple files
        let txn1 = coordinator.begin_transaction("file_1").unwrap();
        let txn2 = coordinator.begin_transaction("file_2").unwrap();
        let txn3 = coordinator.begin_transaction("file_3").unwrap();

        // All should use the same transaction
        assert_eq!(txn1, txn2);
        assert_eq!(txn2, txn3);

        // Commit all (deferred)
        coordinator.commit_transaction(&txn1).unwrap();
        coordinator.commit_transaction(&txn2).unwrap();
        coordinator.commit_transaction(&txn3).unwrap();

        // Finalize with success - should commit all
        let summary = coordinator.finalize(true).unwrap();
        assert_eq!(summary.total_transactions, 1);
        assert_eq!(summary.committed, 1);
        assert_eq!(summary.rolled_back, 0);
    }

    #[test]
    fn test_all_or_nothing_failure() {
        let batch_job = create_test_batch_job(TransactionMode::AllOrNothing);
        let mut coordinator = TransactionCoordinator::new(batch_job);

        let txn1 = coordinator.begin_transaction("file_1").unwrap();
        let txn2 = coordinator.begin_transaction("file_2").unwrap();

        // Finalize with failure - should rollback all
        let summary = coordinator.finalize(false).unwrap();
        assert_eq!(summary.total_transactions, 1);
        assert_eq!(summary.committed, 0);
        assert_eq!(summary.rolled_back, 1);
    }

    #[test]
    fn test_batched_transaction_mode() {
        let batch_job = create_test_batch_job(TransactionMode::Batched { batch_size: 2 });
        let mut coordinator = TransactionCoordinator::new(batch_job);

        // File 1 - starts batch 0
        let txn1 = coordinator.begin_transaction("file_1").unwrap();
        assert!(txn1.contains("batch_0"));

        // File 2 - same batch
        let txn2 = coordinator.begin_transaction("file_2").unwrap();
        assert_eq!(txn1, txn2);

        // File 3 - starts batch 1 (batch_size reached)
        let txn3 = coordinator.begin_transaction("file_3").unwrap();
        assert!(txn3.contains("batch_1"));
        assert_ne!(txn1, txn3);

        // File 4 - same batch as file 3
        let txn4 = coordinator.begin_transaction("file_4").unwrap();
        assert_eq!(txn3, txn4);

        let summary = coordinator.finalize(true).unwrap();
        assert_eq!(summary.total_transactions, 2); // 2 batches
    }

    #[test]
    fn test_transaction_state_transitions() {
        let batch_job = create_test_batch_job(TransactionMode::PerFile);
        let mut coordinator = TransactionCoordinator::new(batch_job);

        let txn_id = coordinator.begin_transaction("file_1").unwrap();

        // Check initial state
        let txn = coordinator.get_transaction(&txn_id).unwrap();
        assert_eq!(txn.state, TransactionState::Active);
        assert_eq!(txn.file_ids.len(), 1);

        // Commit
        coordinator.commit_transaction(&txn_id).unwrap();
        let txn = coordinator.get_transaction(&txn_id).unwrap();
        assert_eq!(txn.state, TransactionState::Committed);
        assert!(txn.ended_at.is_some());
    }

    #[test]
    fn test_transaction_summary() {
        let batch_job = create_test_batch_job(TransactionMode::PerFile);
        let mut coordinator = TransactionCoordinator::new(batch_job);

        let txn1 = coordinator.begin_transaction("file_1").unwrap();
        let txn2 = coordinator.begin_transaction("file_2").unwrap();
        let txn3 = coordinator.begin_transaction("file_3").unwrap();

        coordinator.commit_transaction(&txn1).unwrap();
        coordinator.commit_transaction(&txn2).unwrap();
        coordinator
            .rollback_transaction(&txn3, "Error".to_string())
            .unwrap();

        let summary = coordinator.get_summary();
        assert_eq!(summary.total_transactions, 3);
        assert_eq!(summary.committed, 2);
        assert_eq!(summary.rolled_back, 1);
        assert!(!summary.all_committed());
        assert!(summary.has_rollbacks());
    }
}
