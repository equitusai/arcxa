use anyhow::Result;
use chrono::Utc;
use ed25519_dalek::SigningKey;
use graphica_coordinator::api::sos_validation::integration::service::{
    create_sos_validation_callback, SosValidationService, ValidationExecutionOptions,
};
use graphica_coordinator::api::sos_validation::storage::{
    Contract, Interface, SlaMetric, SosPolicy, SosStorageManager, System,
};
use graphica_coordinator::api::sos_validation::types::{
    AddSosPolicyApprovalEvidenceRequest, ApproveSosPolicyApprovalRequest,
    CreateSosPolicyApprovalRequest, CreateSosPolicyRequest, EvaluatePolicyRequest,
    UpdateSosPolicyRequest,
};
use graphica_coordinator::api::sos_validation::PolicyAttestationSigningMaterial;
use graphica_coordinator::governance::rdf_store::{GraphicaRdfStore, RdfStore};
use graphica_core::orchestration::workflow::{
    ExecutionContext, SosValidationConfig, SosValidationSpec,
};
use rand::{rngs::OsRng, RngCore};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tempfile::TempDir;

const SOS_CATALOG_GRAPH: &str = "http://graphica.io/graph/sos-catalog";
const SOS_GOVERNANCE_GRAPH: &str = "http://graphica.io/graph/sos-governance";
const SOS_VALIDATION_GRAPH: &str = "http://graphica.io/graph/sos-validations";
const SOS_NS: &str = "http://graphica.io/sos#";
const PROV_NS: &str = "http://www.w3.org/ns/prov#";
const RDF_NS: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";

fn setup_service_with_rdf() -> Result<(
    TempDir,
    Arc<SosStorageManager>,
    Arc<GraphicaRdfStore>,
    SosValidationService,
)> {
    let temp_dir = TempDir::new()?;
    let storage_manager = Arc::new(SosStorageManager::new(
        temp_dir
            .path()
            .join("sos_policy_seam")
            .to_str()
            .expect("temporary path should be UTF-8"),
    )?);
    let rdf_store = Arc::new(GraphicaRdfStore::new_in_memory()?);
    let service = SosValidationService::new(storage_manager.clone(), Some(rdf_store.clone()), None);
    Ok((temp_dir, storage_manager, rdf_store, service))
}

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
            json!({"from": "SI", "to": "Imperial"}),
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

fn sample_policy(policy_id: &str) -> SosPolicy {
    let now = Utc::now();
    SosPolicy {
        policy_id: policy_id.to_string(),
        revision: 1,
        policy_name: format!("Policy {policy_id}"),
        description: Some("Storage seam validation policy".to_string()),
        lifecycle_state: Some("active".to_string()),
        approval_status: Some("approved".to_string()),
        approval_requested_by: Some("system".to_string()),
        approval_requested_at: Some(now),
        approved_by: Some("system".to_string()),
        approved_at: Some(now),
        rejected_by: None,
        rejected_at: None,
        rejection_reason: None,
        target_type: "interface_pair".to_string(),
        target_key: Some("interface_pair:provider-if:consumer-if".to_string()),
        stages: vec!["pre_execution".to_string()],
        enforcement_level: "mandatory".to_string(),
        severity: "high".to_string(),
        sparql_query: format!(
            "ASK {{ GRAPH <{SOS_CATALOG_GRAPH}> {{ <{}> <{}belongsToSystem> ?system }} }}",
            interface_uri("provider-if"),
            SOS_NS,
        ),
        context: HashMap::new(),
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
        created_by: "system".to_string(),
        updated_by: "system".to_string(),
        superseded_by_revision: None,
        created_at: now,
        updated_at: now,
    }
}

fn sample_interface_pair_policy_request(policy_id: &str) -> CreateSosPolicyRequest {
    CreateSosPolicyRequest {
        policy_id: policy_id.to_string(),
        policy_name: format!("Policy {policy_id}"),
        target_type: "interface_pair".to_string(),
        stages: vec!["pre_execution".to_string()],
        enforcement_level: "mandatory".to_string(),
        severity: "high".to_string(),
        sparql_query: format!(
            "ASK {{ GRAPH <{SOS_CATALOG_GRAPH}> {{ <{{{{provider_interface_uri}}}}> <{SOS_NS}belongsToSystem> ?system }} }}"
        ),
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

fn register_minimal_catalog(storage_manager: &Arc<SosStorageManager>) -> Result<()> {
    storage_manager.put_system(&sample_system("provider-system", "Provider"))?;
    storage_manager.put_system(&sample_system("consumer-system", "Consumer"))?;
    storage_manager.put_interface(&sample_interface("provider-if", "provider-system", "SI"))?;
    storage_manager.put_interface(&sample_interface(
        "consumer-if",
        "consumer-system",
        "Imperial",
    ))?;
    Ok(())
}

fn interface_uri(interface_id: &str) -> String {
    format!("http://graphica.io/sos/interface/{interface_id}")
}

fn policy_uri(policy_id: &str) -> String {
    format!("http://graphica.io/sos/policy/{policy_id}")
}

fn validation_activity_uri(validation_id: &str) -> String {
    format!("http://graphica.io/sos/validation/{validation_id}")
}

fn validation_report_uri(report_id: &str) -> String {
    format!("http://graphica.io/sos/validation-report/{report_id}")
}

fn policy_signing_material() -> PolicyAttestationSigningMaterial {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    PolicyAttestationSigningMaterial {
        signing_key: SigningKey::from_bytes(&bytes),
        signing_key_ref: Some("sos/policies/signing-key".to_string()),
        signing_key_version: Some("1".to_string()),
        signing_key_source: "test".to_string(),
        metadata: HashMap::from([
            ("trust_mode".to_string(), json!("external_reference")),
            ("trust_provider".to_string(), json!("aws-kms")),
            (
                "external_key_ref".to_string(),
                json!("arn:aws:kms:us-east-1:123456789012:key/policy-approval"),
            ),
            (
                "trust_attestation_ref".to_string(),
                json!("kms://policy-approval/attestation/v1"),
            ),
        ]),
    }
}

#[test]
fn sos_policy_storage_round_trip_and_stage_index() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let storage_manager = SosStorageManager::new(
        temp_dir
            .path()
            .join("storage_round_trip")
            .to_str()
            .expect("temporary path should be UTF-8"),
    )?;

    let policy = sample_policy("pair-policy-storage");
    storage_manager.put_policy(&policy)?;

    let persisted = storage_manager
        .get_policy("pair-policy-storage")?
        .expect("policy should exist after storage");
    assert_eq!(persisted.policy_name, "Policy pair-policy-storage");
    assert_eq!(persisted.revision, 1);
    assert_eq!(
        persisted.target_key.as_deref(),
        Some("interface_pair:provider-if:consumer-if")
    );

    let pre_execution = storage_manager.list_policies_by_stage("pre_execution", 10)?;
    assert_eq!(pre_execution.len(), 1);
    assert_eq!(pre_execution[0].policy_id, "pair-policy-storage");

    let all = storage_manager.list_all_policies(0, 10)?;
    assert_eq!(all.len(), 1);

    Ok(())
}

#[test]
fn sos_policy_validation_persists_report_and_projects_graph_lineage() -> Result<()> {
    let (_temp_dir, storage_manager, rdf_store, service) = setup_service_with_rdf()?;
    register_minimal_catalog(&storage_manager)?;
    storage_manager.put_contract(&sample_contract("contract-1", "provider-if", "consumer-if"))?;

    service.create_policy(sample_interface_pair_policy_request("pair-policy"))?;

    let response = service.validate_spec(
        SosValidationSpec::InterfaceCompatibility {
            provider_interface_id: "provider-if".to_string(),
            consumer_interface_id: "consumer-if".to_string(),
        },
        ValidationExecutionOptions::persisted(),
    )?;

    let report_id = response
        .report_id
        .clone()
        .expect("persisted validation should return report_id");
    let persisted = storage_manager
        .get_validation_report(&report_id)?
        .expect("report should be persisted");

    assert!(persisted
        .checks
        .iter()
        .any(|check| check.check_name == "policy:pair-policy"));
    assert!(persisted
        .policy_refs
        .contains(&"policy:pair-policy".to_string()));
    assert!(persisted
        .policy_refs
        .contains(&"policy:pair-policy@1".to_string()));

    let policy_present = rdf_store.query(&format!(
        "ASK {{ GRAPH <{SOS_CATALOG_GRAPH}> {{ <{}> <{}type> <{}Policy> . }} }}",
        policy_uri("pair-policy"),
        RDF_NS,
        SOS_NS,
    ))?;
    assert_eq!(policy_present[0]["ASK"], json!(true));

    let report_present = rdf_store.query(&format!(
        "ASK {{ GRAPH <{SOS_VALIDATION_GRAPH}> {{ <{}> <{}type> <{}Entity> . }} }}",
        validation_report_uri(&report_id),
        RDF_NS,
        PROV_NS,
    ))?;
    assert_eq!(report_present[0]["ASK"], json!(true));

    let activity_present = rdf_store.query(&format!(
        "ASK {{ GRAPH <{SOS_VALIDATION_GRAPH}> {{ <{}> <{}generated> <{}> . }} }}",
        validation_activity_uri(&response.validation_id),
        PROV_NS,
        validation_report_uri(&report_id),
    ))?;
    assert_eq!(activity_present[0]["ASK"], json!(true));

    Ok(())
}

#[test]
fn sos_policy_governance_projects_requests_evidence_and_attestations() -> Result<()> {
    let (_temp_dir, storage_manager, rdf_store, service) = setup_service_with_rdf()?;
    register_minimal_catalog(&storage_manager)?;
    service.reconcile_graphs()?;

    service.create_policy(sample_interface_pair_policy_request("pair-policy-governed"))?;
    service.update_policy(
        "pair-policy-governed",
        UpdateSosPolicyRequest {
            policy_name: None,
            target_type: None,
            stages: None,
            enforcement_level: None,
            severity: Some("critical".to_string()),
            sparql_query: None,
            context: None,
            description: None,
            updated_by: Some("architect-2".to_string()),
            lifecycle_state: Some("draft".to_string()),
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
    )?;

    let approval_request = service.create_policy_approval_request(
        "pair-policy-governed",
        CreateSosPolicyApprovalRequest {
            requested_by: "operator-1".to_string(),
            lifecycle_state: "active".to_string(),
            expires_in_seconds: None,
            note: Some("Ready for enforced rollout".to_string()),
            metadata: HashMap::new(),
        },
    )?;
    let report = service.evaluate_policy_by_id(
        "pair-policy-governed",
        EvaluatePolicyRequest {
            stage: Some("pre_execution".to_string()),
            revision: Some(2),
            context: HashMap::new(),
        },
        ValidationExecutionOptions::persisted(),
    )?;
    let report_id = report
        .report_id
        .expect("persisted policy evaluation should produce evidence");
    let evidence = service.add_policy_approval_evidence(
        "pair-policy-governed",
        &approval_request.request_id,
        AddSosPolicyApprovalEvidenceRequest {
            report_id: report_id.clone(),
            added_by: "qa-reviewer".to_string(),
            note: Some("Passing report for revision 2".to_string()),
            metadata: HashMap::new(),
        },
    )?;
    let approved = service.approve_policy_approval_request_with_attestation(
        "pair-policy-governed",
        &approval_request.request_id,
        ApproveSosPolicyApprovalRequest {
            approved_by: "reviewer-1".to_string(),
        },
        policy_signing_material(),
    )?;

    assert_eq!(approved.status, "approved");
    let approved_policy = service.get_policy("pair-policy-governed")?;
    let attestation = approved_policy
        .attestation
        .expect("approved policy should expose attestation");

    let request_present = rdf_store.query(&format!(
        "ASK {{ GRAPH <{SOS_GOVERNANCE_GRAPH}> {{ <http://graphica.io/sos/policy/pair-policy-governed/approval-request/{}> <{}type> <{}PolicyApprovalRequest> . }} }}",
        approval_request.request_id,
        RDF_NS,
        SOS_NS,
    ))?;
    assert_eq!(request_present[0]["ASK"], json!(true));

    let evidence_present = rdf_store.query(&format!(
        "ASK {{ GRAPH <{SOS_GOVERNANCE_GRAPH}> {{ <http://graphica.io/sos/policy/pair-policy-governed/approval-request/{}/evidence/{}> <{}used> <{}> . }} }}",
        approval_request.request_id,
        evidence.evidence_id,
        PROV_NS,
        validation_report_uri(&report_id),
    ))?;
    assert_eq!(evidence_present[0]["ASK"], json!(true));

    let attestation_present = rdf_store.query(&format!(
        "ASK {{ GRAPH <{SOS_GOVERNANCE_GRAPH}> {{ <http://graphica.io/sos/policy/pair-policy-governed/revision/2/attestation/{}> <{}type> <{}PolicyApprovalAttestation> . }} }}",
        attestation.attestation_id,
        RDF_NS,
        SOS_NS,
    ))?;
    assert_eq!(attestation_present[0]["ASK"], json!(true));

    let revision_link_present = rdf_store.query(&format!(
        "ASK {{ GRAPH <{SOS_GOVERNANCE_GRAPH}> {{ <http://graphica.io/sos/policy/pair-policy-governed/revision/2/attestation/{}> <{}forPolicyRevision> <http://graphica.io/sos/policy/pair-policy-governed@2> . }} }}",
        attestation.attestation_id,
        SOS_NS,
    ))?;
    assert_eq!(revision_link_present[0]["ASK"], json!(true));

    let trust_mode_present = rdf_store.query(&format!(
        "ASK {{ GRAPH <{SOS_GOVERNANCE_GRAPH}> {{ <http://graphica.io/sos/policy/pair-policy-governed/revision/2/attestation/{}> <{}trustMode> \"external_reference\" . }} }}",
        attestation.attestation_id,
        SOS_NS,
    ))?;
    assert_eq!(trust_mode_present[0]["ASK"], json!(true));

    let trust_provider_present = rdf_store.query(&format!(
        "ASK {{ GRAPH <{SOS_GOVERNANCE_GRAPH}> {{ <http://graphica.io/sos/policy/pair-policy-governed/revision/2/attestation/{}> <{}trustProvider> \"aws-kms\" . }} }}",
        attestation.attestation_id,
        SOS_NS,
    ))?;
    assert_eq!(trust_provider_present[0]["ASK"], json!(true));

    let external_key_ref_present = rdf_store.query(&format!(
        "ASK {{ GRAPH <{SOS_GOVERNANCE_GRAPH}> {{ <http://graphica.io/sos/policy/pair-policy-governed/revision/2/attestation/{}> <{}externalKeyRef> \"arn:aws:kms:us-east-1:123456789012:key/policy-approval\" . }} }}",
        attestation.attestation_id,
        SOS_NS,
    ))?;
    assert_eq!(external_key_ref_present[0]["ASK"], json!(true));

    Ok(())
}

#[test]
fn sos_validation_callback_carries_workflow_metadata_into_reports() -> Result<()> {
    let (_temp_dir, storage_manager, _rdf_store, service) = setup_service_with_rdf()?;
    register_minimal_catalog(&storage_manager)?;
    storage_manager.put_contract(&sample_contract("contract-1", "provider-if", "consumer-if"))?;

    let callback = create_sos_validation_callback(Arc::new(service));
    let config = SosValidationConfig {
        validation: SosValidationSpec::ContractCompliance {
            contract_id: "contract-1".to_string(),
        },
        blocking_severities: vec!["error".to_string()],
        persist_report: true,
        emit_graph_lineage: false,
    };

    let mut context = ExecutionContext::new(json!({}));
    context
        .metadata
        .insert("workflow_execution_id".to_string(), "exec-99".to_string());
    context.metadata.insert(
        "workflow_step_id".to_string(),
        "validate-contract".to_string(),
    );

    let step_result = tokio_test::block_on(callback.as_ref()(&config, &context))?;
    let report_id = step_result
        .report_id
        .clone()
        .expect("workflow callback should persist a report");
    let persisted = storage_manager
        .get_validation_report(&report_id)?
        .expect("workflow-linked report should exist");

    assert_eq!(persisted.workflow_execution_id.as_deref(), Some("exec-99"));
    assert_eq!(
        persisted.workflow_step_id.as_deref(),
        Some("validate-contract")
    );

    let linked_reports =
        storage_manager.list_validation_reports_by_workflow_execution("exec-99")?;
    assert_eq!(linked_reports.len(), 1);
    assert_eq!(linked_reports[0].report_id, report_id);

    Ok(())
}
