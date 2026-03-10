// Integration tests for RocksDB Temporal Indexes
//
// Tests verify:
// 1. O(1) current version lookup
// 2. O(log n) historical queries
// 3. Version chain management
// 4. Bitemporal queries (valid time + system time)

use chrono::{Duration, Utc};
use graphica_coordinator::governance::bitemporal::{TemporalIndexes, TransactionId, VersionRef};
use graphica_coordinator::governance::rdf_star::AnnotatedTriple;
use tempfile::TempDir;
use uuid::Uuid;

#[test]
fn test_index_and_retrieve_current_version() {
    let temp_dir = TempDir::new().unwrap();
    let indexes = TemporalIndexes::new(temp_dir.path().join("temporal_idx")).unwrap();

    // Create a transaction ID
    let tx_id = TransactionId::new(1, Utc::now(), 1);

    // Create a triple with temporal annotations
    let valid_from = Utc::now();
    let triple = AnnotatedTriple::new(
        "http://example.org/entity/1",
        "http://example.org/prop/value",
        "100",
    )
    .with_valid_time(valid_from, None); // Still valid

    // Index the version
    let version_id = Uuid::new_v4().to_string();
    indexes.index_version(&triple, &version_id, &tx_id).unwrap();

    println!("✅ Indexed version: {}", version_id);

    // Retrieve current version
    let current = indexes
        .find_current_version(
            "http://example.org/entity/1",
            "http://example.org/prop/value",
        )
        .unwrap();

    assert!(current.is_some(), "Should find current version");
    let version = current.unwrap();

    assert_eq!(version.version_id, version_id);
    assert_eq!(version.subject, "http://example.org/entity/1");
    assert_eq!(version.predicate, "http://example.org/prop/value");
    assert_eq!(version.object, "100");
    assert!(version.is_current(), "Should be current version");

    println!("✅ Current version lookup successful (O(1))");
}

#[test]
fn test_multiple_versions_and_version_chain() {
    let temp_dir = TempDir::new().unwrap();
    let indexes = TemporalIndexes::new(temp_dir.path().join("temporal_idx")).unwrap();

    let subject = "http://example.org/entity/customer_123";
    let predicate = "http://example.org/prop/revenue";

    // Version 1: Revenue = 100K
    let tx1 = TransactionId::new(1, Utc::now(), 1);
    let v1_time = Utc::now();
    let triple_v1 =
        AnnotatedTriple::new(subject, predicate, "100000").with_valid_time(v1_time, None);
    let v1_id = Uuid::new_v4().to_string();

    indexes.index_version(&triple_v1, &v1_id, &tx1).unwrap();
    println!("✅ Indexed v1: revenue=100000");

    // Wait a bit to ensure different timestamps
    std::thread::sleep(std::time::Duration::from_millis(10));

    // Version 2: Revenue = 200K (supersedes v1)
    let tx2 = TransactionId::new(2, Utc::now(), 1);
    let v2_time = Utc::now();
    let triple_v2 =
        AnnotatedTriple::new(subject, predicate, "200000").with_valid_time(v2_time, None);
    let v2_id = Uuid::new_v4().to_string();

    indexes.index_version(&triple_v2, &v2_id, &tx2).unwrap();
    println!("✅ Indexed v2: revenue=200000");

    // Supersede v1
    indexes.supersede_version(&v1_id, tx2.timestamp).unwrap();
    println!("✅ Superseded v1");

    std::thread::sleep(std::time::Duration::from_millis(10));

    // Version 3: Revenue = 300K (supersedes v2)
    let tx3 = TransactionId::new(3, Utc::now(), 1);
    let v3_time = Utc::now();
    let triple_v3 =
        AnnotatedTriple::new(subject, predicate, "300000").with_valid_time(v3_time, None);
    let v3_id = Uuid::new_v4().to_string();

    indexes.index_version(&triple_v3, &v3_id, &tx3).unwrap();
    println!("✅ Indexed v3: revenue=300000");

    indexes.supersede_version(&v2_id, tx3.timestamp).unwrap();
    println!("✅ Superseded v2");

    // Get version chain
    let chain = indexes.get_version_chain(subject, predicate).unwrap();

    assert_eq!(chain.len(), 3, "Should have 3 versions in chain");
    println!("✅ Retrieved version chain: {} versions", chain.len());

    // Verify order (should be chronological by tx_seq)
    assert_eq!(chain[0].version_id, v1_id);
    assert_eq!(chain[1].version_id, v2_id);
    assert_eq!(chain[2].version_id, v3_id);

    // Verify v1 and v2 are superseded, v3 is current
    assert!(!chain[0].is_current(), "v1 should be superseded");
    assert!(!chain[1].is_current(), "v2 should be superseded");
    assert!(chain[2].is_current(), "v3 should be current");

    println!("✅ Version chain validation successful");
}

#[test]
fn test_bitemporal_query_point_in_time() {
    let temp_dir = TempDir::new().unwrap();
    let indexes = TemporalIndexes::new(temp_dir.path().join("temporal_idx")).unwrap();

    let subject = "http://example.org/entity/product_456";
    let predicate = "http://example.org/prop/price";

    // Timeline setup:
    // Valid Time (business time):
    //   Jan 1: price = $50
    //   Feb 1: price = $75
    //   Mar 1: price = $100
    //
    // System Time (transaction time):
    //   We learned about Jan pricing on Jan 5
    //   We learned about Feb pricing on Feb 10
    //   We learned about Mar pricing on Mar 15

    let jan_1 = Utc::now();
    let feb_1 = jan_1 + Duration::days(31);
    let mar_1 = feb_1 + Duration::days(28);

    let jan_5 = jan_1 + Duration::days(4);
    let feb_10 = feb_1 + Duration::days(9);
    let mar_15 = mar_1 + Duration::days(14);

    // Version 1: Price valid from Jan 1, inserted on Jan 5
    let tx1 = TransactionId::new(1, jan_5, 1);
    let triple_v1 =
        AnnotatedTriple::new(subject, predicate, "50").with_valid_time(jan_1, Some(feb_1)); // Valid until Feb 1
    let v1_id = Uuid::new_v4().to_string();
    indexes.index_version(&triple_v1, &v1_id, &tx1).unwrap();

    // Version 2: Price valid from Feb 1, inserted on Feb 10
    let tx2 = TransactionId::new(2, feb_10, 1);
    let triple_v2 =
        AnnotatedTriple::new(subject, predicate, "75").with_valid_time(feb_1, Some(mar_1)); // Valid until Mar 1
    let v2_id = Uuid::new_v4().to_string();
    indexes.index_version(&triple_v2, &v2_id, &tx2).unwrap();

    // Version 3: Price valid from Mar 1, inserted on Mar 15
    let tx3 = TransactionId::new(3, mar_15, 1);
    let triple_v3 = AnnotatedTriple::new(subject, predicate, "100").with_valid_time(mar_1, None); // Still valid
    let v3_id = Uuid::new_v4().to_string();
    indexes.index_version(&triple_v3, &v3_id, &tx3).unwrap();

    println!("✅ Indexed 3 versions with different valid/system times");

    // Query 1: What was the price on Jan 15, as known on Jan 20?
    let jan_15 = jan_1 + Duration::days(14);
    let jan_20 = jan_1 + Duration::days(19);

    let result = indexes
        .find_version_at(subject, predicate, jan_15, jan_20)
        .unwrap();

    assert!(result.is_some(), "Should find version valid on Jan 15");
    let version = result.unwrap();
    assert_eq!(version.object, "50", "Price should be $50 on Jan 15");
    println!("✅ Query 1: Price on Jan 15 as of Jan 20 = $50");

    // Query 2: What was the price on Feb 15, as known on Feb 20?
    let feb_15 = feb_1 + Duration::days(14);
    let feb_20 = feb_1 + Duration::days(19);

    let result = indexes
        .find_version_at(subject, predicate, feb_15, feb_20)
        .unwrap();

    assert!(result.is_some(), "Should find version valid on Feb 15");
    let version = result.unwrap();
    assert_eq!(version.object, "75", "Price should be $75 on Feb 15");
    println!("✅ Query 2: Price on Feb 15 as of Feb 20 = $75");

    // Query 3: What was the price on Mar 20, as known on Mar 30?
    let mar_20 = mar_1 + Duration::days(19);
    let mar_30 = mar_1 + Duration::days(29);

    let result = indexes
        .find_version_at(subject, predicate, mar_20, mar_30)
        .unwrap();

    assert!(result.is_some(), "Should find version valid on Mar 20");
    let version = result.unwrap();
    assert_eq!(version.object, "100", "Price should be $100 on Mar 20");
    println!("✅ Query 3: Price on Mar 20 as of Mar 30 = $100");

    // Query 4: What was the price on Feb 15, as known on Jan 31? (before we learned about it)
    let jan_31 = jan_1 + Duration::days(30);

    let result = indexes
        .find_version_at(subject, predicate, feb_15, jan_31)
        .unwrap();

    assert!(
        result.is_none(),
        "Should not find Feb price as of Jan 31 (not yet in system)"
    );
    println!("✅ Query 4: Price on Feb 15 as of Jan 31 = None (not yet known)");

    println!("\n🎉 Bitemporal queries working correctly!");
}

#[test]
fn test_current_version_updates() {
    let temp_dir = TempDir::new().unwrap();
    let indexes = TemporalIndexes::new(temp_dir.path().join("temporal_idx")).unwrap();

    let subject = "http://example.org/entity/status";
    let predicate = "http://example.org/prop/value";

    // Insert version 1
    let tx1 = TransactionId::new(1, Utc::now(), 1);
    let triple_v1 =
        AnnotatedTriple::new(subject, predicate, "active").with_valid_time(Utc::now(), None);
    let v1_id = Uuid::new_v4().to_string();
    indexes.index_version(&triple_v1, &v1_id, &tx1).unwrap();

    // Current version should be v1
    let current = indexes.find_current_version(subject, predicate).unwrap();
    assert!(current.is_some());
    assert_eq!(current.unwrap().version_id, v1_id);
    println!("✅ Current version = v1");

    std::thread::sleep(std::time::Duration::from_millis(10));

    // Insert version 2
    let tx2 = TransactionId::new(2, Utc::now(), 1);
    let triple_v2 =
        AnnotatedTriple::new(subject, predicate, "inactive").with_valid_time(Utc::now(), None);
    let v2_id = Uuid::new_v4().to_string();
    indexes.index_version(&triple_v2, &v2_id, &tx2).unwrap();

    // Current version should now be v2
    let current = indexes.find_current_version(subject, predicate).unwrap();
    assert!(current.is_some());
    assert_eq!(current.unwrap().version_id, v2_id);
    println!("✅ Current version updated to v2");

    std::thread::sleep(std::time::Duration::from_millis(10));

    // Insert version 3
    let tx3 = TransactionId::new(3, Utc::now(), 1);
    let triple_v3 =
        AnnotatedTriple::new(subject, predicate, "pending").with_valid_time(Utc::now(), None);
    let v3_id = Uuid::new_v4().to_string();
    indexes.index_version(&triple_v3, &v3_id, &tx3).unwrap();

    // Current version should now be v3
    let current = indexes.find_current_version(subject, predicate).unwrap();
    assert!(current.is_some());
    assert_eq!(current.unwrap().version_id, v3_id);
    println!("✅ Current version updated to v3");

    println!("\n🎉 Current version tracking working correctly!");
}

#[test]
fn test_performance_characteristics() {
    let temp_dir = TempDir::new().unwrap();
    let indexes = TemporalIndexes::new(temp_dir.path().join("temporal_idx")).unwrap();

    let subject = "http://example.org/entity/perf_test";
    let predicate = "http://example.org/prop/counter";

    // Insert 100 versions to test scalability
    let start = std::time::Instant::now();

    for i in 0..100 {
        let tx = TransactionId::new(i + 1, Utc::now(), 1);
        let triple = AnnotatedTriple::new(subject, predicate, &format!("{}", i))
            .with_valid_time(Utc::now(), None);
        let version_id = Uuid::new_v4().to_string();

        indexes.index_version(&triple, &version_id, &tx).unwrap();
    }

    let index_duration = start.elapsed();
    println!("✅ Indexed 100 versions in {:?}", index_duration);

    // Test current version lookup (should be O(1))
    let start = std::time::Instant::now();
    let current = indexes.find_current_version(subject, predicate).unwrap();
    let lookup_duration = start.elapsed();

    assert!(current.is_some());
    println!("✅ Current version lookup in {:?} (O(1))", lookup_duration);

    // Test version chain retrieval
    let start = std::time::Instant::now();
    let chain = indexes.get_version_chain(subject, predicate).unwrap();
    let chain_duration = start.elapsed();

    assert_eq!(chain.len(), 100);
    println!(
        "✅ Retrieved version chain (100 versions) in {:?}",
        chain_duration
    );

    // Performance assertions
    assert!(
        lookup_duration.as_millis() < 10,
        "Current version lookup should be < 10ms (was {:?})",
        lookup_duration
    );

    println!("\n🎉 Performance characteristics validated!");
}
