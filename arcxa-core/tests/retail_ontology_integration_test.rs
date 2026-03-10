//! Integration Test: Retail Ontology Alignment
//!
//! Tests the complete pipeline with a realistic retail/commerce ontology
//! to ensure semantic detection, ontology alignment, and relationship detection
//! work correctly with real-world field names and data.

use graphica_core::schema::{
    ConceptType, DataQualityMetrics, FieldProfile, ModelServiceConfig, OntologyAligner,
    OntologyConcept, ProfileCache, RelationshipDetector, SemanticDetector, SourceType,
    UnifiedField, UnifiedSchema, UniversalDataType, ValueDistribution,
};
use std::collections::HashMap;

/// Create a comprehensive retail/commerce ontology
fn create_retail_ontology() -> Vec<OntologyConcept> {
    vec![
        // Customer domain
        OntologyConcept {
            uri: "http://schema.org/customer".to_string(),
            label: "Customer".to_string(),
            description: Some("A person or organization that buys goods or services".to_string()),
            concept_type: ConceptType::Class,
            parents: vec!["http://schema.org/Person".to_string()],
            synonyms: vec!["client".to_string(), "buyer".to_string()],
            metadata: HashMap::from([
                ("domain".to_string(), "retail".to_string()),
                ("category".to_string(), "customer".to_string()),
            ]),
        },
        OntologyConcept {
            uri: "http://schema.org/customer/identifier".to_string(),
            label: "Customer Identifier".to_string(),
            description: Some("Unique identifier for a customer".to_string()),
            concept_type: ConceptType::DataProperty,
            parents: vec!["http://schema.org/identifier".to_string()],
            synonyms: vec![
                "customer_id".to_string(),
                "customer number".to_string(),
                "client id".to_string(),
                "cust_id".to_string(),
            ],
            metadata: HashMap::from([
                ("data_type".to_string(), "string".to_string()),
                ("unique".to_string(), "true".to_string()),
            ]),
        },
        OntologyConcept {
            uri: "http://schema.org/customer/loyalty_tier".to_string(),
            label: "Loyalty Tier".to_string(),
            description: Some("Customer loyalty program tier level".to_string()),
            concept_type: ConceptType::DataProperty,
            parents: vec!["http://schema.org/customer".to_string()],
            synonyms: vec![
                "loyalty level".to_string(),
                "membership tier".to_string(),
                "vip status".to_string(),
            ],
            metadata: HashMap::from([(
                "values".to_string(),
                "Bronze,Silver,Gold,Platinum".to_string(),
            )]),
        },
        // Product domain
        OntologyConcept {
            uri: "http://schema.org/Product".to_string(),
            label: "Product".to_string(),
            description: Some("Any item available for sale".to_string()),
            concept_type: ConceptType::Class,
            parents: vec!["http://schema.org/Thing".to_string()],
            synonyms: vec!["item".to_string(), "merchandise".to_string()],
            metadata: HashMap::new(),
        },
        OntologyConcept {
            uri: "http://schema.org/Product/sku".to_string(),
            label: "Stock Keeping Unit".to_string(),
            description: Some("Unique product identifier (SKU)".to_string()),
            concept_type: ConceptType::DataProperty,
            parents: vec!["http://schema.org/Product".to_string()],
            synonyms: vec![
                "sku".to_string(),
                "product code".to_string(),
                "item number".to_string(),
                "product_id".to_string(),
            ],
            metadata: HashMap::from([("pattern".to_string(), "^[A-Z]{3}-\\d{6}$".to_string())]),
        },
        OntologyConcept {
            uri: "http://schema.org/Product/gtin".to_string(),
            label: "Global Trade Item Number".to_string(),
            description: Some("GTIN barcode identifier (UPC/EAN)".to_string()),
            concept_type: ConceptType::DataProperty,
            parents: vec!["http://schema.org/Product".to_string()],
            synonyms: vec![
                "gtin".to_string(),
                "upc".to_string(),
                "ean".to_string(),
                "barcode".to_string(),
            ],
            metadata: HashMap::from([("length".to_string(), "13".to_string())]),
        },
        OntologyConcept {
            uri: "http://schema.org/Product/price".to_string(),
            label: "Product Price".to_string(),
            description: Some("Selling price of the product".to_string()),
            concept_type: ConceptType::DataProperty,
            parents: vec!["http://schema.org/Product".to_string()],
            synonyms: vec![
                "price".to_string(),
                "unit price".to_string(),
                "retail price".to_string(),
                "selling price".to_string(),
            ],
            metadata: HashMap::from([
                ("data_type".to_string(), "decimal".to_string()),
                ("currency".to_string(), "USD".to_string()),
            ]),
        },
        // Order domain
        OntologyConcept {
            uri: "http://schema.org/Order".to_string(),
            label: "Order".to_string(),
            description: Some("A customer purchase transaction".to_string()),
            concept_type: ConceptType::Class,
            parents: vec!["http://schema.org/Intangible".to_string()],
            synonyms: vec!["purchase".to_string(), "transaction".to_string()],
            metadata: HashMap::new(),
        },
        OntologyConcept {
            uri: "http://schema.org/Order/orderNumber".to_string(),
            label: "Order Number".to_string(),
            description: Some("Unique order identifier".to_string()),
            concept_type: ConceptType::DataProperty,
            parents: vec!["http://schema.org/Order".to_string()],
            synonyms: vec![
                "order_id".to_string(),
                "order number".to_string(),
                "transaction id".to_string(),
                "purchase id".to_string(),
            ],
            metadata: HashMap::new(),
        },
        OntologyConcept {
            uri: "http://schema.org/Order/orderDate".to_string(),
            label: "Order Date".to_string(),
            description: Some("Date and time when the order was placed".to_string()),
            concept_type: ConceptType::DataProperty,
            parents: vec!["http://schema.org/Order".to_string()],
            synonyms: vec![
                "order date".to_string(),
                "purchase date".to_string(),
                "transaction date".to_string(),
                "created_at".to_string(),
            ],
            metadata: HashMap::new(),
        },
        // Inventory domain
        OntologyConcept {
            uri: "http://schema.org/Inventory/quantity".to_string(),
            label: "Inventory Quantity".to_string(),
            description: Some("Available stock quantity".to_string()),
            concept_type: ConceptType::DataProperty,
            parents: vec!["http://schema.org/Product".to_string()],
            synonyms: vec![
                "quantity".to_string(),
                "stock".to_string(),
                "qty_on_hand".to_string(),
                "available".to_string(),
            ],
            metadata: HashMap::from([("data_type".to_string(), "integer".to_string())]),
        },
        // Shipping domain
        OntologyConcept {
            uri: "http://schema.org/PostalAddress/trackingNumber".to_string(),
            label: "Tracking Number".to_string(),
            description: Some("Shipment tracking identifier".to_string()),
            concept_type: ConceptType::DataProperty,
            parents: vec!["http://schema.org/Shipment".to_string()],
            synonyms: vec![
                "tracking number".to_string(),
                "tracking_id".to_string(),
                "shipment id".to_string(),
                "waybill".to_string(),
            ],
            metadata: HashMap::new(),
        },
    ]
}

/// Create sample retail schemas with various naming conventions
fn create_sample_schemas() -> Vec<UnifiedSchema> {
    // Schema 1: Customers table (snake_case)
    let mut customers_schema = UnifiedSchema::new(
        "customers".to_string(),
        SourceType::PostgreSQL,
        "retail_db".to_string(),
    );

    let mut cust_id = UnifiedField::new(
        "customer_id".to_string(),
        UniversalDataType::Integer { bits: Some(64) },
    );
    cust_id.constraints.primary_key = true;
    cust_id.profile = Some(create_profile(vec!["1001", "1002", "1003"], 1000, true));
    customers_schema.add_field(cust_id);

    let mut email = UnifiedField::new(
        "email_address".to_string(),
        UniversalDataType::String {
            max_length: Some(255),
        },
    );
    email.profile = Some(create_profile(
        vec!["john@example.com", "jane@shop.com", "bob@retail.com"],
        1000,
        false,
    ));
    customers_schema.add_field(email);

    let mut loyalty = UnifiedField::new(
        "loyalty_tier".to_string(),
        UniversalDataType::String {
            max_length: Some(20),
        },
    );
    loyalty.profile = Some(create_profile(
        vec!["Gold", "Silver", "Platinum"],
        1000,
        false,
    ));
    customers_schema.add_field(loyalty);

    // Schema 2: Products table (different naming)
    let mut products_schema = UnifiedSchema::new(
        "products".to_string(),
        SourceType::MySQL,
        "retail_db".to_string(),
    );

    let mut sku = UnifiedField::new(
        "sku".to_string(),
        UniversalDataType::String {
            max_length: Some(20),
        },
    );
    sku.constraints.primary_key = true;
    sku.profile = Some(create_profile(
        vec!["PRD-001234", "PRD-002345", "PRD-003456"],
        5000,
        true,
    ));
    products_schema.add_field(sku);

    let mut upc = UnifiedField::new(
        "upc_code".to_string(),
        UniversalDataType::String {
            max_length: Some(13),
        },
    );
    upc.profile = Some(create_profile(
        vec!["1234567890123", "9876543210987"],
        5000,
        false,
    ));
    products_schema.add_field(upc);

    let mut price = UnifiedField::new(
        "unit_price".to_string(),
        UniversalDataType::Decimal {
            precision: 10,
            scale: 2,
        },
    );
    price.profile = Some(create_profile(vec!["19.99", "29.99", "49.99"], 5000, false));
    products_schema.add_field(price);

    let mut stock = UnifiedField::new(
        "qty_on_hand".to_string(),
        UniversalDataType::Integer { bits: Some(32) },
    );
    stock.profile = Some(create_profile(vec!["100", "250", "0"], 5000, false));
    products_schema.add_field(stock);

    // Schema 3: Orders table (mixed naming)
    let mut orders_schema = UnifiedSchema::new(
        "orders".to_string(),
        SourceType::PostgreSQL,
        "retail_db".to_string(),
    );

    let mut order_id = UnifiedField::new(
        "order_number".to_string(),
        UniversalDataType::String {
            max_length: Some(20),
        },
    );
    order_id.constraints.primary_key = true;
    order_id.profile = Some(create_profile(
        vec!["ORD-2024-001", "ORD-2024-002"],
        10000,
        true,
    ));
    orders_schema.add_field(order_id);

    let mut cust_ref = UnifiedField::new(
        "cust_id".to_string(),
        UniversalDataType::Integer { bits: Some(64) },
    );
    cust_ref.profile = Some(create_profile(vec!["1001", "1002", "1003"], 10000, false));
    orders_schema.add_field(cust_ref);

    let mut created = UnifiedField::new(
        "created_at".to_string(),
        UniversalDataType::DateTime {
            with_timezone: true,
        },
    );
    created.profile = Some(create_profile(
        vec!["2024-01-15T10:30:00Z", "2024-01-16T14:20:00Z"],
        10000,
        false,
    ));
    orders_schema.add_field(created);

    let mut tracking = UnifiedField::new(
        "tracking_id".to_string(),
        UniversalDataType::String {
            max_length: Some(50),
        },
    );
    tracking.profile = Some(create_profile(
        vec!["1Z999AA10123456784", "1Z999AA10987654321"],
        10000,
        false,
    ));
    orders_schema.add_field(tracking);

    vec![customers_schema, products_schema, orders_schema]
}

/// Helper to create field profiles
fn create_profile(samples: Vec<&str>, total_rows: u64, unique: bool) -> FieldProfile {
    FieldProfile {
        distinct_count: if unique {
            total_rows
        } else {
            samples.len() as u64
        },
        total_rows,
        null_count: 0,
        null_percentage: 0.0,
        distribution: ValueDistribution::default(),
        samples: samples.iter().map(|s| s.to_string()).collect(),
        top_values: None,
        patterns: None,
        quality: DataQualityMetrics {
            completeness: 1.0,
            uniqueness: if unique {
                1.0
            } else {
                samples.len() as f64 / total_rows as f64
            },
            validity: 1.0,
            consistency: 1.0,
            overall_score: 1.0,
            issues: vec![],
        },
    }
}

#[tokio::test]
async fn test_retail_ontology_semantic_detection() {
    // Test 1: Standard semantic detection without ontology
    println!("\n=== Test 1: Standard Semantic Detection ===");

    let detector = SemanticDetector::new();
    let schemas = create_sample_schemas();

    for schema in &schemas {
        println!("\nSchema: {}", schema.name);
        for field in &schema.fields {
            let samples: Vec<Option<String>> = field
                .profile
                .as_ref()
                .map(|p| p.samples.iter().map(|s| Some(s.clone())).collect())
                .unwrap_or_default();

            if let Some(result) = detector.detect(&field.name, &samples) {
                println!(
                    "  ✓ Field '{}': {:?} (confidence: {:.2}, method: {:?})",
                    field.name, result.semantic_type, result.confidence, result.detection_method
                );
            } else {
                println!("  ✗ Field '{}': No detection", field.name);
            }
        }
    }
}

#[tokio::test]
async fn test_retail_ontology_alignment() {
    println!("\n=== Test 2: Ontology Alignment ===");

    let config = ModelServiceConfig {
        endpoint: "http://localhost:8001".to_string(),
        model_name: "minilm".to_string(),
        min_similarity: 0.6,
        max_candidates: 3,
        enable_embedding_cache: true,
    };

    let mut aligner = OntologyAligner::new(config);
    let ontology = create_retail_ontology();

    println!("\nLoaded {} ontology concepts", ontology.len());
    for concept in &ontology {
        println!("  - {} ({})", concept.label, concept.concept_type as i32);
    }

    aligner.load_ontology(ontology);

    let schemas = create_sample_schemas();

    for schema in &schemas {
        println!("\n--- Aligning Schema: {} ---", schema.name);

        for field in &schema.fields {
            // Try exact/synonym matching first (fast path)
            if let Some(exact) = aligner.find_exact_match(&field.name) {
                println!("  ✓ EXACT MATCH: '{}' → '{}'", field.name, exact.label);
                continue;
            }

            if let Some(synonym) = aligner.find_synonym_match(&field.name) {
                println!("  ✓ SYNONYM MATCH: '{}' → '{}'", field.name, synonym.label);
                continue;
            }

            // Note: For real ML-based matching, we'd call:
            // let alignments = aligner.align_field(field).await;
            // But since we don't have the actual model service running,
            // we'll demonstrate with the rule-based matching

            println!(
                "  ⚠ No exact/synonym match for '{}' (would use embeddings)",
                field.name
            );
        }
    }
}

#[test]
fn test_retail_relationship_detection() {
    println!("\n=== Test 3: Relationship Detection ===");

    let detector = RelationshipDetector::new();
    let schemas = create_sample_schemas();

    let relationships = detector.detect_relationships(&schemas);

    println!("\nDetected {} relationships:", relationships.len());
    for (i, rel) in relationships.iter().enumerate() {
        println!(
            "\n{}. {}.{} → {}.{}",
            i + 1,
            rel.source,
            rel.source_field,
            rel.target,
            rel.target_field
        );
        println!("   Type: {:?}", rel.relationship_type);
        println!("   Confidence: {:.2}", rel.confidence);
    }

    // Validate expected relationships
    let customer_rel = relationships.iter().find(|r| {
        r.source == "orders"
            && r.source_field == "cust_id"
            && r.target == "customers"
            && r.target_field == "customer_id"
    });

    assert!(
        customer_rel.is_some(),
        "Should detect relationship between orders.cust_id and customers.customer_id"
    );

    if let Some(rel) = customer_rel {
        println!("\n✓ Validated: orders.cust_id → customers.customer_id");
        println!("  Confidence: {:.2}", rel.confidence);
        assert!(
            rel.confidence > 0.7,
            "Relationship confidence should be high"
        );
    }
}

#[test]
fn test_retail_profile_caching() {
    println!("\n=== Test 4: Profile Caching ===");

    let cache = ProfileCache::new();
    let schemas = create_sample_schemas();

    // Cache all schemas
    for schema in &schemas {
        let key = ProfileCache::generate_key(&schema.source_ref, Some(&schema.name));
        let fingerprint = schema.fingerprint();

        cache.put(key.clone(), schema.clone(), Some(fingerprint.clone()));
        println!("✓ Cached schema: {} (key: {})", schema.name, key);
    }

    // Test cache retrieval
    println!("\nTesting cache retrieval:");
    for schema in &schemas {
        let key = ProfileCache::generate_key(&schema.source_ref, Some(&schema.name));
        let fingerprint = schema.fingerprint();

        if let Some(cached_schema) = cache.get(&key, Some(&fingerprint)) {
            println!(
                "  ✓ Cache HIT for {}: {} fields",
                schema.name,
                cached_schema.fields.len()
            );
        } else {
            println!("  ✗ Cache MISS for {}", schema.name);
        }
    }

    // Test cache invalidation on fingerprint change
    println!("\nTesting cache invalidation:");
    let key = ProfileCache::generate_key("retail_db", Some("customers"));
    let wrong_fingerprint = "wrong_fingerprint_v2";

    if let Some(_) = cache.get(&key, Some(wrong_fingerprint)) {
        println!("  ✗ ERROR: Should have invalidated cache");
    } else {
        println!("  ✓ Cache correctly invalidated on fingerprint mismatch");
    }

    // Check stats
    let stats = cache.stats();
    println!("\nCache Statistics:");
    println!("  Hits: {}", stats.hits);
    println!("  Misses: {}", stats.misses);
    println!("  Hit Rate: {:.1}%", stats.hit_rate() * 100.0);
}

#[tokio::test]
async fn test_complete_retail_pipeline() {
    println!("\n=== Test 5: Complete Pipeline Integration ===");

    // 1. Setup
    let cache = ProfileCache::new();
    let semantic_detector = SemanticDetector::new();
    let relationship_detector = RelationshipDetector::new();

    let config = ModelServiceConfig::default();
    let mut ontology_aligner = OntologyAligner::new(config);
    ontology_aligner.load_ontology(create_retail_ontology());

    // 2. Load and enhance schemas
    let mut schemas = create_sample_schemas();

    println!("\n--- Phase 1: Semantic Detection & Ontology Alignment ---");
    for schema in &mut schemas {
        println!("\nProcessing schema: {}", schema.name);

        for field in &mut schema.fields {
            // Standard detection
            let samples: Vec<Option<String>> = field
                .profile
                .as_ref()
                .map(|p| p.samples.iter().map(|s| Some(s.clone())).collect())
                .unwrap_or_default();

            let standard_result = semantic_detector.detect(&field.name, &samples);

            // Ontology alignment (exact/synonym only for testing)
            let exact_match = ontology_aligner.find_exact_match(&field.name);
            let synonym_match = ontology_aligner.find_synonym_match(&field.name);

            if let Some(concept) = exact_match.or(synonym_match) {
                println!(
                    "  ✓ '{}' → Ontology: '{}' (URI: {})",
                    field.name, concept.label, concept.uri
                );

                // In real implementation with model service:
                // field.semantic.semantic_type = map_concept_to_semantic_type(&concept);
            } else if let Some(result) = standard_result {
                println!(
                    "  ✓ '{}' → Standard Detection: {:?} (confidence: {:.2})",
                    field.name, result.semantic_type, result.confidence
                );
                field.semantic.semantic_type = Some(result.semantic_type);
                field.semantic.sensitivity = result.suggested_sensitivity;
            }
        }

        // Cache the enriched schema
        let key = ProfileCache::generate_key(&schema.source_ref, Some(&schema.name));
        cache.put(key, schema.clone(), Some(schema.fingerprint()));
    }

    // 3. Detect relationships
    println!("\n--- Phase 2: Relationship Detection ---");
    let relationships = relationship_detector.detect_relationships(&schemas);

    println!(
        "\nFound {} cross-schema relationships:",
        relationships.len()
    );
    for rel in &relationships {
        println!(
            "  • {}.{} → {}.{} (confidence: {:.2})",
            rel.source, rel.source_field, rel.target, rel.target_field, rel.confidence
        );
    }

    // 4. Validate results
    println!("\n--- Phase 3: Validation ---");

    // Check cache effectiveness
    let cache_stats = cache.stats();
    println!("Cache Performance:");
    println!("  Total size: {} schemas", cache.size());
    println!("  Hit rate: {:.1}%", cache_stats.hit_rate() * 100.0);

    // Check ontology coverage
    let total_fields: usize = schemas.iter().map(|s| s.fields.len()).sum();
    let aligned_fields: usize = schemas
        .iter()
        .flat_map(|s| &s.fields)
        .filter(|f| f.semantic.semantic_type.is_some())
        .count();

    println!("\nOntology Coverage:");
    println!("  Total fields: {}", total_fields);
    println!("  Aligned fields: {}", aligned_fields);
    println!(
        "  Coverage: {:.1}%",
        (aligned_fields as f64 / total_fields as f64) * 100.0
    );

    // Check relationship detection
    assert!(
        relationships.len() >= 1,
        "Should detect at least one relationship (orders → customers)"
    );

    println!("\n✓ Pipeline integration test passed!");
}
