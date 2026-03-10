//! Integration Test: Audited Shard Coordinator
//!
//! This test demonstrates the full integration of the cryptographic audit chain
//! with the shard coordinator, showing how all write operations are automatically
//! logged to a tamper-proof audit trail.

#[cfg(feature = "cryptographic-audit")]
mod tests {
    use ed25519_dalek::SigningKey;
    use graphica_coordinator::governance::{
        distributed::ShardRegistry, rdf_store::RdfStore, AuditedShardCoordinator,
        ShardCoordinatingRdfStore,
    };
    use graphica_coordinator::AppContext;
    use rand::{rngs::OsRng, RngCore};
    use tempfile::TempDir;

    fn create_test_signing_key() -> SigningKey {
        let mut bytes = [0u8; 32];
        OsRng.fill_bytes(&mut bytes);
        SigningKey::from_bytes(&bytes)
    }

    #[test]
    fn test_audited_coordinator_creation() {
        // This is a unit test - full integration requires running shards
        // Demonstrates the API for creating an audited coordinator

        let temp_dir = TempDir::new().unwrap();
        let audit_path = temp_dir.path().join("audit");
        let shard_path = temp_dir.path().join("shards");

        // Create shard registry
        let registry = ShardRegistry::new(shard_path, 2, 60).unwrap();

        // Register mock shards with AUTOMATIC hash range distribution
        // No need to calculate hash ranges manually!
        let shards = vec![
            (0, "localhost:9090".to_string(), vec![]),
            (1, "localhost:9091".to_string(), vec![]),
        ];
        registry.register_shards_auto(shards).unwrap();
        // Hash ranges automatically: shard 0 = 0-50%, shard 1 = 50-100%

        // Create base shard coordinator
        let context = AppContext::minimal();
        let shard_store = ShardCoordinatingRdfStore::new(registry.into(), context);

        // Wrap with audit chain
        let signing_key = create_test_signing_key();
        let audited = AuditedShardCoordinator::new(
            shard_store,
            signing_key,
            audit_path,
            "admin_user".to_string(),
        )
        .unwrap();

        // Verify audit chain is initialized
        assert_eq!(audited.audit_chain_length(), 0);
        assert_eq!(
            audited.merkle_root(),
            graphica_coordinator::bitemporal::audit::Hash::ZERO
        );

        // Note: Actual writes would require running shard servers
        // This test demonstrates the structure and API
    }

    #[test]
    fn test_audit_chain_verification() {
        // Unit test for audit chain operations
        let temp_dir = TempDir::new().unwrap();
        let audit_path = temp_dir.path().join("audit");
        let shard_path = temp_dir.path().join("shards");

        let registry = ShardRegistry::new(shard_path, 1, 60).unwrap();

        // Single shard with automatic hash range (0-100%)
        registry
            .register_shards_auto(vec![(0, "localhost:9090".to_string(), vec![])])
            .unwrap();

        let context = AppContext::minimal();
        let shard_store = ShardCoordinatingRdfStore::new(registry.into(), context);

        let signing_key = create_test_signing_key();
        let audited = AuditedShardCoordinator::new(
            shard_store,
            signing_key,
            audit_path,
            "test_user".to_string(),
        )
        .unwrap();

        // Verify audit chain integrity
        assert!(audited.verify_audit_chain().is_ok());
        assert!(audited.verify_audit_chain_with_merkle().is_ok());
    }

    #[test]
    fn test_audited_coordinator_merkle_operations() {
        // Test Merkle tree operations on audit chain
        let temp_dir = TempDir::new().unwrap();
        let audit_path = temp_dir.path().join("audit");
        let shard_path = temp_dir.path().join("shards");

        let registry = ShardRegistry::new(shard_path, 1, 60).unwrap();

        // Single shard with automatic hash range (0-100%)
        registry
            .register_shards_auto(vec![(0, "localhost:9090".to_string(), vec![])])
            .unwrap();

        let context = AppContext::minimal();
        let shard_store = ShardCoordinatingRdfStore::new(registry.into(), context);

        let signing_key = create_test_signing_key();
        let audited = AuditedShardCoordinator::new(
            shard_store,
            signing_key,
            audit_path,
            "test_user".to_string(),
        )
        .unwrap();

        // Test Merkle root
        let root = audited.merkle_root();
        assert_eq!(root, graphica_coordinator::bitemporal::audit::Hash::ZERO);

        // Merkle proof for empty chain should be None
        assert!(audited.merkle_proof(0).is_none());

        // Batch proof for empty indices
        let batch = audited.batch_merkle_proof(&[]);
        assert_eq!(batch.proofs.len(), 0);
    }
}
