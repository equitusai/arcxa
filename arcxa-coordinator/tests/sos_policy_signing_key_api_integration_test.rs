//! HTTP/router coverage for SoS policy signing-key lifecycle and attestation audit views.

#[path = "support/sos_api.rs"]
mod sos_api;

use axum::http::{Method, StatusCode};
use graphica_coordinator::api::sos_validation::types::{
    ListPolicyAttestationsResponse, RotateSosPolicySigningKeyResponse,
    SosPolicyApprovalEvidenceResponse, SosPolicyApprovalRequestResponse, SosPolicyResponse,
    SosPolicySigningKeyStatusResponse, ValidationResponse,
};
use serde_json::{json, Value};
use sos_api::{
    assert_json_response, authed_empty_request, authed_json_request,
    setup_authenticated_build_router_app,
    setup_authenticated_build_router_app_with_inline_secret_store, CONSUMER_INTERFACE_ID,
    CONSUMER_SYSTEM_ID, PROVIDER_INTERFACE_ID, PROVIDER_SYSTEM_ID,
};
use tower::ServiceExt;

fn interface_pair_policy_body(policy_id: &str) -> Value {
    json!({
        "policy_id": policy_id,
        "policy_name": format!("Policy {policy_id}"),
        "target_type": "interface_pair",
        "stages": ["pre_execution"],
        "enforcement_level": "mandatory",
        "severity": "high",
        "sparql_query": "ASK { GRAPH <http://graphica.io/graph/sos-catalog> { <{{provider_interface_uri}}> <http://graphica.io/sos#belongsToSystem> ?system } }",
        "context": {},
        "description": "Synthetic interface pair policy",
        "created_by": "architect-1",
        "tags": ["api-test", "pair"],
        "ontology_refs": ["sos-core"],
        "shape_refs": ["shape:test"],
        "lifecycle_state": "draft",
        "active": false,
        "provider_interface_id": PROVIDER_INTERFACE_ID,
        "consumer_interface_id": CONSUMER_INTERFACE_ID,
    })
}

async fn create_policy(
    harness: &sos_api::SosApiHarness,
    token: &str,
    policy_id: &str,
) -> SosPolicyResponse {
    let response = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            "/api/v1/sos/policies",
            token,
            interface_pair_policy_body(policy_id),
        ))
        .await
        .expect("policy create request should succeed");

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
                "system_type": "synthetic.system",
                "vendor": "Graphica Test",
                "version": "1.0.0",
                "classification": "UNCLASSIFIED",
                "description": format!("Synthetic system {system_id}"),
                "deployment": {},
                "capabilities": {},
                "tags": ["policy-signing-test"],
            }),
        ))
        .await
        .expect("system registration request should succeed");

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
                "system_id": system_id,
                "interface_id": interface_id,
                "interface_name": interface_name,
                "direction": direction,
                "protocol": "REST",
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
                    "sample_id": "identifier",
                    "score": "ratio"
                }
            }),
        ))
        .await
        .expect("interface registration request should succeed");

    let _: serde_json::Value = assert_json_response(response, StatusCode::OK).await;
}

async fn update_policy_for_new_revision(
    harness: &sos_api::SosApiHarness,
    token: &str,
    policy_id: &str,
    revision_suffix: &str,
) -> SosPolicyResponse {
    let response = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::PUT,
            &format!("/api/v1/sos/policies/{policy_id}"),
            token,
            json!({
                "policy_name": format!("Policy {policy_id} {revision_suffix}"),
                "severity": "critical",
                "lifecycle_state": "draft",
                "description": format!("Updated {revision_suffix}"),
                "updated_by": "operator-2",
                "tags": ["api-test", "pair", revision_suffix],
            }),
        ))
        .await
        .expect("policy update request should succeed");

    assert_json_response(response, StatusCode::OK).await
}

async fn create_policy_approval_request(
    harness: &sos_api::SosApiHarness,
    token: &str,
    policy_id: &str,
) -> SosPolicyApprovalRequestResponse {
    let response = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            &format!("/api/v1/sos/policies/{policy_id}/approval-requests"),
            token,
            json!({
                "requested_by": "governance-operator",
                "lifecycle_state": "active",
                "note": "Ready for approval review"
            }),
        ))
        .await
        .expect("policy approval-request create should succeed");

    assert_json_response(response, StatusCode::OK).await
}

async fn persist_policy_validation_report(
    harness: &sos_api::SosApiHarness,
    token: &str,
    policy_id: &str,
) -> String {
    let response = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            &format!("/api/v1/sos/policies/{policy_id}/validate"),
            token,
            json!({"stage": "pre_execution"}),
        ))
        .await
        .expect("policy validation request should succeed");
    let validation: ValidationResponse = assert_json_response(response, StatusCode::OK).await;
    validation
        .report_id
        .expect("persisted policy validation should return a report id")
}

async fn add_policy_approval_evidence(
    harness: &sos_api::SosApiHarness,
    token: &str,
    policy_id: &str,
    request_id: &str,
    report_id: &str,
) -> SosPolicyApprovalEvidenceResponse {
    let response = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            &format!("/api/v1/sos/policies/{policy_id}/approval-requests/{request_id}/evidence"),
            token,
            json!({
                "report_id": report_id,
                "added_by": "qa-reviewer",
                "note": "Passing policy validation"
            }),
        ))
        .await
        .expect("policy approval evidence request should succeed");

    assert_json_response(response, StatusCode::OK).await
}

async fn approve_policy(
    harness: &sos_api::SosApiHarness,
    token: &str,
    policy_id: &str,
    request_id: &str,
) -> SosPolicyApprovalRequestResponse {
    let response = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            &format!("/api/v1/sos/policies/{policy_id}/approval-requests/{request_id}/approve"),
            token,
            json!({"approved_by": "reviewer-1"}),
        ))
        .await
        .expect("policy approval request approval should succeed");

    assert_json_response(response, StatusCode::OK).await
}

async fn get_policy(
    harness: &sos_api::SosApiHarness,
    token: &str,
    policy_id: &str,
) -> SosPolicyResponse {
    let response = harness
        .app
        .clone()
        .oneshot(authed_empty_request(
            Method::GET,
            &format!("/api/v1/sos/policies/{policy_id}"),
            token,
        ))
        .await
        .expect("policy get request should succeed");

    assert_json_response(response, StatusCode::OK).await
}

#[tokio::test]
async fn build_router_policy_signing_key_rotation_preserves_attestation_history_and_records_external_trust(
) {
    let harness = setup_authenticated_build_router_app_with_inline_secret_store();
    let token = harness
        .token
        .as_deref()
        .expect("authenticated harness should expose a token");
    register_system(&harness, token, PROVIDER_SYSTEM_ID, "Provider System").await;
    register_system(&harness, token, CONSUMER_SYSTEM_ID, "Consumer System").await;
    register_interface(
        &harness,
        token,
        PROVIDER_SYSTEM_ID,
        PROVIDER_INTERFACE_ID,
        "Provider Interface",
        "Provider",
    )
    .await;
    register_interface(
        &harness,
        token,
        CONSUMER_SYSTEM_ID,
        CONSUMER_INTERFACE_ID,
        "Consumer Interface",
        "Consumer",
    )
    .await;

    let created = create_policy(&harness, token, "policy-signing-key-http").await;
    assert_eq!(created.revision, 1);

    let request_v1 =
        create_policy_approval_request(&harness, token, "policy-signing-key-http").await;
    let report_v1 =
        persist_policy_validation_report(&harness, token, "policy-signing-key-http").await;
    add_policy_approval_evidence(
        &harness,
        token,
        "policy-signing-key-http",
        &request_v1.request_id,
        &report_v1,
    )
    .await;
    approve_policy(
        &harness,
        token,
        "policy-signing-key-http",
        &request_v1.request_id,
    )
    .await;

    let approved_v1 = get_policy(&harness, token, "policy-signing-key-http").await;
    let attestation_v1 = approved_v1
        .attestation
        .expect("approved policy should expose attestation material");
    assert_eq!(attestation_v1.policy_revision, 1);
    assert_eq!(attestation_v1.trust_mode, "software");
    assert!(attestation_v1.attestation_verified);
    assert_eq!(attestation_v1.signing_key_source, "secret_store");
    assert_eq!(
        attestation_v1.signing_key_ref.as_deref(),
        Some("sos/policies/signing-key")
    );

    let status_response = harness
        .app
        .clone()
        .oneshot(authed_empty_request(
            Method::GET,
            "/api/v1/sos/policies/signing-key",
            token,
        ))
        .await
        .expect("policy signing-key status request should succeed");
    let status_v1: SosPolicySigningKeyStatusResponse =
        assert_json_response(status_response, StatusCode::OK).await;
    assert_eq!(status_v1.trust_mode, "software");
    assert!(status_v1.supports_rotation);

    let rotate_response = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            "/api/v1/sos/policies/signing-key/rotate",
            token,
            json!({
                "reason": "Rotate after rollout",
                "trust_mode": "kms",
                "trust_provider": "aws-kms",
                "external_key_ref": "arn:aws:kms:us-east-1:123456789012:key/policy-approval",
                "trust_attestation_ref": "kms://policy-approval/attestation/v1"
            }),
        ))
        .await
        .expect("policy signing-key rotation request should succeed");
    let rotated: RotateSosPolicySigningKeyResponse =
        assert_json_response(rotate_response, StatusCode::OK).await;
    assert_eq!(rotated.signing_key_ref, "sos/policies/signing-key");
    assert_eq!(rotated.trust_mode, "external_reference");
    assert_eq!(rotated.trust_provider.as_deref(), Some("aws-kms"));
    assert_eq!(
        rotated.external_key_ref.as_deref(),
        Some("arn:aws:kms:us-east-1:123456789012:key/policy-approval")
    );
    assert_ne!(
        rotated.previous_key_fingerprint,
        Some(rotated.current_key_fingerprint.clone())
    );

    let status_response = harness
        .app
        .clone()
        .oneshot(authed_empty_request(
            Method::GET,
            "/api/v1/sos/policies/signing-key",
            token,
        ))
        .await
        .expect("policy signing-key status request should succeed");
    let status_v2: SosPolicySigningKeyStatusResponse =
        assert_json_response(status_response, StatusCode::OK).await;
    assert_eq!(status_v2.trust_mode, "external_reference");
    assert_eq!(status_v2.trust_provider.as_deref(), Some("aws-kms"));
    assert_eq!(
        status_v2.external_key_ref.as_deref(),
        Some("arn:aws:kms:us-east-1:123456789012:key/policy-approval")
    );
    assert_eq!(
        status_v2.trust_attestation_ref.as_deref(),
        Some("kms://policy-approval/attestation/v1")
    );
    assert_eq!(
        status_v2.signing_key_version.as_deref(),
        Some(rotated.current_signing_key_version.as_str())
    );

    let updated =
        update_policy_for_new_revision(&harness, token, "policy-signing-key-http", "v2").await;
    assert_eq!(updated.revision, 2);

    let request_v2 =
        create_policy_approval_request(&harness, token, "policy-signing-key-http").await;
    let report_v2 =
        persist_policy_validation_report(&harness, token, "policy-signing-key-http").await;
    let evidence_v2 = add_policy_approval_evidence(
        &harness,
        token,
        "policy-signing-key-http",
        &request_v2.request_id,
        &report_v2,
    )
    .await;
    approve_policy(
        &harness,
        token,
        "policy-signing-key-http",
        &request_v2.request_id,
    )
    .await;

    let approved_v2 = get_policy(&harness, token, "policy-signing-key-http").await;
    let attestation_v2 = approved_v2
        .attestation
        .expect("newly approved policy revision should expose attestation material");
    assert_eq!(attestation_v2.policy_revision, 2);
    assert_eq!(attestation_v2.trust_mode, "external_reference");
    assert_eq!(attestation_v2.trust_provider.as_deref(), Some("aws-kms"));
    assert_eq!(
        attestation_v2.external_key_ref.as_deref(),
        Some("arn:aws:kms:us-east-1:123456789012:key/policy-approval")
    );
    assert_eq!(
        attestation_v2.trust_attestation_ref.as_deref(),
        Some("kms://policy-approval/attestation/v1")
    );
    assert_eq!(
        attestation_v2.signing_key_version.as_deref(),
        Some(rotated.current_signing_key_version.as_str())
    );
    assert_eq!(
        attestation_v2.approval_request_id.as_deref(),
        Some(request_v2.request_id.as_str())
    );
    assert_eq!(
        attestation_v2.evidence_ids,
        vec![evidence_v2.evidence_id.clone()]
    );
    assert!(attestation_v2.attestation_verified);

    let history_response = harness
        .app
        .clone()
        .oneshot(authed_empty_request(
            Method::GET,
            "/api/v1/sos/policies/policy-signing-key-http/attestations?limit=10",
            token,
        ))
        .await
        .expect("policy attestation history request should succeed");
    let history: ListPolicyAttestationsResponse =
        assert_json_response(history_response, StatusCode::OK).await;

    assert_eq!(history.total, 2);
    assert_eq!(history.attestations.len(), 2);
    assert_eq!(history.attestations[0].policy_revision, 2);
    assert_eq!(history.attestations[0].trust_mode, "external_reference");
    assert_eq!(history.attestations[1].policy_revision, 1);
    assert_eq!(history.attestations[1].trust_mode, "software");
    assert!(history
        .attestations
        .iter()
        .all(|attestation| attestation.attestation_verified));
}

#[tokio::test]
async fn build_router_policy_signing_key_rotation_requires_writable_secret_store() {
    let harness = setup_authenticated_build_router_app();
    let token = harness
        .token
        .as_deref()
        .expect("authenticated harness should expose a token");

    let status_response = harness
        .app
        .clone()
        .oneshot(authed_empty_request(
            Method::GET,
            "/api/v1/sos/policies/signing-key",
            token,
        ))
        .await
        .expect("policy signing-key status request should succeed");
    assert_eq!(status_response.status(), StatusCode::NOT_FOUND);

    let rotate_response = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            "/api/v1/sos/policies/signing-key/rotate",
            token,
            json!({"reason": "No writable secret store"}),
        ))
        .await
        .expect("policy signing-key rotation request should succeed");
    assert_eq!(rotate_response.status(), StatusCode::CONFLICT);
}
