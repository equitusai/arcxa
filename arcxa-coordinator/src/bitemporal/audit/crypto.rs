//! Cryptographic primitives for audit trail.
//!
//! Provides hash computation and verification using SHA-256.

use sha2::{Digest, Sha256};
use std::fmt;

/// 32-byte SHA-256 hash.
///
/// Used for:
/// - Hash chaining in audit log
/// - Tamper detection
/// - Merkle tree construction (Week 7)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Hash([u8; 32]);

impl Hash {
    /// Zero hash (used as genesis hash for chain start).
    pub const ZERO: Hash = Hash([0u8; 32]);

    /// Compute SHA-256 hash of input data.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use graphica_coordinator::bitemporal::audit::Hash;
    ///
    /// let data = b"Hello, world!";
    /// let hash = Hash::compute(data);
    /// ```
    pub fn compute(data: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(data);
        let result = hasher.finalize();
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(&result);
        Hash(bytes)
    }

    /// Compute hash from multiple inputs (for Merkle tree construction).
    ///
    /// Concatenates inputs and hashes the result.
    ///
    /// TODO (Week 7): Optimize for Merkle tree batch hashing
    pub fn compute_multi(inputs: &[&[u8]]) -> Self {
        let mut hasher = Sha256::new();
        for input in inputs {
            hasher.update(input);
        }
        let result = hasher.finalize();
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(&result);
        Hash(bytes)
    }

    /// Create hash from raw bytes.
    ///
    /// # Errors
    ///
    /// Returns `None` if input is not exactly 32 bytes.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != 32 {
            return None;
        }
        let mut hash_bytes = [0u8; 32];
        hash_bytes.copy_from_slice(bytes);
        Some(Hash(hash_bytes))
    }

    /// Get raw bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Convert to hex string.
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    /// Parse from hex string.
    ///
    /// # Errors
    ///
    /// Returns `None` if input is not valid 64-character hex.
    pub fn from_hex(s: &str) -> Option<Self> {
        if s.len() != 64 {
            return None;
        }
        let bytes = hex::decode(s).ok()?;
        Self::from_bytes(&bytes)
    }
}

impl fmt::Display for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

impl AsRef<[u8]> for Hash {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zero_hash() {
        let zero = Hash::ZERO;
        assert_eq!(zero.as_bytes(), &[0u8; 32]);
    }

    #[test]
    fn test_hash_compute_deterministic() {
        let data = b"Hello, world!";
        let hash1 = Hash::compute(data);
        let hash2 = Hash::compute(data);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_hash_compute_different_inputs() {
        let hash1 = Hash::compute(b"data1");
        let hash2 = Hash::compute(b"data2");
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_hash_compute_known_value() {
        // SHA-256("Hello, world!") = 315f5bdb76d078c43b8ac0064e4a0164612b1fce77c869345bfc94c75894edd3
        let hash = Hash::compute(b"Hello, world!");
        let expected = "315f5bdb76d078c43b8ac0064e4a0164612b1fce77c869345bfc94c75894edd3";
        assert_eq!(hash.to_hex(), expected);
    }

    #[test]
    fn test_hash_from_bytes() {
        let bytes = [42u8; 32];
        let hash = Hash::from_bytes(&bytes).unwrap();
        assert_eq!(hash.as_bytes(), &bytes);
    }

    #[test]
    fn test_hash_from_bytes_wrong_length() {
        let bytes = [42u8; 16]; // Wrong length
        assert!(Hash::from_bytes(&bytes).is_none());
    }

    #[test]
    fn test_hash_to_hex() {
        let bytes = [
            0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc,
            0xde, 0xf0, 0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0x12, 0x34, 0x56, 0x78,
            0x9a, 0xbc, 0xde, 0xf0,
        ];
        let hash = Hash::from_bytes(&bytes).unwrap();
        assert_eq!(
            hash.to_hex(),
            "123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0"
        );
    }

    #[test]
    fn test_hash_from_hex() {
        let hex = "123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0";
        let hash = Hash::from_hex(hex).unwrap();
        assert_eq!(hash.to_hex(), hex);
    }

    #[test]
    fn test_hash_from_hex_invalid_length() {
        let hex = "123456"; // Too short
        assert!(Hash::from_hex(hex).is_none());
    }

    #[test]
    fn test_hash_from_hex_invalid_chars() {
        let hex = "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz";
        assert!(Hash::from_hex(hex).is_none());
    }

    #[test]
    fn test_hash_roundtrip_hex() {
        let original = Hash::compute(b"test data");
        let hex = original.to_hex();
        let recovered = Hash::from_hex(&hex).unwrap();
        assert_eq!(original, recovered);
    }

    #[test]
    fn test_hash_roundtrip_bytes() {
        let original = Hash::compute(b"test data");
        let bytes = original.as_bytes();
        let recovered = Hash::from_bytes(bytes).unwrap();
        assert_eq!(original, recovered);
    }

    #[test]
    fn test_hash_display() {
        let hash = Hash::compute(b"test");
        let display = format!("{}", hash);
        assert_eq!(display, hash.to_hex());
    }

    #[test]
    fn test_hash_as_ref() {
        let hash = Hash::compute(b"test");
        let bytes: &[u8] = hash.as_ref();
        assert_eq!(bytes.len(), 32);
    }

    #[test]
    fn test_hash_compute_multi_single_input() {
        let data = b"test";
        let hash1 = Hash::compute(data);
        let hash2 = Hash::compute_multi(&[data]);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_hash_compute_multi_deterministic() {
        let inputs = &[
            b"part1".as_slice(),
            b"part2".as_slice(),
            b"part3".as_slice(),
        ];
        let hash1 = Hash::compute_multi(inputs);
        let hash2 = Hash::compute_multi(inputs);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_hash_compute_multi_order_matters() {
        let hash1 = Hash::compute_multi(&[b"a", b"b"]);
        let hash2 = Hash::compute_multi(&[b"b", b"a"]);
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_hash_compute_multi_concatenation_equivalent() {
        let inputs = &[b"Hello, ".as_slice(), b"world!".as_slice()];
        let multi_hash = Hash::compute_multi(inputs);
        let concat_hash = Hash::compute(b"Hello, world!");
        assert_eq!(multi_hash, concat_hash);
    }

    #[test]
    fn test_hash_equality() {
        let hash1 = Hash::compute(b"test");
        let hash2 = Hash::compute(b"test");
        let hash3 = Hash::compute(b"other");
        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_hash_clone() {
        let hash1 = Hash::compute(b"test");
        let hash2 = hash1.clone();
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_hash_copy() {
        let hash1 = Hash::compute(b"test");
        let hash2 = hash1; // Copy, not move
        assert_eq!(hash1, hash2);
    }
}
