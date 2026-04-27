//! HTTP-level coverage for persisted SoS validation history and lineage.

#[path = "support/sos_api.rs"]
mod sos_api;

use axum::http::{Method, StatusCode};
use graphica_coordinator::api::sos_validation::types::{
    ValidationHistoryResponse, ValidationLineageResponse, ValidationResponse,
};
use sos_api::{
    assert_json_response, assert_status, empty_request, interface_pair_validation_payload,
    json_request, seed_minimal_catalog, setup_direct_app, INTERFACE_PAIR_SUBJECT_KEY,
};
use tokio::time::{sleep, Duration};
use tower::ServiceExt;

#[tokio::test]
async fn validation_history_and_lineage_are_queryable_over_http() {
    let harness = setup_direct_app();
    seed_minimal_catalog(&harness.storage_manager);

    let first_response = harness
        .app
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/sos/validate",
            interface_pair_validation_payload(),
        ))
        .await
        .expect("first validation request should succeed");
    let first: ValidationResponse = assert_json_response(first_response, StatusCode::OK).await;
    let first_report_id = first
        .report_id
        .clone()
        .expect("first validation should persist a report");

    sleep(Duration::from_millis(5)).await;

    let second_response = harness
        .app
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/sos/validate",
            interface_pair_validation_payload(),
        ))
        .await
        .expect("second validation request should succeed");
    let second: ValidationResponse = assert_json_response(second_response, StatusCode::OK).await;
    let second_report_id = second
        .report_id
        .clone()
        .expect("second validation should persist a report");

    let history_response = harness
        .app
        .clone()
        .oneshot(empty_request(&format!(
            "/sos/validation-history?subject_key={INTERFACE_PAIR_SUBJECT_KEY}&subject_type=interface_pair&limit=10"
        )))
        .await
        .expect("history request should succeed");
    let history: ValidationHistoryResponse =
        assert_json_response(history_response, StatusCode::OK).await;

    assert_eq!(history.subject_type, "interface_pair");
    assert_eq!(history.subject_key, INTERFACE_PAIR_SUBJECT_KEY);
    assert_eq!(history.reports.len(), 2);
    assert_eq!(history.reports[0].report_id, second_report_id);
    assert_eq!(history.reports[1].report_id, first_report_id);
    assert_eq!(
        history.reports[0].previous_report_id.as_deref(),
        Some(first_report_id.as_str())
    );

    let lineage_response = harness
        .app
        .clone()
        .oneshot(empty_request(&format!(
            "/sos/validation-lineage?subject_key={INTERFACE_PAIR_SUBJECT_KEY}&subject_type=interface_pair&limit=10"
        )))
        .await
        .expect("lineage request should succeed");
    let lineage: ValidationLineageResponse =
        assert_json_response(lineage_response, StatusCode::OK).await;

    assert_eq!(lineage.reports.len(), 2);
    assert!(lineage.edges.iter().any(|edge| {
        edge.from_report_id == first_report_id && edge.to_report_id == second_report_id
    }));

    let mismatched_subject_type_response = harness
        .app
        .oneshot(empty_request(&format!(
            "/sos/validation-history?subject_key={INTERFACE_PAIR_SUBJECT_KEY}&subject_type=contract&limit=10"
        )))
        .await
        .expect("mismatched subject-type request should complete");
    assert_status(mismatched_subject_type_response, StatusCode::BAD_REQUEST).await;
}
