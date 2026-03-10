//! Cryptographic audit trail for bitemporal operations.
//!
//! Provides tamper-proof audit logging using:
//! - Ed25519 digital signatures
//! - SHA-256 hash chaining
//! - Merkle trees for efficient batch verification (Week 7)
//!
//! ## Compliance
//!
//! Designed to meet SOX, HIPAA, and GDPR audit requirements by providing:
//! - Tamper-evident audit trail
//! - Cryptographic proof of data integrity
//! - Non-repudiation through digital signatures
//! - Chain of custody verification
//!
//! ## Feature Flag
//!
//! This module is gated behind the `cryptographic-audit` feature flag.
//!
//! ## Architecture
//!
//! ```text
//! AuditEntry
//!     ↓
//! SignedAuditEntry (Ed25519 signature + prev hash)
//!     ↓
//! AuditChain (append-only with verification)
//!     ↓
//! ChainVerifier (integrity checks)
//! ```
//!
//! ## Implementation Status
//!
//! - [x] Week 5: Basic structure, hashing, signatures
//! - [x] Week 6: RocksDB persistence and enhanced verification
//! - [x] Week 7: Merkle tree batch verification
//! - [ ] Week 8: Optional blockchain anchoring

#[cfg(feature = "cryptographic-audit")]
pub mod chain;
#[cfg(feature = "cryptographic-audit")]
pub mod crypto;
#[cfg(feature = "cryptographic-audit")]
pub mod entry;
#[cfg(feature = "cryptographic-audit")]
pub mod store;
#[cfg(feature = "cryptographic-audit")]
pub mod verifier;

// Week 7 implementation
#[cfg(feature = "cryptographic-audit")]
pub mod merkle;

#[cfg(feature = "cryptographic-audit")]
pub use chain::AuditChain;
#[cfg(feature = "cryptographic-audit")]
pub use crypto::Hash;
#[cfg(feature = "cryptographic-audit")]
pub use entry::{AuditEntry, AuditOperation, SignedAuditEntry};
#[cfg(feature = "cryptographic-audit")]
pub use merkle::{BatchProof, MerkleProof, MerkleTree};
#[cfg(feature = "cryptographic-audit")]
pub use store::{AuditStore, StoreStatistics};
#[cfg(feature = "cryptographic-audit")]
pub use verifier::{ChainVerifier, VerificationError, VerificationResult};

#[cfg(test)]
mod tests;
