//! SoS Storage Manager
//!
//! High-level API for managing SoS entities with RocksDB + optional RDF integration

use super::rocks_store::{
    Contract, ContractApprovalEvidenceRecord, ContractApprovalRequestRecord,
    ContractSignatureRecord, Interface, PolicyApprovalEvidenceRecord, PolicyApprovalRequestRecord,
    PolicyAttestationRecord, SosPolicy, SosStore, System, ValidationReport,
};
use anyhow::Result;
use chrono::{DateTime, Utc};
use std::sync::Arc;

/// SoS Storage Manager - coordinates RocksDB and RDF storage
pub struct SosStorageManager {
    /// High-performance RocksDB store
    store: Arc<SosStore>,
}

impl SosStorageManager {
    /// Create new storage manager with RocksDB backend
    pub fn new(rocks_path: &str) -> Result<Self> {
        let store = Arc::new(SosStore::new(rocks_path)?);

        Ok(Self { store })
    }

    /// Get reference to underlying RocksDB store
    pub fn store(&self) -> &Arc<SosStore> {
        &self.store
    }

    // ========================================================================
    // System Operations (delegates to RocksDB with future RDF integration)
    // ========================================================================

    pub fn put_system(&self, system: &System) -> Result<()> {
        self.store.put_system(system)
    }

    pub fn get_system(&self, system_id: &str) -> Result<Option<System>> {
        self.store.get_system(system_id)
    }

    pub fn list_systems_by_type(&self, system_type: &str, limit: usize) -> Result<Vec<System>> {
        self.store.list_systems_by_type(system_type, limit)
    }

    pub fn list_all_systems(&self, offset: usize, limit: usize) -> Result<Vec<System>> {
        self.store.list_all_systems(offset, limit)
    }

    pub fn delete_system(&self, system_id: &str, system_type: &str, vendor: &str) -> Result<()> {
        self.store.delete_system(system_id, system_type, vendor)
    }

    // ========================================================================
    // Interface Operations
    // ========================================================================

    pub fn put_interface(&self, interface: &Interface) -> Result<()> {
        self.store.put_interface(interface)
    }

    pub fn get_interface(&self, interface_id: &str) -> Result<Option<Interface>> {
        self.store.get_interface(interface_id)
    }

    pub fn list_interfaces_by_system(&self, system_id: &str) -> Result<Vec<Interface>> {
        self.store.list_interfaces_by_system(system_id)
    }

    pub fn delete_interface(&self, interface_id: &str, system_id: &str) -> Result<()> {
        self.store.delete_interface(interface_id, system_id)
    }

    pub fn list_all_interfaces(&self, offset: usize, limit: usize) -> Result<Vec<Interface>> {
        self.store.list_all_interfaces(offset, limit)
    }

    // ========================================================================
    // Contract Operations
    // ========================================================================

    pub fn put_contract(&self, contract: &Contract) -> Result<()> {
        self.store.put_contract(contract)
    }

    pub fn get_contract(&self, contract_id: &str) -> Result<Option<Contract>> {
        self.store.get_contract(contract_id)
    }

    pub fn get_contract_revision(
        &self,
        contract_id: &str,
        revision: u32,
    ) -> Result<Option<Contract>> {
        self.store.get_contract_revision(contract_id, revision)
    }

    pub fn list_contract_revisions(
        &self,
        contract_id: &str,
        limit: usize,
    ) -> Result<Vec<Contract>> {
        self.store.list_contract_revisions(contract_id, limit)
    }

    pub fn list_all_contract_revisions(&self) -> Result<Vec<Contract>> {
        self.store.list_all_contract_revisions()
    }

    pub fn get_contract_by_interface_pair(
        &self,
        provider_interface_id: &str,
        consumer_interface_id: &str,
    ) -> Result<Option<Contract>> {
        self.store
            .get_contract_by_interface_pair(provider_interface_id, consumer_interface_id)
    }

    pub fn list_contracts_by_provider(&self, provider_interface_id: &str) -> Result<Vec<Contract>> {
        self.store.list_contracts_by_provider(provider_interface_id)
    }

    pub fn list_contracts_by_consumer(&self, consumer_interface_id: &str) -> Result<Vec<Contract>> {
        self.store.list_contracts_by_consumer(consumer_interface_id)
    }

    pub fn list_all_contracts(&self, offset: usize, limit: usize) -> Result<Vec<Contract>> {
        self.store.list_all_contracts(offset, limit)
    }

    pub fn delete_contract(
        &self,
        contract_id: &str,
        provider_interface_id: &str,
        consumer_interface_id: &str,
    ) -> Result<()> {
        self.store
            .delete_contract(contract_id, provider_interface_id, consumer_interface_id)
    }

    pub fn put_contract_approval_request(
        &self,
        request: &ContractApprovalRequestRecord,
    ) -> Result<()> {
        self.store.put_contract_approval_request(request)
    }

    pub fn get_contract_approval_request(
        &self,
        request_id: &str,
    ) -> Result<Option<ContractApprovalRequestRecord>> {
        self.store.get_contract_approval_request(request_id)
    }

    pub fn list_contract_approval_requests(
        &self,
        contract_id: &str,
        limit: usize,
    ) -> Result<Vec<ContractApprovalRequestRecord>> {
        self.store
            .list_contract_approval_requests(contract_id, limit)
    }

    pub fn list_all_contract_approval_requests(
        &self,
    ) -> Result<Vec<ContractApprovalRequestRecord>> {
        self.store.list_all_contract_approval_requests()
    }

    pub fn put_contract_approval_evidence(
        &self,
        evidence: &ContractApprovalEvidenceRecord,
    ) -> Result<()> {
        self.store.put_contract_approval_evidence(evidence)
    }

    pub fn get_contract_approval_evidence(
        &self,
        evidence_id: &str,
    ) -> Result<Option<ContractApprovalEvidenceRecord>> {
        self.store.get_contract_approval_evidence(evidence_id)
    }

    pub fn list_contract_approval_evidence(
        &self,
        request_id: &str,
    ) -> Result<Vec<ContractApprovalEvidenceRecord>> {
        self.store.list_contract_approval_evidence(request_id)
    }

    pub fn list_all_contract_approval_evidence(
        &self,
    ) -> Result<Vec<ContractApprovalEvidenceRecord>> {
        self.store.list_all_contract_approval_evidence()
    }

    pub fn put_contract_signature(&self, signature: &ContractSignatureRecord) -> Result<()> {
        self.store.put_contract_signature(signature)
    }

    pub fn get_contract_signature(
        &self,
        contract_id: &str,
        revision: u32,
    ) -> Result<Option<ContractSignatureRecord>> {
        self.store.get_contract_signature(contract_id, revision)
    }

    pub fn list_contract_signatures(
        &self,
        contract_id: &str,
        limit: usize,
    ) -> Result<Vec<ContractSignatureRecord>> {
        self.store.list_contract_signatures(contract_id, limit)
    }

    pub fn list_all_contract_signatures(&self) -> Result<Vec<ContractSignatureRecord>> {
        self.store.list_all_contract_signatures()
    }

    // ========================================================================
    // Policy Operations
    // ========================================================================

    pub fn put_policy(&self, policy: &SosPolicy) -> Result<()> {
        self.store.put_policy(policy)
    }

    pub fn get_policy(&self, policy_id: &str) -> Result<Option<SosPolicy>> {
        self.store.get_policy(policy_id)
    }

    pub fn get_policy_revision(&self, policy_id: &str, revision: u32) -> Result<Option<SosPolicy>> {
        self.store.get_policy_revision(policy_id, revision)
    }

    pub fn list_all_policies(&self, offset: usize, limit: usize) -> Result<Vec<SosPolicy>> {
        self.store.list_all_policies(offset, limit)
    }

    pub fn list_policy_revisions(&self, policy_id: &str, limit: usize) -> Result<Vec<SosPolicy>> {
        self.store.list_policy_revisions(policy_id, limit)
    }

    pub fn list_all_policy_revisions(&self) -> Result<Vec<SosPolicy>> {
        self.store.list_all_policy_revisions()
    }

    pub fn list_policies_by_stage(&self, stage: &str, limit: usize) -> Result<Vec<SosPolicy>> {
        self.store.list_policies_by_stage(stage, limit)
    }

    pub fn delete_policy(&self, policy_id: &str) -> Result<()> {
        self.store.delete_policy(policy_id)
    }

    pub fn put_policy_approval_request(&self, request: &PolicyApprovalRequestRecord) -> Result<()> {
        self.store.put_policy_approval_request(request)
    }

    pub fn get_policy_approval_request(
        &self,
        request_id: &str,
    ) -> Result<Option<PolicyApprovalRequestRecord>> {
        self.store.get_policy_approval_request(request_id)
    }

    pub fn list_policy_approval_requests(
        &self,
        policy_id: &str,
        limit: usize,
    ) -> Result<Vec<PolicyApprovalRequestRecord>> {
        self.store.list_policy_approval_requests(policy_id, limit)
    }

    pub fn list_all_policy_approval_requests(&self) -> Result<Vec<PolicyApprovalRequestRecord>> {
        self.store.list_all_policy_approval_requests()
    }

    pub fn put_policy_approval_evidence(
        &self,
        evidence: &PolicyApprovalEvidenceRecord,
    ) -> Result<()> {
        self.store.put_policy_approval_evidence(evidence)
    }

    pub fn get_policy_approval_evidence(
        &self,
        evidence_id: &str,
    ) -> Result<Option<PolicyApprovalEvidenceRecord>> {
        self.store.get_policy_approval_evidence(evidence_id)
    }

    pub fn list_policy_approval_evidence(
        &self,
        request_id: &str,
    ) -> Result<Vec<PolicyApprovalEvidenceRecord>> {
        self.store.list_policy_approval_evidence(request_id)
    }

    pub fn list_all_policy_approval_evidence(&self) -> Result<Vec<PolicyApprovalEvidenceRecord>> {
        self.store.list_all_policy_approval_evidence()
    }

    pub fn put_policy_attestation(&self, attestation: &PolicyAttestationRecord) -> Result<()> {
        self.store.put_policy_attestation(attestation)
    }

    pub fn get_policy_attestation(
        &self,
        policy_id: &str,
        revision: u32,
    ) -> Result<Option<PolicyAttestationRecord>> {
        self.store.get_policy_attestation(policy_id, revision)
    }

    pub fn list_policy_attestations(
        &self,
        policy_id: &str,
        limit: usize,
    ) -> Result<Vec<PolicyAttestationRecord>> {
        self.store.list_policy_attestations(policy_id, limit)
    }

    pub fn list_all_policy_attestations(&self) -> Result<Vec<PolicyAttestationRecord>> {
        self.store.list_all_policy_attestations()
    }

    // ========================================================================
    // Validation Report Operations
    // ========================================================================

    pub fn put_validation_report(&self, report: &ValidationReport) -> Result<()> {
        self.store.put_validation_report(report)
    }

    pub fn get_validation_report(&self, report_id: &str) -> Result<Option<ValidationReport>> {
        self.store.get_validation_report(report_id)
    }

    pub fn get_latest_validation_report(
        &self,
        subject_key: &str,
    ) -> Result<Option<ValidationReport>> {
        self.store.get_latest_validation_report(subject_key)
    }

    pub fn list_validation_history(
        &self,
        subject_key: &str,
        limit: usize,
    ) -> Result<Vec<ValidationReport>> {
        self.store.list_validation_history(subject_key, limit)
    }

    pub fn list_validation_reports_by_workflow_execution(
        &self,
        workflow_execution_id: &str,
    ) -> Result<Vec<ValidationReport>> {
        self.store
            .list_validation_reports_by_workflow_execution(workflow_execution_id)
    }

    pub fn list_validation_reports_by_type(
        &self,
        validation_type: &str,
        limit: usize,
    ) -> Result<Vec<ValidationReport>> {
        self.store
            .list_validation_reports_by_type(validation_type, limit)
    }

    pub fn list_all_validation_reports(&self) -> Result<Vec<ValidationReport>> {
        self.store.list_all_validation_reports()
    }

    pub fn delete_validation_report(&self, report_id: &str) -> Result<Option<ValidationReport>> {
        self.store.delete_validation_report(report_id)
    }

    pub fn prune_validation_reports_by_subject(
        &self,
        subject_key: &str,
        max_reports: usize,
        older_than: Option<DateTime<Utc>>,
    ) -> Result<Vec<ValidationReport>> {
        self.store
            .prune_validation_reports_by_subject(subject_key, max_reports, older_than)
    }
}
