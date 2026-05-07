//! HTTP/router coverage for revisioned SoS contract governance.

#[path = "support/sos_api.rs"]
mod sos_api;

use axum::http::{Method, StatusCode};
use chrono::Utc;
use graphica_coordinator::api::sos_validation::{
    storage::{Interface, System},
    types::{
        DataContractResponse, ListContractApprovalRequestsResponse,
        SosContractApprovalEvidenceResponse, SosContractApprovalRequestResponse, SosErrorResponse,
        ValidationReportResponse, ValidationResponse,
    },
};
use serde_json::json;
use serial_test::serial;
use sos_api::{
    assert_json_response, assert_status, authed_empty_request, authed_json_request,
    seed_minimal_catalog, setup_authenticated_build_router_app, subject_exists, subject_objects,
    CONTRACT_ID,
};
use std::collections::HashMap;
use tower::ServiceExt;

const CATALOG_GRAPH: &str = "http://graphica.io/graph/sos-catalog";
const GOVERNANCE_GRAPH: &str = "http://graphica.io/graph/sos-governance";
const VALIDATION_GRAPH: &str = "http://graphica.io/graph/sos-validations";
const PROV_USED: &str = "http://www.w3.org/ns/prov#used";
const SOS_HAS_SIGNATURE_ATTESTATION: &str = "http://graphica.io/sos#hasSignatureAttestation";

fn contract_uri(contract_id: &str) -> String {
    format!("http://graphica.io/sos/contract/{contract_id}")
}

fn contract_revision_uri(contract_id: &str, revision: u32) -> String {
    format!("{}/revision/{revision}", contract_uri(contract_id))
}

fn contract_approval_request_uri(contract_id: &str, request_id: &str) -> String {
    format!(
        "{}/approval-request/{request_id}",
        contract_uri(contract_id)
    )
}

fn contract_approval_evidence_uri(
    contract_id: &str,
    request_id: &str,
    evidence_id: &str,
) -> String {
    format!(
        "{}/approval-request/{request_id}/evidence/{evidence_id}",
        contract_uri(contract_id),
    )
}

fn contract_signature_uri(contract_id: &str, revision: u32, signature_id: &str) -> String {
    format!(
        "{}/revision/{revision}/signature/{signature_id}",
        contract_uri(contract_id),
    )
}

fn validation_activity_uri(validation_id: &str) -> String {
    format!("http://graphica.io/sos/validation/{validation_id}")
}

fn rdf_uri_object(uri: &str) -> String {
    format!("<{uri}>")
}

struct EnvGuard {
    vars: Vec<(&'static str, Option<String>)>,
}

impl EnvGuard {
    fn set(pairs: &[(&'static str, &'static str)]) -> Self {
        let vars = pairs
            .iter()
            .map(|(key, value)| {
                let previous = std::env::var(key).ok();
                std::env::set_var(key, value);
                (*key, previous)
            })
            .collect();

        Self { vars }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (key, previous) in self.vars.drain(..) {
            if let Some(value) = previous {
                std::env::set_var(key, value);
            } else {
                std::env::remove_var(key);
            }
        }
    }
}

async fn create_contract(
    harness: &sos_api::SosApiHarness,
    token: &str,
    contract_id: &str,
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
                "provider_interface_id": "provider-if",
                "consumer_interface_id": "consumer-if",
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

async fn update_contract(
    harness: &sos_api::SosApiHarness,
    token: &str,
    contract_id: &str,
) -> DataContractResponse {
    let response = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::PUT,
            &format!("/api/v1/sos/contracts/{contract_id}"),
            token,
            json!({
                "description": "Updated contract semantics",
            }),
        ))
        .await
        .expect("contract update request should succeed");

    assert_json_response(response, StatusCode::OK).await
}

async fn approve_contract(
    harness: &sos_api::SosApiHarness,
    token: &str,
    contract_id: &str,
) -> DataContractResponse {
    let response = harness
        .app
        .clone()
        .oneshot(authed_empty_request(
            Method::POST,
            &format!("/api/v1/sos/contracts/{contract_id}/approve"),
            token,
        ))
        .await
        .expect("contract approve request should succeed");

    assert_json_response(response, StatusCode::OK).await
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
                "note": "Ready for approval review",
            }),
        ))
        .await
        .expect("contract approval-request create should succeed");

    assert_json_response(response, StatusCode::OK).await
}

async fn add_contract_approval_evidence(
    harness: &sos_api::SosApiHarness,
    token: &str,
    contract_id: &str,
    request_id: &str,
    report_id: &str,
) -> SosContractApprovalEvidenceResponse {
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
                "note": "Passing contract validation",
            }),
        ))
        .await
        .expect("contract approval evidence request should succeed");

    assert_json_response(response, StatusCode::OK).await
}

async fn approve_contract_approval_request(
    harness: &sos_api::SosApiHarness,
    token: &str,
    contract_id: &str,
    request_id: &str,
) -> SosContractApprovalRequestResponse {
    let response = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            &format!("/api/v1/sos/contracts/{contract_id}/approval-requests/{request_id}/approve"),
            token,
            json!({
                "approved_by": "reviewer-1",
            }),
        ))
        .await
        .expect("contract approval request approve should succeed");

    assert_json_response(response, StatusCode::OK).await
}

async fn reject_contract_approval_request(
    harness: &sos_api::SosApiHarness,
    token: &str,
    contract_id: &str,
    request_id: &str,
) -> SosContractApprovalRequestResponse {
    let response = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            &format!("/api/v1/sos/contracts/{contract_id}/approval-requests/{request_id}/reject"),
            token,
            json!({
                "rejected_by": "reviewer-2",
                "reason": "Need updated validation evidence",
            }),
        ))
        .await
        .expect("contract approval request reject should succeed");

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
        .oneshot(authed_empty_request(
            Method::POST,
            &format!("/api/v1/sos/contracts/{contract_id}/sign"),
            token,
        ))
        .await
        .expect("contract sign request should succeed");

    assert_json_response(response, StatusCode::OK).await
}

async fn create_contract_policy(
    harness: &sos_api::SosApiHarness,
    token: &str,
    policy_id: &str,
    contract_id: &str,
    stage: &str,
    sparql_query: &str,
) {
    let response = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            "/api/v1/sos/policies",
            token,
            json!({
                "policy_id": policy_id,
                "policy_name": format!("Policy {policy_id}"),
                "target_type": "contract",
                "stages": [stage],
                "enforcement_level": "mandatory",
                "severity": "high",
                "sparql_query": sparql_query,
                "context": {},
                "description": format!("Contract governance policy for {stage}"),
                "created_by": "policy-admin",
                "tags": ["contract-governance", "api-test"],
                "active": true,
                "contract_id": contract_id,
            }),
        ))
        .await
        .expect("contract policy create request should succeed");

    assert_status(response, StatusCode::OK).await;
}

async fn delete_policy(harness: &sos_api::SosApiHarness, token: &str, policy_id: &str) {
    let response = harness
        .app
        .clone()
        .oneshot(authed_empty_request(
            Method::DELETE,
            &format!("/api/v1/sos/policies/{policy_id}"),
            token,
        ))
        .await
        .expect("policy delete request should succeed");

    assert_status(response, StatusCode::NO_CONTENT).await;
}

fn seed_contract_interfaces_only(harness: &sos_api::SosApiHarness) {
    harness
        .storage_manager
        .put_system(&System {
            system_id: "provider-system".to_string(),
            system_name: "Provider System".to_string(),
            system_type: "provider.synthetic".to_string(),
            vendor: "Graphica".to_string(),
            version: "1.0.0".to_string(),
            classification: "UNCLASSIFIED".to_string(),
            description: Some("Provider test system".to_string()),
            deployment: HashMap::new(),
            capabilities: HashMap::new(),
            tags: vec!["contract-api-test".to_string()],
            active: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
        .expect("provider system should be stored");
    harness
        .storage_manager
        .put_system(&System {
            system_id: "consumer-system".to_string(),
            system_name: "Consumer System".to_string(),
            system_type: "consumer.synthetic".to_string(),
            vendor: "Graphica".to_string(),
            version: "1.0.0".to_string(),
            classification: "UNCLASSIFIED".to_string(),
            description: Some("Consumer test system".to_string()),
            deployment: HashMap::new(),
            capabilities: HashMap::new(),
            tags: vec!["contract-api-test".to_string()],
            active: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
        .expect("consumer system should be stored");
    harness
        .storage_manager
        .put_interface(&Interface {
            interface_id: "provider-if".to_string(),
            system_id: "provider-system".to_string(),
            interface_name: "Provider Interface".to_string(),
            direction: "Provider".to_string(),
            protocol: "REST".to_string(),
            data_format: "JSON".to_string(),
            schema: json!({"type": "object", "properties": {}, "additionalProperties": true}),
            coordinate_system: Some("WGS84".to_string()),
            unit_system: Some("SI".to_string()),
            metadata: HashMap::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
        .expect("provider interface should be stored");
    harness
        .storage_manager
        .put_interface(&Interface {
            interface_id: "consumer-if".to_string(),
            system_id: "consumer-system".to_string(),
            interface_name: "Consumer Interface".to_string(),
            direction: "Consumer".to_string(),
            protocol: "REST".to_string(),
            data_format: "JSON".to_string(),
            schema: json!({"type": "object", "properties": {}, "additionalProperties": true}),
            coordinate_system: Some("WGS84".to_string()),
            unit_system: Some("SI".to_string()),
            metadata: HashMap::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
        .expect("consumer interface should be stored");
}

#[tokio::test]
#[serial]
async fn build_router_contract_create_rejects_malformed_transformation_rules() {
    let harness = setup_authenticated_build_router_app();
    let token = harness
        .token
        .as_deref()
        .expect("authenticated harness should expose a token");
    seed_contract_interfaces_only(&harness);

    let response = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            "/api/v1/sos/contracts",
            token,
            json!({
                "contract_id": "contract-invalid-transform",
                "contract_name": "Invalid Transform Contract",
                "provider_interface_id": "provider-if",
                "consumer_interface_id": "consumer-if",
                "sla_metrics": [{
                    "name": "latency_ms",
                    "value": 100.0,
                    "operator": "<=",
                    "unit": "ms"
                }],
                "transformation_rules": {
                    "unit_transform": "SI->Imperial"
                },
                "description": "Malformed transform rule",
                "tags": ["api-test"]
            }),
        ))
        .await
        .expect("malformed contract creation request should complete");

    let error: SosErrorResponse = assert_json_response(response, StatusCode::BAD_REQUEST).await;
    assert_eq!(error.error, "INVALID_TRANSFORMATION_RULES");
    assert!(error.message.contains("must be an object"));
}

#[tokio::test]
#[serial]
async fn build_router_contract_create_rejects_incomplete_unit_transform_semantics() {
    let harness = setup_authenticated_build_router_app();
    let token = harness
        .token
        .as_deref()
        .expect("authenticated harness should expose a token");
    seed_contract_interfaces_only(&harness);

    let response = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            "/api/v1/sos/contracts",
            token,
            json!({
                "contract_id": "contract-incomplete-unit-transform",
                "contract_name": "Incomplete Unit Transform Contract",
                "provider_interface_id": "provider-if",
                "consumer_interface_id": "consumer-if",
                "sla_metrics": [{
                    "name": "latency_ms",
                    "value": 100.0,
                    "operator": "<=",
                    "unit": "ms"
                }],
                "transformation_rules": {
                    "unit_transform": {
                        "from": "SI",
                        "to": "Imperial"
                    }
                },
                "description": "Transform rule lacks executable unit semantics",
                "tags": ["api-test"]
            }),
        ))
        .await
        .expect("contract creation request should complete");

    let error: SosErrorResponse = assert_json_response(response, StatusCode::BAD_REQUEST).await;
    assert_eq!(error.error, "INVALID_TRANSFORMATION_RULES");
    assert!(error
        .message
        .contains("must declare a unit conversion strategy"));
}

#[tokio::test]
#[serial]
async fn build_router_contract_create_rejects_incomplete_coordinate_transform_semantics() {
    let harness = setup_authenticated_build_router_app();
    let token = harness
        .token
        .as_deref()
        .expect("authenticated harness should expose a token");
    seed_contract_interfaces_only(&harness);
    harness
        .storage_manager
        .put_interface(&Interface {
            interface_id: "consumer-if".to_string(),
            system_id: "consumer-system".to_string(),
            interface_name: "Consumer Interface".to_string(),
            direction: "Consumer".to_string(),
            protocol: "REST".to_string(),
            data_format: "JSON".to_string(),
            schema: json!({"type": "object", "properties": {}, "additionalProperties": true}),
            coordinate_system: Some("ECI_J2000".to_string()),
            unit_system: Some("SI".to_string()),
            metadata: HashMap::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
        .expect("consumer interface should be updated");

    let response = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            "/api/v1/sos/contracts",
            token,
            json!({
                "contract_id": "contract-incomplete-coordinate-transform",
                "contract_name": "Incomplete Coordinate Transform Contract",
                "provider_interface_id": "provider-if",
                "consumer_interface_id": "consumer-if",
                "sla_metrics": [{
                    "name": "latency_ms",
                    "value": 100.0,
                    "operator": "<=",
                    "unit": "ms"
                }],
                "transformation_rules": {
                    "coordinate_transform": {
                        "from": "WGS84",
                        "to": "ECI_J2000"
                    }
                },
                "description": "Transform rule lacks executable coordinate semantics",
                "tags": ["api-test"]
            }),
        ))
        .await
        .expect("contract creation request should complete");

    let error: SosErrorResponse = assert_json_response(response, StatusCode::BAD_REQUEST).await;
    assert_eq!(error.error, "INVALID_TRANSFORMATION_RULES");
    assert!(error
        .message
        .contains("must declare a coordinate conversion strategy"));
}

#[tokio::test]
#[serial]
async fn build_router_interface_validation_rejects_misaligned_unit_transform_rule() {
    let harness = setup_authenticated_build_router_app();
    let token = harness
        .token
        .as_deref()
        .expect("authenticated harness should expose a token");
    seed_contract_interfaces_only(&harness);

    let created = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            "/api/v1/sos/contracts",
            token,
            json!({
                "contract_id": "contract-misaligned-transform",
                "contract_name": "Misaligned Transform Contract",
                "provider_interface_id": "provider-if",
                "consumer_interface_id": "consumer-if",
                "sla_metrics": [{
                    "name": "latency_ms",
                    "value": 100.0,
                    "operator": "<=",
                    "unit": "ms"
                }],
                "transformation_rules": {
                    "unit_transform": {
                        "from": "Imperial",
                        "to": "SI",
                        "strategy": "linear_scale",
                        "scale": 0.3048,
                        "offset": 0.0
                    }
                },
                "description": "Rule endpoints do not match the interface direction",
                "tags": ["api-test"]
            }),
        ))
        .await
        .expect("contract creation request should succeed");
    let _: DataContractResponse = assert_json_response(created, StatusCode::OK).await;

    let validation_response = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            "/api/v1/sos/validate",
            token,
            json!({
                "type": "interface_compatibility",
                "provider_interface_id": "provider-if",
                "consumer_interface_id": "consumer-if"
            }),
        ))
        .await
        .expect("interface compatibility request should succeed");
    let validation: ValidationResponse =
        assert_json_response(validation_response, StatusCode::OK).await;

    assert!(!validation.passed);
    let unit_check = validation
        .checks
        .iter()
        .find(|check| check.check_name == "unit_compatibility")
        .expect("unit compatibility check should be present");
    assert!(!unit_check.passed);
    assert!(unit_check
        .description
        .contains("maps Imperial -> SI instead"));
}

#[tokio::test]
#[serial]
async fn build_router_contract_governance_preserves_revisions_and_audit_metadata() {
    let _guard = EnvGuard::set(&[(
        "GRAPHICA_SOS_CONTRACT_SIGNING_KEY_HEX",
        "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
    )]);
    let harness = setup_authenticated_build_router_app();
    let token = harness
        .token
        .as_deref()
        .expect("authenticated harness should expose a token");
    seed_contract_interfaces_only(&harness);

    let created = create_contract(&harness, token, "contract-governance-http").await;
    assert_eq!(created.revision, 1);
    assert_eq!(created.lifecycle_state, "draft");
    assert_eq!(created.created_by, "test_user");
    assert_eq!(created.updated_by, "test_user");
    assert_eq!(created.approval_status, "pending");
    assert!(!created.approved);
    assert!(!created.signed);

    let unauthenticated_request_create = harness
        .app
        .clone()
        .oneshot(sos_api::json_request(
            Method::POST,
            "/api/v1/sos/contracts/contract-governance-http/approval-requests",
            json!({
                "requested_by": "intruder",
                "lifecycle_state": "approved",
            }),
        ))
        .await
        .expect("unauthenticated contract approval-request create should complete");
    assert_status(unauthenticated_request_create, StatusCode::UNAUTHORIZED).await;

    let approval_request_v1 =
        create_contract_approval_request(&harness, token, "contract-governance-http").await;
    assert_eq!(approval_request_v1.contract_revision, 1);
    assert_eq!(approval_request_v1.status, "pending");
    assert_eq!(approval_request_v1.requested_by, "governance-operator");
    assert!(approval_request_v1.evidence.is_empty());

    let approval_list_response = harness
        .app
        .clone()
        .oneshot(authed_empty_request(
            Method::GET,
            "/api/v1/sos/contracts/contract-governance-http/approval-requests?status=pending&offset=0&limit=10",
            token,
        ))
        .await
        .expect("contract approval request list should succeed");
    let approval_list: ListContractApprovalRequestsResponse =
        assert_json_response(approval_list_response, StatusCode::OK).await;
    assert_eq!(approval_list.total, 1);
    assert_eq!(
        approval_list.requests[0].request_id,
        approval_request_v1.request_id
    );

    let get_approval_response = harness
        .app
        .clone()
        .oneshot(authed_empty_request(
            Method::GET,
            &format!(
                "/api/v1/sos/contracts/contract-governance-http/approval-requests/{}",
                approval_request_v1.request_id
            ),
            token,
        ))
        .await
        .expect("contract approval request get should succeed");
    let fetched_approval: SosContractApprovalRequestResponse =
        assert_json_response(get_approval_response, StatusCode::OK).await;
    assert_eq!(fetched_approval.request_id, approval_request_v1.request_id);

    let validation_v1_response = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            "/api/v1/sos/validate",
            token,
            json!({
                "type": "interface_compatibility",
                "provider_interface_id": "provider-if",
                "consumer_interface_id": "consumer-if",
            }),
        ))
        .await
        .expect("contract validation for approval evidence should succeed");
    let validation_v1: ValidationResponse =
        assert_json_response(validation_v1_response, StatusCode::OK).await;
    let report_id_v1 = validation_v1
        .report_id
        .clone()
        .expect("contract validation should persist report id");

    let evidence_v1 = add_contract_approval_evidence(
        &harness,
        token,
        "contract-governance-http",
        &approval_request_v1.request_id,
        &report_id_v1,
    )
    .await;
    assert_eq!(evidence_v1.contract_revision, 1);

    let approved_v1 = approve_contract_approval_request(
        &harness,
        token,
        "contract-governance-http",
        &approval_request_v1.request_id,
    )
    .await;
    assert_eq!(approved_v1.status, "approved");
    assert_eq!(approved_v1.approved_by.as_deref(), Some("reviewer-1"));
    assert_eq!(approved_v1.evidence.len(), 1);

    let approved_v1_contract = harness
        .storage_manager
        .get_contract("contract-governance-http")
        .expect("contract lookup should succeed")
        .expect("contract should exist");
    assert!(approved_v1_contract.approved);
    assert_eq!(
        approved_v1_contract.approved_by.as_deref(),
        Some("reviewer-1")
    );

    let updated = update_contract(&harness, token, "contract-governance-http").await;
    assert_eq!(updated.revision, 2);
    assert_eq!(updated.lifecycle_state, "draft");
    assert_eq!(updated.approval_status, "pending");
    assert!(!updated.approved);
    assert!(!updated.signed);
    assert_eq!(updated.approved_by, None);
    assert_eq!(updated.updated_by, "test_user");

    let revisions = harness
        .storage_manager
        .list_contract_revisions("contract-governance-http", 10)
        .expect("contract revisions should be queryable after update");
    assert_eq!(revisions.len(), 2);
    assert_eq!(revisions[0].revision, 2);
    assert_eq!(revisions[0].updated_by, "test_user");
    assert_eq!(revisions[1].revision, 1);
    assert_eq!(revisions[1].superseded_by_revision, Some(2));
    assert_eq!(revisions[1].approved_by.as_deref(), Some("reviewer-1"));

    let rejected_request =
        create_contract_approval_request(&harness, token, "contract-governance-http").await;
    let rejected = reject_contract_approval_request(
        &harness,
        token,
        "contract-governance-http",
        &rejected_request.request_id,
    )
    .await;
    assert_eq!(rejected.status, "rejected");
    assert_eq!(rejected.rejected_by.as_deref(), Some("reviewer-2"));
    assert_eq!(
        rejected.rejection_reason.as_deref(),
        Some("Need updated validation evidence")
    );

    let premature_legacy_approve = harness
        .app
        .clone()
        .oneshot(authed_empty_request(
            Method::POST,
            "/api/v1/sos/contracts/contract-governance-http/approve",
            token,
        ))
        .await
        .expect("premature legacy approve request should complete");
    assert_status(premature_legacy_approve, StatusCode::BAD_REQUEST).await;

    let premature_sign = harness
        .app
        .clone()
        .oneshot(authed_empty_request(
            Method::POST,
            "/api/v1/sos/contracts/contract-governance-http/sign",
            token,
        ))
        .await
        .expect("premature sign request should complete");
    assert_status(premature_sign, StatusCode::BAD_REQUEST).await;

    let approval_request_v2 =
        create_contract_approval_request(&harness, token, "contract-governance-http").await;
    let validation_v2_response = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            "/api/v1/sos/validate",
            token,
            json!({
                "type": "interface_compatibility",
                "provider_interface_id": "provider-if",
                "consumer_interface_id": "consumer-if",
            }),
        ))
        .await
        .expect("contract validation for v2 evidence should succeed");
    let validation_v2: ValidationResponse =
        assert_json_response(validation_v2_response, StatusCode::OK).await;
    let report_id_v2 = validation_v2
        .report_id
        .clone()
        .expect("contract validation should persist report id");
    let evidence_v2 = add_contract_approval_evidence(
        &harness,
        token,
        "contract-governance-http",
        &approval_request_v2.request_id,
        &report_id_v2,
    )
    .await;
    assert_eq!(evidence_v2.contract_revision, 2);

    let approved_v2 = approve_contract(&harness, token, "contract-governance-http").await;
    assert_eq!(approved_v2.revision, 2);
    assert_eq!(approved_v2.lifecycle_state, "approved");
    assert_eq!(approved_v2.approved_by.as_deref(), Some("test_user"));
    assert_eq!(approved_v2.approval_status, "approved");
    assert_eq!(
        approved_v2.approval_requested_by.as_deref(),
        Some("governance-operator")
    );

    let signed = sign_contract(&harness, token, "contract-governance-http").await;
    assert_eq!(signed.revision, 2);
    assert_eq!(signed.lifecycle_state, "signed");
    assert!(signed.approved);
    assert!(signed.signed);
    assert_eq!(signed.signed_by.as_deref(), Some("test_user"));
    let signature = signed
        .signature
        .as_ref()
        .expect("signed contract should include attestation material");
    assert_eq!(
        signature.approval_request_id.as_deref(),
        Some(approval_request_v2.request_id.as_str())
    );
    assert_eq!(
        signature.evidence_ids,
        vec![evidence_v2.evidence_id.clone()]
    );
    assert!(signature.signature_verified);
    assert_eq!(
        signature.contract_revision_ref,
        "contract:contract-governance-http@2"
    );

    assert!(subject_exists(
        &harness.rdf_store,
        CATALOG_GRAPH,
        &contract_uri("contract-governance-http"),
    ));
    assert!(subject_exists(
        &harness.rdf_store,
        CATALOG_GRAPH,
        &contract_revision_uri("contract-governance-http", 1),
    ));
    assert!(subject_exists(
        &harness.rdf_store,
        CATALOG_GRAPH,
        &contract_revision_uri("contract-governance-http", 2),
    ));
    assert!(subject_exists(
        &harness.rdf_store,
        GOVERNANCE_GRAPH,
        &contract_approval_request_uri("contract-governance-http", &approval_request_v2.request_id),
    ));
    assert!(subject_exists(
        &harness.rdf_store,
        GOVERNANCE_GRAPH,
        &contract_approval_evidence_uri(
            "contract-governance-http",
            &approval_request_v2.request_id,
            &evidence_v2.evidence_id,
        ),
    ));
    let signature_uri =
        contract_signature_uri("contract-governance-http", 2, &signature.signature_id);
    assert!(subject_exists(
        &harness.rdf_store,
        GOVERNANCE_GRAPH,
        &signature_uri,
    ));
    assert!(subject_objects(
        &harness.rdf_store,
        CATALOG_GRAPH,
        &contract_revision_uri("contract-governance-http", 2),
        SOS_HAS_SIGNATURE_ATTESTATION,
    )
    .contains(&rdf_uri_object(&signature_uri)));
    assert!(subject_objects(
        &harness.rdf_store,
        GOVERNANCE_GRAPH,
        &contract_approval_evidence_uri(
            "contract-governance-http",
            &approval_request_v2.request_id,
            &evidence_v2.evidence_id,
        ),
        PROV_USED,
    )
    .iter()
    .any(|object| object
        == &rdf_uri_object(&format!(
            "http://graphica.io/sos/validation-report/{report_id_v2}"
        ))));

    let delete_response = harness
        .app
        .clone()
        .oneshot(authed_empty_request(
            Method::DELETE,
            "/api/v1/sos/contracts/contract-governance-http",
            token,
        ))
        .await
        .expect("signed contract delete request should complete");
    assert_status(delete_response, StatusCode::CONFLICT).await;

    assert!(
        harness
            .storage_manager
            .list_contract_approval_requests("contract-governance-http", 10)
            .expect("contract approval requests should remain queryable")
            .len()
            >= 3
    );
}

#[tokio::test]
async fn build_router_contract_validation_persists_revisioned_contract_refs() {
    let harness = setup_authenticated_build_router_app();
    let token = harness
        .token
        .as_deref()
        .expect("authenticated harness should expose a token");
    seed_minimal_catalog(&harness.storage_manager);

    let response = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            "/api/v1/sos/validate",
            token,
            json!({
                "type": "contract_compliance",
                "contract_id": CONTRACT_ID,
            }),
        ))
        .await
        .expect("contract validation request should succeed");
    let validation: ValidationResponse = assert_json_response(response, StatusCode::OK).await;
    let report_id = validation
        .report_id
        .clone()
        .expect("persisted validation should include a report id");

    let report_response = harness
        .app
        .clone()
        .oneshot(authed_empty_request(
            Method::GET,
            &format!("/api/v1/sos/validation-reports/{report_id}"),
            token,
        ))
        .await
        .expect("validation report lookup should succeed");
    let report: ValidationReportResponse =
        assert_json_response(report_response, StatusCode::OK).await;

    assert_eq!(report.subject_type, "contract");
    assert_eq!(report.subject_key, format!("contract:{CONTRACT_ID}"));
    assert!(report
        .contract_refs
        .contains(&format!("contract:{CONTRACT_ID}")));
    assert!(report
        .contract_refs
        .contains(&format!("contract:{CONTRACT_ID}@1")));
    assert!(report
        .schema_hashes
        .contains_key(&format!("contract:{CONTRACT_ID}")));
    assert!(report
        .schema_hashes
        .contains_key(&format!("contract:{CONTRACT_ID}@1")));

    let used_objects = subject_objects(
        &harness.rdf_store,
        VALIDATION_GRAPH,
        &validation_activity_uri(&validation.validation_id),
        PROV_USED,
    );
    assert!(used_objects.contains(&rdf_uri_object(&contract_uri(CONTRACT_ID))));
    assert!(used_objects.contains(&rdf_uri_object(&contract_revision_uri(CONTRACT_ID, 1))));
}

#[tokio::test]
#[serial]
async fn build_router_contract_governance_policies_gate_approval_and_signing() {
    let _guard = EnvGuard::set(&[(
        "GRAPHICA_SOS_CONTRACT_SIGNING_KEY_HEX",
        "ffeeddccbbaa99887766554433221100ffeeddccbbaa99887766554433221100",
    )]);
    let harness = setup_authenticated_build_router_app();
    let token = harness
        .token
        .as_deref()
        .expect("authenticated harness should expose a token");
    seed_contract_interfaces_only(&harness);

    let contract_id = "contract-policy-gated-http";
    create_contract(&harness, token, contract_id).await;
    create_contract_policy(
        &harness,
        token,
        "contract-approval-gate",
        contract_id,
        "contract_approval",
        "ASK { FILTER({{contract_approval_evidence_count}} > 1) }",
    )
    .await;

    let approval_request = create_contract_approval_request(&harness, token, contract_id).await;
    let validation_response = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            "/api/v1/sos/validate",
            token,
            json!({
                "type": "interface_compatibility",
                "provider_interface_id": "provider-if",
                "consumer_interface_id": "consumer-if",
            }),
        ))
        .await
        .expect("contract validation for gated approval should succeed");
    let validation: ValidationResponse =
        assert_json_response(validation_response, StatusCode::OK).await;
    let report_id = validation
        .report_id
        .clone()
        .expect("persisted validation should include a report id");
    add_contract_approval_evidence(
        &harness,
        token,
        contract_id,
        &approval_request.request_id,
        &report_id,
    )
    .await;

    let blocked_approval = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            &format!(
                "/api/v1/sos/contracts/{contract_id}/approval-requests/{}/approve",
                approval_request.request_id
            ),
            token,
            json!({
                "approved_by": "reviewer-1",
            }),
        ))
        .await
        .expect("blocked contract approval request should complete");
    assert_status(blocked_approval, StatusCode::BAD_REQUEST).await;

    delete_policy(&harness, token, "contract-approval-gate").await;

    let approved_request = approve_contract_approval_request(
        &harness,
        token,
        contract_id,
        &approval_request.request_id,
    )
    .await;
    assert_eq!(approved_request.status, "approved");

    create_contract_policy(
        &harness,
        token,
        "contract-signing-gate",
        contract_id,
        "contract_signing",
        "ASK { FILTER({{contract_approval_evidence_count}} > 1) }",
    )
    .await;

    let blocked_sign = harness
        .app
        .clone()
        .oneshot(authed_empty_request(
            Method::POST,
            &format!("/api/v1/sos/contracts/{contract_id}/sign"),
            token,
        ))
        .await
        .expect("blocked contract sign request should complete");
    assert_status(blocked_sign, StatusCode::BAD_REQUEST).await;

    delete_policy(&harness, token, "contract-signing-gate").await;

    let signed = sign_contract(&harness, token, contract_id).await;
    assert!(signed.signed);
    assert_eq!(signed.lifecycle_state, "signed");
    assert!(
        signed
            .signature
            .as_ref()
            .expect("signed contract should include attestation")
            .signature_verified
    );
}
