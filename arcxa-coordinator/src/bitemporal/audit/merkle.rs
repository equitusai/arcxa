//! Merkle tree for efficient batch verification of audit entries.
//!
//! Provides:
//! - Binary Merkle tree construction from audit entry hashes
//! - Inclusion proofs for individual entries
//! - Batch verification with O(log n) proof size
//! - Root hash computation for chain snapshots
//!
//! ## Architecture
//!
//! ```text
//!           Root Hash
//!          /         \
//!      H(AB)         H(CD)
//!      /   \         /   \
//!    H(A) H(B)    H(C)  H(D)
//!     |    |       |     |
//!    E1   E2      E3    E4
//! ```
//!
//! ## Performance
//!
//! - Build time: O(n)
//! - Proof size: O(log n)
//! - Verification: O(log n)
//!
//! ## Week 7 Implementation
//!
//! Merkle trees allow verification of thousands of entries with minimal proof overhead.

use super::crypto::Hash;
use super::entry::SignedAuditEntry;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Merkle tree for audit entry hashes.
#[derive(Debug, Clone)]
pub struct MerkleTree {
    /// Root hash of the tree
    root: Hash,

    /// Tree height (0 for single leaf)
    height: usize,

    /// Leaf hashes (entry hashes)
    leaves: Vec<Hash>,

    /// Internal nodes (level -> index -> hash)
    nodes: HashMap<(usize, usize), Hash>,
}

/// Merkle inclusion proof for an audit entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MerkleProof {
    /// Index of the leaf in the tree
    pub leaf_index: usize,

    /// Hash of the leaf
    pub leaf_hash: Hash,

    /// Sibling hashes along the path to root
    pub siblings: Vec<Hash>,

    /// Root hash of the tree
    pub root: Hash,
}

impl MerkleTree {
    /// Build a Merkle tree from audit entry hashes.
    ///
    /// # Arguments
    ///
    /// * `entries` - Signed audit entries to include in tree
    ///
    /// # Example
    ///
    /// ```ignore
    /// let tree = MerkleTree::from_entries(&entries);
    /// let root = tree.root();
    /// ```
    pub fn from_entries(entries: &[SignedAuditEntry]) -> Self {
        if entries.is_empty() {
            return Self::empty();
        }

        let leaves: Vec<Hash> = entries.iter().map(|e| e.entry_hash).collect();
        Self::from_leaves(leaves)
    }

    /// Build a Merkle tree from leaf hashes.
    ///
    /// # Arguments
    ///
    /// * `leaves` - Leaf hashes to include in tree
    pub fn from_leaves(mut leaves: Vec<Hash>) -> Self {
        if leaves.is_empty() {
            return Self::empty();
        }

        // Pad to next power of 2 for balanced tree
        let target_size = leaves.len().next_power_of_two();
        while leaves.len() < target_size {
            leaves.push(Hash::ZERO);
        }

        let mut nodes = HashMap::new();
        let height = (leaves.len() as f64).log2() as usize;

        // Store leaves at level 0
        for (i, &hash) in leaves.iter().enumerate() {
            nodes.insert((0, i), hash);
        }

        // Build tree bottom-up
        for level in 1..=height {
            let prev_level_size = 1 << (height - level + 1);
            let level_size = prev_level_size / 2;

            for i in 0..level_size {
                let left = nodes.get(&(level - 1, i * 2)).unwrap();
                let right = nodes.get(&(level - 1, i * 2 + 1)).unwrap();

                let parent = Self::hash_pair(*left, *right);
                nodes.insert((level, i), parent);
            }
        }

        let root = *nodes.get(&(height, 0)).unwrap();

        Self {
            root,
            height,
            leaves,
            nodes,
        }
    }

    /// Create an empty Merkle tree.
    fn empty() -> Self {
        Self {
            root: Hash::ZERO,
            height: 0,
            leaves: Vec::new(),
            nodes: HashMap::new(),
        }
    }

    /// Get the root hash of the tree.
    pub fn root(&self) -> Hash {
        self.root
    }

    /// Get the height of the tree.
    pub fn height(&self) -> usize {
        self.height
    }

    /// Get the number of leaves in the tree.
    pub fn len(&self) -> usize {
        self.leaves.len()
    }

    /// Check if the tree is empty.
    pub fn is_empty(&self) -> bool {
        self.leaves.is_empty()
    }

    /// Generate an inclusion proof for a leaf at the given index.
    ///
    /// # Arguments
    ///
    /// * `index` - Index of the leaf (0-based)
    ///
    /// # Returns
    ///
    /// Merkle proof for the leaf, or `None` if index is out of bounds.
    pub fn proof(&self, index: usize) -> Option<MerkleProof> {
        if index >= self.leaves.len() {
            return None;
        }

        let leaf_hash = self.leaves[index];
        let mut siblings = Vec::new();

        let mut current_index = index;

        for level in 0..self.height {
            // Determine sibling index
            let sibling_index = if current_index % 2 == 0 {
                current_index + 1
            } else {
                current_index - 1
            };

            // Get sibling hash
            if let Some(&sibling) = self.nodes.get(&(level, sibling_index)) {
                siblings.push(sibling);
            } else {
                siblings.push(Hash::ZERO);
            }

            // Move to parent
            current_index /= 2;
        }

        Some(MerkleProof {
            leaf_index: index,
            leaf_hash,
            siblings,
            root: self.root,
        })
    }

    /// Hash a pair of nodes (internal tree operation).
    fn hash_pair(left: Hash, right: Hash) -> Hash {
        let mut data = Vec::with_capacity(64);
        data.extend_from_slice(left.as_bytes());
        data.extend_from_slice(right.as_bytes());
        Hash::compute(&data)
    }
}

impl MerkleProof {
    /// Verify the inclusion proof.
    ///
    /// # Returns
    ///
    /// `true` if the proof is valid and the leaf is in the tree.
    pub fn verify(&self) -> bool {
        let mut current = self.leaf_hash;
        let mut index = self.leaf_index;

        for sibling in &self.siblings {
            current = if index % 2 == 0 {
                MerkleTree::hash_pair(current, *sibling)
            } else {
                MerkleTree::hash_pair(*sibling, current)
            };
            index /= 2;
        }

        current == self.root
    }

    /// Get the leaf index.
    pub fn leaf_index(&self) -> usize {
        self.leaf_index
    }

    /// Get the proof size in bytes.
    pub fn size(&self) -> usize {
        8 + 32 + (self.siblings.len() * 32) + 32
    }
}

/// Batch proof for multiple audit entries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchProof {
    /// Root hash of the Merkle tree
    pub root: Hash,

    /// Individual proofs for each entry
    pub proofs: Vec<MerkleProof>,
}

impl BatchProof {
    /// Create a batch proof from individual proofs.
    pub fn new(root: Hash, proofs: Vec<MerkleProof>) -> Self {
        Self { root, proofs }
    }

    /// Verify all proofs in the batch.
    ///
    /// # Returns
    ///
    /// `true` if all proofs are valid.
    pub fn verify(&self) -> bool {
        self.proofs
            .iter()
            .all(|proof| proof.root == self.root && proof.verify())
    }

    /// Get the number of proofs in the batch.
    pub fn len(&self) -> usize {
        self.proofs.len()
    }

    /// Check if the batch is empty.
    pub fn is_empty(&self) -> bool {
        self.proofs.is_empty()
    }

    /// Get the total proof size in bytes.
    pub fn size(&self) -> usize {
        32 + self.proofs.iter().map(|p| p.size()).sum::<usize>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bitemporal::audit::entry::{AuditEntry, AuditOperation};
    use crate::bitemporal::TransactionId;
    use chrono::{TimeZone, Utc};
    use ed25519_dalek::SigningKey;
    use rand::{rngs::OsRng, RngCore};

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
    fn test_merkle_tree_empty() {
        let tree = MerkleTree::empty();
        assert_eq!(tree.len(), 0);
        assert!(tree.is_empty());
        assert_eq!(tree.height(), 0);
        assert_eq!(tree.root(), Hash::ZERO);
    }

    #[test]
    fn test_merkle_tree_single_entry() {
        let signing_key = create_test_signing_key();
        let entry = create_test_entry(0);
        let signed = entry.sign(&signing_key, Hash::ZERO);

        let tree = MerkleTree::from_entries(&[signed.clone()]);

        assert_eq!(tree.len(), 1);
        assert_eq!(tree.height(), 0);
        assert_eq!(tree.root(), signed.entry_hash);
    }

    #[test]
    fn test_merkle_tree_two_entries() {
        let signing_key = create_test_signing_key();
        let mut entries = Vec::new();

        for i in 0..2 {
            let entry = create_test_entry(i);
            let signed = entry.sign(&signing_key, Hash::ZERO);
            entries.push(signed);
        }

        let tree = MerkleTree::from_entries(&entries);

        assert_eq!(tree.len(), 2);
        assert_eq!(tree.height(), 1);
    }

    #[test]
    fn test_merkle_tree_power_of_two() {
        let signing_key = create_test_signing_key();
        let mut entries = Vec::new();

        for i in 0..4 {
            let entry = create_test_entry(i);
            let signed = entry.sign(&signing_key, Hash::ZERO);
            entries.push(signed);
        }

        let tree = MerkleTree::from_entries(&entries);

        assert_eq!(tree.len(), 4);
        assert_eq!(tree.height(), 2);
    }

    #[test]
    fn test_merkle_tree_non_power_of_two() {
        let signing_key = create_test_signing_key();
        let mut entries = Vec::new();

        for i in 0..5 {
            let entry = create_test_entry(i);
            let signed = entry.sign(&signing_key, Hash::ZERO);
            entries.push(signed);
        }

        let tree = MerkleTree::from_entries(&entries);

        // Should pad to 8 (next power of 2)
        assert_eq!(tree.len(), 8);
        assert_eq!(tree.height(), 3);
    }

    #[test]
    fn test_merkle_proof_generation() {
        let signing_key = create_test_signing_key();
        let mut entries = Vec::new();

        for i in 0..4 {
            let entry = create_test_entry(i);
            let signed = entry.sign(&signing_key, Hash::ZERO);
            entries.push(signed);
        }

        let tree = MerkleTree::from_entries(&entries);

        // Generate proof for first entry
        let proof = tree.proof(0).unwrap();
        assert_eq!(proof.leaf_index, 0);
        assert_eq!(proof.leaf_hash, entries[0].entry_hash);
        assert_eq!(proof.siblings.len(), tree.height());
    }

    #[test]
    fn test_merkle_proof_verification() {
        let signing_key = create_test_signing_key();
        let mut entries = Vec::new();

        for i in 0..4 {
            let entry = create_test_entry(i);
            let signed = entry.sign(&signing_key, Hash::ZERO);
            entries.push(signed);
        }

        let tree = MerkleTree::from_entries(&entries);

        // Verify proof for each entry
        for i in 0..4 {
            let proof = tree.proof(i).unwrap();
            assert!(proof.verify(), "Proof for entry {} should verify", i);
        }
    }

    #[test]
    fn test_merkle_proof_invalid_index() {
        let signing_key = create_test_signing_key();
        let mut entries = Vec::new();

        for i in 0..4 {
            let entry = create_test_entry(i);
            let signed = entry.sign(&signing_key, Hash::ZERO);
            entries.push(signed);
        }

        let tree = MerkleTree::from_entries(&entries);

        // Request proof for out-of-bounds index
        assert!(tree.proof(100).is_none());
    }

    #[test]
    fn test_merkle_proof_tamper_detection() {
        let signing_key = create_test_signing_key();
        let mut entries = Vec::new();

        for i in 0..4 {
            let entry = create_test_entry(i);
            let signed = entry.sign(&signing_key, Hash::ZERO);
            entries.push(signed);
        }

        let tree = MerkleTree::from_entries(&entries);
        let mut proof = tree.proof(0).unwrap();

        // Tamper with the leaf hash
        proof.leaf_hash = Hash::compute(b"tampered");

        // Verification should fail
        assert!(!proof.verify());
    }

    #[test]
    fn test_batch_proof_verification() {
        let signing_key = create_test_signing_key();
        let mut entries = Vec::new();

        for i in 0..8 {
            let entry = create_test_entry(i);
            let signed = entry.sign(&signing_key, Hash::ZERO);
            entries.push(signed);
        }

        let tree = MerkleTree::from_entries(&entries);

        // Create batch proof for subset of entries
        let proofs = vec![
            tree.proof(0).unwrap(),
            tree.proof(3).unwrap(),
            tree.proof(7).unwrap(),
        ];

        let batch = BatchProof::new(tree.root(), proofs);

        assert_eq!(batch.len(), 3);
        assert!(batch.verify());
    }

    #[test]
    fn test_batch_proof_tampered() {
        let signing_key = create_test_signing_key();
        let mut entries = Vec::new();

        for i in 0..8 {
            let entry = create_test_entry(i);
            let signed = entry.sign(&signing_key, Hash::ZERO);
            entries.push(signed);
        }

        let tree = MerkleTree::from_entries(&entries);

        // Create batch proof
        let mut proofs = vec![tree.proof(0).unwrap(), tree.proof(3).unwrap()];

        // Tamper with one proof
        proofs[1].leaf_hash = Hash::compute(b"tampered");

        let batch = BatchProof::new(tree.root(), proofs);

        // Verification should fail
        assert!(!batch.verify());
    }

    #[test]
    fn test_merkle_tree_large() {
        let signing_key = create_test_signing_key();
        let mut entries = Vec::new();

        for i in 0..100 {
            let entry = create_test_entry(i);
            let signed = entry.sign(&signing_key, Hash::ZERO);
            entries.push(signed);
        }

        let tree = MerkleTree::from_entries(&entries);

        // Should pad to 128 (next power of 2)
        assert_eq!(tree.len(), 128);
        assert_eq!(tree.height(), 7);

        // Verify random proofs
        for &i in &[0, 25, 50, 75, 99] {
            let proof = tree.proof(i).unwrap();
            assert!(proof.verify());
        }
    }

    #[test]
    fn test_proof_size_logarithmic() {
        let signing_key = create_test_signing_key();

        // Test with different tree sizes
        for size in &[4, 8, 16, 32, 64] {
            let mut entries = Vec::new();

            for i in 0..*size {
                let entry = create_test_entry(i as u64);
                let signed = entry.sign(&signing_key, Hash::ZERO);
                entries.push(signed);
            }

            let tree = MerkleTree::from_entries(&entries);
            let proof = tree.proof(0).unwrap();

            // Proof size should be O(log n)
            let expected_height = (*size as f64).log2() as usize;
            assert_eq!(proof.siblings.len(), expected_height);
        }
    }

    #[test]
    fn test_merkle_tree_deterministic() {
        let signing_key = create_test_signing_key();
        let mut entries = Vec::new();

        for i in 0..4 {
            let entry = create_test_entry(i);
            let signed = entry.sign(&signing_key, Hash::ZERO);
            entries.push(signed);
        }

        let tree1 = MerkleTree::from_entries(&entries);
        let tree2 = MerkleTree::from_entries(&entries);

        // Same inputs should produce same root
        assert_eq!(tree1.root(), tree2.root());
    }

    #[test]
    fn test_merkle_tree_different_roots() {
        let signing_key = create_test_signing_key();

        let mut entries1 = Vec::new();
        for i in 0..4 {
            let entry = create_test_entry(i);
            let signed = entry.sign(&signing_key, Hash::ZERO);
            entries1.push(signed);
        }

        let mut entries2 = Vec::new();
        for i in 0..4 {
            let entry = create_test_entry(i + 100); // Different sequence
            let signed = entry.sign(&signing_key, Hash::ZERO);
            entries2.push(signed);
        }

        let tree1 = MerkleTree::from_entries(&entries1);
        let tree2 = MerkleTree::from_entries(&entries2);

        // Different inputs should produce different roots
        assert_ne!(tree1.root(), tree2.root());
    }
}
