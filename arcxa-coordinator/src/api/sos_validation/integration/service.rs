//! SoS validation orchestration service.
//!
//! This module provides the shared execution path used by:
//! - REST validation endpoints
//! - workflow-engine `sos_validation` steps
//! - report/history/lineage query endpoints
//! - RDF reconciliation for SoS catalog and validation lineage graphs

mod analytics;
mod approval;
mod contract_approval;
mod lookup;
mod projection;
mod retention;

use anyhow::Result as AnyhowResult;
use chrono::{DateTime, Utc};
use graphica_core::orchestration::workflow::{
    ExecutionContext, SosValidationCallback, SosValidationCheck as WorkflowSosValidationCheck,
    SosValidationConfig, SosValidationSpec, SosValidationStepResult,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Instant;
use thiserror::Error;
use uuid::Uuid;

use crate::api::sos_validation::contract_governance::{
    contract_revision_ref, effective_contract_approval_status, effective_contract_lifecycle_state,
    set_contract_approved, set_contract_rejected, stable_contract_ref, CONTRACT_LIFECYCLE_SIGNED,
};
use crate::api::sos_validation::contract_signature::verify_contract_signature;
use crate::api::sos_validation::policy_attestation::{
    verify_policy_attestation, PolicyAttestationSigningMaterial,
    POLICY_ATTESTATION_DEFAULT_TRUST_MODE, POLICY_ATTESTATION_EXTERNAL_KEY_REF_METADATA_KEY,
    POLICY_ATTESTATION_TRUST_ATTESTATION_REF_METADATA_KEY,
    POLICY_ATTESTATION_TRUST_MODE_METADATA_KEY, POLICY_ATTESTATION_TRUST_PROVIDER_METADATA_KEY,
};
use crate::api::sos_validation::storage::{
    Contract, ContractApprovalEvidenceRecord, ContractApprovalRequestRecord,
    ContractSignatureRecord, Interface, PolicyAttestationRecord, SosPolicy, System,
    ValidationChangeSummary, ValidationCheckRecord, ValidationReport,
};
use crate::api::sos_validation::types::{
    AddSosContractApprovalEvidenceRequest, AddSosPolicyApprovalEvidenceRequest,
    ApproveSosContractApprovalRequest, ApproveSosPolicyApprovalRequest, ApproveSosPolicyRequest,
    CheckResult, CompatibilityDetail, CompatibilityMatrixQuery, CompatibilityMatrixResponse,
    CompatibilityScore, CompatibilityState, ConfidenceAssessment, ConfidenceContributor,
    CreateSosContractApprovalRequest,
    CreateSosPolicyApprovalRequest,
    CreateSosPolicyRequest, DataContractResponse, DependencyGraphQuery, DependencyGraphResponse,
    EvaluatePolicyRequest, ListContractApprovalRequestsQuery, ListContractApprovalRequestsResponse,
    ListContractSignaturesResponse, ListPoliciesQuery, ListPoliciesResponse,
    ListPolicyApprovalRequestsQuery, ListPolicyApprovalRequestsResponse,
    ListPolicyAttestationsResponse, RejectSosContractApprovalRequest,
    RejectSosPolicyApprovalRequest, RejectSosPolicyRequest, SosContractApprovalEvidenceResponse,
    SosContractApprovalRequestResponse, SosContractSignatureResponse,
    SosPolicyApprovalEvidenceResponse, SosPolicyApprovalRequestResponse,
    SosPolicyAttestationResponse, SosPolicyResponse, UpdateSosPolicyRequest, ValidateRequest,
    ValidationChangeSummaryResponse, ValidationHistoryResponse, ValidationLineageEdge,
    ValidationLineageResponse, ValidationReportResponse, ValidationResponse, WhatIfRequest,
    WhatIfResponse,
};
use crate::api::sos_validation::validators::{
    compare_interface_schemas, evaluate_policy_results, evaluate_schema_transformability,
    DeclaredErrorBudget,
    extract_policy_placeholders, map_policy_severity, render_policy_query,
    TransformCompatibilityMode,
    validate_contract_transformation_rules, validate_coordinate_compatibility,
    validate_data_against_schema, validate_sla_metrics, validate_unit_compatibility,
    PolicyQueryTemplateError,
};
use crate::api::{sos_validation::storage::SosStorageManager, ApiState};
use crate::governance::rdf_store::{GraphicaRdfStore, NamedGraph, RdfStore, RdfTriple};
use crate::mapping::ontology_registry::PersistedOntologyRegistry;
use crate::observability::metrics::SosMetrics;

const RDF_NS: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
const PROV_NS: &str = "http://www.w3.org/ns/prov#";
const SOS_NS: &str = "http://graphica.io/sos#";
const XSD_DATE_TIME: &str = "http://www.w3.org/2001/XMLSchema#dateTime";
const XSD_DOUBLE: &str = "http://www.w3.org/2001/XMLSchema#double";
const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";
const POLICY_STAGE_PRE_EXECUTION: &str = "pre_execution";
const POLICY_STAGE_IN_FLIGHT: &str = "in_flight";
const POLICY_STAGE_POST_EXECUTION: &str = "post_execution";
const POLICY_STAGE_CONTRACT_APPROVAL: &str = "contract_approval";
const POLICY_STAGE_CONTRACT_SIGNING: &str = "contract_signing";
const POLICY_TARGET_GLOBAL: &str = "global";
const POLICY_TARGET_INTERFACE_PAIR: &str = "interface_pair";
const POLICY_TARGET_CONTRACT: &str = "contract";
const POLICY_TARGET_SYSTEM_PAIR: &str = "system_pair";
const POLICY_TARGET_INTERFACE: &str = "interface";
const POLICY_LIFECYCLE_DRAFT: &str = "draft";
const POLICY_LIFECYCLE_DRY_RUN: &str = "dry_run";
const POLICY_LIFECYCLE_ACTIVE: &str = "active";
const POLICY_LIFECYCLE_DEPRECATED: &str = "deprecated";
const POLICY_LIFECYCLE_RETIRED: &str = "retired";
const POLICY_APPROVAL_PENDING: &str = "pending";
const POLICY_APPROVAL_APPROVED: &str = "approved";
const POLICY_APPROVAL_REJECTED: &str = "rejected";

#[derive(Debug, Error)]
pub enum SosValidationServiceError {
    #[error("{0}")]
    InvalidRequest(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Unavailable(String),
    #[error("{0}")]
    Internal(String),
}

#[derive(Debug, Clone)]
pub struct ValidationExecutionOptions {
    pub persist_report: bool,
    pub emit_graph_lineage: bool,
    pub workflow_execution_id: Option<String>,
    pub workflow_step_id: Option<String>,
}

impl ValidationExecutionOptions {
    pub fn persisted() -> Self {
        Self {
            persist_report: true,
            emit_graph_lineage: true,
            workflow_execution_id: None,
            workflow_step_id: None,
        }
    }

    pub fn dry_run() -> Self {
        Self {
            persist_report: false,
            emit_graph_lineage: false,
            workflow_execution_id: None,
            workflow_step_id: None,
        }
    }
}

#[derive(Debug, Clone)]
struct ValidationExecution {
    subject_type: String,
    subject_key: String,
    validation_type: String,
    checks: Vec<ValidationCheckRecord>,
    confidence: f64,
    ontology_refs: Vec<String>,
    shape_refs: Vec<String>,
    policy_refs: Vec<String>,
    contract_refs: Vec<String>,
    schema_hashes: HashMap<String, String>,
}

pub struct SosValidationService {
    storage_manager: Arc<SosStorageManager>,
    rdf_store: Option<Arc<GraphicaRdfStore>>,
    persisted_ontology_registry: Option<Arc<PersistedOntologyRegistry>>,
    retention_config: retention::ValidationReportRetentionConfig,
    sos_metrics: Option<SosMetrics>,
}

impl SosValidationService {
    pub fn new(
        storage_manager: Arc<SosStorageManager>,
        rdf_store: Option<Arc<GraphicaRdfStore>>,
        persisted_ontology_registry: Option<Arc<PersistedOntologyRegistry>>,
    ) -> Self {
        Self {
            storage_manager,
            rdf_store,
            persisted_ontology_registry,
            retention_config: retention::ValidationReportRetentionConfig::from_env(),
            sos_metrics: None,
        }
    }

    pub fn with_metrics(mut self, sos_metrics: Option<SosMetrics>) -> Self {
        self.sos_metrics = sos_metrics;
        self
    }

    #[cfg(test)]
    fn with_retention_config(
        mut self,
        retention_config: retention::ValidationReportRetentionConfig,
    ) -> Self {
        self.retention_config = retention_config;
        self
    }

    pub fn from_api_state(state: &Arc<ApiState>) -> Result<Self, SosValidationServiceError> {
        let storage_manager = state.sos_storage_manager.as_ref().cloned().ok_or_else(|| {
            SosValidationServiceError::Unavailable(
                "SoS validation service is not enabled".to_string(),
            )
        })?;

        Ok(Self::new(
            storage_manager,
            state.rdf_store.clone(),
            state.persisted_ontology_registry.clone(),
        ))
        .map(|service| {
            service.with_metrics(
                state
                    .metrics_registry
                    .as_ref()
                    .map(|registry| registry.sos.clone()),
            )
        })
    }

    pub fn validate_request(
        &self,
        request: ValidateRequest,
        options: ValidationExecutionOptions,
    ) -> Result<ValidationResponse, SosValidationServiceError> {
        let spec = request_to_spec(request);
        self.validate_spec(spec, options)
    }

    pub fn validate_spec(
        &self,
        spec: SosValidationSpec,
        options: ValidationExecutionOptions,
    ) -> Result<ValidationResponse, SosValidationServiceError> {
        let started = Instant::now();
        let fallback_validation_type = validation_type_for_spec(&spec);
        let execution = match self.execute_spec(&spec) {
            Ok(execution) => execution,
            Err(error) => {
                self.record_validation_metrics(&fallback_validation_type, "error", started);
                return Err(error);
            }
        };
        let validation_type = execution.validation_type.clone();
        let response = self.finalize_execution(execution, options);
        let result = match &response {
            Ok(response) if response.passed => "passed",
            Ok(_) => "failed",
            Err(_) => "error",
        };
        self.record_validation_metrics(&validation_type, result, started);
        response
    }

    pub fn get_validation_report(
        &self,
        report_id: &str,
    ) -> Result<ValidationReportResponse, SosValidationServiceError> {
        let report = self
            .storage_manager
            .get_validation_report(report_id)
            .map_err(map_storage_error)?
            .ok_or_else(|| {
                SosValidationServiceError::NotFound(format!(
                    "Validation report '{}' not found",
                    report_id
                ))
            })?;

        Ok(to_report_response(report))
    }

    pub fn get_validation_history(
        &self,
        subject_key: &str,
        subject_type: Option<&str>,
        limit: usize,
    ) -> Result<ValidationHistoryResponse, SosValidationServiceError> {
        let reports = self
            .storage_manager
            .list_validation_history(subject_key, limit)
            .map_err(map_storage_error)?;

        if reports.is_empty() {
            return Err(SosValidationServiceError::NotFound(format!(
                "No validation history found for '{}'",
                subject_key
            )));
        }

        let actual_subject_type = reports[0].subject_type.clone();
        if let Some(expected) = subject_type {
            if !expected.eq_ignore_ascii_case(&actual_subject_type) {
                return Err(SosValidationServiceError::InvalidRequest(format!(
                    "Subject type '{}' does not match stored subject type '{}'",
                    expected, actual_subject_type
                )));
            }
        }
        self.observe_validation_history_length(&actual_subject_type, reports.len());

        Ok(ValidationHistoryResponse {
            subject_type: actual_subject_type,
            subject_key: subject_key.to_string(),
            reports: reports.into_iter().map(to_report_response).collect(),
        })
    }

    pub fn get_validation_lineage(
        &self,
        subject_key: &str,
        subject_type: Option<&str>,
        limit: usize,
    ) -> Result<ValidationLineageResponse, SosValidationServiceError> {
        let history = self.get_validation_history(subject_key, subject_type, limit)?;
        let report_ids: HashSet<String> = history
            .reports
            .iter()
            .map(|report| report.report_id.clone())
            .collect();

        let edges = history
            .reports
            .iter()
            .filter_map(|report| {
                let previous = report.previous_report_id.as_ref()?;
                report_ids
                    .contains(previous)
                    .then(|| ValidationLineageEdge {
                        from_report_id: previous.clone(),
                        to_report_id: report.report_id.clone(),
                        relationship: "prov:wasRevisionOf".to_string(),
                    })
            })
            .collect();

        Ok(ValidationLineageResponse {
            subject_type: history.subject_type,
            subject_key: history.subject_key,
            reports: history.reports,
            edges,
        })
    }

    pub fn create_contract_approval_request(
        &self,
        contract_id: &str,
        request: CreateSosContractApprovalRequest,
    ) -> Result<SosContractApprovalRequestResponse, SosValidationServiceError> {
        contract_approval::create_contract_approval_request(self, contract_id, request)
    }

    pub fn list_contract_approval_requests(
        &self,
        contract_id: &str,
        query: ListContractApprovalRequestsQuery,
    ) -> Result<ListContractApprovalRequestsResponse, SosValidationServiceError> {
        contract_approval::list_contract_approval_requests(self, contract_id, query)
    }

    pub fn get_contract_approval_request(
        &self,
        contract_id: &str,
        request_id: &str,
    ) -> Result<SosContractApprovalRequestResponse, SosValidationServiceError> {
        contract_approval::get_contract_approval_request(self, contract_id, request_id)
    }

    pub fn add_contract_approval_evidence(
        &self,
        contract_id: &str,
        request_id: &str,
        request: AddSosContractApprovalEvidenceRequest,
    ) -> Result<SosContractApprovalEvidenceResponse, SosValidationServiceError> {
        contract_approval::add_contract_approval_evidence(self, contract_id, request_id, request)
    }

    pub fn approve_contract_approval_request(
        &self,
        contract_id: &str,
        request_id: &str,
        request: ApproveSosContractApprovalRequest,
    ) -> Result<SosContractApprovalRequestResponse, SosValidationServiceError> {
        contract_approval::approve_contract_approval_request(self, contract_id, request_id, request)
    }

    pub fn reject_contract_approval_request(
        &self,
        contract_id: &str,
        request_id: &str,
        request: RejectSosContractApprovalRequest,
    ) -> Result<SosContractApprovalRequestResponse, SosValidationServiceError> {
        contract_approval::reject_contract_approval_request(self, contract_id, request_id, request)
    }

    pub fn approve_contract(
        &self,
        contract_id: &str,
        approved_by: &str,
    ) -> Result<DataContractResponse, SosValidationServiceError> {
        contract_approval::approve_contract_via_legacy_route(self, contract_id, approved_by)
            .and_then(|contract| self.to_contract_response(contract))
    }

    pub fn list_contract_signatures(
        &self,
        contract_id: &str,
        limit: usize,
    ) -> Result<ListContractSignaturesResponse, SosValidationServiceError> {
        let signature_records = self
            .storage_manager
            .list_contract_signatures(contract_id, limit)
            .map_err(map_storage_error)?;
        let total = signature_records.len();
        let mut signatures = Vec::with_capacity(signature_records.len());
        for signature in signature_records {
            if let Some(contract) = self
                .storage_manager
                .get_contract_revision(&signature.contract_id, signature.contract_revision)
                .map_err(map_storage_error)?
            {
                signatures.push(to_contract_signature_response(&contract, signature));
            }
        }

        Ok(ListContractSignaturesResponse {
            signatures,
            total,
            limit,
        })
    }

    pub(crate) fn contract_response(
        &self,
        contract: Contract,
    ) -> Result<DataContractResponse, SosValidationServiceError> {
        self.to_contract_response(contract)
    }

    pub fn evaluate_contract_governance_policies(
        &self,
        stage: &str,
        contract: &Contract,
        approval_request: Option<&ContractApprovalRequestRecord>,
        evidence: &[ContractApprovalEvidenceRecord],
        extra_context: HashMap<String, Value>,
    ) -> Result<Vec<String>, SosValidationServiceError> {
        self.enforce_contract_governance_policies(
            stage,
            contract,
            approval_request,
            evidence,
            extra_context,
        )
    }

    pub fn create_policy(
        &self,
        request: CreateSosPolicyRequest,
    ) -> Result<SosPolicyResponse, SosValidationServiceError> {
        if self
            .storage_manager
            .get_policy(&request.policy_id)
            .map_err(map_storage_error)?
            .is_some()
        {
            return Err(SosValidationServiceError::InvalidRequest(format!(
                "Policy '{}' already exists",
                request.policy_id
            )));
        }

        let policy = self.build_policy_from_create(request)?;
        self.storage_manager
            .put_policy(&policy)
            .map_err(map_storage_error)?;
        projection::project_policy_upsert(self, &policy)?;
        self.to_policy_response(policy)
    }

    pub fn get_policy(
        &self,
        policy_id: &str,
    ) -> Result<SosPolicyResponse, SosValidationServiceError> {
        let policy = self
            .storage_manager
            .get_policy(policy_id)
            .map_err(map_storage_error)?
            .ok_or_else(|| {
                SosValidationServiceError::NotFound(format!("Policy '{}' not found", policy_id))
            })?;
        self.to_policy_response(policy)
    }

    pub fn list_policy_revisions(
        &self,
        policy_id: &str,
        limit: usize,
    ) -> Result<Vec<SosPolicyResponse>, SosValidationServiceError> {
        let revisions = self
            .storage_manager
            .list_policy_revisions(policy_id, limit)
            .map_err(map_storage_error)?;

        if revisions.is_empty() {
            return Err(SosValidationServiceError::NotFound(format!(
                "Policy '{}' not found",
                policy_id
            )));
        }

        revisions
            .into_iter()
            .map(|policy| self.to_policy_response(policy))
            .collect()
    }

    pub fn list_policies(
        &self,
        query: ListPoliciesQuery,
    ) -> Result<ListPoliciesResponse, SosValidationServiceError> {
        let mut policies = if let Some(stage) = query.stage.as_deref() {
            let normalized_stage = normalize_policy_stage(stage)?;
            self.storage_manager
                .list_policies_by_stage(&normalized_stage, usize::MAX)
                .map_err(map_storage_error)?
        } else {
            self.storage_manager
                .list_all_policies(0, usize::MAX)
                .map_err(map_storage_error)?
        };

        if let Some(target_type) = query.target_type.as_deref() {
            let normalized_target_type = normalize_policy_target_type(target_type)?;
            policies.retain(|policy| policy.target_type == normalized_target_type);
        }

        if let Some(active) = query.active {
            policies.retain(|policy| policy_is_automatic(policy) == active);
        }

        if let Some(lifecycle_state) = query.lifecycle_state.as_deref() {
            let normalized_lifecycle_state = normalize_policy_lifecycle_state(lifecycle_state)?;
            policies.retain(|policy| {
                effective_policy_lifecycle_state(policy) == normalized_lifecycle_state
            });
        }

        if let Some(approval_status) = query.approval_status.as_deref() {
            let normalized_approval_status = normalize_policy_approval_status(approval_status)?;
            policies.retain(|policy| {
                effective_policy_approval_status(policy) == normalized_approval_status
            });
        }

        policies.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        let total = policies.len();
        let policies = policies
            .into_iter()
            .skip(query.offset)
            .take(query.limit)
            .map(|policy| self.to_policy_response(policy))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(ListPoliciesResponse {
            policies,
            total,
            offset: query.offset,
            limit: query.limit,
        })
    }

    pub fn list_policy_attestations(
        &self,
        policy_id: &str,
        limit: usize,
    ) -> Result<ListPolicyAttestationsResponse, SosValidationServiceError> {
        self.storage_manager
            .get_policy(policy_id)
            .map_err(map_storage_error)?
            .ok_or_else(|| {
                SosValidationServiceError::NotFound(format!("Policy '{}' not found", policy_id))
            })?;

        let attestation_records = self
            .storage_manager
            .list_policy_attestations(policy_id, limit)
            .map_err(map_storage_error)?;
        let total = attestation_records.len();
        let mut attestations = Vec::with_capacity(attestation_records.len());
        for attestation in attestation_records {
            if let Some(policy) = self
                .storage_manager
                .get_policy_revision(&attestation.policy_id, attestation.policy_revision)
                .map_err(map_storage_error)?
            {
                attestations.push(to_policy_attestation_response(&policy, attestation));
            }
        }

        Ok(ListPolicyAttestationsResponse {
            attestations,
            total,
            limit,
        })
    }

    pub fn update_policy(
        &self,
        policy_id: &str,
        request: UpdateSosPolicyRequest,
    ) -> Result<SosPolicyResponse, SosValidationServiceError> {
        let existing = self
            .storage_manager
            .get_policy(policy_id)
            .map_err(map_storage_error)?
            .ok_or_else(|| {
                SosValidationServiceError::NotFound(format!("Policy '{}' not found", policy_id))
            })?;

        let updated = self.build_policy_from_update(existing, request)?;
        self.storage_manager
            .put_policy(&updated)
            .map_err(map_storage_error)?;
        projection::project_policy_upsert(self, &updated)?;
        self.to_policy_response(updated)
    }

    pub fn create_policy_approval_request(
        &self,
        policy_id: &str,
        request: CreateSosPolicyApprovalRequest,
    ) -> Result<SosPolicyApprovalRequestResponse, SosValidationServiceError> {
        approval::create_policy_approval_request(self, policy_id, request)
    }

    pub fn list_policy_approval_requests(
        &self,
        policy_id: &str,
        query: ListPolicyApprovalRequestsQuery,
    ) -> Result<ListPolicyApprovalRequestsResponse, SosValidationServiceError> {
        approval::list_policy_approval_requests(self, policy_id, query)
    }

    pub fn get_policy_approval_request(
        &self,
        policy_id: &str,
        request_id: &str,
    ) -> Result<SosPolicyApprovalRequestResponse, SosValidationServiceError> {
        approval::get_policy_approval_request(self, policy_id, request_id)
    }

    pub fn add_policy_approval_evidence(
        &self,
        policy_id: &str,
        request_id: &str,
        request: AddSosPolicyApprovalEvidenceRequest,
    ) -> Result<SosPolicyApprovalEvidenceResponse, SosValidationServiceError> {
        approval::add_policy_approval_evidence(self, policy_id, request_id, request)
    }

    pub fn approve_policy_approval_request(
        &self,
        policy_id: &str,
        request_id: &str,
        request: ApproveSosPolicyApprovalRequest,
    ) -> Result<SosPolicyApprovalRequestResponse, SosValidationServiceError> {
        approval::approve_policy_approval_request(self, policy_id, request_id, request)
    }

    pub fn reject_policy_approval_request(
        &self,
        policy_id: &str,
        request_id: &str,
        request: RejectSosPolicyApprovalRequest,
    ) -> Result<SosPolicyApprovalRequestResponse, SosValidationServiceError> {
        approval::reject_policy_approval_request(self, policy_id, request_id, request)
    }

    pub fn approve_policy(
        &self,
        policy_id: &str,
        request: ApproveSosPolicyRequest,
    ) -> Result<SosPolicyResponse, SosValidationServiceError> {
        approval::approve_policy_via_legacy_route(self, policy_id, request)
    }

    pub fn approve_policy_with_attestation(
        &self,
        policy_id: &str,
        request: ApproveSosPolicyRequest,
        signing_material: PolicyAttestationSigningMaterial,
    ) -> Result<SosPolicyResponse, SosValidationServiceError> {
        approval::approve_policy_via_legacy_route_with_attestation(
            self,
            policy_id,
            request,
            signing_material,
        )
    }

    pub fn reject_policy(
        &self,
        policy_id: &str,
        request: RejectSosPolicyRequest,
    ) -> Result<SosPolicyResponse, SosValidationServiceError> {
        approval::reject_policy_via_legacy_route(self, policy_id, request)
    }

    pub fn approve_policy_approval_request_with_attestation(
        &self,
        policy_id: &str,
        request_id: &str,
        request: ApproveSosPolicyApprovalRequest,
        signing_material: PolicyAttestationSigningMaterial,
    ) -> Result<SosPolicyApprovalRequestResponse, SosValidationServiceError> {
        approval::approve_policy_approval_request_with_attestation(
            self,
            policy_id,
            request_id,
            request,
            signing_material,
        )
    }

    pub fn delete_policy(&self, policy_id: &str) -> Result<(), SosValidationServiceError> {
        if self
            .storage_manager
            .get_policy(policy_id)
            .map_err(map_storage_error)?
            .is_none()
        {
            return Err(SosValidationServiceError::NotFound(format!(
                "Policy '{}' not found",
                policy_id
            )));
        }

        let revisions = self
            .storage_manager
            .list_policy_revisions(policy_id, usize::MAX)
            .map_err(map_storage_error)?;
        let approval_requests = self
            .storage_manager
            .list_policy_approval_requests(policy_id, usize::MAX)
            .map_err(map_storage_error)?;
        let mut approval_evidence = Vec::new();
        for request in &approval_requests {
            approval_evidence.extend(
                self.storage_manager
                    .list_policy_approval_evidence(&request.request_id)
                    .map_err(map_storage_error)?,
            );
        }
        let attestations = self
            .storage_manager
            .list_policy_attestations(policy_id, usize::MAX)
            .map_err(map_storage_error)?;
        self.storage_manager
            .delete_policy(policy_id)
            .map_err(map_storage_error)?;
        projection::project_policy_delete(
            self,
            policy_id,
            &revisions,
            &approval_requests,
            &approval_evidence,
            &attestations,
        )?;
        Ok(())
    }

    pub fn evaluate_policy_by_id(
        &self,
        policy_id: &str,
        request: EvaluatePolicyRequest,
        options: ValidationExecutionOptions,
    ) -> Result<ValidationResponse, SosValidationServiceError> {
        let started = Instant::now();
        let response = (|| {
            let policy = match request.revision {
                Some(0) => {
                    return Err(SosValidationServiceError::InvalidRequest(
                        "Policy revision must be greater than zero".to_string(),
                    ))
                }
                Some(revision) => self
                    .storage_manager
                    .get_policy_revision(policy_id, revision)
                    .map_err(map_storage_error)?
                    .ok_or_else(|| {
                        SosValidationServiceError::NotFound(format!(
                            "Policy '{}' revision {} not found",
                            policy_id, revision
                        ))
                    })?,
                None => self
                    .storage_manager
                    .get_policy(policy_id)
                    .map_err(map_storage_error)?
                    .ok_or_else(|| {
                        SosValidationServiceError::NotFound(format!(
                            "Policy '{}' not found",
                            policy_id
                        ))
                    })?,
            };

            let stage = request
                .stage
                .as_deref()
                .map(normalize_policy_stage)
                .transpose()?
                .or_else(|| policy.stages.first().cloned())
                .unwrap_or_else(|| POLICY_STAGE_PRE_EXECUTION.to_string());

            if !policy.stages.iter().any(|configured| configured == &stage) {
                return Err(SosValidationServiceError::InvalidRequest(format!(
                    "Policy '{}' is not configured for stage '{}'",
                    policy_id, stage
                )));
            }

            let execution = self.evaluate_policy_definition(
                &policy,
                &stage,
                &request.context,
                Some((
                    "policy".to_string(),
                    format!("policy:{}", policy.policy_id),
                    "policy_check".to_string(),
                )),
            )?;

            self.finalize_execution(execution, options)
        })();
        let result = match &response {
            Ok(response) if response.passed => "passed",
            Ok(_) => "failed",
            Err(_) => "error",
        };
        self.record_validation_metrics("policy_check", result, started);
        response
    }

    pub fn build_compatibility_matrix(
        &self,
    ) -> Result<CompatibilityMatrixResponse, SosValidationServiceError> {
        self.build_compatibility_matrix_with_query(CompatibilityMatrixQuery {
            evaluation_budget: None,
        })
    }

    pub fn build_compatibility_matrix_with_query(
        &self,
        query: CompatibilityMatrixQuery,
    ) -> Result<CompatibilityMatrixResponse, SosValidationServiceError> {
        let started = Instant::now();
        let result = analytics::build_compatibility_matrix(self, query.evaluation_budget);
        self.record_analytics_metrics("compatibility_matrix", started);
        result
    }

    pub fn build_dependency_graph(
        &self,
    ) -> Result<DependencyGraphResponse, SosValidationServiceError> {
        self.build_dependency_graph_with_query(DependencyGraphQuery {
            node_budget: None,
            edge_budget: None,
        })
    }

    pub fn build_dependency_graph_with_query(
        &self,
        query: DependencyGraphQuery,
    ) -> Result<DependencyGraphResponse, SosValidationServiceError> {
        let started = Instant::now();
        let result = analytics::build_dependency_graph(self, query);
        self.record_analytics_metrics("dependency_graph", started);
        result
    }

    pub fn run_what_if_analysis(
        &self,
        request: WhatIfRequest,
    ) -> Result<WhatIfResponse, SosValidationServiceError> {
        let started = Instant::now();
        let result = analytics::run_what_if_analysis(self, request);
        self.record_analytics_metrics("what_if_analysis", started);
        result
    }

    fn finalize_execution(
        &self,
        mut execution: ValidationExecution,
        options: ValidationExecutionOptions,
    ) -> Result<ValidationResponse, SosValidationServiceError> {
        annotate_checks_with_confidence(&mut execution.checks);
        execution.confidence = compute_confidence(&execution.checks);

        let validation_id = Uuid::new_v4().to_string();
        let validated_at = Utc::now();
        let passed = execution_passed(&execution.checks);
        let confidence_assessment = Some(build_confidence_assessment(&execution.checks));

        let report = if options.persist_report {
            let previous_report = self
                .storage_manager
                .get_latest_validation_report(&execution.subject_key)
                .map_err(map_storage_error)?;

            let change_summary = build_change_summary(
                previous_report.as_ref(),
                &execution.checks,
                execution.confidence,
                &execution.schema_hashes,
                &execution.policy_refs,
                &execution.contract_refs,
                &execution.shape_refs,
            );

            let report = ValidationReport {
                report_id: Uuid::new_v4().to_string(),
                validation_id: validation_id.clone(),
                subject_type: execution.subject_type.clone(),
                subject_key: execution.subject_key.clone(),
                validation_type: execution.validation_type.clone(),
                passed,
                confidence: execution.confidence,
                checks: execution.checks.clone(),
                validated_at,
                previous_report_id: previous_report
                    .as_ref()
                    .map(|report| report.report_id.clone()),
                change_summary,
                workflow_execution_id: options.workflow_execution_id.clone(),
                workflow_step_id: options.workflow_step_id.clone(),
                ontology_refs: execution.ontology_refs.clone(),
                shape_refs: execution.shape_refs.clone(),
                policy_refs: execution.policy_refs.clone(),
                contract_refs: execution.contract_refs.clone(),
                schema_hashes: execution.schema_hashes.clone(),
            };

            self.storage_manager
                .put_validation_report(&report)
                .map_err(map_storage_error)?;
            self.record_report_persisted_metrics(&report);

            if options.emit_graph_lineage {
                projection::project_validation_report_upsert(self, &report)?;
            }

            let pruned_reports = retention::prune_after_persist(self, &report)?;
            self.record_reports_pruned_metrics(pruned_reports.len());
            for pruned_report in pruned_reports {
                projection::project_validation_report_delete(self, &pruned_report)?;
            }

            Some(report)
        } else {
            None
        };

        let compatibility_state = match execution.validation_type.as_str() {
            "interface_compatibility" => derive_interface_compatibility_state(&execution.checks),
            _ => None,
        };

        Ok(ValidationResponse {
            validation_id,
            passed,
            checks: execution.checks.into_iter().map(into_api_check).collect(),
            confidence: execution.confidence,
            compatibility_state,
            confidence_assessment,
            validated_at: validated_at.to_rfc3339(),
            report_id: report.map(|persisted| persisted.report_id),
        })
    }

    fn build_policy_from_create(
        &self,
        request: CreateSosPolicyRequest,
    ) -> Result<SosPolicy, SosValidationServiceError> {
        let now = Utc::now();
        let target_type = normalize_policy_target_type(&request.target_type)?;
        let stages = normalize_policy_stages(request.stages)?;
        let enforcement_level = normalize_policy_enforcement_level(&request.enforcement_level)?;
        let severity = normalize_policy_severity(&request.severity)?;
        let created_by = normalize_policy_actor(
            "created_by",
            request.created_by.clone().or(request.updated_by.clone()),
        )?;
        let updated_by = normalize_policy_actor(
            "updated_by",
            request.updated_by.or_else(|| Some(created_by.clone())),
        )?;
        let lifecycle_state = request
            .lifecycle_state
            .as_deref()
            .map(normalize_policy_lifecycle_state)
            .transpose()?
            .unwrap_or_else(|| legacy_lifecycle_state_for_active(request.active));

        let mut policy = SosPolicy {
            policy_id: normalize_non_empty("policy_id", request.policy_id)?,
            revision: 1,
            policy_name: normalize_non_empty("policy_name", request.policy_name)?,
            description: request.description,
            lifecycle_state: Some(lifecycle_state.clone()),
            approval_status: None,
            approval_requested_by: None,
            approval_requested_at: None,
            approved_by: None,
            approved_at: None,
            rejected_by: None,
            rejected_at: None,
            rejection_reason: None,
            target_type,
            target_key: None,
            stages,
            enforcement_level,
            severity,
            sparql_query: normalize_non_empty("sparql_query", request.sparql_query)?,
            context: request.context,
            tags: request.tags,
            ontology_refs: request.ontology_refs,
            shape_refs: request.shape_refs,
            active: policy_state_is_automatic(&lifecycle_state),
            provider_interface_id: request.provider_interface_id,
            consumer_interface_id: request.consumer_interface_id,
            contract_id: request.contract_id,
            source_system_id: request.source_system_id,
            target_system_id: request.target_system_id,
            interface_id: request.interface_id,
            created_by: created_by.clone(),
            updated_by: updated_by.clone(),
            superseded_by_revision: None,
            created_at: now,
            updated_at: now,
        };
        initialize_policy_approval(&mut policy, &created_by, now);
        self.validate_and_enrich_policy(&mut policy)?;
        Ok(policy)
    }

    fn build_policy_from_update(
        &self,
        mut existing: SosPolicy,
        request: UpdateSosPolicyRequest,
    ) -> Result<SosPolicy, SosValidationServiceError> {
        let current_lifecycle_state = effective_policy_lifecycle_state(&existing).to_string();
        let current_approval_status = effective_policy_approval_status(&existing).to_string();
        let updated_by = normalize_policy_actor("updated_by", request.updated_by.clone())?;
        let now = Utc::now();
        let mut semantic_changes = false;

        if let Some(policy_name) = request.policy_name {
            existing.policy_name = normalize_non_empty("policy_name", policy_name)?;
        }
        if let Some(target_type) = request.target_type {
            let target_type = normalize_policy_target_type(&target_type)?;
            semantic_changes |= existing.target_type != target_type;
            existing.target_type = target_type;
        }
        if let Some(stages) = request.stages {
            let stages = normalize_policy_stages(stages)?;
            semantic_changes |= existing.stages != stages;
            existing.stages = stages;
        }
        if let Some(enforcement_level) = request.enforcement_level {
            let enforcement_level = normalize_policy_enforcement_level(&enforcement_level)?;
            semantic_changes |= existing.enforcement_level != enforcement_level;
            existing.enforcement_level = enforcement_level;
        }
        if let Some(severity) = request.severity {
            let severity = normalize_policy_severity(&severity)?;
            semantic_changes |= existing.severity != severity;
            existing.severity = severity;
        }
        if let Some(sparql_query) = request.sparql_query {
            let sparql_query = normalize_non_empty("sparql_query", sparql_query)?;
            semantic_changes |= existing.sparql_query != sparql_query;
            existing.sparql_query = sparql_query;
        }
        if let Some(context) = request.context {
            semantic_changes |= existing.context != context;
            existing.context = context;
        }
        if let Some(description) = request.description {
            existing.description = Some(description);
        }
        if let Some(tags) = request.tags {
            existing.tags = tags;
        }
        if let Some(ontology_refs) = request.ontology_refs {
            existing.ontology_refs = ontology_refs;
        }
        if let Some(shape_refs) = request.shape_refs {
            existing.shape_refs = shape_refs;
        }
        let next_lifecycle_state = next_policy_lifecycle_state(
            &current_lifecycle_state,
            request.lifecycle_state.as_deref(),
            request.active,
        )?;
        existing.lifecycle_state = Some(next_lifecycle_state.clone());
        existing.active = policy_state_is_automatic(&next_lifecycle_state);
        if request.provider_interface_id.is_some() {
            semantic_changes |= existing.provider_interface_id != request.provider_interface_id;
            existing.provider_interface_id = request.provider_interface_id;
        }
        if request.consumer_interface_id.is_some() {
            semantic_changes |= existing.consumer_interface_id != request.consumer_interface_id;
            existing.consumer_interface_id = request.consumer_interface_id;
        }
        if request.contract_id.is_some() {
            semantic_changes |= existing.contract_id != request.contract_id;
            existing.contract_id = request.contract_id;
        }
        if request.source_system_id.is_some() {
            semantic_changes |= existing.source_system_id != request.source_system_id;
            existing.source_system_id = request.source_system_id;
        }
        if request.target_system_id.is_some() {
            semantic_changes |= existing.target_system_id != request.target_system_id;
            existing.target_system_id = request.target_system_id;
        }
        if request.interface_id.is_some() {
            semantic_changes |= existing.interface_id != request.interface_id;
            existing.interface_id = request.interface_id;
        }

        if semantic_changes && policy_state_is_automatic(&next_lifecycle_state) {
            return Err(SosValidationServiceError::InvalidRequest(
                "Policy changes that modify evaluation semantics must remain in a non-automatic lifecycle state until the latest revision is approved".to_string(),
            ));
        }

        if !semantic_changes
            && policy_state_is_automatic(&next_lifecycle_state)
            && current_approval_status != POLICY_APPROVAL_APPROVED
        {
            return Err(SosValidationServiceError::InvalidRequest(format!(
                "Policy lifecycle_state '{}' requires an approved policy revision",
                next_lifecycle_state
            )));
        }

        if semantic_changes {
            existing.revision = existing.revision.saturating_add(1);
            existing.superseded_by_revision = None;
            set_policy_approval_pending(&mut existing, &updated_by, now);
        }

        existing.updated_by = updated_by;
        existing.updated_at = now;
        self.validate_and_enrich_policy(&mut existing)?;
        Ok(existing)
    }

    fn approve_policy_revision(
        &self,
        mut policy: SosPolicy,
        approved_by: String,
        requested_lifecycle_state: Option<&str>,
    ) -> Result<SosPolicy, SosValidationServiceError> {
        let now = Utc::now();
        let current_lifecycle_state = effective_policy_lifecycle_state(&policy).to_string();
        if let Some(next_lifecycle_state) =
            approval_endpoint_lifecycle_state(&current_lifecycle_state, requested_lifecycle_state)?
        {
            policy.lifecycle_state = Some(next_lifecycle_state.clone());
            policy.active = policy_state_is_automatic(&next_lifecycle_state);
        }

        policy.updated_by = approved_by.clone();
        policy.updated_at = now;
        set_policy_approval_approved(&mut policy, &approved_by, now);
        self.validate_and_enrich_policy(&mut policy)?;
        Ok(policy)
    }

    fn reject_policy_revision(
        &self,
        mut policy: SosPolicy,
        rejected_by: String,
        reason: String,
    ) -> Result<SosPolicy, SosValidationServiceError> {
        if policy_is_automatic(&policy) {
            return Err(SosValidationServiceError::InvalidRequest(
                "Automatic policies must be moved to a non-automatic lifecycle_state before rejection".to_string(),
            ));
        }

        let reason = normalize_non_empty("reason", reason)?;
        let now = Utc::now();
        policy.updated_by = rejected_by.clone();
        policy.updated_at = now;
        set_policy_approval_rejected(&mut policy, &rejected_by, now, reason);
        self.validate_and_enrich_policy(&mut policy)?;
        Ok(policy)
    }

    fn approve_contract_revision(
        &self,
        mut contract: Contract,
        approved_by: String,
    ) -> Result<Contract, SosValidationServiceError> {
        let now = Utc::now();
        contract.updated_by = approved_by.clone();
        contract.updated_at = now;
        set_contract_approved(&mut contract, &approved_by, now);
        Ok(contract)
    }

    fn reject_contract_revision(
        &self,
        mut contract: Contract,
        rejected_by: String,
        reason: String,
    ) -> Result<Contract, SosValidationServiceError> {
        if effective_contract_lifecycle_state(&contract) == CONTRACT_LIFECYCLE_SIGNED {
            return Err(SosValidationServiceError::InvalidRequest(
                "Signed contracts cannot be rejected".to_string(),
            ));
        }

        let reason = normalize_non_empty("reason", reason)?;
        let now = Utc::now();
        contract.updated_by = rejected_by.clone();
        contract.updated_at = now;
        set_contract_rejected(&mut contract, &rejected_by, now, &reason);
        Ok(contract)
    }

    fn validate_and_enrich_policy(
        &self,
        policy: &mut SosPolicy,
    ) -> Result<(), SosValidationServiceError> {
        let target_key = match policy.target_type.as_str() {
            POLICY_TARGET_GLOBAL => None,
            POLICY_TARGET_INTERFACE_PAIR => {
                let provider_interface_id =
                    policy.provider_interface_id.as_deref().ok_or_else(|| {
                        SosValidationServiceError::InvalidRequest(
                            "interface_pair policies require provider_interface_id".to_string(),
                        )
                    })?;
                let consumer_interface_id =
                    policy.consumer_interface_id.as_deref().ok_or_else(|| {
                        SosValidationServiceError::InvalidRequest(
                            "interface_pair policies require consumer_interface_id".to_string(),
                        )
                    })?;
                lookup::get_interface(self, provider_interface_id)?;
                lookup::get_interface(self, consumer_interface_id)?;
                Some(format!(
                    "interface_pair:{}:{}",
                    provider_interface_id, consumer_interface_id
                ))
            }
            POLICY_TARGET_CONTRACT => {
                let contract_id = policy.contract_id.as_deref().ok_or_else(|| {
                    SosValidationServiceError::InvalidRequest(
                        "contract policies require contract_id".to_string(),
                    )
                })?;
                lookup::get_contract(self, contract_id)?;
                Some(format!("contract:{}", contract_id))
            }
            POLICY_TARGET_SYSTEM_PAIR => {
                let source_system_id = policy.source_system_id.as_deref().ok_or_else(|| {
                    SosValidationServiceError::InvalidRequest(
                        "system_pair policies require source_system_id".to_string(),
                    )
                })?;
                let target_system_id = policy.target_system_id.as_deref().ok_or_else(|| {
                    SosValidationServiceError::InvalidRequest(
                        "system_pair policies require target_system_id".to_string(),
                    )
                })?;
                lookup::get_system(self, source_system_id)?;
                lookup::get_system(self, target_system_id)?;
                Some(format!(
                    "system_pair:{}:{}",
                    source_system_id, target_system_id
                ))
            }
            POLICY_TARGET_INTERFACE => {
                let interface_id = policy.interface_id.as_deref().ok_or_else(|| {
                    SosValidationServiceError::InvalidRequest(
                        "interface policies require interface_id".to_string(),
                    )
                })?;
                lookup::get_interface(self, interface_id)?;
                Some(format!("interface:{}", interface_id))
            }
            _ => {
                return Err(SosValidationServiceError::InvalidRequest(format!(
                    "Unsupported policy target type '{}'",
                    policy.target_type
                )));
            }
        };

        policy.target_key = target_key;

        let placeholder_context =
            self.build_policy_context(policy, &policy.stages[0], &HashMap::new())?;
        validate_policy_placeholders_for_definition(&policy.sparql_query, &placeholder_context)?;

        if let Some(rdf_store) = self.rdf_store.as_ref() {
            let placeholders = extract_policy_placeholders(&policy.sparql_query)
                .map_err(map_policy_template_error)?;
            if placeholders
                .iter()
                .all(|placeholder| placeholder_context.contains_key(placeholder))
            {
                let rendered_query =
                    render_policy_query(&policy.sparql_query, &placeholder_context)
                        .map_err(map_policy_template_error)?;
                rdf_store.query(&rendered_query).map_err(|error| {
                    SosValidationServiceError::InvalidRequest(format!(
                        "Policy '{}' contains an invalid or unsupported SPARQL query: {}",
                        policy.policy_id, error
                    ))
                })?;
            }
        }

        dedupe_strings(&mut policy.tags);
        dedupe_strings(&mut policy.ontology_refs);
        dedupe_strings(&mut policy.shape_refs);

        Ok(())
    }

    fn build_policy_context(
        &self,
        policy: &SosPolicy,
        stage: &str,
        runtime_context: &HashMap<String, Value>,
    ) -> Result<HashMap<String, Value>, SosValidationServiceError> {
        let mut context = HashMap::from([
            (
                "policy_id".to_string(),
                Value::String(policy.policy_id.clone()),
            ),
            (
                "policy_uri".to_string(),
                Value::String(projection::policy_uri(&policy.policy_id)),
            ),
            ("policy_revision".to_string(), Value::from(policy.revision)),
            (
                "policy_revision_ref".to_string(),
                Value::String(policy_revision_ref(policy)),
            ),
            (
                "policy_revision_uri".to_string(),
                Value::String(projection::policy_ref_to_uri(&policy_revision_ref(policy))),
            ),
            (
                "policy_name".to_string(),
                Value::String(policy.policy_name.clone()),
            ),
            (
                "policy_created_by".to_string(),
                Value::String(policy.created_by.clone()),
            ),
            (
                "policy_updated_by".to_string(),
                Value::String(policy.updated_by.clone()),
            ),
            (
                "policy_lifecycle_state".to_string(),
                Value::String(effective_policy_lifecycle_state(policy).to_string()),
            ),
            (
                "policy_approval_status".to_string(),
                Value::String(effective_policy_approval_status(policy).to_string()),
            ),
            (
                "policy_active".to_string(),
                Value::Bool(policy_is_automatic(policy)),
            ),
            (
                "target_type".to_string(),
                Value::String(policy.target_type.clone()),
            ),
            ("stage".to_string(), Value::String(stage.to_string())),
            (
                "enforcement_level".to_string(),
                Value::String(policy.enforcement_level.clone()),
            ),
            (
                "severity".to_string(),
                Value::String(policy.severity.clone()),
            ),
        ]);

        if let Some(approval_requested_by) = &policy.approval_requested_by {
            context.insert(
                "policy_approval_requested_by".to_string(),
                Value::String(approval_requested_by.clone()),
            );
        }
        if let Some(approval_requested_at) = policy.approval_requested_at.as_ref() {
            context.insert(
                "policy_approval_requested_at".to_string(),
                Value::String(approval_requested_at.to_rfc3339()),
            );
        }
        if let Some(approved_by) = &policy.approved_by {
            context.insert(
                "policy_approved_by".to_string(),
                Value::String(approved_by.clone()),
            );
        }
        if let Some(approved_at) = policy.approved_at.as_ref() {
            context.insert(
                "policy_approved_at".to_string(),
                Value::String(approved_at.to_rfc3339()),
            );
        }
        if let Some(rejected_by) = &policy.rejected_by {
            context.insert(
                "policy_rejected_by".to_string(),
                Value::String(rejected_by.clone()),
            );
        }
        if let Some(rejected_at) = policy.rejected_at.as_ref() {
            context.insert(
                "policy_rejected_at".to_string(),
                Value::String(rejected_at.to_rfc3339()),
            );
        }
        if let Some(rejection_reason) = &policy.rejection_reason {
            context.insert(
                "policy_rejection_reason".to_string(),
                Value::String(rejection_reason.clone()),
            );
        }

        if let Some(target_key) = &policy.target_key {
            context.insert("target_key".to_string(), Value::String(target_key.clone()));
        }

        context.extend(policy.context.clone());
        context.extend(self.policy_target_context(policy)?);
        context.extend(runtime_context.clone());

        Ok(context)
    }

    fn policy_target_context(
        &self,
        policy: &SosPolicy,
    ) -> Result<HashMap<String, Value>, SosValidationServiceError> {
        let mut context = HashMap::new();

        if let Some(provider_interface_id) = &policy.provider_interface_id {
            let provider = lookup::get_interface(self, provider_interface_id)?;
            context.insert(
                "provider_interface_id".to_string(),
                Value::String(provider.interface_id.clone()),
            );
            context.insert(
                "provider_interface_uri".to_string(),
                Value::String(projection::interface_uri(&provider.interface_id)),
            );
            context.insert(
                "provider_system_id".to_string(),
                Value::String(provider.system_id.clone()),
            );
            context.insert(
                "provider_system_uri".to_string(),
                Value::String(projection::system_uri(&provider.system_id)),
            );
        }
        if let Some(consumer_interface_id) = &policy.consumer_interface_id {
            let consumer = lookup::get_interface(self, consumer_interface_id)?;
            context.insert(
                "consumer_interface_id".to_string(),
                Value::String(consumer.interface_id.clone()),
            );
            context.insert(
                "consumer_interface_uri".to_string(),
                Value::String(projection::interface_uri(&consumer.interface_id)),
            );
            context.insert(
                "consumer_system_id".to_string(),
                Value::String(consumer.system_id.clone()),
            );
            context.insert(
                "consumer_system_uri".to_string(),
                Value::String(projection::system_uri(&consumer.system_id)),
            );
        }
        if let Some(contract_id) = &policy.contract_id {
            let contract = lookup::get_contract(self, contract_id)?;
            context.insert(
                "contract_id".to_string(),
                Value::String(contract.contract_id.clone()),
            );
            context.insert(
                "contract_uri".to_string(),
                Value::String(projection::contract_uri(&contract.contract_id)),
            );
            context.insert(
                "contract_revision".to_string(),
                Value::from(contract.revision),
            );
            context.insert(
                "contract_revision_ref".to_string(),
                Value::String(contract_revision_ref(&contract)),
            );
            context.insert(
                "contract_revision_uri".to_string(),
                Value::String(projection::contract_revision_uri(
                    &contract.contract_id,
                    contract.revision,
                )),
            );
            context.insert(
                "contract_lifecycle_state".to_string(),
                Value::String(effective_contract_lifecycle_state(&contract).to_string()),
            );
            context.insert(
                "contract_approval_status".to_string(),
                Value::String(effective_contract_approval_status(&contract).to_string()),
            );
            context.insert(
                "provider_interface_id".to_string(),
                Value::String(contract.provider_interface_id.clone()),
            );
            context.insert(
                "consumer_interface_id".to_string(),
                Value::String(contract.consumer_interface_id.clone()),
            );
            if let Some(requested_by) = &contract.approval_requested_by {
                context.insert(
                    "contract_approval_requested_by".to_string(),
                    Value::String(requested_by.clone()),
                );
            }
            if let Some(approved_by) = &contract.approved_by {
                context.insert(
                    "contract_approved_by".to_string(),
                    Value::String(approved_by.clone()),
                );
            }
            if let Some(signed_by) = &contract.signed_by {
                context.insert(
                    "contract_signed_by".to_string(),
                    Value::String(signed_by.clone()),
                );
            }
        }
        if let Some(source_system_id) = &policy.source_system_id {
            lookup::get_system(self, source_system_id)?;
            context.insert(
                "source_system_id".to_string(),
                Value::String(source_system_id.clone()),
            );
            context.insert(
                "source_system_uri".to_string(),
                Value::String(projection::system_uri(source_system_id)),
            );
        }
        if let Some(target_system_id) = &policy.target_system_id {
            lookup::get_system(self, target_system_id)?;
            context.insert(
                "target_system_id".to_string(),
                Value::String(target_system_id.clone()),
            );
            context.insert(
                "target_system_uri".to_string(),
                Value::String(projection::system_uri(target_system_id)),
            );
        }
        if let Some(interface_id) = &policy.interface_id {
            let interface = lookup::get_interface(self, interface_id)?;
            context.insert(
                "interface_id".to_string(),
                Value::String(interface.interface_id.clone()),
            );
            context.insert(
                "interface_uri".to_string(),
                Value::String(projection::interface_uri(&interface.interface_id)),
            );
            context.insert(
                "system_id".to_string(),
                Value::String(interface.system_id.clone()),
            );
            context.insert(
                "system_uri".to_string(),
                Value::String(projection::system_uri(&interface.system_id)),
            );
        }

        Ok(context)
    }

    fn apply_policies_for_stage(
        &self,
        stage: &str,
        runtime_context: &HashMap<String, Value>,
        execution: &mut ValidationExecution,
    ) -> Result<(), SosValidationServiceError> {
        let policies = self
            .storage_manager
            .list_policies_by_stage(stage, usize::MAX)
            .map_err(map_storage_error)?;

        for policy in policies
            .into_iter()
            .filter(|policy| policy_is_automatic(policy))
            .filter(|policy| effective_policy_approval_status(policy) == POLICY_APPROVAL_APPROVED)
            .filter(|policy| {
                policy_matches_subject(policy, &execution.subject_type, &execution.subject_key)
            })
        {
            let mut policy_execution =
                self.evaluate_policy_definition(&policy, stage, runtime_context, None)?;
            adapt_policy_execution_for_automatic_application(&policy, &mut policy_execution);
            execution.checks.extend(policy_execution.checks);
            execution
                .ontology_refs
                .extend(policy_execution.ontology_refs);
            execution.shape_refs.extend(policy_execution.shape_refs);
            execution.policy_refs.extend(policy_execution.policy_refs);
            execution
                .schema_hashes
                .extend(policy_execution.schema_hashes);
        }

        dedupe_strings(&mut execution.ontology_refs);
        dedupe_strings(&mut execution.shape_refs);
        dedupe_strings(&mut execution.policy_refs);
        Ok(())
    }

    fn enforce_contract_governance_policies(
        &self,
        stage: &str,
        contract: &Contract,
        approval_request: Option<&ContractApprovalRequestRecord>,
        evidence: &[ContractApprovalEvidenceRecord],
        extra_context: HashMap<String, Value>,
    ) -> Result<Vec<String>, SosValidationServiceError> {
        let mut runtime_context = self.contract_policy_runtime_context(contract)?;

        if let Some(request) = approval_request {
            runtime_context.insert(
                "contract_approval_request_id".to_string(),
                Value::String(request.request_id.clone()),
            );
            runtime_context.insert(
                "contract_approval_request_uri".to_string(),
                Value::String(projection::contract_approval_request_uri(
                    &request.contract_id,
                    &request.request_id,
                )),
            );
            runtime_context.insert(
                "contract_approval_request_status".to_string(),
                Value::String(request.status.clone()),
            );
            runtime_context.insert(
                "contract_approval_requested_lifecycle_state".to_string(),
                Value::String(request.requested_lifecycle_state.clone()),
            );
            if let Some(note) = &request.note {
                runtime_context.insert(
                    "contract_approval_request_note".to_string(),
                    Value::String(note.clone()),
                );
            }
        }

        let mut report_ids = evidence
            .iter()
            .map(|record| record.report_id.clone())
            .collect::<Vec<_>>();
        report_ids.sort();
        report_ids.dedup();
        runtime_context.insert(
            "contract_approval_evidence_count".to_string(),
            Value::from(report_ids.len() as u64),
        );
        runtime_context.insert(
            "contract_approval_evidence_report_ids".to_string(),
            Value::Array(report_ids.into_iter().map(Value::String).collect()),
        );
        runtime_context.extend(extra_context);

        let mut execution = ValidationExecution {
            subject_type: POLICY_TARGET_CONTRACT.to_string(),
            subject_key: format!("contract:{}", contract.contract_id),
            validation_type: stage.to_string(),
            checks: Vec::new(),
            confidence: 1.0,
            ontology_refs: Vec::new(),
            shape_refs: Vec::new(),
            policy_refs: Vec::new(),
            contract_refs: vec![
                stable_contract_ref(&contract.contract_id),
                contract_revision_ref(contract),
            ],
            schema_hashes: HashMap::new(),
        };
        self.apply_policies_for_stage(stage, &runtime_context, &mut execution)?;
        execution.confidence = compute_confidence(&execution.checks);

        let blocking_failures = execution
            .checks
            .iter()
            .filter(|check| !check.passed && check.severity.eq_ignore_ascii_case("error"))
            .map(|check| format!("{} ({})", check.check_name, check.description))
            .collect::<Vec<_>>();

        if blocking_failures.is_empty() {
            Ok(execution.policy_refs)
        } else {
            Err(SosValidationServiceError::InvalidRequest(format!(
                "Contract '{}' revision {} is blocked during '{}' by policy checks: {}",
                contract.contract_id,
                contract.revision,
                stage,
                blocking_failures.join("; ")
            )))
        }
    }

    fn contract_policy_runtime_context(
        &self,
        contract: &Contract,
    ) -> Result<HashMap<String, Value>, SosValidationServiceError> {
        let provider = lookup::get_interface(self, &contract.provider_interface_id)?;
        let consumer = lookup::get_interface(self, &contract.consumer_interface_id)?;
        let mut context = HashMap::from([
            (
                "contract_id".to_string(),
                Value::String(contract.contract_id.clone()),
            ),
            (
                "contract_uri".to_string(),
                Value::String(projection::contract_uri(&contract.contract_id)),
            ),
            (
                "contract_revision".to_string(),
                Value::from(contract.revision),
            ),
            (
                "contract_revision_ref".to_string(),
                Value::String(contract_revision_ref(contract)),
            ),
            (
                "contract_revision_uri".to_string(),
                Value::String(projection::contract_revision_uri(
                    &contract.contract_id,
                    contract.revision,
                )),
            ),
            (
                "contract_lifecycle_state".to_string(),
                Value::String(effective_contract_lifecycle_state(contract).to_string()),
            ),
            (
                "contract_approval_status".to_string(),
                Value::String(effective_contract_approval_status(contract).to_string()),
            ),
            (
                "provider_interface_id".to_string(),
                Value::String(provider.interface_id.clone()),
            ),
            (
                "provider_interface_uri".to_string(),
                Value::String(projection::interface_uri(&provider.interface_id)),
            ),
            (
                "consumer_interface_id".to_string(),
                Value::String(consumer.interface_id.clone()),
            ),
            (
                "consumer_interface_uri".to_string(),
                Value::String(projection::interface_uri(&consumer.interface_id)),
            ),
            (
                "provider_system_id".to_string(),
                Value::String(provider.system_id.clone()),
            ),
            (
                "provider_system_uri".to_string(),
                Value::String(projection::system_uri(&provider.system_id)),
            ),
            (
                "consumer_system_id".to_string(),
                Value::String(consumer.system_id.clone()),
            ),
            (
                "consumer_system_uri".to_string(),
                Value::String(projection::system_uri(&consumer.system_id)),
            ),
        ]);

        if let Some(requested_by) = &contract.approval_requested_by {
            context.insert(
                "contract_approval_requested_by".to_string(),
                Value::String(requested_by.clone()),
            );
        }
        if let Some(approved_by) = &contract.approved_by {
            context.insert(
                "contract_approved_by".to_string(),
                Value::String(approved_by.clone()),
            );
        }
        if let Some(signed_by) = &contract.signed_by {
            context.insert(
                "contract_signed_by".to_string(),
                Value::String(signed_by.clone()),
            );
        }

        if let Some(signature) = self
            .storage_manager
            .get_contract_signature(&contract.contract_id, contract.revision)
            .map_err(map_storage_error)?
        {
            let verified = verify_contract_signature(contract, &signature);
            context.insert(
                "contract_signature_algorithm".to_string(),
                Value::String(signature.signature_algorithm.clone()),
            );
            context.insert(
                "contract_signature_payload_hash".to_string(),
                Value::String(signature.payload_hash.clone()),
            );
            context.insert(
                "contract_signature_public_key".to_string(),
                Value::String(signature.public_key.clone()),
            );
            context.insert(
                "contract_signature_key_fingerprint".to_string(),
                Value::String(signature.key_fingerprint.clone()),
            );
            context.insert(
                "contract_signing_key_fingerprint".to_string(),
                Value::String(signature.key_fingerprint.clone()),
            );
            if let Some(signing_key_ref) = &signature.signing_key_ref {
                context.insert(
                    "contract_signing_key_ref".to_string(),
                    Value::String(signing_key_ref.clone()),
                );
            }
            if let Some(signing_key_version) = &signature.signing_key_version {
                context.insert(
                    "contract_signing_key_version".to_string(),
                    Value::String(signing_key_version.clone()),
                );
            }
            context.insert(
                "contract_signing_key_source".to_string(),
                Value::String(signature.signing_key_source.clone()),
            );
            context.insert(
                "contract_signature_verified".to_string(),
                Value::Bool(verified),
            );
        }

        Ok(context)
    }

    fn evaluate_policy_definition(
        &self,
        policy: &SosPolicy,
        stage: &str,
        runtime_context: &HashMap<String, Value>,
        subject_override: Option<(String, String, String)>,
    ) -> Result<ValidationExecution, SosValidationServiceError> {
        let rdf_store = self.rdf_store.as_ref().ok_or_else(|| {
            SosValidationServiceError::Unavailable(
                "RDF store is not available for policy validation".to_string(),
            )
        })?;

        let context = self.build_policy_context(policy, stage, runtime_context)?;
        validate_policy_placeholders(&policy.sparql_query, &context)?;

        let rendered_query = render_policy_query(&policy.sparql_query, &context)
            .map_err(map_policy_template_error)?;
        let results = rdf_store.query(&rendered_query).map_err(map_rdf_error)?;
        let evaluation = evaluate_policy_results(&rendered_query, &results, &context);
        let lifecycle_state = effective_policy_lifecycle_state(policy).to_string();
        let check_severity = if evaluation.passed {
            "info".to_string()
        } else {
            map_policy_severity(&policy.severity)
        };
        let check_name = format!("policy:{}", policy.policy_id);
        let stable_policy_ref = format!("policy:{}", policy.policy_id);
        let revision_policy_ref = policy_revision_ref(policy);
        let query_hash = format!("sha256:{}", sha256_string(&rendered_query));
        let (subject_type, subject_key, validation_type) = subject_override.unwrap_or_else(|| {
            (
                "policy".to_string(),
                format!("policy:{}", policy.policy_id),
                "policy_check".to_string(),
            )
        });

        Ok(ValidationExecution {
            subject_type,
            subject_key,
            validation_type,
            confidence: if evaluation.passed { 1.0 } else { 0.0 },
            checks: vec![ValidationCheckRecord {
                check_name,
                passed: evaluation.passed,
                severity: check_severity,
                description: if evaluation.passed {
                    format!(
                        "Policy '{}' passed during '{}' evaluation",
                        policy.policy_id, stage
                    )
                } else {
                    format!(
                        "Policy '{}' failed during '{}' evaluation with {} violation row(s)",
                        policy.policy_id, stage, evaluation.violation_count
                    )
                },
                details: Some(json!({
                    "policy_id": policy.policy_id,
                    "policy_revision": policy.revision,
                    "policy_revision_ref": revision_policy_ref.clone(),
                    "policy_created_by": policy.created_by.clone(),
                    "policy_updated_by": policy.updated_by.clone(),
                    "policy_lifecycle_state": lifecycle_state,
                    "policy_approval_status": effective_policy_approval_status(policy),
                    "policy_approval_requested_by": policy.approval_requested_by.clone(),
                    "policy_approval_requested_at": policy.approval_requested_at.as_ref().map(|value| value.to_rfc3339()),
                    "policy_approved_by": policy.approved_by.clone(),
                    "policy_approved_at": policy.approved_at.as_ref().map(|value| value.to_rfc3339()),
                    "policy_rejected_by": policy.rejected_by.clone(),
                    "policy_rejected_at": policy.rejected_at.as_ref().map(|value| value.to_rfc3339()),
                    "policy_rejection_reason": policy.rejection_reason.clone(),
                    "policy_active": policy_is_automatic(policy),
                    "stage": stage,
                    "enforcement_level": policy.enforcement_level,
                    "violation_count": evaluation.violation_count,
                    "evaluation": evaluation.details,
                })),
            }],
            ontology_refs: policy.ontology_refs.clone(),
            shape_refs: policy.shape_refs.clone(),
            policy_refs: vec![stable_policy_ref.clone(), revision_policy_ref.clone()],
            contract_refs: Vec::new(),
            schema_hashes: HashMap::from([
                (stable_policy_ref, query_hash.clone()),
                (revision_policy_ref, query_hash),
            ]),
        })
    }

    pub fn reconcile_graphs(&self) -> Result<(), SosValidationServiceError> {
        projection::reconcile_graphs(self)
    }

    pub fn project_system_upsert(&self, system: &System) -> Result<(), SosValidationServiceError> {
        projection::project_system_upsert(self, system)
    }

    pub fn project_system_delete(&self, system_id: &str) -> Result<(), SosValidationServiceError> {
        projection::project_system_delete(self, system_id)
    }

    pub fn project_interface_upsert(
        &self,
        interface: &Interface,
    ) -> Result<(), SosValidationServiceError> {
        projection::project_interface_upsert(self, interface)
    }

    pub fn project_interface_delete(
        &self,
        interface_id: &str,
    ) -> Result<(), SosValidationServiceError> {
        projection::project_interface_delete(self, interface_id)
    }

    pub fn project_contract_upsert(
        &self,
        contract: &Contract,
    ) -> Result<(), SosValidationServiceError> {
        projection::project_contract_upsert(self, contract)
    }

    pub fn project_contract_delete(
        &self,
        contract_id: &str,
    ) -> Result<(), SosValidationServiceError> {
        projection::project_contract_delete(self, contract_id)
    }

    fn record_validation_metrics(&self, validation_type: &str, result: &str, started: Instant) {
        if let Some(metrics) = &self.sos_metrics {
            metrics.record_validation(validation_type, result, started.elapsed().as_secs_f64());
        }
    }

    fn record_analytics_metrics(&self, operation: &str, started: Instant) {
        if let Some(metrics) = &self.sos_metrics {
            metrics.record_analytics(operation, started.elapsed().as_secs_f64());
        }
    }

    fn record_projection_metrics(&self, entity_type: &str, operation: &str, started: Instant) {
        if let Some(metrics) = &self.sos_metrics {
            metrics.record_projection(entity_type, operation, started.elapsed().as_secs_f64());
        }
    }

    fn record_report_persisted_metrics(&self, report: &ValidationReport) {
        if let Some(metrics) = &self.sos_metrics {
            metrics.record_report_persisted(&report.validation_type, &report.subject_type);
        }
    }

    fn record_reports_pruned_metrics(&self, count: usize) {
        if let Some(metrics) = &self.sos_metrics {
            metrics.record_reports_pruned("retention", count);
        }
    }

    fn observe_validation_history_length(&self, subject_type: &str, count: usize) {
        if let Some(metrics) = &self.sos_metrics {
            metrics.observe_history_length(subject_type, count);
        }
    }

    pub fn validate_interface_schema_payload(
        &self,
        interface_id: &str,
        data: Value,
        persist_report: bool,
    ) -> Result<ValidationResponse, SosValidationServiceError> {
        self.validate_spec(
            SosValidationSpec::DataValidation {
                interface_id: interface_id.to_string(),
                data,
            },
            if persist_report {
                ValidationExecutionOptions::persisted()
            } else {
                ValidationExecutionOptions::dry_run()
            },
        )
    }

    fn execute_spec(
        &self,
        spec: &SosValidationSpec,
    ) -> Result<ValidationExecution, SosValidationServiceError> {
        match spec {
            SosValidationSpec::InterfaceCompatibility {
                provider_interface_id,
                consumer_interface_id,
            } => {
                let provider = lookup::get_interface(self, provider_interface_id)?;
                let consumer = lookup::get_interface(self, consumer_interface_id)?;
                let contract = lookup::find_contract_between(
                    self,
                    provider_interface_id,
                    consumer_interface_id,
                )?;
                self.validate_interface_pair(&provider, &consumer, contract.as_ref())
            }
            SosValidationSpec::ContractCompliance { contract_id } => {
                self.validate_contract_compliance(contract_id)
            }
            SosValidationSpec::SystemIntegration {
                source_system_id,
                target_system_id,
            } => self.validate_system_integration(source_system_id, target_system_id),
            SosValidationSpec::PolicyCheck {
                sparql_query,
                context,
            } => self.validate_policy_check(sparql_query, context),
            SosValidationSpec::DataValidation { interface_id, data } => {
                self.validate_data_payload(interface_id, data)
            }
        }
    }

    fn validate_interface_pair(
        &self,
        provider: &Interface,
        consumer: &Interface,
        contract: Option<&Contract>,
    ) -> Result<ValidationExecution, SosValidationServiceError> {
        let schema_report = compare_interface_schemas(&provider.schema, &consumer.schema)
            .map_err(map_internal_error)?;
        let transformation_report = contract
            .map(|contract| validate_contract_transformation_rules(&contract.transformation_rules))
            .unwrap_or_default();
        let unit_report = validate_unit_compatibility(
            provider.unit_system.as_deref(),
            consumer.unit_system.as_deref(),
            transformation_report.unit_rule.as_ref(),
        );
        let coordinate_report = validate_coordinate_compatibility(
            provider.coordinate_system.as_deref(),
            consumer.coordinate_system.as_deref(),
            transformation_report.coordinate_rule.as_ref(),
        );
        let schema_transformability = evaluate_schema_transformability(
            &schema_report,
            transformation_report.field_mapping_rule.as_ref(),
        );

        let mut checks = Vec::new();
        checks.push(simple_check(
            "data_format",
            provider
                .data_format
                .eq_ignore_ascii_case(&consumer.data_format),
            "error",
            if provider
                .data_format
                .eq_ignore_ascii_case(&consumer.data_format)
            {
                format!(
                    "Data formats are aligned ({} -> {})",
                    provider.data_format, consumer.data_format
                )
            } else {
                format!(
                    "Data formats differ ({} -> {})",
                    provider.data_format, consumer.data_format
                )
            },
        ));
        checks.push(simple_check(
            "schema_compatibility",
            schema_report.compatible,
            "error",
            if schema_report.compatible {
                "Provider schema satisfies consumer requirements".to_string()
            } else {
                schema_report.issues.join("; ")
            },
        ));
        checks.push(simple_check(
            "schema_transformability",
            schema_transformability.transformable,
            if schema_transformability.transformable {
                "info"
            } else {
                "warning"
            },
            if schema_report.compatible {
                "No schema transformations are required".to_string()
            } else if schema_transformability.transformable {
                format!(
                    "Explicit field mappings cover schema gaps for: {}",
                    schema_transformability.covered_paths.join(", ")
                )
            } else if schema_transformability.covered_paths.is_empty() {
                "No explicit field mappings cover the current schema gaps".to_string()
            } else {
                format!(
                    "Explicit field mappings cover {} but unresolved schema gaps remain: {}",
                    schema_transformability.covered_paths.join(", "),
                    schema_transformability.uncovered_issues.join("; ")
                )
            },
        ));
        checks.push(simple_check_with_details(
            "unit_compatibility",
            unit_report.compatible,
            unit_report.severity(),
            unit_report.explanation,
            Some(transform_compatibility_details(
                &unit_report.compatibility_mode,
                unit_report.declared_error_budget.as_ref(),
                unit_report.confidence_score,
            )),
        ));
        checks.push(simple_check_with_details(
            "coordinate_compatibility",
            coordinate_report.compatible,
            coordinate_report.severity(),
            coordinate_report.explanation,
            Some(transform_compatibility_details(
                &coordinate_report.compatibility_mode,
                coordinate_report.declared_error_budget.as_ref(),
                coordinate_report.confidence_score,
            )),
        ));
        if contract.is_some() && !transformation_report.valid {
            checks.push(simple_check(
                "transformation_rules",
                false,
                "error",
                transformation_report.issues.join("; "),
            ));
        }
        checks.push(simple_check(
            "contract_alignment",
            contract.is_some(),
            if contract.is_some() {
                "info"
            } else {
                "warning"
            },
            match contract {
                Some(contract) => format!(
                    "Compatibility is governed by contract '{}'",
                    contract.contract_id
                ),
                None => "No governing contract found for this interface pair".to_string(),
            },
        ));

        let (mut ontology_refs, mut shape_refs, ontology_checks) =
            self.collect_semantic_refs(provider, &hash_json(&provider.schema)?)?;
        let (consumer_ontology_refs, consumer_shape_refs, consumer_ontology_checks) =
            self.collect_semantic_refs(consumer, &hash_json(&consumer.schema)?)?;
        ontology_refs.extend(consumer_ontology_refs);
        shape_refs.extend(consumer_shape_refs);
        checks.extend(ontology_checks);
        checks.extend(consumer_ontology_checks);

        dedupe_strings(&mut ontology_refs);
        dedupe_strings(&mut shape_refs);

        let schema_hashes =
            self.current_interface_pair_schema_hashes(provider, consumer, contract)?;
        let mut contract_refs = Vec::new();

        let mut runtime_context = HashMap::from([
            (
                "provider_interface_id".to_string(),
                Value::String(provider.interface_id.clone()),
            ),
            (
                "provider_interface_uri".to_string(),
                Value::String(projection::interface_uri(&provider.interface_id)),
            ),
            (
                "consumer_interface_id".to_string(),
                Value::String(consumer.interface_id.clone()),
            ),
            (
                "consumer_interface_uri".to_string(),
                Value::String(projection::interface_uri(&consumer.interface_id)),
            ),
            (
                "provider_system_id".to_string(),
                Value::String(provider.system_id.clone()),
            ),
            (
                "provider_system_uri".to_string(),
                Value::String(projection::system_uri(&provider.system_id)),
            ),
            (
                "consumer_system_id".to_string(),
                Value::String(consumer.system_id.clone()),
            ),
            (
                "consumer_system_uri".to_string(),
                Value::String(projection::system_uri(&consumer.system_id)),
            ),
        ]);
        if let Some(contract) = contract {
            let stable_ref = stable_contract_ref(&contract.contract_id);
            let revision_ref = contract_revision_ref(contract);
            contract_refs.push(stable_ref);
            contract_refs.push(revision_ref.clone());
            runtime_context.insert(
                "contract_id".to_string(),
                Value::String(contract.contract_id.clone()),
            );
            runtime_context.insert(
                "contract_uri".to_string(),
                Value::String(projection::contract_uri(&contract.contract_id)),
            );
            runtime_context.insert(
                "contract_revision".to_string(),
                Value::from(contract.revision),
            );
            runtime_context.insert(
                "contract_revision_ref".to_string(),
                Value::String(revision_ref),
            );
            runtime_context.insert(
                "contract_lifecycle_state".to_string(),
                Value::String(effective_contract_lifecycle_state(contract).to_string()),
            );
            runtime_context.insert(
                "contract_approval_status".to_string(),
                Value::String(effective_contract_approval_status(contract).to_string()),
            );
            if let Some(requested_by) = &contract.approval_requested_by {
                runtime_context.insert(
                    "contract_approval_requested_by".to_string(),
                    Value::String(requested_by.clone()),
                );
            }
            if let Some(approved_by) = &contract.approved_by {
                runtime_context.insert(
                    "contract_approved_by".to_string(),
                    Value::String(approved_by.clone()),
                );
            }
            if let Some(signed_by) = &contract.signed_by {
                runtime_context.insert(
                    "contract_signed_by".to_string(),
                    Value::String(signed_by.clone()),
                );
            }
        }

        let mut execution = ValidationExecution {
            subject_type: "interface_pair".to_string(),
            subject_key: format!(
                "interface_pair:{}:{}",
                provider.interface_id, consumer.interface_id
            ),
            validation_type: "interface_compatibility".to_string(),
            confidence: compute_confidence(&checks),
            checks,
            ontology_refs,
            shape_refs,
            policy_refs: Vec::new(),
            contract_refs,
            schema_hashes,
        };
        self.apply_policies_for_stage(
            POLICY_STAGE_PRE_EXECUTION,
            &runtime_context,
            &mut execution,
        )?;
        execution.confidence = compute_confidence(&execution.checks);
        Ok(execution)
    }

    fn current_interface_pair_schema_hashes(
        &self,
        provider: &Interface,
        consumer: &Interface,
        contract: Option<&Contract>,
    ) -> Result<HashMap<String, String>, SosValidationServiceError> {
        let mut schema_hashes = HashMap::new();
        schema_hashes.insert(provider.interface_id.clone(), hash_json(&provider.schema)?);
        schema_hashes.insert(consumer.interface_id.clone(), hash_json(&consumer.schema)?);

        if let Some(contract) = contract {
            let contract_hash = hash_json(&json!({
                "revision": contract.revision,
                "provider_interface_id": contract.provider_interface_id,
                "consumer_interface_id": contract.consumer_interface_id,
                "sla_metrics": contract.sla_metrics,
                "transformation_rules": contract.transformation_rules,
            }))?;
            schema_hashes.insert(
                stable_contract_ref(&contract.contract_id),
                contract_hash.clone(),
            );
            schema_hashes.insert(contract_revision_ref(contract), contract_hash);
        }

        Ok(schema_hashes)
    }

    fn validate_contract_compliance(
        &self,
        contract_id: &str,
    ) -> Result<ValidationExecution, SosValidationServiceError> {
        let contract = lookup::get_contract(self, contract_id)?;
        let provider = lookup::get_interface(self, &contract.provider_interface_id)?;
        let consumer = lookup::get_interface(self, &contract.consumer_interface_id)?;

        self.validate_contract_compliance_for_entities(&contract, &provider, &consumer)
    }

    fn validate_contract_compliance_for_entities(
        &self,
        contract: &Contract,
        provider: &Interface,
        consumer: &Interface,
    ) -> Result<ValidationExecution, SosValidationServiceError> {
        let mut execution = self.validate_interface_pair(provider, consumer, Some(contract))?;
        execution.subject_type = "contract".to_string();
        execution.subject_key = format!("contract:{}", contract.contract_id);
        execution.validation_type = "contract_compliance".to_string();
        execution.contract_refs = vec![
            stable_contract_ref(&contract.contract_id),
            contract_revision_ref(contract),
        ];
        let contract_lifecycle_state = effective_contract_lifecycle_state(contract).to_string();

        execution.checks.push(simple_check(
            "contract_approved",
            contract.approved,
            "error",
            if contract.approved {
                format!("Contract '{}' is approved", contract.contract_id)
            } else {
                format!("Contract '{}' has not been approved", contract.contract_id)
            },
        ));
        execution.checks.push(simple_check(
            "contract_signed",
            contract.signed,
            "error",
            if contract.signed {
                format!("Contract '{}' is signed", contract.contract_id)
            } else {
                format!("Contract '{}' has not been signed", contract.contract_id)
            },
        ));
        execution.checks.push(simple_check(
            "sla_compliance",
            validate_sla_metrics(&contract.sla_metrics).is_ok(),
            "error",
            match validate_sla_metrics(&contract.sla_metrics) {
                Ok(()) => format!(
                    "{} SLA metric(s) validated successfully",
                    contract.sla_metrics.len()
                ),
                Err(error) => format!("Invalid SLA metric configuration: {}", error),
            },
        ));

        if let Some(policy_query) = contract
            .transformation_rules
            .get("policy_query")
            .and_then(Value::as_str)
        {
            let mut context = HashMap::new();
            context.insert(
                "contract_id".to_string(),
                Value::String(contract.contract_id.clone()),
            );
            context.insert(
                "contract_revision".to_string(),
                Value::from(contract.revision),
            );
            context.insert(
                "contract_lifecycle_state".to_string(),
                Value::String(contract_lifecycle_state.clone()),
            );
            context.insert("severity".to_string(), Value::String("High".to_string()));
            let policy_execution = self.validate_policy_check(policy_query, &context)?;
            execution.policy_refs.extend(policy_execution.policy_refs);
            execution.checks.extend(policy_execution.checks);
        }

        let mut runtime_context = HashMap::from([
            (
                "contract_id".to_string(),
                Value::String(contract.contract_id.clone()),
            ),
            (
                "contract_uri".to_string(),
                Value::String(projection::contract_uri(&contract.contract_id)),
            ),
            (
                "contract_revision".to_string(),
                Value::from(contract.revision),
            ),
            (
                "contract_revision_ref".to_string(),
                Value::String(contract_revision_ref(contract)),
            ),
            (
                "contract_lifecycle_state".to_string(),
                Value::String(contract_lifecycle_state),
            ),
            (
                "contract_approval_status".to_string(),
                Value::String(effective_contract_approval_status(contract).to_string()),
            ),
            (
                "provider_interface_id".to_string(),
                Value::String(provider.interface_id.clone()),
            ),
            (
                "provider_interface_uri".to_string(),
                Value::String(projection::interface_uri(&provider.interface_id)),
            ),
            (
                "consumer_interface_id".to_string(),
                Value::String(consumer.interface_id.clone()),
            ),
            (
                "consumer_interface_uri".to_string(),
                Value::String(projection::interface_uri(&consumer.interface_id)),
            ),
            (
                "provider_system_id".to_string(),
                Value::String(provider.system_id.clone()),
            ),
            (
                "provider_system_uri".to_string(),
                Value::String(projection::system_uri(&provider.system_id)),
            ),
            (
                "consumer_system_id".to_string(),
                Value::String(consumer.system_id.clone()),
            ),
            (
                "consumer_system_uri".to_string(),
                Value::String(projection::system_uri(&consumer.system_id)),
            ),
        ]);
        if let Some(approved_by) = &contract.approved_by {
            runtime_context.insert(
                "contract_approved_by".to_string(),
                Value::String(approved_by.clone()),
            );
        }
        if let Some(requested_by) = &contract.approval_requested_by {
            runtime_context.insert(
                "contract_approval_requested_by".to_string(),
                Value::String(requested_by.clone()),
            );
        }
        if let Some(signed_by) = &contract.signed_by {
            runtime_context.insert(
                "contract_signed_by".to_string(),
                Value::String(signed_by.clone()),
            );
        }
        self.apply_policies_for_stage(
            POLICY_STAGE_PRE_EXECUTION,
            &runtime_context,
            &mut execution,
        )?;
        execution.confidence = compute_confidence(&execution.checks);
        Ok(execution)
    }

    fn validate_system_integration(
        &self,
        source_system_id: &str,
        target_system_id: &str,
    ) -> Result<ValidationExecution, SosValidationServiceError> {
        let _source = lookup::get_system(self, source_system_id)?;
        let _target = lookup::get_system(self, target_system_id)?;
        let source_interfaces = self
            .storage_manager
            .list_interfaces_by_system(source_system_id)
            .map_err(map_storage_error)?;
        let target_interfaces = self
            .storage_manager
            .list_interfaces_by_system(target_system_id)
            .map_err(map_storage_error)?;

        self.validate_system_integration_for_catalog(
            source_system_id,
            target_system_id,
            &source_interfaces,
            &target_interfaces,
            |provider_interface_id, consumer_interface_id| {
                lookup::find_contract_between(self, provider_interface_id, consumer_interface_id)
            },
        )
    }

    fn validate_system_integration_for_catalog<F>(
        &self,
        source_system_id: &str,
        target_system_id: &str,
        source_interfaces: &[Interface],
        target_interfaces: &[Interface],
        mut find_contract: F,
    ) -> Result<ValidationExecution, SosValidationServiceError>
    where
        F: FnMut(&str, &str) -> Result<Option<Contract>, SosValidationServiceError>,
    {
        let mut checks = vec![
            simple_check(
                "source_interfaces_present",
                !source_interfaces.is_empty(),
                "error",
                format!(
                    "Source system '{}' exposes {} interface(s)",
                    source_system_id,
                    source_interfaces.len()
                ),
            ),
            simple_check(
                "target_interfaces_present",
                !target_interfaces.is_empty(),
                "error",
                format!(
                    "Target system '{}' exposes {} interface(s)",
                    target_system_id,
                    target_interfaces.len()
                ),
            ),
        ];

        let mut compatible_contracts = 0usize;
        let mut ontology_refs = Vec::new();
        let mut shape_refs = Vec::new();
        let mut policy_refs = Vec::new();
        let mut contract_refs = Vec::new();
        let mut schema_hashes = HashMap::new();

        for provider in source_interfaces {
            for consumer in target_interfaces {
                if let Some(contract) =
                    find_contract(&provider.interface_id, &consumer.interface_id)?
                {
                    let compatibility =
                        self.validate_interface_pair(provider, consumer, Some(&contract))?;
                    if execution_passed(&compatibility.checks) {
                        compatible_contracts += 1;
                    }
                    let path_passed = execution_passed(&compatibility.checks);
                    let path_confidence = compute_confidence(&compatibility.checks);
                    checks.push(simple_check_with_details(
                        format!(
                            "contract_path:{}:{}",
                            provider.interface_id, consumer.interface_id
                        ),
                        path_passed,
                        if path_passed {
                            "info"
                        } else {
                            "error"
                        },
                        format!(
                            "Interface path '{} -> {}' evaluated via contract '{}'",
                            provider.interface_id, consumer.interface_id, contract.contract_id
                        ),
                        Some(json!({
                            "nested_validation_type": "interface_compatibility",
                            "provider_interface_id": provider.interface_id.clone(),
                            "consumer_interface_id": consumer.interface_id.clone(),
                            "contract_id": contract.contract_id.clone(),
                            "compatibility_state": derive_interface_compatibility_state(&compatibility.checks),
                            "confidence_score": path_confidence,
                            "confidence_source": "nested_validation",
                            "confidence_category": if path_passed {
                                "nested_interface_validation"
                            } else {
                                "nested_interface_validation_failure"
                            },
                            "confidence_reason": if path_passed {
                                "System-integration path confidence is derived from the nested interface-compatibility result"
                            } else {
                                "System-integration path confidence is reduced by the nested interface-compatibility failure"
                            },
                        })),
                    ));
                    ontology_refs.extend(compatibility.ontology_refs);
                    shape_refs.extend(compatibility.shape_refs);
                    policy_refs.extend(compatibility.policy_refs);
                    contract_refs.extend(compatibility.contract_refs);
                    schema_hashes.extend(compatibility.schema_hashes);
                }
            }
        }

        checks.push(simple_check(
            "contracted_integration_path",
            compatible_contracts > 0,
            "error",
            if compatible_contracts > 0 {
                format!(
                    "Found {} compatible contract-backed integration path(s)",
                    compatible_contracts
                )
            } else {
                "No compatible contract-backed integration path found".to_string()
            },
        ));

        dedupe_strings(&mut ontology_refs);
        dedupe_strings(&mut shape_refs);
        dedupe_strings(&mut policy_refs);
        dedupe_strings(&mut contract_refs);

        let mut execution = ValidationExecution {
            subject_type: "system_pair".to_string(),
            subject_key: format!("system_pair:{}:{}", source_system_id, target_system_id),
            validation_type: "system_integration".to_string(),
            confidence: compute_confidence(&checks),
            checks,
            ontology_refs,
            shape_refs,
            policy_refs,
            contract_refs,
            schema_hashes,
        };
        let runtime_context = HashMap::from([
            (
                "source_system_id".to_string(),
                Value::String(source_system_id.to_string()),
            ),
            (
                "source_system_uri".to_string(),
                Value::String(projection::system_uri(source_system_id)),
            ),
            (
                "target_system_id".to_string(),
                Value::String(target_system_id.to_string()),
            ),
            (
                "target_system_uri".to_string(),
                Value::String(projection::system_uri(target_system_id)),
            ),
            (
                "source_interface_count".to_string(),
                Value::from(source_interfaces.len() as u64),
            ),
            (
                "target_interface_count".to_string(),
                Value::from(target_interfaces.len() as u64),
            ),
        ]);
        self.apply_policies_for_stage(
            POLICY_STAGE_PRE_EXECUTION,
            &runtime_context,
            &mut execution,
        )?;
        execution.confidence = compute_confidence(&execution.checks);
        Ok(execution)
    }

    fn validate_policy_check(
        &self,
        sparql_query: &str,
        context: &HashMap<String, Value>,
    ) -> Result<ValidationExecution, SosValidationServiceError> {
        let rdf_store = self.rdf_store.as_ref().ok_or_else(|| {
            SosValidationServiceError::Unavailable(
                "RDF store is not available for policy validation".to_string(),
            )
        })?;

        validate_policy_placeholders(sparql_query, context)?;
        let rendered_query =
            render_policy_query(sparql_query, context).map_err(map_policy_template_error)?;
        let results = rdf_store.query(&rendered_query).map_err(map_rdf_error)?;
        let evaluation = evaluate_policy_results(&rendered_query, &results, context);
        let policy_hash = sha256_string(&rendered_query);
        let severity = if evaluation.passed {
            "info".to_string()
        } else {
            evaluation.severity.clone()
        };

        Ok(ValidationExecution {
            subject_type: "policy".to_string(),
            subject_key: format!("policy:{}", policy_hash),
            validation_type: "policy_check".to_string(),
            confidence: if evaluation.passed { 1.0 } else { 0.0 },
            checks: vec![ValidationCheckRecord {
                check_name: context
                    .get("policy_id")
                    .and_then(Value::as_str)
                    .unwrap_or("policy_query")
                    .to_string(),
                passed: evaluation.passed,
                severity,
                description: if evaluation.passed {
                    "Policy query returned no violations".to_string()
                } else {
                    format!(
                        "Policy query returned {} violation row(s)",
                        evaluation.violation_count
                    )
                },
                details: evaluation.details,
            }],
            ontology_refs: Vec::new(),
            shape_refs: Vec::new(),
            policy_refs: vec![format!("policy:{}", policy_hash)],
            contract_refs: Vec::new(),
            schema_hashes: HashMap::from([(
                "policy_query".to_string(),
                format!("sha256:{}", policy_hash),
            )]),
        })
    }

    fn validate_data_payload(
        &self,
        interface_id: &str,
        data: &Value,
    ) -> Result<ValidationExecution, SosValidationServiceError> {
        let interface = lookup::get_interface(self, interface_id)?;
        let errors =
            validate_data_against_schema(&interface.schema, data).map_err(map_internal_error)?;
        let schema_hash = hash_json(&interface.schema)?;
        let (ontology_refs, shape_refs, ontology_checks) =
            self.collect_semantic_refs(&interface, &schema_hash)?;

        let mut checks = vec![ValidationCheckRecord {
            check_name: "schema_validation".to_string(),
            passed: errors.is_empty(),
            severity: if errors.is_empty() {
                "info".to_string()
            } else {
                "error".to_string()
            },
            description: if errors.is_empty() {
                format!(
                    "Payload conforms to interface schema '{}'",
                    interface.interface_id
                )
            } else {
                format!("Payload violates interface schema: {}", errors.join("; "))
            },
            details: (!errors.is_empty()).then(|| json!({ "errors": errors })),
        }];
        checks.extend(ontology_checks);

        let mut execution = ValidationExecution {
            subject_type: "interface".to_string(),
            subject_key: format!("interface:{}", interface.interface_id),
            validation_type: "data_validation".to_string(),
            confidence: compute_confidence(&checks),
            checks,
            ontology_refs,
            shape_refs,
            policy_refs: Vec::new(),
            contract_refs: Vec::new(),
            schema_hashes: HashMap::from([(interface.interface_id.clone(), schema_hash)]),
        };
        let runtime_context = HashMap::from([
            (
                "interface_id".to_string(),
                Value::String(interface.interface_id.clone()),
            ),
            (
                "interface_uri".to_string(),
                Value::String(projection::interface_uri(&interface.interface_id)),
            ),
            (
                "system_id".to_string(),
                Value::String(interface.system_id.clone()),
            ),
            (
                "system_uri".to_string(),
                Value::String(projection::system_uri(&interface.system_id)),
            ),
            ("data".to_string(), data.clone()),
            ("payload".to_string(), data.clone()),
        ]);
        self.apply_policies_for_stage(POLICY_STAGE_IN_FLIGHT, &runtime_context, &mut execution)?;
        execution.confidence = compute_confidence(&execution.checks);
        Ok(execution)
    }

    fn collect_semantic_refs(
        &self,
        interface: &Interface,
        schema_hash: &str,
    ) -> Result<(Vec<String>, Vec<String>, Vec<ValidationCheckRecord>), SosValidationServiceError>
    {
        let mut ontology_refs = Vec::new();
        let mut shape_refs = vec![format!(
            "http://graphica.io/sos/interface/{}/shape/{}",
            interface.interface_id, schema_hash
        )];
        let mut checks = Vec::new();

        if let Some(shape_ref) = interface.metadata.get("shape_ref").and_then(Value::as_str) {
            shape_refs.push(shape_ref.to_string());
        }

        let registry = self.persisted_ontology_registry.as_ref();
        let ontology_ids = extract_string_list(&interface.metadata, "ontology_ids")
            .into_iter()
            .chain(extract_optional_string(&interface.metadata, "ontology_id"))
            .collect::<Vec<_>>();

        for ontology_id in ontology_ids {
            let found = registry
                .map(|registry| registry.get_ontology(&ontology_id).is_some())
                .unwrap_or(false);

            ontology_refs.push(ontology_id.clone());
            checks.push(simple_check(
                format!("ontology_ref:{}", ontology_id),
                found,
                if found { "info" } else { "error" },
                if found {
                    format!(
                        "Interface '{}' resolved ontology reference '{}'",
                        interface.interface_id, ontology_id
                    )
                } else {
                    format!(
                        "Interface '{}' references missing ontology '{}'",
                        interface.interface_id, ontology_id
                    )
                },
            ));
        }

        dedupe_strings(&mut ontology_refs);
        dedupe_strings(&mut shape_refs);

        Ok((ontology_refs, shape_refs, checks))
    }
}

pub fn create_sos_validation_callback(
    service: Arc<SosValidationService>,
) -> Arc<SosValidationCallback> {
    Arc::new(Box::new(
        move |config: &SosValidationConfig, context: &ExecutionContext| {
            let service = service.clone();
            let spec = config.validation.clone();
            let options = ValidationExecutionOptions {
                persist_report: config.persist_report,
                emit_graph_lineage: config.emit_graph_lineage,
                workflow_execution_id: extract_workflow_execution_id(context),
                workflow_step_id: extract_workflow_step_id(context),
            };

            Box::pin(async move {
                let response = service
                    .validate_spec(spec, options)
                    .map_err(anyhow::Error::msg)?;
                Ok(SosValidationStepResult {
                    validation_id: response.validation_id,
                    passed: response.passed,
                    checks: response
                        .checks
                        .into_iter()
                        .map(|check| WorkflowSosValidationCheck {
                            check_name: check.check_name,
                            passed: check.passed,
                            severity: check.severity,
                            description: check.description,
                            details: check.details,
                        })
                        .collect(),
                    confidence: response.confidence,
                    validated_at: response.validated_at,
                    report_id: response.report_id,
                })
            })
                as Pin<Box<dyn Future<Output = AnyhowResult<SosValidationStepResult>> + Send>>
        },
    ))
}

fn request_to_spec(request: ValidateRequest) -> SosValidationSpec {
    match request {
        ValidateRequest::InterfaceCompatibility {
            provider_interface_id,
            consumer_interface_id,
        } => SosValidationSpec::InterfaceCompatibility {
            provider_interface_id,
            consumer_interface_id,
        },
        ValidateRequest::ContractCompliance { contract_id } => {
            SosValidationSpec::ContractCompliance { contract_id }
        }
        ValidateRequest::SystemIntegration {
            source_system_id,
            target_system_id,
        } => SosValidationSpec::SystemIntegration {
            source_system_id,
            target_system_id,
        },
        ValidateRequest::PolicyCheck {
            sparql_query,
            context,
        } => SosValidationSpec::PolicyCheck {
            sparql_query,
            context,
        },
        ValidateRequest::DataValidation { interface_id, data } => {
            SosValidationSpec::DataValidation { interface_id, data }
        }
    }
}

fn validation_type_for_spec(spec: &SosValidationSpec) -> &'static str {
    match spec {
        SosValidationSpec::InterfaceCompatibility { .. } => "interface_compatibility",
        SosValidationSpec::ContractCompliance { .. } => "contract_compliance",
        SosValidationSpec::SystemIntegration { .. } => "system_integration",
        SosValidationSpec::PolicyCheck { .. } => "policy_check",
        SosValidationSpec::DataValidation { .. } => "data_validation",
    }
}

fn simple_check(
    check_name: impl Into<String>,
    passed: bool,
    severity: impl Into<String>,
    description: impl Into<String>,
) -> ValidationCheckRecord {
    simple_check_with_details(check_name, passed, severity, description, None)
}

fn simple_check_with_details(
    check_name: impl Into<String>,
    passed: bool,
    severity: impl Into<String>,
    description: impl Into<String>,
    details: Option<Value>,
) -> ValidationCheckRecord {
    ValidationCheckRecord {
        check_name: check_name.into(),
        passed,
        severity: severity.into(),
        description: description.into(),
        details,
    }
}

fn compute_confidence(checks: &[ValidationCheckRecord]) -> f64 {
    if checks.is_empty() {
        return 1.0;
    }

    let total = checks.iter().map(check_confidence_score).sum::<f64>();
    total / checks.len() as f64
}

fn annotate_checks_with_confidence(checks: &mut [ValidationCheckRecord]) {
    for check in checks {
        annotate_check_with_confidence(check);
    }
}

fn annotate_check_with_confidence(check: &mut ValidationCheckRecord) {
    let assessment = assess_check_confidence(check);
    let mut details = take_details_object(&mut check.details);
    details.insert("confidence_score".to_string(), json!(assessment.score));
    details
        .entry("confidence_category".to_string())
        .or_insert_with(|| Value::String(assessment.category.clone()));
    details
        .entry("confidence_source".to_string())
        .or_insert_with(|| Value::String(assessment.source.clone()));
    details
        .entry("confidence_reason".to_string())
        .or_insert_with(|| Value::String(assessment.reason.clone()));
    check.details = Some(Value::Object(details));
}

fn derive_interface_compatibility_state(
    checks: &[ValidationCheckRecord],
) -> Option<CompatibilityState> {
    let check_map: HashMap<&str, &ValidationCheckRecord> = checks
        .iter()
        .map(|check| (check.check_name.as_str(), check))
        .collect();

    let data_format = check_map.get("data_format")?;
    let schema = check_map.get("schema_compatibility")?;
    let schema_transformability = check_map.get("schema_transformability");
    let unit = check_map.get("unit_compatibility")?;
    let coordinate = check_map.get("coordinate_compatibility")?;
    let transformation_rules = check_map.get("transformation_rules");

    if !data_format.passed {
        return Some(CompatibilityState::Incompatible);
    }
    if matches!(transformation_rules, Some(check) if !check.passed) {
        return Some(CompatibilityState::Incompatible);
    }

    let unit_mode = compatibility_mode_from_check(unit);
    let coordinate_mode = compatibility_mode_from_check(coordinate);
    let any_transform = matches!(
        unit_mode,
        Some(TransformCompatibilityMode::BoundedTransform | TransformCompatibilityMode::UnboundedTransform)
    ) || matches!(
        coordinate_mode,
        Some(TransformCompatibilityMode::BoundedTransform | TransformCompatibilityMode::UnboundedTransform)
    );
    let any_metadata_absent = matches!(unit_mode, Some(TransformCompatibilityMode::MetadataAbsent))
        || matches!(coordinate_mode, Some(TransformCompatibilityMode::MetadataAbsent));

    let only_transformable_failure = !schema.passed
        && schema_transformability.map(|check| check.passed).unwrap_or(false)
        && unit.passed
        && coordinate.passed
        && checks.iter().all(|check| {
            check.passed
                || !check.severity.eq_ignore_ascii_case("error")
                || check.check_name == "schema_compatibility"
        });
    if only_transformable_failure {
        return Some(CompatibilityState::Transformable);
    }

    if checks
        .iter()
        .any(|check| !check.passed && check.severity.eq_ignore_ascii_case("error"))
    {
        return Some(CompatibilityState::Incompatible);
    }

    if any_transform {
        return Some(CompatibilityState::Transformable);
    }

    if schema.passed && unit.passed && coordinate.passed {
        if any_metadata_absent {
            Some(CompatibilityState::SyntacticallyCompatible)
        } else {
            Some(CompatibilityState::SemanticallyEquivalent)
        }
    } else {
        Some(CompatibilityState::Incompatible)
    }
}

fn compatibility_mode_from_check(
    check: &ValidationCheckRecord,
) -> Option<TransformCompatibilityMode> {
    compatibility_mode_from_optional_check(check)
}

fn check_confidence_score(check: &ValidationCheckRecord) -> f64 {
    assess_check_confidence(check).score
}

fn transform_compatibility_details(
    compatibility_mode: &TransformCompatibilityMode,
    declared_error_budget: Option<&DeclaredErrorBudget>,
    confidence_score: f64,
) -> Value {
    let compatibility_mode = match compatibility_mode {
        TransformCompatibilityMode::DirectAlignment => "direct_alignment",
        TransformCompatibilityMode::MetadataAbsent => "metadata_absent",
        TransformCompatibilityMode::BoundedTransform => "bounded_transform",
        TransformCompatibilityMode::UnboundedTransform => "unbounded_transform",
    };

    let mut details = serde_json::Map::new();
    details.insert(
        "compatibility_mode".to_string(),
        Value::String(compatibility_mode.to_string()),
    );
    details.insert("confidence_score".to_string(), json!(confidence_score));
    details.insert(
        "requires_runtime_verification".to_string(),
        Value::Bool(matches!(
            compatibility_mode,
            "unbounded_transform"
        )),
    );
    if let Some(budget) = declared_error_budget {
        details.insert(
            "declared_error_budget".to_string(),
            json!({
                "value": budget.value,
                "label": budget.label,
            }),
        );
    }

    Value::Object(details)
}

fn build_confidence_assessment(checks: &[ValidationCheckRecord]) -> ConfidenceAssessment {
    let contributors = checks
        .iter()
        .map(|check| (check, assess_check_confidence(check)))
        .collect::<Vec<_>>();

    let passed_check_count = checks.iter().filter(|check| check.passed).count();
    let failed_check_count = checks.len().saturating_sub(passed_check_count);
    let warning_check_count = checks
        .iter()
        .filter(|check| check.severity.eq_ignore_ascii_case("warning"))
        .count();
    let runtime_verification_required = checks.iter().any(requires_runtime_verification);

    let mut material_contributors = contributors
        .iter()
        .filter(|(check, assessment)| {
            !check.passed
                || check.severity.eq_ignore_ascii_case("warning")
                || assessment.score < 0.999
                || requires_runtime_verification(check)
        })
        .map(|(check, assessment)| ConfidenceContributor {
            check_name: check.check_name.clone(),
            passed: check.passed,
            severity: check.severity.clone(),
            score: assessment.score,
            category: assessment.category.clone(),
            source: assessment.source.clone(),
            reason: assessment.reason.clone(),
        })
        .collect::<Vec<_>>();
    material_contributors.sort_by(|left, right| {
        left.score
            .partial_cmp(&right.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.check_name.cmp(&right.check_name))
    });

    ConfidenceAssessment {
        method: "per_check_average_v2".to_string(),
        summary: summarize_confidence_findings(
            checks,
            failed_check_count,
            warning_check_count,
            runtime_verification_required,
            &material_contributors,
        ),
        passed_check_count,
        failed_check_count,
        warning_check_count,
        runtime_verification_required,
        contributors: material_contributors,
    }
}

fn summarize_confidence_findings(
    checks: &[ValidationCheckRecord],
    failed_check_count: usize,
    warning_check_count: usize,
    runtime_verification_required: bool,
    contributors: &[ConfidenceContributor],
) -> String {
    if checks.is_empty() {
        return "No checks were executed, so confidence defaults to 1.0.".to_string();
    }

    if contributors.is_empty() {
        return "All checks passed without declared uncertainty.".to_string();
    }

    let blocking_failures = contributors
        .iter()
        .filter(|contributor| contributor.category == "blocking_failure")
        .count();
    let non_blocking_failures = contributors
        .iter()
        .filter(|contributor| {
            contributor.category == "non_blocking_policy_failure"
                || contributor.category == "warning_failure"
        })
        .count();
    let transform_contributors = contributors
        .iter()
        .filter(|contributor| {
            contributor.category == "bounded_transform"
                || contributor.category == "runtime_verification_required"
        })
        .count();
    let metadata_gaps = contributors
        .iter()
        .filter(|contributor| contributor.category == "metadata_absent")
        .count();

    let mut findings = Vec::new();
    if blocking_failures > 0 {
        findings.push(format!("{blocking_failures} blocking failure(s)"));
    }
    if non_blocking_failures > 0 {
        findings.push(format!("{non_blocking_failures} non-blocking warning failure(s)"));
    }
    if transform_contributors > 0 {
        findings.push(format!("{transform_contributors} transform-driven check(s)"));
    }
    if metadata_gaps > 0 {
        findings.push(format!("{metadata_gaps} metadata-gap check(s)"));
    }
    if runtime_verification_required {
        findings.push("runtime verification is still required".to_string());
    }
    if findings.is_empty() && warning_check_count > 0 {
        findings.push(format!("{warning_check_count} warning-severity check(s)"));
    }
    if findings.is_empty() && failed_check_count > 0 {
        findings.push(format!("{failed_check_count} failed check(s)"));
    }

    format!("Confidence reflects {}.", findings.join(", "))
}

#[derive(Debug, Clone)]
struct CheckConfidenceAssessment {
    score: f64,
    category: String,
    source: String,
    reason: String,
}

fn assess_check_confidence(check: &ValidationCheckRecord) -> CheckConfidenceAssessment {
    if let Some(score) = explicit_confidence_score(check) {
        let category = confidence_category(check);
        return CheckConfidenceAssessment {
            score,
            category: category.clone(),
            source: "explicit".to_string(),
            reason: confidence_reason(check, &category, score),
        };
    }

    let (score, category) = if !check.passed {
        if check.severity.eq_ignore_ascii_case("warning") {
            if is_non_blocking_policy_failure(check) {
                (0.65, "non_blocking_policy_failure".to_string())
            } else {
                (0.5, "warning_failure".to_string())
            }
        } else if check.severity.eq_ignore_ascii_case("info") {
            (0.75, "informational_failure".to_string())
        } else {
            (0.0, "blocking_failure".to_string())
        }
    } else if check.severity.eq_ignore_ascii_case("warning") {
        (0.85, "warning_pass".to_string())
    } else {
        (1.0, confidence_category(check))
    };

    CheckConfidenceAssessment {
        score,
        category: category.clone(),
        source: "derived".to_string(),
        reason: confidence_reason(check, &category, score),
    }
}

fn explicit_confidence_score(check: &ValidationCheckRecord) -> Option<f64> {
    check.details
        .as_ref()
        .and_then(|details| details.get("confidence_score"))
        .and_then(Value::as_f64)
        .filter(|score| (0.0..=1.0).contains(score))
}

fn confidence_category(check: &ValidationCheckRecord) -> String {
    if let Some(existing) = check
        .details
        .as_ref()
        .and_then(|details| details.get("confidence_category"))
        .and_then(Value::as_str)
    {
        return existing.to_string();
    }

    match compatibility_mode_from_optional_check(check) {
        Some(TransformCompatibilityMode::DirectAlignment) => "semantic_alignment".to_string(),
        Some(TransformCompatibilityMode::MetadataAbsent) => "metadata_absent".to_string(),
        Some(TransformCompatibilityMode::BoundedTransform) => "bounded_transform".to_string(),
        Some(TransformCompatibilityMode::UnboundedTransform) => {
            "runtime_verification_required".to_string()
        }
        None if is_non_blocking_policy_failure(check) => "non_blocking_policy_failure".to_string(),
        None if !check.passed && check.severity.eq_ignore_ascii_case("error") => {
            "blocking_failure".to_string()
        }
        None if !check.passed && check.severity.eq_ignore_ascii_case("warning") => {
            "warning_failure".to_string()
        }
        None if check.passed && check.severity.eq_ignore_ascii_case("warning") => {
            "warning_pass".to_string()
        }
        _ => "passed_check".to_string(),
    }
}

fn confidence_reason(check: &ValidationCheckRecord, category: &str, score: f64) -> String {
    if let Some(existing) = check
        .details
        .as_ref()
        .and_then(|details| details.get("confidence_reason"))
        .and_then(Value::as_str)
    {
        return existing.to_string();
    }

    let description = check.description.trim();
    match category {
        "bounded_transform" => format!(
            "{description}. Confidence is reduced because this check depends on a declared transform."
        ),
        "runtime_verification_required" => format!(
            "{description}. Confidence is reduced further because runtime verification is still required."
        ),
        "metadata_absent" => format!(
            "{description}. Confidence remains below semantic-equivalence because metadata is incomplete."
        ),
        "non_blocking_policy_failure" => {
            let mode = check
                .details
                .as_ref()
                .and_then(|details| details.get("policy_execution_mode"))
                .and_then(Value::as_str)
                .unwrap_or("non_blocking");
            format!(
                "{description}. This {mode} policy violation is non-blocking but still reduces confidence."
            )
        }
        "blocking_failure" => {
            format!("{description}. This blocking failure drives confidence to {score:.2}.")
        }
        "warning_failure" => format!(
            "{description}. This warning-level failure lowers confidence without blocking execution."
        ),
        "warning_pass" => format!(
            "{description}. The check passed, but warning severity keeps confidence below 1.0."
        ),
        "semantic_alignment" => {
            "Direct semantic alignment supports full confidence for this check.".to_string()
        }
        "nested_interface_validation" | "nested_interface_validation_failure" => {
            description.to_string()
        }
        _ => {
            if check.passed {
                "The check passed without declared uncertainty.".to_string()
            } else {
                description.to_string()
            }
        }
    }
}

fn compatibility_mode_from_optional_check(
    check: &ValidationCheckRecord,
) -> Option<TransformCompatibilityMode> {
    let raw = check
        .details
        .as_ref()
        .and_then(|details| details.get("compatibility_mode"))
        .and_then(Value::as_str)?;

    match raw {
        "direct_alignment" => Some(TransformCompatibilityMode::DirectAlignment),
        "metadata_absent" => Some(TransformCompatibilityMode::MetadataAbsent),
        "bounded_transform" => Some(TransformCompatibilityMode::BoundedTransform),
        "unbounded_transform" => Some(TransformCompatibilityMode::UnboundedTransform),
        _ => None,
    }
}

fn requires_runtime_verification(check: &ValidationCheckRecord) -> bool {
    check.details
        .as_ref()
        .and_then(|details| details.get("requires_runtime_verification"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn is_non_blocking_policy_failure(check: &ValidationCheckRecord) -> bool {
    !check.passed
        && check.severity.eq_ignore_ascii_case("warning")
        && check
            .details
            .as_ref()
            .and_then(|details| details.get("policy_blocking"))
            .and_then(Value::as_bool)
            == Some(false)
}

fn take_details_object(details: &mut Option<Value>) -> serde_json::Map<String, Value> {
    match details.take() {
        Some(Value::Object(map)) => map,
        Some(other) => {
            let mut map = serde_json::Map::new();
            map.insert("detail".to_string(), other);
            map
        }
        None => serde_json::Map::new(),
    }
}

fn execution_passed(checks: &[ValidationCheckRecord]) -> bool {
    !checks
        .iter()
        .any(|check| !check.passed && check.severity.eq_ignore_ascii_case("error"))
}

fn build_change_summary(
    previous: Option<&ValidationReport>,
    checks: &[ValidationCheckRecord],
    confidence: f64,
    schema_hashes: &HashMap<String, String>,
    policy_refs: &[String],
    contract_refs: &[String],
    shape_refs: &[String],
) -> ValidationChangeSummary {
    let Some(previous) = previous else {
        return ValidationChangeSummary {
            resolved_checks: Vec::new(),
            new_failures: checks
                .iter()
                .filter(|check| !check.passed)
                .map(|check| check.check_name.clone())
                .collect(),
            confidence_delta: 0.0,
            schema_or_policy_version_changed: false,
        };
    };

    let previous_map: HashMap<_, _> = previous
        .checks
        .iter()
        .map(|check| (check.check_name.as_str(), check))
        .collect();
    let current_map: HashMap<_, _> = checks
        .iter()
        .map(|check| (check.check_name.as_str(), check))
        .collect();

    let resolved_checks = previous_map
        .iter()
        .filter_map(|(name, old_check)| {
            let new_check = current_map.get(name)?;
            (!old_check.passed && new_check.passed).then(|| (*name).to_string())
        })
        .collect();

    let new_failures = current_map
        .iter()
        .filter_map(|(name, new_check)| {
            let old_failed = previous_map
                .get(name)
                .map(|old_check| !old_check.passed)
                .unwrap_or(false);
            (!new_check.passed && !old_failed).then(|| (*name).to_string())
        })
        .collect();

    ValidationChangeSummary {
        resolved_checks,
        new_failures,
        confidence_delta: confidence - previous.confidence,
        schema_or_policy_version_changed: previous.schema_hashes != *schema_hashes
            || previous.policy_refs != policy_refs
            || previous.contract_refs != contract_refs
            || previous.shape_refs != shape_refs,
    }
}

fn policy_revision_ref(policy: &SosPolicy) -> String {
    format!("policy:{}@{}", policy.policy_id, policy.revision)
}

fn to_report_response(mut report: ValidationReport) -> ValidationReportResponse {
    annotate_checks_with_confidence(&mut report.checks);
    let compatibility_state = if report.validation_type == "interface_compatibility" {
        derive_interface_compatibility_state(&report.checks)
    } else {
        None
    };
    let confidence_assessment = Some(build_confidence_assessment(&report.checks));

    ValidationReportResponse {
        report_id: report.report_id,
        validation_id: report.validation_id,
        subject_type: report.subject_type,
        subject_key: report.subject_key,
        validation_type: report.validation_type,
        passed: report.passed,
        confidence: report.confidence,
        compatibility_state,
        confidence_assessment,
        checks: report.checks.into_iter().map(into_api_check).collect(),
        validated_at: report.validated_at.to_rfc3339(),
        previous_report_id: report.previous_report_id,
        change_summary: ValidationChangeSummaryResponse {
            resolved_checks: report.change_summary.resolved_checks,
            new_failures: report.change_summary.new_failures,
            confidence_delta: report.change_summary.confidence_delta,
            schema_or_policy_version_changed: report
                .change_summary
                .schema_or_policy_version_changed,
        },
        workflow_execution_id: report.workflow_execution_id,
        workflow_step_id: report.workflow_step_id,
        ontology_refs: report.ontology_refs,
        shape_refs: report.shape_refs,
        policy_refs: report.policy_refs,
        contract_refs: report.contract_refs,
        schema_hashes: report.schema_hashes,
    }
}

impl SosValidationService {
    fn to_contract_response(
        &self,
        contract: Contract,
    ) -> Result<DataContractResponse, SosValidationServiceError> {
        let lifecycle_state = effective_contract_lifecycle_state(&contract).to_string();
        let approval_status = effective_contract_approval_status(&contract).to_string();
        let signature = self
            .storage_manager
            .get_contract_signature(&contract.contract_id, contract.revision)
            .map_err(map_storage_error)?
            .map(|signature| to_contract_signature_response(&contract, signature));
        let sla_metrics = contract
            .sla_metrics
            .into_iter()
            .map(|metric| crate::api::sos_validation::types::SlaMetric {
                name: metric.name,
                value: metric.value,
                operator: metric.operator,
                unit: metric.unit,
            })
            .collect();

        Ok(DataContractResponse {
            contract_id: contract.contract_id,
            revision: contract.revision,
            contract_name: contract.contract_name,
            provider_interface_id: contract.provider_interface_id,
            consumer_interface_id: contract.consumer_interface_id,
            sla_metrics,
            transformation_rules: contract.transformation_rules,
            description: contract.description,
            tags: contract.tags,
            approved: contract.approved,
            signed: contract.signed,
            lifecycle_state,
            approval_status,
            approval_requested_by: contract.approval_requested_by,
            approval_requested_at: contract
                .approval_requested_at
                .map(|value| value.to_rfc3339()),
            approved_by: contract.approved_by,
            approved_at: contract.approved_at.map(|value| value.to_rfc3339()),
            signed_by: contract.signed_by,
            signed_at: contract.signed_at.map(|value| value.to_rfc3339()),
            signature,
            created_by: contract.created_by,
            updated_by: contract.updated_by,
            rejected_by: contract.rejected_by,
            rejected_at: contract.rejected_at.map(|value| value.to_rfc3339()),
            rejection_reason: contract.rejection_reason,
            superseded_by_revision: contract.superseded_by_revision,
            created_at: contract.created_at.to_rfc3339(),
            updated_at: contract.updated_at.to_rfc3339(),
        })
    }

    fn to_policy_response(
        &self,
        policy: SosPolicy,
    ) -> Result<SosPolicyResponse, SosValidationServiceError> {
        let lifecycle_state = effective_policy_lifecycle_state(&policy).to_string();
        let approval_status = effective_policy_approval_status(&policy).to_string();
        let active = policy_is_automatic(&policy);
        let attestation = self
            .storage_manager
            .get_policy_attestation(&policy.policy_id, policy.revision)
            .map_err(map_storage_error)?
            .map(|attestation| to_policy_attestation_response(&policy, attestation));

        Ok(SosPolicyResponse {
            policy_id: policy.policy_id.clone(),
            revision: policy.revision,
            policy_ref: format!("policy:{}", policy.policy_id),
            policy_revision_ref: policy_revision_ref(&policy),
            policy_name: policy.policy_name,
            description: policy.description,
            lifecycle_state,
            approval_status,
            approval_requested_by: policy.approval_requested_by,
            approval_requested_at: policy.approval_requested_at.map(|value| value.to_rfc3339()),
            approved_by: policy.approved_by,
            approved_at: policy.approved_at.map(|value| value.to_rfc3339()),
            rejected_by: policy.rejected_by,
            rejected_at: policy.rejected_at.map(|value| value.to_rfc3339()),
            rejection_reason: policy.rejection_reason,
            target_type: policy.target_type,
            target_key: policy.target_key,
            stages: policy.stages,
            enforcement_level: policy.enforcement_level,
            severity: policy.severity,
            sparql_query: policy.sparql_query,
            context: policy.context,
            tags: policy.tags,
            ontology_refs: policy.ontology_refs,
            shape_refs: policy.shape_refs,
            active,
            provider_interface_id: policy.provider_interface_id,
            consumer_interface_id: policy.consumer_interface_id,
            contract_id: policy.contract_id,
            source_system_id: policy.source_system_id,
            target_system_id: policy.target_system_id,
            interface_id: policy.interface_id,
            attestation,
            created_by: policy.created_by,
            updated_by: policy.updated_by,
            superseded_by_revision: policy.superseded_by_revision,
            created_at: policy.created_at.to_rfc3339(),
            updated_at: policy.updated_at.to_rfc3339(),
        })
    }
}

fn to_contract_signature_response(
    contract: &Contract,
    signature: ContractSignatureRecord,
) -> SosContractSignatureResponse {
    let signature_verified = verify_contract_signature(contract, &signature);
    SosContractSignatureResponse {
        signature_id: signature.signature_id,
        contract_id: signature.contract_id,
        contract_revision: signature.contract_revision,
        contract_revision_ref: signature.contract_revision_ref,
        payload_hash: signature.payload_hash,
        payload_hash_algorithm: signature.payload_hash_algorithm,
        signature_algorithm: signature.signature_algorithm,
        signature: signature.signature,
        public_key: signature.public_key,
        key_fingerprint: signature.key_fingerprint,
        signing_key_ref: signature.signing_key_ref,
        signing_key_version: signature.signing_key_version,
        signing_key_source: signature.signing_key_source,
        signed_by: signature.signed_by,
        signed_at: signature.signed_at.to_rfc3339(),
        approval_request_id: signature.approval_request_id,
        evidence_ids: signature.evidence_ids,
        policy_refs: signature.policy_refs,
        signature_verified,
        metadata: signature.metadata,
    }
}

fn to_policy_attestation_response(
    policy: &SosPolicy,
    attestation: PolicyAttestationRecord,
) -> SosPolicyAttestationResponse {
    let attestation_verified = verify_policy_attestation(policy, &attestation);
    let trust_mode = metadata_string(
        &attestation.metadata,
        POLICY_ATTESTATION_TRUST_MODE_METADATA_KEY,
    )
    .unwrap_or_else(|| POLICY_ATTESTATION_DEFAULT_TRUST_MODE.to_string());
    let trust_provider = metadata_string(
        &attestation.metadata,
        POLICY_ATTESTATION_TRUST_PROVIDER_METADATA_KEY,
    );
    let external_key_ref = metadata_string(
        &attestation.metadata,
        POLICY_ATTESTATION_EXTERNAL_KEY_REF_METADATA_KEY,
    );
    let trust_attestation_ref = metadata_string(
        &attestation.metadata,
        POLICY_ATTESTATION_TRUST_ATTESTATION_REF_METADATA_KEY,
    );
    SosPolicyAttestationResponse {
        attestation_id: attestation.attestation_id,
        policy_id: attestation.policy_id,
        policy_revision: attestation.policy_revision,
        policy_revision_ref: attestation.policy_revision_ref,
        payload_hash: attestation.payload_hash,
        payload_hash_algorithm: attestation.payload_hash_algorithm,
        signature_algorithm: attestation.signature_algorithm,
        signature: attestation.signature,
        public_key: attestation.public_key,
        key_fingerprint: attestation.key_fingerprint,
        signing_key_ref: attestation.signing_key_ref,
        signing_key_version: attestation.signing_key_version,
        signing_key_source: attestation.signing_key_source,
        trust_mode,
        trust_provider,
        external_key_ref,
        trust_attestation_ref,
        attested_by: attestation.attested_by,
        attested_at: attestation.attested_at.to_rfc3339(),
        approval_request_id: attestation.approval_request_id,
        evidence_ids: attestation.evidence_ids,
        policy_refs: attestation.policy_refs,
        attestation_verified,
        metadata: attestation.metadata,
    }
}

fn metadata_string(metadata: &HashMap<String, serde_json::Value>, key: &str) -> Option<String> {
    metadata
        .get(key)
        .and_then(|value| value.as_str().map(ToString::to_string))
}

fn into_api_check(check: ValidationCheckRecord) -> CheckResult {
    CheckResult {
        check_name: check.check_name,
        passed: check.passed,
        severity: check.severity,
        description: check.description,
        details: check.details,
    }
}

fn map_storage_error(error: anyhow::Error) -> SosValidationServiceError {
    SosValidationServiceError::Internal(format!("SoS storage error: {}", error))
}

fn map_internal_error(error: anyhow::Error) -> SosValidationServiceError {
    SosValidationServiceError::Internal(error.to_string())
}

fn map_rdf_error(error: anyhow::Error) -> SosValidationServiceError {
    SosValidationServiceError::Internal(format!("RDF projection error: {}", error))
}

fn hash_json(value: &Value) -> Result<String, SosValidationServiceError> {
    let bytes = serde_json::to_vec(value).map_err(|error| {
        SosValidationServiceError::Internal(format!(
            "Failed to serialize JSON for hashing: {}",
            error
        ))
    })?;
    Ok(format!("sha256:{}", sha256_bytes(&bytes)))
}

fn sha256_string(value: &str) -> String {
    sha256_bytes(value.as_bytes())
}

fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn normalize_non_empty(
    field_name: &str,
    value: String,
) -> Result<String, SosValidationServiceError> {
    let normalized = value.trim().to_string();
    if normalized.is_empty() {
        Err(SosValidationServiceError::InvalidRequest(format!(
            "{} cannot be empty",
            field_name
        )))
    } else {
        Ok(normalized)
    }
}

fn normalize_policy_actor(
    field_name: &str,
    value: Option<String>,
) -> Result<String, SosValidationServiceError> {
    match value {
        Some(value) => normalize_non_empty(field_name, value),
        None => Ok("system".to_string()),
    }
}

fn normalize_policy_approval_status(raw: &str) -> Result<String, SosValidationServiceError> {
    let normalized = raw.trim().to_ascii_lowercase();
    match normalized.as_str() {
        POLICY_APPROVAL_PENDING | POLICY_APPROVAL_APPROVED | POLICY_APPROVAL_REJECTED => {
            Ok(normalized)
        }
        _ => Err(SosValidationServiceError::InvalidRequest(format!(
            "Unsupported policy approval_status '{}'",
            raw
        ))),
    }
}

fn normalize_policy_target_type(raw: &str) -> Result<String, SosValidationServiceError> {
    let normalized = raw.trim().to_ascii_lowercase();
    match normalized.as_str() {
        POLICY_TARGET_GLOBAL
        | POLICY_TARGET_INTERFACE_PAIR
        | POLICY_TARGET_CONTRACT
        | POLICY_TARGET_SYSTEM_PAIR
        | POLICY_TARGET_INTERFACE => Ok(normalized),
        _ => Err(SosValidationServiceError::InvalidRequest(format!(
            "Unsupported policy target_type '{}'",
            raw
        ))),
    }
}

fn normalize_policy_stage(raw: &str) -> Result<String, SosValidationServiceError> {
    let normalized = raw.trim().to_ascii_lowercase();
    match normalized.as_str() {
        POLICY_STAGE_PRE_EXECUTION
        | POLICY_STAGE_IN_FLIGHT
        | POLICY_STAGE_POST_EXECUTION
        | POLICY_STAGE_CONTRACT_APPROVAL
        | POLICY_STAGE_CONTRACT_SIGNING => Ok(normalized),
        _ => Err(SosValidationServiceError::InvalidRequest(format!(
            "Unsupported policy stage '{}'",
            raw
        ))),
    }
}

fn normalize_policy_lifecycle_state(raw: &str) -> Result<String, SosValidationServiceError> {
    let normalized = raw.trim().to_ascii_lowercase();
    match normalized.as_str() {
        POLICY_LIFECYCLE_DRAFT
        | POLICY_LIFECYCLE_DRY_RUN
        | POLICY_LIFECYCLE_ACTIVE
        | POLICY_LIFECYCLE_DEPRECATED
        | POLICY_LIFECYCLE_RETIRED => Ok(normalized),
        _ => Err(SosValidationServiceError::InvalidRequest(format!(
            "Unsupported policy lifecycle_state '{}'",
            raw
        ))),
    }
}

fn legacy_lifecycle_state_for_active(active: bool) -> String {
    if active {
        POLICY_LIFECYCLE_ACTIVE.to_string()
    } else {
        POLICY_LIFECYCLE_DRAFT.to_string()
    }
}

fn effective_policy_lifecycle_state(policy: &SosPolicy) -> &str {
    match policy.lifecycle_state.as_deref() {
        Some(POLICY_LIFECYCLE_DRAFT)
        | Some(POLICY_LIFECYCLE_DRY_RUN)
        | Some(POLICY_LIFECYCLE_ACTIVE)
        | Some(POLICY_LIFECYCLE_DEPRECATED)
        | Some(POLICY_LIFECYCLE_RETIRED) => policy.lifecycle_state.as_deref().unwrap(),
        Some(_) | None => {
            if policy.active {
                POLICY_LIFECYCLE_ACTIVE
            } else {
                POLICY_LIFECYCLE_DRAFT
            }
        }
    }
}

fn effective_policy_approval_status(policy: &SosPolicy) -> &str {
    match policy.approval_status.as_deref() {
        Some(POLICY_APPROVAL_PENDING)
        | Some(POLICY_APPROVAL_APPROVED)
        | Some(POLICY_APPROVAL_REJECTED) => policy.approval_status.as_deref().unwrap(),
        Some(_) | None => {
            if policy_is_automatic(policy)
                || policy.approved_by.is_some()
                || policy.approved_at.is_some()
            {
                POLICY_APPROVAL_APPROVED
            } else if policy.rejected_by.is_some() || policy.rejected_at.is_some() {
                POLICY_APPROVAL_REJECTED
            } else {
                POLICY_APPROVAL_PENDING
            }
        }
    }
}

fn policy_state_is_automatic(state: &str) -> bool {
    matches!(
        state,
        POLICY_LIFECYCLE_DRY_RUN | POLICY_LIFECYCLE_ACTIVE | POLICY_LIFECYCLE_DEPRECATED
    )
}

fn policy_is_automatic(policy: &SosPolicy) -> bool {
    policy_state_is_automatic(effective_policy_lifecycle_state(policy))
}

fn policy_state_is_enforced(state: &str) -> bool {
    matches!(state, POLICY_LIFECYCLE_ACTIVE | POLICY_LIFECYCLE_DEPRECATED)
}

fn initialize_policy_approval(policy: &mut SosPolicy, actor: &str, at: DateTime<Utc>) {
    policy.approval_requested_by = Some(actor.to_string());
    policy.approval_requested_at = Some(at);
    if policy_state_is_automatic(effective_policy_lifecycle_state(policy)) {
        set_policy_approval_approved(policy, actor, at);
    } else {
        set_policy_approval_pending(policy, actor, at);
    }
}

fn set_policy_approval_pending(policy: &mut SosPolicy, actor: &str, at: DateTime<Utc>) {
    policy.approval_status = Some(POLICY_APPROVAL_PENDING.to_string());
    policy.approval_requested_by = Some(actor.to_string());
    policy.approval_requested_at = Some(at);
    policy.approved_by = None;
    policy.approved_at = None;
    policy.rejected_by = None;
    policy.rejected_at = None;
    policy.rejection_reason = None;
}

fn set_policy_approval_approved(policy: &mut SosPolicy, actor: &str, at: DateTime<Utc>) {
    policy.approval_status = Some(POLICY_APPROVAL_APPROVED.to_string());
    if policy.approval_requested_by.is_none() {
        policy.approval_requested_by = Some(actor.to_string());
    }
    if policy.approval_requested_at.is_none() {
        policy.approval_requested_at = Some(at);
    }
    policy.approved_by = Some(actor.to_string());
    policy.approved_at = Some(at);
    policy.rejected_by = None;
    policy.rejected_at = None;
    policy.rejection_reason = None;
}

fn set_policy_approval_rejected(
    policy: &mut SosPolicy,
    actor: &str,
    at: DateTime<Utc>,
    reason: String,
) {
    policy.approval_status = Some(POLICY_APPROVAL_REJECTED.to_string());
    if policy.approval_requested_by.is_none() {
        policy.approval_requested_by = Some(actor.to_string());
    }
    if policy.approval_requested_at.is_none() {
        policy.approval_requested_at = Some(at);
    }
    policy.approved_by = None;
    policy.approved_at = None;
    policy.rejected_by = Some(actor.to_string());
    policy.rejected_at = Some(at);
    policy.rejection_reason = Some(reason);
}

fn validate_policy_lifecycle_transition(
    current: &str,
    next: &str,
) -> Result<(), SosValidationServiceError> {
    if current == POLICY_LIFECYCLE_RETIRED && next != POLICY_LIFECYCLE_RETIRED {
        return Err(SosValidationServiceError::InvalidRequest(format!(
            "Policy lifecycle_state cannot transition from '{}' back to '{}'",
            current, next
        )));
    }

    Ok(())
}

fn next_policy_lifecycle_state(
    current: &str,
    requested_state: Option<&str>,
    requested_active: Option<bool>,
) -> Result<String, SosValidationServiceError> {
    let next = if let Some(requested_state) = requested_state {
        normalize_policy_lifecycle_state(requested_state)?
    } else if let Some(requested_active) = requested_active {
        if requested_active {
            match current {
                POLICY_LIFECYCLE_DRAFT | POLICY_LIFECYCLE_DRY_RUN | POLICY_LIFECYCLE_RETIRED => {
                    POLICY_LIFECYCLE_ACTIVE.to_string()
                }
                _ => current.to_string(),
            }
        } else if current == POLICY_LIFECYCLE_RETIRED {
            POLICY_LIFECYCLE_RETIRED.to_string()
        } else {
            POLICY_LIFECYCLE_DRAFT.to_string()
        }
    } else {
        current.to_string()
    };

    validate_policy_lifecycle_transition(current, &next)?;
    Ok(next)
}

fn approval_endpoint_lifecycle_state(
    current: &str,
    requested_state: Option<&str>,
) -> Result<Option<String>, SosValidationServiceError> {
    let Some(requested_state) = requested_state else {
        return Ok(None);
    };
    if !policy_state_is_automatic(requested_state) {
        return Err(SosValidationServiceError::InvalidRequest(format!(
            "Policy approval can only promote a revision into an automatic lifecycle_state, not '{}'",
            requested_state
        )));
    }
    validate_policy_lifecycle_transition(current, requested_state)?;
    Ok(Some(requested_state.to_string()))
}

fn automatic_policy_execution_mode(policy: &SosPolicy) -> &'static str {
    let lifecycle_state = effective_policy_lifecycle_state(policy);
    if lifecycle_state == POLICY_LIFECYCLE_DRY_RUN {
        POLICY_LIFECYCLE_DRY_RUN
    } else if policy.enforcement_level.eq_ignore_ascii_case("advisory") {
        "advisory"
    } else if policy_state_is_enforced(lifecycle_state) {
        "enforced"
    } else {
        "manual_only"
    }
}

fn adapt_policy_execution_for_automatic_application(
    policy: &SosPolicy,
    execution: &mut ValidationExecution,
) {
    let lifecycle_state = effective_policy_lifecycle_state(policy).to_string();
    let execution_mode = automatic_policy_execution_mode(policy);
    let blocking = execution_mode == "enforced";

    for check in &mut execution.checks {
        if !check.passed && !blocking && check.severity.eq_ignore_ascii_case("error") {
            check.severity = "warning".to_string();
        }

        if !check.passed {
            match execution_mode {
                POLICY_LIFECYCLE_DRY_RUN => {
                    check.description = format!("Dry-run rollout: {}", check.description);
                }
                "advisory" => {
                    check.description =
                        format!("Advisory policy (non-blocking): {}", check.description);
                }
                _ => {}
            }
        }

        let mut details = match check.details.take() {
            Some(Value::Object(map)) => map,
            Some(other) => {
                let mut map = serde_json::Map::new();
                map.insert("detail".to_string(), other);
                map
            }
            None => serde_json::Map::new(),
        };
        details.insert(
            "policy_lifecycle_state".to_string(),
            Value::String(lifecycle_state.clone()),
        );
        details.insert(
            "policy_approval_status".to_string(),
            Value::String(effective_policy_approval_status(policy).to_string()),
        );
        details.insert(
            "policy_execution_mode".to_string(),
            Value::String(execution_mode.to_string()),
        );
        details.insert("policy_blocking".to_string(), Value::Bool(blocking));
        details.insert(
            "policy_active".to_string(),
            Value::Bool(policy_is_automatic(policy)),
        );
        check.details = Some(Value::Object(details));
    }
}

fn normalize_policy_stages(
    raw_stages: Vec<String>,
) -> Result<Vec<String>, SosValidationServiceError> {
    if raw_stages.is_empty() {
        return Err(SosValidationServiceError::InvalidRequest(
            "Policy must declare at least one stage".to_string(),
        ));
    }

    let mut stages = raw_stages
        .into_iter()
        .map(|stage| normalize_policy_stage(&stage))
        .collect::<Result<Vec<_>, _>>()?;
    dedupe_strings(&mut stages);
    Ok(stages)
}

fn normalize_policy_enforcement_level(raw: &str) -> Result<String, SosValidationServiceError> {
    let normalized = raw.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "mandatory" | "advisory" => Ok(normalized),
        _ => Err(SosValidationServiceError::InvalidRequest(format!(
            "Unsupported policy enforcement_level '{}'",
            raw
        ))),
    }
}

fn normalize_policy_severity(raw: &str) -> Result<String, SosValidationServiceError> {
    let normalized = raw.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "critical" | "high" | "medium" | "low" | "error" | "warning" | "info" => Ok(normalized),
        _ => Err(SosValidationServiceError::InvalidRequest(format!(
            "Unsupported policy severity '{}'",
            raw
        ))),
    }
}

fn validate_policy_placeholders(
    query: &str,
    context: &HashMap<String, Value>,
) -> Result<(), SosValidationServiceError> {
    let missing: Vec<_> = extract_policy_placeholders(query)
        .map_err(map_policy_template_error)?
        .into_iter()
        .filter(|placeholder| !context.contains_key(placeholder))
        .collect();

    if missing.is_empty() {
        Ok(())
    } else {
        Err(SosValidationServiceError::InvalidRequest(format!(
            "Policy query references missing template variables: {}",
            missing.join(", ")
        )))
    }
}

fn validate_policy_placeholders_for_definition(
    query: &str,
    context: &HashMap<String, Value>,
) -> Result<(), SosValidationServiceError> {
    let missing: Vec<_> = extract_policy_placeholders(query)
        .map_err(map_policy_template_error)?
        .into_iter()
        .filter(|placeholder| {
            !context.contains_key(placeholder) && !is_known_runtime_policy_placeholder(placeholder)
        })
        .collect();

    if missing.is_empty() {
        Ok(())
    } else {
        Err(SosValidationServiceError::InvalidRequest(format!(
            "Policy query references unknown template variables: {}",
            missing.join(", ")
        )))
    }
}

fn map_policy_template_error(error: PolicyQueryTemplateError) -> SosValidationServiceError {
    SosValidationServiceError::InvalidRequest(error.to_string())
}

fn is_known_runtime_policy_placeholder(placeholder: &str) -> bool {
    matches!(
        placeholder,
        "policy_id"
            | "policy_uri"
            | "policy_revision"
            | "policy_revision_ref"
            | "policy_revision_uri"
            | "policy_name"
            | "policy_created_by"
            | "policy_updated_by"
            | "policy_lifecycle_state"
            | "policy_approval_status"
            | "policy_approval_requested_by"
            | "policy_approval_requested_at"
            | "policy_approved_by"
            | "policy_approved_at"
            | "policy_rejected_by"
            | "policy_rejected_at"
            | "policy_rejection_reason"
            | "policy_active"
            | "target_type"
            | "target_key"
            | "stage"
            | "enforcement_level"
            | "severity"
            | "provider_interface_id"
            | "provider_interface_uri"
            | "consumer_interface_id"
            | "consumer_interface_uri"
            | "provider_system_id"
            | "provider_system_uri"
            | "consumer_system_id"
            | "consumer_system_uri"
            | "contract_id"
            | "contract_uri"
            | "contract_revision"
            | "contract_revision_ref"
            | "contract_revision_uri"
            | "contract_lifecycle_state"
            | "contract_approval_status"
            | "contract_approval_requested_by"
            | "contract_approved_by"
            | "contract_signed_by"
            | "source_system_id"
            | "source_system_uri"
            | "target_system_id"
            | "target_system_uri"
            | "interface_id"
            | "interface_uri"
            | "system_id"
            | "system_uri"
            | "data"
            | "payload"
            | "source_interface_count"
            | "target_interface_count"
            | "contract_approval_request_id"
            | "contract_approval_request_uri"
            | "contract_approval_request_status"
            | "contract_approval_request_note"
            | "contract_approval_requested_lifecycle_state"
            | "contract_approval_evidence_count"
            | "contract_approval_evidence_report_ids"
            | "contract_approval_actor"
            | "contract_signing_actor"
            | "contract_signing_key_ref"
            | "contract_signing_key_version"
            | "contract_signing_key_source"
            | "contract_signing_key_fingerprint"
            | "contract_signature_algorithm"
            | "contract_signature_payload_hash"
            | "contract_signature_public_key"
            | "contract_signature_key_fingerprint"
            | "contract_signature_verified"
    )
}

fn policy_matches_subject(policy: &SosPolicy, subject_type: &str, subject_key: &str) -> bool {
    if policy.target_type == POLICY_TARGET_GLOBAL {
        return true;
    }

    policy.target_type == subject_type && policy.target_key.as_deref() == Some(subject_key)
}

fn extract_optional_string(metadata: &HashMap<String, Value>, key: &str) -> Option<String> {
    metadata
        .get(key)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn extract_string_list(metadata: &HashMap<String, Value>, key: &str) -> Vec<String> {
    metadata
        .get(key)
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn dedupe_strings(values: &mut Vec<String>) {
    let mut seen = HashSet::new();
    values.retain(|value| seen.insert(value.clone()));
}

fn extract_workflow_execution_id(context: &ExecutionContext) -> Option<String> {
    context
        .row_lineage
        .as_ref()
        .map(|lineage| lineage.execution_id.clone())
        .or_else(|| context.metadata.get("workflow_execution_id").cloned())
        .or_else(|| context.metadata.get("execution_id").cloned())
}

fn extract_workflow_step_id(context: &ExecutionContext) -> Option<String> {
    context
        .row_lineage
        .as_ref()
        .and_then(|lineage| lineage.current_step_id.clone())
        .or_else(|| context.metadata.get("workflow_step_id").cloned())
        .or_else(|| context.metadata.get("step_id").cloned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::sos_validation::contract_governance::{
        set_contract_draft, CONTRACT_APPROVAL_APPROVED, CONTRACT_APPROVAL_REJECTED,
    };
    use crate::api::sos_validation::storage::{SlaMetric, System};
    use crate::governance::rdf_store::{GraphicaRdfStore, RdfStore};
    use serde_json::json;
    use tempfile::TempDir;

    fn sample_system(system_id: &str, system_name: &str) -> System {
        System {
            system_id: system_id.to_string(),
            system_name: system_name.to_string(),
            system_type: "sensor".to_string(),
            vendor: "Acme".to_string(),
            version: "1.0.0".to_string(),
            classification: "UNCLASSIFIED".to_string(),
            description: Some(format!("Synthetic system {system_id}")),
            deployment: HashMap::new(),
            capabilities: HashMap::new(),
            tags: vec!["sos".to_string()],
            active: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn sample_interface(interface_id: &str, system_id: &str, unit_system: &str) -> Interface {
        Interface {
            interface_id: interface_id.to_string(),
            system_id: system_id.to_string(),
            interface_name: format!("Interface {interface_id}"),
            direction: "bidirectional".to_string(),
            protocol: "https".to_string(),
            data_format: "json".to_string(),
            schema: json!({
                "type": "object",
                "required": ["sample_id", "score"],
                "properties": {
                    "sample_id": { "type": "string" },
                    "score": { "type": "number" }
                }
            }),
            coordinate_system: Some("WGS84".to_string()),
            unit_system: Some(unit_system.to_string()),
            metadata: HashMap::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn sample_contract(
        contract_id: &str,
        provider_interface_id: &str,
        consumer_interface_id: &str,
    ) -> Contract {
        let now = Utc::now();
        Contract {
            contract_id: contract_id.to_string(),
            revision: 1,
            contract_name: format!("Contract {contract_id}"),
            provider_interface_id: provider_interface_id.to_string(),
            consumer_interface_id: consumer_interface_id.to_string(),
            sla_metrics: vec![SlaMetric {
                name: "latency_ms".to_string(),
                value: 250.0,
                operator: "<=".to_string(),
                unit: Some("ms".to_string()),
            }],
            transformation_rules: HashMap::from([(
                "unit_transform".to_string(),
                json!({
                    "from": "SI",
                    "to": "Imperial",
                    "strategy": "linear_scale",
                    "scale": 3.28084,
                    "offset": 0.0
                }),
            )]),
            description: Some("Synthetic test contract".to_string()),
            tags: vec!["test".to_string()],
            approved: true,
            signed: true,
            lifecycle_state: Some("signed".to_string()),
            approval_status: Some("approved".to_string()),
            approval_requested_by: Some("system".to_string()),
            approval_requested_at: Some(now.clone()),
            approved_by: Some("system".to_string()),
            approved_at: Some(now.clone()),
            rejected_by: None,
            rejected_at: None,
            rejection_reason: None,
            signed_by: Some("system".to_string()),
            signed_at: Some(now.clone()),
            created_by: "system".to_string(),
            updated_by: "system".to_string(),
            superseded_by_revision: None,
            created_at: now.clone(),
            updated_at: now,
        }
    }

    fn create_service() -> (TempDir, Arc<SosStorageManager>, SosValidationService) {
        let temp_dir = TempDir::new().expect("temporary directory should be created");
        let storage_manager = Arc::new(
            SosStorageManager::new(
                temp_dir
                    .path()
                    .to_str()
                    .expect("temporary directory should be UTF-8"),
            )
            .expect("SoS storage manager should be created"),
        );
        let service = SosValidationService::new(storage_manager.clone(), None, None);
        (temp_dir, storage_manager, service)
    }

    fn create_service_with_rdf() -> (
        TempDir,
        Arc<SosStorageManager>,
        Arc<GraphicaRdfStore>,
        SosValidationService,
    ) {
        let temp_dir = TempDir::new().expect("temporary directory should be created");
        let storage_manager = Arc::new(
            SosStorageManager::new(
                temp_dir
                    .path()
                    .to_str()
                    .expect("temporary directory should be UTF-8"),
            )
            .expect("SoS storage manager should be created"),
        );
        let rdf_store = Arc::new(
            GraphicaRdfStore::new_in_memory().expect("in-memory RDF store should be created"),
        );
        let service =
            SosValidationService::new(storage_manager.clone(), Some(rdf_store.clone()), None);
        (temp_dir, storage_manager, rdf_store, service)
    }

    fn extract_count(results: &[serde_json::Value]) -> usize {
        let first = results.first().expect("query should return a count row");
        let count_value = first.get("count").expect("count binding should exist");

        match count_value {
            serde_json::Value::Number(value) => value
                .as_u64()
                .expect("count number should be an unsigned integer")
                as usize,
            serde_json::Value::String(value) => value
                .split('^')
                .next()
                .unwrap_or(value)
                .trim_matches('"')
                .parse::<usize>()
                .expect("count string should parse as usize"),
            other => panic!("unexpected count binding shape: {other:?}"),
        }
    }

    fn graph_triples(
        rdf_store: &GraphicaRdfStore,
        graph_uri: &str,
    ) -> Vec<(String, String, String)> {
        let mut triples = rdf_store
            .query(&format!(
                "SELECT ?subject ?predicate ?object WHERE {{ GRAPH <{graph_uri}> {{ ?subject ?predicate ?object }} }}"
            ))
            .expect("graph query should succeed")
            .into_iter()
            .map(|row| {
                let subject = row
                    .get("subject")
                    .and_then(serde_json::Value::as_str)
                    .expect("subject binding should be a string")
                    .to_string();
                let predicate = row
                    .get("predicate")
                    .and_then(serde_json::Value::as_str)
                    .expect("predicate binding should be a string")
                    .to_string();
                let object = row
                    .get("object")
                    .and_then(serde_json::Value::as_str)
                    .expect("object binding should be a string")
                    .to_string();
                (subject, predicate, object)
            })
            .collect::<Vec<_>>();
        triples.sort();
        triples
    }

    fn subject_objects(
        rdf_store: &GraphicaRdfStore,
        graph_uri: &str,
        subject_uri: &str,
        predicate_uri: &str,
    ) -> Vec<String> {
        graph_triples(rdf_store, graph_uri)
            .into_iter()
            .filter(|(subject, predicate, _)| subject == subject_uri && predicate == predicate_uri)
            .map(|(_, _, object)| object)
            .collect()
    }

    fn register_minimal_catalog(storage_manager: &Arc<SosStorageManager>) {
        storage_manager
            .put_system(&sample_system("provider-system", "Provider"))
            .expect("provider system should be stored");
        storage_manager
            .put_system(&sample_system("consumer-system", "Consumer"))
            .expect("consumer system should be stored");
        storage_manager
            .put_interface(&sample_interface("provider-if", "provider-system", "SI"))
            .expect("provider interface should be stored");
        storage_manager
            .put_interface(&sample_interface(
                "consumer-if",
                "consumer-system",
                "Imperial",
            ))
            .expect("consumer interface should be stored");
    }

    fn sample_interface_pair_policy_request(policy_id: &str) -> CreateSosPolicyRequest {
        CreateSosPolicyRequest {
            policy_id: policy_id.to_string(),
            policy_name: format!("Policy {policy_id}"),
            target_type: "interface_pair".to_string(),
            stages: vec!["pre_execution".to_string()],
            enforcement_level: "mandatory".to_string(),
            severity: "high".to_string(),
            sparql_query: "ASK { GRAPH <http://graphica.io/graph/sos-catalog> { <{{provider_interface_uri}}> <http://graphica.io/sos#belongsToSystem> ?system } }".to_string(),
            context: HashMap::new(),
            description: Some("Synthetic interface pair policy".to_string()),
            created_by: None,
            updated_by: None,
            lifecycle_state: None,
            tags: vec!["test".to_string()],
            ontology_refs: vec!["sos-core".to_string()],
            shape_refs: vec!["shape:test".to_string()],
            active: true,
            provider_interface_id: Some("provider-if".to_string()),
            consumer_interface_id: Some("consumer-if".to_string()),
            contract_id: None,
            source_system_id: None,
            target_system_id: None,
            interface_id: None,
        }
    }

    fn sample_interface_policy_request(policy_id: &str) -> CreateSosPolicyRequest {
        CreateSosPolicyRequest {
            policy_id: policy_id.to_string(),
            policy_name: format!("Policy {policy_id}"),
            target_type: "interface".to_string(),
            stages: vec!["in_flight".to_string()],
            enforcement_level: "mandatory".to_string(),
            severity: "medium".to_string(),
            sparql_query: "ASK { GRAPH <http://graphica.io/graph/sos-catalog> { <{{interface_uri}}> <http://graphica.io/sos#belongsToSystem> ?system } }".to_string(),
            context: HashMap::new(),
            description: Some("Synthetic interface policy".to_string()),
            created_by: None,
            updated_by: None,
            lifecycle_state: None,
            tags: vec!["test".to_string()],
            ontology_refs: vec![],
            shape_refs: vec![],
            active: true,
            provider_interface_id: None,
            consumer_interface_id: None,
            contract_id: None,
            source_system_id: None,
            target_system_id: None,
            interface_id: Some("provider-if".to_string()),
        }
    }

    fn failing_interface_policy_request(
        policy_id: &str,
        lifecycle_state: Option<&str>,
        enforcement_level: &str,
    ) -> CreateSosPolicyRequest {
        let mut request = sample_interface_policy_request(policy_id);
        request.sparql_query =
            "ASK { GRAPH <http://graphica.io/graph/sos-catalog> { <{{interface_uri}}> <http://graphica.io/sos#nonexistentConstraint> ?value } }"
                .to_string();
        request.severity = "high".to_string();
        request.enforcement_level = enforcement_level.to_string();
        request.lifecycle_state = lifecycle_state.map(ToOwned::to_owned);
        request
    }

    #[test]
    fn validate_spec_persists_history_and_lineage() {
        let (_temp_dir, storage_manager, service) = create_service();
        register_minimal_catalog(&storage_manager);

        let spec = SosValidationSpec::InterfaceCompatibility {
            provider_interface_id: "provider-if".to_string(),
            consumer_interface_id: "consumer-if".to_string(),
        };

        let first = service
            .validate_spec(spec.clone(), ValidationExecutionOptions::persisted())
            .expect("initial validation should succeed");
        let first_report_id = first
            .report_id
            .clone()
            .expect("persisted validation should return report_id");

        std::thread::sleep(std::time::Duration::from_millis(2));

        storage_manager
            .put_contract(&sample_contract("contract-1", "provider-if", "consumer-if"))
            .expect("contract should be stored");

        let second = service
            .validate_spec(spec, ValidationExecutionOptions::persisted())
            .expect("follow-up validation should succeed");
        let second_report_id = second
            .report_id
            .clone()
            .expect("persisted validation should return report_id");

        let persisted_second = storage_manager
            .get_validation_report(&second_report_id)
            .expect("report lookup should succeed")
            .expect("second report should exist");

        assert_eq!(
            persisted_second.previous_report_id.as_deref(),
            Some(first_report_id.as_str())
        );
        assert!(persisted_second
            .change_summary
            .resolved_checks
            .contains(&"unit_compatibility".to_string()));
        assert!(persisted_second
            .change_summary
            .resolved_checks
            .contains(&"contract_alignment".to_string()));

        let history = service
            .get_validation_history(
                "interface_pair:provider-if:consumer-if",
                Some("interface_pair"),
                10,
            )
            .expect("history lookup should succeed");
        assert_eq!(history.reports.len(), 2);
        assert_eq!(history.reports[0].report_id, second_report_id);
        assert_eq!(history.reports[1].report_id, first_report_id);

        let lineage = service
            .get_validation_lineage(
                "interface_pair:provider-if:consumer-if",
                Some("interface_pair"),
                10,
            )
            .expect("lineage lookup should succeed");
        assert_eq!(lineage.reports.len(), 2);
        assert_eq!(lineage.edges.len(), 1);
        assert_eq!(lineage.edges[0].relationship, "prov:wasRevisionOf");
    }

    #[test]
    fn interface_validation_reports_nested_schema_incompatibility() {
        let (_temp_dir, storage_manager, service) = create_service();
        storage_manager
            .put_system(&sample_system("provider-system", "Provider"))
            .expect("provider system should be stored");
        storage_manager
            .put_system(&sample_system("consumer-system", "Consumer"))
            .expect("consumer system should be stored");

        let mut provider = sample_interface("provider-if", "provider-system", "SI");
        provider.schema = json!({
            "type": "object",
            "required": ["payload"],
            "properties": {
                "payload": {
                    "type": "object",
                    "required": ["status"],
                    "properties": {
                        "status": { "type": "string" }
                    }
                }
            }
        });

        let mut consumer = sample_interface("consumer-if", "consumer-system", "SI");
        consumer.schema = json!({
            "type": "object",
            "required": ["payload"],
            "properties": {
                "payload": {
                    "type": "object",
                    "required": ["status", "priority"],
                    "properties": {
                        "status": { "type": "string" },
                        "priority": { "type": "integer" }
                    }
                }
            }
        });

        storage_manager
            .put_interface(&provider)
            .expect("provider interface should be stored");
        storage_manager
            .put_interface(&consumer)
            .expect("consumer interface should be stored");

        let response = service
            .validate_spec(
                SosValidationSpec::InterfaceCompatibility {
                    provider_interface_id: "provider-if".to_string(),
                    consumer_interface_id: "consumer-if".to_string(),
                },
                ValidationExecutionOptions::dry_run(),
            )
            .expect("nested schema validation should execute");

        assert!(!response.passed);

        let schema_check = response
            .checks
            .iter()
            .find(|check| check.check_name == "schema_compatibility")
            .expect("schema compatibility check should be present");

        assert!(!schema_check.passed);
        assert!(schema_check.description.contains("$.payload.priority"));
    }

    #[test]
    fn interface_validation_rejects_misaligned_unit_transform_rule() {
        let (_temp_dir, storage_manager, service) = create_service();
        register_minimal_catalog(&storage_manager);

        let mut contract = sample_contract("contract-1", "provider-if", "consumer-if");
        contract.transformation_rules = HashMap::from([(
            "unit_transform".to_string(),
            json!({
                "from": "Imperial",
                "to": "SI",
                "strategy": "linear_scale",
                "scale": 0.3048,
                "offset": 0.0
            }),
        )]);
        storage_manager
            .put_contract(&contract)
            .expect("contract should be stored");

        let response = service
            .validate_spec(
                SosValidationSpec::InterfaceCompatibility {
                    provider_interface_id: "provider-if".to_string(),
                    consumer_interface_id: "consumer-if".to_string(),
                },
                ValidationExecutionOptions::dry_run(),
            )
            .expect("interface compatibility should execute");

        assert!(!response.passed);

        let unit_check = response
            .checks
            .iter()
            .find(|check| check.check_name == "unit_compatibility")
            .expect("unit compatibility check should be present");
        assert!(!unit_check.passed);
        assert!(unit_check
            .description
            .contains("maps Imperial -> SI instead"));
    }

    #[test]
    fn interface_validation_surfaces_invalid_transformation_rule_shape() {
        let (_temp_dir, storage_manager, service) = create_service();
        register_minimal_catalog(&storage_manager);

        let mut contract = sample_contract("contract-1", "provider-if", "consumer-if");
        contract.transformation_rules = HashMap::from([(
            "unit_transform".to_string(),
            Value::String("SI->Imperial".to_string()),
        )]);
        storage_manager
            .put_contract(&contract)
            .expect("contract should be stored");

        let response = service
            .validate_spec(
                SosValidationSpec::InterfaceCompatibility {
                    provider_interface_id: "provider-if".to_string(),
                    consumer_interface_id: "consumer-if".to_string(),
                },
                ValidationExecutionOptions::dry_run(),
            )
            .expect("interface compatibility should execute");

        assert!(!response.passed);

        let transform_check = response
            .checks
            .iter()
            .find(|check| check.check_name == "transformation_rules")
            .expect("transformation_rules check should be present");
        assert!(!transform_check.passed);
        assert!(transform_check.description.contains("must be an object"));
    }

    #[test]
    fn interface_validation_surfaces_incomplete_unit_transform_semantics() {
        let (_temp_dir, storage_manager, service) = create_service();
        register_minimal_catalog(&storage_manager);

        let mut contract = sample_contract("contract-1", "provider-if", "consumer-if");
        contract.transformation_rules = HashMap::from([(
            "unit_transform".to_string(),
            json!({
                "from": "SI",
                "to": "Imperial"
            }),
        )]);
        storage_manager
            .put_contract(&contract)
            .expect("contract should be stored");

        let response = service
            .validate_spec(
                SosValidationSpec::InterfaceCompatibility {
                    provider_interface_id: "provider-if".to_string(),
                    consumer_interface_id: "consumer-if".to_string(),
                },
                ValidationExecutionOptions::dry_run(),
            )
            .expect("interface compatibility should execute");

        assert!(!response.passed);

        let transform_check = response
            .checks
            .iter()
            .find(|check| check.check_name == "transformation_rules")
            .expect("transformation_rules check should be present");
        assert!(!transform_check.passed);
        assert!(transform_check
            .description
            .contains("must declare a unit conversion strategy"));

        let unit_check = response
            .checks
            .iter()
            .find(|check| check.check_name == "unit_compatibility")
            .expect("unit compatibility check should be present");
        assert!(!unit_check.passed);
        assert!(unit_check
            .description
            .contains("no semantically valid transformation rule"));
    }

    #[test]
    fn interface_validation_surfaces_incomplete_coordinate_transform_semantics() {
        let (_temp_dir, storage_manager, service) = create_service();
        storage_manager
            .put_system(&sample_system("provider-system", "Provider"))
            .expect("provider system should be stored");
        storage_manager
            .put_system(&sample_system("consumer-system", "Consumer"))
            .expect("consumer system should be stored");

        let provider = sample_interface("provider-if", "provider-system", "SI");
        let mut consumer = sample_interface("consumer-if", "consumer-system", "SI");
        consumer.coordinate_system = Some("ECI_J2000".to_string());

        storage_manager
            .put_interface(&provider)
            .expect("provider interface should be stored");
        storage_manager
            .put_interface(&consumer)
            .expect("consumer interface should be stored");

        let mut contract = sample_contract("contract-1", "provider-if", "consumer-if");
        contract.transformation_rules = HashMap::from([(
            "coordinate_transform".to_string(),
            json!({
                "from": "WGS84",
                "to": "ECI_J2000"
            }),
        )]);
        storage_manager
            .put_contract(&contract)
            .expect("contract should be stored");

        let response = service
            .validate_spec(
                SosValidationSpec::InterfaceCompatibility {
                    provider_interface_id: "provider-if".to_string(),
                    consumer_interface_id: "consumer-if".to_string(),
                },
                ValidationExecutionOptions::dry_run(),
            )
            .expect("interface compatibility should execute");

        assert!(!response.passed);

        let transform_check = response
            .checks
            .iter()
            .find(|check| check.check_name == "transformation_rules")
            .expect("transformation_rules check should be present");
        assert!(!transform_check.passed);
        assert!(transform_check
            .description
            .contains("must declare a coordinate conversion strategy"));

        let coordinate_check = response
            .checks
            .iter()
            .find(|check| check.check_name == "coordinate_compatibility")
            .expect("coordinate compatibility check should be present");
        assert!(!coordinate_check.passed);
        assert!(coordinate_check
            .description
            .contains("no semantically valid transformation rule"));
    }

    #[test]
    fn interface_validation_marks_unbounded_unit_transform_as_warning_with_reduced_confidence() {
        let (_temp_dir, storage_manager, service) = create_service();
        register_minimal_catalog(&storage_manager);

        let response = service
            .validate_spec(
                SosValidationSpec::InterfaceCompatibility {
                    provider_interface_id: "provider-if".to_string(),
                    consumer_interface_id: "consumer-if".to_string(),
                },
                ValidationExecutionOptions::dry_run(),
            )
            .expect("interface compatibility should execute");

        assert!(response.passed);
        assert!(response.confidence < 1.0);
        assert_eq!(
            response.compatibility_state,
            Some(CompatibilityState::Transformable)
        );

        let unit_check = response
            .checks
            .iter()
            .find(|check| check.check_name == "unit_compatibility")
            .expect("unit compatibility check should be present");
        assert!(unit_check.passed);
        assert_eq!(unit_check.severity, "warning");
        assert!(unit_check
            .description
            .contains("no declared error budget"));

        let details = unit_check
            .details
            .as_ref()
            .expect("unit compatibility details should be present");
        assert_eq!(
            details.get("compatibility_mode"),
            Some(&json!("unbounded_transform"))
        );
        assert_eq!(details.get("confidence_score"), Some(&json!(0.75)));
        assert_eq!(
            details.get("requires_runtime_verification"),
            Some(&json!(true))
        );
    }

    #[test]
    fn interface_validation_records_bounded_coordinate_transform_error_budget() {
        let (_temp_dir, storage_manager, service) = create_service();
        storage_manager
            .put_system(&sample_system("provider-system", "Provider"))
            .expect("provider system should be stored");
        storage_manager
            .put_system(&sample_system("consumer-system", "Consumer"))
            .expect("consumer system should be stored");

        let provider = sample_interface("provider-if", "provider-system", "SI");
        let mut consumer = sample_interface("consumer-if", "consumer-system", "SI");
        consumer.coordinate_system = Some("ECI_J2000".to_string());

        storage_manager
            .put_interface(&provider)
            .expect("provider interface should be stored");
        storage_manager
            .put_interface(&consumer)
            .expect("consumer interface should be stored");

        let mut contract = sample_contract("contract-1", "provider-if", "consumer-if");
        contract.transformation_rules = HashMap::from([(
            "coordinate_transform".to_string(),
            json!({
                "from": "WGS84",
                "to": "ECI_J2000",
                "strategy": "helmert",
                "translation_m": [1.0, 2.0, 3.0],
                "rotation_arcsec": [0.1, 0.2, 0.3],
                "scale_ppm": 0.0,
                "tolerance_m": 5.0
            }),
        )]);
        storage_manager
            .put_contract(&contract)
            .expect("contract should be stored");

        let response = service
            .validate_spec(
                SosValidationSpec::InterfaceCompatibility {
                    provider_interface_id: "provider-if".to_string(),
                    consumer_interface_id: "consumer-if".to_string(),
                },
                ValidationExecutionOptions::dry_run(),
            )
            .expect("interface compatibility should execute");

        assert!(response.passed);
        assert!(response.confidence < 1.0);
        assert_eq!(
            response.compatibility_state,
            Some(CompatibilityState::Transformable)
        );

        let coordinate_check = response
            .checks
            .iter()
            .find(|check| check.check_name == "coordinate_compatibility")
            .expect("coordinate compatibility check should be present");
        assert!(coordinate_check.passed);
        assert_eq!(coordinate_check.severity, "info");
        assert!(coordinate_check.description.contains("tolerance_m=5"));

        let details = coordinate_check
            .details
            .as_ref()
            .expect("coordinate compatibility details should be present");
        assert_eq!(
            details.get("compatibility_mode"),
            Some(&json!("bounded_transform"))
        );
        assert_eq!(details.get("confidence_score"), Some(&json!(0.9)));
        assert_eq!(
            details.get("requires_runtime_verification"),
            Some(&json!(false))
        );
        assert_eq!(
            details.get("declared_error_budget"),
            Some(&json!({
                "value": 5.0,
                "label": "m"
            }))
        );
    }

    #[test]
    fn interface_validation_reports_schema_transformability_from_field_mapping() {
        let (_temp_dir, storage_manager, service) = create_service();
        storage_manager
            .put_system(&sample_system("provider-system", "Provider"))
            .expect("provider system should be stored");
        storage_manager
            .put_system(&sample_system("consumer-system", "Consumer"))
            .expect("consumer system should be stored");

        let mut provider = sample_interface("provider-if", "provider-system", "SI");
        provider.schema = json!({
            "type": "object",
            "required": ["payload"],
            "properties": {
                "payload": {
                    "type": "object",
                    "required": ["status", "rank"],
                    "properties": {
                        "status": { "type": "string" },
                        "rank": { "type": "integer" }
                    }
                }
            }
        });

        let mut consumer = sample_interface("consumer-if", "consumer-system", "SI");
        consumer.schema = json!({
            "type": "object",
            "required": ["payload"],
            "properties": {
                "payload": {
                    "type": "object",
                    "required": ["status", "priority"],
                    "properties": {
                        "status": { "type": "string" },
                        "priority": { "type": "integer" }
                    }
                }
            }
        });

        storage_manager
            .put_interface(&provider)
            .expect("provider interface should be stored");
        storage_manager
            .put_interface(&consumer)
            .expect("consumer interface should be stored");

        let mut contract = sample_contract("contract-1", "provider-if", "consumer-if");
        contract.transformation_rules = HashMap::from([(
            "field_mapping".to_string(),
            json!({
                "mappings": [
                    { "from": "$.payload.rank", "to": "$.payload.priority" }
                ]
            }),
        )]);
        storage_manager
            .put_contract(&contract)
            .expect("contract should be stored");

        let response = service
            .validate_spec(
                SosValidationSpec::InterfaceCompatibility {
                    provider_interface_id: "provider-if".to_string(),
                    consumer_interface_id: "consumer-if".to_string(),
                },
                ValidationExecutionOptions::dry_run(),
            )
            .expect("transformability validation should execute");

        assert!(!response.passed);
        assert_eq!(
            response.compatibility_state,
            Some(CompatibilityState::Transformable)
        );

        let schema_check = response
            .checks
            .iter()
            .find(|check| check.check_name == "schema_compatibility")
            .expect("schema compatibility check should be present");
        assert!(!schema_check.passed);
        assert!(schema_check.description.contains("$.payload.priority"));

        let transformability_check = response
            .checks
            .iter()
            .find(|check| check.check_name == "schema_transformability")
            .expect("schema transformability check should be present");
        assert!(transformability_check.passed);
        assert!(transformability_check
            .description
            .contains("$.payload.priority"));
    }

    #[test]
    fn interface_validation_reports_semantically_equivalent_when_no_transforms_are_needed() {
        let (_temp_dir, storage_manager, service) = create_service();
        storage_manager
            .put_system(&sample_system("provider-system", "Provider"))
            .expect("provider system should be stored");
        storage_manager
            .put_system(&sample_system("consumer-system", "Consumer"))
            .expect("consumer system should be stored");

        let provider = sample_interface("provider-if", "provider-system", "SI");
        let mut consumer = sample_interface("consumer-if", "consumer-system", "SI");
        consumer.coordinate_system = Some("WGS84".to_string());

        storage_manager
            .put_interface(&provider)
            .expect("provider interface should be stored");
        storage_manager
            .put_interface(&consumer)
            .expect("consumer interface should be stored");

        let response = service
            .validate_spec(
                SosValidationSpec::InterfaceCompatibility {
                    provider_interface_id: "provider-if".to_string(),
                    consumer_interface_id: "consumer-if".to_string(),
                },
                ValidationExecutionOptions::dry_run(),
            )
            .expect("interface compatibility should execute");

        assert!(response.passed);
        assert_eq!(
            response.compatibility_state,
            Some(CompatibilityState::SemanticallyEquivalent)
        );
    }

    #[test]
    fn interface_validation_reports_syntactically_compatible_when_semantic_metadata_is_absent() {
        let (_temp_dir, storage_manager, service) = create_service();
        storage_manager
            .put_system(&sample_system("provider-system", "Provider"))
            .expect("provider system should be stored");
        storage_manager
            .put_system(&sample_system("consumer-system", "Consumer"))
            .expect("consumer system should be stored");

        let mut provider = sample_interface("provider-if", "provider-system", "SI");
        provider.unit_system = None;
        provider.coordinate_system = None;

        let mut consumer = sample_interface("consumer-if", "consumer-system", "SI");
        consumer.unit_system = None;
        consumer.coordinate_system = None;

        storage_manager
            .put_interface(&provider)
            .expect("provider interface should be stored");
        storage_manager
            .put_interface(&consumer)
            .expect("consumer interface should be stored");

        let response = service
            .validate_spec(
                SosValidationSpec::InterfaceCompatibility {
                    provider_interface_id: "provider-if".to_string(),
                    consumer_interface_id: "consumer-if".to_string(),
                },
                ValidationExecutionOptions::dry_run(),
            )
            .expect("interface compatibility should execute");

        assert!(response.passed);
        assert_eq!(
            response.compatibility_state,
            Some(CompatibilityState::SyntacticallyCompatible)
        );
    }

    #[test]
    fn dry_run_validation_does_not_persist_report() {
        let (_temp_dir, storage_manager, service) = create_service();
        register_minimal_catalog(&storage_manager);

        let response = service
            .validate_spec(
                SosValidationSpec::DataValidation {
                    interface_id: "provider-if".to_string(),
                    data: json!({
                        "sample_id": "row-1",
                        "score": 0.93
                    }),
                },
                ValidationExecutionOptions::dry_run(),
            )
            .expect("dry-run validation should succeed");

        assert!(response.report_id.is_none());
        assert!(storage_manager
            .list_all_validation_reports()
            .expect("report listing should succeed")
            .is_empty());
    }

    #[test]
    fn create_policy_rejects_missing_target_reference() {
        let (_temp_dir, _storage_manager, service) = create_service();

        let error = service
            .create_policy(CreateSosPolicyRequest {
                policy_id: "missing-interface-policy".to_string(),
                policy_name: "Missing Interface Policy".to_string(),
                target_type: "interface".to_string(),
                stages: vec!["in_flight".to_string()],
                enforcement_level: "mandatory".to_string(),
                severity: "medium".to_string(),
                sparql_query: "ASK { ?s ?p ?o }".to_string(),
                context: HashMap::new(),
                description: None,
                created_by: None,
                updated_by: None,
                lifecycle_state: None,
                tags: Vec::new(),
                ontology_refs: Vec::new(),
                shape_refs: Vec::new(),
                active: true,
                provider_interface_id: None,
                consumer_interface_id: None,
                contract_id: None,
                source_system_id: None,
                target_system_id: None,
                interface_id: Some("missing-interface".to_string()),
            })
            .expect_err("missing interface should be rejected");

        assert!(matches!(error, SosValidationServiceError::NotFound(_)));
    }

    #[test]
    fn create_policy_rejects_malformed_template_placeholder() {
        let (_temp_dir, _storage_manager, service) = create_service();

        let error = service
            .create_policy(CreateSosPolicyRequest {
                policy_id: "malformed-policy".to_string(),
                policy_name: "Malformed Policy".to_string(),
                target_type: "global".to_string(),
                stages: vec!["pre_execution".to_string()],
                enforcement_level: "mandatory".to_string(),
                severity: "medium".to_string(),
                sparql_query: "ASK { <{{provider_interface_uri}> ?p ?o }".to_string(),
                context: HashMap::new(),
                description: None,
                created_by: None,
                updated_by: None,
                lifecycle_state: None,
                tags: Vec::new(),
                ontology_refs: Vec::new(),
                shape_refs: Vec::new(),
                active: true,
                provider_interface_id: None,
                consumer_interface_id: None,
                contract_id: None,
                source_system_id: None,
                target_system_id: None,
                interface_id: None,
            })
            .expect_err("malformed template should be rejected");

        assert!(matches!(
            error,
            SosValidationServiceError::InvalidRequest(message)
                if message.contains("Malformed policy query template")
        ));
    }

    #[test]
    fn update_policy_creates_new_revision_and_preserves_history() {
        let (_temp_dir, storage_manager, service) = create_service();
        register_minimal_catalog(&storage_manager);

        let mut request = sample_interface_policy_request("revisioned-policy");
        request.created_by = Some("alice".to_string());
        let created = service
            .create_policy(request)
            .expect("policy should be created");

        assert_eq!(created.revision, 1);
        assert_eq!(created.created_by, "alice");
        assert_eq!(created.updated_by, "alice");
        assert_eq!(created.lifecycle_state, POLICY_LIFECYCLE_ACTIVE);
        assert_eq!(created.approval_status, POLICY_APPROVAL_APPROVED);
        assert_eq!(created.approved_by.as_deref(), Some("alice"));

        let updated = service
            .update_policy(
                "revisioned-policy",
                UpdateSosPolicyRequest {
                    policy_name: None,
                    target_type: None,
                    stages: None,
                    enforcement_level: None,
                    severity: Some("high".to_string()),
                    sparql_query: None,
                    context: None,
                    description: None,
                    updated_by: Some("bob".to_string()),
                    lifecycle_state: Some(POLICY_LIFECYCLE_DRAFT.to_string()),
                    tags: None,
                    ontology_refs: None,
                    shape_refs: None,
                    active: None,
                    provider_interface_id: None,
                    consumer_interface_id: None,
                    contract_id: None,
                    source_system_id: None,
                    target_system_id: None,
                    interface_id: None,
                },
            )
            .expect("policy should be revisioned");

        assert_eq!(updated.revision, 2);
        assert_eq!(updated.created_by, "alice");
        assert_eq!(updated.updated_by, "bob");
        assert_eq!(updated.severity, "high");
        assert_eq!(updated.lifecycle_state, POLICY_LIFECYCLE_DRAFT);
        assert_eq!(updated.approval_status, POLICY_APPROVAL_PENDING);
        assert!(!updated.active);
        assert_eq!(updated.approval_requested_by.as_deref(), Some("bob"));
        assert!(updated.approved_by.is_none());

        let revisions = service
            .list_policy_revisions("revisioned-policy", 10)
            .expect("revision history should be listed");
        assert_eq!(revisions.len(), 2);
        assert_eq!(revisions[0].revision, 2);
        assert_eq!(revisions[0].approval_status, POLICY_APPROVAL_PENDING);
        assert_eq!(revisions[0].lifecycle_state, POLICY_LIFECYCLE_DRAFT);
        assert_eq!(revisions[1].revision, 1);
        assert_eq!(revisions[1].superseded_by_revision, Some(2));
        assert_eq!(revisions[1].approval_status, POLICY_APPROVAL_APPROVED);
    }

    #[test]
    fn pending_policy_revision_must_be_approved_before_automatic_rollout() {
        let (_temp_dir, storage_manager, service) = create_service();
        register_minimal_catalog(&storage_manager);

        let mut request = sample_interface_policy_request("pending-policy");
        request.active = false;
        request.lifecycle_state = Some(POLICY_LIFECYCLE_DRAFT.to_string());

        let created = service
            .create_policy(request)
            .expect("draft policy should be created");
        assert_eq!(created.lifecycle_state, POLICY_LIFECYCLE_DRAFT);
        assert_eq!(created.approval_status, POLICY_APPROVAL_PENDING);
        assert!(!created.active);

        let error = service
            .update_policy(
                "pending-policy",
                UpdateSosPolicyRequest {
                    policy_name: None,
                    target_type: None,
                    stages: None,
                    enforcement_level: None,
                    severity: None,
                    sparql_query: None,
                    context: None,
                    description: None,
                    updated_by: Some("operator".to_string()),
                    lifecycle_state: Some(POLICY_LIFECYCLE_ACTIVE.to_string()),
                    tags: None,
                    ontology_refs: None,
                    shape_refs: None,
                    active: None,
                    provider_interface_id: None,
                    consumer_interface_id: None,
                    contract_id: None,
                    source_system_id: None,
                    target_system_id: None,
                    interface_id: None,
                },
            )
            .expect_err("automatic rollout should require approval");

        assert!(matches!(
            error,
            SosValidationServiceError::InvalidRequest(message)
                if message.contains("requires an approved policy revision")
        ));
    }

    #[test]
    fn approve_policy_promotes_pending_revision_into_active_rollout() {
        let (_temp_dir, storage_manager, _rdf_store, service) = create_service_with_rdf();
        register_minimal_catalog(&storage_manager);
        service
            .reconcile_graphs()
            .expect("catalog graph should be projected");

        let mut request = sample_interface_policy_request("approvable-policy");
        request.active = false;
        request.lifecycle_state = Some(POLICY_LIFECYCLE_DRAFT.to_string());

        service
            .create_policy(request)
            .expect("draft policy should be created");

        let approval_request = service
            .create_policy_approval_request(
                "approvable-policy",
                CreateSosPolicyApprovalRequest {
                    requested_by: "operator-1".to_string(),
                    lifecycle_state: POLICY_LIFECYCLE_ACTIVE.to_string(),
                    expires_in_seconds: None,
                    note: Some("Ready for enforcement".to_string()),
                    metadata: HashMap::new(),
                },
            )
            .expect("approval request should be created");

        let report = service
            .evaluate_policy_by_id(
                "approvable-policy",
                EvaluatePolicyRequest {
                    stage: Some("in_flight".to_string()),
                    revision: None,
                    context: HashMap::new(),
                },
                ValidationExecutionOptions::persisted(),
            )
            .expect("policy evaluation should succeed");
        let report_id = report
            .report_id
            .expect("persisted policy evaluation should produce a report");

        service
            .add_policy_approval_evidence(
                "approvable-policy",
                &approval_request.request_id,
                AddSosPolicyApprovalEvidenceRequest {
                    report_id,
                    added_by: "qa-reviewer".to_string(),
                    note: Some("Passing validation evidence".to_string()),
                    metadata: HashMap::new(),
                },
            )
            .expect("approval evidence should be attached");

        let approved = service
            .approve_policy_approval_request(
                "approvable-policy",
                &approval_request.request_id,
                ApproveSosPolicyApprovalRequest {
                    approved_by: "reviewer-1".to_string(),
                },
            )
            .expect("approval should succeed");

        assert_eq!(approved.status, "approved");
        assert_eq!(approved.approved_by.as_deref(), Some("reviewer-1"));
        assert_eq!(approved.requested_lifecycle_state, POLICY_LIFECYCLE_ACTIVE);
        assert_eq!(approved.evidence.len(), 1);

        let persisted_policy = service
            .get_policy("approvable-policy")
            .expect("policy should still be retrievable");
        assert_eq!(persisted_policy.revision, 1);
        assert_eq!(persisted_policy.lifecycle_state, POLICY_LIFECYCLE_ACTIVE);
        assert_eq!(persisted_policy.approval_status, POLICY_APPROVAL_APPROVED);
        assert!(persisted_policy.active);
        assert_eq!(persisted_policy.approved_by.as_deref(), Some("reviewer-1"));
        assert_eq!(persisted_policy.updated_by, "reviewer-1");
    }

    #[test]
    fn reject_policy_records_reviewer_and_reason() {
        let (_temp_dir, storage_manager, service) = create_service();
        register_minimal_catalog(&storage_manager);

        let mut request = sample_interface_policy_request("rejectable-policy");
        request.active = false;
        request.lifecycle_state = Some(POLICY_LIFECYCLE_DRAFT.to_string());

        service
            .create_policy(request)
            .expect("draft policy should be created");

        let approval_request = service
            .create_policy_approval_request(
                "rejectable-policy",
                CreateSosPolicyApprovalRequest {
                    requested_by: "operator-2".to_string(),
                    lifecycle_state: POLICY_LIFECYCLE_ACTIVE.to_string(),
                    expires_in_seconds: None,
                    note: Some("Needs reviewer sign-off".to_string()),
                    metadata: HashMap::new(),
                },
            )
            .expect("approval request should be created");

        let rejected = service
            .reject_policy_approval_request(
                "rejectable-policy",
                &approval_request.request_id,
                RejectSosPolicyApprovalRequest {
                    rejected_by: "reviewer-2".to_string(),
                    reason: "Schema semantics changed without evidence".to_string(),
                },
            )
            .expect("rejection should succeed");

        assert_eq!(rejected.status, "rejected");
        assert_eq!(rejected.rejected_by.as_deref(), Some("reviewer-2"));
        assert_eq!(
            rejected.rejection_reason.as_deref(),
            Some("Schema semantics changed without evidence")
        );
        assert!(rejected.approved_by.is_none());

        let persisted_policy = service
            .get_policy("rejectable-policy")
            .expect("policy should still be retrievable");
        assert_eq!(persisted_policy.lifecycle_state, POLICY_LIFECYCLE_DRAFT);
        assert_eq!(persisted_policy.approval_status, POLICY_APPROVAL_REJECTED);
        assert_eq!(persisted_policy.rejected_by.as_deref(), Some("reviewer-2"));
    }

    #[test]
    fn policy_approval_request_requires_evidence_before_approval() {
        let (_temp_dir, storage_manager, service) = create_service();
        register_minimal_catalog(&storage_manager);

        let mut request = sample_interface_policy_request("request-without-evidence");
        request.active = false;
        request.lifecycle_state = Some(POLICY_LIFECYCLE_DRAFT.to_string());
        service
            .create_policy(request)
            .expect("draft policy should be created");

        let approval_request = service
            .create_policy_approval_request(
                "request-without-evidence",
                CreateSosPolicyApprovalRequest {
                    requested_by: "operator-3".to_string(),
                    lifecycle_state: POLICY_LIFECYCLE_ACTIVE.to_string(),
                    expires_in_seconds: None,
                    note: None,
                    metadata: HashMap::new(),
                },
            )
            .expect("approval request should be created");

        let error = service
            .approve_policy_approval_request(
                "request-without-evidence",
                &approval_request.request_id,
                ApproveSosPolicyApprovalRequest {
                    approved_by: "reviewer-3".to_string(),
                },
            )
            .expect_err("approval should require evidence");

        assert!(matches!(
            error,
            SosValidationServiceError::InvalidRequest(message)
                if message.contains("requires at least one evidence record")
        ));
    }

    #[test]
    fn contract_approval_request_requires_evidence_before_approval() {
        let (_temp_dir, storage_manager, service) = create_service();
        register_minimal_catalog(&storage_manager);

        let mut contract =
            sample_contract("contract-approval-pending", "provider-if", "consumer-if");
        set_contract_draft(&mut contract);
        storage_manager
            .put_contract(&contract)
            .expect("draft contract should be stored");

        let approval_request = service
            .create_contract_approval_request(
                "contract-approval-pending",
                CreateSosContractApprovalRequest {
                    requested_by: "operator-1".to_string(),
                    lifecycle_state: "approved".to_string(),
                    expires_in_seconds: None,
                    note: Some("Ready for review".to_string()),
                    metadata: HashMap::new(),
                },
            )
            .expect("contract approval request should be created");

        let error = service
            .approve_contract_approval_request(
                "contract-approval-pending",
                &approval_request.request_id,
                ApproveSosContractApprovalRequest {
                    approved_by: "reviewer-1".to_string(),
                },
            )
            .expect_err("approval should require evidence");

        assert!(matches!(
            error,
            SosValidationServiceError::InvalidRequest(message)
                if message.contains("requires at least one evidence record")
        ));
    }

    #[test]
    fn approve_contract_promotes_pending_revision_after_evidence() {
        let (_temp_dir, storage_manager, service) = create_service();
        register_minimal_catalog(&storage_manager);

        let mut contract = sample_contract("contract-approval", "provider-if", "consumer-if");
        set_contract_draft(&mut contract);
        contract.sla_metrics = vec![SlaMetric {
            name: "latency_ms".to_string(),
            value: 250.0,
            operator: "<=".to_string(),
            unit: Some("ms".to_string()),
        }];
        storage_manager
            .put_contract(&contract)
            .expect("draft contract should be stored");

        let approval_request = service
            .create_contract_approval_request(
                "contract-approval",
                CreateSosContractApprovalRequest {
                    requested_by: "operator-2".to_string(),
                    lifecycle_state: "approved".to_string(),
                    expires_in_seconds: None,
                    note: Some("Ready for governance approval".to_string()),
                    metadata: HashMap::new(),
                },
            )
            .expect("contract approval request should be created");

        let report = service
            .validate_request(
                ValidateRequest::InterfaceCompatibility {
                    provider_interface_id: "provider-if".to_string(),
                    consumer_interface_id: "consumer-if".to_string(),
                },
                ValidationExecutionOptions::persisted(),
            )
            .expect("interface compatibility validation should succeed");
        let report_id = report
            .report_id
            .expect("persisted validation should produce a report id");

        service
            .add_contract_approval_evidence(
                "contract-approval",
                &approval_request.request_id,
                AddSosContractApprovalEvidenceRequest {
                    report_id,
                    added_by: "qa-reviewer".to_string(),
                    note: Some("Passing interface-pair validation".to_string()),
                    metadata: HashMap::new(),
                },
            )
            .expect("contract approval evidence should be attached");

        let approved = service
            .approve_contract_approval_request(
                "contract-approval",
                &approval_request.request_id,
                ApproveSosContractApprovalRequest {
                    approved_by: "reviewer-2".to_string(),
                },
            )
            .expect("contract approval should succeed");

        assert_eq!(approved.status, "approved");
        assert_eq!(approved.approved_by.as_deref(), Some("reviewer-2"));
        assert_eq!(approved.evidence.len(), 1);

        let persisted_contract = storage_manager
            .get_contract("contract-approval")
            .expect("contract lookup should succeed")
            .expect("contract should still exist");
        assert!(persisted_contract.approved);
        assert_eq!(
            effective_contract_approval_status(&persisted_contract),
            CONTRACT_APPROVAL_APPROVED
        );
        assert_eq!(
            persisted_contract.approved_by.as_deref(),
            Some("reviewer-2")
        );
    }

    #[test]
    fn reject_contract_request_records_reviewer_and_reason() {
        let (_temp_dir, storage_manager, service) = create_service();
        register_minimal_catalog(&storage_manager);

        let mut contract = sample_contract("contract-reject", "provider-if", "consumer-if");
        set_contract_draft(&mut contract);
        storage_manager
            .put_contract(&contract)
            .expect("draft contract should be stored");

        let approval_request = service
            .create_contract_approval_request(
                "contract-reject",
                CreateSosContractApprovalRequest {
                    requested_by: "operator-3".to_string(),
                    lifecycle_state: "approved".to_string(),
                    expires_in_seconds: None,
                    note: Some("Waiting for evidence review".to_string()),
                    metadata: HashMap::new(),
                },
            )
            .expect("contract approval request should be created");

        let rejected = service
            .reject_contract_approval_request(
                "contract-reject",
                &approval_request.request_id,
                RejectSosContractApprovalRequest {
                    rejected_by: "reviewer-3".to_string(),
                    reason: "Need updated compatibility evidence".to_string(),
                },
            )
            .expect("contract rejection should succeed");

        assert_eq!(rejected.status, "rejected");
        assert_eq!(rejected.rejected_by.as_deref(), Some("reviewer-3"));
        assert_eq!(
            rejected.rejection_reason.as_deref(),
            Some("Need updated compatibility evidence")
        );

        let persisted_contract = storage_manager
            .get_contract("contract-reject")
            .expect("contract lookup should succeed")
            .expect("contract should still exist");
        assert_eq!(
            effective_contract_approval_status(&persisted_contract),
            CONTRACT_APPROVAL_REJECTED
        );
        assert_eq!(
            persisted_contract.rejected_by.as_deref(),
            Some("reviewer-3")
        );
    }

    #[test]
    fn direct_policy_validation_dry_run_does_not_persist_report() {
        let (_temp_dir, storage_manager, _rdf_store, service) = create_service_with_rdf();
        register_minimal_catalog(&storage_manager);
        service
            .reconcile_graphs()
            .expect("catalog graph should be available");
        service
            .create_policy(sample_interface_policy_request("interface-policy"))
            .expect("policy should be created");

        let response = service
            .evaluate_policy_by_id(
                "interface-policy",
                EvaluatePolicyRequest {
                    stage: Some("in_flight".to_string()),
                    revision: None,
                    context: HashMap::new(),
                },
                ValidationExecutionOptions::dry_run(),
            )
            .expect("dry-run policy evaluation should succeed");

        assert!(response.passed);
        assert!(response.report_id.is_none());
        assert!(storage_manager
            .list_all_validation_reports()
            .expect("report listing should succeed")
            .is_empty());
    }

    #[test]
    fn draft_policy_is_not_applied_automatically() {
        let (_temp_dir, storage_manager, _rdf_store, service) = create_service_with_rdf();
        register_minimal_catalog(&storage_manager);
        service
            .reconcile_graphs()
            .expect("catalog graph should be projected");
        let created = service
            .create_policy(failing_interface_policy_request(
                "draft-policy",
                Some(POLICY_LIFECYCLE_DRAFT),
                "mandatory",
            ))
            .expect("draft policy should be created");

        assert_eq!(created.lifecycle_state, POLICY_LIFECYCLE_DRAFT);
        assert!(!created.active);

        let response = service
            .validate_spec(
                SosValidationSpec::DataValidation {
                    interface_id: "provider-if".to_string(),
                    data: json!({
                        "sample_id": "row-1",
                        "score": 0.77
                    }),
                },
                ValidationExecutionOptions::persisted(),
            )
            .expect("data validation should succeed");
        assert_eq!(response.compatibility_state, None);
        let schema_check = response
            .checks
            .iter()
            .find(|check| check.check_name == "schema_validation")
            .expect("schema validation check should be present");
        let schema_details = schema_check
            .details
            .as_ref()
            .expect("schema validation should include normalized confidence details");
        assert_eq!(schema_details.get("confidence_score"), Some(&json!(1.0)));
        assert_eq!(
            schema_details.get("confidence_category"),
            Some(&json!("passed_check"))
        );
        assert_eq!(
            response
                .confidence_assessment
                .as_ref()
                .expect("confidence assessment should be present")
                .failed_check_count,
            0
        );

        let report = storage_manager
            .get_validation_report(
                response
                    .report_id
                    .as_deref()
                    .expect("persisted validation should have report id"),
            )
            .expect("report lookup should succeed")
            .expect("report should exist");

        assert!(
            !report
                .checks
                .iter()
                .any(|check| check.check_name == "policy:draft-policy"),
            "draft policies should not auto-apply during validation"
        );
    }

    #[test]
    fn dry_run_policy_failure_is_non_blocking_when_auto_applied() {
        let (_temp_dir, storage_manager, _rdf_store, service) = create_service_with_rdf();
        register_minimal_catalog(&storage_manager);
        service
            .reconcile_graphs()
            .expect("catalog graph should be projected");
        let created = service
            .create_policy(failing_interface_policy_request(
                "dry-run-policy",
                Some(POLICY_LIFECYCLE_DRY_RUN),
                "mandatory",
            ))
            .expect("dry-run policy should be created");

        assert_eq!(created.lifecycle_state, POLICY_LIFECYCLE_DRY_RUN);
        assert!(created.active);

        let response = service
            .validate_spec(
                SosValidationSpec::DataValidation {
                    interface_id: "provider-if".to_string(),
                    data: json!({
                        "sample_id": "row-2",
                        "score": 0.55
                    }),
                },
                ValidationExecutionOptions::persisted(),
            )
            .expect("data validation should succeed");

        assert!(
            response.passed,
            "dry-run rollout should not fail the enclosing validation"
        );

        let report = storage_manager
            .get_validation_report(
                response
                    .report_id
                    .as_deref()
                    .expect("persisted validation should have report id"),
            )
            .expect("report lookup should succeed")
            .expect("report should exist");

        let policy_check = report
            .checks
            .iter()
            .find(|check| check.check_name == "policy:dry-run-policy")
            .expect("dry-run policy check should be present");
        assert!(!policy_check.passed);
        assert_eq!(policy_check.severity, "warning");
        assert!(
            policy_check.description.starts_with("Dry-run rollout:"),
            "dry-run policy failures should be clearly labeled"
        );
        let details = policy_check
            .details
            .as_ref()
            .expect("policy check should include details");
        assert_eq!(
            details.get("policy_lifecycle_state"),
            Some(&json!(POLICY_LIFECYCLE_DRY_RUN))
        );
        assert_eq!(
            details.get("policy_execution_mode"),
            Some(&json!("dry_run"))
        );
        assert_eq!(details.get("policy_blocking"), Some(&json!(false)));
    }

    #[test]
    fn advisory_policy_failure_is_non_blocking_when_auto_applied() {
        let (_temp_dir, storage_manager, _rdf_store, service) = create_service_with_rdf();
        register_minimal_catalog(&storage_manager);
        service
            .reconcile_graphs()
            .expect("catalog graph should be projected");
        service
            .create_policy(failing_interface_policy_request(
                "advisory-policy",
                Some(POLICY_LIFECYCLE_ACTIVE),
                "advisory",
            ))
            .expect("advisory policy should be created");

        let response = service
            .validate_spec(
                SosValidationSpec::DataValidation {
                    interface_id: "provider-if".to_string(),
                    data: json!({
                        "sample_id": "row-3",
                        "score": 0.81
                    }),
                },
                ValidationExecutionOptions::persisted(),
            )
            .expect("data validation should succeed");

        assert!(
            response.passed,
            "advisory policies should not fail the enclosing validation"
        );
        assert!(
            response.confidence > 0.0 && response.confidence < 1.0,
            "non-blocking advisory failures should reduce confidence without zeroing it"
        );
        let confidence_assessment = response
            .confidence_assessment
            .as_ref()
            .expect("confidence assessment should be present");
        assert_eq!(confidence_assessment.method, "per_check_average_v2");
        assert_eq!(confidence_assessment.failed_check_count, 1);
        assert_eq!(confidence_assessment.warning_check_count, 1);
        assert!(
            confidence_assessment
                .summary
                .contains("non-blocking warning failure"),
            "summary should explain why confidence dropped"
        );

        let report = storage_manager
            .get_validation_report(
                response
                    .report_id
                    .as_deref()
                    .expect("persisted validation should have report id"),
            )
            .expect("report lookup should succeed")
            .expect("report should exist");

        let policy_check = report
            .checks
            .iter()
            .find(|check| check.check_name == "policy:advisory-policy")
            .expect("advisory policy check should be present");
        assert!(!policy_check.passed);
        assert_eq!(policy_check.severity, "warning");
        let details = policy_check
            .details
            .as_ref()
            .expect("policy check should include details");
        assert_eq!(
            details.get("policy_execution_mode"),
            Some(&json!("advisory"))
        );
        assert_eq!(details.get("policy_blocking"), Some(&json!(false)));
        assert_eq!(details.get("confidence_score"), Some(&json!(0.65)));
        assert_eq!(
            details.get("confidence_category"),
            Some(&json!("non_blocking_policy_failure"))
        );
    }

    #[test]
    fn direct_evaluation_of_dry_run_policy_reports_failure() {
        let (_temp_dir, storage_manager, _rdf_store, service) = create_service_with_rdf();
        register_minimal_catalog(&storage_manager);
        service
            .reconcile_graphs()
            .expect("catalog graph should be projected");
        service
            .create_policy(failing_interface_policy_request(
                "dry-run-direct-policy",
                Some(POLICY_LIFECYCLE_DRY_RUN),
                "mandatory",
            ))
            .expect("dry-run policy should be created");

        let response = service
            .evaluate_policy_by_id(
                "dry-run-direct-policy",
                EvaluatePolicyRequest {
                    stage: Some("in_flight".to_string()),
                    revision: None,
                    context: HashMap::new(),
                },
                ValidationExecutionOptions::dry_run(),
            )
            .expect("direct policy evaluation should succeed");

        assert!(
            !response.passed,
            "direct policy evaluation should still report the raw policy failure"
        );
        assert_eq!(response.confidence, 0.0);
        let confidence_assessment = response
            .confidence_assessment
            .as_ref()
            .expect("confidence assessment should be present");
        assert_eq!(confidence_assessment.failed_check_count, 1);
        assert!(
            confidence_assessment
                .contributors
                .iter()
                .any(|contributor| contributor.category == "blocking_failure"),
            "blocking direct policy failures should be called out explicitly"
        );
        let policy_check = response
            .checks
            .iter()
            .find(|check| check.check_name == "policy:dry-run-direct-policy")
            .expect("policy check should be present");
        assert_eq!(policy_check.severity, "error");
        let details = policy_check
            .details
            .as_ref()
            .expect("policy check should include details");
        assert_eq!(
            details.get("policy_lifecycle_state"),
            Some(&json!(POLICY_LIFECYCLE_DRY_RUN))
        );
        assert!(
            details.get("policy_execution_mode").is_none(),
            "direct evaluation should not be rewritten as an automatic rollout mode"
        );
        assert_eq!(
            details.get("confidence_category"),
            Some(&json!("blocking_failure"))
        );
    }

    #[test]
    fn retired_policy_cannot_be_reactivated() {
        let (_temp_dir, storage_manager, service) = create_service();
        register_minimal_catalog(&storage_manager);

        service
            .create_policy(sample_interface_policy_request("retired-policy"))
            .expect("policy should be created");
        service
            .update_policy(
                "retired-policy",
                UpdateSosPolicyRequest {
                    policy_name: None,
                    target_type: None,
                    stages: None,
                    enforcement_level: None,
                    severity: None,
                    sparql_query: None,
                    context: None,
                    description: None,
                    updated_by: Some("archivist".to_string()),
                    lifecycle_state: Some(POLICY_LIFECYCLE_RETIRED.to_string()),
                    tags: None,
                    ontology_refs: None,
                    shape_refs: None,
                    active: None,
                    provider_interface_id: None,
                    consumer_interface_id: None,
                    contract_id: None,
                    source_system_id: None,
                    target_system_id: None,
                    interface_id: None,
                },
            )
            .expect("retiring a policy should succeed");

        let error = service
            .update_policy(
                "retired-policy",
                UpdateSosPolicyRequest {
                    policy_name: None,
                    target_type: None,
                    stages: None,
                    enforcement_level: None,
                    severity: None,
                    sparql_query: None,
                    context: None,
                    description: None,
                    updated_by: Some("operator".to_string()),
                    lifecycle_state: Some(POLICY_LIFECYCLE_ACTIVE.to_string()),
                    tags: None,
                    ontology_refs: None,
                    shape_refs: None,
                    active: None,
                    provider_interface_id: None,
                    consumer_interface_id: None,
                    contract_id: None,
                    source_system_id: None,
                    target_system_id: None,
                    interface_id: None,
                },
            )
            .expect_err("retired policies should not be reactivated");

        assert!(matches!(
            error,
            SosValidationServiceError::InvalidRequest(message)
                if message.contains("cannot transition from 'retired'")
        ));
    }

    #[test]
    fn interface_validation_applies_active_pre_execution_policy() {
        let (_temp_dir, storage_manager, _rdf_store, service) = create_service_with_rdf();
        register_minimal_catalog(&storage_manager);
        service
            .reconcile_graphs()
            .expect("catalog graph should be projected");
        service
            .create_policy(sample_interface_pair_policy_request("pair-policy"))
            .expect("interface-pair policy should be created");

        let response = service
            .validate_spec(
                SosValidationSpec::InterfaceCompatibility {
                    provider_interface_id: "provider-if".to_string(),
                    consumer_interface_id: "consumer-if".to_string(),
                },
                ValidationExecutionOptions::persisted(),
            )
            .expect("validation should succeed");

        let report = storage_manager
            .get_validation_report(
                response
                    .report_id
                    .as_deref()
                    .expect("persisted validation should have report id"),
            )
            .expect("report lookup should succeed")
            .expect("report should exist");

        assert!(report
            .checks
            .iter()
            .any(|check| check.check_name == "policy:pair-policy"));
        assert!(report.checks.iter().any(|check| {
            check.check_name == "policy:pair-policy"
                && check
                    .details
                    .as_ref()
                    .and_then(|details| details.get("policy_revision"))
                    .and_then(Value::as_u64)
                    == Some(1)
        }));
        assert!(report
            .policy_refs
            .contains(&"policy:pair-policy".to_string()));
        assert!(report
            .policy_refs
            .contains(&"policy:pair-policy@1".to_string()));
    }

    #[test]
    fn data_validation_applies_active_in_flight_policy() {
        let (_temp_dir, storage_manager, _rdf_store, service) = create_service_with_rdf();
        register_minimal_catalog(&storage_manager);
        service
            .reconcile_graphs()
            .expect("catalog graph should be projected");
        service
            .create_policy(sample_interface_policy_request("payload-policy"))
            .expect("interface policy should be created");

        let response = service
            .validate_spec(
                SosValidationSpec::DataValidation {
                    interface_id: "provider-if".to_string(),
                    data: json!({
                        "sample_id": "row-2",
                        "score": 0.77
                    }),
                },
                ValidationExecutionOptions::persisted(),
            )
            .expect("data validation should succeed");

        let report = storage_manager
            .get_validation_report(
                response
                    .report_id
                    .as_deref()
                    .expect("persisted validation should have report id"),
            )
            .expect("report lookup should succeed")
            .expect("report should exist");

        assert!(report
            .checks
            .iter()
            .any(|check| check.check_name == "policy:payload-policy"));
        assert!(report
            .policy_refs
            .contains(&"policy:payload-policy".to_string()));
    }

    #[test]
    fn system_integration_propagates_nested_interface_confidence() {
        let (_temp_dir, storage_manager, service) = create_service();
        register_minimal_catalog(&storage_manager);
        storage_manager
            .put_contract(&sample_contract("contract-1", "provider-if", "consumer-if"))
            .expect("contract should be stored");

        let response = service
            .validate_spec(
                SosValidationSpec::SystemIntegration {
                    source_system_id: "provider-system".to_string(),
                    target_system_id: "consumer-system".to_string(),
                },
                ValidationExecutionOptions::dry_run(),
            )
            .expect("system integration validation should execute");

        assert!(response.passed);
        assert_eq!(response.compatibility_state, None);
        assert!(
            response.confidence < 1.0,
            "transform-backed path confidence should flow into the system-level score"
        );

        let path_check = response
            .checks
            .iter()
            .find(|check| check.check_name == "contract_path:provider-if:consumer-if")
            .expect("contract-backed path check should be present");
        let details = path_check
            .details
            .as_ref()
            .expect("path check should include nested confidence details");
        assert_eq!(
            details.get("nested_validation_type"),
            Some(&json!("interface_compatibility"))
        );
        assert_eq!(
            details.get("compatibility_state"),
            Some(&json!("transformable"))
        );
        let path_confidence = details
            .get("confidence_score")
            .and_then(Value::as_f64)
            .expect("nested path should expose a numeric confidence score");
        assert!(path_confidence > 0.9 && path_confidence < 1.0);
        assert_eq!(
            details.get("confidence_source"),
            Some(&json!("nested_validation"))
        );

        let confidence_assessment = response
            .confidence_assessment
            .as_ref()
            .expect("confidence assessment should be present");
        assert!(
            confidence_assessment
                .contributors
                .iter()
                .any(|contributor| contributor.check_name == "contract_path:provider-if:consumer-if"),
            "the nested path should be surfaced as a material confidence contributor"
        );
    }

    #[test]
    fn create_policy_allows_runtime_payload_placeholder_without_definition_binding() {
        let (_temp_dir, storage_manager, _rdf_store, service) = create_service_with_rdf();
        register_minimal_catalog(&storage_manager);
        service
            .reconcile_graphs()
            .expect("catalog graph should be projected");

        let mut request = sample_interface_policy_request("payload-json-policy");
        request.sparql_query = "ASK { GRAPH <http://graphica.io/graph/sos-catalog> { <{{interface_uri}}> <http://graphica.io/sos#belongsToSystem> ?system } } # {{payload }}"
            .to_string();
        let policy = service
            .create_policy(request)
            .expect("payload policy should be created");
        assert_eq!(policy.policy_id, "payload-json-policy");
    }

    #[test]
    fn persisted_report_includes_workflow_metadata() {
        let (_temp_dir, storage_manager, service) = create_service();
        register_minimal_catalog(&storage_manager);
        storage_manager
            .put_contract(&sample_contract("contract-1", "provider-if", "consumer-if"))
            .expect("contract should be stored");

        let response = service
            .validate_spec(
                SosValidationSpec::ContractCompliance {
                    contract_id: "contract-1".to_string(),
                },
                ValidationExecutionOptions {
                    persist_report: true,
                    emit_graph_lineage: false,
                    workflow_execution_id: Some("exec-42".to_string()),
                    workflow_step_id: Some("sos-step".to_string()),
                },
            )
            .expect("workflow-linked validation should succeed");

        let report_id = response
            .report_id
            .expect("persisted workflow-linked validation should return report_id");
        let report = storage_manager
            .get_validation_report(&report_id)
            .expect("report lookup should succeed")
            .expect("report should exist");

        assert_eq!(report.workflow_execution_id.as_deref(), Some("exec-42"));
        assert_eq!(report.workflow_step_id.as_deref(), Some("sos-step"));

        let linked_reports = storage_manager
            .list_validation_reports_by_workflow_execution("exec-42")
            .expect("workflow execution index lookup should succeed");
        assert_eq!(linked_reports.len(), 1);
        assert_eq!(linked_reports[0].report_id, report_id);
    }

    #[test]
    fn reconcile_graphs_replays_catalog_and_validation_history() {
        let temp_dir = TempDir::new().expect("temporary directory should be created");
        let storage_manager = Arc::new(
            SosStorageManager::new(
                temp_dir
                    .path()
                    .to_str()
                    .expect("temporary directory should be UTF-8"),
            )
            .expect("SoS storage manager should be created"),
        );
        let rdf_store = Arc::new(
            GraphicaRdfStore::new_in_memory().expect("in-memory RDF store should be created"),
        );
        let service =
            SosValidationService::new(storage_manager.clone(), Some(rdf_store.clone()), None);

        register_minimal_catalog(&storage_manager);
        storage_manager
            .put_contract(&sample_contract("contract-1", "provider-if", "consumer-if"))
            .expect("contract should be stored");

        let response = service
            .validate_spec(
                SosValidationSpec::ContractCompliance {
                    contract_id: "contract-1".to_string(),
                },
                ValidationExecutionOptions {
                    persist_report: true,
                    emit_graph_lineage: false,
                    workflow_execution_id: None,
                    workflow_step_id: None,
                },
            )
            .expect("persisted validation should succeed");
        let report_id = response
            .report_id
            .expect("persisted validation should return report_id");

        service
            .reconcile_graphs()
            .expect("graph reconciliation should succeed");

        let catalog_count = extract_count(
            &rdf_store
                .query(
                    "SELECT (COUNT(*) as ?count) WHERE { GRAPH <http://graphica.io/graph/sos-catalog> { ?s ?p ?o } }",
                )
                .expect("catalog graph query should succeed"),
        );
        assert!(catalog_count > 0);

        let validation_count = extract_count(
            &rdf_store
                .query(
                    "SELECT (COUNT(*) as ?count) WHERE { GRAPH <http://graphica.io/graph/sos-validations> { ?s ?p ?o } }",
                )
                .expect("validation graph query should succeed"),
        );
        assert!(validation_count > 0);

        let report_results = rdf_store
            .query(&format!(
                "SELECT ?p ?o WHERE {{ GRAPH <http://graphica.io/graph/sos-validations> {{ <{}> ?p ?o }} }}",
                projection::validation_report_uri(&report_id)
            ))
            .expect("report graph query should succeed");
        assert!(!report_results.is_empty());
    }

    #[test]
    fn incremental_projection_matches_full_reconcile_for_catalog_and_reports() {
        let temp_dir = TempDir::new().expect("temporary directory should be created");
        let storage_manager = Arc::new(
            SosStorageManager::new(
                temp_dir
                    .path()
                    .to_str()
                    .expect("temporary directory should be UTF-8"),
            )
            .expect("SoS storage manager should be created"),
        );
        let incremental_rdf = Arc::new(
            GraphicaRdfStore::new_in_memory().expect("incremental RDF store should be created"),
        );
        let reconcile_rdf = Arc::new(
            GraphicaRdfStore::new_in_memory().expect("reconcile RDF store should be created"),
        );
        let incremental_service =
            SosValidationService::new(storage_manager.clone(), Some(incremental_rdf.clone()), None);
        let reconcile_service =
            SosValidationService::new(storage_manager.clone(), Some(reconcile_rdf.clone()), None);

        let provider_system = sample_system("provider-system", "Provider");
        let consumer_system = sample_system("consumer-system", "Consumer");
        let provider_interface = sample_interface("provider-if", "provider-system", "SI");
        let consumer_interface = sample_interface("consumer-if", "consumer-system", "Imperial");
        let contract = sample_contract("contract-1", "provider-if", "consumer-if");

        for system in [&provider_system, &consumer_system] {
            storage_manager
                .put_system(system)
                .expect("system should be stored");
            incremental_service
                .project_system_upsert(system)
                .expect("system should be projected incrementally");
        }

        for interface in [&provider_interface, &consumer_interface] {
            storage_manager
                .put_interface(interface)
                .expect("interface should be stored");
            incremental_service
                .project_interface_upsert(interface)
                .expect("interface should be projected incrementally");
        }

        storage_manager
            .put_contract(&contract)
            .expect("contract should be stored");
        incremental_service
            .project_contract_upsert(&contract)
            .expect("contract should be projected incrementally");

        incremental_service
            .create_policy(sample_interface_pair_policy_request("pair-policy"))
            .expect("policy should be created and projected incrementally");

        incremental_service
            .validate_spec(
                SosValidationSpec::InterfaceCompatibility {
                    provider_interface_id: "provider-if".to_string(),
                    consumer_interface_id: "consumer-if".to_string(),
                },
                ValidationExecutionOptions::persisted(),
            )
            .expect("persisted validation should succeed");

        reconcile_service
            .reconcile_graphs()
            .expect("full reconciliation should succeed");

        assert_eq!(
            graph_triples(&incremental_rdf, "http://graphica.io/graph/sos-catalog"),
            graph_triples(&reconcile_rdf, "http://graphica.io/graph/sos-catalog")
        );
        assert_eq!(
            graph_triples(&incremental_rdf, "http://graphica.io/graph/sos-validations"),
            graph_triples(&reconcile_rdf, "http://graphica.io/graph/sos-validations")
        );
    }

    #[test]
    fn incremental_projection_replaces_updated_subjects_and_removes_deleted_entities() {
        let (_temp_dir, storage_manager, rdf_store, service) = create_service_with_rdf();

        let mut system = sample_system("provider-system", "Provider");
        storage_manager
            .put_system(&system)
            .expect("system should be stored");
        service
            .project_system_upsert(&system)
            .expect("system should be projected");

        let system_subject = projection::system_uri("provider-system");
        let system_name_predicate = format!("{}systemName", SOS_NS);
        assert_eq!(
            subject_objects(
                &rdf_store,
                "http://graphica.io/graph/sos-catalog",
                &system_subject,
                &system_name_predicate
            ),
            vec!["\"Provider\"".to_string()]
        );

        system.system_name = "Provider Renamed".to_string();
        system.updated_at = Utc::now();
        storage_manager
            .put_system(&system)
            .expect("updated system should be stored");
        service
            .project_system_upsert(&system)
            .expect("updated system should replace prior projection");

        let updated_names = subject_objects(
            &rdf_store,
            "http://graphica.io/graph/sos-catalog",
            &system_subject,
            &system_name_predicate,
        );
        assert_eq!(updated_names, vec!["\"Provider Renamed\"".to_string()]);

        let consumer_system = sample_system("consumer-system", "Consumer");
        let provider_interface = sample_interface("provider-if", "provider-system", "SI");
        let consumer_interface = sample_interface("consumer-if", "consumer-system", "Imperial");

        storage_manager
            .put_system(&consumer_system)
            .expect("consumer system should be stored");
        service
            .project_system_upsert(&consumer_system)
            .expect("consumer system should be projected");
        storage_manager
            .put_interface(&provider_interface)
            .expect("provider interface should be stored");
        service
            .project_interface_upsert(&provider_interface)
            .expect("provider interface should be projected");
        storage_manager
            .put_interface(&consumer_interface)
            .expect("consumer interface should be stored");
        service
            .project_interface_upsert(&consumer_interface)
            .expect("consumer interface should be projected");

        let contract = sample_contract("contract-1", "provider-if", "consumer-if");
        storage_manager
            .put_contract(&contract)
            .expect("contract should be stored");
        service
            .project_contract_upsert(&contract)
            .expect("contract should be projected");

        let contract_subject = projection::contract_uri("contract-1");
        assert!(
            !graph_triples(&rdf_store, "http://graphica.io/graph/sos-catalog")
                .into_iter()
                .filter(|(subject, _, _)| subject == &contract_subject)
                .collect::<Vec<_>>()
                .is_empty()
        );

        storage_manager
            .delete_contract("contract-1", "provider-if", "consumer-if")
            .expect("contract should be deleted");
        service
            .project_contract_delete("contract-1")
            .expect("contract projection should be deleted");

        assert!(
            graph_triples(&rdf_store, "http://graphica.io/graph/sos-catalog")
                .into_iter()
                .all(|(subject, _, _)| subject != contract_subject)
        );
    }

    #[test]
    fn validation_report_retention_prunes_storage_and_graph_lineage() {
        let (_temp_dir, storage_manager, rdf_store, service) = create_service_with_rdf();
        let service = service.with_retention_config(retention::ValidationReportRetentionConfig {
            pruning_enabled: true,
            max_reports_per_subject: 2,
            max_report_age_days: None,
        });
        register_minimal_catalog(&storage_manager);

        let mut report_ids = Vec::new();
        for _ in 0..3 {
            let response = service
                .validate_spec(
                    SosValidationSpec::InterfaceCompatibility {
                        provider_interface_id: "provider-if".to_string(),
                        consumer_interface_id: "consumer-if".to_string(),
                    },
                    ValidationExecutionOptions::persisted(),
                )
                .expect("persisted validation should succeed");
            report_ids.push(
                response
                    .report_id
                    .expect("persisted validation should return report_id"),
            );
            std::thread::sleep(std::time::Duration::from_millis(2));
        }

        assert!(
            storage_manager
                .get_validation_report(&report_ids[0])
                .expect("pruned report lookup should succeed")
                .is_none(),
            "oldest report should be pruned from primary storage"
        );

        let history = storage_manager
            .list_validation_history("interface_pair:provider-if:consumer-if", 10)
            .expect("validation history lookup should succeed");
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].report_id, report_ids[2]);
        assert_eq!(history[1].report_id, report_ids[1]);

        let latest = storage_manager
            .get_latest_validation_report("interface_pair:provider-if:consumer-if")
            .expect("latest report lookup should succeed")
            .expect("latest report should remain present");
        assert_eq!(latest.report_id, report_ids[2]);

        let pruned_report_uri = projection::validation_report_uri(&report_ids[0]);
        assert!(
            graph_triples(&rdf_store, "http://graphica.io/graph/sos-validations")
                .into_iter()
                .all(|(subject, _, _)| subject != pruned_report_uri),
            "pruned report subject should be removed from validation graph"
        );
    }

    #[test]
    fn reconcile_graphs_replays_only_retained_validation_history_after_restart() {
        let temp_dir = TempDir::new().expect("temporary directory should be created");
        let storage_manager = Arc::new(
            SosStorageManager::new(
                temp_dir
                    .path()
                    .to_str()
                    .expect("temporary directory should be UTF-8"),
            )
            .expect("SoS storage manager should be created"),
        );
        let initial_rdf = Arc::new(
            GraphicaRdfStore::new_in_memory().expect("initial RDF store should be created"),
        );
        let replay_rdf = Arc::new(
            GraphicaRdfStore::new_in_memory().expect("replay RDF store should be created"),
        );
        let initial_service =
            SosValidationService::new(storage_manager.clone(), Some(initial_rdf.clone()), None)
                .with_retention_config(retention::ValidationReportRetentionConfig {
                    pruning_enabled: true,
                    max_reports_per_subject: 2,
                    max_report_age_days: None,
                });
        let replay_service =
            SosValidationService::new(storage_manager.clone(), Some(replay_rdf.clone()), None);

        register_minimal_catalog(&storage_manager);

        let mut report_ids = Vec::new();
        for _ in 0..3 {
            let response = initial_service
                .validate_spec(
                    SosValidationSpec::InterfaceCompatibility {
                        provider_interface_id: "provider-if".to_string(),
                        consumer_interface_id: "consumer-if".to_string(),
                    },
                    ValidationExecutionOptions::persisted(),
                )
                .expect("persisted validation should succeed");
            report_ids.push(
                response
                    .report_id
                    .expect("persisted validation should return report_id"),
            );
            std::thread::sleep(std::time::Duration::from_millis(2));
        }

        let history = storage_manager
            .list_validation_history("interface_pair:provider-if:consumer-if", 10)
            .expect("validation history lookup should succeed");
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].report_id, report_ids[2]);
        assert_eq!(history[1].report_id, report_ids[1]);
        assert!(
            storage_manager
                .get_validation_report(&report_ids[0])
                .expect("pruned report lookup should succeed")
                .is_none(),
            "oldest report should remain pruned in storage before replay"
        );

        replay_service
            .reconcile_graphs()
            .expect("restart replay should rebuild RDF graphs");

        let replayed_validation_graph =
            graph_triples(&replay_rdf, "http://graphica.io/graph/sos-validations");
        let pruned_report_uri = projection::validation_report_uri(&report_ids[0]);
        let retained_report_uris = [
            projection::validation_report_uri(&report_ids[1]),
            projection::validation_report_uri(&report_ids[2]),
        ];

        assert!(
            replayed_validation_graph
                .iter()
                .all(|(subject, _, _)| subject != &pruned_report_uri),
            "restart replay should not resurrect pruned validation lineage"
        );
        for retained_report_uri in retained_report_uris {
            assert!(
                replayed_validation_graph
                    .iter()
                    .any(|(subject, _, _)| subject == &retained_report_uri),
                "restart replay should restore retained validation lineage subjects"
            );
        }
    }

    #[test]
    fn sos_runtime_metrics_record_validation_history_analytics_and_projection() {
        use prometheus::{Encoder, Registry, TextEncoder};

        let (_temp_dir, storage_manager, service) = create_service();
        let metrics_registry = Registry::new();
        let service = service.with_metrics(Some(
            SosMetrics::new(&metrics_registry).expect("SoS metrics should register"),
        ));
        register_minimal_catalog(&storage_manager);

        let response = service
            .validate_spec(
                SosValidationSpec::InterfaceCompatibility {
                    provider_interface_id: "provider-if".to_string(),
                    consumer_interface_id: "consumer-if".to_string(),
                },
                ValidationExecutionOptions::persisted(),
            )
            .expect("persisted validation should succeed");
        let expected_result = if response.passed { "passed" } else { "failed" };
        service
            .get_validation_history("interface_pair:provider-if:consumer-if", None, 10)
            .expect("validation history should be available");
        service
            .build_compatibility_matrix()
            .expect("compatibility matrix should build");

        let mut metrics_text = Vec::new();
        TextEncoder::new()
            .encode(&metrics_registry.gather(), &mut metrics_text)
            .expect("metrics should encode");
        let metrics_text =
            String::from_utf8(metrics_text).expect("metrics payload should be valid UTF-8");

        assert!(metrics_text.contains("graphica_sos_validations_total"));
        assert!(metrics_text.contains("validation_type=\"interface_compatibility\""));
        assert!(metrics_text.contains(&format!("result=\"{expected_result}\"")));
        assert!(metrics_text.contains("graphica_sos_validation_reports_persisted_total"));
        assert!(metrics_text.contains("graphica_sos_projection_duration_seconds"));
        assert!(metrics_text.contains("entity_type=\"validation_report\""));
        assert!(metrics_text.contains("graphica_sos_validation_history_length"));
        assert!(metrics_text.contains("graphica_sos_analytics_duration_seconds"));
        assert!(metrics_text.contains("operation=\"compatibility_matrix\""));
    }

    #[test]
    fn compatibility_matrix_uses_fresh_persisted_reports() {
        let (_temp_dir, storage_manager, service) = create_service();
        register_minimal_catalog(&storage_manager);

        let provider = storage_manager
            .get_interface("provider-if")
            .expect("provider interface lookup should succeed")
            .expect("provider interface should exist");
        let consumer = storage_manager
            .get_interface("consumer-if")
            .expect("consumer interface lookup should succeed")
            .expect("consumer interface should exist");
        let expected_hashes = service
            .current_interface_pair_schema_hashes(&provider, &consumer, None)
            .expect("schema hashes should be computed");

        storage_manager
            .put_validation_report(&ValidationReport {
                report_id: "persisted-report".to_string(),
                validation_id: "persisted-validation".to_string(),
                subject_type: "interface_pair".to_string(),
                subject_key: "interface_pair:provider-if:consumer-if".to_string(),
                validation_type: "interface_compatibility".to_string(),
                passed: true,
                confidence: 0.42,
                checks: vec![ValidationCheckRecord {
                    check_name: "persisted_check".to_string(),
                    passed: true,
                    severity: "info".to_string(),
                    description: "Loaded from persisted report".to_string(),
                    details: None,
                }],
                validated_at: Utc::now(),
                previous_report_id: None,
                change_summary: ValidationChangeSummary::default(),
                workflow_execution_id: None,
                workflow_step_id: None,
                ontology_refs: vec!["sos_core".to_string()],
                shape_refs: vec![
                    "http://graphica.io/sos/interface/provider-if/shape/test".to_string()
                ],
                policy_refs: Vec::new(),
                contract_refs: Vec::new(),
                schema_hashes: expected_hashes,
            })
            .expect("persisted validation report should be stored");

        let matrix = service
            .build_compatibility_matrix()
            .expect("compatibility matrix should build");
        let persisted_entry = matrix
            .matrix
            .into_iter()
            .find(|entry| {
                entry.provider_interface_id == "provider-if"
                    && entry.consumer_interface_id == "consumer-if"
            })
            .expect("provider/consumer matrix entry should exist");

        assert_eq!(persisted_entry.score, 0.42);
        assert_eq!(persisted_entry.compatibility_state, None);
        assert_eq!(persisted_entry.details.len(), 1);
        assert_eq!(persisted_entry.details[0].aspect, "persisted_check");
    }

    #[test]
    fn compatibility_matrix_budget_truncates_deterministically() {
        let (_temp_dir, storage_manager, service) = create_service();
        register_minimal_catalog(&storage_manager);

        let first = service
            .build_compatibility_matrix_with_query(CompatibilityMatrixQuery {
                evaluation_budget: Some(1),
            })
            .expect("budgeted compatibility matrix should build");
        let second = service
            .build_compatibility_matrix_with_query(CompatibilityMatrixQuery {
                evaluation_budget: Some(1),
            })
            .expect("repeated budgeted compatibility matrix should build");

        assert_eq!(first.matrix, second.matrix);
        assert_eq!(first.metadata.total_interfaces, 2);
        assert_eq!(first.metadata.total_candidate_pairs, 2);
        assert_eq!(first.metadata.evaluated_pairs, 1);
        assert_eq!(first.metadata.remaining_candidate_pairs, 1);
        assert!(first.metadata.truncated);
        assert_eq!(first.metadata.requested_evaluation_budget, Some(1));
        assert_eq!(first.metadata.applied_evaluation_budget, 1);
        assert_eq!(first.matrix.len(), 1);
        assert_ne!(
            first.matrix[0].provider_interface_id,
            first.matrix[0].consumer_interface_id
        );
    }

    #[test]
    fn dependency_graph_budget_truncates_deterministically() {
        let (_temp_dir, storage_manager, service) = create_service();
        register_minimal_catalog(&storage_manager);

        let first = service
            .build_dependency_graph_with_query(DependencyGraphQuery {
                node_budget: Some(1),
                edge_budget: Some(1),
            })
            .expect("budgeted dependency graph should build");
        let second = service
            .build_dependency_graph_with_query(DependencyGraphQuery {
                node_budget: Some(1),
                edge_budget: Some(1),
            })
            .expect("repeated budgeted dependency graph should build");

        assert_eq!(first.nodes, second.nodes);
        assert_eq!(first.edges, second.edges);
        assert_eq!(first.metadata, second.metadata);
        assert_eq!(first.metadata.total_nodes, 4);
        assert_eq!(first.metadata.total_edges, 2);
        assert_eq!(first.metadata.returned_nodes, 1);
        assert_eq!(first.metadata.returned_edges, 1);
        assert_eq!(first.metadata.remaining_nodes, 3);
        assert_eq!(first.metadata.remaining_edges, 1);
        assert!(first.metadata.truncated);
        assert_eq!(first.metadata.requested_node_budget, Some(1));
        assert_eq!(first.metadata.applied_node_budget, 1);
        assert_eq!(first.metadata.requested_edge_budget, Some(1));
        assert_eq!(first.metadata.applied_edge_budget, 1);
        assert_eq!(first.nodes.len(), 1);
        assert_eq!(first.edges.len(), 1);
    }

    #[test]
    fn what_if_analysis_recomputes_overlay_compatibility() {
        let (_temp_dir, storage_manager, service) = create_service();
        register_minimal_catalog(&storage_manager);

        let response = service
            .run_what_if_analysis(WhatIfRequest {
                scenario: "Align consumer units with provider".to_string(),
                changes: vec![json!({
                    "entity_type": "interface",
                    "interface_id": "consumer-if",
                    "system_id": "consumer-system",
                    "unit_system": "SI"
                })],
                evaluation_budget: None,
            })
            .expect("what-if analysis should succeed");

        assert!(response
            .affected_entities
            .contains(&"consumer-if".to_string()));
        assert!(response.impact.iter().any(|message| {
            message.contains("Interface compatibility 'provider-if -> consumer-if'")
                && message.contains("failing to passing")
        }));
        assert!(response.recommendations.iter().any(|message| {
            message.contains("system pair")
                || message.contains("interface pair")
                || message.contains("No corrective actions")
        }));
    }

    #[test]
    fn what_if_analysis_budget_truncates_deterministically() {
        let (_temp_dir, storage_manager, service) = create_service();
        register_minimal_catalog(&storage_manager);

        let response = service
            .run_what_if_analysis(WhatIfRequest {
                scenario: "Constrain evaluation budget".to_string(),
                changes: vec![json!({
                    "entity_type": "interface",
                    "interface_id": "consumer-if",
                    "system_id": "consumer-system",
                    "unit_system": "SI"
                })],
                evaluation_budget: Some(1),
            })
            .expect("what-if analysis should succeed");

        assert_eq!(response.metadata.total_candidate_evaluations, 4);
        assert_eq!(response.metadata.evaluated_candidate_evaluations, 1);
        assert_eq!(response.metadata.remaining_candidate_evaluations, 3);
        assert!(response.metadata.truncated);
        assert_eq!(response.metadata.requested_evaluation_budget, Some(1));
        assert_eq!(response.metadata.applied_evaluation_budget, 1);
        assert!(response
            .impact
            .iter()
            .any(|message| message.contains("evaluated 1 of 4 candidate checks")));
        assert!(response
            .recommendations
            .iter()
            .any(|message| { message.contains("Increase the what-if evaluation budget") }));
    }
}
