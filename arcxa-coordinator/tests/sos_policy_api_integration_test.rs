//! HTTP/router coverage for persisted SoS policy CRUD and evaluation paths.

#[path = "support/sos_api.rs"]
mod sos_api;

use axum::http::{Method, StatusCode};
use graphica_coordinator::api::sos_validation::types::{
    ListPoliciesResponse, ListPolicyApprovalRequestsResponse, SosPolicyApprovalEvidenceResponse,
    SosPolicyApprovalRequestResponse, SosPolicyResponse, ValidationHistoryResponse,
    ValidationReportResponse, ValidationResponse,
};
use serde_json::{json, Value};
use sos_api::{
    assert_json_response, assert_status, authed_empty_request, authed_json_request, json_request,
    setup_authenticated_build_router_app_with_inline_secret_store, CONSUMER_INTERFACE_ID,
    PROVIDER_INTERFACE_ID,
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
        "active": true,
        "provider_interface_id": PROVIDER_INTERFACE_ID,
        "consumer_interface_id": CONSUMER_INTERFACE_ID,
    })
}

fn interface_policy_body(policy_id: &str) -> Value {
    json!({
        "policy_id": policy_id,
        "policy_name": format!("Policy {policy_id}"),
        "target_type": "interface",
        "stages": ["in_flight"],
        "enforcement_level": "advisory",
        "severity": "medium",
        "sparql_query": "ASK { GRAPH <http://graphica.io/graph/sos-catalog> { <{{interface_uri}}> <http://graphica.io/sos#belongsToSystem> ?system } }",
        "context": {},
        "description": "Synthetic interface policy",
        "created_by": "architect-2",
        "tags": ["api-test", "interface"],
        "active": false,
        "interface_id": PROVIDER_INTERFACE_ID,
    })
}

async fn create_policy(
    harness: &sos_api::SosApiHarness,
    token: &str,
    body: Value,
) -> SosPolicyResponse {
    let response = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            "/api/v1/sos/policies",
            token,
            body,
        ))
        .await
        .expect("policy create request should succeed");

    assert_json_response(response, StatusCode::OK).await
}

async fn create_policy_approval_request(
    harness: &sos_api::SosApiHarness,
    token: &str,
    policy_id: &str,
    body: Value,
) -> SosPolicyApprovalRequestResponse {
    let response = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            &format!("/api/v1/sos/policies/{policy_id}/approval-requests"),
            token,
            body,
        ))
        .await
        .expect("policy approval-request create should succeed");

    assert_json_response(response, StatusCode::OK).await
}

async fn add_policy_approval_evidence(
    harness: &sos_api::SosApiHarness,
    token: &str,
    policy_id: &str,
    request_id: &str,
    body: Value,
) -> SosPolicyApprovalEvidenceResponse {
    let response = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            &format!("/api/v1/sos/policies/{policy_id}/approval-requests/{request_id}/evidence"),
            token,
            body,
        ))
        .await
        .expect("policy approval evidence request should succeed");

    assert_json_response(response, StatusCode::OK).await
}

async fn approve_policy_approval_request(
    harness: &sos_api::SosApiHarness,
    token: &str,
    policy_id: &str,
    request_id: &str,
    body: Value,
) -> SosPolicyApprovalRequestResponse {
    let response = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            &format!("/api/v1/sos/policies/{policy_id}/approval-requests/{request_id}/approve"),
            token,
            body,
        ))
        .await
        .expect("policy approval request approval should succeed");

    assert_json_response(response, StatusCode::OK).await
}

async fn reject_policy_approval_request(
    harness: &sos_api::SosApiHarness,
    token: &str,
    policy_id: &str,
    request_id: &str,
    body: Value,
) -> SosPolicyApprovalRequestResponse {
    let response = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            &format!("/api/v1/sos/policies/{policy_id}/approval-requests/{request_id}/reject"),
            token,
            body,
        ))
        .await
        .expect("policy approval request rejection should succeed");

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
                "tags": ["policy-api-test"],
            }),
        ))
        .await
        .expect("system registration request should succeed");

    assert_status(response, StatusCode::OK).await;
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
                    "required": ["sample_id", "score"],
                    "properties": {
                        "sample_id": {"type": "string"},
                        "score": {"type": "number"}
                    },
                    "additionalProperties": false
                },
                "coordinate_system": "WGS84",
                "unit_system": "SI",
                "metadata": {},
            }),
        ))
        .await
        .expect("interface registration request should succeed");

    assert_status(response, StatusCode::OK).await;
}

async fn register_projected_minimal_catalog(harness: &sos_api::SosApiHarness, token: &str) {
    register_system(harness, token, "provider-system", "Provider System").await;
    register_system(harness, token, "consumer-system", "Consumer System").await;
    register_interface(
        harness,
        token,
        "provider-system",
        PROVIDER_INTERFACE_ID,
        "Provider Interface",
        "Provider",
    )
    .await;
    register_interface(
        harness,
        token,
        "consumer-system",
        CONSUMER_INTERFACE_ID,
        "Consumer Interface",
        "Consumer",
    )
    .await;
}

#[tokio::test]
async fn build_router_policy_crud_endpoints_require_auth_and_preserve_revisions() {
    let harness = setup_authenticated_build_router_app_with_inline_secret_store();

    let unauthenticated_create = harness
        .app
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/api/v1/sos/policies",
            interface_pair_policy_body("pair-policy-http"),
        ))
        .await
        .expect("unauthenticated create request should complete");
    assert_status(unauthenticated_create, StatusCode::UNAUTHORIZED).await;

    let token = harness
        .token
        .as_deref()
        .expect("authenticated harness should expose a token");
    register_projected_minimal_catalog(&harness, token).await;

    let created = create_policy(
        &harness,
        token,
        interface_pair_policy_body("pair-policy-http"),
    )
    .await;
    assert_eq!(created.policy_id, "pair-policy-http");
    assert_eq!(created.revision, 1);
    assert_eq!(created.created_by, "architect-1");
    assert_eq!(created.updated_by, "architect-1");
    assert_eq!(created.lifecycle_state, "active");
    assert_eq!(created.approval_status, "approved");
    assert_eq!(created.approved_by.as_deref(), Some("architect-1"));
    assert_eq!(
        created.provider_interface_id.as_deref(),
        Some(PROVIDER_INTERFACE_ID)
    );
    assert_eq!(
        created.consumer_interface_id.as_deref(),
        Some(CONSUMER_INTERFACE_ID)
    );

    let secondary = create_policy(
        &harness,
        token,
        interface_policy_body("interface-policy-http"),
    )
    .await;
    assert_eq!(secondary.policy_id, "interface-policy-http");
    assert!(!secondary.active);
    assert_eq!(secondary.lifecycle_state, "draft");
    assert_eq!(secondary.approval_status, "pending");
    assert_eq!(
        secondary.approval_requested_by.as_deref(),
        Some("architect-2")
    );

    let unauthenticated_approve = harness
        .app
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/api/v1/sos/policies/interface-policy-http/approval-requests",
            json!({
                "requested_by": "intruder",
                "lifecycle_state": "active",
            }),
        ))
        .await
        .expect("unauthenticated approval-request create should complete");
    assert_status(unauthenticated_approve, StatusCode::UNAUTHORIZED).await;

    let unauthenticated_reject = harness
        .app
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/api/v1/sos/policies/interface-policy-http/approval-requests/fake-request/reject",
            json!({
                "rejected_by": "intruder",
                "reason": "No reason",
            }),
        ))
        .await
        .expect("unauthenticated rejection request should complete");
    assert_status(unauthenticated_reject, StatusCode::UNAUTHORIZED).await;

    let list_response = harness
        .app
        .clone()
        .oneshot(authed_empty_request(
            Method::GET,
            "/api/v1/sos/policies?target_type=interface_pair&stage=pre_execution&active=true&approval_status=approved&offset=0&limit=10",
            token,
        ))
        .await
        .expect("policy list request should succeed");
    let list: ListPoliciesResponse = assert_json_response(list_response, StatusCode::OK).await;

    assert_eq!(list.total, 1);
    assert_eq!(list.policies.len(), 1);
    assert_eq!(list.policies[0].policy_id, "pair-policy-http");
    assert_eq!(list.policies[0].revision, 1);

    let get_response = harness
        .app
        .clone()
        .oneshot(authed_empty_request(
            Method::GET,
            "/api/v1/sos/policies/pair-policy-http",
            token,
        ))
        .await
        .expect("policy get request should succeed");
    let fetched: SosPolicyResponse = assert_json_response(get_response, StatusCode::OK).await;

    assert_eq!(fetched.policy_name, "Policy pair-policy-http");
    assert_eq!(fetched.revision, 1);

    let update_response = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::PUT,
            "/api/v1/sos/policies/pair-policy-http",
            token,
            json!({
                "policy_name": "Policy pair-policy-http v2",
                "severity": "critical",
                "lifecycle_state": "draft",
                "description": "Updated interface pair policy",
                "tags": ["api-test", "pair", "revised"],
                "updated_by": "operator-2",
            }),
        ))
        .await
        .expect("policy update request should succeed");
    let updated: SosPolicyResponse = assert_json_response(update_response, StatusCode::OK).await;

    assert_eq!(updated.revision, 2);
    assert_eq!(updated.policy_name, "Policy pair-policy-http v2");
    assert_eq!(updated.updated_by, "operator-2");
    assert_eq!(updated.created_by, "architect-1");
    assert_eq!(updated.lifecycle_state, "draft");
    assert_eq!(updated.approval_status, "pending");
    assert!(!updated.active);
    assert_eq!(updated.superseded_by_revision, None);

    let approval_request = create_policy_approval_request(
        &harness,
        token,
        "pair-policy-http",
        json!({
            "requested_by": "operator-2",
            "lifecycle_state": "dry_run",
            "note": "Ready for dry-run rollout"
        }),
    )
    .await;

    assert_eq!(approval_request.policy_revision, 2);
    assert_eq!(approval_request.status, "pending");
    assert_eq!(approval_request.requested_lifecycle_state, "dry_run");
    assert_eq!(approval_request.requested_by, "operator-2");
    assert!(approval_request.evidence.is_empty());

    let approval_list_response = harness
        .app
        .clone()
        .oneshot(authed_empty_request(
            Method::GET,
            "/api/v1/sos/policies/pair-policy-http/approval-requests?status=pending&offset=0&limit=10",
            token,
        ))
        .await
        .expect("approval request list should succeed");
    let approval_list: ListPolicyApprovalRequestsResponse =
        assert_json_response(approval_list_response, StatusCode::OK).await;
    assert_eq!(approval_list.total, 1);
    assert_eq!(
        approval_list.requests[0].request_id,
        approval_request.request_id
    );

    let get_approval_response = harness
        .app
        .clone()
        .oneshot(authed_empty_request(
            Method::GET,
            &format!(
                "/api/v1/sos/policies/pair-policy-http/approval-requests/{}",
                approval_request.request_id
            ),
            token,
        ))
        .await
        .expect("approval request get should succeed");
    let fetched_approval: SosPolicyApprovalRequestResponse =
        assert_json_response(get_approval_response, StatusCode::OK).await;
    assert_eq!(fetched_approval.request_id, approval_request.request_id);

    let approval_evidence_validation_response = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            "/api/v1/sos/policies/pair-policy-http/validate",
            token,
            json!({"stage": "pre_execution"}),
        ))
        .await
        .expect("policy validation report generation should succeed");
    let approval_evidence_validation: ValidationResponse =
        assert_json_response(approval_evidence_validation_response, StatusCode::OK).await;
    let evidence_report_id = approval_evidence_validation
        .report_id
        .clone()
        .expect("policy validation should persist a report for approval evidence");

    let evidence = add_policy_approval_evidence(
        &harness,
        token,
        "pair-policy-http",
        &approval_request.request_id,
        json!({
            "report_id": evidence_report_id,
            "added_by": "qa-reviewer",
            "note": "Passing validation run for revision 2"
        }),
    )
    .await;
    assert_eq!(evidence.policy_revision, 2);
    assert_eq!(evidence.evidence_type, "validation_report");

    let approved = approve_policy_approval_request(
        &harness,
        token,
        "pair-policy-http",
        &approval_request.request_id,
        json!({
            "approved_by": "reviewer-1"
        }),
    )
    .await;
    assert_eq!(approved.status, "approved");
    assert_eq!(approved.approved_by.as_deref(), Some("reviewer-1"));
    assert_eq!(approved.evidence.len(), 1);

    let policy_fetch_response = harness
        .app
        .clone()
        .oneshot(authed_empty_request(
            Method::GET,
            "/api/v1/sos/policies/pair-policy-http",
            token,
        ))
        .await
        .expect("policy get request should succeed after approval");
    let approved_policy: SosPolicyResponse =
        assert_json_response(policy_fetch_response, StatusCode::OK).await;
    let attestation = approved_policy
        .attestation
        .as_ref()
        .expect("approved policy should expose a revision attestation");
    assert_eq!(attestation.policy_revision, 2);
    assert_eq!(
        attestation.approval_request_id.as_deref(),
        Some(approval_request.request_id.as_str())
    );
    assert_eq!(attestation.evidence_ids, vec![evidence.evidence_id.clone()]);
    assert_eq!(attestation.signing_key_source, "secret_store");
    assert!(attestation.attestation_verified);

    let revisions = harness
        .storage_manager
        .list_policy_revisions("pair-policy-http", 10)
        .expect("policy revisions should be queryable after API update");
    assert_eq!(revisions.len(), 2);
    assert_eq!(revisions[0].revision, 2);
    assert_eq!(revisions[0].updated_by, "reviewer-1");
    assert_eq!(revisions[0].lifecycle_state.as_deref(), Some("dry_run"));
    assert_eq!(revisions[0].approval_status.as_deref(), Some("approved"));
    assert_eq!(revisions[1].revision, 1);
    assert_eq!(revisions[1].superseded_by_revision, Some(2));

    let reject_request = create_policy_approval_request(
        &harness,
        token,
        "interface-policy-http",
        json!({
            "requested_by": "architect-2",
            "lifecycle_state": "active",
            "note": "Hold until evidence is reviewed"
        }),
    )
    .await;
    let rejected = reject_policy_approval_request(
        &harness,
        token,
        "interface-policy-http",
        &reject_request.request_id,
        json!({
            "rejected_by": "reviewer-2",
            "reason": "Waiting for rollout evidence"
        }),
    )
    .await;
    assert_eq!(rejected.status, "rejected");
    assert_eq!(rejected.rejected_by.as_deref(), Some("reviewer-2"));
    assert_eq!(
        rejected.rejection_reason.as_deref(),
        Some("Waiting for rollout evidence")
    );

    let delete_response = harness
        .app
        .clone()
        .oneshot(authed_empty_request(
            Method::DELETE,
            "/api/v1/sos/policies/pair-policy-http",
            token,
        ))
        .await
        .expect("policy delete request should succeed");
    assert_status(delete_response, StatusCode::NO_CONTENT).await;

    let missing_response = harness
        .app
        .clone()
        .oneshot(authed_empty_request(
            Method::GET,
            "/api/v1/sos/policies/pair-policy-http",
            token,
        ))
        .await
        .expect("deleted policy lookup should complete");
    assert_status(missing_response, StatusCode::NOT_FOUND).await;

    assert!(
        harness
            .storage_manager
            .list_policy_revisions("pair-policy-http", 10)
            .expect("policy revisions should be queryable after delete")
            .is_empty(),
        "deleting a policy through the API should remove all stored revisions"
    );
    assert!(
        harness
            .storage_manager
            .list_policy_approval_requests("pair-policy-http", 10)
            .expect("policy approval requests should be queryable after delete")
            .is_empty(),
        "deleting a policy through the API should remove approval requests and evidence"
    );
}

#[tokio::test]
async fn build_router_policy_validate_endpoints_persist_reports_and_keep_dry_runs_ephemeral() {
    let harness = setup_authenticated_build_router_app_with_inline_secret_store();

    let unauthenticated_validate = harness
        .app
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/api/v1/sos/policies/pair-policy-http/validate",
            json!({"stage": "pre_execution"}),
        ))
        .await
        .expect("unauthenticated validate request should complete");
    assert_status(unauthenticated_validate, StatusCode::UNAUTHORIZED).await;

    let token = harness
        .token
        .as_deref()
        .expect("authenticated harness should expose a token");

    register_projected_minimal_catalog(&harness, token).await;
    create_policy(
        &harness,
        token,
        interface_pair_policy_body("pair-policy-http"),
    )
    .await;
    let updated_response = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::PUT,
            "/api/v1/sos/policies/pair-policy-http",
            token,
            json!({
                "severity": "critical",
                "lifecycle_state": "draft",
                "updated_by": "operator-9",
            }),
        ))
        .await
        .expect("policy update request should succeed");
    let updated: SosPolicyResponse = assert_json_response(updated_response, StatusCode::OK).await;
    assert_eq!(updated.revision, 2);
    assert_eq!(updated.approval_status, "pending");

    let approval_request = create_policy_approval_request(
        &harness,
        token,
        "pair-policy-http",
        json!({
            "requested_by": "operator-9",
            "lifecycle_state": "active",
        }),
    )
    .await;

    let premature_approve = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            &format!(
                "/api/v1/sos/policies/pair-policy-http/approval-requests/{}/approve",
                approval_request.request_id
            ),
            token,
            json!({
                "approved_by": "reviewer-9"
            }),
        ))
        .await
        .expect("premature approval request should complete");
    assert_status(premature_approve, StatusCode::BAD_REQUEST).await;

    let persisted_response = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            "/api/v1/sos/policies/pair-policy-http/validate",
            token,
            json!({"stage": "pre_execution"}),
        ))
        .await
        .expect("persisted policy validate request should succeed");
    let persisted: ValidationResponse =
        assert_json_response(persisted_response, StatusCode::OK).await;
    let report_id = persisted
        .report_id
        .clone()
        .expect("persisted policy evaluation should return a report id");

    let evidence = add_policy_approval_evidence(
        &harness,
        token,
        "pair-policy-http",
        &approval_request.request_id,
        json!({
            "report_id": report_id.clone(),
            "added_by": "qa-reviewer",
            "note": "Passing report for rollout"
        }),
    )
    .await;
    assert_eq!(evidence.report_id, report_id);

    let approved = approve_policy_approval_request(
        &harness,
        token,
        "pair-policy-http",
        &approval_request.request_id,
        json!({
            "approved_by": "reviewer-9"
        }),
    )
    .await;
    assert_eq!(approved.status, "approved");

    assert!(
        persisted.passed,
        "policy evaluation should pass for the seeded catalog"
    );

    let report_response = harness
        .app
        .clone()
        .oneshot(authed_empty_request(
            Method::GET,
            &format!("/api/v1/sos/validation-reports/{report_id}"),
            token,
        ))
        .await
        .expect("policy report lookup should succeed");
    let report: ValidationReportResponse =
        assert_json_response(report_response, StatusCode::OK).await;

    assert_eq!(report.subject_type, "policy");
    assert_eq!(report.subject_key, "policy:pair-policy-http");
    assert_eq!(report.validation_type, "policy_check");
    assert!(
        report
            .policy_refs
            .contains(&"policy:pair-policy-http".to_string()),
        "report should retain the stable policy reference"
    );
    assert!(
        report
            .policy_refs
            .contains(&"policy:pair-policy-http@2".to_string()),
        "report should retain the evaluated policy revision reference"
    );

    let policy_check = report
        .checks
        .iter()
        .find(|check| check.check_name == "policy:pair-policy-http")
        .expect("policy check should be present in the report");
    let details = policy_check
        .details
        .as_ref()
        .expect("policy check should include execution details");
    assert_eq!(details.get("policy_revision"), Some(&json!(2)));
    assert_eq!(details.get("policy_updated_by"), Some(&json!("operator-9")));
    assert_eq!(
        details.get("policy_approval_status"),
        Some(&json!("pending"))
    );
    assert_eq!(
        details.get("policy_approval_requested_by"),
        Some(&json!("operator-9"))
    );

    let pinned_response = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            "/api/v1/sos/policies/pair-policy-http/validate",
            token,
            json!({"stage": "pre_execution", "revision": 1}),
        ))
        .await
        .expect("pinned policy validate request should succeed");
    let pinned: ValidationResponse = assert_json_response(pinned_response, StatusCode::OK).await;
    let pinned_report_id = pinned
        .report_id
        .clone()
        .expect("pinned policy evaluation should persist a report id");
    let pinned_report_response = harness
        .app
        .clone()
        .oneshot(authed_empty_request(
            Method::GET,
            &format!("/api/v1/sos/validation-reports/{pinned_report_id}"),
            token,
        ))
        .await
        .expect("pinned policy report lookup should succeed");
    let pinned_report: ValidationReportResponse =
        assert_json_response(pinned_report_response, StatusCode::OK).await;
    assert!(
        pinned_report
            .policy_refs
            .contains(&"policy:pair-policy-http@1".to_string()),
        "pinned evaluation should use the requested revision"
    );
    let pinned_check = pinned_report
        .checks
        .iter()
        .find(|check| check.check_name == "policy:pair-policy-http")
        .expect("pinned policy check should be present");
    let pinned_details = pinned_check
        .details
        .as_ref()
        .expect("pinned policy check should include execution details");
    assert_eq!(pinned_details.get("policy_revision"), Some(&json!(1)));

    let dry_run_response = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            "/api/v1/sos/policies/pair-policy-http/validate/dry-run",
            token,
            json!({"stage": "pre_execution"}),
        ))
        .await
        .expect("dry-run policy validate request should succeed");
    let dry_run: ValidationResponse = assert_json_response(dry_run_response, StatusCode::OK).await;

    assert!(dry_run.passed, "dry-run policy evaluation should also pass");
    assert!(
        dry_run.report_id.is_none(),
        "dry-run policy evaluation must not persist a report"
    );

    let history_response = harness
        .app
        .oneshot(authed_empty_request(
            Method::GET,
            "/api/v1/sos/validation-history?subject_key=policy:pair-policy-http&subject_type=policy&limit=10",
            token,
        ))
        .await
        .expect("policy history request should succeed");
    let history: ValidationHistoryResponse =
        assert_json_response(history_response, StatusCode::OK).await;

    assert_eq!(history.subject_type, "policy");
    assert_eq!(history.subject_key, "policy:pair-policy-http");
    assert_eq!(history.reports.len(), 2);
    let history_report_ids = history
        .reports
        .iter()
        .map(|report| report.report_id.clone())
        .collect::<Vec<_>>();
    assert!(
        history_report_ids.contains(&report_id),
        "persisted latest-revision report should still be present after dry-run"
    );
    assert!(
        history_report_ids.contains(&pinned_report_id),
        "pinned persisted report should remain present after dry-run"
    );
}
