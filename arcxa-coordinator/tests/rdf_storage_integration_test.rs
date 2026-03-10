//! Integration Test for RDF Storage
//!
//! Tests the complete flow from enhanced schema inference to RDF triple storage.

use chrono::Utc;
use graphica_coordinator::api::rdf_storage::RdfStorageClient;
use graphica_core::catalog::api_types::{ColumnDefinition, SchemaDefinition, TableDefinition};
use graphica_core::catalog::schema_to_rdf::{RdfNode, RdfTriple, SchemaRdfConverter};
use graphica_core::inference::types::SemanticType;

#[test]
fn test_triple_conversion_all_types() {
    let client = RdfStorageClient::new("http://localhost:9090");

    // Test all RdfNode variants
    let test_cases = vec![
        (
            "URI object",
            RdfTriple {
                subject: "http://example.com/subject".to_string(),
                predicate: "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_string(),
                object: RdfNode::Uri("http://example.com/Type".to_string()),
            },
        ),
        (
            "Simple literal",
            RdfTriple {
                subject: "http://example.com/subject".to_string(),
                predicate: "http://purl.org/dc/terms/title".to_string(),
                object: RdfNode::Literal("Example Title".to_string()),
            },
        ),
        (
            "Typed literal (integer)",
            RdfTriple {
                subject: "http://example.com/subject".to_string(),
                predicate: "http://example.com/count".to_string(),
                object: RdfNode::TypedLiteral(
                    "42".to_string(),
                    "http://www.w3.org/2001/XMLSchema#integer".to_string(),
                ),
            },
        ),
        (
            "Typed literal (double)",
            RdfTriple {
                subject: "http://example.com/subject".to_string(),
                predicate: "http://example.com/percentage".to_string(),
                object: RdfNode::TypedLiteral(
                    "95.5".to_string(),
                    "http://www.w3.org/2001/XMLSchema#double".to_string(),
                ),
            },
        ),
        (
            "Language literal",
            RdfTriple {
                subject: "http://example.com/subject".to_string(),
                predicate: "http://www.w3.org/2000/01/rdf-schema#label".to_string(),
                object: RdfNode::LangLiteral("Hello World".to_string(), "en".to_string()),
            },
        ),
    ];

    for (name, triple) in test_cases {
        let proto = client.convert_to_proto(&triple);

        // Verify subject and predicate are always preserved
        assert_eq!(proto.subject, triple.subject, "Failed for: {}", name);
        assert_eq!(proto.predicate, triple.predicate, "Failed for: {}", name);

        // Verify object conversion based on type
        match &triple.object {
            RdfNode::Uri(uri) => {
                assert_eq!(proto.object, *uri, "URI object mismatch for: {}", name);
                assert_eq!(proto.object_datatype, "", "URI should have empty datatype");
                assert_eq!(proto.object_language, "", "URI should have empty language");
            }
            RdfNode::Literal(lit) => {
                assert_eq!(proto.object, *lit, "Literal object mismatch for: {}", name);
                assert_eq!(
                    proto.object_datatype, "",
                    "Literal should have empty datatype"
                );
                assert_eq!(
                    proto.object_language, "",
                    "Literal should have empty language"
                );
            }
            RdfNode::TypedLiteral(value, datatype) => {
                assert_eq!(
                    proto.object, *value,
                    "Typed literal value mismatch for: {}",
                    name
                );
                assert_eq!(
                    proto.object_datatype, *datatype,
                    "Datatype mismatch for: {}",
                    name
                );
                assert_eq!(
                    proto.object_language, "",
                    "Typed literal should have empty language"
                );
            }
            RdfNode::LangLiteral(value, lang) => {
                assert_eq!(
                    proto.object, *value,
                    "Lang literal value mismatch for: {}",
                    name
                );
                assert_eq!(
                    proto.object_datatype, "",
                    "Lang literal should have empty datatype"
                );
                assert_eq!(
                    proto.object_language, *lang,
                    "Language mismatch for: {}",
                    name
                );
            }
        }

        // Verify graph is set to default
        assert_eq!(proto.graph, "http://graphica.io/catalog/inferred");
    }
}

#[test]
fn test_schema_to_rdf_to_proto_roundtrip() {
    // Create a schema with semantic types and statistics
    let schema = SchemaDefinition {
        name: "test_schema".to_string(),
        tables: vec![TableDefinition {
            name: "users".to_string(),
            columns: vec![
                ColumnDefinition {
                    name: "id".to_string(),
                    data_type: "integer".to_string(),
                    nullable: false,
                    primary_key: true,
                    default_value: None,
                    semantic_type: None,
                    statistics: None,
                },
                ColumnDefinition {
                    name: "email".to_string(),
                    data_type: "varchar".to_string(),
                    nullable: false,
                    primary_key: false,
                    default_value: None,
                    semantic_type: Some(SemanticType::Email),
                    statistics: None,
                },
                ColumnDefinition {
                    name: "phone_number".to_string(),
                    data_type: "varchar".to_string(),
                    nullable: true,
                    primary_key: false,
                    default_value: None,
                    semantic_type: Some(SemanticType::PhoneNumber),
                    statistics: None,
                },
            ],
            estimated_rows: Some(1000),
        }],
        relationships: vec![],
        indexes: vec![],
        inferred_at: Utc::now(),
    };

    // Convert schema to RDF triples
    let converter = SchemaRdfConverter::new("test_source");
    let triples = converter
        .convert_schema(&schema)
        .expect("Schema conversion failed");

    // Verify we got triples
    assert!(triples.len() > 0, "Should generate RDF triples");

    // Convert to proto format
    let client = RdfStorageClient::new("http://localhost:9090");
    let proto_triples: Vec<_> = triples.iter().map(|t| client.convert_to_proto(t)).collect();

    // Verify all triples were converted
    assert_eq!(
        proto_triples.len(),
        triples.len(),
        "All triples should be converted"
    );

    // Verify semantic type triples exist
    let semantic_type_triples: Vec<_> = proto_triples
        .iter()
        .filter(|t| t.predicate.contains("semanticType"))
        .collect();

    assert!(
        semantic_type_triples.len() >= 2,
        "Should have at least 2 semantic type triples (email and phone), got {}",
        semantic_type_triples.len()
    );

    // Verify all proto triples have proper graph set
    for triple in &proto_triples {
        assert_eq!(
            triple.graph, "http://graphica.io/catalog/inferred",
            "All triples should have correct graph"
        );
    }

    // Verify triples contain expected predicates
    let predicates: Vec<&str> = proto_triples.iter().map(|t| t.predicate.as_str()).collect();

    assert!(
        predicates.iter().any(|p| p.contains("type")),
        "Should have rdf:type"
    );
    assert!(
        predicates
            .iter()
            .any(|p| p.contains("title") || p.contains("name")),
        "Should have title/name"
    );
    assert!(
        predicates.iter().any(|p| p.contains("dataType")),
        "Should have dataType"
    );
}

#[test]
fn test_client_configuration() {
    // Test default configuration
    let default_client = RdfStorageClient::new("http://localhost:9090");

    // Create a test triple to verify default graph
    let triple = RdfTriple {
        subject: "http://test/s".to_string(),
        predicate: "http://test/p".to_string(),
        object: RdfNode::Literal("test".to_string()),
    };

    let proto = default_client.convert_to_proto(&triple);
    assert_eq!(proto.graph, "http://graphica.io/catalog/inferred");

    // Test custom graph configuration
    let custom_client = RdfStorageClient::new("http://localhost:9090")
        .with_default_graph("http://custom.graph/test");

    let custom_proto = custom_client.convert_to_proto(&triple);
    assert_eq!(custom_proto.graph, "http://custom.graph/test");
}

#[test]
fn test_special_characters_in_literals() {
    let client = RdfStorageClient::new("http://localhost:9090");

    let test_cases = vec![
        ("Quotes", "Hello \"World\""),
        ("Newlines", "Line1\nLine2\nLine3"),
        ("Tabs", "Col1\tCol2\tCol3"),
        ("Unicode", "Héllo Wörld 你好"),
        ("Special chars", "Test@#$%^&*()"),
    ];

    for (name, value) in test_cases {
        let triple = RdfTriple {
            subject: "http://test/s".to_string(),
            predicate: "http://test/p".to_string(),
            object: RdfNode::Literal(value.to_string()),
        };

        let proto = client.convert_to_proto(&triple);
        assert_eq!(
            proto.object, value,
            "Special characters should be preserved for: {}",
            name
        );
    }
}

#[test]
fn test_semantic_type_uri_generation() {
    let schema = SchemaDefinition {
        name: "test".to_string(),
        tables: vec![TableDefinition {
            name: "test_table".to_string(),
            columns: vec![ColumnDefinition {
                name: "email_col".to_string(),
                data_type: "varchar".to_string(),
                nullable: false,
                primary_key: false,
                default_value: None,
                semantic_type: Some(SemanticType::Email),
                statistics: None,
            }],
            estimated_rows: None,
        }],
        relationships: vec![],
        indexes: vec![],
        inferred_at: Utc::now(),
    };

    let converter = SchemaRdfConverter::new("test_source");
    let triples = converter.convert_schema(&schema).unwrap();

    // Find semantic type triple
    let semantic_triple = triples
        .iter()
        .find(|t| t.predicate.contains("semanticType"))
        .expect("Should have semantic type triple");

    // Verify it's a URI reference to the ontology
    match &semantic_triple.object {
        RdfNode::Uri(uri) => {
            assert!(
                uri.contains("graphica.io"),
                "Semantic type URI should reference Graphica ontology"
            );
            assert!(uri.contains("Email"), "Should reference Email type");
        }
        _ => panic!("Semantic type object should be a URI"),
    }
}
