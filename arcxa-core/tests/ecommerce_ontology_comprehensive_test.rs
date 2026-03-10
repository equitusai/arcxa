//! Comprehensive Ecommerce Ontology Embedding Test
//!
//! Tests the portable TF-IDF embeddings with a large, real-world ontology
//! to ensure robustness, edge case handling, and scalability.

use graphica_core::schema::embeddings::{cosine_similarity, PretrainedEmbeddings};
use graphica_core::schema::{
    ConceptType, ModelServiceConfig, OntologyAligner, OntologyConcept, SourceType, UnifiedField,
    UnifiedSchema, UniversalDataType,
};
use std::collections::HashMap;

/// Parse the ecommerce.rdf ontology file
fn load_ecommerce_ontology() -> Vec<OntologyConcept> {
    // In a real implementation, we'd parse the RDF file
    // For testing, we'll create a comprehensive set of concepts manually
    vec![
        // Customer Domain
        OntologyConcept {
            uri: "http://equitus.ai/ontology/ecommerce#customerId".to_string(),
            label: "customer ID".to_string(),
            description: Some("Unique identifier for a customer".to_string()),
            concept_type: ConceptType::DataProperty,
            synonyms: vec!["cust_id".to_string(), "customer_identifier".to_string()],
            parents: vec!["http://equitus.ai/ontology/ecommerce#Customer".to_string()],
            metadata: HashMap::new(),
        },
        OntologyConcept {
            uri: "http://equitus.ai/ontology/ecommerce#firstName".to_string(),
            label: "first name".to_string(),
            description: Some("Customer's given name".to_string()),
            concept_type: ConceptType::DataProperty,
            synonyms: vec!["given_name".to_string(), "fname".to_string()],
            parents: vec![],
            metadata: HashMap::new(),
        },
        OntologyConcept {
            uri: "http://equitus.ai/ontology/ecommerce#lastName".to_string(),
            label: "last name".to_string(),
            description: Some("Customer's family name".to_string()),
            concept_type: ConceptType::DataProperty,
            synonyms: vec![
                "surname".to_string(),
                "family_name".to_string(),
                "lname".to_string(),
            ],
            parents: vec![],
            metadata: HashMap::new(),
        },
        OntologyConcept {
            uri: "http://equitus.ai/ontology/ecommerce#email".to_string(),
            label: "email".to_string(),
            description: Some("Email address".to_string()),
            concept_type: ConceptType::DataProperty,
            synonyms: vec!["email_address".to_string(), "e_mail".to_string()],
            parents: vec![],
            metadata: HashMap::new(),
        },
        OntologyConcept {
            uri: "http://equitus.ai/ontology/ecommerce#phoneNumber".to_string(),
            label: "phone number".to_string(),
            description: Some("Contact telephone number".to_string()),
            concept_type: ConceptType::DataProperty,
            synonyms: vec![
                "telephone".to_string(),
                "phone".to_string(),
                "mobile".to_string(),
            ],
            parents: vec![],
            metadata: HashMap::new(),
        },
        OntologyConcept {
            uri: "http://equitus.ai/ontology/ecommerce#companyName".to_string(),
            label: "company name".to_string(),
            description: Some("Business customer company name".to_string()),
            concept_type: ConceptType::DataProperty,
            synonyms: vec![
                "company".to_string(),
                "organization".to_string(),
                "org_name".to_string(),
            ],
            parents: vec![],
            metadata: HashMap::new(),
        },
        OntologyConcept {
            uri: "http://equitus.ai/ontology/ecommerce#registrationDate".to_string(),
            label: "registration date".to_string(),
            description: Some("Date customer account was created".to_string()),
            concept_type: ConceptType::DataProperty,
            synonyms: vec![
                "signup_date".to_string(),
                "created_at".to_string(),
                "account_created".to_string(),
            ],
            parents: vec![],
            metadata: HashMap::new(),
        },
        // Address Domain
        OntologyConcept {
            uri: "http://equitus.ai/ontology/ecommerce#streetAddress".to_string(),
            label: "street address".to_string(),
            description: Some("Street name and number".to_string()),
            concept_type: ConceptType::DataProperty,
            synonyms: vec![
                "address".to_string(),
                "street".to_string(),
                "addr_line1".to_string(),
            ],
            parents: vec![],
            metadata: HashMap::new(),
        },
        OntologyConcept {
            uri: "http://equitus.ai/ontology/ecommerce#cityName".to_string(),
            label: "city name".to_string(),
            description: Some("City or municipality".to_string()),
            concept_type: ConceptType::DataProperty,
            synonyms: vec!["city".to_string(), "municipality".to_string()],
            parents: vec![],
            metadata: HashMap::new(),
        },
        OntologyConcept {
            uri: "http://equitus.ai/ontology/ecommerce#regionName".to_string(),
            label: "region name".to_string(),
            description: Some("State or province".to_string()),
            concept_type: ConceptType::DataProperty,
            synonyms: vec![
                "state".to_string(),
                "province".to_string(),
                "region".to_string(),
            ],
            parents: vec![],
            metadata: HashMap::new(),
        },
        OntologyConcept {
            uri: "http://equitus.ai/ontology/ecommerce#postalCode".to_string(),
            label: "postal code".to_string(),
            description: Some("ZIP or postal code".to_string()),
            concept_type: ConceptType::DataProperty,
            synonyms: vec![
                "zip".to_string(),
                "zip_code".to_string(),
                "postcode".to_string(),
            ],
            parents: vec![],
            metadata: HashMap::new(),
        },
        OntologyConcept {
            uri: "http://equitus.ai/ontology/ecommerce#countryName".to_string(),
            label: "country name".to_string(),
            description: Some("Country".to_string()),
            concept_type: ConceptType::DataProperty,
            synonyms: vec!["country".to_string(), "nation".to_string()],
            parents: vec![],
            metadata: HashMap::new(),
        },
        OntologyConcept {
            uri: "http://equitus.ai/ontology/ecommerce#countryCode".to_string(),
            label: "country code".to_string(),
            description: Some("ISO country code".to_string()),
            concept_type: ConceptType::DataProperty,
            synonyms: vec!["country_iso".to_string(), "iso_code".to_string()],
            parents: vec![],
            metadata: HashMap::new(),
        },
        // Product Domain
        OntologyConcept {
            uri: "http://equitus.ai/ontology/ecommerce#productId".to_string(),
            label: "product ID".to_string(),
            description: Some("Unique product identifier".to_string()),
            concept_type: ConceptType::DataProperty,
            synonyms: vec!["prod_id".to_string(), "product_identifier".to_string()],
            parents: vec![],
            metadata: HashMap::new(),
        },
        OntologyConcept {
            uri: "http://equitus.ai/ontology/ecommerce#productName".to_string(),
            label: "product name".to_string(),
            description: Some("Name of the product".to_string()),
            concept_type: ConceptType::DataProperty,
            synonyms: vec![
                "prod_name".to_string(),
                "product_title".to_string(),
                "item_name".to_string(),
            ],
            parents: vec![],
            metadata: HashMap::new(),
        },
        OntologyConcept {
            uri: "http://equitus.ai/ontology/ecommerce#productDescription".to_string(),
            label: "product description".to_string(),
            description: Some("Detailed product description".to_string()),
            concept_type: ConceptType::DataProperty,
            synonyms: vec![
                "description".to_string(),
                "prod_desc".to_string(),
                "details".to_string(),
            ],
            parents: vec![],
            metadata: HashMap::new(),
        },
        OntologyConcept {
            uri: "http://equitus.ai/ontology/ecommerce#sku".to_string(),
            label: "SKU".to_string(),
            description: Some("Stock keeping unit".to_string()),
            concept_type: ConceptType::DataProperty,
            synonyms: vec!["stock_keeping_unit".to_string(), "sku_code".to_string()],
            parents: vec![],
            metadata: HashMap::new(),
        },
        OntologyConcept {
            uri: "http://equitus.ai/ontology/ecommerce#upc".to_string(),
            label: "UPC".to_string(),
            description: Some("Universal product code".to_string()),
            concept_type: ConceptType::DataProperty,
            synonyms: vec![
                "upc_code".to_string(),
                "barcode".to_string(),
                "gtin".to_string(),
            ],
            parents: vec![],
            metadata: HashMap::new(),
        },
        OntologyConcept {
            uri: "http://equitus.ai/ontology/ecommerce#weight".to_string(),
            label: "weight".to_string(),
            description: Some("Product weight".to_string()),
            concept_type: ConceptType::DataProperty,
            synonyms: vec!["product_weight".to_string(), "item_weight".to_string()],
            parents: vec![],
            metadata: HashMap::new(),
        },
        OntologyConcept {
            uri: "http://equitus.ai/ontology/ecommerce#brandName".to_string(),
            label: "brand name".to_string(),
            description: Some("Product brand".to_string()),
            concept_type: ConceptType::DataProperty,
            synonyms: vec!["brand".to_string(), "manufacturer".to_string()],
            parents: vec![],
            metadata: HashMap::new(),
        },
        OntologyConcept {
            uri: "http://equitus.ai/ontology/ecommerce#categoryName".to_string(),
            label: "category name".to_string(),
            description: Some("Product category".to_string()),
            concept_type: ConceptType::DataProperty,
            synonyms: vec!["category".to_string(), "product_category".to_string()],
            parents: vec![],
            metadata: HashMap::new(),
        },
        // Pricing Domain
        OntologyConcept {
            uri: "http://equitus.ai/ontology/ecommerce#amount".to_string(),
            label: "amount".to_string(),
            description: Some("Price amount".to_string()),
            concept_type: ConceptType::DataProperty,
            synonyms: vec![
                "price".to_string(),
                "unit_price".to_string(),
                "cost".to_string(),
            ],
            parents: vec![],
            metadata: HashMap::new(),
        },
        OntologyConcept {
            uri: "http://equitus.ai/ontology/ecommerce#currencyCode".to_string(),
            label: "currency code".to_string(),
            description: Some("ISO currency code".to_string()),
            concept_type: ConceptType::DataProperty,
            synonyms: vec!["currency".to_string(), "currency_iso".to_string()],
            parents: vec![],
            metadata: HashMap::new(),
        },
        OntologyConcept {
            uri: "http://equitus.ai/ontology/ecommerce#discountPercentage".to_string(),
            label: "discount percentage".to_string(),
            description: Some("Discount as percentage".to_string()),
            concept_type: ConceptType::DataProperty,
            synonyms: vec!["discount_pct".to_string(), "discount_rate".to_string()],
            parents: vec![],
            metadata: HashMap::new(),
        },
        // Order Domain
        OntologyConcept {
            uri: "http://equitus.ai/ontology/ecommerce#orderId".to_string(),
            label: "order ID".to_string(),
            description: Some("Unique order identifier".to_string()),
            concept_type: ConceptType::DataProperty,
            synonyms: vec!["order_number".to_string(), "order_identifier".to_string()],
            parents: vec![],
            metadata: HashMap::new(),
        },
        OntologyConcept {
            uri: "http://equitus.ai/ontology/ecommerce#orderDate".to_string(),
            label: "order date".to_string(),
            description: Some("Date order was placed".to_string()),
            concept_type: ConceptType::DataProperty,
            synonyms: vec![
                "order_time".to_string(),
                "placed_at".to_string(),
                "order_created".to_string(),
            ],
            parents: vec![],
            metadata: HashMap::new(),
        },
        OntologyConcept {
            uri: "http://equitus.ai/ontology/ecommerce#totalAmount".to_string(),
            label: "total amount".to_string(),
            description: Some("Order total".to_string()),
            concept_type: ConceptType::DataProperty,
            synonyms: vec![
                "order_total".to_string(),
                "grand_total".to_string(),
                "total".to_string(),
            ],
            parents: vec![],
            metadata: HashMap::new(),
        },
        OntologyConcept {
            uri: "http://equitus.ai/ontology/ecommerce#quantity".to_string(),
            label: "quantity".to_string(),
            description: Some("Item quantity".to_string()),
            concept_type: ConceptType::DataProperty,
            synonyms: vec![
                "qty".to_string(),
                "item_quantity".to_string(),
                "amount".to_string(),
            ],
            parents: vec![],
            metadata: HashMap::new(),
        },
        OntologyConcept {
            uri: "http://equitus.ai/ontology/ecommerce#statusName".to_string(),
            label: "status name".to_string(),
            description: Some("Order status".to_string()),
            concept_type: ConceptType::DataProperty,
            synonyms: vec![
                "status".to_string(),
                "order_status".to_string(),
                "state".to_string(),
            ],
            parents: vec![],
            metadata: HashMap::new(),
        },
        OntologyConcept {
            uri: "http://equitus.ai/ontology/ecommerce#transactionId".to_string(),
            label: "transaction ID".to_string(),
            description: Some("Payment transaction identifier".to_string()),
            concept_type: ConceptType::DataProperty,
            synonyms: vec!["txn_id".to_string(), "payment_id".to_string()],
            parents: vec![],
            metadata: HashMap::new(),
        },
        // Inventory Domain
        OntologyConcept {
            uri: "http://equitus.ai/ontology/ecommerce#stockQuantity".to_string(),
            label: "stock quantity".to_string(),
            description: Some("Available inventory".to_string()),
            concept_type: ConceptType::DataProperty,
            synonyms: vec![
                "stock".to_string(),
                "inventory".to_string(),
                "qty_on_hand".to_string(),
            ],
            parents: vec![],
            metadata: HashMap::new(),
        },
        OntologyConcept {
            uri: "http://equitus.ai/ontology/ecommerce#reorderLevel".to_string(),
            label: "reorder level".to_string(),
            description: Some("Minimum stock before reorder".to_string()),
            concept_type: ConceptType::DataProperty,
            synonyms: vec!["min_stock".to_string(), "reorder_point".to_string()],
            parents: vec![],
            metadata: HashMap::new(),
        },
        OntologyConcept {
            uri: "http://equitus.ai/ontology/ecommerce#warehouseName".to_string(),
            label: "warehouse name".to_string(),
            description: Some("Warehouse location".to_string()),
            concept_type: ConceptType::DataProperty,
            synonyms: vec!["warehouse".to_string(), "location".to_string()],
            parents: vec![],
            metadata: HashMap::new(),
        },
        // Shipping Domain
        OntologyConcept {
            uri: "http://equitus.ai/ontology/ecommerce#trackingNumber".to_string(),
            label: "tracking number".to_string(),
            description: Some("Shipment tracking number".to_string()),
            concept_type: ConceptType::DataProperty,
            synonyms: vec![
                "tracking".to_string(),
                "tracking_code".to_string(),
                "tracking_id".to_string(),
            ],
            parents: vec![],
            metadata: HashMap::new(),
        },
        OntologyConcept {
            uri: "http://equitus.ai/ontology/ecommerce#carrierName".to_string(),
            label: "carrier name".to_string(),
            description: Some("Shipping carrier".to_string()),
            concept_type: ConceptType::DataProperty,
            synonyms: vec!["carrier".to_string(), "shipper".to_string()],
            parents: vec![],
            metadata: HashMap::new(),
        },
        // Demographics Domain
        OntologyConcept {
            uri: "http://equitus.ai/ontology/ecommerce#dateOfBirth".to_string(),
            label: "date of birth".to_string(),
            description: Some("Customer birth date".to_string()),
            concept_type: ConceptType::DataProperty,
            synonyms: vec![
                "dob".to_string(),
                "birth_date".to_string(),
                "birthdate".to_string(),
            ],
            parents: vec![],
            metadata: HashMap::new(),
        },
        OntologyConcept {
            uri: "http://equitus.ai/ontology/ecommerce#genderName".to_string(),
            label: "gender name".to_string(),
            description: Some("Customer gender".to_string()),
            concept_type: ConceptType::DataProperty,
            synonyms: vec!["gender".to_string(), "sex".to_string()],
            parents: vec![],
            metadata: HashMap::new(),
        },
        // Review Domain
        OntologyConcept {
            uri: "http://equitus.ai/ontology/ecommerce#reviewText".to_string(),
            label: "review text".to_string(),
            description: Some("Customer review content".to_string()),
            concept_type: ConceptType::DataProperty,
            synonyms: vec![
                "review".to_string(),
                "comment".to_string(),
                "feedback".to_string(),
            ],
            parents: vec![],
            metadata: HashMap::new(),
        },
        OntologyConcept {
            uri: "http://equitus.ai/ontology/ecommerce#ratingValue".to_string(),
            label: "rating value".to_string(),
            description: Some("Numeric rating score".to_string()),
            concept_type: ConceptType::DataProperty,
            synonyms: vec![
                "rating".to_string(),
                "score".to_string(),
                "stars".to_string(),
            ],
            parents: vec![],
            metadata: HashMap::new(),
        },
    ]
}

/// Create a large ecommerce database schema for testing
fn create_ecommerce_schema() -> Vec<UnifiedSchema> {
    vec![
        // Customers table
        {
            let mut schema = UnifiedSchema::new(
                "customers".to_string(),
                SourceType::PostgreSQL,
                "postgres://localhost/ecommerce".to_string(),
            );

            schema.add_field(UnifiedField::new(
                "cust_id".to_string(),
                UniversalDataType::Integer { bits: Some(64) },
            ));
            schema.add_field(UnifiedField::new(
                "given_name".to_string(),
                UniversalDataType::String {
                    max_length: Some(100),
                },
            ));
            schema.add_field(UnifiedField::new(
                "family_name".to_string(),
                UniversalDataType::String {
                    max_length: Some(100),
                },
            ));
            schema.add_field(UnifiedField::new(
                "email_address".to_string(),
                UniversalDataType::String {
                    max_length: Some(255),
                },
            ));
            schema.add_field(UnifiedField::new(
                "mobile".to_string(),
                UniversalDataType::String {
                    max_length: Some(20),
                },
            ));
            schema.add_field(UnifiedField::new(
                "signup_date".to_string(),
                UniversalDataType::Timestamp,
            ));
            schema.add_field(UnifiedField::new(
                "birth_date".to_string(),
                UniversalDataType::Date,
            ));

            schema
        },
        // Products table
        {
            let mut schema = UnifiedSchema::new(
                "products".to_string(),
                SourceType::PostgreSQL,
                "postgres://localhost/ecommerce".to_string(),
            );

            schema.add_field(UnifiedField::new(
                "prod_id".to_string(),
                UniversalDataType::Integer { bits: Some(64) },
            ));
            schema.add_field(UnifiedField::new(
                "product_title".to_string(),
                UniversalDataType::String {
                    max_length: Some(500),
                },
            ));
            schema.add_field(UnifiedField::new(
                "prod_desc".to_string(),
                UniversalDataType::String { max_length: None },
            ));
            schema.add_field(UnifiedField::new(
                "sku_code".to_string(),
                UniversalDataType::String {
                    max_length: Some(50),
                },
            ));
            schema.add_field(UnifiedField::new(
                "barcode".to_string(),
                UniversalDataType::String {
                    max_length: Some(50),
                },
            ));
            schema.add_field(UnifiedField::new(
                "unit_price".to_string(),
                UniversalDataType::Decimal {
                    precision: 10,
                    scale: 2,
                },
            ));
            schema.add_field(UnifiedField::new(
                "brand".to_string(),
                UniversalDataType::String {
                    max_length: Some(100),
                },
            ));
            schema.add_field(UnifiedField::new(
                "category".to_string(),
                UniversalDataType::String {
                    max_length: Some(100),
                },
            ));
            schema.add_field(UnifiedField::new(
                "item_weight".to_string(),
                UniversalDataType::Float { bits: Some(32) },
            ));

            schema
        },
        // Orders table
        {
            let mut schema = UnifiedSchema::new(
                "orders".to_string(),
                SourceType::PostgreSQL,
                "postgres://localhost/ecommerce".to_string(),
            );

            schema.add_field(UnifiedField::new(
                "order_number".to_string(),
                UniversalDataType::String {
                    max_length: Some(50),
                },
            ));
            schema.add_field(UnifiedField::new(
                "cust_id".to_string(),
                UniversalDataType::Integer { bits: Some(64) },
            ));
            schema.add_field(UnifiedField::new(
                "order_created".to_string(),
                UniversalDataType::Timestamp,
            ));
            schema.add_field(UnifiedField::new(
                "grand_total".to_string(),
                UniversalDataType::Decimal {
                    precision: 10,
                    scale: 2,
                },
            ));
            schema.add_field(UnifiedField::new(
                "order_status".to_string(),
                UniversalDataType::String {
                    max_length: Some(50),
                },
            ));
            schema.add_field(UnifiedField::new(
                "tracking_code".to_string(),
                UniversalDataType::String {
                    max_length: Some(100),
                },
            ));

            schema
        },
        // Addresses table
        {
            let mut schema = UnifiedSchema::new(
                "addresses".to_string(),
                SourceType::PostgreSQL,
                "postgres://localhost/ecommerce".to_string(),
            );

            schema.add_field(UnifiedField::new(
                "addr_line1".to_string(),
                UniversalDataType::String {
                    max_length: Some(255),
                },
            ));
            schema.add_field(UnifiedField::new(
                "city".to_string(),
                UniversalDataType::String {
                    max_length: Some(100),
                },
            ));
            schema.add_field(UnifiedField::new(
                "state".to_string(),
                UniversalDataType::String {
                    max_length: Some(100),
                },
            ));
            schema.add_field(UnifiedField::new(
                "zip_code".to_string(),
                UniversalDataType::String {
                    max_length: Some(20),
                },
            ));
            schema.add_field(UnifiedField::new(
                "country".to_string(),
                UniversalDataType::String {
                    max_length: Some(100),
                },
            ));
            schema.add_field(UnifiedField::new(
                "country_iso".to_string(),
                UniversalDataType::String {
                    max_length: Some(3),
                },
            ));

            schema
        },
        // Inventory table
        {
            let mut schema = UnifiedSchema::new(
                "inventory".to_string(),
                SourceType::PostgreSQL,
                "postgres://localhost/ecommerce".to_string(),
            );

            schema.add_field(UnifiedField::new(
                "prod_id".to_string(),
                UniversalDataType::Integer { bits: Some(64) },
            ));
            schema.add_field(UnifiedField::new(
                "qty_on_hand".to_string(),
                UniversalDataType::Integer { bits: Some(32) },
            ));
            schema.add_field(UnifiedField::new(
                "reorder_point".to_string(),
                UniversalDataType::Integer { bits: Some(32) },
            ));
            schema.add_field(UnifiedField::new(
                "warehouse".to_string(),
                UniversalDataType::String {
                    max_length: Some(100),
                },
            ));

            schema
        },
    ]
}

#[test]
fn test_large_ontology_loading() {
    println!("\n=== Test: Large Ecommerce Ontology Loading ===\n");

    let ontology = load_ecommerce_ontology();
    println!("Loaded {} ontology concepts", ontology.len());
    assert_eq!(ontology.len(), 39); // Updated count

    let config = ModelServiceConfig::default();
    let mut aligner = OntologyAligner::new(config);

    aligner.load_ontology(ontology.clone());

    println!("✓ Successfully loaded large ontology");
    println!("✓ Embedding index built");
}

#[test]
fn test_ecommerce_schema_alignment() {
    println!("\n=== Test: Ecommerce Schema Alignment ===\n");

    let ontology = load_ecommerce_ontology();
    let schemas = create_ecommerce_schema();

    let config = ModelServiceConfig {
        max_candidates: 5,
        min_similarity: 0.5,
        ..Default::default()
    };

    let mut aligner = OntologyAligner::new(config);
    aligner.load_ontology(ontology);

    let mut total_fields = 0;
    let mut aligned_fields = 0;

    for schema in &schemas {
        println!("--- Aligning Schema: {} ---", schema.name);

        for field in &schema.fields {
            total_fields += 1;

            // Use sync version for testing
            let rt = tokio::runtime::Runtime::new().unwrap();
            let alignments = rt.block_on(aligner.align_field(field));

            if !alignments.is_empty() {
                aligned_fields += 1;
                let best = &alignments[0];
                println!(
                    "  ✓ '{}' → '{}' (similarity: {:.2}, method: {:?})",
                    field.name, best.concept.label, best.similarity, best.method
                );
            } else {
                println!("  ⚠ No match for '{}'", field.name);
            }
        }
        println!();
    }

    let coverage = (aligned_fields as f64 / total_fields as f64) * 100.0;
    println!(
        "Alignment Coverage: {}/{} fields ({:.1}%)",
        aligned_fields, total_fields, coverage
    );

    // With a comprehensive ontology and good synonym matching, we expect high coverage
    assert!(
        coverage >= 70.0,
        "Expected at least 70% coverage, got {:.1}%",
        coverage
    );
}

#[test]
fn test_embedding_edge_cases() {
    println!("\n=== Test: Embedding Edge Cases ===\n");

    let embedder = PretrainedEmbeddings::new();

    // Test 1: Empty string
    let emb_empty = embedder.embed("");
    assert_eq!(emb_empty.len(), embedder.dimension());
    println!("✓ Empty string handled");

    // Test 2: Single character
    let emb_single = embedder.embed("a");
    assert_eq!(emb_single.len(), embedder.dimension());
    println!("✓ Single character handled");

    // Test 3: Very long field name
    let long_name = "a".repeat(500);
    let emb_long = embedder.embed(&long_name);
    assert_eq!(emb_long.len(), embedder.dimension());
    println!("✓ Very long field name handled");

    // Test 4: Special characters
    let special = "field@#$%^&*()_+-=";
    let emb_special = embedder.embed(special);
    assert_eq!(emb_special.len(), embedder.dimension());
    println!("✓ Special characters handled");

    // Test 5: Unicode characters
    let unicode = "顾客_标识符";
    let emb_unicode = embedder.embed(unicode);
    assert_eq!(emb_unicode.len(), embedder.dimension());
    println!("✓ Unicode characters handled");

    // Test 6: Mixed case
    let emb_lower = embedder.embed("customer_id");
    let emb_upper = embedder.embed("CUSTOMER_ID");
    let emb_mixed = embedder.embed("Customer_Id");

    let sim_lower_upper = cosine_similarity(&emb_lower, &emb_upper);
    let sim_lower_mixed = cosine_similarity(&emb_lower, &emb_mixed);

    println!("  Similarity (lower vs upper): {:.3}", sim_lower_upper);
    println!("  Similarity (lower vs mixed): {:.3}", sim_lower_mixed);

    // Should be very similar due to lowercase normalization
    assert!(sim_lower_upper > 0.99, "Case should be normalized");
    assert!(sim_lower_mixed > 0.99, "Case should be normalized");
    println!("✓ Case normalization works correctly");

    // Test 7: Similar vs dissimilar fields
    let emb_cust_id = embedder.embed("customer_id");
    let emb_cust_name = embedder.embed("customer_name");
    let emb_prod_id = embedder.embed("product_id");

    let sim_cust_id_name = cosine_similarity(&emb_cust_id, &emb_cust_name);
    let sim_cust_id_prod = cosine_similarity(&emb_cust_id, &emb_prod_id);

    println!(
        "  Similarity (customer_id vs customer_name): {:.3}",
        sim_cust_id_name
    );
    println!(
        "  Similarity (customer_id vs product_id): {:.3}",
        sim_cust_id_prod
    );

    // customer_id should be more similar to customer_name than product_id
    // Both share "id" but customer_id/customer_name share "customer"
    assert!(
        sim_cust_id_name > sim_cust_id_prod,
        "customer_id should be more similar to customer_name than product_id"
    );
    println!("✓ Similarity discrimination works");
}

#[test]
fn test_embedding_performance_scalability() {
    println!("\n=== Test: Embedding Performance & Scalability ===\n");

    let embedder = PretrainedEmbeddings::new();

    // Test with increasing numbers of fields
    let field_counts = vec![10, 50, 100, 500];

    for count in field_counts {
        let start = std::time::Instant::now();

        for i in 0..count {
            let field_name = format!("field_{}", i);
            let _emb = embedder.embed(&field_name);
        }

        let duration = start.elapsed();
        let avg_ms = duration.as_micros() as f64 / count as f64 / 1000.0;

        println!(
            "  {} fields: {:.2}ms total, {:.3}ms avg per field",
            count,
            duration.as_secs_f64() * 1000.0,
            avg_ms
        );

        // Should be fast - under 1ms per field on average
        assert!(
            avg_ms < 1.0,
            "Embedding should be fast (<1ms), got {:.3}ms",
            avg_ms
        );
    }

    println!("✓ Performance is acceptable for all scales");
}

#[test]
fn test_similarity_index_accuracy() {
    println!("\n=== Test: Similarity Index Accuracy ===\n");

    use graphica_core::schema::{CachedSimilarityIndex, IndexConfig};

    let embedder = PretrainedEmbeddings::new();

    // Create embeddings for common database fields
    let fields = vec![
        "customer_id",
        "customer_name",
        "customer_email",
        "product_id",
        "product_name",
        "product_price",
        "order_id",
        "order_date",
        "order_total",
        "quantity",
        "shipping_address",
        "billing_address",
    ];

    let embeddings: Vec<Vec<f32>> = fields.iter().map(|f| embedder.embed(f)).collect();

    let metadata: Vec<Option<String>> = fields.iter().map(|f| Some(f.to_string())).collect();

    let config = IndexConfig {
        k: 3,
        min_similarity: 0.0,
        ..Default::default()
    };

    let index = CachedSimilarityIndex::new(embeddings, metadata, config);

    // Test query: "cust_id" should find "customer_id" as top match
    let query = embedder.embed("cust_id");
    let matches = index.search(&query);

    println!("Query: 'cust_id'");
    for (i, m) in matches.iter().enumerate() {
        println!(
            "  {}. {} (similarity: {:.3})",
            i + 1,
            m.metadata.as_ref().unwrap(),
            m.similarity
        );
    }

    assert!(!matches.is_empty(), "Should find matches");
    assert_eq!(
        matches[0].metadata.as_ref().unwrap(),
        "customer_id",
        "Top match should be customer_id"
    );
    // TF-IDF with character n-grams gives ~0.6 similarity for cust_id vs customer_id
    // This is actually quite good for this approach
    assert!(
        matches[0].similarity > 0.5,
        "Top match should have reasonable similarity, got {:.3}",
        matches[0].similarity
    );

    println!("✓ Similarity index returns accurate results");
}

#[test]
fn test_synonym_vs_embedding_matching() {
    println!("\n=== Test: Synonym vs Embedding Matching ===\n");

    let ontology = vec![OntologyConcept {
        uri: "http://test.com/customerId".to_string(),
        label: "Customer Identifier".to_string(),
        description: Some("Unique customer ID".to_string()),
        concept_type: ConceptType::DataProperty,
        synonyms: vec!["cust_id".to_string(), "customer_id".to_string()],
        parents: vec![],
        metadata: HashMap::new(),
    }];

    let config = ModelServiceConfig::default();
    let mut aligner = OntologyAligner::new(config);
    aligner.load_ontology(ontology);

    // Test 1: Exact synonym match
    let field_exact = UnifiedField::new(
        "cust_id".to_string(),
        UniversalDataType::Integer { bits: Some(64) },
    );

    let rt = tokio::runtime::Runtime::new().unwrap();
    let alignments = rt.block_on(aligner.align_field(&field_exact));

    assert!(!alignments.is_empty());
    assert_eq!(
        alignments[0].method,
        graphica_core::schema::AlignmentMethod::SynonymMatch
    );
    assert!(alignments[0].confidence >= 0.95);
    println!(
        "✓ Exact synonym match works (confidence: {:.3})",
        alignments[0].confidence
    );

    // Test 2: Similar but not exact - should use embeddings
    let field_similar = UnifiedField::new(
        "customer_identifier".to_string(),
        UniversalDataType::Integer { bits: Some(64) },
    );

    let alignments = rt.block_on(aligner.align_field(&field_similar));

    assert!(!alignments.is_empty());
    // Should find match via embedding similarity
    assert!(alignments[0].similarity > 0.6);
    println!(
        "✓ Embedding-based match works for similar terms (similarity: {:.3})",
        alignments[0].similarity
    );
}

#[test]
fn test_comprehensive_ecommerce_pipeline() {
    println!("\n=== Test: Comprehensive Ecommerce Pipeline ===\n");

    let ontology = load_ecommerce_ontology();
    let schemas = create_ecommerce_schema();

    println!("Loaded ontology: {} concepts", ontology.len());
    println!("Test schemas: {} tables", schemas.len());

    let config = ModelServiceConfig {
        max_candidates: 3,
        min_similarity: 0.5, // TF-IDF gives ~0.5-0.6 for good matches
        ..Default::default()
    };

    let mut aligner = OntologyAligner::new(config);
    aligner.load_ontology(ontology);

    let rt = tokio::runtime::Runtime::new().unwrap();

    let mut stats = HashMap::new();
    stats.insert("exact_matches", 0);
    stats.insert("synonym_matches", 0);
    stats.insert("embedding_matches", 0);
    stats.insert("no_matches", 0);

    for schema in &schemas {
        println!("\n--- Processing: {} ---", schema.name);

        for field in &schema.fields {
            let alignments = rt.block_on(aligner.align_field(field));

            if alignments.is_empty() {
                *stats.get_mut("no_matches").unwrap() += 1;
                println!("  ✗ No match: {}", field.name);
            } else {
                let best = &alignments[0];

                match best.method {
                    graphica_core::schema::AlignmentMethod::ExactMatch => {
                        *stats.get_mut("exact_matches").unwrap() += 1;
                        println!(
                            "  ✓ EXACT: {} → {} ({:.2})",
                            field.name, best.concept.label, best.confidence
                        );
                    }
                    graphica_core::schema::AlignmentMethod::SynonymMatch => {
                        *stats.get_mut("synonym_matches").unwrap() += 1;
                        println!(
                            "  ✓ SYNONYM: {} → {} ({:.2})",
                            field.name, best.concept.label, best.confidence
                        );
                    }
                    graphica_core::schema::AlignmentMethod::EmbeddingSimilarity => {
                        *stats.get_mut("embedding_matches").unwrap() += 1;
                        println!(
                            "  ✓ EMBEDDING: {} → {} ({:.2})",
                            field.name, best.concept.label, best.similarity
                        );
                    }
                    graphica_core::schema::AlignmentMethod::PatternMatch
                    | graphica_core::schema::AlignmentMethod::Hybrid => {
                        *stats.get_mut("embedding_matches").unwrap() += 1;
                        println!(
                            "  ✓ OTHER: {} → {} ({:.2})",
                            field.name, best.concept.label, best.confidence
                        );
                    }
                }
            }
        }
    }

    println!("\n=== Alignment Statistics ===");
    println!("  Exact matches: {}", stats["exact_matches"]);
    println!("  Synonym matches: {}", stats["synonym_matches"]);
    println!("  Embedding matches: {}", stats["embedding_matches"]);
    println!("  No matches: {}", stats["no_matches"]);

    let total = stats.values().sum::<i32>();
    let matched = total - stats["no_matches"];
    let coverage = (matched as f64 / total as f64) * 100.0;

    println!(
        "\n  Total coverage: {}/{} ({:.1}%)",
        matched, total, coverage
    );

    // We expect high coverage with the comprehensive ontology
    assert!(
        coverage >= 70.0,
        "Expected at least 70% coverage, got {:.1}%",
        coverage
    );

    // We should have some of each type of match
    assert!(stats["synonym_matches"] > 0, "Should have synonym matches");

    println!("\n✓ Comprehensive pipeline test passed!");
}
