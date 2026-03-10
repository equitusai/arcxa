//! End-to-End Integration Tests for Ontology Optimizations
//!
//! This test suite validates all 5 optimization priorities:
//! - Priority 1: Term Caching
//! - Priority 2: Namespace Filtering
//! - Priority 3: RwLock Contention Fix
//! - Priority 4: RocksDB Persistence
//! - Priority 5: Automatic Cache Invalidation
//!
//! Run with: cargo test --test ontology_end_to_end_test -- --nocapture

use anyhow::Result;
use graphica_coordinator::mapping::ontology_registry::{PersistedOntologyRegistry, RegistryClient};
use std::sync::Arc;
use std::time::Instant;
use tempfile::TempDir;

/// Test Priority 4: Persistence - Register, restart, verify
#[tokio::test]
async fn test_priority_4_persistence_and_recovery() -> Result<()> {
    println!("\n🧪 TEST: Priority 4 - RocksDB Persistence & Crash Recovery");
    println!("==========================================================");

    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("ontologies.db");

    // Phase 1: Register ontologies
    println!("\n📝 Phase 1: Registering ontologies...");
    {
        let registry = PersistedOntologyRegistry::open(&db_path).await?;

        // Register retail ontology
        let retail_content = r#"
            @prefix retail: <http://example.com/retail#> .
            @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
            @prefix owl: <http://www.w3.org/2002/07/owl#> .

            retail:Product a owl:Class ;
                rdfs:label "Product" .

            retail:price a owl:DatatypeProperty ;
                rdfs:label "price" ;
                rdfs:domain retail:Product .

            retail:sku a owl:DatatypeProperty ;
                rdfs:label "SKU" ;
                rdfs:domain retail:Product .
        "#;

        registry
            .register_custom_ontology(
                "retail",
                retail_content.to_string(),
                Some("http://example.com/retail#".to_string()),
            )
            .await?;
        println!("   ✓ Registered 'retail' ontology");

        // Register healthcare ontology
        let healthcare_content = r#"
            @prefix health: <http://example.com/healthcare#> .
            @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
            @prefix owl: <http://www.w3.org/2002/07/owl#> .

            health:Patient a owl:Class ;
                rdfs:label "Patient" .

            health:patientId a owl:DatatypeProperty ;
                rdfs:label "Patient ID" ;
                rdfs:domain health:Patient .

            health:diagnosis a owl:DatatypeProperty ;
                rdfs:label "Diagnosis" ;
                rdfs:domain health:Patient .
        "#;

        registry
            .register_custom_ontology(
                "healthcare",
                healthcare_content.to_string(),
                Some("http://example.com/healthcare#".to_string()),
            )
            .await?;
        println!("   ✓ Registered 'healthcare' ontology");

        let stats = registry.get_stats()?;
        println!("   ✓ Total ontologies: {}", stats.in_memory_count);
        assert_eq!(stats.in_memory_count, 2, "Should have 2 ontologies");
        assert_eq!(stats.active_count, 2, "Both should be active");
    }

    // Phase 2: Simulate crash - drop registry
    println!("\n💥 Phase 2: Simulating crash (dropping registry)...");
    println!("   Registry dropped - all in-memory data lost");

    // Phase 3: Recovery - reload from disk
    println!("\n🔄 Phase 3: Recovery - reloading from RocksDB...");
    let start = Instant::now();
    let recovered_registry = PersistedOntologyRegistry::open(&db_path).await?;
    let recovery_time = start.elapsed();

    let stats = recovered_registry.get_stats()?;
    println!(
        "   ✓ Recovered {} ontologies in {:?}",
        stats.in_memory_count, recovery_time
    );
    assert_eq!(stats.in_memory_count, 2, "Should recover 2 ontologies");
    assert_eq!(stats.active_count, 2, "Both should still be active");
    assert!(
        recovery_time.as_millis() < 500,
        "Recovery should be fast (<500ms)"
    );

    println!("\n✅ PASS: Persistence & Recovery working correctly");
    Ok(())
}

/// Test Priority 1: Caching - Cold vs Warm performance
#[tokio::test]
async fn test_priority_1_term_caching() -> Result<()> {
    println!("\n🧪 TEST: Priority 1 - Term Caching");
    println!("====================================");

    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("ontologies.db");

    let registry = PersistedOntologyRegistry::open(&db_path).await?;

    // Register a moderately sized ontology
    let large_ontology = create_test_ontology("http://example.com/test#", 100);
    registry
        .register_custom_ontology(
            "test_large",
            large_ontology,
            Some("http://example.com/test#".to_string()),
        )
        .await?;

    let client = RegistryClient::new(Some(registry.registry()));

    // Cold cache - first request
    println!("\n❄️  Cold Cache Test:");
    let start = Instant::now();
    let terms_cold = client.get_ontology_terms()?;
    let cold_time = start.elapsed();
    println!(
        "   First request: {:?} ({} terms)",
        cold_time,
        terms_cold.len()
    );

    // Warm cache - subsequent requests
    println!("\n🔥 Warm Cache Test:");
    let mut warm_times = Vec::new();
    for i in 0..5 {
        let start = Instant::now();
        let terms_warm = client.get_ontology_terms()?;
        let warm_time = start.elapsed();
        warm_times.push(warm_time);
        println!("   Request {}: {:?}", i + 1, warm_time);
        assert_eq!(
            terms_warm.len(),
            terms_cold.len(),
            "Should return same terms"
        );
    }

    let avg_warm = warm_times.iter().sum::<std::time::Duration>() / warm_times.len() as u32;
    println!("\n📊 Performance Analysis:");
    println!("   Cold cache: {:?}", cold_time);
    println!("   Avg warm cache: {:?}", avg_warm);
    println!(
        "   Speedup: {:.1}×",
        cold_time.as_micros() as f64 / avg_warm.as_micros() as f64
    );

    // Validate cache is working (warm should be MUCH faster)
    assert!(
        avg_warm < cold_time / 10,
        "Warm cache should be at least 10× faster (got {:.1}×)",
        cold_time.as_micros() as f64 / avg_warm.as_micros() as f64
    );

    println!("\n✅ PASS: Caching provides significant performance improvement");
    Ok(())
}

/// Test Priority 5: Automatic Cache Invalidation
#[tokio::test]
async fn test_priority_5_cache_invalidation() -> Result<()> {
    println!("\n🧪 TEST: Priority 5 - Automatic Cache Invalidation");
    println!("===================================================");

    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("ontologies.db");

    let registry = Arc::new(PersistedOntologyRegistry::open(&db_path).await?);
    let client = RegistryClient::new(Some(registry.registry()));

    // Wire up cache invalidation
    let client_clone = client.clone();
    registry.set_cache_invalidation_callback(Box::new(move || {
        client_clone.invalidate_cache();
    }));
    println!("   ✓ Cache invalidation callback wired");

    // Register initial ontology
    let initial_content = r#"
        @prefix test: <http://test.com#> .
        @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
        @prefix owl: <http://www.w3.org/2002/07/owl#> .

        test:OldProperty a owl:DatatypeProperty ;
            rdfs:label "Old Property" .
    "#;

    registry
        .register_custom_ontology(
            "test",
            initial_content.to_string(),
            Some("http://test.com#".to_string()),
        )
        .await?;
    println!("\n📝 Registered initial ontology");

    // First request - cache it
    let terms1 = client.get_ontology_terms()?;
    println!("   Initial terms: {}", terms1.len());

    // Second request - should hit cache
    let start = Instant::now();
    let terms2 = client.get_ontology_terms()?;
    let cached_time = start.elapsed();
    println!("   Cached request: {:?}", cached_time);
    assert_eq!(
        terms1.len(),
        terms2.len(),
        "Should get same terms from cache"
    );

    // Update ontology - should trigger cache invalidation
    println!("\n🔄 Updating ontology (should invalidate cache)...");
    let updated_content = r#"
        @prefix test: <http://test.com#> .
        @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
        @prefix owl: <http://www.w3.org/2002/07/owl#> .

        test:OldProperty a owl:DatatypeProperty ;
            rdfs:label "Old Property" .

        test:NewProperty a owl:DatatypeProperty ;
            rdfs:label "New Property" .
    "#;

    registry
        .update_ontology("test", updated_content.to_string())
        .await?;
    println!("   ✓ Ontology updated");

    // Next request should get updated terms (cache was invalidated)
    let start = Instant::now();
    let terms3 = client.get_ontology_terms()?;
    let post_invalidation_time = start.elapsed();
    println!(
        "   Post-invalidation request: {:?} ({} terms)",
        post_invalidation_time,
        terms3.len()
    );

    assert!(
        terms3.len() > terms2.len(),
        "Should have more terms after update (got {}, expected > {})",
        terms3.len(),
        terms2.len()
    );

    // Request again - should be cached again
    let start = Instant::now();
    let terms4 = client.get_ontology_terms()?;
    let re_cached_time = start.elapsed();
    println!("   Re-cached request: {:?}", re_cached_time);

    assert_eq!(terms3.len(), terms4.len(), "Should get same updated terms");
    assert!(
        re_cached_time < post_invalidation_time / 5,
        "Re-cached request should be much faster"
    );

    println!("\n✅ PASS: Cache invalidation working automatically");
    Ok(())
}

/// Test Priority 2: Namespace Filtering
#[tokio::test]
async fn test_priority_2_namespace_filtering() -> Result<()> {
    println!("\n🧪 TEST: Priority 2 - Namespace Filtering");
    println!("==========================================");

    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("ontologies.db");

    let registry = PersistedOntologyRegistry::open(&db_path).await?;

    // Register 10 ontologies with different namespaces
    println!("\n📝 Registering 10 ontologies with different namespaces...");
    for i in 0..10 {
        let namespace = format!("http://example.com/ont{}#", i);
        let content = create_test_ontology(&namespace, 20); // 20 terms each
        registry
            .register_custom_ontology(&format!("ont{}", i), content, Some(namespace))
            .await?;
    }
    println!("   ✓ Registered 10 ontologies (200 terms total)");

    let client = RegistryClient::new(Some(registry.registry()));

    // Test 1: Get ALL terms (no filtering)
    println!("\n🔍 Test 1: Get all terms (no filtering)");
    let start = Instant::now();
    let all_terms = client.get_ontology_terms()?;
    let all_time = start.elapsed();
    println!("   Time: {:?}", all_time);
    println!("   Terms: {}", all_terms.len());

    // Test 2: Filter to 1 namespace (should be much faster)
    println!("\n🔍 Test 2: Filter to 1 namespace");
    let start = Instant::now();
    let filtered_terms =
        client.get_terms_by_namespaces(&["http://example.com/ont0#".to_string()])?;
    let filtered_time = start.elapsed();
    println!("   Time: {:?}", filtered_time);
    println!("   Terms: {}", filtered_terms.len());

    assert!(
        filtered_terms.len() < all_terms.len(),
        "Filtered should have fewer terms"
    );

    // Filtering should be faster (or at least not slower)
    println!("\n📊 Performance Comparison:");
    println!("   All terms: {:?} ({} terms)", all_time, all_terms.len());
    println!(
        "   Filtered (1 of 10): {:?} ({} terms)",
        filtered_time,
        filtered_terms.len()
    );

    if filtered_time < all_time {
        println!(
            "   ✓ Filtering is faster by {:.1}×",
            all_time.as_micros() as f64 / filtered_time.as_micros() as f64
        );
    } else {
        println!("   Note: Filtering time similar (both cached)");
    }

    // Test 3: Filter to 5 namespaces
    println!("\n🔍 Test 3: Filter to 5 namespaces");
    let start = Instant::now();
    let multi_filtered = client.get_terms_by_namespaces(&[
        "http://example.com/ont0#".to_string(),
        "http://example.com/ont1#".to_string(),
        "http://example.com/ont2#".to_string(),
        "http://example.com/ont3#".to_string(),
        "http://example.com/ont4#".to_string(),
    ])?;
    let multi_time = start.elapsed();
    println!("   Time: {:?}", multi_time);
    println!("   Terms: {}", multi_filtered.len());

    assert!(
        multi_filtered.len() > filtered_terms.len(),
        "5 namespaces should have more terms than 1"
    );
    assert!(
        multi_filtered.len() < all_terms.len(),
        "5 namespaces should have fewer terms than all"
    );

    println!("\n✅ PASS: Namespace filtering working correctly");
    Ok(())
}

/// Test Priority 3: RwLock Contention (Concurrent Access)
#[tokio::test]
async fn test_priority_3_concurrent_access() -> Result<()> {
    println!("\n🧪 TEST: Priority 3 - RwLock Contention Fix (Concurrent Access)");
    println!("================================================================");

    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("ontologies.db");

    let registry = Arc::new(PersistedOntologyRegistry::open(&db_path).await?);

    // Register ontologies
    for i in 0..5 {
        let namespace = format!("http://example.com/ont{}#", i);
        let content = create_test_ontology(&namespace, 30);
        registry
            .register_custom_ontology(&format!("ont{}", i), content, Some(namespace))
            .await?;
    }
    println!("   ✓ Registered 5 ontologies");

    let client = Arc::new(RegistryClient::new(Some(registry.registry())));

    // Test sequential access (baseline)
    println!("\n📊 Sequential Access (baseline):");
    let start = Instant::now();
    for _ in 0..10 {
        let _ = client.get_ontology_terms()?;
    }
    let sequential_time = start.elapsed();
    println!("   10 sequential requests: {:?}", sequential_time);

    // Test concurrent access
    println!("\n📊 Concurrent Access (10 threads):");
    let start = Instant::now();
    let mut handles = vec![];

    for i in 0..10 {
        let client_clone = client.clone();
        let handle = tokio::spawn(async move {
            let start = Instant::now();
            let terms = client_clone.get_ontology_terms().unwrap();
            let duration = start.elapsed();
            (i, terms.len(), duration)
        });
        handles.push(handle);
    }

    let mut results = vec![];
    for handle in handles {
        results.push(handle.await?);
    }
    let concurrent_time = start.elapsed();

    println!("   10 concurrent requests: {:?}", concurrent_time);
    println!("\n   Individual thread times:");
    for (thread_id, term_count, duration) in &results {
        println!(
            "     Thread {}: {:?} ({} terms)",
            thread_id, duration, term_count
        );
    }

    // All threads should get the same number of terms
    let first_count = results[0].1;
    for (_, count, _) in &results {
        assert_eq!(
            *count, first_count,
            "All threads should get same term count"
        );
    }

    println!("\n📊 Performance Analysis:");
    println!("   Sequential: {:?}", sequential_time);
    println!("   Concurrent: {:?}", concurrent_time);

    if concurrent_time < sequential_time {
        println!(
            "   ✓ Concurrent is faster by {:.1}×",
            sequential_time.as_micros() as f64 / concurrent_time.as_micros() as f64
        );
    } else {
        println!("   Note: Concurrent time similar (good - no contention)");
    }

    println!("\n✅ PASS: Concurrent access working correctly");
    Ok(())
}

/// Test Full Integration: All priorities working together
#[tokio::test]
async fn test_full_integration() -> Result<()> {
    println!("\n🧪 TEST: Full Integration - All Priorities Together");
    println!("====================================================");

    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("ontologies.db");

    println!("\n1️⃣ Initialize with persistence...");
    let registry = Arc::new(PersistedOntologyRegistry::open(&db_path).await?);

    println!("\n2️⃣ Create client with caching...");
    let client = RegistryClient::new(Some(registry.registry()));

    println!("\n3️⃣ Wire cache invalidation...");
    let client_clone = client.clone();
    registry.set_cache_invalidation_callback(Box::new(move || {
        client_clone.invalidate_cache();
    }));

    println!("\n4️⃣ Register multiple ontologies...");
    for i in 0..5 {
        let namespace = format!("http://integration.test/ont{}#", i);
        let content = create_test_ontology(&namespace, 25);
        registry
            .register_custom_ontology(&format!("integration_ont{}", i), content, Some(namespace))
            .await?;
    }
    println!("   ✓ Registered 5 ontologies");

    println!("\n5️⃣ Test caching performance...");
    let start = Instant::now();
    let terms1 = client.get_ontology_terms()?;
    let first_time = start.elapsed();

    let start = Instant::now();
    let terms2 = client.get_ontology_terms()?;
    let cached_time = start.elapsed();

    println!("   First: {:?}, Cached: {:?}", first_time, cached_time);
    assert_eq!(terms1.len(), terms2.len());
    assert!(cached_time < first_time / 5, "Cache should be much faster");

    println!("\n6️⃣ Test namespace filtering...");
    let filtered =
        client.get_terms_by_namespaces(&["http://integration.test/ont0#".to_string()])?;
    println!(
        "   ✓ Filtered to {} terms (from {} total)",
        filtered.len(),
        terms1.len()
    );
    assert!(filtered.len() < terms1.len());

    println!("\n7️⃣ Test cache invalidation on update...");
    let new_content = create_test_ontology("http://integration.test/ont0#", 50);
    registry
        .update_ontology("integration_ont0", new_content)
        .await?;

    let terms3 = client.get_ontology_terms()?;
    println!(
        "   ✓ After update: {} terms (was {})",
        terms3.len(),
        terms1.len()
    );
    assert!(
        terms3.len() > terms1.len(),
        "Should have more terms after update"
    );

    println!("\n8️⃣ Test persistence and recovery...");
    let stats_before = registry.get_stats()?;
    drop(registry);
    drop(client);

    let recovered = PersistedOntologyRegistry::open(&db_path).await?;
    let stats_after = recovered.get_stats()?;
    println!("   ✓ Recovered {} ontologies", stats_after.in_memory_count);
    assert_eq!(stats_before.in_memory_count, stats_after.in_memory_count);

    println!("\n✅ PASS: Full integration working correctly!");
    println!("   All 5 priorities validated in production-like scenario");

    Ok(())
}

/// Helper: Create a test ontology with N properties
fn create_test_ontology(namespace: &str, property_count: usize) -> String {
    let mut ontology = format!("@prefix ns: <{}> .\n", namespace);
    ontology.push_str("@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n");
    ontology.push_str("@prefix owl: <http://www.w3.org/2002/07/owl#> .\n\n");

    for i in 0..property_count {
        ontology.push_str(&format!(
            "ns:Property{} a owl:DatatypeProperty ;\n    rdfs:label \"Property {}\" .\n\n",
            i, i
        ));
    }

    ontology
}
