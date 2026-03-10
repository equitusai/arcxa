// src/governance/bitemporal/mod.rs
//
// Bitemporal MVCC (Multi-Version Concurrency Control) for Graphica RDF-star triples.
//
// This module provides temporal versioning capabilities combining:
// - **Transaction Time (txFrom/txTo)**: When the system learned about the data
// - **Valid Time (validFrom/validTo)**: When the data was actually true in the real world
//
// Architecture:
// - `TransactionManager`: Generates monotonic transaction IDs with timestamps
// - `BitemporalAnnotations`: Core temporal metadata attached to RDF-star triples
// - `VersionManager`: Handles version superseding when new data arrives
// - `MVCCQueryExecutor`: Executes point-in-time queries
//
// Design Principles:
// 1. **Hybrid Transaction ID**: Combines monotonic sequence with wall clock for ordering + readability
// 2. **Two-Phase Annotation**: Valid time from source data, transaction time at storage
// 3. **Reversible Operations**: All versions retained, queries select appropriate version
// 4. **Backward Compatible**: Existing triples without temporal annotations still queryable

pub mod annotations;
pub mod indexes;
pub mod maintenance;
pub mod metrics;
pub mod query_executor;
pub mod transaction_manager;
pub mod version_manager;
pub mod wal;

// Enterprise features (Phase 1 - Foundation)
#[cfg(feature = "raft-consensus")]
pub mod consensus;

#[cfg(feature = "cryptographic-audit")]
pub mod audit;

// Re-export core types for convenience
pub use annotations::{BitemporalAnnotations, TransactionId};
pub use indexes::{
    IndexStatistics, LongChain, TemporalIndexHealth, TemporalIndexes, TripleMetadata,
    VersionChainAnalysis, VersionRef,
};
pub use maintenance::{MaintenanceConfig, MaintenanceScheduler};
pub use query_executor::{AuditEntry, MVCCQueryExecutor};
pub use transaction_manager::TransactionManager;
pub use version_manager::{ExistingVersion, VersionManager};
pub use wal::{WalEntry, WalOperation, WalStatistics, WriteAheadLog};

// Re-export cryptographic audit types (gated by feature flag)
#[cfg(feature = "cryptographic-audit")]
pub use audit::{
    AuditChain, AuditEntry as CryptoAuditEntry, AuditOperation, AuditStore, BatchProof,
    ChainVerifier, Hash, MerkleProof, MerkleTree, SignedAuditEntry, StoreStatistics,
    VerificationError, VerificationResult,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_exports() {
        // Verify all core types are exported
        let _mgr = TransactionManager::new(1);
        let tx_id = _mgr.begin_transaction();
        assert_eq!(tx_id.node_id, 1);
    }
}
