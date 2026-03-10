//! Performance benchmarks for Field Lineage
//!
//! Tests high-throughput golden record creation and RDF persistence.

use chrono::Utc;
use graphica_core::orchestration::field_lineage::{FieldResolver, SourceValue, StrategyType};
use std::collections::HashMap;
use std::time::Instant;

/// Benchmark: Create 100 golden records with frequency voting
/// Target: 50+ golden records/sec
#[test]
fn bench_golden_record_creation_frequency() {
    let resolver = FieldResolver::new();
    let num_records = 100;

    let start = Instant::now();

    for i in 0..num_records {
        let entity_id = format!("customer_{}", i);

        // Create realistic source values
        let email_sources = create_email_sources(i);
        let name_sources = create_name_sources(i);
        let address_sources = create_address_sources(i);

        let mut fields = HashMap::new();
        fields.insert("email".to_string(), email_sources);
        fields.insert("name".to_string(), name_sources);
        fields.insert("address".to_string(), address_sources);

        // Resolve fields
        let resolutions = resolver
            .resolve_fields(&entity_id, fields, None)
            .expect("Field resolution should succeed");

        // Create golden record
        let _golden_record = resolver
            .create_resolved_entity(&entity_id, resolutions)
            .expect("Golden record creation should succeed");

        // In production, this would also persist to RDF via rdf_store.update()
    }

    let elapsed = start.elapsed();
    let records_per_sec = num_records as f64 / elapsed.as_secs_f64();

    println!("\n=== Golden Record Creation Performance ===");
    println!("Total records: {}", num_records);
    println!("Total time: {:?}", elapsed);
    println!("Throughput: {:.2} records/sec", records_per_sec);
    println!(
        "Avg latency: {:.2} ms/record",
        elapsed.as_millis() as f64 / num_records as f64
    );

    // Assert we meet performance target
    assert!(
        records_per_sec >= 50.0,
        "Performance target not met: {:.2} records/sec (target: 50+)",
        records_per_sec
    );
}

/// Benchmark: Create golden records with authority voting
#[test]
fn bench_golden_record_creation_authority() {
    let resolver = FieldResolver::with_strategy(StrategyType::Authority);
    let num_records = 100;

    let start = Instant::now();

    for i in 0..num_records {
        let entity_id = format!("customer_{}", i);

        let mut fields = HashMap::new();
        fields.insert("email".to_string(), create_email_sources(i));
        fields.insert("name".to_string(), create_name_sources(i));

        let resolutions = resolver
            .resolve_fields(&entity_id, fields, None)
            .expect("Resolution should succeed");

        let _golden_record = resolver
            .create_resolved_entity(&entity_id, resolutions)
            .expect("Golden record creation should succeed");
    }

    let elapsed = start.elapsed();
    let records_per_sec = num_records as f64 / elapsed.as_secs_f64();

    println!("\n=== Authority Voting Performance ===");
    println!("Throughput: {:.2} records/sec", records_per_sec);
    println!(
        "Avg latency: {:.2} ms/record",
        elapsed.as_millis() as f64 / num_records as f64
    );

    assert!(
        records_per_sec >= 50.0,
        "Performance target not met: {:.2} records/sec",
        records_per_sec
    );
}

/// Benchmark: SPARQL generation overhead
#[test]
fn bench_sparql_generation() {
    let resolver = FieldResolver::new();
    let num_records = 100;

    // Create golden records first
    let mut golden_records = Vec::new();
    for i in 0..num_records {
        let entity_id = format!("customer_{}", i);
        let mut fields = HashMap::new();
        fields.insert("email".to_string(), create_email_sources(i));
        fields.insert("name".to_string(), create_name_sources(i));

        let resolutions = resolver.resolve_fields(&entity_id, fields, None).unwrap();
        let golden_record = resolver
            .create_resolved_entity(&entity_id, resolutions)
            .unwrap();
        golden_records.push(golden_record);
    }

    // Benchmark SPARQL generation
    let start = Instant::now();

    for gr in &golden_records {
        let _sparql = resolver.resolved_entity_to_sparql(gr);
        // In production: rdf_store.update(&sparql)
    }

    let elapsed = start.elapsed();
    let sparql_per_sec = num_records as f64 / elapsed.as_secs_f64();

    println!("\n=== SPARQL Generation Performance ===");
    println!("Throughput: {:.2} SPARQL/sec", sparql_per_sec);
    println!(
        "Avg latency: {:.2} ms/query",
        elapsed.as_millis() as f64 / num_records as f64
    );

    // SPARQL generation should be very fast (>500/sec)
    assert!(
        sparql_per_sec >= 500.0,
        "SPARQL generation too slow: {:.2}/sec",
        sparql_per_sec
    );
}

/// Benchmark: Field resolution with many sources
#[test]
fn bench_field_resolution_many_sources() {
    let resolver = FieldResolver::new();
    let num_resolutions = 1000;

    let start = Instant::now();

    for i in 0..num_resolutions {
        // 10 source values per field (realistic for entity fusion)
        let sources = (0..10)
            .map(|j| SourceValue {
                id: format!("src_{}_{}", i, j),
                value: serde_json::json!(format!("value_{}", j % 3)), // 3 distinct values
                source_system: format!("System{}", j),
                source_timestamp: Utc::now(),
                source_authority: 0.5 + (j as f64 * 0.05),
                confidence: Some(0.8),
                vote_count: 0,
                vote_weight: 1.0,
                metadata: HashMap::new(),
            })
            .collect();

        let _resolution = resolver
            .resolve_field("entity_123", "field_name", sources, None)
            .expect("Resolution should succeed");
    }

    let elapsed = start.elapsed();
    let resolutions_per_sec = num_resolutions as f64 / elapsed.as_secs_f64();

    println!("\n=== Field Resolution Performance (10 sources) ===");
    println!("Throughput: {:.2} resolutions/sec", resolutions_per_sec);
    println!(
        "Avg latency: {:.2} ms/resolution",
        elapsed.as_millis() as f64 / num_resolutions as f64
    );

    // Should handle 1000+ resolutions/sec
    assert!(
        resolutions_per_sec >= 1000.0,
        "Resolution performance too slow: {:.2}/sec",
        resolutions_per_sec
    );
}

// Helper functions to create realistic test data

fn create_email_sources(index: usize) -> Vec<SourceValue> {
    vec![
        SourceValue {
            id: format!("email_crm_{}", index),
            value: serde_json::json!(format!("user{}@example.com", index)),
            source_system: "CRM".to_string(),
            source_timestamp: Utc::now(),
            source_authority: 0.95,
            confidence: Some(0.98),
            vote_count: 0,
            vote_weight: 1.0,
            metadata: HashMap::new(),
        },
        SourceValue {
            id: format!("email_web_{}", index),
            value: serde_json::json!(format!("user{}@example.com", index)),
            source_system: "Website".to_string(),
            source_timestamp: Utc::now(),
            source_authority: 0.7,
            confidence: Some(0.85),
            vote_count: 0,
            vote_weight: 1.0,
            metadata: HashMap::new(),
        },
        SourceValue {
            id: format!("email_old_{}", index),
            value: serde_json::json!(format!("olduser{}@example.com", index)),
            source_system: "LegacyDB".to_string(),
            source_timestamp: Utc::now(),
            source_authority: 0.4,
            confidence: Some(0.6),
            vote_count: 0,
            vote_weight: 1.0,
            metadata: HashMap::new(),
        },
    ]
}

fn create_name_sources(index: usize) -> Vec<SourceValue> {
    vec![
        SourceValue {
            id: format!("name_crm_{}", index),
            value: serde_json::json!(format!("User {}", index)),
            source_system: "CRM".to_string(),
            source_timestamp: Utc::now(),
            source_authority: 0.9,
            confidence: Some(0.95),
            vote_count: 0,
            vote_weight: 1.0,
            metadata: HashMap::new(),
        },
        SourceValue {
            id: format!("name_erp_{}", index),
            value: serde_json::json!(format!("User {}", index)),
            source_system: "ERP".to_string(),
            source_timestamp: Utc::now(),
            source_authority: 0.85,
            confidence: Some(0.90),
            vote_count: 0,
            vote_weight: 1.0,
            metadata: HashMap::new(),
        },
    ]
}

fn create_address_sources(index: usize) -> Vec<SourceValue> {
    vec![
        SourceValue {
            id: format!("addr_shipping_{}", index),
            value: serde_json::json!(format!("{} Main St", index)),
            source_system: "Shipping".to_string(),
            source_timestamp: Utc::now(),
            source_authority: 0.8,
            confidence: Some(0.85),
            vote_count: 0,
            vote_weight: 1.0,
            metadata: HashMap::new(),
        },
        SourceValue {
            id: format!("addr_billing_{}", index),
            value: serde_json::json!(format!("{} Oak Ave", index)),
            source_system: "Billing".to_string(),
            source_timestamp: Utc::now(),
            source_authority: 0.75,
            confidence: Some(0.80),
            vote_count: 0,
            vote_weight: 1.0,
            metadata: HashMap::new(),
        },
    ]
}
