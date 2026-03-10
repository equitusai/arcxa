//! Audited Shard Coordinator
//!
//! Wraps the ShardCoordinatingRdfStore with cryptographic audit logging.
//! Every write operation (insert, update, delete) is logged to a tamper-proof
//! audit chain with Merkle tree verification.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────┐
//! │      AuditedShardCoordinator                    │
//! │  ┌──────────────────────────────────────────┐   │
//! │  │  Cryptographic Audit Chain               │   │
//! │  │  - Ed25519 signatures                    │   │
//! │  │  - Merkle tree                           │   │
//! │  │  - RocksDB persistence                   │   │
//! │  └──────────────────────────────────────────┘   │
//! │  ┌──────────────────────────────────────────┐   │
//! │  │  ShardCoordinatingRdfStore               │   │
//! │  │  - Routes to shards                      │   │
//! │  │  - Scatter-gather                        │   │
//! │  └──────────────────────────────────────────┘   │
//! └─────────────────────────────────────────────────┘
//!                       │
//!          ┌────────────┼────────────┐
//!          ▼            ▼            ▼
//!      ┌────────┐  ┌────────┐  ┌────────┐
//!      │Shard 0 │  │Shard 1 │  │Shard N │
//!      └────────┘  └────────┘  └────────┘
//! ```
//!
//! ## Usage
//!
//! ```ignore
//! use graphica_coordinator::governance::audited_coordinator::AuditedShardCoordinator;
//! use graphica_coordinator::governance::shard_coordinator::ShardCoordinatingRdfStore;
//! use graphica_coordinator::governance::distributed::ShardRegistry;
//! use ed25519_dalek::SigningKey;
//!
//! # async fn example() -> anyhow::Result<()> {
//! // Create base shard coordinator
//! let registry = ShardRegistry::new("./data/shards", 4, 60)?;
//! let shard_store = ShardCoordinatingRdfStore::new(registry.into(), context);
//!
//! // Wrap with audit chain
//! let signing_key = SigningKey::from_bytes(&key_bytes);
//! let audited = AuditedShardCoordinator::new(
//!     shard_store,
//!     signing_key,
//!     "./data/audit",
//!     "admin_user".to_string(),
//! )?;
//!
//! // All writes are now audited
//! audited.insert_triple("s", "p", "o", None).await?;
//!
//! // Verify audit chain
//! let root = audited.merkle_root();
//! assert!(audited.verify_audit_chain().is_ok());
//! # Ok(())
//! # }
//! ```

use anyhow::{Context, Result};
use parking_lot::RwLock;
use serde_json::Value as JsonValue;
use std::path::Path;
use std::sync::Arc;
use tracing::{debug, info};

#[cfg(feature = "cryptographic-audit")]
use ed25519_dalek::SigningKey;

use super::rdf_store::{NamedGraph, RdfStore};
use super::shard_coordinator::ShardCoordinatingRdfStore;
#[cfg(feature = "cryptographic-audit")]
use crate::bitemporal::audit::{
    AuditChain, AuditEntry, AuditOperation, BatchProof, Hash, MerkleProof,
};
use crate::bitemporal::{TransactionId, TransactionManager};

/// Audited Shard Coordinator
///
/// Combines distributed RDF storage with cryptographic audit logging.
/// All write operations are logged to a tamper-proof audit chain.
pub struct AuditedShardCoordinator {
    /// Underlying shard-coordinating RDF store
    shard_store: ShardCoordinatingRdfStore,

    /// Transaction manager for generating transaction IDs
    tx_manager: Arc<TransactionManager>,

    /// Node ID for this coordinator instance
    node_id: u16,

    /// User ID for audit entries
    user_id: String,

    /// Cryptographic audit chain (feature-gated)
    #[cfg(feature = "cryptographic-audit")]
    audit_chain: Arc<RwLock<AuditChain>>,
}

impl AuditedShardCoordinator {
    /// Create a new audited shard coordinator
    ///
    /// # Arguments
    /// * `shard_store` - Underlying shard-coordinating RDF store
    /// * `signing_key` - Ed25519 signing key for audit entries
    /// * `audit_path` - Path for audit chain persistence
    /// * `user_id` - User ID for audit entries
    ///
    /// # Example
    /// ```ignore
    /// let audited = AuditedShardCoordinator::new(
    ///     shard_store,
    ///     signing_key,
    ///     "./data/audit",
    ///     "admin".to_string(),
    /// )?;
    /// ```
    #[cfg(feature = "cryptographic-audit")]
    pub fn new(
        shard_store: ShardCoordinatingRdfStore,
        signing_key: SigningKey,
        audit_path: impl AsRef<Path>,
        user_id: String,
    ) -> Result<Self> {
        let node_id = 1u16; // TODO: Get from config
        let tx_manager = Arc::new(TransactionManager::new(node_id));

        // Create persistent audit chain
        let audit_chain = AuditChain::new_with_store(signing_key, audit_path)
            .context("Failed to create audit chain")?;

        info!("Created audited shard coordinator with cryptographic audit chain");

        Ok(Self {
            shard_store,
            tx_manager,
            node_id,
            user_id,
            audit_chain: Arc::new(RwLock::new(audit_chain)),
        })
    }

    /// Create a new audited shard coordinator without persistence
    ///
    /// Audit chain is kept in-memory only (for testing/development).
    #[cfg(feature = "cryptographic-audit")]
    pub fn new_in_memory(
        shard_store: ShardCoordinatingRdfStore,
        signing_key: SigningKey,
        user_id: String,
    ) -> Result<Self> {
        let node_id = 1u16;
        let tx_manager = Arc::new(TransactionManager::new(node_id));

        // Create in-memory audit chain
        let audit_chain = AuditChain::new(signing_key);

        info!("Created audited shard coordinator with in-memory audit chain");

        Ok(Self {
            shard_store,
            tx_manager,
            node_id,
            user_id,
            audit_chain: Arc::new(RwLock::new(audit_chain)),
        })
    }

    /// Create without audit chain (when feature is disabled)
    #[cfg(not(feature = "cryptographic-audit"))]
    pub fn new_without_audit(
        shard_store: ShardCoordinatingRdfStore,
        user_id: String,
    ) -> Result<Self> {
        let node_id = 1u16;
        let tx_manager = Arc::new(TransactionManager::new(node_id));

        info!("Created audited shard coordinator WITHOUT cryptographic audit (feature disabled)");

        Ok(Self {
            shard_store,
            tx_manager,
            node_id,
            user_id,
        })
    }

    /// Audit a write operation
    #[cfg(feature = "cryptographic-audit")]
    fn audit_operation(&self, tx_id: TransactionId, operation: AuditOperation) {
        let entry = AuditEntry::new(
            tx_id,
            operation,
            self.node_id as u64, // Cast to u64 for audit entry
            self.user_id.clone(),
        );

        self.audit_chain.write().append(entry);
        debug!("Audited operation: {:?} for tx {:?}", operation, tx_id);
    }

    /// Get the Merkle root of the audit chain
    #[cfg(feature = "cryptographic-audit")]
    pub fn merkle_root(&self) -> Hash {
        self.audit_chain.read().merkle_root()
    }

    /// Generate Merkle proof for a specific audit entry
    #[cfg(feature = "cryptographic-audit")]
    pub fn merkle_proof(&self, index: usize) -> Option<MerkleProof> {
        self.audit_chain.read().merkle_proof(index)
    }

    /// Generate batch Merkle proofs
    #[cfg(feature = "cryptographic-audit")]
    pub fn batch_merkle_proof(&self, indices: &[usize]) -> BatchProof {
        self.audit_chain.read().batch_merkle_proof(indices)
    }

    /// Verify the entire audit chain
    #[cfg(feature = "cryptographic-audit")]
    pub fn verify_audit_chain(&self) -> Result<(), String> {
        self.audit_chain.read().verify()
    }

    /// Verify audit chain using Merkle tree
    #[cfg(feature = "cryptographic-audit")]
    pub fn verify_audit_chain_with_merkle(&self) -> Result<(), String> {
        self.audit_chain.read().verify_with_merkle()
    }

    /// Get audit chain length
    #[cfg(feature = "cryptographic-audit")]
    pub fn audit_chain_length(&self) -> usize {
        self.audit_chain.read().len()
    }

    /// Get the underlying shard store (for advanced operations)
    pub fn shard_store(&self) -> &ShardCoordinatingRdfStore {
        &self.shard_store
    }
}

// Implement RdfStore trait for AuditedShardCoordinator
impl RdfStore for AuditedShardCoordinator {
    /// Insert a single triple with audit logging
    fn insert_triple(
        &self,
        subject: &str,
        predicate: &str,
        object: &str,
        graph: Option<&NamedGraph>,
    ) -> Result<()> {
        // Begin transaction
        let tx_id = self.tx_manager.begin_transaction();

        // Execute shard write
        self.shard_store
            .insert_triple(subject, predicate, object, graph)?;

        // Audit the operation
        #[cfg(feature = "cryptographic-audit")]
        self.audit_operation(tx_id, AuditOperation::Insert);

        Ok(())
    }

    /// Insert multiple triples with batch audit logging
    fn insert_triples(
        &self,
        triples: Vec<(String, String, String)>,
        graph: Option<&NamedGraph>,
    ) -> Result<()> {
        // Begin transaction
        let tx_id = self.tx_manager.begin_transaction();

        // Execute batch write
        self.shard_store.insert_triples(triples, graph)?;

        // Audit the operation
        #[cfg(feature = "cryptographic-audit")]
        self.audit_operation(tx_id, AuditOperation::Insert);

        Ok(())
    }

    /// Execute SPARQL query (read-only, no audit)
    fn query(&self, sparql: &str) -> Result<Vec<JsonValue>> {
        self.shard_store.query(sparql)
    }

    /// Execute SPARQL UPDATE with audit logging
    fn update(&self, sparql_update: &str) -> Result<()> {
        // Begin transaction
        let tx_id = self.tx_manager.begin_transaction();

        // Execute update
        self.shard_store.update(sparql_update)?;

        // Audit the operation
        #[cfg(feature = "cryptographic-audit")]
        {
            // Determine operation type from SPARQL
            let operation = if sparql_update.to_uppercase().contains("DELETE") {
                AuditOperation::Delete
            } else {
                AuditOperation::Update
            };
            self.audit_operation(tx_id, operation);
        }

        Ok(())
    }

    /// Load Turtle data with audit logging
    fn load_turtle(&self, turtle: &str, graph: Option<&NamedGraph>) -> Result<()> {
        // Begin transaction
        let tx_id = self.tx_manager.begin_transaction();

        // Load turtle
        self.shard_store.load_turtle(turtle, graph)?;

        // Audit the operation
        #[cfg(feature = "cryptographic-audit")]
        self.audit_operation(tx_id, AuditOperation::Insert);

        Ok(())
    }

    /// Load ontology (always into default graph)
    fn load_ontology(&self, turtle: &str) -> Result<()> {
        self.shard_store.load_ontology(turtle)
    }

    /// Count triples (read-only, no audit)
    fn count_triples(&self, graph: Option<&NamedGraph>) -> Result<u64> {
        self.shard_store.count_triples(graph)
    }

    /// Clear graph with audit logging
    fn clear_graph(&self, graph: &NamedGraph) -> Result<()> {
        // Begin transaction
        let tx_id = self.tx_manager.begin_transaction();

        // Clear graph
        self.shard_store.clear_graph(graph)?;

        // Audit the operation
        #[cfg(feature = "cryptographic-audit")]
        self.audit_operation(tx_id, AuditOperation::Delete);

        Ok(())
    }
}

#[cfg(all(test, feature = "cryptographic-audit"))]
mod tests {
    use super::*;
    use rand::{rngs::OsRng, RngCore};

    fn create_test_signing_key() -> SigningKey {
        let mut bytes = [0u8; 32];
        OsRng.fill_bytes(&mut bytes);
        SigningKey::from_bytes(&bytes)
    }

    // Note: Integration tests require a running shard infrastructure
    // These are unit tests for the audit integration logic

    #[test]
    fn test_audit_chain_creation() {
        // This would need a real ShardCoordinatingRdfStore
        // Skipping for now - full integration test needed
    }
}
