use chrono::{DateTime, Utc};

use super::storage::Contract;

pub(crate) const CONTRACT_LIFECYCLE_DRAFT: &str = "draft";
pub(crate) const CONTRACT_LIFECYCLE_APPROVED: &str = "approved";
pub(crate) const CONTRACT_LIFECYCLE_SIGNED: &str = "signed";
pub(crate) const CONTRACT_APPROVAL_PENDING: &str = "pending";
pub(crate) const CONTRACT_APPROVAL_APPROVED: &str = "approved";
pub(crate) const CONTRACT_APPROVAL_REJECTED: &str = "rejected";

pub(crate) fn effective_contract_lifecycle_state(contract: &Contract) -> &str {
    contract.lifecycle_state.as_deref().unwrap_or_else(|| {
        if contract.signed {
            CONTRACT_LIFECYCLE_SIGNED
        } else if contract.approved {
            CONTRACT_LIFECYCLE_APPROVED
        } else {
            CONTRACT_LIFECYCLE_DRAFT
        }
    })
}

pub(crate) fn effective_contract_approval_status(contract: &Contract) -> &str {
    match contract.approval_status.as_deref() {
        Some(CONTRACT_APPROVAL_PENDING)
        | Some(CONTRACT_APPROVAL_APPROVED)
        | Some(CONTRACT_APPROVAL_REJECTED) => contract.approval_status.as_deref().unwrap(),
        Some(_) | None => {
            if contract.approved {
                CONTRACT_APPROVAL_APPROVED
            } else if contract.rejected_by.is_some()
                || contract.rejected_at.is_some()
                || contract.rejection_reason.is_some()
            {
                CONTRACT_APPROVAL_REJECTED
            } else {
                CONTRACT_APPROVAL_PENDING
            }
        }
    }
}

pub(crate) fn stable_contract_ref(contract_id: &str) -> String {
    format!("contract:{}", contract_id)
}

pub(crate) fn contract_revision_ref(contract: &Contract) -> String {
    format!("contract:{}@{}", contract.contract_id, contract.revision)
}

pub(crate) fn set_contract_draft(contract: &mut Contract) {
    contract.approved = false;
    contract.signed = false;
    contract.lifecycle_state = Some(CONTRACT_LIFECYCLE_DRAFT.to_string());
    contract.approval_status = Some(CONTRACT_APPROVAL_PENDING.to_string());
    contract.approval_requested_by = None;
    contract.approval_requested_at = None;
    contract.approved_by = None;
    contract.approved_at = None;
    contract.rejected_by = None;
    contract.rejected_at = None;
    contract.rejection_reason = None;
    contract.signed_by = None;
    contract.signed_at = None;
}

pub(crate) fn set_contract_approval_pending(
    contract: &mut Contract,
    actor: &str,
    at: DateTime<Utc>,
) {
    contract.lifecycle_state = Some(CONTRACT_LIFECYCLE_DRAFT.to_string());
    contract.approval_status = Some(CONTRACT_APPROVAL_PENDING.to_string());
    contract.approval_requested_by = Some(actor.to_string());
    contract.approval_requested_at = Some(at);
    contract.approved = false;
    contract.signed = false;
    contract.approved_by = None;
    contract.approved_at = None;
    contract.rejected_by = None;
    contract.rejected_at = None;
    contract.rejection_reason = None;
    contract.signed_by = None;
    contract.signed_at = None;
}

pub(crate) fn set_contract_approved(contract: &mut Contract, actor: &str, at: DateTime<Utc>) {
    contract.approved = true;
    contract.signed = false;
    if effective_contract_lifecycle_state(contract) != CONTRACT_LIFECYCLE_SIGNED {
        contract.lifecycle_state = Some(CONTRACT_LIFECYCLE_APPROVED.to_string());
    }
    contract.approval_status = Some(CONTRACT_APPROVAL_APPROVED.to_string());
    contract.approved_by = Some(actor.to_string());
    contract.approved_at = Some(at);
    contract.rejected_by = None;
    contract.rejected_at = None;
    contract.rejection_reason = None;
    contract.signed_by = None;
    contract.signed_at = None;
}

pub(crate) fn set_contract_rejected(
    contract: &mut Contract,
    actor: &str,
    at: DateTime<Utc>,
    reason: &str,
) {
    contract.approved = false;
    contract.signed = false;
    contract.lifecycle_state = Some(CONTRACT_LIFECYCLE_DRAFT.to_string());
    contract.approval_status = Some(CONTRACT_APPROVAL_REJECTED.to_string());
    contract.approved_by = None;
    contract.approved_at = None;
    contract.rejected_by = Some(actor.to_string());
    contract.rejected_at = Some(at);
    contract.rejection_reason = Some(reason.to_string());
    contract.signed_by = None;
    contract.signed_at = None;
}

pub(crate) fn set_contract_signed(contract: &mut Contract, actor: &str, at: DateTime<Utc>) {
    contract.approved = true;
    contract.signed = true;
    contract.lifecycle_state = Some(CONTRACT_LIFECYCLE_SIGNED.to_string());
    contract.approval_status = Some(CONTRACT_APPROVAL_APPROVED.to_string());
    contract.signed_by = Some(actor.to_string());
    contract.signed_at = Some(at);
}
