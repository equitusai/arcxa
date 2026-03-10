// src/governance/bitemporal/mod.rs
//
// **BACKWARD COMPATIBILITY BRIDGE**
//
// This module has been consolidated with the root-level bitemporal module.
// All types and functionality are now re-exported from `crate::bitemporal`.
//
// **DEPRECATED**: This module location is deprecated as of version 0.2.0.
// Please update your imports to use `crate::bitemporal` instead.
//
// Original documentation:
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

// Re-export all modules from canonical location
pub use crate::bitemporal::annotations;
pub use crate::bitemporal::indexes;
pub use crate::bitemporal::maintenance;
pub use crate::bitemporal::metrics;
pub use crate::bitemporal::query_executor;
pub use crate::bitemporal::transaction_manager;
pub use crate::bitemporal::version_manager;
pub use crate::bitemporal::wal;

// Re-export core types for convenience
pub use crate::bitemporal::{
    AuditEntry, BitemporalAnnotations, ExistingVersion, IndexStatistics, LongChain,
    MVCCQueryExecutor, MaintenanceConfig, MaintenanceScheduler, TemporalIndexHealth,
    TemporalIndexes, TransactionId, TransactionManager, TripleMetadata, VersionChainAnalysis,
    VersionManager, VersionRef, WalEntry, WalOperation, WalStatistics, WriteAheadLog,
};

// Re-export enterprise features if available
#[cfg(feature = "raft-consensus")]
pub use crate::bitemporal::consensus;

#[cfg(feature = "cryptographic-audit")]
pub use crate::bitemporal::audit;

#[cfg(feature = "cryptographic-audit")]
pub use crate::bitemporal::{
    AuditChain, AuditOperation, AuditStore, BatchProof, ChainVerifier, Hash, MerkleProof,
    MerkleTree, SignedAuditEntry, StoreStatistics, VerificationError, VerificationResult,
};

#[deprecated(
    since = "0.2.0",
    note = "Use crate::bitemporal instead of crate::governance::bitemporal"
)]
pub use crate::bitemporal as canonical_bitemporal;

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
