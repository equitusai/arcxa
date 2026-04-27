//! High-performance RocksDB storage for SoS validation
//!
//! Tightly integrated with Graphica's storage layer using optimized column families,
//! batch writes, and efficient indexing for sub-millisecond lookups.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rocksdb::{IteratorMode, Options, WriteBatch, DB};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

use crate::storage::rocks_config::{self, RocksProfile};

// Column families for SoS data (optimized for high-speed access)
const CF_SYSTEMS: &str = "sos_systems"; // system_id -> System
const CF_INTERFACES: &str = "sos_interfaces"; // interface_id -> Interface
const CF_CONTRACTS: &str = "sos_contracts"; // contract_id -> Contract
const CF_CONTRACT_REVISIONS: &str = "sos_contract_revisions"; // (contract_id, revision) -> Contract snapshot
const CF_CONTRACT_APPROVAL_REQUESTS: &str = "sos_contract_approval_requests"; // request_id -> ContractApprovalRequestRecord
const CF_CONTRACT_APPROVAL_REQUESTS_BY_CONTRACT: &str =
    "sos_contract_approval_requests_by_contract"; // (contract_id, reverse_ts, request_id) -> request_id
const CF_CONTRACT_APPROVAL_EVIDENCE: &str = "sos_contract_approval_evidence"; // evidence_id -> ContractApprovalEvidenceRecord
const CF_CONTRACT_APPROVAL_EVIDENCE_BY_REQUEST: &str = "sos_contract_approval_evidence_by_request"; // (request_id, reverse_ts, evidence_id) -> evidence_id
const CF_CONTRACT_SIGNATURES: &str = "sos_contract_signatures"; // signature_id -> ContractSignatureRecord
const CF_CONTRACT_SIGNATURES_BY_CONTRACT: &str = "sos_contract_signatures_by_contract"; // (contract_id, revision) -> signature_id
const CF_POLICIES: &str = "sos_policies"; // policy_id -> SosPolicy
const CF_POLICY_REVISIONS: &str = "sos_policy_revisions"; // (policy_id, revision) -> SosPolicy snapshot
const CF_VALIDATION_REPORTS: &str = "sos_val_reports"; // report_id -> ValidationReport

// Secondary indexes for fast lookups
const CF_SYSTEM_BY_TYPE: &str = "sos_sys_type_idx"; // (type, system_id) -> empty
const CF_SYSTEM_BY_VENDOR: &str = "sos_sys_vendor_idx"; // (vendor, system_id) -> empty
const CF_INTERFACE_BY_SYSTEM: &str = "sos_if_sys_idx"; // (system_id, interface_id) -> empty
const CF_CONTRACT_BY_PROVIDER: &str = "sos_con_prov_idx"; // (provider_if_id, contract_id) -> empty
const CF_CONTRACT_BY_CONSUMER: &str = "sos_con_cons_idx"; // (consumer_if_id, contract_id) -> empty
const CF_CONTRACT_BY_INTERFACE_PAIR: &str = "sos_con_pair_idx"; // (provider_if_id, consumer_if_id, contract_id) -> empty
const CF_POLICY_BY_STAGE: &str = "sos_policy_stage_idx"; // (stage, policy_id) -> empty
const CF_POLICY_BY_TARGET_TYPE: &str = "sos_policy_target_idx"; // (target_type, policy_id) -> empty
const CF_POLICY_BY_ACTIVE: &str = "sos_policy_active_idx"; // (active, policy_id) -> empty
const CF_POLICY_APPROVAL_REQUESTS: &str = "sos_policy_approval_requests"; // request_id -> PolicyApprovalRequestRecord
const CF_POLICY_APPROVAL_REQUESTS_BY_POLICY: &str = "sos_policy_approval_requests_by_policy"; // (policy_id, reverse_ts, request_id) -> request_id
const CF_POLICY_APPROVAL_EVIDENCE: &str = "sos_policy_approval_evidence"; // evidence_id -> PolicyApprovalEvidenceRecord
const CF_POLICY_APPROVAL_EVIDENCE_BY_REQUEST: &str = "sos_policy_approval_evidence_by_request"; // (request_id, reverse_ts, evidence_id) -> evidence_id
const CF_POLICY_ATTESTATIONS: &str = "sos_policy_attestations"; // attestation_id -> PolicyAttestationRecord
const CF_POLICY_ATTESTATIONS_BY_POLICY: &str = "sos_policy_attestations_by_policy"; // (policy_id, revision) -> attestation_id
const CF_VALIDATION_LATEST_BY_SUBJECT: &str = "sos_val_latest_subject_idx"; // subject_key -> report_id
const CF_VALIDATION_HISTORY_BY_SUBJECT: &str = "sos_val_history_subject_idx"; // (subject_key, reverse_ts, report_id) -> report_id
const CF_VALIDATION_BY_WORKFLOW_EXECUTION: &str = "sos_val_workflow_exec_idx"; // (workflow_execution_id, reverse_ts, report_id) -> report_id
const CF_VALIDATION_BY_TYPE: &str = "sos_val_type_idx"; // (validation_type, reverse_ts, report_id) -> report_id

fn default_policy_revision() -> u32 {
    1
}

fn default_policy_actor() -> String {
    "system".to_string()
}

fn default_contract_revision() -> u32 {
    1
}

fn default_contract_actor() -> String {
    "system".to_string()
}

/// System entity (optimized for serialization speed)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct System {
    pub system_id: String,
    pub system_name: String,
    pub system_type: String,
    pub vendor: String,
    pub version: String,
    pub classification: String,
    pub description: Option<String>,
    pub deployment: HashMap<String, serde_json::Value>,
    pub capabilities: HashMap<String, serde_json::Value>,
    pub tags: Vec<String>,
    pub active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Interface entity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Interface {
    pub interface_id: String,
    pub system_id: String,
    pub interface_name: String,
    pub direction: String,
    pub protocol: String,
    pub data_format: String,
    pub schema: serde_json::Value,
    pub coordinate_system: Option<String>,
    pub unit_system: Option<String>,
    pub metadata: HashMap<String, serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Data contract entity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contract {
    pub contract_id: String,
    #[serde(default = "default_contract_revision")]
    pub revision: u32,
    pub contract_name: String,
    pub provider_interface_id: String,
    pub consumer_interface_id: String,
    pub sla_metrics: Vec<SlaMetric>,
    pub transformation_rules: HashMap<String, serde_json::Value>,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub approved: bool,
    pub signed: bool,
    #[serde(default)]
    pub lifecycle_state: Option<String>,
    #[serde(default)]
    pub approval_status: Option<String>,
    #[serde(default)]
    pub approval_requested_by: Option<String>,
    #[serde(default)]
    pub approval_requested_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub approved_by: Option<String>,
    #[serde(default)]
    pub approved_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub rejected_by: Option<String>,
    #[serde(default)]
    pub rejected_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub rejection_reason: Option<String>,
    #[serde(default)]
    pub signed_by: Option<String>,
    #[serde(default)]
    pub signed_at: Option<DateTime<Utc>>,
    #[serde(default = "default_contract_actor")]
    pub created_by: String,
    #[serde(default = "default_contract_actor")]
    pub updated_by: String,
    #[serde(default)]
    pub superseded_by_revision: Option<u32>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Persisted SoS policy definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SosPolicy {
    pub policy_id: String,
    #[serde(default = "default_policy_revision")]
    pub revision: u32,
    pub policy_name: String,
    pub description: Option<String>,
    #[serde(default)]
    pub lifecycle_state: Option<String>,
    #[serde(default)]
    pub approval_status: Option<String>,
    #[serde(default)]
    pub approval_requested_by: Option<String>,
    #[serde(default)]
    pub approval_requested_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub approved_by: Option<String>,
    #[serde(default)]
    pub approved_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub rejected_by: Option<String>,
    #[serde(default)]
    pub rejected_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub rejection_reason: Option<String>,
    pub target_type: String,
    pub target_key: Option<String>,
    pub stages: Vec<String>,
    pub enforcement_level: String,
    pub severity: String,
    pub sparql_query: String,
    pub context: HashMap<String, serde_json::Value>,
    pub tags: Vec<String>,
    pub ontology_refs: Vec<String>,
    pub shape_refs: Vec<String>,
    pub active: bool,
    pub provider_interface_id: Option<String>,
    pub consumer_interface_id: Option<String>,
    pub contract_id: Option<String>,
    pub source_system_id: Option<String>,
    pub target_system_id: Option<String>,
    pub interface_id: Option<String>,
    #[serde(default = "default_policy_actor")]
    pub created_by: String,
    #[serde(default = "default_policy_actor")]
    pub updated_by: String,
    #[serde(default)]
    pub superseded_by_revision: Option<u32>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// First-class approval request for a persisted SoS policy revision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyApprovalRequestRecord {
    pub request_id: String,
    pub policy_id: String,
    pub policy_revision: u32,
    pub approval_type: String,
    pub requested_lifecycle_state: String,
    pub status: String,
    pub note: Option<String>,
    pub requested_by: String,
    pub requested_at: DateTime<Utc>,
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub approved_by: Option<String>,
    #[serde(default)]
    pub approved_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub rejected_by: Option<String>,
    #[serde(default)]
    pub rejected_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub rejection_reason: Option<String>,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Evidence attached to a persisted policy approval request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyApprovalEvidenceRecord {
    pub evidence_id: String,
    pub request_id: String,
    pub policy_id: String,
    pub policy_revision: u32,
    pub evidence_type: String,
    pub report_id: String,
    pub added_by: String,
    pub added_at: DateTime<Utc>,
    pub note: Option<String>,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

/// First-class approval request for a persisted contract revision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractApprovalRequestRecord {
    pub request_id: String,
    pub contract_id: String,
    pub contract_revision: u32,
    pub approval_type: String,
    pub requested_lifecycle_state: String,
    pub status: String,
    pub note: Option<String>,
    pub requested_by: String,
    pub requested_at: DateTime<Utc>,
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub approved_by: Option<String>,
    #[serde(default)]
    pub approved_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub rejected_by: Option<String>,
    #[serde(default)]
    pub rejected_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub rejection_reason: Option<String>,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Evidence attached to a persisted contract approval request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractApprovalEvidenceRecord {
    pub evidence_id: String,
    pub request_id: String,
    pub contract_id: String,
    pub contract_revision: u32,
    pub evidence_type: String,
    pub report_id: String,
    pub added_by: String,
    pub added_at: DateTime<Utc>,
    pub note: Option<String>,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Cryptographic signature/attestation for one immutable contract revision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractSignatureRecord {
    pub signature_id: String,
    pub contract_id: String,
    pub contract_revision: u32,
    pub contract_revision_ref: String,
    pub payload_hash: String,
    pub payload_hash_algorithm: String,
    pub signature_algorithm: String,
    pub signature: String,
    pub public_key: String,
    pub key_fingerprint: String,
    #[serde(default)]
    pub signing_key_ref: Option<String>,
    #[serde(default)]
    pub signing_key_version: Option<String>,
    #[serde(default = "default_contract_signing_key_source")]
    pub signing_key_source: String,
    pub signed_by: String,
    pub signed_at: DateTime<Utc>,
    #[serde(default)]
    pub approval_request_id: Option<String>,
    #[serde(default)]
    pub evidence_ids: Vec<String>,
    #[serde(default)]
    pub policy_refs: Vec<String>,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

fn default_contract_signing_key_source() -> String {
    "embedded_public_key".to_string()
}

/// Cryptographic approval attestation for one immutable policy revision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyAttestationRecord {
    pub attestation_id: String,
    pub policy_id: String,
    pub policy_revision: u32,
    pub policy_revision_ref: String,
    pub payload_hash: String,
    pub payload_hash_algorithm: String,
    pub signature_algorithm: String,
    pub signature: String,
    pub public_key: String,
    pub key_fingerprint: String,
    #[serde(default)]
    pub signing_key_ref: Option<String>,
    #[serde(default)]
    pub signing_key_version: Option<String>,
    #[serde(default = "default_policy_signing_key_source")]
    pub signing_key_source: String,
    pub attested_by: String,
    pub attested_at: DateTime<Utc>,
    #[serde(default)]
    pub approval_request_id: Option<String>,
    #[serde(default)]
    pub evidence_ids: Vec<String>,
    #[serde(default)]
    pub policy_refs: Vec<String>,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

fn default_policy_signing_key_source() -> String {
    "embedded_public_key".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SlaMetric {
    pub name: String,
    pub value: f64,
    pub operator: String,
    pub unit: Option<String>,
}

/// Persisted validation check record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationCheckRecord {
    pub check_name: String,
    pub passed: bool,
    pub severity: String,
    pub description: String,
    pub details: Option<serde_json::Value>,
}

/// Summary of how a validation changed compared with the previous report.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ValidationChangeSummary {
    pub resolved_checks: Vec<String>,
    pub new_failures: Vec<String>,
    pub confidence_delta: f64,
    pub schema_or_policy_version_changed: bool,
}

/// Authoritative persisted validation report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationReport {
    pub report_id: String,
    pub validation_id: String,
    pub subject_type: String,
    pub subject_key: String,
    pub validation_type: String,
    pub passed: bool,
    pub confidence: f64,
    pub checks: Vec<ValidationCheckRecord>,
    pub validated_at: DateTime<Utc>,
    pub previous_report_id: Option<String>,
    pub change_summary: ValidationChangeSummary,
    pub workflow_execution_id: Option<String>,
    pub workflow_step_id: Option<String>,
    pub ontology_refs: Vec<String>,
    pub shape_refs: Vec<String>,
    pub policy_refs: Vec<String>,
    #[serde(default)]
    pub contract_refs: Vec<String>,
    pub schema_hashes: HashMap<String, String>,
}

/// High-performance SoS RocksDB store
pub struct SosStore {
    db: Arc<DB>,
}

impl SosStore {
    /// Create new SoS store with HighThroughput profile for maximum performance
    pub fn new(path: &str) -> Result<Self> {
        Self::with_profile(path, RocksProfile::HighThroughput)
    }

    /// Create SoS store with specified performance profile
    pub fn with_profile(path: &str, profile: RocksProfile) -> Result<Self> {
        // Print configuration summary
        rocks_config::print_config_summary(profile);

        // Get optimized options for profile
        let mut opts = rocks_config::create_options(profile);
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);

        // Define all column families
        let cfs = vec![
            CF_SYSTEMS,
            CF_INTERFACES,
            CF_CONTRACTS,
            CF_CONTRACT_REVISIONS,
            CF_CONTRACT_APPROVAL_REQUESTS,
            CF_CONTRACT_APPROVAL_REQUESTS_BY_CONTRACT,
            CF_CONTRACT_APPROVAL_EVIDENCE,
            CF_CONTRACT_APPROVAL_EVIDENCE_BY_REQUEST,
            CF_CONTRACT_SIGNATURES,
            CF_CONTRACT_SIGNATURES_BY_CONTRACT,
            CF_POLICIES,
            CF_POLICY_REVISIONS,
            CF_VALIDATION_REPORTS,
            CF_SYSTEM_BY_TYPE,
            CF_SYSTEM_BY_VENDOR,
            CF_INTERFACE_BY_SYSTEM,
            CF_CONTRACT_BY_PROVIDER,
            CF_CONTRACT_BY_CONSUMER,
            CF_CONTRACT_BY_INTERFACE_PAIR,
            CF_POLICY_BY_STAGE,
            CF_POLICY_BY_TARGET_TYPE,
            CF_POLICY_BY_ACTIVE,
            CF_POLICY_APPROVAL_REQUESTS,
            CF_POLICY_APPROVAL_REQUESTS_BY_POLICY,
            CF_POLICY_APPROVAL_EVIDENCE,
            CF_POLICY_APPROVAL_EVIDENCE_BY_REQUEST,
            CF_POLICY_ATTESTATIONS,
            CF_POLICY_ATTESTATIONS_BY_POLICY,
            CF_VALIDATION_LATEST_BY_SUBJECT,
            CF_VALIDATION_HISTORY_BY_SUBJECT,
            CF_VALIDATION_BY_WORKFLOW_EXECUTION,
            CF_VALIDATION_BY_TYPE,
        ];

        let db = DB::open_cf(&opts, path, cfs).context("Failed to open SoS RocksDB")?;

        Ok(Self { db: Arc::new(db) })
    }

    /// Get column family handle (optimized inline for hot path)
    #[inline(always)]
    fn cf(&self, name: &str) -> Result<&rocksdb::ColumnFamily> {
        self.db
            .cf_handle(name)
            .ok_or_else(|| anyhow::anyhow!("Column family {} not found", name))
    }

    /// Get reference to underlying DB (for sharing across repositories)
    pub fn db(&self) -> &Arc<DB> {
        &self.db
    }

    #[inline]
    fn reverse_timestamp(validated_at: &DateTime<Utc>) -> u64 {
        let millis = validated_at.timestamp_millis().max(0) as u64;
        u64::MAX.saturating_sub(millis)
    }

    #[inline]
    fn subject_history_key(
        subject_key: &str,
        validated_at: &DateTime<Utc>,
        report_id: &str,
    ) -> String {
        format!(
            "{}|{:020}|{}",
            subject_key,
            Self::reverse_timestamp(validated_at),
            report_id
        )
    }

    #[inline]
    fn workflow_execution_key(
        workflow_execution_id: &str,
        validated_at: &DateTime<Utc>,
        report_id: &str,
    ) -> String {
        format!(
            "{}|{:020}|{}",
            workflow_execution_id,
            Self::reverse_timestamp(validated_at),
            report_id
        )
    }

    #[inline]
    fn validation_type_key(
        validation_type: &str,
        validated_at: &DateTime<Utc>,
        report_id: &str,
    ) -> String {
        format!(
            "{}|{:020}|{}",
            validation_type,
            Self::reverse_timestamp(validated_at),
            report_id
        )
    }

    #[inline]
    fn policy_approval_request_policy_key(
        policy_id: &str,
        requested_at: &DateTime<Utc>,
        request_id: &str,
    ) -> String {
        format!(
            "{}|{:020}|{}",
            policy_id,
            Self::reverse_timestamp(requested_at),
            request_id
        )
    }

    #[inline]
    fn contract_approval_request_contract_key(
        contract_id: &str,
        requested_at: &DateTime<Utc>,
        request_id: &str,
    ) -> String {
        format!(
            "{}|{:020}|{}",
            contract_id,
            Self::reverse_timestamp(requested_at),
            request_id
        )
    }

    #[inline]
    fn policy_approval_evidence_request_key(
        request_id: &str,
        added_at: &DateTime<Utc>,
        evidence_id: &str,
    ) -> String {
        format!(
            "{}|{:020}|{}",
            request_id,
            Self::reverse_timestamp(added_at),
            evidence_id
        )
    }

    #[inline]
    fn contract_approval_evidence_request_key(
        request_id: &str,
        added_at: &DateTime<Utc>,
        evidence_id: &str,
    ) -> String {
        format!(
            "{}|{:020}|{}",
            request_id,
            Self::reverse_timestamp(added_at),
            evidence_id
        )
    }

    #[inline]
    fn contract_pair_key(
        provider_interface_id: &str,
        consumer_interface_id: &str,
        contract_id: &str,
    ) -> String {
        format!(
            "{}|{}|{}",
            provider_interface_id, consumer_interface_id, contract_id
        )
    }

    #[inline]
    fn policy_revision_key(policy_id: &str, revision: u32) -> String {
        format!("{}|{:010}", policy_id, revision)
    }

    #[inline]
    fn contract_revision_key(contract_id: &str, revision: u32) -> String {
        format!("{}|{:010}", contract_id, revision)
    }

    #[inline]
    fn contract_signature_contract_key(contract_id: &str, revision: u32) -> String {
        format!("{}|{:010}", contract_id, revision)
    }

    #[inline]
    fn policy_attestation_policy_key(policy_id: &str, revision: u32) -> String {
        format!("{}|{:010}", policy_id, revision)
    }

    // ========================================================================
    // System Operations (High-speed CRUD)
    // ========================================================================

    /// Store system (optimized for write throughput)
    pub fn put_system(&self, system: &System) -> Result<()> {
        let cf_systems = self.cf(CF_SYSTEMS)?;
        let cf_type_idx = self.cf(CF_SYSTEM_BY_TYPE)?;
        let cf_vendor_idx = self.cf(CF_SYSTEM_BY_VENDOR)?;

        // Load existing system (if any) to update secondary indexes correctly
        let existing = self
            .db
            .get_cf(cf_systems, system.system_id.as_bytes())?
            .map(|data| serde_json::from_slice::<System>(&data))
            .transpose()
            .context("Failed to deserialize existing system")?;

        // Serialize once for efficiency
        let data = serde_json::to_vec(system).context("Failed to serialize system")?;

        // Use batch writes for atomicity + performance
        let mut batch = WriteBatch::default();

        // Remove old secondary indexes if type/vendor changed
        if let Some(existing_system) = existing {
            if existing_system.system_type != system.system_type {
                let old_type_key = format!("{}|{}", existing_system.system_type, system.system_id);
                batch.delete_cf(cf_type_idx, old_type_key.as_bytes());
            }

            if existing_system.vendor != system.vendor {
                let old_vendor_key = format!("{}|{}", existing_system.vendor, system.system_id);
                batch.delete_cf(cf_vendor_idx, old_vendor_key.as_bytes());
            }
        }

        // Primary storage
        batch.put_cf(cf_systems, system.system_id.as_bytes(), &data);

        // Secondary indexes (empty values for space efficiency)
        let type_key = format!("{}|{}", system.system_type, system.system_id);
        batch.put_cf(cf_type_idx, type_key.as_bytes(), b"");

        let vendor_key = format!("{}|{}", system.vendor, system.system_id);
        batch.put_cf(cf_vendor_idx, vendor_key.as_bytes(), b"");

        self.db.write(batch).context("Failed to write system batch")
    }

    /// Get system by ID (sub-millisecond lookup)
    pub fn get_system(&self, system_id: &str) -> Result<Option<System>> {
        let cf = self.cf(CF_SYSTEMS)?;

        match self.db.get_cf(cf, system_id.as_bytes())? {
            Some(data) => {
                let system =
                    serde_json::from_slice(&data).context("Failed to deserialize system")?;
                Ok(Some(system))
            }
            None => Ok(None),
        }
    }

    /// List systems by type (uses secondary index for speed)
    pub fn list_systems_by_type(&self, system_type: &str, limit: usize) -> Result<Vec<System>> {
        let cf_type_idx = self.cf(CF_SYSTEM_BY_TYPE)?;
        let cf_systems = self.cf(CF_SYSTEMS)?;

        let prefix = format!("{}|", system_type);
        let mut systems = Vec::new();

        // Prefix scan on index (very fast)
        for item in self.db.iterator_cf(
            cf_type_idx,
            IteratorMode::From(prefix.as_bytes(), rocksdb::Direction::Forward),
        ) {
            let (key, _) = item?;
            let key_str = std::str::from_utf8(&key)?;

            if !key_str.starts_with(&prefix) {
                break;
            }

            // Extract system_id from "type|system_id" key
            if let Some(system_id) = key_str.split('|').nth(1) {
                if let Some(data) = self.db.get_cf(cf_systems, system_id.as_bytes())? {
                    let system: System = serde_json::from_slice(&data)?;
                    systems.push(system);

                    if systems.len() >= limit {
                        break;
                    }
                }
            }
        }

        Ok(systems)
    }

    /// Delete system (with index cleanup)
    pub fn delete_system(&self, system_id: &str, system_type: &str, vendor: &str) -> Result<()> {
        let cf_systems = self.cf(CF_SYSTEMS)?;
        let cf_type_idx = self.cf(CF_SYSTEM_BY_TYPE)?;
        let cf_vendor_idx = self.cf(CF_SYSTEM_BY_VENDOR)?;

        let mut batch = WriteBatch::default();

        // Delete from primary storage
        batch.delete_cf(cf_systems, system_id.as_bytes());

        // Delete from secondary indexes
        let type_key = format!("{}|{}", system_type, system_id);
        batch.delete_cf(cf_type_idx, type_key.as_bytes());

        let vendor_key = format!("{}|{}", vendor, system_id);
        batch.delete_cf(cf_vendor_idx, vendor_key.as_bytes());

        self.db.write(batch).context("Failed to delete system")
    }

    // ========================================================================
    // Interface Operations
    // ========================================================================

    /// Store interface
    pub fn put_interface(&self, interface: &Interface) -> Result<()> {
        let cf_interfaces = self.cf(CF_INTERFACES)?;
        let cf_sys_idx = self.cf(CF_INTERFACE_BY_SYSTEM)?;

        let data = serde_json::to_vec(interface).context("Failed to serialize interface")?;

        let mut batch = WriteBatch::default();

        // Primary storage
        batch.put_cf(cf_interfaces, interface.interface_id.as_bytes(), &data);

        // System index
        let sys_key = format!("{}|{}", interface.system_id, interface.interface_id);
        batch.put_cf(cf_sys_idx, sys_key.as_bytes(), b"");

        self.db
            .write(batch)
            .context("Failed to write interface batch")
    }

    /// Get interface by ID
    pub fn get_interface(&self, interface_id: &str) -> Result<Option<Interface>> {
        let cf = self.cf(CF_INTERFACES)?;

        match self.db.get_cf(cf, interface_id.as_bytes())? {
            Some(data) => {
                let interface =
                    serde_json::from_slice(&data).context("Failed to deserialize interface")?;
                Ok(Some(interface))
            }
            None => Ok(None),
        }
    }

    /// List interfaces by system (fast indexed lookup)
    pub fn list_interfaces_by_system(&self, system_id: &str) -> Result<Vec<Interface>> {
        let cf_sys_idx = self.cf(CF_INTERFACE_BY_SYSTEM)?;
        let cf_interfaces = self.cf(CF_INTERFACES)?;

        let prefix = format!("{}|", system_id);
        let mut interfaces = Vec::new();

        for item in self.db.iterator_cf(
            cf_sys_idx,
            IteratorMode::From(prefix.as_bytes(), rocksdb::Direction::Forward),
        ) {
            let (key, _) = item?;
            let key_str = std::str::from_utf8(&key)?;

            if !key_str.starts_with(&prefix) {
                break;
            }

            if let Some(interface_id) = key_str.split('|').nth(1) {
                if let Some(data) = self.db.get_cf(cf_interfaces, interface_id.as_bytes())? {
                    let interface: Interface = serde_json::from_slice(&data)?;
                    interfaces.push(interface);
                }
            }
        }

        Ok(interfaces)
    }

    /// Delete interface (with index cleanup)
    pub fn delete_interface(&self, interface_id: &str, system_id: &str) -> Result<()> {
        let cf_interfaces = self.cf(CF_INTERFACES)?;
        let cf_sys_idx = self.cf(CF_INTERFACE_BY_SYSTEM)?;

        let mut batch = WriteBatch::default();

        // Delete from primary storage
        batch.delete_cf(cf_interfaces, interface_id.as_bytes());

        // Delete from system index
        let sys_key = format!("{}|{}", system_id, interface_id);
        batch.delete_cf(cf_sys_idx, sys_key.as_bytes());

        self.db.write(batch).context("Failed to delete interface")
    }

    /// List all interfaces (with pagination for large datasets)
    pub fn list_all_interfaces(&self, offset: usize, limit: usize) -> Result<Vec<Interface>> {
        let cf = self.cf(CF_INTERFACES)?;
        let mut interfaces = Vec::new();
        let mut count = 0;

        for item in self.db.iterator_cf(cf, IteratorMode::Start) {
            let (_key, value) = item?;

            // Skip offset
            if count < offset {
                count += 1;
                continue;
            }

            let interface: Interface = serde_json::from_slice(&value)?;
            interfaces.push(interface);

            if interfaces.len() >= limit {
                break;
            }
        }

        Ok(interfaces)
    }

    // ========================================================================
    // Contract Operations
    // ========================================================================

    /// Store contract
    pub fn put_contract(&self, contract: &Contract) -> Result<()> {
        let cf_contracts = self.cf(CF_CONTRACTS)?;
        let cf_revisions = self.cf(CF_CONTRACT_REVISIONS)?;
        let cf_prov_idx = self.cf(CF_CONTRACT_BY_PROVIDER)?;
        let cf_cons_idx = self.cf(CF_CONTRACT_BY_CONSUMER)?;
        let cf_pair_idx = self.cf(CF_CONTRACT_BY_INTERFACE_PAIR)?;

        let existing = self
            .db
            .get_cf(cf_contracts, contract.contract_id.as_bytes())?
            .map(|data| serde_json::from_slice::<Contract>(&data))
            .transpose()
            .context("Failed to deserialize existing contract")?;

        let data = serde_json::to_vec(contract).context("Failed to serialize contract")?;

        let mut batch = WriteBatch::default();

        if let Some(existing_contract) = existing {
            if contract.revision < existing_contract.revision {
                return Err(anyhow::anyhow!(
                    "Contract '{}' revision must increase (existing {}, attempted {})",
                    contract.contract_id,
                    existing_contract.revision,
                    contract.revision
                ));
            }

            if contract.revision == existing_contract.revision
                && contract_semantics_require_new_revision(&existing_contract, contract)
            {
                return Err(anyhow::anyhow!(
                    "Contract '{}' semantic changes require a new revision (existing {}, attempted {})",
                    contract.contract_id,
                    existing_contract.revision,
                    contract.revision
                ));
            }

            if existing_contract.provider_interface_id != contract.provider_interface_id {
                let old_prov_key = format!(
                    "{}|{}",
                    existing_contract.provider_interface_id, existing_contract.contract_id
                );
                batch.delete_cf(cf_prov_idx, old_prov_key.as_bytes());
            }

            if existing_contract.consumer_interface_id != contract.consumer_interface_id {
                let old_cons_key = format!(
                    "{}|{}",
                    existing_contract.consumer_interface_id, existing_contract.contract_id
                );
                batch.delete_cf(cf_cons_idx, old_cons_key.as_bytes());
            }

            if existing_contract.provider_interface_id != contract.provider_interface_id
                || existing_contract.consumer_interface_id != contract.consumer_interface_id
            {
                let old_pair_key = Self::contract_pair_key(
                    &existing_contract.provider_interface_id,
                    &existing_contract.consumer_interface_id,
                    &existing_contract.contract_id,
                );
                batch.delete_cf(cf_pair_idx, old_pair_key.as_bytes());
            }

            if contract.revision > existing_contract.revision {
                let existing_revision_key = Self::contract_revision_key(
                    &existing_contract.contract_id,
                    existing_contract.revision,
                );
                let mut existing_revision = existing_contract.clone();
                existing_revision.lifecycle_state = Some("superseded".to_string());
                existing_revision.superseded_by_revision = Some(contract.revision);
                let existing_revision_data = serde_json::to_vec(&existing_revision)
                    .context("Failed to serialize superseded contract revision")?;
                batch.put_cf(
                    cf_revisions,
                    existing_revision_key.as_bytes(),
                    &existing_revision_data,
                );
            }
        }

        // Primary storage
        batch.put_cf(cf_contracts, contract.contract_id.as_bytes(), &data);
        let revision_key = Self::contract_revision_key(&contract.contract_id, contract.revision);
        batch.put_cf(cf_revisions, revision_key.as_bytes(), &data);

        // Provider index
        let prov_key = format!(
            "{}|{}",
            contract.provider_interface_id, contract.contract_id
        );
        batch.put_cf(cf_prov_idx, prov_key.as_bytes(), b"");

        // Consumer index
        let cons_key = format!(
            "{}|{}",
            contract.consumer_interface_id, contract.contract_id
        );
        batch.put_cf(cf_cons_idx, cons_key.as_bytes(), b"");

        // Direct interface-pair index. The contract_id suffix preserves ordering
        // for duplicate pairs and lets lookup avoid full contract scans.
        let pair_key = Self::contract_pair_key(
            &contract.provider_interface_id,
            &contract.consumer_interface_id,
            &contract.contract_id,
        );
        batch.put_cf(cf_pair_idx, pair_key.as_bytes(), b"");

        self.db
            .write(batch)
            .context("Failed to write contract batch")
    }

    /// Get contract by ID
    pub fn get_contract(&self, contract_id: &str) -> Result<Option<Contract>> {
        let cf = self.cf(CF_CONTRACTS)?;

        match self.db.get_cf(cf, contract_id.as_bytes())? {
            Some(data) => {
                let contract =
                    serde_json::from_slice(&data).context("Failed to deserialize contract")?;
                Ok(Some(contract))
            }
            None => Ok(None),
        }
    }

    /// Get a specific immutable contract revision snapshot.
    pub fn get_contract_revision(
        &self,
        contract_id: &str,
        revision: u32,
    ) -> Result<Option<Contract>> {
        let cf = self.cf(CF_CONTRACT_REVISIONS)?;
        let key = Self::contract_revision_key(contract_id, revision);

        match self.db.get_cf(cf, key.as_bytes())? {
            Some(data) => {
                let contract = serde_json::from_slice(&data)
                    .context("Failed to deserialize contract revision")?;
                Ok(Some(contract))
            }
            None => Ok(self
                .get_contract(contract_id)?
                .filter(|contract| contract.revision == revision)),
        }
    }

    /// List all persisted revisions for a logical contract ID, newest first.
    pub fn list_contract_revisions(
        &self,
        contract_id: &str,
        limit: usize,
    ) -> Result<Vec<Contract>> {
        let cf = self.cf(CF_CONTRACT_REVISIONS)?;
        let prefix = format!("{}|", contract_id);
        let mut contracts = Vec::new();

        for item in self.db.iterator_cf(
            cf,
            IteratorMode::From(prefix.as_bytes(), rocksdb::Direction::Forward),
        ) {
            let (key, value) = item?;
            let key_str = std::str::from_utf8(&key)?;

            if !key_str.starts_with(&prefix) {
                break;
            }

            let contract: Contract = serde_json::from_slice(&value)
                .context("Failed to deserialize contract revision")?;
            contracts.push(contract);
        }

        if contracts.is_empty() {
            if let Some(contract) = self.get_contract(contract_id)? {
                contracts.push(contract);
            }
        }

        contracts.sort_by(|left, right| right.revision.cmp(&left.revision));
        contracts.truncate(limit);
        Ok(contracts)
    }

    /// List every persisted immutable contract revision across the catalog.
    pub fn list_all_contract_revisions(&self) -> Result<Vec<Contract>> {
        let cf = self.cf(CF_CONTRACT_REVISIONS)?;
        let mut contracts = Vec::new();

        for item in self.db.iterator_cf(cf, IteratorMode::Start) {
            let (_key, value) = item?;
            let contract: Contract = serde_json::from_slice(&value)
                .context("Failed to deserialize contract revision")?;
            contracts.push(contract);
        }

        contracts.sort_by(|left, right| {
            left.contract_id
                .cmp(&right.contract_id)
                .then_with(|| right.revision.cmp(&left.revision))
        });
        Ok(contracts)
    }

    /// Get the first contract for an interface pair without scanning the full contract set.
    ///
    /// Existing databases may predate the pair index. When that happens, this
    /// falls back to the provider index, repairs the pair index lazily, and
    /// preserves the lexical contract_id ordering that the prior full scan used.
    pub fn get_contract_by_interface_pair(
        &self,
        provider_interface_id: &str,
        consumer_interface_id: &str,
    ) -> Result<Option<Contract>> {
        let cf_pair_idx = self.cf(CF_CONTRACT_BY_INTERFACE_PAIR)?;
        let cf_contracts = self.cf(CF_CONTRACTS)?;
        let prefix = format!("{}|{}|", provider_interface_id, consumer_interface_id);

        for item in self.db.iterator_cf(
            cf_pair_idx,
            IteratorMode::From(prefix.as_bytes(), rocksdb::Direction::Forward),
        ) {
            let (key, _) = item?;
            let key_str = std::str::from_utf8(&key)?;

            if !key_str.starts_with(&prefix) {
                break;
            }

            if let Some(contract_id) = key_str.rsplit('|').next() {
                if let Some(data) = self.db.get_cf(cf_contracts, contract_id.as_bytes())? {
                    let contract: Contract = serde_json::from_slice(&data)?;
                    return Ok(Some(contract));
                }
            }
        }

        let matching_contracts: Vec<Contract> = self
            .list_contracts_by_provider(provider_interface_id)?
            .into_iter()
            .filter(|contract| contract.consumer_interface_id == consumer_interface_id)
            .collect();

        if matching_contracts.is_empty() {
            return Ok(None);
        }

        let mut batch = WriteBatch::default();
        for contract in &matching_contracts {
            let pair_key = Self::contract_pair_key(
                &contract.provider_interface_id,
                &contract.consumer_interface_id,
                &contract.contract_id,
            );
            batch.put_cf(cf_pair_idx, pair_key.as_bytes(), b"");
        }
        self.db
            .write(batch)
            .context("Failed to backfill contract pair index")?;

        Ok(matching_contracts.into_iter().next())
    }

    /// List contracts by provider interface (fast indexed lookup)
    pub fn list_contracts_by_provider(&self, provider_interface_id: &str) -> Result<Vec<Contract>> {
        let cf_prov_idx = self.cf(CF_CONTRACT_BY_PROVIDER)?;
        let cf_contracts = self.cf(CF_CONTRACTS)?;

        let prefix = format!("{}|", provider_interface_id);
        let mut contracts = Vec::new();

        for item in self.db.iterator_cf(
            cf_prov_idx,
            IteratorMode::From(prefix.as_bytes(), rocksdb::Direction::Forward),
        ) {
            let (key, _) = item?;
            let key_str = std::str::from_utf8(&key)?;

            if !key_str.starts_with(&prefix) {
                break;
            }

            if let Some(contract_id) = key_str.split('|').nth(1) {
                if let Some(data) = self.db.get_cf(cf_contracts, contract_id.as_bytes())? {
                    let contract: Contract = serde_json::from_slice(&data)?;
                    contracts.push(contract);
                }
            }
        }

        Ok(contracts)
    }

    /// List contracts by consumer interface (fast indexed lookup)
    pub fn list_contracts_by_consumer(&self, consumer_interface_id: &str) -> Result<Vec<Contract>> {
        let cf_cons_idx = self.cf(CF_CONTRACT_BY_CONSUMER)?;
        let cf_contracts = self.cf(CF_CONTRACTS)?;

        let prefix = format!("{}|", consumer_interface_id);
        let mut contracts = Vec::new();

        for item in self.db.iterator_cf(
            cf_cons_idx,
            IteratorMode::From(prefix.as_bytes(), rocksdb::Direction::Forward),
        ) {
            let (key, _) = item?;
            let key_str = std::str::from_utf8(&key)?;

            if !key_str.starts_with(&prefix) {
                break;
            }

            if let Some(contract_id) = key_str.split('|').nth(1) {
                if let Some(data) = self.db.get_cf(cf_contracts, contract_id.as_bytes())? {
                    let contract: Contract = serde_json::from_slice(&data)?;
                    contracts.push(contract);
                }
            }
        }

        Ok(contracts)
    }

    /// List all contracts (with pagination for large datasets)
    pub fn list_all_contracts(&self, offset: usize, limit: usize) -> Result<Vec<Contract>> {
        let cf = self.cf(CF_CONTRACTS)?;
        let mut contracts = Vec::new();
        let mut count = 0;

        for item in self.db.iterator_cf(cf, IteratorMode::Start) {
            let (_key, value) = item?;

            // Skip offset
            if count < offset {
                count += 1;
                continue;
            }

            let contract: Contract = serde_json::from_slice(&value)?;
            contracts.push(contract);

            if contracts.len() >= limit {
                break;
            }
        }

        Ok(contracts)
    }

    /// Delete contract (with index cleanup)
    pub fn delete_contract(
        &self,
        contract_id: &str,
        provider_interface_id: &str,
        consumer_interface_id: &str,
    ) -> Result<()> {
        let cf_contracts = self.cf(CF_CONTRACTS)?;
        let cf_revisions = self.cf(CF_CONTRACT_REVISIONS)?;
        let cf_prov_idx = self.cf(CF_CONTRACT_BY_PROVIDER)?;
        let cf_cons_idx = self.cf(CF_CONTRACT_BY_CONSUMER)?;
        let cf_pair_idx = self.cf(CF_CONTRACT_BY_INTERFACE_PAIR)?;
        let cf_requests = self.cf(CF_CONTRACT_APPROVAL_REQUESTS)?;
        let cf_requests_by_contract = self.cf(CF_CONTRACT_APPROVAL_REQUESTS_BY_CONTRACT)?;
        let cf_evidence = self.cf(CF_CONTRACT_APPROVAL_EVIDENCE)?;
        let cf_evidence_by_request = self.cf(CF_CONTRACT_APPROVAL_EVIDENCE_BY_REQUEST)?;
        let cf_signatures = self.cf(CF_CONTRACT_SIGNATURES)?;
        let cf_signatures_by_contract = self.cf(CF_CONTRACT_SIGNATURES_BY_CONTRACT)?;

        let mut batch = WriteBatch::default();

        // Delete from primary storage
        batch.delete_cf(cf_contracts, contract_id.as_bytes());

        // Delete from provider index
        let prov_key = format!("{}|{}", provider_interface_id, contract_id);
        batch.delete_cf(cf_prov_idx, prov_key.as_bytes());

        // Delete from consumer index
        let cons_key = format!("{}|{}", consumer_interface_id, contract_id);
        batch.delete_cf(cf_cons_idx, cons_key.as_bytes());

        // Delete from direct interface-pair index
        let pair_key =
            Self::contract_pair_key(provider_interface_id, consumer_interface_id, contract_id);
        batch.delete_cf(cf_pair_idx, pair_key.as_bytes());

        for revision in self.list_contract_revisions(contract_id, usize::MAX)? {
            let revision_key = Self::contract_revision_key(contract_id, revision.revision);
            batch.delete_cf(cf_revisions, revision_key.as_bytes());
        }

        for request in self.list_contract_approval_requests(contract_id, usize::MAX)? {
            let request_key = Self::contract_approval_request_contract_key(
                &request.contract_id,
                &request.requested_at,
                &request.request_id,
            );
            batch.delete_cf(cf_requests, request.request_id.as_bytes());
            batch.delete_cf(cf_requests_by_contract, request_key.as_bytes());

            for evidence in self.list_contract_approval_evidence(&request.request_id)? {
                let evidence_key = Self::contract_approval_evidence_request_key(
                    &evidence.request_id,
                    &evidence.added_at,
                    &evidence.evidence_id,
                );
                batch.delete_cf(cf_evidence, evidence.evidence_id.as_bytes());
                batch.delete_cf(cf_evidence_by_request, evidence_key.as_bytes());
            }
        }

        for signature in self.list_contract_signatures(contract_id, usize::MAX)? {
            let signature_key = Self::contract_signature_contract_key(
                &signature.contract_id,
                signature.contract_revision,
            );
            batch.delete_cf(cf_signatures, signature.signature_id.as_bytes());
            batch.delete_cf(cf_signatures_by_contract, signature_key.as_bytes());
        }

        self.db.write(batch).context("Failed to delete contract")
    }

    pub fn put_contract_approval_request(
        &self,
        request: &ContractApprovalRequestRecord,
    ) -> Result<()> {
        let cf_requests = self.cf(CF_CONTRACT_APPROVAL_REQUESTS)?;
        let cf_contract_idx = self.cf(CF_CONTRACT_APPROVAL_REQUESTS_BY_CONTRACT)?;
        let data =
            serde_json::to_vec(request).context("Failed to serialize contract approval request")?;

        let mut batch = WriteBatch::default();
        if let Some(existing) = self
            .db
            .get_cf(cf_requests, request.request_id.as_bytes())?
            .map(|data| serde_json::from_slice::<ContractApprovalRequestRecord>(&data))
            .transpose()
            .context("Failed to deserialize existing contract approval request")?
        {
            let existing_key = Self::contract_approval_request_contract_key(
                &existing.contract_id,
                &existing.requested_at,
                &existing.request_id,
            );
            batch.delete_cf(cf_contract_idx, existing_key.as_bytes());
        }

        let request_key = Self::contract_approval_request_contract_key(
            &request.contract_id,
            &request.requested_at,
            &request.request_id,
        );
        batch.put_cf(cf_requests, request.request_id.as_bytes(), &data);
        batch.put_cf(
            cf_contract_idx,
            request_key.as_bytes(),
            request.request_id.as_bytes(),
        );

        self.db
            .write(batch)
            .context("Failed to write contract approval request")
    }

    pub fn get_contract_approval_request(
        &self,
        request_id: &str,
    ) -> Result<Option<ContractApprovalRequestRecord>> {
        let cf = self.cf(CF_CONTRACT_APPROVAL_REQUESTS)?;

        match self.db.get_cf(cf, request_id.as_bytes())? {
            Some(data) => {
                let request = serde_json::from_slice(&data)
                    .context("Failed to deserialize contract approval request")?;
                Ok(Some(request))
            }
            None => Ok(None),
        }
    }

    pub fn list_contract_approval_requests(
        &self,
        contract_id: &str,
        limit: usize,
    ) -> Result<Vec<ContractApprovalRequestRecord>> {
        let cf_contract_idx = self.cf(CF_CONTRACT_APPROVAL_REQUESTS_BY_CONTRACT)?;
        let cf_requests = self.cf(CF_CONTRACT_APPROVAL_REQUESTS)?;
        let prefix = format!("{}|", contract_id);
        let mut requests = Vec::new();

        for item in self.db.iterator_cf(
            cf_contract_idx,
            IteratorMode::From(prefix.as_bytes(), rocksdb::Direction::Forward),
        ) {
            let (key, value) = item?;
            let key_str = std::str::from_utf8(&key)?;

            if !key_str.starts_with(&prefix) {
                break;
            }

            let request_id = std::str::from_utf8(&value)?;
            if let Some(data) = self.db.get_cf(cf_requests, request_id.as_bytes())? {
                let request: ContractApprovalRequestRecord = serde_json::from_slice(&data)
                    .context("Failed to deserialize contract approval request")?;
                requests.push(request);
            }

            if requests.len() >= limit {
                break;
            }
        }

        Ok(requests)
    }

    pub fn list_all_contract_approval_requests(
        &self,
    ) -> Result<Vec<ContractApprovalRequestRecord>> {
        let cf = self.cf(CF_CONTRACT_APPROVAL_REQUESTS)?;
        let mut requests = Vec::new();

        for item in self.db.iterator_cf(cf, IteratorMode::Start) {
            let (_key, value) = item?;
            let request: ContractApprovalRequestRecord = serde_json::from_slice(&value)
                .context("Failed to deserialize contract approval request")?;
            requests.push(request);
        }

        requests.sort_by(|left, right| right.requested_at.cmp(&left.requested_at));
        Ok(requests)
    }

    pub fn put_contract_approval_evidence(
        &self,
        evidence: &ContractApprovalEvidenceRecord,
    ) -> Result<()> {
        let cf_evidence = self.cf(CF_CONTRACT_APPROVAL_EVIDENCE)?;
        let cf_request_idx = self.cf(CF_CONTRACT_APPROVAL_EVIDENCE_BY_REQUEST)?;
        let data = serde_json::to_vec(evidence)
            .context("Failed to serialize contract approval evidence")?;

        let mut batch = WriteBatch::default();
        if let Some(existing) = self
            .db
            .get_cf(cf_evidence, evidence.evidence_id.as_bytes())?
            .map(|data| serde_json::from_slice::<ContractApprovalEvidenceRecord>(&data))
            .transpose()
            .context("Failed to deserialize existing contract approval evidence")?
        {
            let existing_key = Self::contract_approval_evidence_request_key(
                &existing.request_id,
                &existing.added_at,
                &existing.evidence_id,
            );
            batch.delete_cf(cf_request_idx, existing_key.as_bytes());
        }

        let evidence_key = Self::contract_approval_evidence_request_key(
            &evidence.request_id,
            &evidence.added_at,
            &evidence.evidence_id,
        );
        batch.put_cf(cf_evidence, evidence.evidence_id.as_bytes(), &data);
        batch.put_cf(
            cf_request_idx,
            evidence_key.as_bytes(),
            evidence.evidence_id.as_bytes(),
        );

        self.db
            .write(batch)
            .context("Failed to write contract approval evidence")
    }

    pub fn get_contract_approval_evidence(
        &self,
        evidence_id: &str,
    ) -> Result<Option<ContractApprovalEvidenceRecord>> {
        let cf = self.cf(CF_CONTRACT_APPROVAL_EVIDENCE)?;

        match self.db.get_cf(cf, evidence_id.as_bytes())? {
            Some(data) => {
                let evidence = serde_json::from_slice(&data)
                    .context("Failed to deserialize contract approval evidence")?;
                Ok(Some(evidence))
            }
            None => Ok(None),
        }
    }

    pub fn list_contract_approval_evidence(
        &self,
        request_id: &str,
    ) -> Result<Vec<ContractApprovalEvidenceRecord>> {
        let cf_request_idx = self.cf(CF_CONTRACT_APPROVAL_EVIDENCE_BY_REQUEST)?;
        let cf_evidence = self.cf(CF_CONTRACT_APPROVAL_EVIDENCE)?;
        let prefix = format!("{}|", request_id);
        let mut evidence = Vec::new();

        for item in self.db.iterator_cf(
            cf_request_idx,
            IteratorMode::From(prefix.as_bytes(), rocksdb::Direction::Forward),
        ) {
            let (key, value) = item?;
            let key_str = std::str::from_utf8(&key)?;

            if !key_str.starts_with(&prefix) {
                break;
            }

            let evidence_id = std::str::from_utf8(&value)?;
            if let Some(data) = self.db.get_cf(cf_evidence, evidence_id.as_bytes())? {
                let record: ContractApprovalEvidenceRecord = serde_json::from_slice(&data)
                    .context("Failed to deserialize contract approval evidence")?;
                evidence.push(record);
            }
        }

        Ok(evidence)
    }

    pub fn list_all_contract_approval_evidence(
        &self,
    ) -> Result<Vec<ContractApprovalEvidenceRecord>> {
        let cf = self.cf(CF_CONTRACT_APPROVAL_EVIDENCE)?;
        let mut evidence = Vec::new();

        for item in self.db.iterator_cf(cf, IteratorMode::Start) {
            let (_key, value) = item?;
            let record: ContractApprovalEvidenceRecord = serde_json::from_slice(&value)
                .context("Failed to deserialize contract approval evidence")?;
            evidence.push(record);
        }

        evidence.sort_by(|left, right| right.added_at.cmp(&left.added_at));
        Ok(evidence)
    }

    /// Persist immutable cryptographic signature material for one contract revision.
    pub fn put_contract_signature(&self, signature: &ContractSignatureRecord) -> Result<()> {
        let cf_signatures = self.cf(CF_CONTRACT_SIGNATURES)?;
        let cf_contract_idx = self.cf(CF_CONTRACT_SIGNATURES_BY_CONTRACT)?;
        let data =
            serde_json::to_vec(signature).context("Failed to serialize contract signature")?;
        let key = Self::contract_signature_contract_key(
            &signature.contract_id,
            signature.contract_revision,
        );

        if let Some(existing_id) = self.db.get_cf(cf_contract_idx, key.as_bytes())? {
            let existing_id = String::from_utf8(existing_id.to_vec())
                .context("Failed to decode existing contract signature index")?;
            if existing_id != signature.signature_id {
                return Err(anyhow::anyhow!(
                    "Contract '{}' revision {} already has signature '{}'",
                    signature.contract_id,
                    signature.contract_revision,
                    existing_id
                ));
            }
        }

        let mut batch = WriteBatch::default();
        batch.put_cf(cf_signatures, signature.signature_id.as_bytes(), &data);
        batch.put_cf(
            cf_contract_idx,
            key.as_bytes(),
            signature.signature_id.as_bytes(),
        );
        self.db
            .write(batch)
            .context("Failed to persist contract signature")
    }

    /// Fetch the contract signature for one logical contract revision.
    pub fn get_contract_signature(
        &self,
        contract_id: &str,
        revision: u32,
    ) -> Result<Option<ContractSignatureRecord>> {
        let cf_contract_idx = self.cf(CF_CONTRACT_SIGNATURES_BY_CONTRACT)?;
        let cf_signatures = self.cf(CF_CONTRACT_SIGNATURES)?;
        let key = Self::contract_signature_contract_key(contract_id, revision);

        let Some(signature_id) = self.db.get_cf(cf_contract_idx, key.as_bytes())? else {
            return Ok(None);
        };
        let signature_id = String::from_utf8(signature_id.to_vec())
            .context("Failed to decode contract signature id")?;

        match self.db.get_cf(cf_signatures, signature_id.as_bytes())? {
            Some(data) => Ok(Some(
                serde_json::from_slice(&data)
                    .context("Failed to deserialize contract signature")?,
            )),
            None => Ok(None),
        }
    }

    /// List every contract signature for one logical contract newest-first.
    pub fn list_contract_signatures(
        &self,
        contract_id: &str,
        limit: usize,
    ) -> Result<Vec<ContractSignatureRecord>> {
        let cf_contract_idx = self.cf(CF_CONTRACT_SIGNATURES_BY_CONTRACT)?;
        let cf_signatures = self.cf(CF_CONTRACT_SIGNATURES)?;
        let prefix = format!("{}|", contract_id);
        let mut signatures = Vec::new();

        for item in self.db.iterator_cf(
            cf_contract_idx,
            IteratorMode::From(prefix.as_bytes(), rocksdb::Direction::Forward),
        ) {
            let (key, value) = item?;
            let key_str = std::str::from_utf8(&key)?;
            if !key_str.starts_with(&prefix) {
                break;
            }

            let signature_id =
                std::str::from_utf8(&value).context("Failed to decode contract signature id")?;
            if let Some(data) = self.db.get_cf(cf_signatures, signature_id.as_bytes())? {
                let signature: ContractSignatureRecord = serde_json::from_slice(&data)
                    .context("Failed to deserialize contract signature")?;
                signatures.push(signature);
            }

            if signatures.len() >= limit {
                break;
            }
        }

        signatures.sort_by(|left, right| right.contract_revision.cmp(&left.contract_revision));
        Ok(signatures)
    }

    /// List all contract signatures across the catalog.
    pub fn list_all_contract_signatures(&self) -> Result<Vec<ContractSignatureRecord>> {
        let cf = self.cf(CF_CONTRACT_SIGNATURES)?;
        let mut signatures = Vec::new();

        for item in self.db.iterator_cf(cf, IteratorMode::Start) {
            let (_key, value) = item?;
            let signature: ContractSignatureRecord = serde_json::from_slice(&value)
                .context("Failed to deserialize contract signature")?;
            signatures.push(signature);
        }

        signatures.sort_by(|left, right| {
            left.contract_id
                .cmp(&right.contract_id)
                .then_with(|| right.contract_revision.cmp(&left.contract_revision))
        });
        Ok(signatures)
    }

    // ========================================================================
    // Policy Operations
    // ========================================================================

    /// Store a persisted SoS policy and maintain secondary indexes.
    pub fn put_policy(&self, policy: &SosPolicy) -> Result<()> {
        let cf_policies = self.cf(CF_POLICIES)?;
        let cf_revisions = self.cf(CF_POLICY_REVISIONS)?;
        let cf_stage_idx = self.cf(CF_POLICY_BY_STAGE)?;
        let cf_target_idx = self.cf(CF_POLICY_BY_TARGET_TYPE)?;
        let cf_active_idx = self.cf(CF_POLICY_BY_ACTIVE)?;

        let existing = self
            .db
            .get_cf(cf_policies, policy.policy_id.as_bytes())?
            .map(|data| serde_json::from_slice::<SosPolicy>(&data))
            .transpose()
            .context("Failed to deserialize existing SoS policy")?;

        let data = serde_json::to_vec(policy).context("Failed to serialize SoS policy")?;
        let mut batch = WriteBatch::default();

        if let Some(existing_policy) = existing {
            if policy.revision < existing_policy.revision {
                return Err(anyhow::anyhow!(
                    "Policy '{}' revision must increase (existing {}, attempted {})",
                    policy.policy_id,
                    existing_policy.revision,
                    policy.revision
                ));
            }

            if policy.revision == existing_policy.revision
                && policy_semantics_require_new_revision(&existing_policy, policy)
            {
                return Err(anyhow::anyhow!(
                    "Policy '{}' semantic changes require a new revision (existing {}, attempted {})",
                    policy.policy_id,
                    existing_policy.revision,
                    policy.revision
                ));
            }

            for stage in &existing_policy.stages {
                let stage_key = format!("{}|{}", stage, existing_policy.policy_id);
                batch.delete_cf(cf_stage_idx, stage_key.as_bytes());
            }

            let target_key = format!(
                "{}|{}",
                existing_policy.target_type, existing_policy.policy_id
            );
            batch.delete_cf(cf_target_idx, target_key.as_bytes());

            let active_key = format!("{}|{}", existing_policy.active, existing_policy.policy_id);
            batch.delete_cf(cf_active_idx, active_key.as_bytes());

            if policy.revision > existing_policy.revision {
                let existing_revision_key =
                    Self::policy_revision_key(&existing_policy.policy_id, existing_policy.revision);
                let mut existing_revision = existing_policy.clone();
                existing_revision.superseded_by_revision = Some(policy.revision);
                let existing_revision_data = serde_json::to_vec(&existing_revision)
                    .context("Failed to serialize superseded SoS policy revision")?;
                batch.put_cf(
                    cf_revisions,
                    existing_revision_key.as_bytes(),
                    &existing_revision_data,
                );
            }
        }

        batch.put_cf(cf_policies, policy.policy_id.as_bytes(), &data);
        let revision_key = Self::policy_revision_key(&policy.policy_id, policy.revision);
        batch.put_cf(cf_revisions, revision_key.as_bytes(), &data);

        for stage in &policy.stages {
            let stage_key = format!("{}|{}", stage, policy.policy_id);
            batch.put_cf(cf_stage_idx, stage_key.as_bytes(), b"");
        }

        let target_key = format!("{}|{}", policy.target_type, policy.policy_id);
        batch.put_cf(cf_target_idx, target_key.as_bytes(), b"");

        let active_key = format!("{}|{}", policy.active, policy.policy_id);
        batch.put_cf(cf_active_idx, active_key.as_bytes(), b"");

        self.db.write(batch).context("Failed to persist SoS policy")
    }

    /// Fetch a SoS policy by ID.
    pub fn get_policy(&self, policy_id: &str) -> Result<Option<SosPolicy>> {
        let cf = self.cf(CF_POLICIES)?;

        match self.db.get_cf(cf, policy_id.as_bytes())? {
            Some(data) => {
                let policy =
                    serde_json::from_slice(&data).context("Failed to deserialize SoS policy")?;
                Ok(Some(policy))
            }
            None => Ok(None),
        }
    }

    /// Fetch a specific immutable policy revision by logical ID and revision number.
    pub fn get_policy_revision(&self, policy_id: &str, revision: u32) -> Result<Option<SosPolicy>> {
        let cf = self.cf(CF_POLICY_REVISIONS)?;
        let key = Self::policy_revision_key(policy_id, revision);

        match self.db.get_cf(cf, key.as_bytes())? {
            Some(data) => {
                let policy = serde_json::from_slice(&data)
                    .context("Failed to deserialize SoS policy revision")?;
                Ok(Some(policy))
            }
            None => Ok(self
                .get_policy(policy_id)?
                .filter(|policy| policy.revision == revision)),
        }
    }

    /// List all persisted SoS policies with pagination.
    pub fn list_all_policies(&self, offset: usize, limit: usize) -> Result<Vec<SosPolicy>> {
        let cf = self.cf(CF_POLICIES)?;
        let mut policies = Vec::new();
        let mut count = 0usize;

        for item in self.db.iterator_cf(cf, IteratorMode::Start) {
            let (_key, value) = item?;

            if count < offset {
                count += 1;
                continue;
            }

            let policy: SosPolicy =
                serde_json::from_slice(&value).context("Failed to deserialize SoS policy")?;
            policies.push(policy);

            if policies.len() >= limit {
                break;
            }
        }

        Ok(policies)
    }

    /// List immutable policy revision snapshots for a logical policy ID.
    pub fn list_policy_revisions(&self, policy_id: &str, limit: usize) -> Result<Vec<SosPolicy>> {
        let cf = self.cf(CF_POLICY_REVISIONS)?;
        let prefix = format!("{}|", policy_id);
        let mut policies = Vec::new();

        for item in self.db.iterator_cf(cf, IteratorMode::Start) {
            let (key, value) = item?;
            let key_str = std::str::from_utf8(&key)?;

            if !key_str.starts_with(&prefix) {
                continue;
            }

            let policy: SosPolicy = serde_json::from_slice(&value)
                .context("Failed to deserialize SoS policy revision")?;
            policies.push(policy);
        }

        if policies.is_empty() {
            if let Some(policy) = self.get_policy(policy_id)? {
                policies.push(policy);
            }
        }

        policies.sort_by(|left, right| right.revision.cmp(&left.revision));
        policies.truncate(limit);
        Ok(policies)
    }

    /// List all immutable policy revision snapshots across all logical policies.
    pub fn list_all_policy_revisions(&self) -> Result<Vec<SosPolicy>> {
        let cf = self.cf(CF_POLICY_REVISIONS)?;
        let mut policies = Vec::new();

        for item in self.db.iterator_cf(cf, IteratorMode::Start) {
            let (_key, value) = item?;
            let policy: SosPolicy = serde_json::from_slice(&value)
                .context("Failed to deserialize SoS policy revision")?;
            policies.push(policy);
        }

        for policy in self.list_all_policies(0, usize::MAX)? {
            if !policies.iter().any(|revision| {
                revision.policy_id == policy.policy_id && revision.revision == policy.revision
            }) {
                policies.push(policy);
            }
        }

        policies.sort_by(|left, right| {
            left.policy_id
                .cmp(&right.policy_id)
                .then(right.revision.cmp(&left.revision))
        });
        Ok(policies)
    }

    /// List persisted policies for a validation stage.
    pub fn list_policies_by_stage(&self, stage: &str, limit: usize) -> Result<Vec<SosPolicy>> {
        let cf_stage_idx = self.cf(CF_POLICY_BY_STAGE)?;
        let cf_policies = self.cf(CF_POLICIES)?;
        let prefix = format!("{}|", stage);
        let mut policies = Vec::new();

        for item in self.db.iterator_cf(
            cf_stage_idx,
            IteratorMode::From(prefix.as_bytes(), rocksdb::Direction::Forward),
        ) {
            let (key, _) = item?;
            let key_str = std::str::from_utf8(&key)?;

            if !key_str.starts_with(&prefix) {
                break;
            }

            if let Some(policy_id) = key_str.split('|').nth(1) {
                if let Some(data) = self.db.get_cf(cf_policies, policy_id.as_bytes())? {
                    let policy: SosPolicy = serde_json::from_slice(&data)
                        .context("Failed to deserialize SoS policy from stage index")?;
                    policies.push(policy);
                }
            }

            if policies.len() >= limit {
                break;
            }
        }

        Ok(policies)
    }

    // ========================================================================
    // Policy Approval Request Operations
    // ========================================================================

    /// Persist a policy approval request and maintain its policy-scoped index.
    pub fn put_policy_approval_request(&self, request: &PolicyApprovalRequestRecord) -> Result<()> {
        let cf_requests = self.cf(CF_POLICY_APPROVAL_REQUESTS)?;
        let cf_policy_idx = self.cf(CF_POLICY_APPROVAL_REQUESTS_BY_POLICY)?;
        let data =
            serde_json::to_vec(request).context("Failed to serialize policy approval request")?;

        let existing = self
            .db
            .get_cf(cf_requests, request.request_id.as_bytes())?
            .map(|data| serde_json::from_slice::<PolicyApprovalRequestRecord>(&data))
            .transpose()
            .context("Failed to deserialize existing policy approval request")?;

        let mut batch = WriteBatch::default();

        if let Some(existing_request) = existing {
            let existing_key = Self::policy_approval_request_policy_key(
                &existing_request.policy_id,
                &existing_request.requested_at,
                &existing_request.request_id,
            );
            batch.delete_cf(cf_policy_idx, existing_key.as_bytes());
        }

        batch.put_cf(cf_requests, request.request_id.as_bytes(), &data);
        let request_key = Self::policy_approval_request_policy_key(
            &request.policy_id,
            &request.requested_at,
            &request.request_id,
        );
        batch.put_cf(
            cf_policy_idx,
            request_key.as_bytes(),
            request.request_id.as_bytes(),
        );

        self.db
            .write(batch)
            .context("Failed to persist policy approval request")
    }

    /// Fetch one policy approval request by ID.
    pub fn get_policy_approval_request(
        &self,
        request_id: &str,
    ) -> Result<Option<PolicyApprovalRequestRecord>> {
        let cf = self.cf(CF_POLICY_APPROVAL_REQUESTS)?;

        match self.db.get_cf(cf, request_id.as_bytes())? {
            Some(data) => Ok(Some(
                serde_json::from_slice(&data)
                    .context("Failed to deserialize policy approval request")?,
            )),
            None => Ok(None),
        }
    }

    /// List policy approval requests for a logical policy newest-first.
    pub fn list_policy_approval_requests(
        &self,
        policy_id: &str,
        limit: usize,
    ) -> Result<Vec<PolicyApprovalRequestRecord>> {
        let cf_policy_idx = self.cf(CF_POLICY_APPROVAL_REQUESTS_BY_POLICY)?;
        let cf_requests = self.cf(CF_POLICY_APPROVAL_REQUESTS)?;
        let prefix = format!("{policy_id}|");
        let mut requests = Vec::new();

        for item in self.db.iterator_cf(
            cf_policy_idx,
            IteratorMode::From(prefix.as_bytes(), rocksdb::Direction::Forward),
        ) {
            let (key, request_id) = item?;
            let key_str = std::str::from_utf8(&key)?;
            if !key_str.starts_with(&prefix) {
                break;
            }

            let request_id = std::str::from_utf8(&request_id)?;
            if let Some(data) = self.db.get_cf(cf_requests, request_id.as_bytes())? {
                let request: PolicyApprovalRequestRecord = serde_json::from_slice(&data)
                    .context("Failed to deserialize policy approval request from index")?;
                requests.push(request);
            }

            if requests.len() >= limit {
                break;
            }
        }

        Ok(requests)
    }

    /// List all policy approval requests across every policy.
    pub fn list_all_policy_approval_requests(&self) -> Result<Vec<PolicyApprovalRequestRecord>> {
        let cf = self.cf(CF_POLICY_APPROVAL_REQUESTS)?;
        let mut requests = Vec::new();

        for item in self.db.iterator_cf(cf, IteratorMode::Start) {
            let (_key, value) = item?;
            let request: PolicyApprovalRequestRecord = serde_json::from_slice(&value)
                .context("Failed to deserialize policy approval request")?;
            requests.push(request);
        }

        requests.sort_by(|left, right| right.requested_at.cmp(&left.requested_at));
        Ok(requests)
    }

    /// Persist approval evidence and maintain its request-scoped index.
    pub fn put_policy_approval_evidence(
        &self,
        evidence: &PolicyApprovalEvidenceRecord,
    ) -> Result<()> {
        let cf_evidence = self.cf(CF_POLICY_APPROVAL_EVIDENCE)?;
        let cf_request_idx = self.cf(CF_POLICY_APPROVAL_EVIDENCE_BY_REQUEST)?;
        let data =
            serde_json::to_vec(evidence).context("Failed to serialize policy approval evidence")?;

        let existing = self
            .db
            .get_cf(cf_evidence, evidence.evidence_id.as_bytes())?
            .map(|data| serde_json::from_slice::<PolicyApprovalEvidenceRecord>(&data))
            .transpose()
            .context("Failed to deserialize existing policy approval evidence")?;

        let mut batch = WriteBatch::default();

        if let Some(existing_evidence) = existing {
            let existing_key = Self::policy_approval_evidence_request_key(
                &existing_evidence.request_id,
                &existing_evidence.added_at,
                &existing_evidence.evidence_id,
            );
            batch.delete_cf(cf_request_idx, existing_key.as_bytes());
        }

        batch.put_cf(cf_evidence, evidence.evidence_id.as_bytes(), &data);
        let evidence_key = Self::policy_approval_evidence_request_key(
            &evidence.request_id,
            &evidence.added_at,
            &evidence.evidence_id,
        );
        batch.put_cf(
            cf_request_idx,
            evidence_key.as_bytes(),
            evidence.evidence_id.as_bytes(),
        );

        self.db
            .write(batch)
            .context("Failed to persist policy approval evidence")
    }

    /// Fetch one policy approval evidence record by ID.
    pub fn get_policy_approval_evidence(
        &self,
        evidence_id: &str,
    ) -> Result<Option<PolicyApprovalEvidenceRecord>> {
        let cf = self.cf(CF_POLICY_APPROVAL_EVIDENCE)?;

        match self.db.get_cf(cf, evidence_id.as_bytes())? {
            Some(data) => Ok(Some(
                serde_json::from_slice(&data)
                    .context("Failed to deserialize policy approval evidence")?,
            )),
            None => Ok(None),
        }
    }

    /// List policy approval evidence for a request newest-first.
    pub fn list_policy_approval_evidence(
        &self,
        request_id: &str,
    ) -> Result<Vec<PolicyApprovalEvidenceRecord>> {
        let cf_request_idx = self.cf(CF_POLICY_APPROVAL_EVIDENCE_BY_REQUEST)?;
        let cf_evidence = self.cf(CF_POLICY_APPROVAL_EVIDENCE)?;
        let prefix = format!("{request_id}|");
        let mut evidence = Vec::new();

        for item in self.db.iterator_cf(
            cf_request_idx,
            IteratorMode::From(prefix.as_bytes(), rocksdb::Direction::Forward),
        ) {
            let (key, evidence_id) = item?;
            let key_str = std::str::from_utf8(&key)?;
            if !key_str.starts_with(&prefix) {
                break;
            }

            let evidence_id = std::str::from_utf8(&evidence_id)?;
            if let Some(data) = self.db.get_cf(cf_evidence, evidence_id.as_bytes())? {
                let record: PolicyApprovalEvidenceRecord = serde_json::from_slice(&data)
                    .context("Failed to deserialize policy approval evidence from index")?;
                evidence.push(record);
            }
        }

        Ok(evidence)
    }

    /// List all policy approval evidence across every request.
    pub fn list_all_policy_approval_evidence(&self) -> Result<Vec<PolicyApprovalEvidenceRecord>> {
        let cf = self.cf(CF_POLICY_APPROVAL_EVIDENCE)?;
        let mut evidence = Vec::new();

        for item in self.db.iterator_cf(cf, IteratorMode::Start) {
            let (_key, value) = item?;
            let record: PolicyApprovalEvidenceRecord = serde_json::from_slice(&value)
                .context("Failed to deserialize policy approval evidence")?;
            evidence.push(record);
        }

        evidence.sort_by(|left, right| right.added_at.cmp(&left.added_at));
        Ok(evidence)
    }

    /// Persist immutable approval attestation material for one policy revision.
    pub fn put_policy_attestation(&self, attestation: &PolicyAttestationRecord) -> Result<()> {
        let cf_attestations = self.cf(CF_POLICY_ATTESTATIONS)?;
        let cf_policy_idx = self.cf(CF_POLICY_ATTESTATIONS_BY_POLICY)?;
        let data =
            serde_json::to_vec(attestation).context("Failed to serialize policy attestation")?;
        let key = Self::policy_attestation_policy_key(
            &attestation.policy_id,
            attestation.policy_revision,
        );

        if let Some(existing_id) = self.db.get_cf(cf_policy_idx, key.as_bytes())? {
            let existing_id = String::from_utf8(existing_id.to_vec())
                .context("Failed to decode existing policy attestation index")?;
            if existing_id != attestation.attestation_id {
                anyhow::bail!(
                    "Policy '{}' revision {} already has attestation '{}'",
                    attestation.policy_id,
                    attestation.policy_revision,
                    existing_id
                );
            }
        }

        let mut batch = WriteBatch::default();
        batch.put_cf(
            cf_attestations,
            attestation.attestation_id.as_bytes(),
            &data,
        );
        batch.put_cf(
            cf_policy_idx,
            key.as_bytes(),
            attestation.attestation_id.as_bytes(),
        );

        self.db
            .write(batch)
            .context("Failed to persist policy attestation")
    }

    /// Fetch the policy attestation for one logical policy revision.
    pub fn get_policy_attestation(
        &self,
        policy_id: &str,
        revision: u32,
    ) -> Result<Option<PolicyAttestationRecord>> {
        let cf_attestations = self.cf(CF_POLICY_ATTESTATIONS)?;
        let cf_policy_idx = self.cf(CF_POLICY_ATTESTATIONS_BY_POLICY)?;
        let key = Self::policy_attestation_policy_key(policy_id, revision);

        let Some(attestation_id) = self.db.get_cf(cf_policy_idx, key.as_bytes())? else {
            return Ok(None);
        };
        let attestation_id = String::from_utf8(attestation_id.to_vec())
            .context("Failed to decode policy attestation id")?;

        match self.db.get_cf(cf_attestations, attestation_id.as_bytes())? {
            Some(data) => Ok(Some(
                serde_json::from_slice(&data)
                    .context("Failed to deserialize policy attestation")?,
            )),
            None => Ok(None),
        }
    }

    /// List every policy attestation for one logical policy newest-first.
    pub fn list_policy_attestations(
        &self,
        policy_id: &str,
        limit: usize,
    ) -> Result<Vec<PolicyAttestationRecord>> {
        let cf_policy_idx = self.cf(CF_POLICY_ATTESTATIONS_BY_POLICY)?;
        let cf_attestations = self.cf(CF_POLICY_ATTESTATIONS)?;
        let prefix = format!("{policy_id}|");
        let mut attestations = Vec::new();

        for item in self.db.iterator_cf(
            cf_policy_idx,
            IteratorMode::From(prefix.as_bytes(), rocksdb::Direction::Forward),
        ) {
            let (key, value) = item?;
            let key_str = std::str::from_utf8(&key)?;
            if !key_str.starts_with(&prefix) {
                break;
            }

            let attestation_id =
                std::str::from_utf8(&value).context("Failed to decode policy attestation id")?;
            if let Some(data) = self.db.get_cf(cf_attestations, attestation_id.as_bytes())? {
                let attestation: PolicyAttestationRecord = serde_json::from_slice(&data)
                    .context("Failed to deserialize policy attestation")?;
                attestations.push(attestation);
            }

            if attestations.len() >= limit {
                break;
            }
        }

        attestations.sort_by(|left, right| right.policy_revision.cmp(&left.policy_revision));
        Ok(attestations)
    }

    /// List all policy attestations across the catalog.
    pub fn list_all_policy_attestations(&self) -> Result<Vec<PolicyAttestationRecord>> {
        let cf = self.cf(CF_POLICY_ATTESTATIONS)?;
        let mut attestations = Vec::new();

        for item in self.db.iterator_cf(cf, IteratorMode::Start) {
            let (_key, value) = item?;
            let attestation: PolicyAttestationRecord = serde_json::from_slice(&value)
                .context("Failed to deserialize policy attestation")?;
            attestations.push(attestation);
        }

        attestations.sort_by(|left, right| {
            left.policy_id
                .cmp(&right.policy_id)
                .then(right.policy_revision.cmp(&left.policy_revision))
        });
        Ok(attestations)
    }

    /// Delete a persisted SoS policy and all related indexes.
    pub fn delete_policy(&self, policy_id: &str) -> Result<()> {
        let Some(policy) = self.get_policy(policy_id)? else {
            return Ok(());
        };

        let cf_policies = self.cf(CF_POLICIES)?;
        let cf_revisions = self.cf(CF_POLICY_REVISIONS)?;
        let cf_stage_idx = self.cf(CF_POLICY_BY_STAGE)?;
        let cf_target_idx = self.cf(CF_POLICY_BY_TARGET_TYPE)?;
        let cf_active_idx = self.cf(CF_POLICY_BY_ACTIVE)?;
        let cf_requests = self.cf(CF_POLICY_APPROVAL_REQUESTS)?;
        let cf_requests_by_policy = self.cf(CF_POLICY_APPROVAL_REQUESTS_BY_POLICY)?;
        let cf_evidence = self.cf(CF_POLICY_APPROVAL_EVIDENCE)?;
        let cf_evidence_by_request = self.cf(CF_POLICY_APPROVAL_EVIDENCE_BY_REQUEST)?;
        let cf_attestations = self.cf(CF_POLICY_ATTESTATIONS)?;
        let cf_attestations_by_policy = self.cf(CF_POLICY_ATTESTATIONS_BY_POLICY)?;

        let mut batch = WriteBatch::default();
        batch.delete_cf(cf_policies, policy.policy_id.as_bytes());

        for stage in &policy.stages {
            let stage_key = format!("{}|{}", stage, policy.policy_id);
            batch.delete_cf(cf_stage_idx, stage_key.as_bytes());
        }

        let target_key = format!("{}|{}", policy.target_type, policy.policy_id);
        batch.delete_cf(cf_target_idx, target_key.as_bytes());

        let active_key = format!("{}|{}", policy.active, policy.policy_id);
        batch.delete_cf(cf_active_idx, active_key.as_bytes());

        for revision in self.list_policy_revisions(policy_id, usize::MAX)? {
            let revision_key = Self::policy_revision_key(policy_id, revision.revision);
            batch.delete_cf(cf_revisions, revision_key.as_bytes());
        }
        for attestation in self.list_policy_attestations(policy_id, usize::MAX)? {
            let attestation_key = Self::policy_attestation_policy_key(
                &attestation.policy_id,
                attestation.policy_revision,
            );
            batch.delete_cf(cf_attestations, attestation.attestation_id.as_bytes());
            batch.delete_cf(cf_attestations_by_policy, attestation_key.as_bytes());
        }

        for request in self.list_policy_approval_requests(policy_id, usize::MAX)? {
            let request_key = Self::policy_approval_request_policy_key(
                &request.policy_id,
                &request.requested_at,
                &request.request_id,
            );
            batch.delete_cf(cf_requests, request.request_id.as_bytes());
            batch.delete_cf(cf_requests_by_policy, request_key.as_bytes());

            for evidence in self.list_policy_approval_evidence(&request.request_id)? {
                let evidence_key = Self::policy_approval_evidence_request_key(
                    &evidence.request_id,
                    &evidence.added_at,
                    &evidence.evidence_id,
                );
                batch.delete_cf(cf_evidence, evidence.evidence_id.as_bytes());
                batch.delete_cf(cf_evidence_by_request, evidence_key.as_bytes());
            }
        }

        self.db.write(batch).context("Failed to delete SoS policy")
    }

    /// List all systems (with pagination for large datasets)
    pub fn list_all_systems(&self, offset: usize, limit: usize) -> Result<Vec<System>> {
        let cf = self.cf(CF_SYSTEMS)?;
        let mut systems = Vec::new();
        let mut count = 0;

        for item in self.db.iterator_cf(cf, IteratorMode::Start) {
            let (_key, value) = item?;

            // Skip offset
            if count < offset {
                count += 1;
                continue;
            }

            let system: System = serde_json::from_slice(&value)?;
            systems.push(system);

            if systems.len() >= limit {
                break;
            }
        }

        Ok(systems)
    }

    // ========================================================================
    // Validation Report Operations
    // ========================================================================

    /// Persist a validation report and all secondary indexes atomically.
    pub fn put_validation_report(&self, report: &ValidationReport) -> Result<()> {
        let cf_reports = self.cf(CF_VALIDATION_REPORTS)?;
        let cf_latest = self.cf(CF_VALIDATION_LATEST_BY_SUBJECT)?;
        let cf_history = self.cf(CF_VALIDATION_HISTORY_BY_SUBJECT)?;
        let cf_workflow_execution = self.cf(CF_VALIDATION_BY_WORKFLOW_EXECUTION)?;
        let cf_type = self.cf(CF_VALIDATION_BY_TYPE)?;

        let data = serde_json::to_vec(report).context("Failed to serialize validation report")?;
        let history_key =
            Self::subject_history_key(&report.subject_key, &report.validated_at, &report.report_id);
        let type_key = Self::validation_type_key(
            &report.validation_type,
            &report.validated_at,
            &report.report_id,
        );

        let mut batch = WriteBatch::default();
        batch.put_cf(cf_reports, report.report_id.as_bytes(), &data);
        batch.put_cf(
            cf_latest,
            report.subject_key.as_bytes(),
            report.report_id.as_bytes(),
        );
        batch.put_cf(
            cf_history,
            history_key.as_bytes(),
            report.report_id.as_bytes(),
        );
        batch.put_cf(cf_type, type_key.as_bytes(), report.report_id.as_bytes());

        if let Some(workflow_execution_id) = &report.workflow_execution_id {
            let workflow_key = Self::workflow_execution_key(
                workflow_execution_id,
                &report.validated_at,
                &report.report_id,
            );
            batch.put_cf(
                cf_workflow_execution,
                workflow_key.as_bytes(),
                report.report_id.as_bytes(),
            );
        }

        self.db
            .write(batch)
            .context("Failed to persist validation report")
    }

    /// Fetch a validation report by report ID.
    pub fn get_validation_report(&self, report_id: &str) -> Result<Option<ValidationReport>> {
        let cf = self.cf(CF_VALIDATION_REPORTS)?;

        match self.db.get_cf(cf, report_id.as_bytes())? {
            Some(data) => {
                let report = serde_json::from_slice(&data)
                    .context("Failed to deserialize validation report")?;
                Ok(Some(report))
            }
            None => Ok(None),
        }
    }

    /// Fetch the latest report for a normalized subject key.
    pub fn get_latest_validation_report(
        &self,
        subject_key: &str,
    ) -> Result<Option<ValidationReport>> {
        let cf_latest = self.cf(CF_VALIDATION_LATEST_BY_SUBJECT)?;

        match self.db.get_cf(cf_latest, subject_key.as_bytes())? {
            Some(report_id) => {
                let report_id = String::from_utf8(report_id.to_vec())
                    .context("Validation report index contained invalid UTF-8")?;
                self.get_validation_report(&report_id)
            }
            None => Ok(None),
        }
    }

    /// List validation history newest-first for a normalized subject key.
    pub fn list_validation_history(
        &self,
        subject_key: &str,
        limit: usize,
    ) -> Result<Vec<ValidationReport>> {
        let cf_history = self.cf(CF_VALIDATION_HISTORY_BY_SUBJECT)?;
        let prefix = format!("{}|", subject_key);
        let mut reports = Vec::new();

        for item in self.db.iterator_cf(
            cf_history,
            IteratorMode::From(prefix.as_bytes(), rocksdb::Direction::Forward),
        ) {
            let (key, value) = item?;
            let key_str = std::str::from_utf8(&key)?;

            if !key_str.starts_with(&prefix) {
                break;
            }

            let report_id = std::str::from_utf8(&value)?;
            if let Some(report) = self.get_validation_report(report_id)? {
                reports.push(report);
            }

            if reports.len() >= limit {
                break;
            }
        }

        Ok(reports)
    }

    /// List validation reports associated with a workflow execution.
    pub fn list_validation_reports_by_workflow_execution(
        &self,
        workflow_execution_id: &str,
    ) -> Result<Vec<ValidationReport>> {
        let cf_index = self.cf(CF_VALIDATION_BY_WORKFLOW_EXECUTION)?;
        let prefix = format!("{}|", workflow_execution_id);
        let mut reports = Vec::new();

        for item in self.db.iterator_cf(
            cf_index,
            IteratorMode::From(prefix.as_bytes(), rocksdb::Direction::Forward),
        ) {
            let (key, value) = item?;
            let key_str = std::str::from_utf8(&key)?;

            if !key_str.starts_with(&prefix) {
                break;
            }

            let report_id = std::str::from_utf8(&value)?;
            if let Some(report) = self.get_validation_report(report_id)? {
                reports.push(report);
            }
        }

        Ok(reports)
    }

    /// List validation reports by validation type newest-first.
    pub fn list_validation_reports_by_type(
        &self,
        validation_type: &str,
        limit: usize,
    ) -> Result<Vec<ValidationReport>> {
        let cf_index = self.cf(CF_VALIDATION_BY_TYPE)?;
        let prefix = format!("{}|", validation_type);
        let mut reports = Vec::new();

        for item in self.db.iterator_cf(
            cf_index,
            IteratorMode::From(prefix.as_bytes(), rocksdb::Direction::Forward),
        ) {
            let (key, value) = item?;
            let key_str = std::str::from_utf8(&key)?;

            if !key_str.starts_with(&prefix) {
                break;
            }

            let report_id = std::str::from_utf8(&value)?;
            if let Some(report) = self.get_validation_report(report_id)? {
                reports.push(report);
            }

            if reports.len() >= limit {
                break;
            }
        }

        Ok(reports)
    }

    /// List every persisted validation report.
    pub fn list_all_validation_reports(&self) -> Result<Vec<ValidationReport>> {
        let cf = self.cf(CF_VALIDATION_REPORTS)?;
        let mut reports = Vec::new();

        for item in self.db.iterator_cf(cf, IteratorMode::Start) {
            let (_key, value) = item?;
            let report: ValidationReport = serde_json::from_slice(&value)
                .context("Failed to deserialize validation report during full scan")?;
            reports.push(report);
        }

        reports.sort_by(|left, right| right.validated_at.cmp(&left.validated_at));
        Ok(reports)
    }

    /// Delete one validation report and remove all secondary index entries.
    pub fn delete_validation_report(&self, report_id: &str) -> Result<Option<ValidationReport>> {
        let Some(report) = self.get_validation_report(report_id)? else {
            return Ok(None);
        };

        let cf_latest = self.cf(CF_VALIDATION_LATEST_BY_SUBJECT)?;
        let current_latest_id = self
            .db
            .get_cf(cf_latest, report.subject_key.as_bytes())?
            .map(|bytes| String::from_utf8(bytes.to_vec()))
            .transpose()
            .context("Validation report latest index contained invalid UTF-8")?;

        let replacement_latest_id = if current_latest_id.as_deref() == Some(report_id) {
            self.list_validation_history(&report.subject_key, usize::MAX)?
                .into_iter()
                .find(|candidate| candidate.report_id != report_id)
                .map(|candidate| candidate.report_id)
        } else {
            None
        };

        let mut batch = WriteBatch::default();
        self.queue_validation_report_delete(&mut batch, &report)?;

        if current_latest_id.as_deref() == Some(report_id) {
            if let Some(replacement_latest_id) = replacement_latest_id {
                batch.put_cf(
                    cf_latest,
                    report.subject_key.as_bytes(),
                    replacement_latest_id.as_bytes(),
                );
            } else {
                batch.delete_cf(cf_latest, report.subject_key.as_bytes());
            }
        }

        self.db
            .write(batch)
            .context("Failed to delete validation report")?;

        Ok(Some(report))
    }

    /// Prune stale reports for one subject and return the removed reports.
    ///
    /// The newest report is always retained, even when it exceeds an age limit.
    pub fn prune_validation_reports_by_subject(
        &self,
        subject_key: &str,
        max_reports: usize,
        older_than: Option<DateTime<Utc>>,
    ) -> Result<Vec<ValidationReport>> {
        let keep_count = max_reports.max(1);
        let history = self.list_validation_history(subject_key, usize::MAX)?;

        if history.len() <= 1 {
            return Ok(Vec::new());
        }

        let pruned_reports: Vec<ValidationReport> = history
            .into_iter()
            .enumerate()
            .filter_map(|(idx, report)| {
                if idx == 0 {
                    return None;
                }

                let exceeds_count_limit = idx >= keep_count;
                let exceeds_age_limit = older_than
                    .map(|cutoff| report.validated_at < cutoff)
                    .unwrap_or(false);

                (exceeds_count_limit || exceeds_age_limit).then_some(report)
            })
            .collect();

        if pruned_reports.is_empty() {
            return Ok(Vec::new());
        }

        let mut batch = WriteBatch::default();
        for report in &pruned_reports {
            self.queue_validation_report_delete(&mut batch, report)?;
        }

        self.db
            .write(batch)
            .context("Failed to prune validation reports")?;

        Ok(pruned_reports)
    }

    fn queue_validation_report_delete(
        &self,
        batch: &mut WriteBatch,
        report: &ValidationReport,
    ) -> Result<()> {
        let cf_reports = self.cf(CF_VALIDATION_REPORTS)?;
        let cf_history = self.cf(CF_VALIDATION_HISTORY_BY_SUBJECT)?;
        let cf_workflow_execution = self.cf(CF_VALIDATION_BY_WORKFLOW_EXECUTION)?;
        let cf_type = self.cf(CF_VALIDATION_BY_TYPE)?;

        batch.delete_cf(cf_reports, report.report_id.as_bytes());

        let history_key =
            Self::subject_history_key(&report.subject_key, &report.validated_at, &report.report_id);
        batch.delete_cf(cf_history, history_key.as_bytes());

        let type_key = Self::validation_type_key(
            &report.validation_type,
            &report.validated_at,
            &report.report_id,
        );
        batch.delete_cf(cf_type, type_key.as_bytes());

        if let Some(workflow_execution_id) = &report.workflow_execution_id {
            let workflow_key = Self::workflow_execution_key(
                workflow_execution_id,
                &report.validated_at,
                &report.report_id,
            );
            batch.delete_cf(cf_workflow_execution, workflow_key.as_bytes());
        }

        Ok(())
    }
}

fn contract_semantics_require_new_revision(existing: &Contract, candidate: &Contract) -> bool {
    existing.contract_name != candidate.contract_name
        || existing.provider_interface_id != candidate.provider_interface_id
        || existing.consumer_interface_id != candidate.consumer_interface_id
        || existing.sla_metrics != candidate.sla_metrics
        || existing.transformation_rules != candidate.transformation_rules
        || existing.description != candidate.description
}

fn policy_semantics_require_new_revision(existing: &SosPolicy, candidate: &SosPolicy) -> bool {
    existing.target_type != candidate.target_type
        || existing.target_key != candidate.target_key
        || existing.stages != candidate.stages
        || existing.enforcement_level != candidate.enforcement_level
        || existing.severity != candidate.severity
        || existing.sparql_query != candidate.sparql_query
        || existing.context != candidate.context
        || existing.provider_interface_id != candidate.provider_interface_id
        || existing.consumer_interface_id != candidate.consumer_interface_id
        || existing.contract_id != candidate.contract_id
        || existing.source_system_id != candidate.source_system_id
        || existing.target_system_id != candidate.target_system_id
        || existing.interface_id != candidate.interface_id
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use serde_json::json;
    use tempfile::TempDir;

    #[test]
    fn test_system_crud() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let store = SosStore::new(temp_dir.path().to_str().unwrap())?;

        let system = System {
            system_id: "test-sys-1".to_string(),
            system_name: "Test System".to_string(),
            system_type: "radar".to_string(),
            vendor: "Test Vendor".to_string(),
            version: "1.0".to_string(),
            classification: "UNCLASSIFIED".to_string(),
            description: Some("Test description".to_string()),
            deployment: HashMap::new(),
            capabilities: HashMap::new(),
            tags: vec!["test".to_string()],
            active: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        // Test put
        store.put_system(&system)?;

        // Test get
        let retrieved = store.get_system("test-sys-1")?;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().system_name, "Test System");

        // Test list by type
        let systems = store.list_systems_by_type("radar", 10)?;
        assert_eq!(systems.len(), 1);

        Ok(())
    }

    fn sample_validation_report(
        report_id: &str,
        subject_key: &str,
        validation_type: &str,
        validated_at: DateTime<Utc>,
        previous_report_id: Option<&str>,
        workflow_execution_id: Option<&str>,
    ) -> ValidationReport {
        ValidationReport {
            report_id: report_id.to_string(),
            validation_id: format!("validation-{report_id}"),
            subject_type: "interface_pair".to_string(),
            subject_key: subject_key.to_string(),
            validation_type: validation_type.to_string(),
            passed: previous_report_id.is_some(),
            confidence: if previous_report_id.is_some() {
                1.0
            } else {
                0.5
            },
            checks: vec![ValidationCheckRecord {
                check_name: "schema_compatibility".to_string(),
                passed: previous_report_id.is_some(),
                severity: "error".to_string(),
                description: "Synthetic validation check".to_string(),
                details: None,
            }],
            validated_at,
            previous_report_id: previous_report_id.map(ToOwned::to_owned),
            change_summary: ValidationChangeSummary {
                resolved_checks: previous_report_id
                    .map(|_| vec!["schema_compatibility".to_string()])
                    .unwrap_or_default(),
                new_failures: previous_report_id
                    .is_none()
                    .then(|| vec!["schema_compatibility".to_string()])
                    .unwrap_or_default(),
                confidence_delta: if previous_report_id.is_some() {
                    0.5
                } else {
                    0.0
                },
                schema_or_policy_version_changed: previous_report_id.is_some(),
            },
            workflow_execution_id: workflow_execution_id.map(ToOwned::to_owned),
            workflow_step_id: workflow_execution_id.map(|_| "validate-step".to_string()),
            ontology_refs: vec!["mission-ontology".to_string()],
            shape_refs: vec!["shape:v1".to_string()],
            policy_refs: vec!["policy:test".to_string()],
            contract_refs: Vec::new(),
            schema_hashes: HashMap::from([(
                "provider-if".to_string(),
                format!("sha256:{report_id}"),
            )]),
        }
    }

    fn sample_policy(policy_id: &str, stage: &str, target_type: &str, active: bool) -> SosPolicy {
        let now = Utc::now();
        SosPolicy {
            policy_id: policy_id.to_string(),
            revision: 1,
            policy_name: format!("Policy {policy_id}"),
            description: Some("Synthetic policy".to_string()),
            lifecycle_state: Some(if active { "active" } else { "draft" }.to_string()),
            approval_status: Some(if active { "approved" } else { "pending" }.to_string()),
            approval_requested_by: Some("system".to_string()),
            approval_requested_at: Some(now),
            approved_by: active.then(|| "system".to_string()),
            approved_at: active.then_some(now),
            rejected_by: None,
            rejected_at: None,
            rejection_reason: None,
            target_type: target_type.to_string(),
            target_key: Some(match target_type {
                "interface_pair" => "interface_pair:provider-if:consumer-if".to_string(),
                "contract" => "contract:contract-1".to_string(),
                "system_pair" => "system_pair:source-system:target-system".to_string(),
                "interface" => "interface:provider-if".to_string(),
                _ => "global".to_string(),
            }),
            stages: vec![stage.to_string()],
            enforcement_level: "mandatory".to_string(),
            severity: "high".to_string(),
            sparql_query: "SELECT * WHERE { ?s ?p ?o } LIMIT 1".to_string(),
            context: HashMap::new(),
            tags: vec!["test".to_string()],
            ontology_refs: vec!["sos-core".to_string()],
            shape_refs: vec!["shape:test".to_string()],
            active,
            provider_interface_id: (target_type == "interface_pair")
                .then(|| "provider-if".to_string()),
            consumer_interface_id: (target_type == "interface_pair")
                .then(|| "consumer-if".to_string()),
            contract_id: (target_type == "contract").then(|| "contract-1".to_string()),
            source_system_id: (target_type == "system_pair").then(|| "source-system".to_string()),
            target_system_id: (target_type == "system_pair").then(|| "target-system".to_string()),
            interface_id: (target_type == "interface").then(|| "provider-if".to_string()),
            created_by: "system".to_string(),
            updated_by: "system".to_string(),
            superseded_by_revision: None,
            created_at: now,
            updated_at: now,
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
            sla_metrics: Vec::new(),
            transformation_rules: HashMap::new(),
            description: Some("Synthetic contract".to_string()),
            tags: vec!["test".to_string()],
            approved: false,
            signed: false,
            lifecycle_state: Some("draft".to_string()),
            approval_status: Some("pending".to_string()),
            approval_requested_by: None,
            approval_requested_at: None,
            approved_by: None,
            approved_at: None,
            rejected_by: None,
            rejected_at: None,
            rejection_reason: None,
            signed_by: None,
            signed_at: None,
            created_by: "system".to_string(),
            updated_by: "system".to_string(),
            superseded_by_revision: None,
            created_at: now.clone(),
            updated_at: now,
        }
    }

    #[test]
    fn test_validation_report_indexes_and_history() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let store = SosStore::new(temp_dir.path().to_str().unwrap())?;
        let subject_key = "interface_pair:provider-if:consumer-if";
        let base_time = Utc::now();

        let first = sample_validation_report(
            "report-1",
            subject_key,
            "interface_compatibility",
            base_time,
            None,
            Some("workflow-1"),
        );
        let second = sample_validation_report(
            "report-2",
            subject_key,
            "interface_compatibility",
            base_time + Duration::seconds(1),
            Some("report-1"),
            Some("workflow-1"),
        );

        store.put_validation_report(&first)?;
        store.put_validation_report(&second)?;

        let retrieved = store.get_validation_report("report-2")?;
        assert!(retrieved.is_some());
        assert_eq!(
            retrieved.unwrap().previous_report_id.as_deref(),
            Some("report-1")
        );

        let latest = store.get_latest_validation_report(subject_key)?;
        assert_eq!(latest.unwrap().report_id, "report-2");

        let history = store.list_validation_history(subject_key, 10)?;
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].report_id, "report-2");
        assert_eq!(history[1].report_id, "report-1");

        let by_workflow = store.list_validation_reports_by_workflow_execution("workflow-1")?;
        assert_eq!(by_workflow.len(), 2);
        assert_eq!(by_workflow[0].report_id, "report-2");

        let by_type = store.list_validation_reports_by_type("interface_compatibility", 10)?;
        assert_eq!(by_type.len(), 2);
        assert_eq!(by_type[0].report_id, "report-2");

        let full_scan = store.list_all_validation_reports()?;
        assert_eq!(full_scan.len(), 2);
        assert_eq!(full_scan[0].report_id, "report-2");

        Ok(())
    }

    #[test]
    fn test_validation_report_pruning_removes_secondary_indexes() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let store = SosStore::new(temp_dir.path().to_str().unwrap())?;
        let subject_key = "interface_pair:provider-if:consumer-if";
        let base_time = Utc::now();

        let first = sample_validation_report(
            "report-1",
            subject_key,
            "interface_compatibility",
            base_time,
            None,
            Some("workflow-1"),
        );
        let second = sample_validation_report(
            "report-2",
            subject_key,
            "interface_compatibility",
            base_time + Duration::seconds(1),
            Some("report-1"),
            Some("workflow-1"),
        );
        let third = sample_validation_report(
            "report-3",
            subject_key,
            "interface_compatibility",
            base_time + Duration::seconds(2),
            Some("report-2"),
            Some("workflow-1"),
        );

        store.put_validation_report(&first)?;
        store.put_validation_report(&second)?;
        store.put_validation_report(&third)?;

        let pruned = store.prune_validation_reports_by_subject(subject_key, 2, None)?;
        assert_eq!(
            pruned
                .iter()
                .map(|report| report.report_id.as_str())
                .collect::<Vec<_>>(),
            vec!["report-1"]
        );
        assert!(store.get_validation_report("report-1")?.is_none());

        let latest = store.get_latest_validation_report(subject_key)?;
        assert_eq!(latest.unwrap().report_id, "report-3");

        let history = store.list_validation_history(subject_key, 10)?;
        assert_eq!(
            history
                .iter()
                .map(|report| report.report_id.as_str())
                .collect::<Vec<_>>(),
            vec!["report-3", "report-2"]
        );

        let by_workflow = store.list_validation_reports_by_workflow_execution("workflow-1")?;
        assert_eq!(
            by_workflow
                .iter()
                .map(|report| report.report_id.as_str())
                .collect::<Vec<_>>(),
            vec!["report-3", "report-2"]
        );

        let by_type = store.list_validation_reports_by_type("interface_compatibility", 10)?;
        assert_eq!(
            by_type
                .iter()
                .map(|report| report.report_id.as_str())
                .collect::<Vec<_>>(),
            vec!["report-3", "report-2"]
        );

        Ok(())
    }

    #[test]
    fn test_validation_report_age_pruning_keeps_latest_report_floor() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let store = SosStore::new(temp_dir.path().to_str().unwrap())?;
        let subject_key = "interface_pair:provider-if:consumer-if";
        let base_time = Utc::now() - Duration::days(10);

        let first = sample_validation_report(
            "report-1",
            subject_key,
            "interface_compatibility",
            base_time,
            None,
            Some("workflow-1"),
        );
        let second = sample_validation_report(
            "report-2",
            subject_key,
            "interface_compatibility",
            base_time + Duration::seconds(1),
            Some("report-1"),
            Some("workflow-1"),
        );
        let third = sample_validation_report(
            "report-3",
            subject_key,
            "interface_compatibility",
            base_time + Duration::seconds(2),
            Some("report-2"),
            Some("workflow-1"),
        );

        store.put_validation_report(&first)?;
        store.put_validation_report(&second)?;
        store.put_validation_report(&third)?;

        let cutoff = Utc::now() - Duration::days(7);
        let pruned = store.prune_validation_reports_by_subject(subject_key, 10, Some(cutoff))?;
        assert_eq!(
            pruned
                .iter()
                .map(|report| report.report_id.as_str())
                .collect::<Vec<_>>(),
            vec!["report-2", "report-1"]
        );

        let latest = store
            .get_latest_validation_report(subject_key)?
            .expect("latest report should still exist");
        assert_eq!(latest.report_id, "report-3");

        let history = store.list_validation_history(subject_key, 10)?;
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].report_id, "report-3");

        let by_workflow = store.list_validation_reports_by_workflow_execution("workflow-1")?;
        assert_eq!(
            by_workflow
                .iter()
                .map(|report| report.report_id.as_str())
                .collect::<Vec<_>>(),
            vec!["report-3"]
        );

        let by_type = store.list_validation_reports_by_type("interface_compatibility", 10)?;
        assert_eq!(
            by_type
                .iter()
                .map(|report| report.report_id.as_str())
                .collect::<Vec<_>>(),
            vec!["report-3"]
        );

        Ok(())
    }

    #[test]
    fn test_policy_indexes_and_stage_lookup() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let store = SosStore::new(temp_dir.path().to_str().unwrap())?;

        let mut policy = sample_policy("policy-1", "pre_execution", "interface_pair", true);
        let inactive_policy = sample_policy("policy-2", "in_flight", "interface", false);

        store.put_policy(&policy)?;
        store.put_policy(&inactive_policy)?;

        let retrieved = store.get_policy("policy-1")?;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().target_type, "interface_pair");

        let pre_execution = store.list_policies_by_stage("pre_execution", 10)?;
        assert_eq!(pre_execution.len(), 1);
        assert_eq!(pre_execution[0].policy_id, "policy-1");
        assert!(pre_execution[0].active);

        let all_policies = store.list_all_policies(0, 10)?;
        assert_eq!(all_policies.len(), 2);

        policy.stages = vec!["post_execution".to_string()];
        policy.active = false;
        policy.revision = 2;
        policy.updated_by = "operator-1".to_string();
        policy.updated_at = Utc::now() + Duration::seconds(1);
        store.put_policy(&policy)?;

        let pre_execution_after_update = store.list_policies_by_stage("pre_execution", 10)?;
        assert!(pre_execution_after_update.is_empty());

        let post_execution = store.list_policies_by_stage("post_execution", 10)?;
        assert_eq!(post_execution.len(), 1);
        assert_eq!(post_execution[0].policy_id, "policy-1");
        assert_eq!(post_execution[0].revision, 2);
        assert!(!post_execution[0].active);

        let revisions = store.list_policy_revisions("policy-1", 10)?;
        assert_eq!(revisions.len(), 2);
        assert_eq!(revisions[0].revision, 2);
        assert_eq!(revisions[0].updated_by, "operator-1");
        assert_eq!(revisions[1].revision, 1);
        assert_eq!(revisions[1].superseded_by_revision, Some(2));

        let revision_one = store
            .get_policy_revision("policy-1", 1)?
            .expect("first revision should exist");
        assert_eq!(revision_one.superseded_by_revision, Some(2));

        store.delete_policy("policy-1")?;
        assert!(store.get_policy("policy-1")?.is_none());
        assert!(store
            .list_policies_by_stage("post_execution", 10)?
            .is_empty());
        assert!(store.list_policy_revisions("policy-1", 10)?.is_empty());

        Ok(())
    }

    #[test]
    fn test_policy_same_revision_allows_rollout_metadata_updates() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let store = SosStore::new(temp_dir.path().to_str().unwrap())?;

        let mut policy = sample_policy("policy-rollout", "pre_execution", "interface", false);
        store.put_policy(&policy)?;

        policy.lifecycle_state = Some("dry_run".to_string());
        policy.active = true;
        policy.approval_status = Some("approved".to_string());
        policy.approved_by = Some("reviewer-1".to_string());
        policy.approved_at = Some(Utc::now());
        policy.updated_by = "reviewer-1".to_string();
        policy.updated_at = Utc::now() + Duration::seconds(1);

        store.put_policy(&policy)?;

        let latest = store
            .get_policy("policy-rollout")?
            .expect("policy should remain present");
        assert_eq!(latest.revision, 1);
        assert_eq!(latest.lifecycle_state.as_deref(), Some("dry_run"));
        assert_eq!(latest.approval_status.as_deref(), Some("approved"));
        assert_eq!(latest.updated_by, "reviewer-1");

        let revisions = store.list_policy_revisions("policy-rollout", 10)?;
        assert_eq!(revisions.len(), 1);
        assert_eq!(revisions[0].revision, 1);
        assert_eq!(revisions[0].approval_status.as_deref(), Some("approved"));

        Ok(())
    }

    #[test]
    fn test_policy_same_revision_rejects_semantic_mutation() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let store = SosStore::new(temp_dir.path().to_str().unwrap())?;

        let mut policy = sample_policy("policy-semantic", "pre_execution", "interface", false);
        store.put_policy(&policy)?;

        policy.severity = "critical".to_string();
        let error = store
            .put_policy(&policy)
            .expect_err("same-revision semantic changes should be rejected");

        assert!(
            error
                .to_string()
                .contains("semantic changes require a new revision"),
            "unexpected error: {error}"
        );

        Ok(())
    }

    #[test]
    fn test_policy_approval_request_and_evidence_round_trip() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let store = SosStore::new(temp_dir.path().to_str().unwrap())?;

        let requested_at = Utc::now();
        let request = PolicyApprovalRequestRecord {
            request_id: "request-1".to_string(),
            policy_id: "policy-approval".to_string(),
            policy_revision: 2,
            approval_type: "policy_rollout".to_string(),
            requested_lifecycle_state: "active".to_string(),
            status: "pending".to_string(),
            note: Some("Please review rollout evidence".to_string()),
            requested_by: "architect-1".to_string(),
            requested_at,
            expires_at: Some(requested_at + Duration::hours(4)),
            approved_by: None,
            approved_at: None,
            rejected_by: None,
            rejected_at: None,
            rejection_reason: None,
            metadata: HashMap::from([("ticket".to_string(), json!("SOS-42"))]),
        };
        store.put_policy_approval_request(&request)?;

        let persisted_request = store
            .get_policy_approval_request("request-1")?
            .expect("approval request should exist");
        assert_eq!(persisted_request.policy_id, "policy-approval");
        assert_eq!(persisted_request.policy_revision, 2);

        let listed_requests = store.list_policy_approval_requests("policy-approval", 10)?;
        assert_eq!(listed_requests.len(), 1);
        assert_eq!(listed_requests[0].request_id, "request-1");

        let evidence = PolicyApprovalEvidenceRecord {
            evidence_id: "evidence-1".to_string(),
            request_id: "request-1".to_string(),
            policy_id: "policy-approval".to_string(),
            policy_revision: 2,
            evidence_type: "validation_report".to_string(),
            report_id: "report-1".to_string(),
            added_by: "qa-reviewer".to_string(),
            added_at: requested_at + Duration::minutes(5),
            note: Some("Passing validation".to_string()),
            metadata: HashMap::from([("environment".to_string(), json!("staging"))]),
        };
        store.put_policy_approval_evidence(&evidence)?;

        let listed_evidence = store.list_policy_approval_evidence("request-1")?;
        assert_eq!(listed_evidence.len(), 1);
        assert_eq!(listed_evidence[0].report_id, "report-1");

        Ok(())
    }

    #[test]
    fn test_policy_attestation_round_trip() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let store = SosStore::new(temp_dir.path().to_str().unwrap())?;

        let attested_at = Utc::now();
        let attestation = PolicyAttestationRecord {
            attestation_id: "attestation-1".to_string(),
            policy_id: "policy-approval".to_string(),
            policy_revision: 2,
            policy_revision_ref: "policy:policy-approval@2".to_string(),
            payload_hash: "sha256:payload".to_string(),
            payload_hash_algorithm: "sha256".to_string(),
            signature_algorithm: "ed25519".to_string(),
            signature: "signature".to_string(),
            public_key: "public-key".to_string(),
            key_fingerprint: "sha256:key".to_string(),
            signing_key_ref: Some("sos/policies/signing-key".to_string()),
            signing_key_version: Some("1".to_string()),
            signing_key_source: "secret_store".to_string(),
            attested_by: "reviewer-1".to_string(),
            attested_at,
            approval_request_id: Some("request-1".to_string()),
            evidence_ids: vec!["evidence-1".to_string()],
            policy_refs: vec!["policy:governance".to_string()],
            metadata: HashMap::from([("payload_version".to_string(), json!(1))]),
        };
        store.put_policy_attestation(&attestation)?;

        let persisted = store
            .get_policy_attestation("policy-approval", 2)?
            .expect("policy attestation should exist");
        assert_eq!(persisted.attestation_id, attestation.attestation_id);

        let listed = store.list_policy_attestations("policy-approval", 10)?;
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].policy_revision_ref, "policy:policy-approval@2");

        Ok(())
    }

    #[test]
    fn test_contract_pair_lookup_and_index_cleanup() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let store = SosStore::new(temp_dir.path().to_str().unwrap())?;

        let contract = sample_contract("contract-1", "provider-if", "consumer-if");
        store.put_contract(&contract)?;

        let retrieved = store.get_contract_by_interface_pair("provider-if", "consumer-if")?;
        assert_eq!(retrieved.unwrap().contract_id, "contract-1");

        store.delete_contract("contract-1", "provider-if", "consumer-if")?;
        assert!(store
            .get_contract_by_interface_pair("provider-if", "consumer-if")?
            .is_none());

        Ok(())
    }

    #[test]
    fn test_contract_pair_lookup_backfills_preexisting_contracts() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let store = SosStore::new(temp_dir.path().to_str().unwrap())?;
        let contract = sample_contract("contract-1", "provider-if", "consumer-if");
        let pair_key = SosStore::contract_pair_key("provider-if", "consumer-if", "contract-1");
        let cf_pair_idx = store.cf(CF_CONTRACT_BY_INTERFACE_PAIR)?;

        store.put_contract(&contract)?;

        // Simulate data written before the pair index existed.
        store.db.delete_cf(cf_pair_idx, pair_key.as_bytes())?;

        let retrieved = store.get_contract_by_interface_pair("provider-if", "consumer-if")?;
        assert_eq!(retrieved.unwrap().contract_id, "contract-1");
        assert!(store.db.get_cf(cf_pair_idx, pair_key.as_bytes())?.is_some());

        Ok(())
    }

    #[test]
    fn test_contract_pair_lookup_preserves_lexical_contract_order_for_duplicates() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let store = SosStore::new(temp_dir.path().to_str().unwrap())?;

        store.put_contract(&sample_contract("contract-b", "provider-if", "consumer-if"))?;
        store.put_contract(&sample_contract("contract-a", "provider-if", "consumer-if"))?;

        let retrieved = store.get_contract_by_interface_pair("provider-if", "consumer-if")?;
        assert_eq!(retrieved.unwrap().contract_id, "contract-a");

        Ok(())
    }

    #[test]
    fn test_contract_revisions_track_supersession_and_delete_cleanup() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let store = SosStore::new(temp_dir.path().to_str().unwrap())?;

        let mut contract = sample_contract("contract-rev", "provider-if", "consumer-if");
        store.put_contract(&contract)?;

        contract.revision = 2;
        contract.contract_name = "Contract contract-rev v2".to_string();
        contract.updated_by = "operator-1".to_string();
        contract.updated_at = Utc::now() + Duration::seconds(1);
        store.put_contract(&contract)?;

        let latest = store
            .get_contract("contract-rev")?
            .expect("latest contract should exist");
        assert_eq!(latest.revision, 2);
        assert_eq!(latest.contract_name, "Contract contract-rev v2");

        let revisions = store.list_contract_revisions("contract-rev", 10)?;
        assert_eq!(revisions.len(), 2);
        assert_eq!(revisions[0].revision, 2);
        assert_eq!(revisions[1].revision, 1);
        assert_eq!(revisions[1].superseded_by_revision, Some(2));

        let revision_one = store
            .get_contract_revision("contract-rev", 1)?
            .expect("first revision should be present");
        assert_eq!(revision_one.superseded_by_revision, Some(2));

        store.delete_contract("contract-rev", "provider-if", "consumer-if")?;
        assert!(store.get_contract("contract-rev")?.is_none());
        assert!(store
            .list_contract_revisions("contract-rev", 10)?
            .is_empty());

        Ok(())
    }

    #[test]
    fn test_contract_same_revision_rejects_semantic_mutation_but_allows_governance_updates(
    ) -> Result<()> {
        let temp_dir = TempDir::new()?;
        let store = SosStore::new(temp_dir.path().to_str().unwrap())?;

        let mut contract = sample_contract("contract-governance", "provider-if", "consumer-if");
        store.put_contract(&contract)?;

        contract.approved = true;
        contract.lifecycle_state = Some("approved".to_string());
        contract.approved_by = Some("reviewer-1".to_string());
        contract.approved_at = Some(Utc::now());
        contract.updated_by = "reviewer-1".to_string();
        contract.updated_at = Utc::now() + Duration::seconds(1);
        store.put_contract(&contract)?;

        let latest = store
            .get_contract("contract-governance")?
            .expect("contract should still exist");
        assert_eq!(latest.revision, 1);
        assert!(latest.approved);
        assert_eq!(latest.approved_by.as_deref(), Some("reviewer-1"));

        contract.contract_name = "Mutated semantics".to_string();
        let error = store
            .put_contract(&contract)
            .expect_err("same-revision semantic changes should be rejected");
        assert!(
            error
                .to_string()
                .contains("semantic changes require a new revision"),
            "unexpected error: {error}"
        );

        Ok(())
    }

    #[test]
    fn test_contract_approval_request_and_evidence_round_trip() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let store = SosStore::new(temp_dir.path().to_str().unwrap())?;

        let contract = sample_contract("contract-approval", "provider-if", "consumer-if");
        store.put_contract(&contract)?;

        let requested_at = Utc::now();
        let request = ContractApprovalRequestRecord {
            request_id: "contract-request-1".to_string(),
            contract_id: contract.contract_id.clone(),
            contract_revision: contract.revision,
            approval_type: "contract_approval".to_string(),
            requested_lifecycle_state: "approved".to_string(),
            status: "pending".to_string(),
            note: Some("Ready for review".to_string()),
            requested_by: "operator-1".to_string(),
            requested_at,
            expires_at: None,
            approved_by: None,
            approved_at: None,
            rejected_by: None,
            rejected_at: None,
            rejection_reason: None,
            metadata: HashMap::new(),
        };
        store.put_contract_approval_request(&request)?;

        let stored_request = store
            .get_contract_approval_request("contract-request-1")?
            .expect("request should be persisted");
        assert_eq!(stored_request.contract_id, contract.contract_id);

        let evidence = ContractApprovalEvidenceRecord {
            evidence_id: "contract-evidence-1".to_string(),
            request_id: request.request_id.clone(),
            contract_id: request.contract_id.clone(),
            contract_revision: request.contract_revision,
            evidence_type: "validation_report".to_string(),
            report_id: "report-1".to_string(),
            added_by: "qa-reviewer".to_string(),
            added_at: requested_at + Duration::seconds(1),
            note: Some("Passing interface validation".to_string()),
            metadata: HashMap::new(),
        };
        store.put_contract_approval_evidence(&evidence)?;

        let listed_requests = store.list_contract_approval_requests(&contract.contract_id, 10)?;
        assert_eq!(listed_requests.len(), 1);
        assert_eq!(listed_requests[0].request_id, request.request_id);

        let listed_evidence = store.list_contract_approval_evidence(&request.request_id)?;
        assert_eq!(listed_evidence.len(), 1);
        assert_eq!(listed_evidence[0].evidence_id, evidence.evidence_id);

        store.delete_contract(&contract.contract_id, "provider-if", "consumer-if")?;
        assert!(store
            .list_contract_approval_requests(&contract.contract_id, 10)?
            .is_empty());
        assert!(store
            .list_contract_approval_evidence(&request.request_id)?
            .is_empty());

        Ok(())
    }
}
