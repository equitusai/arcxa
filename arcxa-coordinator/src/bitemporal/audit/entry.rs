//! Audit entry types and operations.
//!
//! Provides tamper-proof audit log entries with Ed25519 signatures.

use super::crypto::Hash;
use crate::bitemporal::TransactionId;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::time::SystemTime;

// Helper module for serializing large arrays
mod sig_array {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S>(bytes: &[u8; 64], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        bytes.as_slice().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<[u8; 64], D::Error>
    where
        D: Deserializer<'de>,
    {
        let vec = Vec::<u8>::deserialize(deserializer)?;
        if vec.len() != 64 {
            return Err(serde::de::Error::custom("Expected 64 bytes"));
        }
        let mut array = [0u8; 64];
        array.copy_from_slice(&vec);
        Ok(array)
    }
}

/// Type of audit operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditOperation {
    /// Insert new triple
    Insert,
    /// Update existing triple
    Update,
    /// Delete triple (soft delete)
    Delete,
    /// Query operation (read-only)
    Query,
}

/// Audit log entry (before signing).
///
/// Contains all metadata about a bitemporal operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Transaction ID
    pub tx_id: TransactionId,

    /// Operation type
    pub operation: AuditOperation,

    /// Node ID that performed the operation
    pub node_id: u64,

    /// Timestamp (Unix epoch milliseconds)
    pub timestamp: u64,

    /// Subject of RDF triple (if applicable)
    pub subject: Option<String>,

    /// Predicate of RDF triple (if applicable)
    pub predicate: Option<String>,

    /// Object of RDF triple (if applicable)
    pub object: Option<String>,

    /// User/service that initiated the operation
    pub initiator: String,
}

impl AuditEntry {
    /// Create a new audit entry.
    pub fn new(
        tx_id: TransactionId,
        operation: AuditOperation,
        node_id: u64,
        initiator: String,
    ) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        Self {
            tx_id,
            operation,
            node_id,
            timestamp,
            subject: None,
            predicate: None,
            object: None,
            initiator,
        }
    }

    /// Add RDF triple data.
    pub fn with_triple(mut self, subject: String, predicate: String, object: String) -> Self {
        self.subject = Some(subject);
        self.predicate = Some(predicate);
        self.object = Some(object);
        self
    }

    /// Compute deterministic hash of this entry.
    ///
    /// Used for hash chaining in the audit log.
    pub fn compute_hash(&self) -> Hash {
        // Serialize to canonical JSON for deterministic hashing
        let json = serde_json::to_string(self).expect("Serialization should not fail");
        Hash::compute(json.as_bytes())
    }

    /// Sign this entry with a signing key, creating a SignedAuditEntry.
    ///
    /// # Arguments
    ///
    /// * `signing_key` - Ed25519 signing key
    /// * `previous_hash` - Hash of previous entry in the chain (or Hash::ZERO for genesis)
    pub fn sign(self, signing_key: &SigningKey, previous_hash: Hash) -> SignedAuditEntry {
        let entry_hash = self.compute_hash();

        // Sign the combination of entry hash + previous hash
        let mut message = Vec::new();
        message.extend_from_slice(entry_hash.as_bytes());
        message.extend_from_slice(previous_hash.as_bytes());

        let signature = signing_key.sign(&message);

        SignedAuditEntry {
            entry: self,
            entry_hash,
            previous_hash,
            signature: signature.to_bytes(),
            public_key: signing_key.verifying_key().to_bytes(),
        }
    }
}

/// Signed audit entry with hash chain.
///
/// Provides:
/// - Ed25519 signature for non-repudiation
/// - Hash chaining for tamper detection
/// - Public key for verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedAuditEntry {
    /// The audit entry
    pub entry: AuditEntry,

    /// Hash of the entry
    pub entry_hash: Hash,

    /// Hash of previous entry (hash chain)
    pub previous_hash: Hash,

    /// Ed25519 signature (64 bytes)
    #[serde(with = "sig_array")]
    pub signature: [u8; 64],

    /// Public key used for signing (32 bytes)
    pub public_key: [u8; 32],
}

impl SignedAuditEntry {
    /// Verify the signature on this entry.
    ///
    /// # Returns
    ///
    /// `true` if signature is valid, `false` otherwise.
    pub fn verify_signature(&self) -> bool {
        // Reconstruct the message that was signed
        let mut message = Vec::new();
        message.extend_from_slice(self.entry_hash.as_bytes());
        message.extend_from_slice(self.previous_hash.as_bytes());

        // Parse verifying key and signature
        let verifying_key = match VerifyingKey::from_bytes(&self.public_key) {
            Ok(key) => key,
            Err(_) => return false,
        };

        let signature = Signature::from_bytes(&self.signature);

        // Verify signature
        verifying_key.verify(&message, &signature).is_ok()
    }

    /// Verify the entry hash matches the stored hash.
    ///
    /// Detects tampering with the entry data.
    pub fn verify_entry_hash(&self) -> bool {
        let computed = self.entry.compute_hash();
        computed == self.entry_hash
    }

    /// Verify both signature and entry hash.
    ///
    /// Full integrity check.
    pub fn verify(&self) -> bool {
        self.verify_entry_hash() && self.verify_signature()
    }

    /// Serialize to bytes for storage.
    pub fn to_bytes(&self) -> Result<Vec<u8>, bincode::Error> {
        bincode::serialize(self)
    }

    /// Deserialize from bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, bincode::Error> {
        bincode::deserialize(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use rand::{rngs::OsRng, RngCore};

    fn create_test_signing_key() -> SigningKey {
        let mut bytes = [0u8; 32];
        OsRng.fill_bytes(&mut bytes);
        SigningKey::from_bytes(&bytes)
    }

    fn create_test_entry() -> AuditEntry {
        AuditEntry::new(
            TransactionId {
                node_id: 1,
                seq: 100,
                timestamp: Utc.timestamp_opt(1234567890, 0).unwrap(),
            },
            AuditOperation::Insert,
            1,
            "test_user".to_string(),
        )
    }

    #[test]
    fn test_audit_operation_serialization() {
        let ops = vec![
            AuditOperation::Insert,
            AuditOperation::Update,
            AuditOperation::Delete,
            AuditOperation::Query,
        ];

        for op in ops {
            let json = serde_json::to_string(&op).unwrap();
            let recovered: AuditOperation = serde_json::from_str(&json).unwrap();
            assert_eq!(op, recovered);
        }
    }

    #[test]
    fn test_audit_entry_creation() {
        let entry = create_test_entry();
        assert_eq!(entry.tx_id.node_id, 1);
        assert_eq!(entry.operation, AuditOperation::Insert);
        assert_eq!(entry.initiator, "test_user");
        assert!(entry.subject.is_none());
    }

    #[test]
    fn test_audit_entry_with_triple() {
        let entry = create_test_entry().with_triple(
            "subject".to_string(),
            "predicate".to_string(),
            "object".to_string(),
        );

        assert_eq!(entry.subject.as_deref(), Some("subject"));
        assert_eq!(entry.predicate.as_deref(), Some("predicate"));
        assert_eq!(entry.object.as_deref(), Some("object"));
    }

    #[test]
    fn test_audit_entry_hash_deterministic() {
        let entry = create_test_entry();
        let hash1 = entry.compute_hash();
        let hash2 = entry.compute_hash();
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_audit_entry_hash_different_for_different_entries() {
        let entry1 = AuditEntry::new(
            TransactionId {
                node_id: 1,
                seq: 100,
                timestamp: Utc.timestamp_opt(1234567890, 0).unwrap(),
            },
            AuditOperation::Insert,
            1,
            "user1".to_string(),
        );

        let entry2 = AuditEntry::new(
            TransactionId {
                node_id: 1,
                seq: 101,
                timestamp: Utc.timestamp_opt(1234567891, 0).unwrap(),
            },
            AuditOperation::Insert,
            1,
            "user1".to_string(),
        );

        let hash1 = entry1.compute_hash();
        let hash2 = entry2.compute_hash();
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_sign_entry() {
        let signing_key = create_test_signing_key();
        let entry = create_test_entry();
        let signed = entry.sign(&signing_key, Hash::ZERO);

        assert_eq!(signed.previous_hash, Hash::ZERO);
        assert_eq!(signed.public_key, signing_key.verifying_key().to_bytes());
    }

    #[test]
    fn test_verify_signature_valid() {
        let signing_key = create_test_signing_key();
        let entry = create_test_entry();
        let signed = entry.sign(&signing_key, Hash::ZERO);

        assert!(signed.verify_signature());
    }

    #[test]
    fn test_verify_signature_invalid_tampering() {
        let signing_key = create_test_signing_key();
        let entry = create_test_entry();
        let mut signed = entry.sign(&signing_key, Hash::ZERO);

        // Tamper with the signature
        signed.signature[0] ^= 0xFF;

        assert!(!signed.verify_signature());
    }

    #[test]
    fn test_verify_entry_hash_valid() {
        let signing_key = create_test_signing_key();
        let entry = create_test_entry();
        let signed = entry.sign(&signing_key, Hash::ZERO);

        assert!(signed.verify_entry_hash());
    }

    #[test]
    fn test_verify_entry_hash_invalid_tampering() {
        let signing_key = create_test_signing_key();
        let entry = create_test_entry();
        let mut signed = entry.sign(&signing_key, Hash::ZERO);

        // Tamper with the entry data
        signed.entry.initiator = "attacker".to_string();

        assert!(!signed.verify_entry_hash());
    }

    #[test]
    fn test_verify_full_integrity() {
        let signing_key = create_test_signing_key();
        let entry = create_test_entry();
        let signed = entry.sign(&signing_key, Hash::ZERO);

        assert!(signed.verify());
    }

    #[test]
    fn test_verify_full_integrity_fails_on_tamper() {
        let signing_key = create_test_signing_key();
        let entry = create_test_entry();
        let mut signed = entry.sign(&signing_key, Hash::ZERO);

        // Tamper with the entry
        signed.entry.initiator = "attacker".to_string();

        assert!(!signed.verify());
    }

    #[test]
    fn test_hash_chain_linking() {
        let signing_key = create_test_signing_key();

        // First entry
        let entry1 = create_test_entry();
        let signed1 = entry1.sign(&signing_key, Hash::ZERO);

        // Second entry chains to first
        let entry2 = AuditEntry::new(
            TransactionId {
                node_id: 1,
                seq: 101,
                timestamp: Utc.timestamp_opt(1234567891, 0).unwrap(),
            },
            AuditOperation::Update,
            1,
            "test_user".to_string(),
        );
        let signed2 = entry2.sign(&signing_key, signed1.entry_hash);

        assert_eq!(signed2.previous_hash, signed1.entry_hash);
    }

    #[test]
    fn test_serialization_roundtrip() {
        let signing_key = create_test_signing_key();
        let entry =
            create_test_entry().with_triple("s".to_string(), "p".to_string(), "o".to_string());
        let signed = entry.sign(&signing_key, Hash::ZERO);

        let bytes = signed.to_bytes().unwrap();
        let recovered = SignedAuditEntry::from_bytes(&bytes).unwrap();

        assert_eq!(recovered.entry_hash, signed.entry_hash);
        assert_eq!(recovered.previous_hash, signed.previous_hash);
        assert_eq!(recovered.signature, signed.signature);
        assert_eq!(recovered.public_key, signed.public_key);
    }

    #[test]
    fn test_serialization_preserves_verification() {
        let signing_key = create_test_signing_key();
        let entry = create_test_entry();
        let signed = entry.sign(&signing_key, Hash::ZERO);

        let bytes = signed.to_bytes().unwrap();
        let recovered = SignedAuditEntry::from_bytes(&bytes).unwrap();

        assert!(recovered.verify());
    }

    #[test]
    fn test_different_operations() {
        let operations = vec![
            AuditOperation::Insert,
            AuditOperation::Update,
            AuditOperation::Delete,
            AuditOperation::Query,
        ];

        for op in operations {
            let entry = AuditEntry::new(
                TransactionId {
                    node_id: 1,
                    seq: 100,
                    timestamp: Utc.timestamp_opt(1234567890, 0).unwrap(),
                },
                op,
                1,
                "test_user".to_string(),
            );
            assert_eq!(entry.operation, op);
        }
    }

    #[test]
    fn test_timestamp_is_set() {
        let entry = create_test_entry();
        assert!(entry.timestamp > 0);
    }

    #[test]
    fn test_multiple_keys_different_signatures() {
        let signing_key1 = create_test_signing_key();
        let signing_key2 = create_test_signing_key();
        let entry = create_test_entry();

        let signed1 = entry.clone().sign(&signing_key1, Hash::ZERO);
        let signed2 = entry.sign(&signing_key2, Hash::ZERO);

        assert_ne!(signed1.signature, signed2.signature);
        assert_ne!(signed1.public_key, signed2.public_key);
    }

    #[test]
    fn test_both_signatures_verify() {
        let signing_key1 = create_test_signing_key();
        let signing_key2 = create_test_signing_key();
        let entry = create_test_entry();

        let signed1 = entry.clone().sign(&signing_key1, Hash::ZERO);
        let signed2 = entry.sign(&signing_key2, Hash::ZERO);

        assert!(signed1.verify());
        assert!(signed2.verify());
    }

    #[test]
    fn test_cross_key_verification_fails() {
        let signing_key1 = create_test_signing_key();
        let signing_key2 = create_test_signing_key();
        let entry = create_test_entry();

        let mut signed = entry.sign(&signing_key1, Hash::ZERO);

        // Replace public key with different key
        signed.public_key = signing_key2.verifying_key().to_bytes();

        // Signature should no longer verify
        assert!(!signed.verify_signature());
    }

    #[test]
    fn test_entry_serialization_roundtrip() {
        let entry =
            create_test_entry().with_triple("s".to_string(), "p".to_string(), "o".to_string());

        let json = serde_json::to_string(&entry).unwrap();
        let recovered: AuditEntry = serde_json::from_str(&json).unwrap();

        assert_eq!(recovered.tx_id.seq, entry.tx_id.seq);
        assert_eq!(recovered.operation, entry.operation);
        assert_eq!(recovered.subject, entry.subject);
    }

    #[test]
    fn test_hash_changes_with_triple_data() {
        let entry1 = create_test_entry();
        let entry2 =
            create_test_entry().with_triple("s".to_string(), "p".to_string(), "o".to_string());

        let hash1 = entry1.compute_hash();
        let hash2 = entry2.compute_hash();

        assert_ne!(hash1, hash2);
    }
}
