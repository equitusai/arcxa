//! Integration tests for Field Lineage API
//!
//! Tests golden record creation, field resolution, and conflict detection.

use chrono::Utc;
use graphica_core::orchestration::field_lineage::{
    FieldResolver, SourceValue, StrategyType, VotingStrategy,
};
use std::collections::HashMap;

/// Test creating a golden record with frequency voting
#[test]
fn test_golden_record_creation_frequency_voting() {
    let resolver = FieldResolver::new();

    // Create source values for customer email field
    let email_sources = vec![
        SourceValue {
            id: "src_email_1".to_string(),
            value: serde_json::json!("john.doe@example.com"),
            source_system: "CRM".to_string(),
            source_timestamp: Utc::now(),
            source_authority: 0.9,
            confidence: Some(0.95),
            vote_count: 0,
            vote_weight: 1.0,
            metadata: HashMap::new(),
        },
        SourceValue {
            id: "src_email_2".to_string(),
            value: serde_json::json!("john.doe@example.com"), // Same value
            source_system: "Email".to_string(),
            source_timestamp: Utc::now(),
            source_authority: 0.7,
            confidence: Some(0.85),
            vote_count: 0,
            vote_weight: 1.0,
            metadata: HashMap::new(),
        },
        SourceValue {
            id: "src_email_3".to_string(),
            value: serde_json::json!("j.doe@example.com"), // Different value
            source_system: "Website".to_string(),
            source_timestamp: Utc::now(),
            source_authority: 0.5,
            confidence: Some(0.70),
            vote_count: 0,
            vote_weight: 1.0,
            metadata: HashMap::new(),
        },
    ];

    // Create source values for customer name field
    let name_sources = vec![
        SourceValue {
            id: "src_name_1".to_string(),
            value: serde_json::json!("John Doe"),
            source_system: "CRM".to_string(),
            source_timestamp: Utc::now(),
            source_authority: 0.9,
            confidence: Some(0.95),
            vote_count: 0,
            vote_weight: 1.0,
            metadata: HashMap::new(),
        },
        SourceValue {
            id: "src_name_2".to_string(),
            value: serde_json::json!("John Doe"),
            source_system: "Email".to_string(),
            source_timestamp: Utc::now(),
            source_authority: 0.8,
            confidence: Some(0.90),
            vote_count: 0,
            vote_weight: 1.0,
            metadata: HashMap::new(),
        },
    ];

    // Resolve fields
    let mut fields = HashMap::new();
    fields.insert("email".to_string(), email_sources);
    fields.insert("name".to_string(), name_sources);

    let resolutions = resolver
        .resolve_fields("customer_123", fields, None)
        .expect("Field resolution should succeed");

    assert_eq!(resolutions.len(), 2, "Should resolve 2 fields");

    // Create golden record
    let golden_record = resolver
        .create_resolved_entity("customer_123", resolutions)
        .expect("Golden record creation should succeed");

    assert_eq!(golden_record.entity_id, "customer_123");
    assert_eq!(golden_record.fields.len(), 2);
    assert!(golden_record.overall_confidence > 0.0);

    // Verify email field resolution
    let email_field = golden_record
        .get_field("email")
        .expect("Email field should exist");
    assert_eq!(email_field.value, serde_json::json!("john.doe@example.com"));
    assert!(email_field.confidence > 0.5);

    // Verify name field resolution
    let name_field = golden_record
        .get_field("name")
        .expect("Name field should exist");
    assert_eq!(name_field.value, serde_json::json!("John Doe"));
    assert!(name_field.confidence > 0.5);
}

/// Test conflict detection with evenly split values
#[test]
fn test_conflict_detection_even_split() {
    let resolver = FieldResolver::new();

    // Create evenly split source values (high conflict)
    let sources = vec![
        SourceValue {
            id: "src_1".to_string(),
            value: serde_json::json!("value_A"),
            source_system: "System1".to_string(),
            source_timestamp: Utc::now(),
            source_authority: 0.9,
            confidence: Some(0.95),
            vote_count: 0,
            vote_weight: 1.0,
            metadata: HashMap::new(),
        },
        SourceValue {
            id: "src_2".to_string(),
            value: serde_json::json!("value_B"),
            source_system: "System2".to_string(),
            source_timestamp: Utc::now(),
            source_authority: 0.9,
            confidence: Some(0.95),
            vote_count: 0,
            vote_weight: 1.0,
            metadata: HashMap::new(),
        },
    ];

    let resolution = resolver
        .resolve_field("entity_123", "field_name", sources, None)
        .expect("Resolution should succeed");

    // Verify conflict was detected
    assert!(resolution.conflict.is_some(), "Conflict should be detected");

    let conflict = resolution.conflict.unwrap();
    assert!(conflict.requires_review, "Even split should require review");
    assert_eq!(conflict.conflicting_values.len(), 2);
}

/// Test authority-based voting
#[test]
fn test_authority_voting_strategy() {
    let resolver = FieldResolver::with_strategy(StrategyType::Authority);

    let sources = vec![
        SourceValue {
            id: "src_low_auth".to_string(),
            value: serde_json::json!("low_authority_value"),
            source_system: "Website".to_string(),
            source_timestamp: Utc::now(),
            source_authority: 0.3,
            confidence: Some(0.7),
            vote_count: 0,
            vote_weight: 1.0,
            metadata: HashMap::new(),
        },
        SourceValue {
            id: "src_high_auth".to_string(),
            value: serde_json::json!("high_authority_value"),
            source_system: "CRM".to_string(),
            source_timestamp: Utc::now(),
            source_authority: 0.95,
            confidence: Some(0.98),
            vote_count: 0,
            vote_weight: 1.0,
            metadata: HashMap::new(),
        },
        SourceValue {
            id: "src_med_auth".to_string(),
            value: serde_json::json!("medium_authority_value"),
            source_system: "Email".to_string(),
            source_timestamp: Utc::now(),
            source_authority: 0.6,
            confidence: Some(0.8),
            vote_count: 0,
            vote_weight: 1.0,
            metadata: HashMap::new(),
        },
    ];

    let resolution = resolver
        .resolve_field("entity_123", "field_name", sources, None)
        .expect("Resolution should succeed");

    // High authority value should win
    assert_eq!(
        resolution.selected_value.value,
        serde_json::json!("high_authority_value")
    );
    assert_eq!(resolution.rejected_values.len(), 2);
}

/// Test time decay voting with recent values
#[test]
fn test_time_decay_voting_strategy() {
    use chrono::Duration;

    let resolver = FieldResolver::with_strategy(StrategyType::TimeDecay);

    let now = Utc::now();

    let sources = vec![
        SourceValue {
            id: "src_old".to_string(),
            value: serde_json::json!("old_value"),
            source_system: "System1".to_string(),
            source_timestamp: now - Duration::days(30),
            source_authority: 0.8,
            confidence: Some(0.9),
            vote_count: 0,
            vote_weight: 1.0,
            metadata: HashMap::new(),
        },
        SourceValue {
            id: "src_recent".to_string(),
            value: serde_json::json!("recent_value"),
            source_system: "System2".to_string(),
            source_timestamp: now - Duration::hours(1),
            source_authority: 0.7,
            confidence: Some(0.85),
            vote_count: 0,
            vote_weight: 1.0,
            metadata: HashMap::new(),
        },
    ];

    let strategy = VotingStrategy {
        strategy_type: StrategyType::TimeDecay,
        parameters: serde_json::json!({
            "decay_rate": 0.1,
        }),
        description: "Time decay with 0.1 rate".to_string(),
    };

    let resolution = resolver
        .resolve_field("entity_123", "field_name", sources, Some(strategy))
        .expect("Resolution should succeed");

    // Recent value should win despite lower authority
    assert_eq!(
        resolution.selected_value.value,
        serde_json::json!("recent_value")
    );
}

/// Test ensemble voting combines multiple strategies
#[test]
fn test_ensemble_voting_strategy() {
    let resolver = FieldResolver::with_strategy(StrategyType::Ensemble);

    let sources = vec![
        SourceValue {
            id: "src_1".to_string(),
            value: serde_json::json!("value_A"),
            source_system: "CRM".to_string(),
            source_timestamp: Utc::now(),
            source_authority: 0.9,
            confidence: Some(0.95),
            vote_count: 0,
            vote_weight: 1.0,
            metadata: HashMap::new(),
        },
        SourceValue {
            id: "src_2".to_string(),
            value: serde_json::json!("value_A"), // Same value
            source_system: "Email".to_string(),
            source_timestamp: Utc::now(),
            source_authority: 0.8,
            confidence: Some(0.90),
            vote_count: 0,
            vote_weight: 1.0,
            metadata: HashMap::new(),
        },
        SourceValue {
            id: "src_3".to_string(),
            value: serde_json::json!("value_B"),
            source_system: "Website".to_string(),
            source_timestamp: Utc::now(),
            source_authority: 0.5,
            confidence: Some(0.70),
            vote_count: 0,
            vote_weight: 1.0,
            metadata: HashMap::new(),
        },
    ];

    // Create ensemble strategy with multiple sub-strategies
    let strategy = VotingStrategy {
        strategy_type: StrategyType::Ensemble,
        parameters: serde_json::json!({
            "strategies": [
                {"type": "frequency", "weight": 1.0},
                {"type": "authority", "weight": 0.5}
            ]
        }),
        description: "Ensemble: frequency + authority".to_string(),
    };

    let resolution = resolver
        .resolve_field("entity_123", "field_name", sources, Some(strategy))
        .expect("Resolution should succeed");

    // value_A should win (majority + higher authority)
    assert_eq!(
        resolution.selected_value.value,
        serde_json::json!("value_A")
    );
    assert_eq!(resolution.strategy.strategy_type, StrategyType::Ensemble);
}

/// Test golden record JSON serialization
#[test]
fn test_golden_record_json_serialization() {
    let resolver = FieldResolver::new();

    let sources = vec![SourceValue {
        id: "src_1".to_string(),
        value: serde_json::json!("test_value"),
        source_system: "TestSystem".to_string(),
        source_timestamp: Utc::now(),
        source_authority: 0.9,
        confidence: Some(0.95),
        vote_count: 0,
        vote_weight: 1.0,
        metadata: HashMap::new(),
    }];

    let mut fields = HashMap::new();
    fields.insert("test_field".to_string(), sources);

    let resolutions = resolver
        .resolve_fields("entity_123", fields, None)
        .expect("Resolution should succeed");

    let golden_record = resolver
        .create_resolved_entity("entity_123", resolutions)
        .expect("Golden record creation should succeed");

    // Convert to JSON
    let json = golden_record.to_json();

    // Verify JSON structure
    assert_eq!(json["entity_id"], "entity_123");
    assert!(json["fields"].is_object());
    assert!(json["overall_confidence"].is_number());
    assert!(json["created_at"].is_string());
    assert_eq!(json["conflict_count"], 0);
    assert_eq!(json["requires_review"], false);
}

/// Test low confidence field detection
#[test]
fn test_low_confidence_field_detection() {
    let resolver = FieldResolver::new().with_min_confidence(0.80);

    // Create sources that will result in low confidence
    let low_conf_sources = vec![
        SourceValue {
            id: "src_1".to_string(),
            value: serde_json::json!("value_A"),
            source_system: "System1".to_string(),
            source_timestamp: Utc::now(),
            source_authority: 0.4,
            confidence: Some(0.5),
            vote_count: 0,
            vote_weight: 1.0,
            metadata: HashMap::new(),
        },
        SourceValue {
            id: "src_2".to_string(),
            value: serde_json::json!("value_B"),
            source_system: "System2".to_string(),
            source_timestamp: Utc::now(),
            source_authority: 0.4,
            confidence: Some(0.5),
            vote_count: 0,
            vote_weight: 1.0,
            metadata: HashMap::new(),
        },
        SourceValue {
            id: "src_3".to_string(),
            value: serde_json::json!("value_C"),
            source_system: "System3".to_string(),
            source_timestamp: Utc::now(),
            source_authority: 0.4,
            confidence: Some(0.5),
            vote_count: 0,
            vote_weight: 1.0,
            metadata: HashMap::new(),
        },
    ];

    let mut fields = HashMap::new();
    fields.insert("uncertain_field".to_string(), low_conf_sources);

    let resolutions = resolver
        .resolve_fields("entity_123", fields, None)
        .expect("Resolution should succeed");

    let golden_record = resolver
        .create_resolved_entity("entity_123", resolutions)
        .expect("Golden record creation should succeed");

    // Check for low confidence fields
    let low_conf_fields = golden_record.low_confidence_fields(0.80);

    // With 3 different values, confidence should be low
    assert!(
        !low_conf_fields.is_empty(),
        "Should detect low confidence field"
    );
}

/// Test field resolution with metadata
#[test]
fn test_field_resolution_with_metadata() {
    let resolver = FieldResolver::new();

    let mut metadata = HashMap::new();
    metadata.insert("data_quality_score".to_string(), "0.95".to_string());
    metadata.insert("last_verified".to_string(), "2024-01-15".to_string());

    let sources = vec![SourceValue {
        id: "src_1".to_string(),
        value: serde_json::json!("verified_value"),
        source_system: "TrustedSource".to_string(),
        source_timestamp: Utc::now(),
        source_authority: 0.95,
        confidence: Some(0.98),
        vote_count: 0,
        vote_weight: 1.0,
        metadata,
    }];

    let resolution = resolver
        .resolve_field("entity_123", "verified_field", sources, None)
        .expect("Resolution should succeed");

    // Verify metadata is preserved
    assert!(resolution
        .selected_value
        .metadata
        .contains_key("data_quality_score"));
    assert_eq!(
        resolution.selected_value.metadata.get("data_quality_score"),
        Some(&"0.95".to_string())
    );
}
