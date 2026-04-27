use super::*;
use crate::api::sos_validation::contract_governance::{
    effective_contract_approval_status, effective_contract_lifecycle_state,
    set_contract_approval_pending, CONTRACT_APPROVAL_APPROVED, CONTRACT_LIFECYCLE_APPROVED,
    CONTRACT_LIFECYCLE_SIGNED,
};
use crate::api::sos_validation::storage::{
    ContractApprovalEvidenceRecord, ContractApprovalRequestRecord,
};
use crate::api::sos_validation::types::{
    AddSosContractApprovalEvidenceRequest, ApproveSosContractApprovalRequest,
    CreateSosContractApprovalRequest, ListContractApprovalRequestsQuery,
    ListContractApprovalRequestsResponse, RejectSosContractApprovalRequest,
    SosContractApprovalEvidenceResponse, SosContractApprovalRequestResponse,
};

const CONTRACT_APPROVAL_REQUEST_TYPE_APPROVAL: &str = "contract_approval";
const CONTRACT_APPROVAL_REQUEST_PENDING: &str = "pending";
const CONTRACT_APPROVAL_REQUEST_APPROVED: &str = "approved";
const CONTRACT_APPROVAL_REQUEST_REJECTED: &str = "rejected";
const CONTRACT_APPROVAL_REQUEST_EXPIRED: &str = "expired";
const CONTRACT_APPROVAL_REQUEST_CANCELLED: &str = "cancelled";
const CONTRACT_APPROVAL_EVIDENCE_TYPE_VALIDATION_REPORT: &str = "validation_report";

pub(super) fn create_contract_approval_request(
    service: &SosValidationService,
    contract_id: &str,
    request: CreateSosContractApprovalRequest,
) -> Result<SosContractApprovalRequestResponse, SosValidationServiceError> {
    let mut contract = service
        .storage_manager
        .get_contract(contract_id)
        .map_err(map_storage_error)?
        .ok_or_else(|| {
            SosValidationServiceError::NotFound(format!("Contract '{}' not found", contract_id))
        })?;

    if effective_contract_lifecycle_state(&contract) == CONTRACT_LIFECYCLE_SIGNED {
        return Err(SosValidationServiceError::InvalidRequest(format!(
            "Signed contract '{}' cannot open a new approval request",
            contract.contract_id
        )));
    }

    if effective_contract_approval_status(&contract) == CONTRACT_APPROVAL_APPROVED {
        return Err(SosValidationServiceError::InvalidRequest(format!(
            "Contract '{}' revision {} is already approved",
            contract.contract_id, contract.revision
        )));
    }

    let requested_by = normalize_contract_actor("requested_by", Some(request.requested_by))?;
    let requested_lifecycle_state =
        normalize_contract_requested_lifecycle(&request.lifecycle_state)?;
    let existing_pending = service
        .storage_manager
        .list_contract_approval_requests(contract_id, usize::MAX)
        .map_err(map_storage_error)?
        .into_iter()
        .find(|existing| {
            existing.contract_revision == contract.revision
                && effective_contract_approval_request_status(existing)
                    == CONTRACT_APPROVAL_REQUEST_PENDING
        });
    if let Some(existing_pending) = existing_pending {
        return Err(SosValidationServiceError::InvalidRequest(format!(
            "Contract '{}' revision {} already has pending approval request '{}'",
            contract.contract_id, contract.revision, existing_pending.request_id
        )));
    }

    let requested_at = Utc::now();
    let note = request
        .note
        .map(|value| normalize_non_empty("note", value))
        .transpose()?;
    let approval_request = ContractApprovalRequestRecord {
        request_id: Uuid::new_v4().to_string(),
        contract_id: contract.contract_id.clone(),
        contract_revision: contract.revision,
        approval_type: CONTRACT_APPROVAL_REQUEST_TYPE_APPROVAL.to_string(),
        requested_lifecycle_state,
        status: CONTRACT_APPROVAL_REQUEST_PENDING.to_string(),
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

    contract.updated_by = requested_by.clone();
    contract.updated_at = requested_at;
    set_contract_approval_pending(&mut contract, &requested_by, requested_at);

    service
        .storage_manager
        .put_contract(&contract)
        .map_err(map_storage_error)?;
    service
        .storage_manager
        .put_contract_approval_request(&approval_request)
        .map_err(map_storage_error)?;
    projection::project_contract_upsert(service, &contract)?;
    projection::project_contract_approval_request_upsert(service, &approval_request)?;

    to_contract_approval_request_response(service, approval_request)
}

pub(super) fn list_contract_approval_requests(
    service: &SosValidationService,
    contract_id: &str,
    query: ListContractApprovalRequestsQuery,
) -> Result<ListContractApprovalRequestsResponse, SosValidationServiceError> {
    service
        .storage_manager
        .get_contract(contract_id)
        .map_err(map_storage_error)?
        .ok_or_else(|| {
            SosValidationServiceError::NotFound(format!("Contract '{}' not found", contract_id))
        })?;

    let mut requests = service
        .storage_manager
        .list_contract_approval_requests(contract_id, usize::MAX)
        .map_err(map_storage_error)?;

    if let Some(status) = query.status.as_deref() {
        let status = normalize_contract_approval_request_status(status)?;
        requests.retain(|request| effective_contract_approval_request_status(request) == status);
    }

    requests.sort_by(|left, right| right.requested_at.cmp(&left.requested_at));
    let total = requests.len();
    let requests = requests
        .into_iter()
        .skip(query.offset)
        .take(query.limit)
        .map(|request| to_contract_approval_request_response(service, request))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(ListContractApprovalRequestsResponse {
        requests,
        total,
        offset: query.offset,
        limit: query.limit,
    })
}

pub(super) fn get_contract_approval_request(
    service: &SosValidationService,
    contract_id: &str,
    request_id: &str,
) -> Result<SosContractApprovalRequestResponse, SosValidationServiceError> {
    let request = get_contract_approval_request_record(service, contract_id, request_id)?;
    to_contract_approval_request_response(service, request)
}

pub(super) fn add_contract_approval_evidence(
    service: &SosValidationService,
    contract_id: &str,
    request_id: &str,
    request: AddSosContractApprovalEvidenceRequest,
) -> Result<SosContractApprovalEvidenceResponse, SosValidationServiceError> {
    let approval_request = get_contract_approval_request_record(service, contract_id, request_id)?;
    ensure_contract_approval_request_pending(&approval_request)?;
    ensure_contract_request_revision_is_current(service, &approval_request)?;

    let added_by = normalize_contract_actor("added_by", Some(request.added_by))?;
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
    ensure_validation_report_matches_contract_revision(&approval_request, &report)?;

    let existing = service
        .storage_manager
        .list_contract_approval_evidence(request_id)
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

    let evidence = ContractApprovalEvidenceRecord {
        evidence_id: Uuid::new_v4().to_string(),
        request_id: approval_request.request_id.clone(),
        contract_id: approval_request.contract_id.clone(),
        contract_revision: approval_request.contract_revision,
        evidence_type: CONTRACT_APPROVAL_EVIDENCE_TYPE_VALIDATION_REPORT.to_string(),
        report_id: request.report_id,
        added_by,
        added_at: Utc::now(),
        note,
        metadata: request.metadata,
    };

    service
        .storage_manager
        .put_contract_approval_evidence(&evidence)
        .map_err(map_storage_error)?;
    projection::project_contract_approval_evidence_upsert(service, &evidence)?;

    Ok(to_contract_approval_evidence_response(evidence))
}

pub(super) fn approve_contract_approval_request(
    service: &SosValidationService,
    contract_id: &str,
    request_id: &str,
    request: ApproveSosContractApprovalRequest,
) -> Result<SosContractApprovalRequestResponse, SosValidationServiceError> {
    let mut approval_request =
        get_contract_approval_request_record(service, contract_id, request_id)?;
    ensure_contract_approval_request_pending(&approval_request)?;
    let approved_by = normalize_contract_actor("approved_by", Some(request.approved_by))?;

    let evidence = service
        .storage_manager
        .list_contract_approval_evidence(&approval_request.request_id)
        .map_err(map_storage_error)?;
    if evidence.is_empty() {
        return Err(SosValidationServiceError::InvalidRequest(format!(
            "Contract approval request '{}' requires at least one evidence record before approval",
            approval_request.request_id
        )));
    }

    let contract = service
        .storage_manager
        .get_contract(contract_id)
        .map_err(map_storage_error)?
        .ok_or_else(|| {
            SosValidationServiceError::NotFound(format!("Contract '{}' not found", contract_id))
        })?;
    if contract.revision != approval_request.contract_revision {
        cancel_superseded_request(
            service,
            &mut approval_request,
            format!(
                "Contract '{}' has advanced to revision {}",
                contract.contract_id, contract.revision
            ),
        )?;
        return Err(SosValidationServiceError::InvalidRequest(format!(
            "Contract approval request '{}' targets superseded revision {}",
            approval_request.request_id, approval_request.contract_revision
        )));
    }

    let policy_refs = service.evaluate_contract_governance_policies(
        super::POLICY_STAGE_CONTRACT_APPROVAL,
        &contract,
        Some(&approval_request),
        &evidence,
        HashMap::from([(
            "contract_approval_actor".to_string(),
            Value::String(approved_by.clone()),
        )]),
    )?;
    let approved_contract = service.approve_contract_revision(contract, approved_by.clone())?;
    let approved_at = Utc::now();
    approval_request.status = CONTRACT_APPROVAL_REQUEST_APPROVED.to_string();
    approval_request.approved_by = Some(approved_by);
    approval_request.approved_at = Some(approved_at);
    approval_request.rejected_by = None;
    approval_request.rejected_at = None;
    approval_request.rejection_reason = None;
    if !policy_refs.is_empty() {
        approval_request.metadata.insert(
            "policy_refs".to_string(),
            Value::Array(policy_refs.into_iter().map(Value::String).collect()),
        );
    }

    service
        .storage_manager
        .put_contract(&approved_contract)
        .map_err(map_storage_error)?;
    service
        .storage_manager
        .put_contract_approval_request(&approval_request)
        .map_err(map_storage_error)?;
    projection::project_contract_upsert(service, &approved_contract)?;
    projection::project_contract_approval_request_upsert(service, &approval_request)?;

    to_contract_approval_request_response(service, approval_request)
}

pub(super) fn reject_contract_approval_request(
    service: &SosValidationService,
    contract_id: &str,
    request_id: &str,
    request: RejectSosContractApprovalRequest,
) -> Result<SosContractApprovalRequestResponse, SosValidationServiceError> {
    let mut approval_request =
        get_contract_approval_request_record(service, contract_id, request_id)?;
    ensure_contract_approval_request_pending(&approval_request)?;

    let rejected_by = normalize_contract_actor("rejected_by", Some(request.rejected_by))?;
    let reason = normalize_non_empty("reason", request.reason)?;

    let contract = service
        .storage_manager
        .get_contract(contract_id)
        .map_err(map_storage_error)?
        .ok_or_else(|| {
            SosValidationServiceError::NotFound(format!("Contract '{}' not found", contract_id))
        })?;
    if contract.revision != approval_request.contract_revision {
        cancel_superseded_request(
            service,
            &mut approval_request,
            format!(
                "Contract '{}' has advanced to revision {}",
                contract.contract_id, contract.revision
            ),
        )?;
        return Err(SosValidationServiceError::InvalidRequest(format!(
            "Contract approval request '{}' targets superseded revision {}",
            approval_request.request_id, approval_request.contract_revision
        )));
    }

    let rejected_contract =
        service.reject_contract_revision(contract, rejected_by.clone(), reason.clone())?;
    let rejected_at = Utc::now();
    approval_request.status = CONTRACT_APPROVAL_REQUEST_REJECTED.to_string();
    approval_request.approved_by = None;
    approval_request.approved_at = None;
    approval_request.rejected_by = Some(rejected_by);
    approval_request.rejected_at = Some(rejected_at);
    approval_request.rejection_reason = Some(reason);

    service
        .storage_manager
        .put_contract(&rejected_contract)
        .map_err(map_storage_error)?;
    service
        .storage_manager
        .put_contract_approval_request(&approval_request)
        .map_err(map_storage_error)?;
    projection::project_contract_upsert(service, &rejected_contract)?;
    projection::project_contract_approval_request_upsert(service, &approval_request)?;

    to_contract_approval_request_response(service, approval_request)
}

pub(super) fn approve_contract_via_legacy_route(
    service: &SosValidationService,
    contract_id: &str,
    approved_by: &str,
) -> Result<Contract, SosValidationServiceError> {
    let contract = lookup::get_contract(service, contract_id)?;
    if effective_contract_approval_status(&contract) == CONTRACT_APPROVAL_APPROVED {
        return Ok(contract);
    }

    let request_id = select_pending_request_for_legacy_contract_approval(service, contract_id)?;
    approve_contract_approval_request(
        service,
        contract_id,
        &request_id,
        ApproveSosContractApprovalRequest {
            approved_by: approved_by.to_string(),
        },
    )?;

    lookup::get_contract(service, contract_id)
}

fn get_contract_approval_request_record(
    service: &SosValidationService,
    contract_id: &str,
    request_id: &str,
) -> Result<ContractApprovalRequestRecord, SosValidationServiceError> {
    let request = service
        .storage_manager
        .get_contract_approval_request(request_id)
        .map_err(map_storage_error)?
        .ok_or_else(|| {
            SosValidationServiceError::NotFound(format!(
                "Contract approval request '{}' not found",
                request_id
            ))
        })?;

    if request.contract_id != contract_id {
        return Err(SosValidationServiceError::InvalidRequest(format!(
            "Contract approval request '{}' does not belong to contract '{}'",
            request_id, contract_id
        )));
    }

    Ok(request)
}

fn to_contract_approval_request_response(
    service: &SosValidationService,
    request: ContractApprovalRequestRecord,
) -> Result<SosContractApprovalRequestResponse, SosValidationServiceError> {
    let status = effective_contract_approval_request_status(&request).to_string();
    let evidence = service
        .storage_manager
        .list_contract_approval_evidence(&request.request_id)
        .map_err(map_storage_error)?
        .into_iter()
        .map(to_contract_approval_evidence_response)
        .collect();

    Ok(SosContractApprovalRequestResponse {
        request_id: request.request_id,
        contract_id: request.contract_id,
        contract_revision: request.contract_revision,
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

fn to_contract_approval_evidence_response(
    evidence: ContractApprovalEvidenceRecord,
) -> SosContractApprovalEvidenceResponse {
    SosContractApprovalEvidenceResponse {
        evidence_id: evidence.evidence_id,
        request_id: evidence.request_id,
        contract_id: evidence.contract_id,
        contract_revision: evidence.contract_revision,
        evidence_type: evidence.evidence_type,
        report_id: evidence.report_id,
        added_by: evidence.added_by,
        added_at: evidence.added_at.to_rfc3339(),
        note: evidence.note,
        metadata: evidence.metadata,
    }
}

fn normalize_contract_requested_lifecycle(raw: &str) -> Result<String, SosValidationServiceError> {
    let normalized = raw.trim().to_ascii_lowercase();
    match normalized.as_str() {
        CONTRACT_LIFECYCLE_APPROVED => Ok(normalized),
        _ => Err(SosValidationServiceError::InvalidRequest(format!(
            "Contract approval requests must target lifecycle_state '{}'",
            CONTRACT_LIFECYCLE_APPROVED
        ))),
    }
}

fn normalize_contract_approval_request_status(
    raw: &str,
) -> Result<String, SosValidationServiceError> {
    let normalized = raw.trim().to_ascii_lowercase();
    match normalized.as_str() {
        CONTRACT_APPROVAL_REQUEST_PENDING
        | CONTRACT_APPROVAL_REQUEST_APPROVED
        | CONTRACT_APPROVAL_REQUEST_REJECTED
        | CONTRACT_APPROVAL_REQUEST_EXPIRED
        | CONTRACT_APPROVAL_REQUEST_CANCELLED => Ok(normalized),
        _ => Err(SosValidationServiceError::InvalidRequest(format!(
            "Unsupported contract approval request status '{}'",
            raw
        ))),
    }
}

fn normalize_contract_actor(
    field: &str,
    actor: Option<String>,
) -> Result<String, SosValidationServiceError> {
    let actor = actor.unwrap_or_else(|| "system".to_string());
    normalize_non_empty(field, actor)
}

fn effective_contract_approval_request_status(request: &ContractApprovalRequestRecord) -> &str {
    if request.status == CONTRACT_APPROVAL_REQUEST_PENDING {
        if let Some(expires_at) = request.expires_at.as_ref() {
            if *expires_at < Utc::now() {
                return CONTRACT_APPROVAL_REQUEST_EXPIRED;
            }
        }
    }

    match request.status.as_str() {
        CONTRACT_APPROVAL_REQUEST_PENDING
        | CONTRACT_APPROVAL_REQUEST_APPROVED
        | CONTRACT_APPROVAL_REQUEST_REJECTED
        | CONTRACT_APPROVAL_REQUEST_EXPIRED
        | CONTRACT_APPROVAL_REQUEST_CANCELLED => request.status.as_str(),
        _ => CONTRACT_APPROVAL_REQUEST_PENDING,
    }
}

fn ensure_contract_approval_request_pending(
    request: &ContractApprovalRequestRecord,
) -> Result<(), SosValidationServiceError> {
    match effective_contract_approval_request_status(request) {
        CONTRACT_APPROVAL_REQUEST_PENDING => Ok(()),
        CONTRACT_APPROVAL_REQUEST_EXPIRED => {
            Err(SosValidationServiceError::InvalidRequest(format!(
                "Contract approval request '{}' has expired",
                request.request_id
            )))
        }
        other => Err(SosValidationServiceError::InvalidRequest(format!(
            "Contract approval request '{}' is already in terminal status '{}'",
            request.request_id, other
        ))),
    }
}

fn ensure_contract_request_revision_is_current(
    service: &SosValidationService,
    request: &ContractApprovalRequestRecord,
) -> Result<(), SosValidationServiceError> {
    let contract = service
        .storage_manager
        .get_contract(&request.contract_id)
        .map_err(map_storage_error)?
        .ok_or_else(|| {
            SosValidationServiceError::NotFound(format!(
                "Contract '{}' not found",
                request.contract_id
            ))
        })?;

    if contract.revision != request.contract_revision {
        return Err(SosValidationServiceError::InvalidRequest(format!(
            "Contract approval request '{}' targets superseded revision {}",
            request.request_id, request.contract_revision
        )));
    }

    Ok(())
}

fn ensure_validation_report_matches_contract_revision(
    request: &ContractApprovalRequestRecord,
    report: &ValidationReport,
) -> Result<(), SosValidationServiceError> {
    if !report.passed {
        return Err(SosValidationServiceError::InvalidRequest(format!(
            "Validation report '{}' did not pass and cannot be used as approval evidence",
            report.report_id
        )));
    }

    let revision_ref = format!(
        "contract:{}@{}",
        request.contract_id, request.contract_revision
    );
    if report
        .contract_refs
        .iter()
        .any(|contract_ref| contract_ref == &revision_ref)
    {
        return Ok(());
    }

    Err(SosValidationServiceError::InvalidRequest(format!(
        "Validation report '{}' does not reference contract '{}' revision {}",
        report.report_id, request.contract_id, request.contract_revision
    )))
}

fn select_pending_request_for_legacy_contract_approval(
    service: &SosValidationService,
    contract_id: &str,
) -> Result<String, SosValidationServiceError> {
    let contract = service
        .storage_manager
        .get_contract(contract_id)
        .map_err(map_storage_error)?
        .ok_or_else(|| {
            SosValidationServiceError::NotFound(format!("Contract '{}' not found", contract_id))
        })?;

    let mut matching = service
        .storage_manager
        .list_contract_approval_requests(contract_id, usize::MAX)
        .map_err(map_storage_error)?
        .into_iter()
        .filter(|request| {
            request.contract_revision == contract.revision
                && effective_contract_approval_request_status(request)
                    == CONTRACT_APPROVAL_REQUEST_PENDING
        });

    let Some(request) = matching.next() else {
        return Err(SosValidationServiceError::InvalidRequest(format!(
            "Contract '{}' requires a pending approval request before approval",
            contract_id
        )));
    };
    if matching.next().is_some() {
        return Err(SosValidationServiceError::InvalidRequest(format!(
            "Contract '{}' has multiple pending approval requests; use /approval-requests/{{request_id}}/approve",
            contract_id
        )));
    }

    Ok(request.request_id)
}

fn cancel_superseded_request(
    service: &SosValidationService,
    request: &mut ContractApprovalRequestRecord,
    reason: String,
) -> Result<(), SosValidationServiceError> {
    request.status = CONTRACT_APPROVAL_REQUEST_CANCELLED.to_string();
    request
        .metadata
        .insert("cancel_reason".to_string(), Value::String(reason));
    service
        .storage_manager
        .put_contract_approval_request(request)
        .map_err(map_storage_error)?;
    projection::project_contract_approval_request_upsert(service, request)
}
