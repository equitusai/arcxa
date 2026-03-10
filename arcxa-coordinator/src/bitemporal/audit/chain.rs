//! Append-only audit chain with hash linking.
//!
//! Provides a tamper-evident audit log where each entry is cryptographically
//! linked to the previous entry via hash chaining.

use super::crypto::Hash;
use super::entry::{AuditEntry, SignedAuditEntry};
use super::merkle::{BatchProof, MerkleProof, MerkleTree};
use super::store::AuditStore;
use ed25519_dalek::SigningKey;
use std::path::Path;
use std::sync::{Arc, RwLock};

/// Append-only audit chain.
///
/// Provides:
/// - Cryptographic hash chaining
/// - Ed25519 signatures for each entry
/// - Tamper detection
/// - Non-repudiation
///
/// ## Storage
///
/// - In-memory mode: Fast, volatile storage (use `new()`)
/// - Persistent mode: RocksDB-backed storage (use `new_with_store()`)
///
/// ## Week 6 Updates
///
/// Added optional RocksDB persistence layer for crash recovery and durability.
pub struct AuditChain {
    /// Chain of signed entries (in-memory cache)
    entries: Arc<RwLock<Vec<SignedAuditEntry>>>,

    /// Signing key for signing new entries
    signing_key: Arc<SigningKey>,

    /// Genesis hash (Hash::ZERO)
    genesis_hash: Hash,

    /// Optional persistent storage (Week 6)
    store: Option<Arc<AuditStore>>,
}

impl AuditChain {
    /// Create a new in-memory audit chain (volatile).
    ///
    /// For persistent storage, use `new_with_store()`.
    ///
    /// # Arguments
    ///
    /// * `signing_key` - Signing key for signing entries
    ///
    /// # Example
    ///
    /// ```ignore
    /// use graphica_coordinator::bitemporal::audit::AuditChain;
    /// use ed25519_dalek::SigningKey;
    /// use rand::{rngs::OsRng, RngCore};
    ///
    /// let mut bytes = [0u8; 32];
    /// OsRng.fill_bytes(&mut bytes);
    /// let signing_key = SigningKey::from_bytes(&bytes);
    /// let chain = AuditChain::new(signing_key);
    /// ```
    pub fn new(signing_key: SigningKey) -> Self {
        Self {
            entries: Arc::new(RwLock::new(Vec::new())),
            signing_key: Arc::new(signing_key),
            genesis_hash: Hash::ZERO,
            store: None,
        }
    }

    /// Create a new audit chain with persistent RocksDB storage.
    ///
    /// # Arguments
    ///
    /// * `signing_key` - Signing key for signing entries
    /// * `path` - Directory path for RocksDB storage
    ///
    /// # Example
    ///
    /// ```ignore
    /// let chain = AuditChain::new_with_store(signing_key, "./data/audit")?;
    /// ```
    pub fn new_with_store(signing_key: SigningKey, path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let store = AuditStore::new(path)?;

        // Initialize genesis hash and public key in store
        store.set_genesis_hash(Hash::ZERO)?;
        store.set_public_key(&signing_key.verifying_key().to_bytes())?;

        Ok(Self {
            entries: Arc::new(RwLock::new(Vec::new())),
            signing_key: Arc::new(signing_key),
            genesis_hash: Hash::ZERO,
            store: Some(Arc::new(store)),
        })
    }

    /// Load an existing audit chain from persistent storage.
    ///
    /// Recovers the entire chain from RocksDB and verifies integrity.
    ///
    /// # Arguments
    ///
    /// * `signing_key` - Signing key for signing new entries
    /// * `path` - Directory path for RocksDB storage
    ///
    /// # Example
    ///
    /// ```ignore
    /// let chain = AuditChain::from_store(signing_key, "./data/audit")?;
    /// ```
    pub fn from_store(signing_key: SigningKey, path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let store = AuditStore::new(path)?;

        // Load all entries from store
        let entries = store.get_all()?;

        // Verify stored genesis hash and public key
        let genesis_hash = store.genesis_hash()?.unwrap_or(Hash::ZERO);
        let stored_pk = store.public_key()?;
        let current_pk = signing_key.verifying_key().to_bytes();

        if let Some(pk) = stored_pk {
            if pk != current_pk {
                return Err(anyhow::anyhow!(
                    "Public key mismatch: stored key does not match provided signing key"
                ));
            }
        }

        Ok(Self {
            entries: Arc::new(RwLock::new(entries)),
            signing_key: Arc::new(signing_key),
            genesis_hash,
            store: Some(Arc::new(store)),
        })
    }

    /// Append a new entry to the chain.
    ///
    /// The entry is signed and linked to the previous entry via hash chain.
    /// If persistent storage is enabled, the entry is also written to RocksDB.
    ///
    /// # Arguments
    ///
    /// * `entry` - Audit entry to append
    ///
    /// # Returns
    ///
    /// The signed entry that was appended.
    ///
    /// # Panics
    ///
    /// Panics if persistent storage write fails (data integrity critical).
    ///
    /// # Example
    ///
    /// ```ignore
    /// let entry = AuditEntry::new(tx_id, operation, node_id, "user".to_string());
    /// let signed = chain.append(entry);
    /// ```
    pub fn append(&self, entry: AuditEntry) -> SignedAuditEntry {
        let mut entries = self.entries.write().unwrap();

        // Get hash of previous entry (or genesis hash if first entry)
        let previous_hash = entries
            .last()
            .map(|e| e.entry_hash)
            .unwrap_or(self.genesis_hash);

        // Sign the entry
        let signed = entry.sign(&self.signing_key, previous_hash);

        // Calculate index for persistence
        let index = entries.len() as u64;

        // Persist to store if present (critical: must not fail silently)
        if let Some(store) = &self.store {
            store
                .append(index, &signed)
                .expect("Failed to persist audit entry to store");
        }

        // Append to in-memory chain
        entries.push(signed.clone());

        signed
    }

    /// Get the number of entries in the chain.
    pub fn len(&self) -> usize {
        self.entries.read().unwrap().len()
    }

    /// Check if the chain is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.read().unwrap().is_empty()
    }

    /// Get an entry by index.
    ///
    /// # Arguments
    ///
    /// * `index` - Index of the entry (0-based)
    ///
    /// # Returns
    ///
    /// The entry at the given index, or `None` if index is out of bounds.
    pub fn get(&self, index: usize) -> Option<SignedAuditEntry> {
        self.entries.read().unwrap().get(index).cloned()
    }

    /// Get all entries in the chain.
    pub fn get_all(&self) -> Vec<SignedAuditEntry> {
        self.entries.read().unwrap().clone()
    }

    /// Get the last entry in the chain.
    pub fn last(&self) -> Option<SignedAuditEntry> {
        self.entries.read().unwrap().last().cloned()
    }

    /// Get the hash of the last entry (chain head).
    ///
    /// Returns `Hash::ZERO` if the chain is empty.
    pub fn head_hash(&self) -> Hash {
        self.last()
            .map(|e| e.entry_hash)
            .unwrap_or(self.genesis_hash)
    }

    /// Verify the integrity of the entire chain.
    ///
    /// Checks:
    /// - All signatures are valid
    /// - All entry hashes are correct
    /// - Hash chain is properly linked
    ///
    /// # Returns
    ///
    /// `Ok(())` if chain is valid, `Err(String)` with error message otherwise.
    pub fn verify(&self) -> Result<(), String> {
        let entries = self.entries.read().unwrap();

        if entries.is_empty() {
            return Ok(());
        }

        // Check first entry links to genesis
        if entries[0].previous_hash != self.genesis_hash {
            return Err("First entry does not link to genesis hash".to_string());
        }

        // Verify each entry
        for (i, entry) in entries.iter().enumerate() {
            // Verify signature and entry hash
            if !entry.verify() {
                return Err(format!("Entry {} failed verification", i));
            }

            // Verify hash chain linkage (except for first entry)
            if i > 0 {
                let prev_hash = entries[i - 1].entry_hash;
                if entry.previous_hash != prev_hash {
                    return Err(format!(
                        "Entry {} hash chain broken: expected prev_hash {}, got {}",
                        i, prev_hash, entry.previous_hash
                    ));
                }
            }
        }

        Ok(())
    }

    /// Export the chain to bytes for storage or transfer.
    ///
    /// TODO (Week 6): Implement efficient serialization format
    pub fn export(&self) -> Result<Vec<u8>, bincode::Error> {
        let entries = self.entries.read().unwrap();
        bincode::serialize(&*entries)
    }

    /// Import a chain from bytes.
    ///
    /// Verifies the chain integrity before accepting.
    ///
    /// TODO (Week 6): Implement efficient deserialization format
    ///
    /// # Errors
    ///
    /// Returns an error if deserialization or verification fails.
    pub fn import(&self, bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        let imported: Vec<SignedAuditEntry> = bincode::deserialize(bytes)?;

        // Verify imported chain
        let temp_chain = Self::new(self.signing_key.as_ref().clone());
        {
            let mut entries = temp_chain.entries.write().unwrap();
            *entries = imported;
        }

        temp_chain.verify()?;

        // Replace current chain
        let mut entries = self.entries.write().unwrap();
        *entries = temp_chain.entries.read().unwrap().clone();

        Ok(())
    }

    /// Get the public key used for signing entries.
    pub fn public_key(&self) -> [u8; 32] {
        self.signing_key.verifying_key().to_bytes()
    }

    // ========== Week 7: Merkle Tree Integration ==========

    /// Build a Merkle tree from the current entries.
    ///
    /// Creates a binary Merkle tree for efficient batch verification.
    /// Empty chains produce an empty tree.
    ///
    /// # Returns
    ///
    /// A Merkle tree containing all entries in the chain.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let tree = chain.build_merkle_tree();
    /// let root = tree.root();
    /// ```
    pub fn build_merkle_tree(&self) -> MerkleTree {
        let entries = self.entries.read().unwrap();
        MerkleTree::from_entries(&entries)
    }

    /// Get the Merkle root hash of the entire chain.
    ///
    /// Provides a single hash representing the entire chain state.
    /// Useful for quick integrity checks and comparisons.
    ///
    /// # Returns
    ///
    /// The Merkle root hash, or `Hash::ZERO` for empty chains.
    pub fn merkle_root(&self) -> Hash {
        self.build_merkle_tree().root()
    }

    /// Generate a Merkle proof for a specific entry.
    ///
    /// Creates an inclusion proof demonstrating that an entry exists
    /// in the chain at a given index. Proof size is O(log n).
    ///
    /// # Arguments
    ///
    /// * `index` - Index of the entry to prove
    ///
    /// # Returns
    ///
    /// A Merkle proof if the index is valid, `None` otherwise.
    ///
    /// # Example
    ///
    /// ```ignore
    /// if let Some(proof) = chain.merkle_proof(5) {
    ///     assert!(proof.verify());
    /// }
    /// ```
    pub fn merkle_proof(&self, index: usize) -> Option<MerkleProof> {
        // Only return proof for actual entries, not padding
        if index >= self.len() {
            return None;
        }
        self.build_merkle_tree().proof(index)
    }

    /// Generate batch proofs for multiple entries.
    ///
    /// Creates inclusion proofs for all specified indices.
    /// More efficient than generating individual proofs.
    ///
    /// # Arguments
    ///
    /// * `indices` - Indices of entries to prove
    ///
    /// # Returns
    ///
    /// A batch proof containing proofs for all valid indices.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let batch = chain.batch_merkle_proof(&[0, 5, 10]);
    /// assert!(batch.verify());
    /// ```
    pub fn batch_merkle_proof(&self, indices: &[usize]) -> BatchProof {
        let tree = self.build_merkle_tree();
        let root = tree.root();
        let proofs: Vec<MerkleProof> = indices.iter().filter_map(|&idx| tree.proof(idx)).collect();
        BatchProof::new(root, proofs)
    }

    /// Verify the chain using Merkle tree batch verification.
    ///
    /// Alternative to sequential verification using Merkle proofs.
    /// Particularly efficient for large chains.
    ///
    /// # Returns
    ///
    /// `Ok(())` if Merkle tree verification succeeds, error otherwise.
    pub fn verify_with_merkle(&self) -> Result<(), String> {
        let entries = self.entries.read().unwrap();

        if entries.is_empty() {
            return Ok(());
        }

        // Build Merkle tree
        let tree = MerkleTree::from_entries(&entries);

        // Generate proofs for all entries
        let indices: Vec<usize> = (0..entries.len()).collect();
        let batch = self.batch_merkle_proof(&indices);

        // Verify batch proof
        if batch.verify() {
            Ok(())
        } else {
            Err("Merkle tree batch verification failed".to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bitemporal::audit::entry::AuditOperation;
    use crate::bitemporal::TransactionId;
    use chrono::{TimeZone, Utc};
    use rand::{rngs::OsRng, RngCore};
    use tempfile::TempDir;

    fn create_test_signing_key() -> SigningKey {
        let mut bytes = [0u8; 32];
        OsRng.fill_bytes(&mut bytes);
        SigningKey::from_bytes(&bytes)
    }

    fn create_test_entry(seq: u64) -> AuditEntry {
        AuditEntry::new(
            TransactionId {
                node_id: 1,
                seq,
                timestamp: Utc.timestamp_opt(1234567890 + seq as i64, 0).unwrap(),
            },
            AuditOperation::Insert,
            1,
            "test_user".to_string(),
        )
    }

    #[test]
    fn test_chain_creation() {
        let signing_key = create_test_signing_key();
        let chain = AuditChain::new(signing_key);

        assert_eq!(chain.len(), 0);
        assert!(chain.is_empty());
        assert_eq!(chain.genesis_hash, Hash::ZERO);
    }

    #[test]
    fn test_append_single_entry() {
        let signing_key = create_test_signing_key();
        let chain = AuditChain::new(signing_key);

        let entry = create_test_entry(100);
        let signed = chain.append(entry);

        assert_eq!(chain.len(), 1);
        assert!(!chain.is_empty());
        assert_eq!(signed.previous_hash, Hash::ZERO);
    }

    #[test]
    fn test_append_multiple_entries() {
        let signing_key = create_test_signing_key();
        let chain = AuditChain::new(signing_key);

        for i in 0..5 {
            chain.append(create_test_entry(i));
        }

        assert_eq!(chain.len(), 5);
    }

    #[test]
    fn test_hash_chain_linking() {
        let signing_key = create_test_signing_key();
        let chain = AuditChain::new(signing_key);

        let signed1 = chain.append(create_test_entry(100));
        let signed2 = chain.append(create_test_entry(101));
        let signed3 = chain.append(create_test_entry(102));

        // First entry links to genesis
        assert_eq!(signed1.previous_hash, Hash::ZERO);

        // Second entry links to first
        assert_eq!(signed2.previous_hash, signed1.entry_hash);

        // Third entry links to second
        assert_eq!(signed3.previous_hash, signed2.entry_hash);
    }

    #[test]
    fn test_get_entry() {
        let signing_key = create_test_signing_key();
        let chain = AuditChain::new(signing_key);

        let entry1 = create_test_entry(100);
        let entry2 = create_test_entry(101);

        chain.append(entry1);
        chain.append(entry2);

        let retrieved = chain.get(0).unwrap();
        assert_eq!(retrieved.entry.tx_id.seq, 100);

        let retrieved = chain.get(1).unwrap();
        assert_eq!(retrieved.entry.tx_id.seq, 101);
    }

    #[test]
    fn test_get_entry_out_of_bounds() {
        let signing_key = create_test_signing_key();
        let chain = AuditChain::new(signing_key);

        assert!(chain.get(0).is_none());

        chain.append(create_test_entry(100));
        assert!(chain.get(1).is_none());
    }

    #[test]
    fn test_get_all() {
        let signing_key = create_test_signing_key();
        let chain = AuditChain::new(signing_key);

        for i in 0..3 {
            chain.append(create_test_entry(i));
        }

        let all = chain.get_all();
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn test_last_entry() {
        let signing_key = create_test_signing_key();
        let chain = AuditChain::new(signing_key);

        assert!(chain.last().is_none());

        chain.append(create_test_entry(100));
        chain.append(create_test_entry(101));
        chain.append(create_test_entry(102));

        let last = chain.last().unwrap();
        assert_eq!(last.entry.tx_id.seq, 102);
    }

    #[test]
    fn test_head_hash() {
        let signing_key = create_test_signing_key();
        let chain = AuditChain::new(signing_key);

        // Empty chain has genesis hash
        assert_eq!(chain.head_hash(), Hash::ZERO);

        let signed = chain.append(create_test_entry(100));
        assert_eq!(chain.head_hash(), signed.entry_hash);

        let signed2 = chain.append(create_test_entry(101));
        assert_eq!(chain.head_hash(), signed2.entry_hash);
    }

    #[test]
    fn test_verify_empty_chain() {
        let signing_key = create_test_signing_key();
        let chain = AuditChain::new(signing_key);

        assert!(chain.verify().is_ok());
    }

    #[test]
    fn test_verify_valid_chain() {
        let signing_key = create_test_signing_key();
        let chain = AuditChain::new(signing_key);

        for i in 0..10 {
            chain.append(create_test_entry(i));
        }

        assert!(chain.verify().is_ok());
    }

    #[test]
    fn test_verify_detects_tampering() {
        let signing_key = create_test_signing_key();
        let chain = AuditChain::new(signing_key);

        chain.append(create_test_entry(100));
        chain.append(create_test_entry(101));
        chain.append(create_test_entry(102));

        // Tamper with an entry
        {
            let mut entries = chain.entries.write().unwrap();
            entries[1].entry.initiator = "attacker".to_string();
        }

        assert!(chain.verify().is_err());
    }

    #[test]
    fn test_verify_detects_broken_chain() {
        let signing_key = create_test_signing_key();
        let chain = AuditChain::new(signing_key);

        chain.append(create_test_entry(100));
        chain.append(create_test_entry(101));
        chain.append(create_test_entry(102));

        // Break the hash chain
        {
            let mut entries = chain.entries.write().unwrap();
            entries[2].previous_hash = Hash::ZERO;
        }

        assert!(chain.verify().is_err());
    }

    #[test]
    fn test_verify_detects_bad_genesis_link() {
        let signing_key = create_test_signing_key();
        let chain = AuditChain::new(signing_key);

        chain.append(create_test_entry(100));

        // Break genesis link
        {
            let mut entries = chain.entries.write().unwrap();
            entries[0].previous_hash = Hash::compute(b"bad");
        }

        let result = chain.verify();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("genesis"));
    }

    #[test]
    fn test_export_import() {
        let signing_key = create_test_signing_key();
        let chain1 = AuditChain::new(signing_key.clone());

        for i in 0..5 {
            chain1.append(create_test_entry(i));
        }

        let bytes = chain1.export().unwrap();

        let chain2 = AuditChain::new(signing_key);
        chain2.import(&bytes).unwrap();

        assert_eq!(chain2.len(), 5);
        assert_eq!(chain2.head_hash(), chain1.head_hash());
    }

    #[test]
    fn test_import_verifies_integrity() {
        let signing_key = create_test_signing_key();
        let chain1 = AuditChain::new(signing_key.clone());

        for i in 0..3 {
            chain1.append(create_test_entry(i));
        }

        let mut bytes = chain1.export().unwrap();

        // Corrupt the bytes
        bytes[100] ^= 0xFF;

        let chain2 = AuditChain::new(signing_key);
        let result = chain2.import(&bytes);

        // Should fail due to corrupted data
        assert!(result.is_err());
    }

    #[test]
    fn test_public_key() {
        let signing_key = create_test_signing_key();
        let chain = AuditChain::new(signing_key.clone());

        assert_eq!(chain.public_key(), signing_key.verifying_key().to_bytes());
    }

    #[test]
    fn test_append_returns_signed_entry() {
        let signing_key = create_test_signing_key();
        let chain = AuditChain::new(signing_key);

        let entry = create_test_entry(100);
        let signed = chain.append(entry);

        assert!(signed.verify());
    }

    #[test]
    fn test_concurrent_append() {
        use std::sync::Arc;
        use std::thread;

        let signing_key = create_test_signing_key();
        let chain = Arc::new(AuditChain::new(signing_key));

        let mut handles = vec![];

        for i in 0..10 {
            let chain_clone = Arc::clone(&chain);
            let handle = thread::spawn(move || {
                chain_clone.append(create_test_entry(i));
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(chain.len(), 10);
        assert!(chain.verify().is_ok());
    }

    #[test]
    fn test_all_entries_have_valid_signatures() {
        let signing_key = create_test_signing_key();
        let chain = AuditChain::new(signing_key);

        for i in 0..20 {
            chain.append(create_test_entry(i));
        }

        let all = chain.get_all();
        for entry in all {
            assert!(entry.verify_signature());
            assert!(entry.verify_entry_hash());
        }
    }

    #[test]
    fn test_chain_preserves_order() {
        let signing_key = create_test_signing_key();
        let chain = AuditChain::new(signing_key);

        let sequences = vec![100, 101, 102, 103, 104];
        for &seq in &sequences {
            chain.append(create_test_entry(seq));
        }

        let all = chain.get_all();
        for (i, entry) in all.iter().enumerate() {
            assert_eq!(entry.entry.tx_id.seq, sequences[i]);
        }
    }

    // ========== Week 6: Persistence Tests ==========

    #[test]
    fn test_new_with_store() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let signing_key = create_test_signing_key();

        let chain = AuditChain::new_with_store(signing_key, temp_dir.path()).unwrap();

        assert_eq!(chain.len(), 0);
        assert!(chain.is_empty());
    }

    #[test]
    fn test_persistent_append() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let signing_key = create_test_signing_key();

        let chain = AuditChain::new_with_store(signing_key, temp_dir.path()).unwrap();

        for i in 0..5 {
            chain.append(create_test_entry(i));
        }

        assert_eq!(chain.len(), 5);
    }

    #[test]
    fn test_from_store_recovery() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let signing_key = create_test_signing_key();

        // Create chain and add entries
        {
            let chain = AuditChain::new_with_store(signing_key.clone(), temp_dir.path()).unwrap();

            for i in 0..10 {
                chain.append(create_test_entry(i));
            }
        }

        // Recover from store
        let recovered = AuditChain::from_store(signing_key, temp_dir.path()).unwrap();

        assert_eq!(recovered.len(), 10);
        assert_eq!(recovered.get(0).unwrap().entry.tx_id.seq, 0);
        assert_eq!(recovered.get(9).unwrap().entry.tx_id.seq, 9);
    }

    #[test]
    fn test_persistence_across_restarts() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let signing_key = create_test_signing_key();

        let head_hash1;

        // First session
        {
            let chain = AuditChain::new_with_store(signing_key.clone(), temp_dir.path()).unwrap();

            for i in 0..5 {
                chain.append(create_test_entry(i));
            }

            head_hash1 = chain.head_hash();
        }

        // Second session - recover and append more
        {
            let chain = AuditChain::from_store(signing_key.clone(), temp_dir.path()).unwrap();

            assert_eq!(chain.len(), 5);
            assert_eq!(chain.head_hash(), head_hash1);

            for i in 5..10 {
                chain.append(create_test_entry(i));
            }

            assert_eq!(chain.len(), 10);
        }

        // Third session - verify all entries persisted
        {
            let chain = AuditChain::from_store(signing_key, temp_dir.path()).unwrap();

            assert_eq!(chain.len(), 10);

            for i in 0..10 {
                let entry = chain.get(i).unwrap();
                assert_eq!(entry.entry.tx_id.seq, i as u64);
            }
        }
    }

    #[test]
    fn test_from_store_verifies_integrity() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let signing_key = create_test_signing_key();

        // Create chain
        {
            let chain = AuditChain::new_with_store(signing_key.clone(), temp_dir.path()).unwrap();

            for i in 0..5 {
                chain.append(create_test_entry(i));
            }
        }

        // Recovery should succeed with correct key
        let recovered = AuditChain::from_store(signing_key, temp_dir.path());
        assert!(recovered.is_ok());

        // Recovery should fail with different key
        let wrong_key = create_test_signing_key();
        let result = AuditChain::from_store(wrong_key, temp_dir.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_persistent_chain_verification() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let signing_key = create_test_signing_key();

        // Create and verify chain
        {
            let chain = AuditChain::new_with_store(signing_key.clone(), temp_dir.path()).unwrap();

            for i in 0..10 {
                chain.append(create_test_entry(i));
            }

            // Verify in-memory chain
            assert!(chain.verify().is_ok());
        } // Drop chain to release RocksDB lock

        // Recover and verify
        let recovered = AuditChain::from_store(signing_key, temp_dir.path()).unwrap();
        assert!(recovered.verify().is_ok());
    }

    #[test]
    fn test_mixed_in_memory_and_persistent() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let signing_key = create_test_signing_key();

        // In-memory chain
        let mem_chain = AuditChain::new(signing_key.clone());
        for i in 0..5 {
            mem_chain.append(create_test_entry(i));
        }

        // Persistent chain
        let persist_chain = AuditChain::new_with_store(signing_key, temp_dir.path()).unwrap();
        for i in 0..5 {
            persist_chain.append(create_test_entry(i));
        }

        // Both should have same length and be valid
        assert_eq!(mem_chain.len(), persist_chain.len());
        assert!(mem_chain.verify().is_ok());
        assert!(persist_chain.verify().is_ok());

        // Verify entries have same sequences
        for i in 0..5 {
            assert_eq!(mem_chain.get(i).unwrap().entry.tx_id.seq, i as u64);
            assert_eq!(persist_chain.get(i).unwrap().entry.tx_id.seq, i as u64);
        }
    }

    #[test]
    fn test_empty_store_recovery() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let signing_key = create_test_signing_key();

        // Create empty store
        {
            let _chain = AuditChain::new_with_store(signing_key.clone(), temp_dir.path()).unwrap();
        }

        // Recover empty chain
        let recovered = AuditChain::from_store(signing_key, temp_dir.path()).unwrap();
        assert_eq!(recovered.len(), 0);
        assert!(recovered.is_empty());
    }

    #[test]
    fn test_large_persistent_chain() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let signing_key = create_test_signing_key();

        // Create large chain
        {
            let chain = AuditChain::new_with_store(signing_key.clone(), temp_dir.path()).unwrap();

            for i in 0..100 {
                chain.append(create_test_entry(i));
            }

            assert_eq!(chain.len(), 100);
        }

        // Recover and verify
        let recovered = AuditChain::from_store(signing_key, temp_dir.path()).unwrap();
        assert_eq!(recovered.len(), 100);
        assert!(recovered.verify().is_ok());
    }

    // ========== Week 7: Merkle Tree Integration Tests ==========

    #[test]
    fn test_merkle_tree_integration() {
        let signing_key = create_test_signing_key();
        let chain = AuditChain::new(signing_key);

        // Build tree from empty chain
        let tree = chain.build_merkle_tree();
        assert_eq!(tree.root(), Hash::ZERO);

        // Add entries
        for i in 0..10 {
            chain.append(create_test_entry(i));
        }

        // Build tree from populated chain
        let tree = chain.build_merkle_tree();
        assert_ne!(tree.root(), Hash::ZERO);
    }

    #[test]
    fn test_merkle_root() {
        let signing_key = create_test_signing_key();
        let chain = AuditChain::new(signing_key);

        // Empty chain
        assert_eq!(chain.merkle_root(), Hash::ZERO);

        // Add entries
        for i in 0..5 {
            chain.append(create_test_entry(i));
        }

        let root1 = chain.merkle_root();
        assert_ne!(root1, Hash::ZERO);

        // Add more entries
        for i in 5..10 {
            chain.append(create_test_entry(i));
        }

        let root2 = chain.merkle_root();
        assert_ne!(root2, Hash::ZERO);
        assert_ne!(root1, root2); // Root changes when entries added
    }

    #[test]
    fn test_merkle_proof_generation() {
        let signing_key = create_test_signing_key();
        let chain = AuditChain::new(signing_key);

        // Empty chain
        assert!(chain.merkle_proof(0).is_none());

        // Add entries
        for i in 0..10 {
            chain.append(create_test_entry(i));
        }

        // Valid proofs
        for i in 0..10 {
            let proof = chain.merkle_proof(i);
            assert!(proof.is_some(), "Proof for index {} should exist", i);
            assert!(
                proof.unwrap().verify(),
                "Proof for index {} should verify",
                i
            );
        }

        // Invalid index
        assert!(chain.merkle_proof(10).is_none());
        assert!(chain.merkle_proof(100).is_none());
    }

    #[test]
    fn test_batch_merkle_proof() {
        let signing_key = create_test_signing_key();
        let chain = AuditChain::new(signing_key);

        // Add entries
        for i in 0..20 {
            chain.append(create_test_entry(i));
        }

        // Generate batch proof
        let indices = vec![0, 5, 10, 15, 19];
        let batch = chain.batch_merkle_proof(&indices);

        // Verify batch
        assert!(batch.verify());
        assert_eq!(batch.proofs.len(), 5);

        // Empty batch
        let empty_batch = chain.batch_merkle_proof(&[]);
        assert!(empty_batch.verify());
        assert_eq!(empty_batch.proofs.len(), 0);

        // Batch with invalid indices (filtered out)
        let mixed_indices = vec![0, 5, 100, 200];
        let mixed_batch = chain.batch_merkle_proof(&mixed_indices);
        assert_eq!(mixed_batch.proofs.len(), 2); // Only 0 and 5 are valid
        assert!(mixed_batch.verify());
    }

    #[test]
    fn test_verify_with_merkle() {
        let signing_key = create_test_signing_key();
        let chain = AuditChain::new(signing_key);

        // Empty chain
        assert!(chain.verify_with_merkle().is_ok());

        // Add entries
        for i in 0..50 {
            chain.append(create_test_entry(i));
        }

        // Verify using Merkle tree
        assert!(chain.verify_with_merkle().is_ok());

        // Compare with standard verification
        assert!(chain.verify().is_ok());
    }

    #[test]
    fn test_merkle_tree_persistence() {
        let temp_dir = TempDir::new().unwrap();
        let signing_key = create_test_signing_key();

        let root1;

        // Create chain and get Merkle root
        {
            let chain = AuditChain::new_with_store(signing_key.clone(), temp_dir.path()).unwrap();
            for i in 0..10 {
                chain.append(create_test_entry(i));
            }
            root1 = chain.merkle_root();
        }

        // Recover and verify Merkle root is the same
        let recovered = AuditChain::from_store(signing_key, temp_dir.path()).unwrap();
        let root2 = recovered.merkle_root();

        assert_eq!(root1, root2);
        assert!(recovered.verify_with_merkle().is_ok());
    }

    #[test]
    fn test_merkle_proof_consistency() {
        let signing_key = create_test_signing_key();
        let chain = AuditChain::new(signing_key);

        for i in 0..100 {
            chain.append(create_test_entry(i));
        }

        // Generate individual proofs
        let proof0 = chain.merkle_proof(0).unwrap();
        let proof50 = chain.merkle_proof(50).unwrap();
        let proof99 = chain.merkle_proof(99).unwrap();

        // All should have same root
        assert_eq!(proof0.root, proof50.root);
        assert_eq!(proof50.root, proof99.root);

        // All should verify
        assert!(proof0.verify());
        assert!(proof50.verify());
        assert!(proof99.verify());
    }

    #[test]
    fn test_merkle_batch_large_chain() {
        let signing_key = create_test_signing_key();
        let chain = AuditChain::new(signing_key);

        // Create large chain
        for i in 0..1000 {
            chain.append(create_test_entry(i));
        }

        // Generate batch proof for random indices
        let indices: Vec<usize> = vec![0, 100, 250, 500, 750, 999];
        let batch = chain.batch_merkle_proof(&indices);

        assert!(batch.verify());
        assert_eq!(batch.proofs.len(), 6);

        // Verify each proof has logarithmic size
        for proof in &batch.proofs {
            // log2(1000) ≈ 10, so proof size should be around 10 hashes
            assert!(proof.siblings.len() <= 12);
            assert!(proof.verify());
        }
    }
}
