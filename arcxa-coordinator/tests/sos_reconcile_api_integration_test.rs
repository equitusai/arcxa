//! HTTP/router coverage for explicit SoS reconcile/recovery controls.

#[path = "support/sos_api.rs"]
mod sos_api;

use axum::http::{Method, StatusCode};
use graphica_coordinator::api::{
    auth::Role,
    sos_validation::types::{ReconcileSosResponse, SosErrorResponse},
};
use graphica_coordinator::governance::rdf_store::{NamedGraph, RdfStore};
use sos_api::{
    assert_json_response, assert_status, empty_request, issue_test_token, seed_minimal_catalog,
    setup_authenticated_build_router_app, subject_exists,
};
use tower::ServiceExt;

const SOS_CATALOG_GRAPH: &str = "http://graphica.io/graph/sos-catalog";
const PROVIDER_SYSTEM_SUBJECT: &str = "http://graphica.io/sos/system/provider-system";

#[tokio::test]
async fn reconcile_endpoint_requires_admin_and_rebuilds_catalog_graph() {
    let harness = setup_authenticated_build_router_app();
    seed_minimal_catalog(&harness.storage_manager);
    let admin_token = harness
        .token
        .as_deref()
        .expect("authenticated harness should expose a bearer token");
    let operator_token = issue_test_token(Role::Operator, "operator_user");

    let unauthenticated_response = harness
        .app
        .clone()
        .oneshot(empty_request("/api/v1/sos/reconcile"))
        .await
        .expect("unauthenticated reconcile request should complete");
    assert_status(unauthenticated_response, StatusCode::UNAUTHORIZED).await;

    let operator_response = harness
        .app
        .clone()
        .oneshot(sos_api::authed_json_request(
            Method::POST,
            "/api/v1/sos/reconcile",
            &operator_token,
            serde_json::json!({ "include_ontology_sync": false }),
        ))
        .await
        .expect("operator reconcile request should complete");
    let operator_error: SosErrorResponse =
        assert_json_response(operator_response, StatusCode::FORBIDDEN).await;
    assert!(operator_error.message.contains("Admin access required"));

    let reconcile_response = harness
        .app
        .clone()
        .oneshot(sos_api::authed_json_request(
            Method::POST,
            "/api/v1/sos/reconcile",
            admin_token,
            serde_json::json!({ "include_ontology_sync": false }),
        ))
        .await
        .expect("admin reconcile request should complete");
    let reconcile: ReconcileSosResponse =
        assert_json_response(reconcile_response, StatusCode::OK).await;

    assert_eq!(reconcile.triggered_by, "test_user");
    assert!(!reconcile.include_ontology_sync);
    assert!(!reconcile.ontology_sync_performed);
    assert!(reconcile.graph_reconcile_performed);
    assert_eq!(reconcile.system_count, 2);
    assert_eq!(reconcile.interface_count, 2);
    assert_eq!(reconcile.contract_count, 1);
    assert_eq!(reconcile.policy_count, 0);

    assert!(subject_exists(
        &harness.rdf_store,
        SOS_CATALOG_GRAPH,
        PROVIDER_SYSTEM_SUBJECT,
    ));
}

#[tokio::test]
async fn reconcile_endpoint_restores_cleared_catalog_graph_and_uses_default_request_behavior() {
    let harness = setup_authenticated_build_router_app();
    seed_minimal_catalog(&harness.storage_manager);
    let admin_token = harness
        .token
        .as_deref()
        .expect("authenticated harness should expose a bearer token");

    let initial_reconcile_response = harness
        .app
        .clone()
        .oneshot(sos_api::authed_json_request(
            Method::POST,
            "/api/v1/sos/reconcile",
            admin_token,
            serde_json::json!({ "include_ontology_sync": false }),
        ))
        .await
        .expect("initial admin reconcile request should complete");
    let _: ReconcileSosResponse =
        assert_json_response(initial_reconcile_response, StatusCode::OK).await;

    assert!(subject_exists(
        &harness.rdf_store,
        SOS_CATALOG_GRAPH,
        PROVIDER_SYSTEM_SUBJECT,
    ));

    harness
        .rdf_store
        .clear_graph(&NamedGraph::new(SOS_CATALOG_GRAPH))
        .expect("catalog graph should be cleared to simulate drift");
    assert!(
        !subject_exists(
            &harness.rdf_store,
            SOS_CATALOG_GRAPH,
            PROVIDER_SYSTEM_SUBJECT
        ),
        "catalog subject should be absent after simulated drift"
    );

    let replay_reconcile_response = harness
        .app
        .clone()
        .oneshot(sos_api::authed_json_request(
            Method::POST,
            "/api/v1/sos/reconcile",
            admin_token,
            serde_json::json!({}),
        ))
        .await
        .expect("recovery reconcile request should complete");
    let replay_reconcile: ReconcileSosResponse =
        assert_json_response(replay_reconcile_response, StatusCode::OK).await;

    assert!(replay_reconcile.include_ontology_sync);
    assert!(!replay_reconcile.ontology_registry_available);
    assert!(!replay_reconcile.ontology_sync_performed);
    assert!(replay_reconcile.graph_reconcile_performed);
    assert!(subject_exists(
        &harness.rdf_store,
        SOS_CATALOG_GRAPH,
        PROVIDER_SYSTEM_SUBJECT,
    ));
}
