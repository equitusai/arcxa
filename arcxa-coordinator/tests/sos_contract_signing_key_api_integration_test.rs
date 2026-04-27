//! HTTP/router coverage for SoS contract signing-key lifecycle and signature audit views.

#[path = "support/sos_api.rs"]
mod sos_api;

use axum::http::{Method, StatusCode};
use graphica_coordinator::api::sos_validation::types::{
    DataContractResponse, ListContractSignaturesResponse, RotateSosContractSigningKeyResponse,
    SosContractApprovalRequestResponse, SosContractSigningKeyStatusResponse, ValidationResponse,
};
use serde_json::json;
use sos_api::{
    assert_json_response, authed_json_request, seed_minimal_catalog,
    setup_authenticated_build_router_app,
    setup_authenticated_build_router_app_with_inline_secret_store,
};
use tower::ServiceExt;

async fn create_contract(
    harness: &sos_api::SosApiHarness,
    token: &str,
    contract_id: &str,
    provider_interface_id: &str,
    consumer_interface_id: &str,
) -> DataContractResponse {
    let response = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            "/api/v1/sos/contracts",
            token,
            json!({
                "contract_id": contract_id,
                "contract_name": format!("Contract {contract_id}"),
                "provider_interface_id": provider_interface_id,
                "consumer_interface_id": consumer_interface_id,
                "sla_metrics": [{
                    "name": "latency_ms",
                    "value": 100.0,
                    "operator": "<=",
                    "unit": "ms"
                }],
                "transformation_rules": {},
                "description": "Synthetic contract",
                "tags": ["api-test"],
            }),
        ))
        .await
        .expect("contract creation request should succeed");

    assert_json_response(response, StatusCode::OK).await
}

async fn register_system(
    harness: &sos_api::SosApiHarness,
    token: &str,
    system_id: &str,
    system_name: &str,
) {
    let response = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            "/api/v1/sos/systems",
            token,
            json!({
                "system_id": system_id,
                "system_name": system_name,
                "system_type": "synthetic",
                "vendor": "graphica",
                "version": "1.0.0",
                "classification": "UNCLASSIFIED",
                "deployment": {
                    "environment": "test"
                },
                "capabilities": {
                    "integration_test": true
                },
                "tags": ["api-test"],
            }),
        ))
        .await
        .expect("system registration should succeed");
    let _: serde_json::Value = assert_json_response(response, StatusCode::OK).await;
}

async fn register_interface(
    harness: &sos_api::SosApiHarness,
    token: &str,
    system_id: &str,
    interface_id: &str,
    interface_name: &str,
    direction: &str,
) {
    let response = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            "/api/v1/sos/interfaces",
            token,
            json!({
                "interface_id": interface_id,
                "system_id": system_id,
                "interface_name": interface_name,
                "direction": direction,
                "protocol": "HTTP",
                "data_format": "JSON",
                "schema": {
                    "type": "object",
                    "properties": {
                        "sample_id": { "type": "string" },
                        "score": { "type": "number" }
                    },
                    "required": ["sample_id", "score"]
                },
                "unit_system": "SI",
                "metadata": {
                    "score": "ratio"
                },
            }),
        ))
        .await
        .expect("interface registration should succeed");
    let _: serde_json::Value = assert_json_response(response, StatusCode::OK).await;
}

async fn create_contract_approval_request(
    harness: &sos_api::SosApiHarness,
    token: &str,
    contract_id: &str,
) -> SosContractApprovalRequestResponse {
    let response = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            &format!("/api/v1/sos/contracts/{contract_id}/approval-requests"),
            token,
            json!({
                "requested_by": "governance-operator",
                "lifecycle_state": "approved",
                "note": "Ready for approval review"
            }),
        ))
        .await
        .expect("approval request creation should succeed");

    assert_json_response(response, StatusCode::OK).await
}

async fn persist_contract_validation_report(
    harness: &sos_api::SosApiHarness,
    token: &str,
    provider_interface_id: &str,
    consumer_interface_id: &str,
) -> String {
    let response = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            "/api/v1/sos/validate",
            token,
            json!({
                "type": "interface_compatibility",
                "provider_interface_id": provider_interface_id,
                "consumer_interface_id": consumer_interface_id,
            }),
        ))
        .await
        .expect("contract validation should succeed");
    let validation: ValidationResponse = assert_json_response(response, StatusCode::OK).await;
    validation
        .report_id
        .expect("persisted contract validation should return report id")
}

async fn add_contract_approval_evidence(
    harness: &sos_api::SosApiHarness,
    token: &str,
    contract_id: &str,
    request_id: &str,
    report_id: &str,
) {
    let response = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            &format!("/api/v1/sos/contracts/{contract_id}/approval-requests/{request_id}/evidence"),
            token,
            json!({
                "report_id": report_id,
                "added_by": "qa-reviewer",
                "note": "Passing contract validation"
            }),
        ))
        .await
        .expect("approval evidence request should succeed");

    let _: serde_json::Value = assert_json_response(response, StatusCode::OK).await;
}

async fn approve_contract(
    harness: &sos_api::SosApiHarness,
    token: &str,
    contract_id: &str,
) -> DataContractResponse {
    let response = harness
        .app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/sos/contracts/{contract_id}/approve"))
                .header("authorization", format!("Bearer {token}"))
                .body(axum::body::Body::empty())
                .expect("approve request should build"),
        )
        .await
        .expect("approve request should succeed");

    assert_json_response(response, StatusCode::OK).await
}

async fn sign_contract(
    harness: &sos_api::SosApiHarness,
    token: &str,
    contract_id: &str,
) -> DataContractResponse {
    let response = harness
        .app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/sos/contracts/{contract_id}/sign"))
                .header("authorization", format!("Bearer {token}"))
                .body(axum::body::Body::empty())
                .expect("sign request should build"),
        )
        .await
        .expect("sign request should succeed");

    assert_json_response(response, StatusCode::OK).await
}

#[tokio::test]
async fn build_router_contract_signing_key_rotation_preserves_historical_signatures() {
    let harness = setup_authenticated_build_router_app_with_inline_secret_store();
    let token = harness
        .token
        .as_deref()
        .expect("authenticated harness should expose a token");
    seed_minimal_catalog(&harness.storage_manager);

    create_contract(
        &harness,
        token,
        "contract-signing-key-http",
        "provider-if",
        "consumer-if",
    )
    .await;
    let request_v1 =
        create_contract_approval_request(&harness, token, "contract-signing-key-http").await;
    let report_id_v1 =
        persist_contract_validation_report(&harness, token, "provider-if", "consumer-if").await;
    add_contract_approval_evidence(
        &harness,
        token,
        "contract-signing-key-http",
        &request_v1.request_id,
        &report_id_v1,
    )
    .await;
    approve_contract(&harness, token, "contract-signing-key-http").await;
    let signed_v1 = sign_contract(&harness, token, "contract-signing-key-http").await;
    let signature_v1 = signed_v1
        .signature
        .clone()
        .expect("signed contract should expose attestation material");
    assert!(signature_v1.signature_verified);
    assert_eq!(signature_v1.signing_key_source, "secret_store");
    assert!(signature_v1.signing_key_version.is_some());
    assert_eq!(
        signature_v1.signing_key_ref.as_deref(),
        Some("sos/contracts/signing-key")
    );

    let status_response = harness
        .app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method(Method::GET)
                .uri("/api/v1/sos/contracts/signing-key")
                .header("authorization", format!("Bearer {token}"))
                .body(axum::body::Body::empty())
                .expect("status request should build"),
        )
        .await
        .expect("signing-key status request should succeed");
    let status_before: SosContractSigningKeyStatusResponse =
        assert_json_response(status_response, StatusCode::OK).await;
    assert_eq!(status_before.signing_key_source, "secret_store");
    assert!(status_before.supports_rotation);
    assert_eq!(
        status_before.signing_key_version,
        signature_v1.signing_key_version
    );
    assert_eq!(status_before.key_fingerprint, signature_v1.key_fingerprint);

    let rotate_response = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            "/api/v1/sos/contracts/signing-key/rotate",
            token,
            json!({
                "reason": "routine rotation"
            }),
        ))
        .await
        .expect("signing-key rotation request should succeed");
    let rotated: RotateSosContractSigningKeyResponse =
        assert_json_response(rotate_response, StatusCode::OK).await;
    assert_eq!(rotated.signing_key_ref, "sos/contracts/signing-key");
    assert_eq!(
        rotated.previous_signing_key_version,
        signature_v1.signing_key_version
    );
    assert_eq!(
        rotated.previous_key_fingerprint,
        Some(signature_v1.key_fingerprint.clone())
    );
    assert_ne!(
        rotated.current_key_fingerprint,
        signature_v1.key_fingerprint
    );

    let status_after_response = harness
        .app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method(Method::GET)
                .uri("/api/v1/sos/contracts/signing-key")
                .header("authorization", format!("Bearer {token}"))
                .body(axum::body::Body::empty())
                .expect("status request should build"),
        )
        .await
        .expect("signing-key status request should succeed");
    let status_after: SosContractSigningKeyStatusResponse =
        assert_json_response(status_after_response, StatusCode::OK).await;
    assert_eq!(
        status_after.signing_key_version.as_deref(),
        Some(rotated.current_signing_key_version.as_str())
    );
    assert_eq!(
        status_after.key_fingerprint,
        rotated.current_key_fingerprint
    );

    register_system(&harness, token, "provider-rotated", "Provider Rotated").await;
    register_system(&harness, token, "consumer-rotated", "Consumer Rotated").await;
    register_interface(
        &harness,
        token,
        "provider-rotated",
        "provider-rotated-if",
        "Provider Rotated Interface",
        "Provider",
    )
    .await;
    register_interface(
        &harness,
        token,
        "consumer-rotated",
        "consumer-rotated-if",
        "Consumer Rotated Interface",
        "Consumer",
    )
    .await;
    create_contract(
        &harness,
        token,
        "contract-signing-key-http-rotated",
        "provider-rotated-if",
        "consumer-rotated-if",
    )
    .await;
    let request_v2 =
        create_contract_approval_request(&harness, token, "contract-signing-key-http-rotated")
            .await;
    let report_id_v2 = persist_contract_validation_report(
        &harness,
        token,
        "provider-rotated-if",
        "consumer-rotated-if",
    )
    .await;
    add_contract_approval_evidence(
        &harness,
        token,
        "contract-signing-key-http-rotated",
        &request_v2.request_id,
        &report_id_v2,
    )
    .await;
    approve_contract(&harness, token, "contract-signing-key-http-rotated").await;
    let signed_v2 = sign_contract(&harness, token, "contract-signing-key-http-rotated").await;
    let signature_v2 = signed_v2
        .signature
        .clone()
        .expect("signed revision should expose attestation material");
    assert!(signature_v2.signature_verified);
    assert_ne!(signature_v2.key_fingerprint, signature_v1.key_fingerprint);
    assert_ne!(
        signature_v2.signing_key_version,
        signature_v1.signing_key_version
    );
    assert_eq!(
        signature_v2.signing_key_version.as_deref(),
        Some(rotated.current_signing_key_version.as_str())
    );

    let history_response = harness
        .app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method(Method::GET)
                .uri("/api/v1/sos/contracts/contract-signing-key-http/signatures?limit=10")
                .header("authorization", format!("Bearer {token}"))
                .body(axum::body::Body::empty())
                .expect("signature history request should build"),
        )
        .await
        .expect("signature history request should succeed");
    let history: ListContractSignaturesResponse =
        assert_json_response(history_response, StatusCode::OK).await;
    assert_eq!(history.total, 1);
    assert_eq!(history.signatures.len(), 1);
    assert!(history
        .signatures
        .iter()
        .all(|signature| signature.signature_verified));
    assert!(history
        .signatures
        .iter()
        .any(|signature| signature.contract_revision == 1
            && signature.signing_key_version == signature_v1.signing_key_version));
}

#[tokio::test]
async fn build_router_contract_signing_key_status_requires_configuration() {
    let harness = setup_authenticated_build_router_app();
    let token = harness
        .token
        .as_deref()
        .expect("authenticated harness should expose a token");

    let status_response = harness
        .app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method(Method::GET)
                .uri("/api/v1/sos/contracts/signing-key")
                .header("authorization", format!("Bearer {token}"))
                .body(axum::body::Body::empty())
                .expect("status request should build"),
        )
        .await
        .expect("status request should complete");
    let status = status_response.status();
    assert_eq!(status, StatusCode::NOT_FOUND);

    let rotate_response = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            "/api/v1/sos/contracts/signing-key/rotate",
            token,
            json!({ "reason": "rotation should fail without a store" }),
        ))
        .await
        .expect("rotation request should complete");
    assert_eq!(rotate_response.status(), StatusCode::CONFLICT);
}
