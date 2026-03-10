//! Integration tests for the audit module.
//!
//! These tests verify the end-to-end functionality of the cryptographic audit chain.

#[cfg(feature = "cryptographic-audit")]
#[cfg(test)]
mod integration_tests {
    use crate::bitemporal::audit::{
        AuditChain, AuditEntry, AuditOperation, ChainVerifier, Hash, VerificationError,
    };
    use crate::bitemporal::TransactionId;
    use chrono::{TimeZone, Utc};
    use ed25519_dalek::SigningKey;
    use rand::{rngs::OsRng, RngCore};

    fn create_test_signing_key() -> SigningKey {
        let mut bytes = [0u8; 32];
        OsRng.fill_bytes(&mut bytes);
        SigningKey::from_bytes(&bytes)
    }

    fn create_test_entry(node_id: u64, seq: u64, operation: AuditOperation) -> AuditEntry {
        AuditEntry::new(
            TransactionId {
                node_id: node_id as u16,
                seq,
                timestamp: Utc.timestamp_opt(1234567890 + seq as i64, 0).unwrap(),
            },
            operation,
            node_id,
            format!("user_{}", node_id),
        )
    }

    #[test]
    fn test_end_to_end_audit_workflow() {
        // Create a new audit chain
        let signing_key = create_test_signing_key();
        let chain = AuditChain::new(signing_key);

        // Simulate a series of bitemporal operations
        let operations = vec![
            (1, 100, AuditOperation::Insert),
            (1, 101, AuditOperation::Update),
            (1, 102, AuditOperation::Insert),
            (1, 103, AuditOperation::Delete),
            (1, 104, AuditOperation::Query),
        ];

        for (node_id, sequence, operation) in operations {
            let entry = create_test_entry(node_id, sequence, operation);
            chain.append(entry);
        }

        // Verify the chain
        let result = ChainVerifier::verify(&chain);
        assert!(result.valid);
        assert_eq!(result.entries_verified, 5);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_audit_chain_with_rdf_triples() {
        let signing_key = create_test_signing_key();
        let chain = AuditChain::new(signing_key);

        // Create entries with RDF triple data
        let entry1 = create_test_entry(1, 100, AuditOperation::Insert).with_triple(
            "graphica:entity/123".to_string(),
            "rdf:type".to_string(),
            "graphica:Customer".to_string(),
        );

        let entry2 = create_test_entry(1, 101, AuditOperation::Update).with_triple(
            "graphica:entity/123".to_string(),
            "graphica:name".to_string(),
            "John Doe".to_string(),
        );

        chain.append(entry1);
        chain.append(entry2);

        // Verify chain integrity
        assert!(ChainVerifier::verify(&chain).valid);

        // Verify RDF data is preserved
        let entries = chain.get_all();
        assert_eq!(
            entries[0].entry.subject.as_deref(),
            Some("graphica:entity/123")
        );
        assert_eq!(entries[0].entry.predicate.as_deref(), Some("rdf:type"));
        assert_eq!(entries[1].entry.predicate.as_deref(), Some("graphica:name"));
    }

    #[test]
    fn test_audit_chain_export_import() {
        let signing_key = create_test_signing_key();
        let chain1 = AuditChain::new(signing_key.clone());

        // Build a chain
        for i in 0..10 {
            let entry = create_test_entry(1, i, AuditOperation::Insert);
            chain1.append(entry);
        }

        // Export the chain
        let exported = chain1.export().unwrap();

        // Import into a new chain
        let chain2 = AuditChain::new(signing_key);
        chain2.import(&exported).unwrap();

        // Verify both chains are identical
        assert_eq!(chain1.len(), chain2.len());
        assert_eq!(chain1.head_hash(), chain2.head_hash());

        // Verify imported chain integrity
        let result = ChainVerifier::verify(&chain2);
        assert!(result.valid);
        assert_eq!(result.entries_verified, 10);
    }

    #[test]
    fn test_tamper_detection() {
        let signing_key = create_test_signing_key();
        let chain = AuditChain::new(signing_key);

        // Build a chain
        for i in 0..5 {
            chain.append(create_test_entry(1, i, AuditOperation::Insert));
        }

        // Verify it's initially valid
        assert!(ChainVerifier::verify(&chain).valid);

        // Export and tamper with the data
        let mut exported = chain.export().unwrap();
        exported[100] ^= 0xFF; // Flip some bits

        // Try to import tampered data
        let chain2 = AuditChain::new(create_test_signing_key());
        let import_result = chain2.import(&exported);

        // Should fail due to tamper detection
        assert!(import_result.is_err());
    }

    #[test]
    fn test_multi_node_audit_trail() {
        let signing_key = create_test_signing_key();
        let chain = AuditChain::new(signing_key);

        // Simulate operations from multiple nodes
        let operations = vec![
            (1, 100, AuditOperation::Insert),
            (2, 50, AuditOperation::Insert),
            (1, 101, AuditOperation::Update),
            (3, 10, AuditOperation::Insert),
            (2, 51, AuditOperation::Delete),
        ];

        for (node_id, sequence, operation) in operations {
            let entry = create_test_entry(node_id, sequence, operation);
            chain.append(entry);
        }

        // Verify chain
        let result = ChainVerifier::verify(&chain);
        assert!(result.valid);
        assert_eq!(result.entries_verified, 5);

        // Verify entries from different nodes
        let entries = chain.get_all();
        assert_eq!(entries[0].entry.tx_id.node_id, 1);
        assert_eq!(entries[1].entry.tx_id.node_id, 2);
        assert_eq!(entries[2].entry.tx_id.node_id, 1);
        assert_eq!(entries[3].entry.tx_id.node_id, 3);
        assert_eq!(entries[4].entry.tx_id.node_id, 2);
    }

    #[test]
    fn test_compliance_audit_trail() {
        // This test simulates a SOX/HIPAA compliance audit scenario
        let signing_key = create_test_signing_key();
        let chain = AuditChain::new(signing_key);

        // Record a series of operations on sensitive data
        let operations = vec![
            ("CREATE customer record", AuditOperation::Insert),
            ("UPDATE customer email", AuditOperation::Update),
            ("QUERY customer data", AuditOperation::Query),
            ("DELETE customer PII", AuditOperation::Delete),
        ];

        for (i, (description, operation)) in operations.iter().enumerate() {
            let entry = create_test_entry(1, i as u64, *operation).with_triple(
                "graphica:entity/customer_123".to_string(),
                "audit:description".to_string(),
                description.to_string(),
            );
            chain.append(entry);
        }

        // Verify complete audit trail
        let result = ChainVerifier::verify(&chain);
        assert!(result.valid);
        assert_eq!(result.entries_verified, 4);

        // Verify all operations are audited
        let entries = chain.get_all();
        assert_eq!(entries.len(), 4);

        // Verify non-repudiation (all entries are signed)
        for entry in &entries {
            assert!(entry.verify_signature());
        }

        // Verify tamper-evidence (hash chain is intact)
        assert_eq!(entries[0].previous_hash, Hash::ZERO);
        for i in 1..entries.len() {
            assert_eq!(entries[i].previous_hash, entries[i - 1].entry_hash);
        }
    }

    #[test]
    fn test_concurrent_audit_writes() {
        use std::sync::Arc;
        use std::thread;

        let signing_key = create_test_signing_key();
        let chain = Arc::new(AuditChain::new(signing_key));

        // Spawn multiple threads appending to the chain
        let mut handles = vec![];
        for node_id in 1..=5 {
            let chain_clone = Arc::clone(&chain);
            let handle = thread::spawn(move || {
                for seq in 0..10 {
                    let entry = create_test_entry(node_id, seq, AuditOperation::Insert);
                    chain_clone.append(entry);
                }
            });
            handles.push(handle);
        }

        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }

        // Verify chain integrity after concurrent writes
        assert_eq!(chain.len(), 50);
        let result = ChainVerifier::verify(&chain);

        // NOTE: With concurrent writes, timestamp ordering may not be strict
        // because entries from different threads can be interleaved.
        // The important thing is that signatures and hash chain are valid.
        // Filter out timestamp regression errors as they're expected with concurrency.
        let non_timestamp_errors: Vec<_> = result
            .errors
            .iter()
            .filter(|e| !matches!(e, VerificationError::TimestampRegression { .. }))
            .collect();

        assert!(
            non_timestamp_errors.is_empty(),
            "Chain should be valid except for possible timestamp regressions: {:?}",
            non_timestamp_errors
        );
        assert_eq!(result.entries_verified, 50);
    }

    #[test]
    fn test_large_audit_chain() {
        let signing_key = create_test_signing_key();
        let chain = AuditChain::new(signing_key);

        // Create a large chain (1000 entries)
        for i in 0..1000 {
            let operation = match i % 4 {
                0 => AuditOperation::Insert,
                1 => AuditOperation::Update,
                2 => AuditOperation::Delete,
                _ => AuditOperation::Query,
            };
            chain.append(create_test_entry(1, i, operation));
        }

        // Verify the entire chain
        let result = ChainVerifier::verify(&chain);
        assert!(result.valid);
        assert_eq!(result.entries_verified, 1000);
    }

    #[test]
    fn test_audit_chain_immutability() {
        let signing_key = create_test_signing_key();
        let chain = AuditChain::new(signing_key);

        // Append some entries
        chain.append(create_test_entry(1, 100, AuditOperation::Insert));
        chain.append(create_test_entry(1, 101, AuditOperation::Update));

        // Get a copy of the first entry
        let first_entry_hash = chain.get(0).unwrap().entry_hash;

        // Append more entries
        chain.append(create_test_entry(1, 102, AuditOperation::Delete));
        chain.append(create_test_entry(1, 103, AuditOperation::Query));

        // Verify the first entry hasn't changed
        assert_eq!(chain.get(0).unwrap().entry_hash, first_entry_hash);

        // Verify chain integrity
        assert!(ChainVerifier::verify(&chain).valid);
    }
}
