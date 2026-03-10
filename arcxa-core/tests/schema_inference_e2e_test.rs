//! End-to-End Integration Test for Enhanced Schema Inference
//!
//! Tests the complete pipeline:
//! 1. PostgreSQL schema inference
//! 2. Statistics extraction
//! 3. Semantic type detection
//! 4. RDF conversion

use chrono::Utc;
use graphica_core::catalog::api_types::{ColumnDefinition, SchemaDefinition, TableDefinition};
use graphica_core::catalog::connectors::enhanced_inference::{
    normalize_postgres_type, ColumnInferenceEngine,
};
use graphica_core::catalog::schema_to_rdf::SchemaRdfConverter;
use graphica_core::inference::types::SemanticType;

#[tokio::test]
async fn test_end_to_end_schema_inference_pipeline() {
    // Step 1: Simulate basic schema inference from PostgreSQL
    let mut schema = SchemaDefinition {
        name: "public".to_string(),
        tables: vec![TableDefinition {
            name: "users".to_string(),
            columns: vec![
                ColumnDefinition {
                    name: "id".to_string(),
                    data_type: "integer".to_string(),
                    nullable: false,
                    primary_key: true,
                    default_value: Some("nextval('users_id_seq')".to_string()),
                    semantic_type: None,
                    statistics: None,
                },
                ColumnDefinition {
                    name: "email".to_string(),
                    data_type: "character varying".to_string(),
                    nullable: false,
                    primary_key: false,
                    default_value: None,
                    semantic_type: None,
                    statistics: None,
                },
                ColumnDefinition {
                    name: "phone_number".to_string(),
                    data_type: "character varying".to_string(),
                    nullable: true,
                    primary_key: false,
                    default_value: None,
                    semantic_type: None,
                    statistics: None,
                },
                ColumnDefinition {
                    name: "created_at".to_string(),
                    data_type: "timestamp without time zone".to_string(),
                    nullable: false,
                    primary_key: false,
                    default_value: Some("CURRENT_TIMESTAMP".to_string()),
                    semantic_type: None,
                    statistics: None,
                },
            ],
            estimated_rows: Some(10000),
        }],
        relationships: vec![],
        indexes: vec![],
        inferred_at: Utc::now(),
    };

    // Step 2: Enhance with semantic type detection
    let engine = ColumnInferenceEngine::new("public", "users");

    for table in &mut schema.tables {
        for column in &mut table.columns {
            let enriched = engine
                .enrich_column(
                    column.clone(),
                    None,   // No pg_stats data for this test
                    vec![], // No sample values
                    table.estimated_rows,
                )
                .await
                .unwrap();

            *column = enriched;
        }
    }

    // Step 3: Verify semantic types were detected
    let email_col = schema.tables[0]
        .columns
        .iter()
        .find(|c| c.name == "email")
        .expect("Email column should exist");

    assert!(
        email_col.semantic_type.is_some(),
        "Email column should have semantic type"
    );
    assert_eq!(
        email_col.semantic_type.as_ref().unwrap(),
        &SemanticType::Email,
        "Email column should be detected as Email type"
    );

    let phone_col = schema.tables[0]
        .columns
        .iter()
        .find(|c| c.name == "phone_number")
        .expect("Phone column should exist");

    assert!(
        phone_col.semantic_type.is_some(),
        "Phone column should have semantic type"
    );
    assert_eq!(
        phone_col.semantic_type.as_ref().unwrap(),
        &SemanticType::PhoneNumber,
        "Phone column should be detected as PhoneNumber type"
    );

    // Step 4: Convert to RDF triples
    let converter = SchemaRdfConverter::new("test_source");
    let triples = converter.convert_schema(&schema).unwrap();

    // Verify RDF triples were generated
    assert!(triples.len() > 0, "Should generate RDF triples");

    // Check for semantic type triples
    let semantic_type_triples: Vec<_> = triples
        .iter()
        .filter(|t| t.predicate.contains("semanticType"))
        .collect();

    assert!(
        semantic_type_triples.len() >= 2,
        "Should have at least 2 semantic type triples (email and phone), got {}",
        semantic_type_triples.len()
    );

    // Step 5: Generate Turtle representation
    let turtle = SchemaRdfConverter::triples_to_turtle(&triples);

    // Verify Turtle output
    assert!(turtle.contains("@prefix"), "Turtle should have prefixes");
    assert!(
        turtle.contains("semanticType"),
        "Turtle should include semantic type"
    );
    assert!(
        turtle.contains("Email") || turtle.contains("email"),
        "Turtle should reference email type"
    );

    println!("✅ End-to-end schema inference pipeline test passed!");
    println!("   - Generated {} RDF triples", triples.len());
    println!(
        "   - Detected {} semantic types",
        semantic_type_triples.len()
    );
    println!(
        "\nSample Turtle output:\n{}",
        &turtle[..turtle.len().min(500)]
    );
}

#[test]
fn test_postgres_type_normalization() {
    assert_eq!(normalize_postgres_type("character varying(255)"), "varchar");
    assert_eq!(normalize_postgres_type("character varying"), "varchar");
    assert_eq!(normalize_postgres_type("integer"), "int");
    assert_eq!(
        normalize_postgres_type("timestamp without time zone"),
        "timestamp"
    );
    assert_eq!(
        normalize_postgres_type("timestamp with time zone"),
        "timestamptz"
    );
}

#[tokio::test]
async fn test_column_enrichment_with_statistics() {
    use serde_json::json;
    use std::collections::HashMap;

    let engine = ColumnInferenceEngine::new("public", "customers");

    // Simulate PostgreSQL statistics
    let pg_stats_row: HashMap<String, serde_json::Value> = [
        ("column_name".to_string(), json!("email")),
        ("null_frac".to_string(), json!(0.05)),
        ("avg_width".to_string(), json!(32)),
        ("n_distinct".to_string(), json!(9500.0)),
        ("correlation".to_string(), json!(0.12)),
    ]
    .into_iter()
    .collect();

    let column = ColumnDefinition {
        name: "email".to_string(),
        data_type: "varchar".to_string(),
        nullable: false,
        primary_key: false,
        default_value: None,
        semantic_type: None,
        statistics: None,
    };

    let enriched = engine
        .enrich_column(
            column,
            Some(&pg_stats_row),
            vec!["test@example.com".to_string()],
            Some(10000),
        )
        .await
        .unwrap();

    // Verify statistics were extracted
    assert!(enriched.statistics.is_some(), "Should have statistics");
    let stats = enriched.statistics.unwrap();

    assert_eq!(
        stats.null_percentage, 5.0,
        "Should have correct null percentage (as percentage)"
    );
    assert_eq!(stats.avg_width, Some(32), "Should have average width");
    assert!(stats.correlation.is_some(), "Should have correlation");

    // Verify semantic type was detected
    assert_eq!(
        enriched.semantic_type.as_ref().unwrap(),
        &SemanticType::Email,
        "Should detect Email semantic type"
    );
}

#[test]
fn test_rdf_triple_generation_for_statistics() {
    use graphica_core::inference::types::{CardinalityClass, ColumnStatistics};

    let schema = SchemaDefinition {
        name: "public".to_string(),
        tables: vec![TableDefinition {
            name: "products".to_string(),
            columns: vec![ColumnDefinition {
                name: "sku".to_string(),
                data_type: "varchar".to_string(),
                nullable: false,
                primary_key: true,
                default_value: None,
                semantic_type: Some(SemanticType::ProductCode),
                statistics: Some(ColumnStatistics {
                    distinct_count: Some(50000),
                    null_count: 0,
                    null_percentage: 0.0,
                    min_value: Some("A001".to_string()),
                    max_value: Some("Z999".to_string()),
                    avg_length: Some(12.0),
                    histogram: None,
                    most_common_values: None,
                    correlation: Some(0.95),
                    n_distinct: Some(50000.0),
                    avg_width: Some(12),
                    cardinality: Some(CardinalityClass::Unique),
                    sample_size: Some(50000),
                    last_analyzed: None,
                    statistics_stale: false,
                }),
            }],
            estimated_rows: Some(50000),
        }],
        relationships: vec![],
        indexes: vec![],
        inferred_at: Utc::now(),
    };

    let converter = SchemaRdfConverter::new("test_source");
    let triples = converter.convert_schema(&schema).unwrap();

    // Verify statistics triples
    let stat_triples: Vec<_> = triples
        .iter()
        .filter(|t| t.predicate.contains("inference"))
        .collect();

    assert!(stat_triples.len() > 0, "Should have statistics triples");

    // Check for specific statistics
    let has_distinct_count = triples
        .iter()
        .any(|t| t.predicate.contains("distinctCount"));
    let has_null_pct = triples
        .iter()
        .any(|t| t.predicate.contains("nullPercentage"));
    let has_cardinality = triples
        .iter()
        .any(|t| t.predicate.contains("cardinalityClass"));

    assert!(has_distinct_count, "Should include distinct count");
    assert!(has_null_pct, "Should include null percentage");
    assert!(has_cardinality, "Should include cardinality class");
}
