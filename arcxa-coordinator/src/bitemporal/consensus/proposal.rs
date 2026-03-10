//! Transaction proposals for Raft consensus.
//!
//! Proposals represent operations that need distributed consensus before
//! being applied to the transaction state machine.

use serde::{Deserialize, Serialize};

/// Transaction proposal for Raft consensus.
///
/// Each proposal is submitted to the Raft cluster and, once committed by
/// a quorum of nodes, is applied to the transaction state machine.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TransactionProposal {
    /// Allocate a new transaction ID
    ///
    /// This is the most common proposal type, used to generate a new
    /// transaction ID with distributed consensus to ensure uniqueness.
    AllocateId,

    /// Begin a transaction with specific isolation level
    ///
    /// Used in Phase 2 when isolation levels are implemented.
    BeginTransaction {
        /// The desired isolation level for this transaction
        isolation: IsolationLevel,
    },

    /// Commit a transaction
    ///
    /// Records that a transaction has been committed.
    CommitTransaction {
        /// The transaction ID being committed
        tx_id: u64,
    },

    /// Abort a transaction
    ///
    /// Records that a transaction has been aborted.
    AbortTransaction {
        /// The transaction ID being aborted
        tx_id: u64,
    },
}

/// Transaction isolation levels.
///
/// These will be fully implemented in Phase 2 (SSI).
/// For Phase 1, we only support ReadCommitted.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum IsolationLevel {
    /// Read committed isolation level
    ///
    /// Currently supported. Prevents dirty reads but allows
    /// non-repeatable reads and phantoms.
    ReadCommitted,

    /// Repeatable read isolation level
    ///
    /// Phase 2 implementation. Prevents dirty reads and
    /// non-repeatable reads, but allows phantoms.
    RepeatableRead,

    /// Serializable isolation level
    ///
    /// Phase 2 implementation (SSI). Full isolation with
    /// no anomalies allowed.
    Serializable,
}

impl Default for IsolationLevel {
    fn default() -> Self {
        IsolationLevel::ReadCommitted
    }
}

impl TransactionProposal {
    /// Create a proposal to allocate a new transaction ID
    pub fn allocate_id() -> Self {
        TransactionProposal::AllocateId
    }

    /// Create a proposal to begin a transaction
    pub fn begin(isolation: IsolationLevel) -> Self {
        TransactionProposal::BeginTransaction { isolation }
    }

    /// Create a proposal to commit a transaction
    pub fn commit(tx_id: u64) -> Self {
        TransactionProposal::CommitTransaction { tx_id }
    }

    /// Create a proposal to abort a transaction
    pub fn abort(tx_id: u64) -> Self {
        TransactionProposal::AbortTransaction { tx_id }
    }

    /// Serialize the proposal to bytes for Raft submission
    pub fn to_bytes(&self) -> Result<Vec<u8>, bincode::Error> {
        bincode::serialize(self)
    }

    /// Deserialize a proposal from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, bincode::Error> {
        bincode::deserialize(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proposal_allocate_id() {
        let proposal = TransactionProposal::allocate_id();
        assert_eq!(proposal, TransactionProposal::AllocateId);
    }

    #[test]
    fn test_proposal_begin() {
        let proposal = TransactionProposal::begin(IsolationLevel::Serializable);
        assert_eq!(
            proposal,
            TransactionProposal::BeginTransaction {
                isolation: IsolationLevel::Serializable
            }
        );
    }

    #[test]
    fn test_proposal_commit() {
        let proposal = TransactionProposal::commit(42);
        assert_eq!(
            proposal,
            TransactionProposal::CommitTransaction { tx_id: 42 }
        );
    }

    #[test]
    fn test_proposal_abort() {
        let proposal = TransactionProposal::abort(99);
        assert_eq!(
            proposal,
            TransactionProposal::AbortTransaction { tx_id: 99 }
        );
    }

    #[test]
    fn test_proposal_serialization() {
        let original = TransactionProposal::allocate_id();
        let bytes = original.to_bytes().unwrap();
        let deserialized = TransactionProposal::from_bytes(&bytes).unwrap();
        assert_eq!(original, deserialized);
    }

    #[test]
    fn test_proposal_serialization_with_data() {
        let original = TransactionProposal::begin(IsolationLevel::RepeatableRead);
        let bytes = original.to_bytes().unwrap();
        let deserialized = TransactionProposal::from_bytes(&bytes).unwrap();
        assert_eq!(original, deserialized);
    }

    #[test]
    fn test_isolation_level_default() {
        let level = IsolationLevel::default();
        assert_eq!(level, IsolationLevel::ReadCommitted);
    }

    #[test]
    fn test_isolation_levels_equality() {
        assert_eq!(IsolationLevel::ReadCommitted, IsolationLevel::ReadCommitted);
        assert_ne!(IsolationLevel::ReadCommitted, IsolationLevel::Serializable);
    }
}
