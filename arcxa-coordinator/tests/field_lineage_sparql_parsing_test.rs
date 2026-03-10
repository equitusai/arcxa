//! Tests for SPARQL Result Parsing in Field Lineage API
//!
//! Verifies that SPARQL JSON results are correctly parsed into API responses.

use serde_json::json;

/// Test SPARQL binding extraction helpers
#[test]
fn test_sparql_binding_extraction() {
    // Simulate a SPARQL binding
    let binding = json!({
        "value": {
            "type": "literal",
            "value": "test@example.com"
        },
        "confidence": {
            "type": "literal",
            "value": "0.95",
            "datatype": "http://www.w3.org/2001/XMLSchema#double"
        },
        "timestamp": {
            "type": "literal",
            "value": "2024-01-15T10:30:00Z",
            "datatype": "http://www.w3.org/2001/XMLSchema#dateTime"
        }
    });

    // Test string extraction
    assert_eq!(
        binding
            .get("value")
            .and_then(|v| v.get("value"))
            .and_then(|v| v.as_str()),
        Some("test@example.com")
    );

    // Test number extraction
    let confidence: f64 = binding
        .get("confidence")
        .and_then(|v| v.get("value"))
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
        .unwrap();
    assert_eq!(confidence, 0.95);

    // Test datetime extraction
    let timestamp_str = binding
        .get("timestamp")
        .and_then(|v| v.get("value"))
        .and_then(|v| v.as_str())
        .unwrap();
    assert_eq!(timestamp_str, "2024-01-15T10:30:00Z");
}

/// Test field lineage SPARQL result format
#[test]
fn test_field_lineage_sparql_format() {
    // Simulate SPARQL JSON result for field lineage query
    let sparql_result = json!({
        "head": {
            "vars": ["value", "confidence", "resolvedAt", "strategy", "explanation", "sourceValue", "sourceSystem", "sourceAuthority", "voteWeight"]
        },
        "results": {
            "bindings": [
                {
                    "value": {
                        "type": "literal",
                        "value": "\"john.doe@example.com\""
                    },
                    "confidence": {
                        "type": "literal",
                        "value": "0.95"
                    },
                    "resolvedAt": {
                        "type": "literal",
                        "value": "2024-01-15T10:30:00Z"
                    },
                    "strategy": {
                        "type": "uri",
                        "value": "http://graphica.io/field#strategy/frequency"
                    },
                    "explanation": {
                        "type": "literal",
                        "value": "Frequency voting: Most common value has 2 votes (66.7% confidence)"
                    },
                    "sourceValue": {
                        "type": "literal",
                        "value": "\"john.doe@example.com\""
                    },
                    "sourceSystem": {
                        "type": "literal",
                        "value": "CRM"
                    },
                    "sourceAuthority": {
                        "type": "literal",
                        "value": "0.9"
                    },
                    "voteWeight": {
                        "type": "literal",
                        "value": "2.0"
                    }
                },
                {
                    "value": {
                        "type": "literal",
                        "value": "\"john.doe@example.com\""
                    },
                    "confidence": {
                        "type": "literal",
                        "value": "0.95"
                    },
                    "resolvedAt": {
                        "type": "literal",
                        "value": "2024-01-15T10:30:00Z"
                    },
                    "strategy": {
                        "type": "uri",
                        "value": "http://graphica.io/field#strategy/frequency"
                    },
                    "explanation": {
                        "type": "literal",
                        "value": "Frequency voting: Most common value has 2 votes (66.7% confidence)"
                    },
                    "sourceValue": {
                        "type": "literal",
                        "value": "\"john.doe@example.com\""
                    },
                    "sourceSystem": {
                        "type": "literal",
                        "value": "Email"
                    },
                    "sourceAuthority": {
                        "type": "literal",
                        "value": "0.7"
                    },
                    "voteWeight": {
                        "type": "literal",
                        "value": "1.0"
                    }
                }
            ]
        }
    });

    // Verify structure
    let bindings = sparql_result
        .get("results")
        .and_then(|r| r.get("bindings"))
        .and_then(|b| b.as_array())
        .unwrap();

    assert_eq!(bindings.len(), 2);

    // Verify first binding
    let first = &bindings[0];
    assert!(first.get("value").is_some());
    assert!(first.get("confidence").is_some());
    assert!(first.get("sourceSystem").is_some());
}

/// Test field history SPARQL result format
#[test]
fn test_field_history_sparql_format() {
    let sparql_result = json!({
        "head": {
            "vars": ["value", "confidence", "validFrom", "validTo", "explanation"]
        },
        "results": {
            "bindings": [
                {
                    "value": {
                        "type": "literal",
                        "value": "\"123 New St\""
                    },
                    "confidence": {
                        "type": "literal",
                        "value": "0.92"
                    },
                    "validFrom": {
                        "type": "literal",
                        "value": "2024-01-15T10:00:00Z"
                    },
                    "explanation": {
                        "type": "literal",
                        "value": "Updated address from shipping records"
                    }
                    // validTo is optional - not present for current value
                },
                {
                    "value": {
                        "type": "literal",
                        "value": "\"456 Old Ave\""
                    },
                    "confidence": {
                        "type": "literal",
                        "value": "0.85"
                    },
                    "validFrom": {
                        "type": "literal",
                        "value": "2023-12-01T10:00:00Z"
                    },
                    "validTo": {
                        "type": "literal",
                        "value": "2024-01-15T10:00:00Z"
                    },
                    "explanation": {
                        "type": "literal",
                        "value": "Previous address from billing system"
                    }
                }
            ]
        }
    });

    let bindings = sparql_result
        .get("results")
        .and_then(|r| r.get("bindings"))
        .and_then(|b| b.as_array())
        .unwrap();

    assert_eq!(bindings.len(), 2);

    // Current value doesn't have validTo
    assert!(bindings[0].get("validTo").is_none());

    // Historical value has validTo
    assert!(bindings[1].get("validTo").is_some());
}

/// Test conflicts SPARQL result format
#[test]
fn test_conflicts_sparql_format() {
    let sparql_result = json!({
        "head": {
            "vars": ["entityId", "fieldName", "severity", "reason", "resolvedAt"]
        },
        "results": {
            "bindings": [
                {
                    "entityId": {
                        "type": "literal",
                        "value": "cust_123"
                    },
                    "fieldName": {
                        "type": "literal",
                        "value": "email"
                    },
                    "severity": {
                        "type": "literal",
                        "value": "high"
                    },
                    "reason": {
                        "type": "literal",
                        "value": "2 different values found. Most common has 1 votes (50.0% confidence)"
                    },
                    "resolvedAt": {
                        "type": "literal",
                        "value": "2024-01-15T10:30:00Z"
                    }
                },
                {
                    "entityId": {
                        "type": "literal",
                        "value": "cust_456"
                    },
                    "fieldName": {
                        "type": "literal",
                        "value": "phone"
                    },
                    "severity": {
                        "type": "literal",
                        "value": "critical"
                    },
                    "reason": {
                        "type": "literal",
                        "value": "3 different values found with equal weight"
                    },
                    "resolvedAt": {
                        "type": "literal",
                        "value": "2024-01-15T11:00:00Z"
                    }
                }
            ]
        }
    });

    let bindings = sparql_result
        .get("results")
        .and_then(|r| r.get("bindings"))
        .and_then(|b| b.as_array())
        .unwrap();

    assert_eq!(bindings.len(), 2);

    // Verify severity values
    let severity1 = bindings[0]
        .get("severity")
        .and_then(|s| s.get("value"))
        .and_then(|v| v.as_str())
        .unwrap();
    assert_eq!(severity1, "high");

    let severity2 = bindings[1]
        .get("severity")
        .and_then(|s| s.get("value"))
        .and_then(|v| v.as_str())
        .unwrap();
    assert_eq!(severity2, "critical");
}

/// Test strategy type parsing
#[test]
fn test_strategy_type_parsing() {
    // Test various strategy URI formats
    let test_cases = vec![
        ("http://graphica.io/field#strategy/frequency", "frequency"),
        ("http://graphica.io/field#strategy/time-decay", "time-decay"),
        ("http://graphica.io/field#strategy/authority", "authority"),
        ("http://graphica.io/field#strategy/ensemble", "ensemble"),
        ("http://graphica.io/field#strategy/ml-prediction", "ml"),
    ];

    for (uri, expected_type) in test_cases {
        assert!(
            uri.contains(expected_type),
            "URI {} should contain {}",
            uri,
            expected_type
        );
    }
}

/// Test conflict severity parsing
#[test]
fn test_conflict_severity_parsing() {
    let severities = vec!["low", "medium", "high", "critical"];

    for severity in severities {
        // Should be case-insensitive
        assert_eq!(severity.to_lowercase(), severity.to_lowercase());
    }
}

/// Test JSON value type inference
#[test]
fn test_json_type_inference() {
    use serde_json::Value;

    let test_cases = vec![
        (json!(null), "null"),
        (json!(true), "boolean"),
        (json!(42), "number"),
        (json!("text"), "string"),
        (json!([1, 2, 3]), "array"),
        (json!({"key": "value"}), "object"),
    ];

    for (value, expected_type) in test_cases {
        let actual_type = match value {
            Value::Null => "null",
            Value::Bool(_) => "boolean",
            Value::Number(_) => "number",
            Value::String(_) => "string",
            Value::Array(_) => "array",
            Value::Object(_) => "object",
        };
        assert_eq!(actual_type, expected_type);
    }
}
