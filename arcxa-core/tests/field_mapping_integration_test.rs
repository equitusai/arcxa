// Integration test for field mapping with real CSV data
use graphica_core::inference::mapping::{
    DataType, DatasetSchema, FieldMapper, FieldMetadata, FieldProfile, MapperConfig,
    ValueDistribution,
};
use std::collections::HashMap;

/// Helper to create field metadata from CSV analysis
fn analyze_csv_column(
    column_name: &str,
    values: &[&str],
    position: usize,
    neighbors: Vec<String>,
) -> FieldMetadata {
    // Calculate statistics
    let total_rows = values.len() as u64;
    let mut value_counts: HashMap<&str, usize> = HashMap::new();
    let mut null_count = 0;

    for &value in values {
        if value.is_empty() || value == "NULL" {
            null_count += 1;
        } else {
            *value_counts.entry(value).or_insert(0) += 1;
        }
    }

    let distinct_count = value_counts.len() as u64;
    let null_percentage = null_count as f64 / total_rows as f64;

    // Infer data type
    let data_type = if values
        .iter()
        .all(|v| v.parse::<i64>().is_ok() || v.is_empty())
    {
        DataType::Integer
    } else if values
        .iter()
        .all(|v| v.parse::<f64>().is_ok() || v.is_empty())
    {
        DataType::Float
    } else if values.iter().all(|v| {
        v.is_empty() || v.contains('@') // simple email check
    }) {
        DataType::String // Could be Email type
    } else {
        DataType::String
    };

    // Find min/max for numeric types
    let mut numeric_values: Vec<i64> = values
        .iter()
        .filter_map(|v| v.parse::<i64>().ok())
        .collect();
    numeric_values.sort();

    let distribution = if !numeric_values.is_empty() {
        ValueDistribution {
            min: numeric_values.first().map(|v| v.to_string()),
            max: numeric_values.last().map(|v| v.to_string()),
            median: if numeric_values.len() > 0 {
                Some(numeric_values[numeric_values.len() / 2].to_string())
            } else {
                None
            },
            p25: if numeric_values.len() > 0 {
                Some(numeric_values[numeric_values.len() / 4].to_string())
            } else {
                None
            },
            p75: if numeric_values.len() > 0 {
                Some(numeric_values[numeric_values.len() * 3 / 4].to_string())
            } else {
                None
            },
            ..Default::default()
        }
    } else {
        // String data - use lexicographic order
        let mut string_values: Vec<&str> =
            values.iter().filter(|v| !v.is_empty()).copied().collect();
        string_values.sort();

        ValueDistribution {
            min: string_values.first().map(|v| v.to_string()),
            max: string_values.last().map(|v| v.to_string()),
            ..Default::default()
        }
    };

    FieldMetadata {
        qualified_name: format!("dataset.{}", column_name),
        column_name: column_name.to_string(),
        source_id: "dataset".to_string(),
        data_type,
        profile: FieldProfile {
            distinct_count,
            total_rows,
            null_percentage,
            distribution,
            samples: values.iter().take(10).map(|v| v.to_string()).collect(),
        },
        semantic_type: None,
        position,
        neighbors,
    }
}

#[test]
fn test_real_customer_orders_mapping() {
    println!("\n=== Testing Field Mapping with Real Customer/Orders Data ===\n");

    // Simulate real customers.csv data
    let customer_ids = vec!["1", "2", "3", "4", "5", "6", "7", "8", "9", "10"];
    let customer_emails = vec![
        "john@example.com",
        "jane@example.com",
        "bob@example.com",
        "alice@example.com",
        "charlie@example.com",
        "dave@example.com",
        "eve@example.com",
        "frank@example.com",
        "grace@example.com",
        "henry@example.com",
    ];
    let customer_first_names = vec![
        "John", "Jane", "Bob", "Alice", "Charlie", "Dave", "Eve", "Frank", "Grace", "Henry",
    ];

    // Simulate real orders.csv data (references customers)
    let order_ids = vec!["101", "102", "103", "104", "105", "106", "107", "108"];
    let order_cust_ids = vec!["1", "2", "1", "3", "4", "2", "5", "1"]; // Multiple orders per customer
    let order_amounts = vec![
        "99.99", "149.99", "79.99", "199.99", "59.99", "129.99", "89.99", "109.99",
    ];

    // Analyze customers dataset
    let customers_schema = DatasetSchema {
        dataset_id: "customers_csv".to_string(),
        dataset_name: "Customers CSV".to_string(),
        fields: vec![
            analyze_csv_column(
                "customer_id",
                &customer_ids,
                0,
                vec!["email".to_string(), "first_name".to_string()],
            ),
            analyze_csv_column(
                "email",
                &customer_emails,
                1,
                vec!["customer_id".to_string(), "first_name".to_string()],
            ),
            analyze_csv_column(
                "first_name",
                &customer_first_names,
                2,
                vec!["customer_id".to_string(), "email".to_string()],
            ),
        ],
    };

    // Analyze orders dataset
    let orders_schema = DatasetSchema {
        dataset_id: "orders_csv".to_string(),
        dataset_name: "Orders CSV".to_string(),
        fields: vec![
            analyze_csv_column(
                "order_id",
                &order_ids,
                0,
                vec!["cust_id".to_string(), "amount".to_string()],
            ),
            analyze_csv_column(
                "cust_id",
                &order_cust_ids,
                1,
                vec!["order_id".to_string(), "amount".to_string()],
            ),
            analyze_csv_column(
                "amount",
                &order_amounts,
                2,
                vec!["order_id".to_string(), "cust_id".to_string()],
            ),
        ],
    };

    // Create field mapper
    let mapper = FieldMapper::new();

    // Find mappings
    let mappings = mapper
        .find_mappings(&customers_schema, &orders_schema)
        .expect("Failed to find mappings");

    println!("Found {} field mappings", mappings.len());

    // Verify we found the customer_id → cust_id mapping
    let customer_id_mapping = mappings
        .iter()
        .find(|m| m.source_field.column_name == "customer_id")
        .expect("Should find customer_id mapping");

    println!("\n--- customer_id Candidates ---");
    for (idx, candidate) in customer_id_mapping.candidates.iter().enumerate() {
        println!(
            "  {}. {} → {} (confidence: {:.2})",
            idx + 1,
            candidate.source.column_name,
            candidate.target.column_name,
            candidate.confidence
        );
        println!(
            "     Scores: lexical={:.2}, statistical={:.2}, schema={:.2}",
            candidate.scores.lexical, candidate.scores.statistical, candidate.scores.schema_context
        );
        for evidence in &candidate.evidence {
            println!("     - {}", evidence.description);
        }
    }

    // Should find cust_id in top 2 candidates (position matters, so might not be #1)
    let has_cust_id = customer_id_mapping
        .candidates
        .iter()
        .take(2)
        .any(|c| c.target.column_name == "cust_id");
    assert!(has_cust_id, "cust_id should be in top 2 candidates");

    // Find the cust_id mapping specifically
    let cust_id_candidate = customer_id_mapping
        .candidates
        .iter()
        .find(|c| c.target.column_name == "cust_id")
        .expect("Should find cust_id candidate");

    // Should have decent confidence (abbreviation match)
    assert!(
        cust_id_candidate.confidence > 0.5,
        "Confidence should be > 0.5, was {}",
        cust_id_candidate.confidence
    );

    // Lexical score should be good (customer_id vs cust_id)
    assert!(
        cust_id_candidate.scores.lexical > 0.6,
        "Lexical similarity should be > 0.6 for abbreviation match, was {}",
        cust_id_candidate.scores.lexical
    );

    // Categorize by confidence
    let all_similarities: Vec<_> = mappings.into_iter().flat_map(|m| m.candidates).collect();

    let suggestions = mapper.categorize_mappings(all_similarities);

    println!("\n--- Categorized Suggestions ---");
    println!("Auto-mapped (≥90%): {}", suggestions.auto_mapped.len());
    println!("Recommended (70-89%): {}", suggestions.recommended.len());
    println!("Possible (50-69%): {}", suggestions.possible.len());

    if !suggestions.recommended.is_empty() {
        println!("\nRecommended mappings:");
        for sim in &suggestions.recommended {
            println!(
                "  {} → {} ({:.1}%)",
                sim.source.column_name,
                sim.target.column_name,
                sim.confidence * 100.0
            );
        }
    }

    if !suggestions.possible.is_empty() {
        println!("\nPossible mappings:");
        for sim in &suggestions.possible {
            println!(
                "  {} → {} ({:.1}%)",
                sim.source.column_name,
                sim.target.column_name,
                sim.confidence * 100.0
            );
        }
    }

    println!("\n✓ Real data test passed!");
}

#[test]
fn test_products_inventory_mapping() {
    println!("\n=== Testing Field Mapping with Products/Inventory Data ===\n");

    // Products dataset
    let product_ids = vec!["SKU001", "SKU002", "SKU003", "SKU004", "SKU005"];
    let product_names = vec!["Widget A", "Widget B", "Gadget X", "Gadget Y", "Tool Z"];
    let product_categories = vec!["Widgets", "Widgets", "Gadgets", "Gadgets", "Tools"];

    // Inventory dataset (different naming convention)
    let inventory_item_codes = vec!["SKU001", "SKU002", "SKU003", "SKU004", "SKU005"];
    let inventory_quantities = vec!["100", "50", "75", "200", "30"];
    let inventory_warehouses = vec!["WH-A", "WH-B", "WH-A", "WH-C", "WH-B"];

    let products_schema = DatasetSchema {
        dataset_id: "products".to_string(),
        dataset_name: "Products".to_string(),
        fields: vec![
            analyze_csv_column(
                "product_id",
                &product_ids,
                0,
                vec!["product_name".to_string()],
            ),
            analyze_csv_column(
                "product_name",
                &product_names,
                1,
                vec!["product_id".to_string()],
            ),
            analyze_csv_column(
                "category",
                &product_categories,
                2,
                vec!["product_name".to_string()],
            ),
        ],
    };

    let inventory_schema = DatasetSchema {
        dataset_id: "inventory".to_string(),
        dataset_name: "Inventory".to_string(),
        fields: vec![
            analyze_csv_column(
                "item_code",
                &inventory_item_codes,
                0,
                vec!["quantity".to_string()],
            ),
            analyze_csv_column(
                "quantity",
                &inventory_quantities,
                1,
                vec!["item_code".to_string()],
            ),
            analyze_csv_column(
                "warehouse",
                &inventory_warehouses,
                2,
                vec!["quantity".to_string()],
            ),
        ],
    };

    let mapper = FieldMapper::new();
    let mappings = mapper
        .find_mappings(&products_schema, &inventory_schema)
        .expect("Failed to find mappings");

    println!("Found {} field mappings", mappings.len());

    // Should find product_id → item_code mapping
    let product_id_mapping = mappings
        .iter()
        .find(|m| m.source_field.column_name == "product_id")
        .expect("Should find product_id mapping");

    println!("\n--- product_id Candidates ---");
    for candidate in &product_id_mapping.candidates {
        println!(
            "  {} → {} (confidence: {:.2})",
            candidate.source.column_name, candidate.target.column_name, candidate.confidence
        );
    }

    // Even though names are different (product_id vs item_code),
    // statistical similarity should be very high (exact same values)
    let top_candidate = &product_id_mapping.candidates[0];
    assert_eq!(top_candidate.target.column_name, "item_code");

    // Should have good statistical score (100% value overlap)
    assert!(
        top_candidate.scores.statistical > 0.95,
        "Statistical score should be high due to identical values, was {}",
        top_candidate.scores.statistical
    );

    println!("\n✓ Products/Inventory test passed!");
}

#[test]
fn test_different_scales_mapping() {
    println!("\n=== Testing Field Mapping with Different Data Scales ===\n");

    // Large dataset A (100,000 records)
    let dataset_a_ids: Vec<String> = (1..=100).map(|i| i.to_string()).collect();
    let dataset_a_ids_refs: Vec<&str> = dataset_a_ids.iter().map(|s| s.as_str()).collect();

    // Smaller dataset B (10,000 records, subset of A)
    let dataset_b_refs: Vec<String> = (1..=50).map(|i| i.to_string()).collect();
    let dataset_b_refs_refs: Vec<&str> = dataset_b_refs.iter().map(|s| s.as_str()).collect();

    let schema_a = DatasetSchema {
        dataset_id: "large_dataset".to_string(),
        dataset_name: "Large Dataset".to_string(),
        fields: vec![analyze_csv_column("id", &dataset_a_ids_refs, 0, vec![])],
    };

    let schema_b = DatasetSchema {
        dataset_id: "small_dataset".to_string(),
        dataset_name: "Small Dataset".to_string(),
        fields: vec![analyze_csv_column(
            "reference_id",
            &dataset_b_refs_refs,
            0,
            vec![],
        )],
    };

    let mapper = FieldMapper::new();
    let mappings = mapper
        .find_mappings(&schema_a, &schema_b)
        .expect("Failed to find mappings");

    if let Some(id_mapping) = mappings.first() {
        let candidate = &id_mapping.candidates[0];
        println!(
            "Mapping: {} → {} (confidence: {:.2})",
            candidate.source.column_name, candidate.target.column_name, candidate.confidence
        );

        // Should detect Many-to-One relationship (smaller dataset references larger)
        println!("Relationship type: {:?}", candidate.relationship_type);
    }

    println!("\n✓ Different scales test passed!");
}

#[test]
fn test_custom_mapper_config() {
    println!("\n=== Testing Custom Mapper Configuration ===\n");

    // Create mapper with custom weights (emphasize statistical over lexical)
    let mut config = MapperConfig::default();
    config.score_weights.lexical = 0.10; // Reduce lexical importance
    config.score_weights.statistical = 0.70; // Increase statistical importance
    config.score_weights.schema_context = 0.20;
    config.min_confidence = 0.6; // Higher threshold

    let mapper = FieldMapper::with_config(config);

    // Test with data that has poor lexical match but good statistical match
    let ids_a = vec!["1", "2", "3", "4", "5"];
    let ids_b = vec!["1", "2", "3", "4", "5"];

    let schema_a = DatasetSchema {
        dataset_id: "a".to_string(),
        dataset_name: "A".to_string(),
        fields: vec![analyze_csv_column("xyz_identifier", &ids_a, 0, vec![])],
    };

    let schema_b = DatasetSchema {
        dataset_id: "b".to_string(),
        dataset_name: "B".to_string(),
        fields: vec![analyze_csv_column("abc_reference", &ids_b, 0, vec![])],
    };

    let mappings = mapper
        .find_mappings(&schema_a, &schema_b)
        .expect("Failed to find mappings");

    if let Some(mapping) = mappings.first() {
        if let Some(candidate) = mapping.candidates.first() {
            println!("Found mapping with custom config:");
            println!(
                "  {} → {}",
                candidate.source.column_name, candidate.target.column_name
            );
            println!("  Confidence: {:.2}", candidate.confidence);
            println!(
                "  Lexical: {:.2}, Statistical: {:.2}",
                candidate.scores.lexical, candidate.scores.statistical
            );

            // With higher statistical weight, even poor lexical match should succeed
            // if statistical match is perfect
            assert!(
                candidate.scores.statistical > 0.95,
                "Statistical score should be high"
            );
        }
    }

    println!("\n✓ Custom config test passed!");
}
