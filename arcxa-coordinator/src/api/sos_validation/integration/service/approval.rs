use super::*;
use crate::api::sos_validation::policy_attestation::{
    build_policy_attestation_record, PolicyAttestationSigningMaterial,
};
use crate::api::sos_validation::storage::{
    PolicyApprovalEvidenceRecord, PolicyApprovalRequestRecord,
};
use crate::api::sos_validation::types::{
    AddSosPolicyApprovalEvidenceRequest, ApproveSosPolicyApprovalRequest,
    CreateSosPolicyApprovalRequest, ListPolicyApprovalRequestsQuery,
    ListPolicyApprovalRequestsResponse, RejectSosPolicyApprovalRequest,
    SosPolicyApprovalEvidenceResponse, SosPolicyApprovalRequestResponse,
};

const POLICY_APPROVAL_REQUEST_TYPE_ROLLOUT: &str = "policy_rollout";
const POLICY_APPROVAL_REQUEST_PENDING: &str = "pending";
const POLICY_APPROVAL_REQUEST_APPROVED: &str = "approved";
const POLICY_APPROVAL_REQUEST_REJECTED: &str = "rejected";
const POLICY_APPROVAL_REQUEST_EXPIRED: &str = "expired";
const POLICY_APPROVAL_REQUEST_CANCELLED: &str = "cancelled";
const POLICY_APPROVAL_EVIDENCE_TYPE_VALIDATION_REPORT: &str = "validation_report";

pub(super) fn create_policy_approval_request(
    service: &SosValidationService,
    policy_id: &str,
    request: CreateSosPolicyApprovalRequest,
) -> Result<SosPolicyApprovalRequestResponse, SosValidationServiceError> {
    let mut policy = service
        .storage_manager
        .get_policy(policy_id)
        .map_err(map_storage_error)?
        .ok_or_else(|| {
            SosValidationServiceError::NotFound(format!("Policy '{}' not found", policy_id))
        })?;

    if effective_policy_approval_status(&policy) == POLICY_APPROVAL_APPROVED {
        return Err(SosValidationServiceError::InvalidRequest(format!(
            "Policy '{}' revision {} is already approved",
            policy.policy_id, policy.revision
        )));
    }

    let requested_by = normalize_policy_actor("requested_by", Some(request.requested_by))?;
    let requested_lifecycle_state = normalize_policy_lifecycle_state(&request.lifecycle_state)?;
    if !policy_state_is_automatic(&requested_lifecycle_state) {
        return Err(SosValidationServiceError::InvalidRequest(format!(
            "Policy approval requests must target an automatic lifecycle_state, not '{}'",
            requested_lifecycle_state
        )));
    }

    let existing_pending = service
        .storage_manager
        .list_policy_approval_requests(policy_id, usize::MAX)
        .map_err(map_storage_error)?
        .into_iter()
        .find(|existing| {
            existing.policy_revision == policy.revision
                && effective_policy_approval_request_status(existing)
                    == POLICY_APPROVAL_REQUEST_PENDING
        });
    if let Some(existing_pending) = existing_pending {
        return Err(SosValidationServiceError::InvalidRequest(format!(
            "Policy '{}' revision {} already has pending approval request '{}'",
            policy.policy_id, policy.revision, existing_pending.request_id
        )));
    }

    let requested_at = Utc::now();
    let note = request
        .note
        .map(|value| normalize_non_empty("note", value))
        .transpose()?;
    let approval_request = PolicyApprovalRequestRecord {
        request_id: Uuid::new_v4().to_string(),
        policy_id: policy.policy_id.clone(),
        policy_revision: policy.revision,
        approval_type: POLICY_APPROVAL_REQUEST_TYPE_ROLLOUT.to_string(),
        requested_lifecycle_state,
        status: POLICY_APPROVAL_REQUEST_PENDING.to_string(),
        note,
        requested_by: requested_by.clone(),
        requested_at,
        expires_at: request
            .expires_in_seconds
            .map(|seconds| requested_at + chrono::Duration::seconds(seconds as i64)),
        approved_by: None,
        approved_at: None,
        rejected_by: None,
        rejected_at: None,
        rejection_reason: None,
        metadata: request.metadata,
    };

    policy.updated_by = requested_by.clone();
    policy.updated_at = requested_at;
    set_policy_approval_pending(&mut policy, &requested_by, requested_at);

    service
        .storage_manager
        .put_policy(&policy)
        .map_err(map_storage_error)?;
    service
        .storage_manager
        .put_policy_approval_request(&approval_request)
        .map_err(map_storage_error)?;
    projection::project_policy_upsert(service, &policy)?;
    projection::project_policy_approval_request_upsert(service, &approval_request)?;

    to_policy_approval_request_response(service, approval_request)
}

pub(super) fn list_policy_approval_requests(
    service: &SosValidationService,
    policy_id: &str,
    query: ListPolicyApprovalRequestsQuery,
) -> Result<ListPolicyApprovalRequestsResponse, SosValidationServiceError> {
    service
        .storage_manager
        .get_policy(policy_id)
        .map_err(map_storage_error)?
        .ok_or_else(|| {
            SosValidationServiceError::NotFound(format!("Policy '{}' not found", policy_id))
        })?;

    let mut requests = service
        .storage_manager
        .list_policy_approval_requests(policy_id, usize::MAX)
        .map_err(map_storage_error)?;

    if let Some(status) = query.status.as_deref() {
        let status = normalize_policy_approval_request_status(status)?;
        requests.retain(|request| effective_policy_approval_request_status(request) == status);
    }

    requests.sort_by(|left, right| right.requested_at.cmp(&left.requested_at));
    let total = requests.len();
    let requests = requests
        .into_iter()
        .skip(query.offset)
        .take(query.limit)
        .map(|request| to_policy_approval_request_response(service, request))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(ListPolicyApprovalRequestsResponse {
        requests,
        total,
        offset: query.offset,
        limit: query.limit,
    })
}

pub(super) fn get_policy_approval_request(
    service: &SosValidationService,
    policy_id: &str,
    request_id: &str,
) -> Result<SosPolicyApprovalRequestResponse, SosValidationServiceError> {
    let request = get_policy_approval_request_record(service, policy_id, request_id)?;
    to_policy_approval_request_response(service, request)
}

pub(super) fn add_policy_approval_evidence(
    service: &SosValidationService,
    policy_id: &str,
    request_id: &str,
    request: AddSosPolicyApprovalEvidenceRequest,
) -> Result<SosPolicyApprovalEvidenceResponse, SosValidationServiceError> {
    let approval_request = get_policy_approval_request_record(service, policy_id, request_id)?;
    ensure_policy_approval_request_pending(&approval_request)?;
    ensure_request_revision_is_current(service, &approval_request)?;

    let added_by = normalize_policy_actor("added_by", Some(request.added_by))?;
    let note = request
        .note
        .map(|value| normalize_non_empty("note", value))
        .transpose()?;

    let report = service
        .storage_manager
        .get_validation_report(&request.report_id)
        .map_err(map_storage_error)?
        .ok_or_else(|| {
            SosValidationServiceError::NotFound(format!(
                "Validation report '{}' not found",
                request.report_id
            ))
        })?;
    ensure_validation_report_matches_policy_revision(&approval_request, &report)?;

    let existing = service
        .storage_manager
        .list_policy_approval_evidence(request_id)
        .map_err(map_storage_error)?;
    if existing
        .iter()
        .any(|evidence| evidence.report_id == request.report_id)
    {
        return Err(SosValidationServiceError::InvalidRequest(format!(
            "Validation report '{}' is already attached to approval request '{}'",
            request.report_id, request_id
        )));
    }

    let evidence = PolicyApprovalEvidenceRecord {
        evidence_id: Uuid::new_v4().to_string(),
        request_id: approval_request.request_id.clone(),
        policy_id: approval_request.policy_id.clone(),
        policy_revision: approval_request.policy_revision,
        evidence_type: POLICY_APPROVAL_EVIDENCE_TYPE_VALIDATION_REPORT.to_string(),
        report_id: request.report_id,
        added_by,
        added_at: Utc::now(),
        note,
        metadata: request.metadata,
    };

    service
        .storage_manager
        .put_policy_approval_evidence(&evidence)
        .map_err(map_storage_error)?;
    projection::project_policy_approval_evidence_upsert(service, &evidence)?;

    Ok(to_policy_approval_evidence_response(evidence))
}

pub(super) fn approve_policy_approval_request(
    service: &SosValidationService,
    policy_id: &str,
    request_id: &str,
    request: ApproveSosPolicyApprovalRequest,
) -> Result<SosPolicyApprovalRequestResponse, SosValidationServiceError> {
    approve_policy_approval_request_internal(service, policy_id, request_id, request, None)
}

pub(super) fn approve_policy_approval_request_with_attestation(
    service: &SosValidationService,
    policy_id: &str,
    request_id: &str,
    request: ApproveSosPolicyApprovalRequest,
    signing_material: PolicyAttestationSigningMaterial,
) -> Result<SosPolicyApprovalRequestResponse, SosValidationServiceError> {
    approve_policy_approval_request_internal(
        service,
        policy_id,
        request_id,
        request,
        Some(signing_material),
    )
}

fn approve_policy_approval_request_internal(
    service: &SosValidationService,
    policy_id: &str,
    request_id: &str,
    request: ApproveSosPolicyApprovalRequest,
    signing_material: Option<PolicyAttestationSigningMaterial>,
) -> Result<SosPolicyApprovalRequestResponse, SosValidationServiceError> {
    let mut approval_request = get_policy_approval_request_record(service, policy_id, request_id)?;
    ensure_policy_approval_request_pending(&approval_request)?;
    let approved_by = normalize_policy_actor("approved_by", Some(request.approved_by))?;

    let evidence = service
        .storage_manager
        .list_policy_approval_evidence(&approval_request.request_id)
        .map_err(map_storage_error)?;
    if evidence.is_empty() {
        return Err(SosValidationServiceError::InvalidRequest(format!(
            "Policy approval request '{}' requires at least one evidence record before approval",
            approval_request.request_id
        )));
    }

    let policy = service
        .storage_manager
        .get_policy(policy_id)
        .map_err(map_storage_error)?
        .ok_or_else(|| {
            SosValidationServiceError::NotFound(format!("Policy '{}' not found", policy_id))
        })?;
    if policy.revision != approval_request.policy_revision {
        cancel_superseded_request(
            service,
            &mut approval_request,
            format!(
                "Policy '{}' has advanced to revision {}",
                policy.policy_id, policy.revision
            ),
        )?;
        return Err(SosValidationServiceError::InvalidRequest(format!(
            "Policy approval request '{}' targets superseded revision {}",
            approval_request.request_id, approval_request.policy_revision
        )));
    }

    let approved_policy = service.approve_policy_revision(
        policy,
        approved_by.clone(),
        Some(&approval_request.requested_lifecycle_state),
    )?;
    let approved_at = Utc::now();
    approval_request.status = POLICY_APPROVAL_REQUEST_APPROVED.to_string();
    approval_request.approved_by = Some(approved_by);
    approval_request.approved_at = Some(approved_at);
    approval_request.rejected_by = None;
    approval_request.rejected_at = None;
    approval_request.rejection_reason = None;

    service
        .storage_manager
        .put_policy(&approved_policy)
        .map_err(map_storage_error)?;
    service
        .storage_manager
        .put_policy_approval_request(&approval_request)
        .map_err(map_storage_error)?;
    projection::project_policy_upsert(service, &approved_policy)?;
    projection::project_policy_approval_request_upsert(service, &approval_request)?;

    if let Some(signing_material) = signing_material {
        let policy_refs = approval_request
            .metadata
            .get("policy_refs")
            .and_then(|value| value.as_array())
            .map(|values| {
                values
                    .iter()
                    .filter_map(|value| value.as_str().map(ToString::to_string))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let attestation = build_policy_attestation_record(
            &approved_policy,
            &signing_material,
            Some(approval_request.request_id.clone()),
            evidence
                .iter()
                .map(|record| record.evidence_id.clone())
                .collect(),
            policy_refs,
        )
        .map_err(|error| {
            SosValidationServiceError::Internal(format!(
                "Failed to build policy attestation: {}",
                error
            ))
        })?;
        service
            .storage_manager
            .put_policy_attestation(&attestation)
            .map_err(map_storage_error)?;
        projection::project_policy_attestation_upsert(service, &approved_policy, &attestation)?;
    }

    to_policy_approval_request_response(service, approval_request)
}

pub(super) fn reject_policy_approval_request(
    service: &SosValidationService,
    policy_id: &str,
    request_id: &str,
    request: RejectSosPolicyApprovalRequest,
) -> Result<SosPolicyApprovalRequestResponse, SosValidationServiceError> {
    let mut approval_request = get_policy_approval_request_record(service, policy_id, request_id)?;
    ensure_policy_approval_request_pending(&approval_request)?;

    let rejected_by = normalize_policy_actor("rejected_by", Some(request.rejected_by))?;
    let reason = normalize_non_empty("reason", request.reason)?;

    let policy = service
        .storage_manager
        .get_policy(policy_id)
        .map_err(map_storage_error)?
        .ok_or_else(|| {
            SosValidationServiceError::NotFound(format!("Policy '{}' not found", policy_id))
        })?;
    if policy.revision != approval_request.policy_revision {
        cancel_superseded_request(
            service,
            &mut approval_request,
            format!(
                "Policy '{}' has advanced to revision {}",
                policy.policy_id, policy.revision
            ),
        )?;
        return Err(SosValidationServiceError::InvalidRequest(format!(
            "Policy approval request '{}' targets superseded revision {}",
            approval_request.request_id, approval_request.policy_revision
        )));
    }

    let rejected_policy =
        service.reject_policy_revision(policy, rejected_by.clone(), reason.clone())?;
    let rejected_at = Utc::now();
    approval_request.status = POLICY_APPROVAL_REQUEST_REJECTED.to_string();
    approval_request.approved_by = None;
    approval_request.approved_at = None;
    approval_request.rejected_by = Some(rejected_by);
    approval_request.rejected_at = Some(rejected_at);
    approval_request.rejection_reason = Some(reason);

    service
        .storage_manager
        .put_policy(&rejected_policy)
        .map_err(map_storage_error)?;
    service
        .storage_manager
        .put_policy_approval_request(&approval_request)
        .map_err(map_storage_error)?;
    projection::project_policy_upsert(service, &rejected_policy)?;
    projection::project_policy_approval_request_upsert(service, &approval_request)?;

    to_policy_approval_request_response(service, approval_request)
}

pub(super) fn approve_policy_via_legacy_route(
    service: &SosValidationService,
    policy_id: &str,
    request: ApproveSosPolicyRequest,
) -> Result<SosPolicyResponse, SosValidationServiceError> {
    let request_id = select_pending_request_for_legacy_policy_decision(
        service,
        policy_id,
        request.request_id.as_deref(),
        request.lifecycle_state.as_deref(),
    )?;

    approve_policy_approval_request(
        service,
        policy_id,
        &request_id,
        ApproveSosPolicyApprovalRequest {
            approved_by: request.approved_by,
        },
    )?;

    service.get_policy(policy_id)
}

pub(super) fn approve_policy_via_legacy_route_with_attestation(
    service: &SosValidationService,
    policy_id: &str,
    request: ApproveSosPolicyRequest,
    signing_material: PolicyAttestationSigningMaterial,
) -> Result<SosPolicyResponse, SosValidationServiceError> {
    let request_id = select_pending_request_for_legacy_policy_decision(
        service,
        policy_id,
        request.request_id.as_deref(),
        request.lifecycle_state.as_deref(),
    )?;

    approve_policy_approval_request_with_attestation(
        service,
        policy_id,
        &request_id,
        ApproveSosPolicyApprovalRequest {
            approved_by: request.approved_by,
        },
        signing_material,
    )?;

    service.get_policy(policy_id)
}

pub(super) fn reject_policy_via_legacy_route(
    service: &SosValidationService,
    policy_id: &str,
    request: RejectSosPolicyRequest,
) -> Result<SosPolicyResponse, SosValidationServiceError> {
    let request_id = select_pending_request_for_legacy_policy_decision(
        service,
        policy_id,
        request.request_id.as_deref(),
        None,
    )?;

    reject_policy_approval_request(
        service,
        policy_id,
        &request_id,
        RejectSosPolicyApprovalRequest {
            rejected_by: request.rejected_by,
            reason: request.reason,
        },
    )?;

    service.get_policy(policy_id)
}

fn get_policy_approval_request_record(
    service: &SosValidationService,
    policy_id: &str,
    request_id: &str,
) -> Result<PolicyApprovalRequestRecord, SosValidationServiceError> {
    let request = service
        .storage_manager
        .get_policy_approval_request(request_id)
        .map_err(map_storage_error)?
        .ok_or_else(|| {
            SosValidationServiceError::NotFound(format!(
                "Policy approval request '{}' not found",
                request_id
            ))
        })?;

    if request.policy_id != policy_id {
        return Err(SosValidationServiceError::InvalidRequest(format!(
            "Policy approval request '{}' does not belong to policy '{}'",
            request_id, policy_id
        )));
    }

    Ok(request)
}

fn to_policy_approval_request_response(
    service: &SosValidationService,
    request: PolicyApprovalRequestRecord,
) -> Result<SosPolicyApprovalRequestResponse, SosValidationServiceError> {
    let status = effective_policy_approval_request_status(&request).to_string();
    let evidence = service
        .storage_manager
        .list_policy_approval_evidence(&request.request_id)
        .map_err(map_storage_error)?
        .into_iter()
        .map(to_policy_approval_evidence_response)
        .collect();

    Ok(SosPolicyApprovalRequestResponse {
        request_id: request.request_id,
        policy_revision_ref: policy_revision_ref_string(
            &request.policy_id,
            request.policy_revision,
        ),
        policy_id: request.policy_id,
        policy_revision: request.policy_revision,
        approval_type: request.approval_type,
        requested_lifecycle_state: request.requested_lifecycle_state,
        status,
        note: request.note,
        requested_by: request.requested_by,
        requested_at: request.requested_at.to_rfc3339(),
        expires_at: request.expires_at.map(|value| value.to_rfc3339()),
        approved_by: request.approved_by,
        approved_at: request.approved_at.map(|value| value.to_rfc3339()),
        rejected_by: request.rejected_by,
        rejected_at: request.rejected_at.map(|value| value.to_rfc3339()),
        rejection_reason: request.rejection_reason,
        metadata: request.metadata,
        evidence,
    })
}

fn to_policy_approval_evidence_response(
    evidence: PolicyApprovalEvidenceRecord,
) -> SosPolicyApprovalEvidenceResponse {
    SosPolicyApprovalEvidenceResponse {
        evidence_id: evidence.evidence_id,
        request_id: evidence.request_id,
        policy_revision_ref: policy_revision_ref_string(
            &evidence.policy_id,
            evidence.policy_revision,
        ),
        policy_id: evidence.policy_id,
        policy_revision: evidence.policy_revision,
        evidence_type: evidence.evidence_type,
        report_id: evidence.report_id,
        added_by: evidence.added_by,
        added_at: evidence.added_at.to_rfc3339(),
        note: evidence.note,
        metadata: evidence.metadata,
    }
}

fn normalize_policy_approval_request_status(
    raw: &str,
) -> Result<String, SosValidationServiceError> {
    let normalized = raw.trim().to_ascii_lowercase();
    match normalized.as_str() {
        POLICY_APPROVAL_REQUEST_PENDING
        | POLICY_APPROVAL_REQUEST_APPROVED
        | POLICY_APPROVAL_REQUEST_REJECTED
        | POLICY_APPROVAL_REQUEST_EXPIRED
        | POLICY_APPROVAL_REQUEST_CANCELLED => Ok(normalized),
        _ => Err(SosValidationServiceError::InvalidRequest(format!(
            "Unsupported policy approval request status '{}'",
            raw
        ))),
    }
}

fn effective_policy_approval_request_status(request: &PolicyApprovalRequestRecord) -> &str {
    if request.status == POLICY_APPROVAL_REQUEST_PENDING {
        if let Some(expires_at) = request.expires_at.as_ref() {
            if *expires_at < Utc::now() {
                return POLICY_APPROVAL_REQUEST_EXPIRED;
            }
        }
    }

    match request.status.as_str() {
        POLICY_APPROVAL_REQUEST_PENDING
        | POLICY_APPROVAL_REQUEST_APPROVED
        | POLICY_APPROVAL_REQUEST_REJECTED
        | POLICY_APPROVAL_REQUEST_EXPIRED
        | POLICY_APPROVAL_REQUEST_CANCELLED => request.status.as_str(),
        _ => POLICY_APPROVAL_REQUEST_PENDING,
    }
}

fn ensure_policy_approval_request_pending(
    request: &PolicyApprovalRequestRecord,
) -> Result<(), SosValidationServiceError> {
    match effective_policy_approval_request_status(request) {
        POLICY_APPROVAL_REQUEST_PENDING => Ok(()),
        POLICY_APPROVAL_REQUEST_EXPIRED => Err(SosValidationServiceError::InvalidRequest(format!(
            "Policy approval request '{}' has expired",
            request.request_id
        ))),
        other => Err(SosValidationServiceError::InvalidRequest(format!(
            "Policy approval request '{}' is already in terminal status '{}'",
            request.request_id, other
        ))),
    }
}

fn ensure_request_revision_is_current(
    service: &SosValidationService,
    request: &PolicyApprovalRequestRecord,
) -> Result<(), SosValidationServiceError> {
    let policy = service
        .storage_manager
        .get_policy(&request.policy_id)
        .map_err(map_storage_error)?
        .ok_or_else(|| {
            SosValidationServiceError::NotFound(format!("Policy '{}' not found", request.policy_id))
        })?;

    if policy.revision != request.policy_revision {
        return Err(SosValidationServiceError::InvalidRequest(format!(
            "Policy approval request '{}' targets superseded revision {}",
            request.request_id, request.policy_revision
        )));
    }

    Ok(())
}

fn ensure_validation_report_matches_policy_revision(
    request: &PolicyApprovalRequestRecord,
    report: &ValidationReport,
) -> Result<(), SosValidationServiceError> {
    if !report.passed {
        return Err(SosValidationServiceError::InvalidRequest(format!(
            "Validation report '{}' did not pass and cannot be used as rollout evidence",
            report.report_id
        )));
    }

    let revision_ref = format!("policy:{}@{}", request.policy_id, request.policy_revision);
    if report
        .policy_refs
        .iter()
        .any(|policy_ref| policy_ref == &revision_ref)
    {
        return Ok(());
    }

    Err(SosValidationServiceError::InvalidRequest(format!(
        "Validation report '{}' does not reference policy '{}' revision {}",
        report.report_id, request.policy_id, request.policy_revision
    )))
}

fn select_pending_request_for_legacy_policy_decision(
    service: &SosValidationService,
    policy_id: &str,
    request_id: Option<&str>,
    requested_lifecycle_state: Option<&str>,
) -> Result<String, SosValidationServiceError> {
    if let Some(request_id) = request_id {
        return Ok(get_policy_approval_request_record(service, policy_id, request_id)?.request_id);
    }

    let policy = service
        .storage_manager
        .get_policy(policy_id)
        .map_err(map_storage_error)?
        .ok_or_else(|| {
            SosValidationServiceError::NotFound(format!("Policy '{}' not found", policy_id))
        })?;
    let normalized_lifecycle_state = requested_lifecycle_state
        .map(normalize_policy_lifecycle_state)
        .transpose()?;

    let mut matching = service
        .storage_manager
        .list_policy_approval_requests(policy_id, usize::MAX)
        .map_err(map_storage_error)?
        .into_iter()
        .filter(|request| {
            request.policy_revision == policy.revision
                && effective_policy_approval_request_status(request)
                    == POLICY_APPROVAL_REQUEST_PENDING
        })
        .filter(|request| {
            normalized_lifecycle_state
                .as_deref()
                .map(|state| request.requested_lifecycle_state == state)
                .unwrap_or(true)
        });

    let Some(request) = matching.next() else {
        return Err(SosValidationServiceError::InvalidRequest(format!(
            "Policy '{}' requires a pending approval request before approval or rejection",
            policy_id
        )));
    };
    if matching.next().is_some() {
        return Err(SosValidationServiceError::InvalidRequest(format!(
            "Policy '{}' has multiple pending approval requests; provide request_id explicitly",
            policy_id
        )));
    }

    Ok(request.request_id)
}

fn cancel_superseded_request(
    service: &SosValidationService,
    request: &mut PolicyApprovalRequestRecord,
    reason: String,
) -> Result<(), SosValidationServiceError> {
    request.status = POLICY_APPROVAL_REQUEST_CANCELLED.to_string();
    request
        .metadata
        .insert("cancel_reason".to_string(), Value::String(reason));
    service
        .storage_manager
        .put_policy_approval_request(request)
        .map_err(map_storage_error)?;
    projection::project_policy_approval_request_upsert(service, request)
}

fn policy_revision_ref_string(policy_id: &str, revision: u32) -> String {
    format!("policy:{}@{}", policy_id, revision)
}
