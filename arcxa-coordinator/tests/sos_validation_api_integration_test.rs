//! HTTP-level coverage for SoS validation endpoints.

#[path = "support/sos_api.rs"]
mod sos_api;

use axum::http::{Method, StatusCode};
use graphica_coordinator::api::sos_validation::types::{
    ValidationHistoryResponse, ValidationReportResponse, ValidationResponse,
};
use sos_api::{
    assert_json_response, assert_status, authed_json_request, empty_request,
    interface_pair_validation_payload, json_request, seed_minimal_catalog,
    setup_authenticated_build_router_app, setup_direct_app, valid_data_payload,
    INTERFACE_PAIR_SUBJECT_KEY, PROVIDER_INTERFACE_SUBJECT_KEY,
};
use tower::ServiceExt;

#[tokio::test]
async fn validate_persisted_and_dry_run_semantics_over_http() {
    let harness = setup_direct_app();
    seed_minimal_catalog(&harness.storage_manager);

    let dry_run_response = harness
        .app
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/sos/validate/dry-run",
            interface_pair_validation_payload(),
        ))
        .await
        .expect("dry-run request should succeed");
    let dry_run: ValidationResponse = assert_json_response(dry_run_response, StatusCode::OK).await;

    assert!(
        dry_run.passed,
        "dry-run should validate compatible fixtures"
    );
    assert!(
        dry_run.report_id.is_none(),
        "dry-run validation must not persist a report"
    );

    let empty_history_response = harness
        .app
        .clone()
        .oneshot(empty_request(&format!(
            "/sos/validation-history?subject_key={INTERFACE_PAIR_SUBJECT_KEY}&limit=10"
        )))
        .await
        .expect("history request should succeed");
    assert_status(empty_history_response, StatusCode::NOT_FOUND).await;

    let persisted_response = harness
        .app
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/sos/validate",
            interface_pair_validation_payload(),
        ))
        .await
        .expect("persisted request should succeed");
    let persisted: ValidationResponse =
        assert_json_response(persisted_response, StatusCode::OK).await;
    let report_id = persisted
        .report_id
        .clone()
        .expect("persisted validation should return a report id");

    assert!(persisted.passed, "persisted validation should pass");

    let report_response = harness
        .app
        .oneshot(empty_request(&format!(
            "/sos/validation-reports/{report_id}"
        )))
        .await
        .expect("report lookup should succeed");
    let report: ValidationReportResponse =
        assert_json_response(report_response, StatusCode::OK).await;

    assert_eq!(report.report_id, report_id);
    assert_eq!(report.subject_type, "interface_pair");
    assert_eq!(report.subject_key, INTERFACE_PAIR_SUBJECT_KEY);
    assert_eq!(report.validation_type, "interface_compatibility");
}

#[tokio::test]
async fn validate_schema_endpoint_persists_data_validation_report() {
    let harness = setup_direct_app();
    seed_minimal_catalog(&harness.storage_manager);

    let schema_response = harness
        .app
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/sos/interfaces/provider-if/validate-schema",
            valid_data_payload(),
        ))
        .await
        .expect("schema validation request should succeed");
    let validation: ValidationResponse =
        assert_json_response(schema_response, StatusCode::OK).await;

    assert!(validation.passed, "valid payload should satisfy schema");
    assert!(
        validation.report_id.is_some(),
        "schema validation endpoint should persist a data-validation report"
    );

    let history_response = harness
        .app
        .oneshot(empty_request(&format!(
            "/sos/validation-history?subject_key={PROVIDER_INTERFACE_SUBJECT_KEY}&subject_type=interface&limit=10"
        )))
        .await
        .expect("schema history request should succeed");
    let history: ValidationHistoryResponse =
        assert_json_response(history_response, StatusCode::OK).await;

    assert_eq!(history.subject_type, "interface");
    assert_eq!(history.subject_key, PROVIDER_INTERFACE_SUBJECT_KEY);
    assert_eq!(history.reports.len(), 1);
    assert_eq!(history.reports[0].validation_type, "data_validation");
}

#[tokio::test]
async fn validate_via_build_router_requires_and_accepts_auth() {
    let harness = setup_authenticated_build_router_app();
    seed_minimal_catalog(&harness.storage_manager);

    let unauthenticated_response = harness
        .app
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/api/v1/sos/validate",
            interface_pair_validation_payload(),
        ))
        .await
        .expect("unauthenticated request should complete");
    assert_status(unauthenticated_response, StatusCode::UNAUTHORIZED).await;

    let token = harness
        .token
        .as_deref()
        .expect("authenticated harness should expose a token");
    let authenticated_response = harness
        .app
        .oneshot(authed_json_request(
            Method::POST,
            "/api/v1/sos/validate",
            token,
            interface_pair_validation_payload(),
        ))
        .await
        .expect("authenticated request should succeed");
    let validation: ValidationResponse =
        assert_json_response(authenticated_response, StatusCode::OK).await;

    assert!(validation.passed, "authenticated validation should pass");
    assert!(
        validation.report_id.is_some(),
        "production-router validation should persist reports"
    );
}
