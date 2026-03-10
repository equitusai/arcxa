//! Workflow RDF Lineage Integration Test
//!
//! Tests RDF-first lineage tracking for workflow executions with field-level provenance.

use graphica_coordinator::governance::rdf_store::RdfStore; // Import trait separately for clarity
use graphica_coordinator::governance::rdf_store::{
    GraphicaRdfStore, NamedGraph, RdfTriple, RdfValue,
};
use graphica_coordinator::workflows::lineage::{FieldModification, WorkflowLineageGenerator};
use std::sync::Arc;

/// Test 1: RdfTriple creation and basic operations
#[test]
fn test_rdf_triple_creation() {
    // Test auto-detection constructor
    let triple1 = RdfTriple::new("ex:alice", "rdf:type", "foaf:Person");
    assert_eq!(triple1.subject, "ex:alice");
    assert_eq!(triple1.predicate, "rdf:type");
    match triple1.object {
        RdfValue::Uri(_) => {} // Expected - contains ':'
        _ => panic!("Expected URI"),
    }

    // Test literal constructor
    let triple2 = RdfTriple::new_literal("ex:alice", "foaf:name", "Alice");
    assert_eq!(triple2.subject, "ex:alice");
    match triple2.object {
        RdfValue::Literal(ref s) => assert_eq!(s, "Alice"),
        _ => panic!("Expected Literal"),
    }

    // Test typed literal constructor
    let triple3 = RdfTriple::new_typed("ex:alice", "foaf:age", "30", "xsd:integer");
    match triple3.object {
        RdfValue::TypedLiteral {
            ref value,
            ref datatype,
        } => {
            assert_eq!(value, "30");
            assert_eq!(datatype, "xsd:integer");
        }
        _ => panic!("Expected TypedLiteral"),
    }

    println!("✅ Test 1 passed: RdfTriple creation and types");
}

/// Test 2: insert_batch() method with in-memory store
#[test]
fn test_insert_batch_with_in_memory_store() {
    let store = GraphicaRdfStore::new_in_memory().unwrap();

    let triples = vec![
        RdfTriple::new_uri("http://example.com/alice", "rdf:type", "foaf:Person"),
        RdfTriple::new_literal("http://example.com/alice", "foaf:name", "Alice"),
        RdfTriple::new_typed("http://example.com/alice", "foaf:age", "30", "xsd:integer"),
    ];

    let graph = NamedGraph::workflows();
    let result = store.insert_batch(&triples, Some(&graph));

    assert!(result.is_ok(), "insert_batch should succeed");

    // Verify triples were stored
    let count = store.count_triples(Some(&graph)).unwrap();
    assert_eq!(count, 3, "Should have 3 triples in graph");

    println!("✅ Test 2 passed: insert_batch with in-memory store");
}

/// Test 3: Named graph helpers for workflows
#[test]
fn test_workflow_named_graphs() {
    let wf_graph = NamedGraph::workflows();
    assert_eq!(wf_graph.uri, "http://graphica.io/graph/workflows");

    let exec_graph = NamedGraph::workflow_executions();
    assert_eq!(
        exec_graph.uri,
        "http://graphica.io/graph/workflow-executions"
    );

    println!("✅ Test 3 passed: Workflow named graphs");
}

/// Test 4: RdfTriple to_tuple conversion
#[test]
fn test_rdf_triple_to_tuple() {
    let triple = RdfTriple::new_uri("ex:alice", "rdf:type", "foaf:Person");
    let (subject, predicate, object) = triple.to_tuple();

    assert_eq!(subject, "ex:alice");
    assert_eq!(predicate, "rdf:type");
    assert!(object.contains("foaf:Person"), "Object should contain URI");

    println!("✅ Test 4 passed: RdfTriple to_tuple conversion");
}

/// Test 5: FieldModification structure (from lineage module)
#[test]
fn test_field_modification_structure() {
    use serde_json::json;

    let modification = FieldModification {
        field_name: "email".to_string(),
        old_value: json!("OLD@Example.com"),
        new_value: json!("old@example.com"),
        confidence: 1.0,
        is_reversible: true,
    };

    assert_eq!(modification.field_name, "email");
    assert_eq!(modification.confidence, 1.0);
    assert_eq!(modification.is_reversible, true);
    assert_eq!(modification.old_value, json!("OLD@Example.com"));
    assert_eq!(modification.new_value, json!("old@example.com"));

    println!("✅ Test 5 passed: FieldModification structure");
}

/// Test 6: WorkflowLineageGenerator creation
#[test]
fn test_workflow_lineage_generator_creation() {
    let store = GraphicaRdfStore::new_in_memory().unwrap();
    let generator = WorkflowLineageGenerator::new(Arc::new(store));

    // Generator should be created successfully
    // This verifies the basic infrastructure is in place

    println!("✅ Test 6 passed: WorkflowLineageGenerator creation");
}

/// Test 7: Multiple triples in different graphs
#[test]
fn test_multiple_graphs_isolation() {
    let store = GraphicaRdfStore::new_in_memory().unwrap();

    // Insert into workflows graph
    let wf_triples = vec![RdfTriple::new_uri(
        "wf:exec_1",
        "rdf:type",
        "wf:WorkflowExecution",
    )];
    store
        .insert_batch(&wf_triples, Some(&NamedGraph::workflows()))
        .unwrap();

    // Insert into models graph
    let model_triples = vec![RdfTriple::new_uri("ml:model_1", "rdf:type", "ml:Model")];
    store
        .insert_batch(&model_triples, Some(&NamedGraph::models()))
        .unwrap();

    // Verify counts in each graph
    let wf_count = store.count_triples(Some(&NamedGraph::workflows())).unwrap();
    let model_count = store.count_triples(Some(&NamedGraph::models())).unwrap();

    assert_eq!(wf_count, 1, "Workflows graph should have 1 triple");
    assert_eq!(model_count, 1, "Models graph should have 1 triple");

    println!("✅ Test 7 passed: Multiple graphs isolation");
}

/// Test 8: RdfValue to_string() formatting
#[test]
fn test_rdf_value_formatting() {
    // Test literal formatting (with quote escaping)
    let literal = RdfValue::Literal("Hello \"World\"".to_string());
    let formatted = literal.to_string();
    assert!(formatted.contains("\\\""), "Should escape quotes");

    // Test URI formatting
    let uri = RdfValue::Uri("http://example.com/resource".to_string());
    let formatted_uri = uri.to_string();
    assert_eq!(formatted_uri, "<http://example.com/resource>");

    // Test typed literal formatting
    let typed = RdfValue::TypedLiteral {
        value: "123".to_string(),
        datatype: "xsd:integer".to_string(),
    };
    let formatted_typed = typed.to_string();
    assert!(formatted_typed.contains("\"123\""));
    assert!(formatted_typed.contains("xsd:integer"));

    println!("✅ Test 8 passed: RdfValue formatting");
}
