//! HTTP-level coverage for SoS analytics endpoints.

#[path = "support/sos_api.rs"]
mod sos_api;

use axum::http::{Method, StatusCode};
use graphica_coordinator::api::sos_validation::types::{
    CompatibilityMatrixResponse, DependencyGraphResponse, SosErrorResponse, WhatIfResponse,
};
use sos_api::{
    assert_json_response, assert_status, authed_empty_request, empty_request, json_request,
    seed_minimal_catalog, setup_authenticated_build_router_app, setup_direct_app,
    what_if_unit_change_payload, CONSUMER_INTERFACE_ID, PROVIDER_INTERFACE_ID,
};
use tower::ServiceExt;

#[tokio::test]
async fn analytics_endpoints_return_matrix_dependency_graph_and_what_if() {
    let harness = setup_direct_app();
    seed_minimal_catalog(&harness.storage_manager);

    let matrix_response = harness
        .app
        .clone()
        .oneshot(empty_request("/sos/compatibility-matrix"))
        .await
        .expect("compatibility matrix request should succeed");
    let matrix: CompatibilityMatrixResponse =
        assert_json_response(matrix_response, StatusCode::OK).await;

    assert_eq!(matrix.metadata.total_interfaces, 2);
    assert_eq!(matrix.metadata.total_candidate_pairs, 2);
    assert_eq!(matrix.metadata.evaluated_pairs, 2);
    assert_eq!(matrix.metadata.remaining_candidate_pairs, 0);
    assert!(!matrix.metadata.truncated);

    assert!(
        matrix.matrix.iter().any(|score| {
            score.provider_interface_id == PROVIDER_INTERFACE_ID
                && score.consumer_interface_id == CONSUMER_INTERFACE_ID
        }),
        "compatibility matrix should include the seeded provider/consumer interface path"
    );

    let graph_response = harness
        .app
        .clone()
        .oneshot(empty_request("/sos/dependency-graph"))
        .await
        .expect("dependency graph request should succeed");
    let graph: DependencyGraphResponse = assert_json_response(graph_response, StatusCode::OK).await;

    assert!(
        graph
            .nodes
            .iter()
            .any(|node| node.id == PROVIDER_INTERFACE_ID),
        "dependency graph should include interface nodes"
    );
    assert!(
        graph
            .edges
            .iter()
            .any(|edge| edge.kind == "governs_consumer"),
        "dependency graph should include contract governance edges"
    );
    assert_eq!(graph.metadata.total_nodes, 5);
    assert_eq!(graph.metadata.total_edges, 5);
    assert!(!graph.metadata.truncated);

    let what_if_response = harness
        .app
        .oneshot(json_request(
            Method::POST,
            "/sos/what-if",
            what_if_unit_change_payload(),
        ))
        .await
        .expect("what-if request should succeed");
    let what_if: WhatIfResponse = assert_json_response(what_if_response, StatusCode::OK).await;

    assert!(!what_if.scenario_id.is_empty());
    assert!(
        what_if
            .affected_entities
            .iter()
            .any(|entity| entity == CONSUMER_INTERFACE_ID),
        "what-if response should identify the changed interface"
    );
    assert!(
        what_if
            .recommendations
            .iter()
            .any(|recommendation| recommendation.contains("unit")),
        "unit-system what-if should produce unit-alignment guidance"
    );
    assert_eq!(what_if.metadata.total_candidate_evaluations, 4);
    assert_eq!(what_if.metadata.evaluated_candidate_evaluations, 4);
    assert!(!what_if.metadata.truncated);
}

#[tokio::test]
async fn analytics_router_auth_and_budget_paths_are_enforced() {
    let harness = setup_authenticated_build_router_app();
    seed_minimal_catalog(&harness.storage_manager);
    let token = harness
        .token
        .as_deref()
        .expect("authenticated harness should expose a bearer token");

    let unauthenticated_response = harness
        .app
        .clone()
        .oneshot(empty_request("/api/v1/sos/compatibility-matrix"))
        .await
        .expect("unauthenticated compatibility matrix request should complete");
    assert_status(unauthenticated_response, StatusCode::UNAUTHORIZED).await;

    let invalid_budget_response = harness
        .app
        .clone()
        .oneshot(authed_empty_request(
            Method::GET,
            "/api/v1/sos/compatibility-matrix?evaluation_budget=0",
            token,
        ))
        .await
        .expect("invalid budget compatibility matrix request should complete");
    let invalid_budget: SosErrorResponse =
        assert_json_response(invalid_budget_response, StatusCode::BAD_REQUEST).await;
    assert!(
        invalid_budget.message.contains("evaluation_budget"),
        "bad-request payload should explain the invalid matrix budget"
    );

    let truncated_response = harness
        .app
        .clone()
        .oneshot(authed_empty_request(
            Method::GET,
            "/api/v1/sos/compatibility-matrix?evaluation_budget=1",
            token,
        ))
        .await
        .expect("budgeted compatibility matrix request should complete");
    let truncated: CompatibilityMatrixResponse =
        assert_json_response(truncated_response, StatusCode::OK).await;

    assert_eq!(truncated.metadata.total_candidate_pairs, 2);
    assert_eq!(truncated.metadata.evaluated_pairs, 1);
    assert_eq!(truncated.metadata.remaining_candidate_pairs, 1);
    assert!(truncated.metadata.truncated);
    assert_eq!(truncated.metadata.requested_evaluation_budget, Some(1));
    assert_eq!(truncated.metadata.applied_evaluation_budget, 1);
    assert_eq!(truncated.matrix.len(), 1);

    let unauthenticated_graph_response = harness
        .app
        .clone()
        .oneshot(empty_request("/api/v1/sos/dependency-graph"))
        .await
        .expect("unauthenticated dependency graph request should complete");
    assert_status(unauthenticated_graph_response, StatusCode::UNAUTHORIZED).await;

    let invalid_graph_budget_response = harness
        .app
        .clone()
        .oneshot(authed_empty_request(
            Method::GET,
            "/api/v1/sos/dependency-graph?node_budget=0",
            token,
        ))
        .await
        .expect("invalid dependency graph budget request should complete");
    let invalid_graph_budget: SosErrorResponse =
        assert_json_response(invalid_graph_budget_response, StatusCode::BAD_REQUEST).await;
    assert!(
        invalid_graph_budget.message.contains("node_budget"),
        "bad-request payload should explain the invalid dependency graph budget"
    );

    let truncated_graph_response = harness
        .app
        .clone()
        .oneshot(authed_empty_request(
            Method::GET,
            "/api/v1/sos/dependency-graph?node_budget=1&edge_budget=1",
            token,
        ))
        .await
        .expect("budgeted dependency graph request should complete");
    let truncated_graph: DependencyGraphResponse =
        assert_json_response(truncated_graph_response, StatusCode::OK).await;
    assert_eq!(truncated_graph.metadata.total_nodes, 5);
    assert_eq!(truncated_graph.metadata.total_edges, 5);
    assert_eq!(truncated_graph.metadata.returned_nodes, 1);
    assert_eq!(truncated_graph.metadata.returned_edges, 1);
    assert!(truncated_graph.metadata.truncated);

    let unauthenticated_what_if_response = harness
        .app
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/api/v1/sos/what-if",
            what_if_unit_change_payload(),
        ))
        .await
        .expect("unauthenticated what-if request should complete");
    assert_status(unauthenticated_what_if_response, StatusCode::UNAUTHORIZED).await;

    let invalid_what_if_budget_response = harness
        .app
        .clone()
        .oneshot(sos_api::authed_json_request(
            Method::POST,
            "/api/v1/sos/what-if",
            token,
            serde_json::json!({
                "scenario": "invalid budget",
                "changes": [],
                "evaluation_budget": 0
            }),
        ))
        .await
        .expect("invalid what-if budget request should complete");
    let invalid_what_if_budget: SosErrorResponse =
        assert_json_response(invalid_what_if_budget_response, StatusCode::BAD_REQUEST).await;
    assert!(
        invalid_what_if_budget.message.contains("evaluation_budget"),
        "bad-request payload should explain the invalid what-if budget"
    );

    let truncated_what_if_response = harness
        .app
        .oneshot(sos_api::authed_json_request(
            Method::POST,
            "/api/v1/sos/what-if",
            token,
            serde_json::json!({
                "scenario": "budgeted what-if",
                "changes": [{
                    "entity_type": "interface",
                    "interface_id": CONSUMER_INTERFACE_ID,
                    "system_id": "consumer-system",
                    "unit_system": "SI"
                }],
                "evaluation_budget": 1
            }),
        ))
        .await
        .expect("budgeted what-if request should complete");
    let truncated_what_if: WhatIfResponse =
        assert_json_response(truncated_what_if_response, StatusCode::OK).await;
    assert_eq!(truncated_what_if.metadata.total_candidate_evaluations, 4);
    assert_eq!(
        truncated_what_if.metadata.evaluated_candidate_evaluations,
        1
    );
    assert_eq!(
        truncated_what_if.metadata.remaining_candidate_evaluations,
        3
    );
    assert!(truncated_what_if.metadata.truncated);
}
