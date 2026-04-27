use super::*;
use crate::api::sos_validation::contract_governance::contract_revision_ref;
use crate::api::sos_validation::contract_signature::verify_contract_signature;
use crate::api::sos_validation::policy_attestation::{
    verify_policy_attestation, POLICY_ATTESTATION_EXTERNAL_KEY_REF_METADATA_KEY,
    POLICY_ATTESTATION_TRUST_ATTESTATION_REF_METADATA_KEY,
    POLICY_ATTESTATION_TRUST_MODE_METADATA_KEY, POLICY_ATTESTATION_TRUST_PROVIDER_METADATA_KEY,
};
use crate::api::sos_validation::storage::{
    ContractApprovalEvidenceRecord, ContractApprovalRequestRecord, ContractSignatureRecord,
    PolicyApprovalEvidenceRecord, PolicyApprovalRequestRecord, PolicyAttestationRecord,
};
use std::time::Instant;

pub(super) fn subject_resource_uris(subject_type: &str, subject_key: &str) -> Vec<String> {
    match subject_type {
        "interface_pair" => {
            let parts: Vec<_> = subject_key.split(':').collect();
            if parts.len() == 3 {
                vec![interface_uri(parts[1]), interface_uri(parts[2])]
            } else {
                Vec::new()
            }
        }
        "contract" => subject_key
            .strip_prefix("contract:")
            .map(|contract_ref| {
                contract_ref
                    .split_once('@')
                    .and_then(|(contract_id, revision)| {
                        revision.parse::<u32>().ok().map(|revision| {
                            vec![
                                contract_uri(contract_id),
                                contract_revision_uri(contract_id, revision),
                            ]
                        })
                    })
                    .unwrap_or_else(|| vec![contract_uri(contract_ref)])
            })
            .unwrap_or_default(),
        "system_pair" => {
            let parts: Vec<_> = subject_key.split(':').collect();
            if parts.len() == 3 {
                vec![system_uri(parts[1]), system_uri(parts[2])]
            } else {
                Vec::new()
            }
        }
        "interface" => subject_key
            .strip_prefix("interface:")
            .map(|interface_id| vec![interface_uri(interface_id)])
            .unwrap_or_default(),
        "policy" => vec![policy_ref_to_uri(subject_key)],
        _ => Vec::new(),
    }
}

pub(super) fn system_uri(system_id: &str) -> String {
    format!("http://graphica.io/sos/system/{}", system_id)
}

pub(super) fn interface_uri(interface_id: &str) -> String {
    format!("http://graphica.io/sos/interface/{}", interface_id)
}

pub(super) fn contract_uri(contract_id: &str) -> String {
    format!("http://graphica.io/sos/contract/{}", contract_id)
}

pub(super) fn contract_revision_uri(contract_id: &str, revision: u32) -> String {
    format!("{}/revision/{}", contract_uri(contract_id), revision)
}

pub(super) fn contract_approval_request_uri(contract_id: &str, request_id: &str) -> String {
    format!(
        "{}/approval-request/{}",
        contract_uri(contract_id),
        request_id
    )
}

pub(super) fn contract_approval_evidence_uri(
    contract_id: &str,
    request_id: &str,
    evidence_id: &str,
) -> String {
    format!(
        "{}/approval-request/{}/evidence/{}",
        contract_uri(contract_id),
        request_id,
        evidence_id
    )
}

pub(super) fn contract_signature_uri(
    contract_id: &str,
    revision: u32,
    signature_id: &str,
) -> String {
    format!(
        "{}/revision/{}/signature/{}",
        contract_uri(contract_id),
        revision,
        signature_id
    )
}

pub(super) fn policy_uri(policy_id: &str) -> String {
    policy_ref_to_uri(&format!("policy:{policy_id}"))
}

pub(super) fn policy_approval_request_uri(policy_id: &str, request_id: &str) -> String {
    format!("{}/approval-request/{}", policy_uri(policy_id), request_id)
}

pub(super) fn policy_approval_evidence_uri(
    policy_id: &str,
    request_id: &str,
    evidence_id: &str,
) -> String {
    format!(
        "{}/approval-request/{}/evidence/{}",
        policy_uri(policy_id),
        request_id,
        evidence_id
    )
}

pub(super) fn policy_attestation_uri(
    policy_id: &str,
    revision: u32,
    attestation_id: &str,
) -> String {
    format!(
        "{}/revision/{}/attestation/{}",
        policy_uri(policy_id),
        revision,
        attestation_id
    )
}

pub(super) fn validation_activity_uri(validation_id: &str) -> String {
    format!("http://graphica.io/sos/validation/{}", validation_id)
}

pub(super) fn validation_report_uri(report_id: &str) -> String {
    format!("http://graphica.io/sos/validation-report/{}", report_id)
}

pub(super) fn workflow_step_uri(execution_id: &str, step_id: &str) -> String {
    format!(
        "http://graphica.io/workflow/execution/{}/step/{}",
        execution_id, step_id
    )
}

pub(super) fn ontology_ref_to_uri(ontology_ref: &str) -> String {
    if ontology_ref.starts_with("http://") || ontology_ref.starts_with("https://") {
        ontology_ref.to_string()
    } else {
        format!("http://graphica.io/ontology/{}", ontology_ref)
    }
}

pub(super) fn shape_ref_to_uri(shape_ref: &str) -> String {
    if shape_ref.starts_with("http://") || shape_ref.starts_with("https://") {
        shape_ref.to_string()
    } else {
        format!("http://graphica.io/shape/{}", shape_ref)
    }
}

pub(super) fn policy_ref_to_uri(policy_ref: &str) -> String {
    if policy_ref.starts_with("http://") || policy_ref.starts_with("https://") {
        policy_ref.to_string()
    } else {
        format!("http://graphica.io/sos/{}", policy_ref.replace(':', "/"))
    }
}

pub(super) fn contract_ref_to_uri(contract_ref: &str) -> String {
    if contract_ref.starts_with("http://") || contract_ref.starts_with("https://") {
        return contract_ref.to_string();
    }

    if let Some((contract_id, revision)) = contract_ref
        .strip_prefix("contract:")
        .and_then(|value| value.split_once('@'))
        .and_then(|(contract_id, revision)| revision.parse::<u32>().ok().map(|r| (contract_id, r)))
    {
        return contract_revision_uri(contract_id, revision);
    }

    contract_ref
        .strip_prefix("contract:")
        .map(contract_uri)
        .unwrap_or_else(|| format!("http://graphica.io/sos/{}", contract_ref.replace(':', "/")))
}

fn policy_revision_uri(policy: &SosPolicy) -> String {
    policy_ref_to_uri(&policy_revision_ref(policy))
}

fn policy_lifecycle_state(policy: &SosPolicy) -> &str {
    match policy.lifecycle_state.as_deref() {
        Some("draft") | Some("dry_run") | Some("active") | Some("deprecated") | Some("retired") => {
            policy.lifecycle_state.as_deref().unwrap()
        }
        Some(_) | None => {
            if policy.active {
                "active"
            } else {
                "draft"
            }
        }
    }
}

fn policy_approval_status(policy: &SosPolicy) -> &str {
    match policy.approval_status.as_deref() {
        Some("pending") | Some("approved") | Some("rejected") => {
            policy.approval_status.as_deref().unwrap()
        }
        Some(_) | None => {
            if policy_is_automatic(policy)
                || policy.approved_by.is_some()
                || policy.approved_at.is_some()
            {
                "approved"
            } else if policy.rejected_by.is_some() || policy.rejected_at.is_some() {
                "rejected"
            } else {
                "pending"
            }
        }
    }
}

fn policy_is_automatic(policy: &SosPolicy) -> bool {
    matches!(
        policy_lifecycle_state(policy),
        "dry_run" | "active" | "deprecated"
    )
}

pub(super) fn project_system_upsert(
    service: &SosValidationService,
    system: &crate::api::sos_validation::storage::System,
) -> Result<(), SosValidationServiceError> {
    let started = Instant::now();
    let result = {
        let graph = catalog_graph();
        replace_subject_triples(
            service,
            &graph,
            &system_uri(&system.system_id),
            &catalog_system_triples(system),
        )
    };
    service.record_projection_metrics("system", "upsert", started);
    result
}

pub(super) fn project_system_delete(
    service: &SosValidationService,
    system_id: &str,
) -> Result<(), SosValidationServiceError> {
    let started = Instant::now();
    let result = delete_subject_triples(service, &catalog_graph(), &system_uri(system_id));
    service.record_projection_metrics("system", "delete", started);
    result
}

pub(super) fn project_interface_upsert(
    service: &SosValidationService,
    interface: &Interface,
) -> Result<(), SosValidationServiceError> {
    let started = Instant::now();
    let result = {
        let graph = catalog_graph();
        replace_subject_triples(
            service,
            &graph,
            &interface_uri(&interface.interface_id),
            &catalog_interface_triples(interface),
        )
    };
    service.record_projection_metrics("interface", "upsert", started);
    result
}

pub(super) fn project_interface_delete(
    service: &SosValidationService,
    interface_id: &str,
) -> Result<(), SosValidationServiceError> {
    let started = Instant::now();
    let result = delete_subject_triples(service, &catalog_graph(), &interface_uri(interface_id));
    service.record_projection_metrics("interface", "delete", started);
    result
}

pub(super) fn project_contract_upsert(
    service: &SosValidationService,
    contract: &Contract,
) -> Result<(), SosValidationServiceError> {
    let started = Instant::now();
    let result = {
        let graph = catalog_graph();
        replace_subject_triples(
            service,
            &graph,
            &contract_uri(&contract.contract_id),
            &catalog_contract_latest_triples(contract),
        )?;
        replace_subject_triples(
            service,
            &graph,
            &contract_revision_uri(&contract.contract_id, contract.revision),
            &catalog_contract_revision_triples(service, contract)?,
        )?;
        if let Some(signature) = service
            .storage_manager
            .get_contract_signature(&contract.contract_id, contract.revision)
            .map_err(map_storage_error)?
        {
            replace_subject_triples(
                service,
                &governance_graph(),
                &contract_signature_uri(
                    &signature.contract_id,
                    signature.contract_revision,
                    &signature.signature_id,
                ),
                &governance_contract_signature_triples(contract, &signature),
            )?;
        }

        if contract.revision > 1 {
            if let Some(previous_revision) = service
                .storage_manager
                .get_contract_revision(&contract.contract_id, contract.revision - 1)
                .map_err(map_storage_error)?
            {
                replace_subject_triples(
                    service,
                    &graph,
                    &contract_revision_uri(
                        &previous_revision.contract_id,
                        previous_revision.revision,
                    ),
                    &catalog_contract_revision_triples(service, &previous_revision)?,
                )?;
            }
        }

        Ok(())
    };
    service.record_projection_metrics("contract", "upsert", started);
    result
}

pub(super) fn project_contract_delete(
    service: &SosValidationService,
    contract_id: &str,
) -> Result<(), SosValidationServiceError> {
    let started = Instant::now();
    let result = delete_contract_related_triples(service, &catalog_graph(), contract_id)
        .and_then(|_| delete_contract_related_triples(service, &governance_graph(), contract_id));
    service.record_projection_metrics("contract", "delete", started);
    result
}

pub(super) fn project_contract_approval_request_upsert(
    service: &SosValidationService,
    request: &ContractApprovalRequestRecord,
) -> Result<(), SosValidationServiceError> {
    let started = Instant::now();
    let result = replace_subject_triples(
        service,
        &governance_graph(),
        &contract_approval_request_uri(&request.contract_id, &request.request_id),
        &governance_contract_approval_request_triples(request),
    );
    service.record_projection_metrics("contract_approval_request", "upsert", started);
    result
}

pub(super) fn project_contract_approval_evidence_upsert(
    service: &SosValidationService,
    evidence: &ContractApprovalEvidenceRecord,
) -> Result<(), SosValidationServiceError> {
    let started = Instant::now();
    let result = replace_subject_triples(
        service,
        &governance_graph(),
        &contract_approval_evidence_uri(
            &evidence.contract_id,
            &evidence.request_id,
            &evidence.evidence_id,
        ),
        &governance_contract_approval_evidence_triples(evidence),
    );
    service.record_projection_metrics("contract_approval_evidence", "upsert", started);
    result
}

pub(super) fn project_policy_upsert(
    service: &SosValidationService,
    policy: &SosPolicy,
) -> Result<(), SosValidationServiceError> {
    let started = Instant::now();
    let result = (|| {
        let graph = catalog_graph();
        replace_subject_triples(
            service,
            &graph,
            &policy_uri(&policy.policy_id),
            &catalog_policy_latest_triples(policy),
        )?;
        replace_subject_triples(
            service,
            &graph,
            &policy_revision_uri(policy),
            &catalog_policy_revision_triples(policy),
        )?;
        if let Some(attestation) = service
            .storage_manager
            .get_policy_attestation(&policy.policy_id, policy.revision)
            .map_err(map_storage_error)?
        {
            replace_subject_triples(
                service,
                &governance_graph(),
                &policy_attestation_uri(
                    &attestation.policy_id,
                    attestation.policy_revision,
                    &attestation.attestation_id,
                ),
                &governance_policy_attestation_triples(policy, &attestation),
            )?;
        }

        if policy.revision > 1 {
            if let Some(previous_revision) = service
                .storage_manager
                .get_policy_revision(&policy.policy_id, policy.revision - 1)
                .map_err(map_storage_error)?
            {
                replace_subject_triples(
                    service,
                    &graph,
                    &policy_revision_uri(&previous_revision),
                    &catalog_policy_revision_triples(&previous_revision),
                )?;
            }
        }

        Ok(())
    })();
    service.record_projection_metrics("policy", "upsert", started);
    result
}

pub(super) fn project_policy_delete(
    service: &SosValidationService,
    policy_id: &str,
    revisions: &[SosPolicy],
    approval_requests: &[PolicyApprovalRequestRecord],
    approval_evidence: &[PolicyApprovalEvidenceRecord],
    attestations: &[PolicyAttestationRecord],
) -> Result<(), SosValidationServiceError> {
    let started = Instant::now();
    let result = (|| {
        let graph = catalog_graph();
        let governance_graph = governance_graph();

        delete_subject_triples(service, &graph, &policy_uri(policy_id))?;

        for revision in revisions {
            delete_subject_triples(service, &graph, &policy_revision_uri(&revision))?;
        }

        for request in approval_requests {
            delete_subject_triples(
                service,
                &governance_graph,
                &policy_approval_request_uri(&request.policy_id, &request.request_id),
            )?;
        }
        for evidence in approval_evidence {
            delete_subject_triples(
                service,
                &governance_graph,
                &policy_approval_evidence_uri(
                    &evidence.policy_id,
                    &evidence.request_id,
                    &evidence.evidence_id,
                ),
            )?;
        }
        for attestation in attestations {
            delete_subject_triples(
                service,
                &governance_graph,
                &policy_attestation_uri(
                    &attestation.policy_id,
                    attestation.policy_revision,
                    &attestation.attestation_id,
                ),
            )?;
        }

        Ok(())
    })();
    service.record_projection_metrics("policy", "delete", started);
    result
}

pub(super) fn project_policy_approval_request_upsert(
    service: &SosValidationService,
    request: &PolicyApprovalRequestRecord,
) -> Result<(), SosValidationServiceError> {
    let started = Instant::now();
    let result = replace_subject_triples(
        service,
        &governance_graph(),
        &policy_approval_request_uri(&request.policy_id, &request.request_id),
        &governance_policy_approval_request_triples(request),
    );
    service.record_projection_metrics("policy_approval_request", "upsert", started);
    result
}

pub(super) fn project_policy_approval_evidence_upsert(
    service: &SosValidationService,
    evidence: &PolicyApprovalEvidenceRecord,
) -> Result<(), SosValidationServiceError> {
    let started = Instant::now();
    let result = replace_subject_triples(
        service,
        &governance_graph(),
        &policy_approval_evidence_uri(
            &evidence.policy_id,
            &evidence.request_id,
            &evidence.evidence_id,
        ),
        &governance_policy_approval_evidence_triples(evidence),
    );
    service.record_projection_metrics("policy_approval_evidence", "upsert", started);
    result
}

pub(super) fn project_policy_attestation_upsert(
    service: &SosValidationService,
    policy: &SosPolicy,
    attestation: &PolicyAttestationRecord,
) -> Result<(), SosValidationServiceError> {
    let started = Instant::now();
    let result = replace_subject_triples(
        service,
        &governance_graph(),
        &policy_attestation_uri(
            &attestation.policy_id,
            attestation.policy_revision,
            &attestation.attestation_id,
        ),
        &governance_policy_attestation_triples(policy, attestation),
    );
    service.record_projection_metrics("policy_attestation", "upsert", started);
    result
}

pub(super) fn project_validation_report_upsert(
    service: &SosValidationService,
    report: &ValidationReport,
) -> Result<(), SosValidationServiceError> {
    let started = Instant::now();
    let result = (|| {
        let graph = validation_graph();
        delete_subject_triples(
            service,
            &graph,
            &validation_activity_uri(&report.validation_id),
        )?;
        delete_subject_triples(service, &graph, &validation_report_uri(&report.report_id))?;
        insert_triples(service, &graph, &validation_report_triples(report))
    })();
    service.record_projection_metrics("validation_report", "upsert", started);
    result
}

pub(super) fn project_validation_report_delete(
    service: &SosValidationService,
    report: &ValidationReport,
) -> Result<(), SosValidationServiceError> {
    let started = Instant::now();
    let result = (|| {
        let graph = validation_graph();
        delete_subject_triples(
            service,
            &graph,
            &validation_activity_uri(&report.validation_id),
        )?;
        delete_subject_triples(service, &graph, &validation_report_uri(&report.report_id))
    })();
    service.record_projection_metrics("validation_report", "delete", started);
    result
}

pub(super) fn reconcile_graphs(
    service: &SosValidationService,
) -> Result<(), SosValidationServiceError> {
    let started = Instant::now();
    let result = (|| {
        let Some(rdf_store) = service.rdf_store.as_ref() else {
            return Ok(());
        };

        let catalog_graph = catalog_graph();
        let validation_graph = validation_graph();
        let governance_graph = governance_graph();

        rdf_store
            .clear_graph(&catalog_graph)
            .map_err(map_rdf_error)?;
        rdf_store
            .clear_graph(&validation_graph)
            .map_err(map_rdf_error)?;
        rdf_store
            .clear_graph(&governance_graph)
            .map_err(map_rdf_error)?;

        let systems = service
            .storage_manager
            .list_all_systems(0, usize::MAX)
            .map_err(map_storage_error)?;
        let interfaces = service
            .storage_manager
            .list_all_interfaces(0, usize::MAX)
            .map_err(map_storage_error)?;
        let contracts = service
            .storage_manager
            .list_all_contracts(0, usize::MAX)
            .map_err(map_storage_error)?;
        let contract_revisions = service
            .storage_manager
            .list_all_contract_revisions()
            .map_err(map_storage_error)?;
        let contract_approval_requests = service
            .storage_manager
            .list_all_contract_approval_requests()
            .map_err(map_storage_error)?;
        let contract_approval_evidence = service
            .storage_manager
            .list_all_contract_approval_evidence()
            .map_err(map_storage_error)?;
        let contract_signatures = service
            .storage_manager
            .list_all_contract_signatures()
            .map_err(map_storage_error)?;
        let policies = service
            .storage_manager
            .list_all_policies(0, usize::MAX)
            .map_err(map_storage_error)?;
        let policy_revisions = service
            .storage_manager
            .list_all_policy_revisions()
            .map_err(map_storage_error)?;
        let policy_approval_requests = service
            .storage_manager
            .list_all_policy_approval_requests()
            .map_err(map_storage_error)?;
        let policy_approval_evidence = service
            .storage_manager
            .list_all_policy_approval_evidence()
            .map_err(map_storage_error)?;
        let policy_attestations = service
            .storage_manager
            .list_all_policy_attestations()
            .map_err(map_storage_error)?;
        let reports = service
            .storage_manager
            .list_all_validation_reports()
            .map_err(map_storage_error)?;

        let mut catalog_triples = Vec::new();
        for system in &systems {
            catalog_triples.extend(catalog_system_triples(system));
        }
        for interface in &interfaces {
            catalog_triples.extend(catalog_interface_triples(interface));
        }
        for contract in &contracts {
            catalog_triples.extend(catalog_contract_latest_triples(contract));
        }
        for contract_revision in &contract_revisions {
            catalog_triples.extend(catalog_contract_revision_triples(
                service,
                contract_revision,
            )?);
        }
        for policy in &policies {
            catalog_triples.extend(catalog_policy_latest_triples(policy));
        }
        for policy_revision in &policy_revisions {
            catalog_triples.extend(catalog_policy_revision_triples(policy_revision));
        }

        insert_triples(service, &catalog_graph, &catalog_triples)?;

        let contract_revision_map = contract_revisions
            .iter()
            .map(|contract| {
                (
                    (contract.contract_id.clone(), contract.revision),
                    contract.clone(),
                )
            })
            .collect::<std::collections::HashMap<_, _>>();
        let policy_revision_map = policy_revisions
            .iter()
            .map(|policy| ((policy.policy_id.clone(), policy.revision), policy.clone()))
            .collect::<std::collections::HashMap<_, _>>();
        let mut governance_triples = Vec::new();
        for request in &contract_approval_requests {
            governance_triples.extend(governance_contract_approval_request_triples(request));
        }
        for evidence in &contract_approval_evidence {
            governance_triples.extend(governance_contract_approval_evidence_triples(evidence));
        }
        for signature in &contract_signatures {
            if let Some(contract) = contract_revision_map
                .get(&(signature.contract_id.clone(), signature.contract_revision))
            {
                governance_triples
                    .extend(governance_contract_signature_triples(contract, signature));
            }
        }
        for request in &policy_approval_requests {
            governance_triples.extend(governance_policy_approval_request_triples(request));
        }
        for evidence in &policy_approval_evidence {
            governance_triples.extend(governance_policy_approval_evidence_triples(evidence));
        }
        for attestation in &policy_attestations {
            if let Some(policy) = policy_revision_map
                .get(&(attestation.policy_id.clone(), attestation.policy_revision))
            {
                governance_triples
                    .extend(governance_policy_attestation_triples(policy, attestation));
            }
        }

        insert_triples(service, &governance_graph, &governance_triples)?;

        let mut validation_triples = Vec::new();
        for report in &reports {
            validation_triples.extend(validation_report_triples(report));
        }

        insert_triples(service, &validation_graph, &validation_triples)?;

        Ok(())
    })();
    service.record_projection_metrics("graph", "reconcile", started);
    result
}

fn replace_subject_triples(
    service: &SosValidationService,
    graph: &NamedGraph,
    subject_uri: &str,
    triples: &[RdfTriple],
) -> Result<(), SosValidationServiceError> {
    delete_subject_triples(service, graph, subject_uri)?;
    insert_triples(service, graph, triples)
}

fn delete_subject_triples(
    service: &SosValidationService,
    graph: &NamedGraph,
    subject_uri: &str,
) -> Result<(), SosValidationServiceError> {
    let Some(rdf_store) = service.rdf_store.as_ref() else {
        return Ok(());
    };

    let delete_query = format!(
        "DELETE WHERE {{ GRAPH <{}> {{ <{}> ?p ?o . }} }}",
        graph.uri, subject_uri
    );

    rdf_store.update(&delete_query).map_err(map_rdf_error)
}

fn delete_contract_related_triples(
    service: &SosValidationService,
    graph: &NamedGraph,
    contract_id: &str,
) -> Result<(), SosValidationServiceError> {
    let Some(rdf_store) = service.rdf_store.as_ref() else {
        return Ok(());
    };

    let prefix = contract_uri(contract_id);
    let delete_query = format!(
        "DELETE WHERE {{ GRAPH <{}> {{ ?s ?p ?o . FILTER(STRSTARTS(STR(?s), \"{}\")) }} }}",
        graph.uri, prefix
    );

    rdf_store.update(&delete_query).map_err(map_rdf_error)
}

fn insert_triples(
    service: &SosValidationService,
    graph: &NamedGraph,
    triples: &[RdfTriple],
) -> Result<(), SosValidationServiceError> {
    let Some(rdf_store) = service.rdf_store.as_ref() else {
        return Ok(());
    };

    if triples.is_empty() {
        return Ok(());
    }

    rdf_store
        .insert_batch(triples, Some(graph))
        .map_err(map_rdf_error)
}

fn metadata_string<'a>(
    metadata: &'a std::collections::HashMap<String, serde_json::Value>,
    key: &str,
) -> Option<&'a str> {
    metadata.get(key).and_then(|value| value.as_str())
}

fn catalog_graph() -> NamedGraph {
    NamedGraph::new("http://graphica.io/graph/sos-catalog")
}

fn validation_graph() -> NamedGraph {
    NamedGraph::new("http://graphica.io/graph/sos-validations")
}

fn governance_graph() -> NamedGraph {
    NamedGraph::new("http://graphica.io/graph/sos-governance")
}

fn catalog_system_triples(system: &crate::api::sos_validation::storage::System) -> Vec<RdfTriple> {
    let uri = system_uri(&system.system_id);
    vec![
        RdfTriple::new(&uri, format!("{}type", RDF_NS), format!("{}System", SOS_NS)),
        RdfTriple::new_literal(&uri, format!("{}systemName", SOS_NS), &system.system_name),
        RdfTriple::new_literal(&uri, format!("{}systemType", SOS_NS), &system.system_type),
        RdfTriple::new_literal(&uri, format!("{}vendor", SOS_NS), &system.vendor),
        RdfTriple::new_literal(&uri, format!("{}version", SOS_NS), &system.version),
        RdfTriple::new_literal(
            &uri,
            format!("{}classification", SOS_NS),
            &system.classification,
        ),
        RdfTriple::new_typed(
            &uri,
            format!("{}active", SOS_NS),
            system.active.to_string(),
            XSD_BOOLEAN,
        ),
        RdfTriple::new_typed(
            &uri,
            format!("{}createdAt", SOS_NS),
            system.created_at.to_rfc3339(),
            XSD_DATE_TIME,
        ),
        RdfTriple::new_typed(
            &uri,
            format!("{}updatedAt", SOS_NS),
            system.updated_at.to_rfc3339(),
            XSD_DATE_TIME,
        ),
    ]
}

fn catalog_interface_triples(interface: &Interface) -> Vec<RdfTriple> {
    let uri = interface_uri(&interface.interface_id);
    let mut triples = vec![
        RdfTriple::new(
            &uri,
            format!("{}type", RDF_NS),
            format!("{}Interface", SOS_NS),
        ),
        RdfTriple::new(
            &uri,
            format!("{}belongsToSystem", SOS_NS),
            system_uri(&interface.system_id),
        ),
        RdfTriple::new_literal(
            &uri,
            format!("{}interfaceName", SOS_NS),
            &interface.interface_name,
        ),
        RdfTriple::new_literal(&uri, format!("{}direction", SOS_NS), &interface.direction),
        RdfTriple::new_literal(&uri, format!("{}protocol", SOS_NS), &interface.protocol),
        RdfTriple::new_literal(
            &uri,
            format!("{}dataFormat", SOS_NS),
            &interface.data_format,
        ),
        RdfTriple::new_typed(
            &uri,
            format!("{}createdAt", SOS_NS),
            interface.created_at.to_rfc3339(),
            XSD_DATE_TIME,
        ),
        RdfTriple::new_typed(
            &uri,
            format!("{}updatedAt", SOS_NS),
            interface.updated_at.to_rfc3339(),
            XSD_DATE_TIME,
        ),
    ];

    if let Some(coordinate_system) = &interface.coordinate_system {
        triples.push(RdfTriple::new_literal(
            &uri,
            format!("{}coordinateSystem", SOS_NS),
            coordinate_system,
        ));
    }
    if let Some(unit_system) = &interface.unit_system {
        triples.push(RdfTriple::new_literal(
            &uri,
            format!("{}unitSystem", SOS_NS),
            unit_system,
        ));
    }

    triples
}

fn catalog_contract_triples(contract: &Contract) -> Vec<RdfTriple> {
    let uri = contract_uri(&contract.contract_id);
    let revision_uri = contract_revision_uri(&contract.contract_id, contract.revision);
    let approval_status = effective_contract_approval_status(contract);
    let mut triples = vec![
        RdfTriple::new(
            &uri,
            format!("{}type", RDF_NS),
            format!("{}Contract", SOS_NS),
        ),
        RdfTriple::new(&uri, format!("{}currentRevision", SOS_NS), revision_uri),
        RdfTriple::new_typed(
            &uri,
            format!("{}revision", SOS_NS),
            contract.revision.to_string(),
            "http://www.w3.org/2001/XMLSchema#unsignedInt",
        ),
        RdfTriple::new_literal(
            &uri,
            format!("{}contractName", SOS_NS),
            &contract.contract_name,
        ),
        RdfTriple::new(
            &uri,
            format!("{}providerInterface", SOS_NS),
            interface_uri(&contract.provider_interface_id),
        ),
        RdfTriple::new(
            &uri,
            format!("{}consumerInterface", SOS_NS),
            interface_uri(&contract.consumer_interface_id),
        ),
        RdfTriple::new_typed(
            &uri,
            format!("{}approved", SOS_NS),
            contract.approved.to_string(),
            XSD_BOOLEAN,
        ),
        RdfTriple::new_typed(
            &uri,
            format!("{}signed", SOS_NS),
            contract.signed.to_string(),
            XSD_BOOLEAN,
        ),
        RdfTriple::new_literal(
            &uri,
            format!("{}lifecycleState", SOS_NS),
            effective_contract_lifecycle_state(contract),
        ),
        RdfTriple::new_literal(&uri, format!("{}approvalStatus", SOS_NS), approval_status),
        RdfTriple::new_typed(
            &uri,
            format!("{}createdAt", SOS_NS),
            contract.created_at.to_rfc3339(),
            XSD_DATE_TIME,
        ),
        RdfTriple::new_typed(
            &uri,
            format!("{}updatedAt", SOS_NS),
            contract.updated_at.to_rfc3339(),
            XSD_DATE_TIME,
        ),
        RdfTriple::new_literal(&uri, format!("{}createdBy", SOS_NS), &contract.created_by),
        RdfTriple::new_literal(&uri, format!("{}updatedBy", SOS_NS), &contract.updated_by),
    ];

    if let Some(approved_by) = &contract.approved_by {
        triples.push(RdfTriple::new_literal(
            &uri,
            format!("{}approvedBy", SOS_NS),
            approved_by,
        ));
    }
    if let Some(approved_at) = contract.approved_at.as_ref() {
        triples.push(RdfTriple::new_typed(
            &uri,
            format!("{}approvedAt", SOS_NS),
            approved_at.to_rfc3339(),
            XSD_DATE_TIME,
        ));
    }
    if let Some(requested_by) = &contract.approval_requested_by {
        triples.push(RdfTriple::new_literal(
            &uri,
            format!("{}approvalRequestedBy", SOS_NS),
            requested_by,
        ));
    }
    if let Some(requested_at) = contract.approval_requested_at.as_ref() {
        triples.push(RdfTriple::new_typed(
            &uri,
            format!("{}approvalRequestedAt", SOS_NS),
            requested_at.to_rfc3339(),
            XSD_DATE_TIME,
        ));
    }
    if let Some(rejected_by) = &contract.rejected_by {
        triples.push(RdfTriple::new_literal(
            &uri,
            format!("{}rejectedBy", SOS_NS),
            rejected_by,
        ));
    }
    if let Some(rejected_at) = contract.rejected_at.as_ref() {
        triples.push(RdfTriple::new_typed(
            &uri,
            format!("{}rejectedAt", SOS_NS),
            rejected_at.to_rfc3339(),
            XSD_DATE_TIME,
        ));
    }
    if let Some(rejection_reason) = &contract.rejection_reason {
        triples.push(RdfTriple::new_literal(
            &uri,
            format!("{}rejectionReason", SOS_NS),
            rejection_reason,
        ));
    }
    if let Some(signed_by) = &contract.signed_by {
        triples.push(RdfTriple::new_literal(
            &uri,
            format!("{}signedBy", SOS_NS),
            signed_by,
        ));
    }
    if let Some(signed_at) = contract.signed_at.as_ref() {
        triples.push(RdfTriple::new_typed(
            &uri,
            format!("{}signedAt", SOS_NS),
            signed_at.to_rfc3339(),
            XSD_DATE_TIME,
        ));
    }
    if let Some(superseded_by_revision) = contract.superseded_by_revision {
        triples.push(RdfTriple::new_literal(
            &uri,
            format!("{}supersededByRevision", SOS_NS),
            superseded_by_revision.to_string(),
        ));
    }

    triples
}

fn catalog_contract_latest_triples(contract: &Contract) -> Vec<RdfTriple> {
    catalog_contract_triples(contract)
}

fn catalog_contract_revision_triples(
    service: &SosValidationService,
    contract: &Contract,
) -> Result<Vec<RdfTriple>, SosValidationServiceError> {
    let latest_uri = contract_uri(&contract.contract_id);
    let revision_uri = contract_revision_uri(&contract.contract_id, contract.revision);
    let approval_status = effective_contract_approval_status(contract);
    let signature = service
        .storage_manager
        .get_contract_signature(&contract.contract_id, contract.revision)
        .map_err(map_storage_error)?;
    let mut triples = vec![
        RdfTriple::new(
            &revision_uri,
            format!("{}type", RDF_NS),
            format!("{}ContractRevision", SOS_NS),
        ),
        RdfTriple::new(
            &revision_uri,
            format!("{}specializationOf", PROV_NS),
            latest_uri,
        ),
        RdfTriple::new_literal(
            &revision_uri,
            format!("{}contractName", SOS_NS),
            &contract.contract_name,
        ),
        RdfTriple::new(
            &revision_uri,
            format!("{}providerInterface", SOS_NS),
            interface_uri(&contract.provider_interface_id),
        ),
        RdfTriple::new(
            &revision_uri,
            format!("{}consumerInterface", SOS_NS),
            interface_uri(&contract.consumer_interface_id),
        ),
        RdfTriple::new_typed(
            &revision_uri,
            format!("{}revision", SOS_NS),
            contract.revision.to_string(),
            "http://www.w3.org/2001/XMLSchema#unsignedInt",
        ),
        RdfTriple::new_typed(
            &revision_uri,
            format!("{}approved", SOS_NS),
            contract.approved.to_string(),
            XSD_BOOLEAN,
        ),
        RdfTriple::new_typed(
            &revision_uri,
            format!("{}signed", SOS_NS),
            contract.signed.to_string(),
            XSD_BOOLEAN,
        ),
        RdfTriple::new_literal(
            &revision_uri,
            format!("{}lifecycleState", SOS_NS),
            effective_contract_lifecycle_state(contract),
        ),
        RdfTriple::new_literal(
            &revision_uri,
            format!("{}approvalStatus", SOS_NS),
            approval_status,
        ),
        RdfTriple::new_literal(
            &revision_uri,
            format!("{}createdBy", SOS_NS),
            &contract.created_by,
        ),
        RdfTriple::new_literal(
            &revision_uri,
            format!("{}updatedBy", SOS_NS),
            &contract.updated_by,
        ),
        RdfTriple::new_typed(
            &revision_uri,
            format!("{}createdAt", SOS_NS),
            contract.created_at.to_rfc3339(),
            XSD_DATE_TIME,
        ),
        RdfTriple::new_typed(
            &revision_uri,
            format!("{}updatedAt", SOS_NS),
            contract.updated_at.to_rfc3339(),
            XSD_DATE_TIME,
        ),
    ];

    if contract.revision > 1 {
        triples.push(RdfTriple::new(
            &revision_uri,
            format!("{}wasRevisionOf", PROV_NS),
            contract_revision_uri(&contract.contract_id, contract.revision - 1),
        ));
    }
    if let Some(superseded_by_revision) = contract.superseded_by_revision {
        triples.push(RdfTriple::new(
            &revision_uri,
            format!("{}supersededBy", SOS_NS),
            contract_revision_uri(&contract.contract_id, superseded_by_revision),
        ));
    }
    if let Some(description) = &contract.description {
        triples.push(RdfTriple::new_literal(
            &revision_uri,
            format!("{}description", SOS_NS),
            description,
        ));
    }
    for tag in &contract.tags {
        triples.push(RdfTriple::new_literal(
            &revision_uri,
            format!("{}tag", SOS_NS),
            tag,
        ));
    }
    if !contract.sla_metrics.is_empty() {
        triples.push(RdfTriple::new_literal(
            &revision_uri,
            format!("{}slaMetricsJson", SOS_NS),
            serde_json::to_string(&contract.sla_metrics)
                .map_err(|error| map_internal_error(error.into()))?,
        ));
    }
    if !contract.transformation_rules.is_empty() {
        triples.push(RdfTriple::new_literal(
            &revision_uri,
            format!("{}transformationRulesJson", SOS_NS),
            serde_json::to_string(&contract.transformation_rules)
                .map_err(|error| map_internal_error(error.into()))?,
        ));
    }
    if let Some(requested_by) = &contract.approval_requested_by {
        triples.push(RdfTriple::new_literal(
            &revision_uri,
            format!("{}approvalRequestedBy", SOS_NS),
            requested_by,
        ));
    }
    if let Some(requested_at) = contract.approval_requested_at.as_ref() {
        triples.push(RdfTriple::new_typed(
            &revision_uri,
            format!("{}approvalRequestedAt", SOS_NS),
            requested_at.to_rfc3339(),
            XSD_DATE_TIME,
        ));
    }
    if let Some(approved_by) = &contract.approved_by {
        triples.push(RdfTriple::new_literal(
            &revision_uri,
            format!("{}approvedBy", SOS_NS),
            approved_by,
        ));
    }
    if let Some(approved_at) = contract.approved_at.as_ref() {
        triples.push(RdfTriple::new_typed(
            &revision_uri,
            format!("{}approvedAt", SOS_NS),
            approved_at.to_rfc3339(),
            XSD_DATE_TIME,
        ));
    }
    if let Some(rejected_by) = &contract.rejected_by {
        triples.push(RdfTriple::new_literal(
            &revision_uri,
            format!("{}rejectedBy", SOS_NS),
            rejected_by,
        ));
    }
    if let Some(rejected_at) = contract.rejected_at.as_ref() {
        triples.push(RdfTriple::new_typed(
            &revision_uri,
            format!("{}rejectedAt", SOS_NS),
            rejected_at.to_rfc3339(),
            XSD_DATE_TIME,
        ));
    }
    if let Some(rejection_reason) = &contract.rejection_reason {
        triples.push(RdfTriple::new_literal(
            &revision_uri,
            format!("{}rejectionReason", SOS_NS),
            rejection_reason,
        ));
    }
    if let Some(signed_by) = &contract.signed_by {
        triples.push(RdfTriple::new_literal(
            &revision_uri,
            format!("{}signedBy", SOS_NS),
            signed_by,
        ));
    }
    if let Some(signed_at) = contract.signed_at.as_ref() {
        triples.push(RdfTriple::new_typed(
            &revision_uri,
            format!("{}signedAt", SOS_NS),
            signed_at.to_rfc3339(),
            XSD_DATE_TIME,
        ));
    }
    if let Some(signature) = signature.as_ref() {
        triples.push(RdfTriple::new(
            &revision_uri,
            format!("{}hasSignatureAttestation", SOS_NS),
            contract_signature_uri(
                &signature.contract_id,
                signature.contract_revision,
                &signature.signature_id,
            ),
        ));
    }

    Ok(triples)
}

fn governance_contract_approval_request_triples(
    request: &ContractApprovalRequestRecord,
) -> Vec<RdfTriple> {
    let uri = contract_approval_request_uri(&request.contract_id, &request.request_id);
    let mut triples = vec![
        RdfTriple::new(
            &uri,
            format!("{}type", RDF_NS),
            format!("{}ContractApprovalRequest", SOS_NS),
        ),
        RdfTriple::new(
            &uri,
            format!("{}forContract", SOS_NS),
            contract_uri(&request.contract_id),
        ),
        RdfTriple::new(
            &uri,
            format!("{}forContractRevision", SOS_NS),
            contract_revision_uri(&request.contract_id, request.contract_revision),
        ),
        RdfTriple::new_literal(
            &uri,
            format!("{}approvalType", SOS_NS),
            &request.approval_type,
        ),
        RdfTriple::new_literal(
            &uri,
            format!("{}requestedLifecycleState", SOS_NS),
            &request.requested_lifecycle_state,
        ),
        RdfTriple::new_literal(&uri, format!("{}status", SOS_NS), &request.status),
        RdfTriple::new_literal(
            &uri,
            format!("{}requestedBy", SOS_NS),
            &request.requested_by,
        ),
        RdfTriple::new_typed(
            &uri,
            format!("{}requestedAt", SOS_NS),
            request.requested_at.to_rfc3339(),
            XSD_DATE_TIME,
        ),
    ];

    if let Some(note) = &request.note {
        triples.push(RdfTriple::new_literal(
            &uri,
            format!("{}note", SOS_NS),
            note,
        ));
    }
    if let Some(expires_at) = request.expires_at.as_ref() {
        triples.push(RdfTriple::new_typed(
            &uri,
            format!("{}expiresAt", SOS_NS),
            expires_at.to_rfc3339(),
            XSD_DATE_TIME,
        ));
    }
    if let Some(approved_by) = &request.approved_by {
        triples.push(RdfTriple::new_literal(
            &uri,
            format!("{}approvedBy", SOS_NS),
            approved_by,
        ));
    }
    if let Some(approved_at) = request.approved_at.as_ref() {
        triples.push(RdfTriple::new_typed(
            &uri,
            format!("{}approvedAt", SOS_NS),
            approved_at.to_rfc3339(),
            XSD_DATE_TIME,
        ));
    }
    if let Some(rejected_by) = &request.rejected_by {
        triples.push(RdfTriple::new_literal(
            &uri,
            format!("{}rejectedBy", SOS_NS),
            rejected_by,
        ));
    }
    if let Some(rejected_at) = request.rejected_at.as_ref() {
        triples.push(RdfTriple::new_typed(
            &uri,
            format!("{}rejectedAt", SOS_NS),
            rejected_at.to_rfc3339(),
            XSD_DATE_TIME,
        ));
    }
    if let Some(rejection_reason) = &request.rejection_reason {
        triples.push(RdfTriple::new_literal(
            &uri,
            format!("{}rejectionReason", SOS_NS),
            rejection_reason,
        ));
    }
    if let Some(policy_refs) = request
        .metadata
        .get("policy_refs")
        .and_then(|value| value.as_array())
    {
        for policy_ref in policy_refs.iter().filter_map(Value::as_str) {
            triples.push(RdfTriple::new(
                &uri,
                format!("{}governedByPolicy", SOS_NS),
                policy_ref_to_uri(policy_ref),
            ));
        }
    }
    if !request.metadata.is_empty() {
        triples.push(RdfTriple::new_literal(
            &uri,
            format!("{}metadataJson", SOS_NS),
            &serde_json::to_string(&request.metadata)
                .expect("serializing contract approval request metadata should not fail"),
        ));
    }

    triples
}

fn governance_contract_approval_evidence_triples(
    evidence: &ContractApprovalEvidenceRecord,
) -> Vec<RdfTriple> {
    let uri = contract_approval_evidence_uri(
        &evidence.contract_id,
        &evidence.request_id,
        &evidence.evidence_id,
    );
    let mut triples = vec![
        RdfTriple::new(
            &uri,
            format!("{}type", RDF_NS),
            format!("{}ContractApprovalEvidence", SOS_NS),
        ),
        RdfTriple::new(
            &uri,
            format!("{}forApprovalRequest", SOS_NS),
            contract_approval_request_uri(&evidence.contract_id, &evidence.request_id),
        ),
        RdfTriple::new(
            &uri,
            format!("{}forContractRevision", SOS_NS),
            contract_revision_uri(&evidence.contract_id, evidence.contract_revision),
        ),
        RdfTriple::new_literal(
            &uri,
            format!("{}evidenceType", SOS_NS),
            &evidence.evidence_type,
        ),
        RdfTriple::new_literal(&uri, format!("{}reportId", SOS_NS), &evidence.report_id),
        RdfTriple::new(
            &uri,
            format!("{}used", PROV_NS),
            validation_report_uri(&evidence.report_id),
        ),
        RdfTriple::new_literal(&uri, format!("{}addedBy", SOS_NS), &evidence.added_by),
        RdfTriple::new_typed(
            &uri,
            format!("{}addedAt", SOS_NS),
            evidence.added_at.to_rfc3339(),
            XSD_DATE_TIME,
        ),
    ];

    if let Some(note) = &evidence.note {
        triples.push(RdfTriple::new_literal(
            &uri,
            format!("{}note", SOS_NS),
            note,
        ));
    }
    if !evidence.metadata.is_empty() {
        triples.push(RdfTriple::new_literal(
            &uri,
            format!("{}metadataJson", SOS_NS),
            &serde_json::to_string(&evidence.metadata)
                .expect("serializing contract approval evidence metadata should not fail"),
        ));
    }

    triples
}

fn governance_contract_signature_triples(
    contract: &Contract,
    signature: &ContractSignatureRecord,
) -> Vec<RdfTriple> {
    let uri = contract_signature_uri(
        &signature.contract_id,
        signature.contract_revision,
        &signature.signature_id,
    );
    let mut triples = vec![
        RdfTriple::new(
            &uri,
            format!("{}type", RDF_NS),
            format!("{}ContractSignatureAttestation", SOS_NS),
        ),
        RdfTriple::new(
            &uri,
            format!("{}forContract", SOS_NS),
            contract_uri(&signature.contract_id),
        ),
        RdfTriple::new(
            &uri,
            format!("{}forContractRevision", SOS_NS),
            contract_revision_uri(&signature.contract_id, signature.contract_revision),
        ),
        RdfTriple::new_literal(
            &uri,
            format!("{}contractRevisionRef", SOS_NS),
            &signature.contract_revision_ref,
        ),
        RdfTriple::new_literal(
            &uri,
            format!("{}payloadHash", SOS_NS),
            &signature.payload_hash,
        ),
        RdfTriple::new_literal(
            &uri,
            format!("{}payloadHashAlgorithm", SOS_NS),
            &signature.payload_hash_algorithm,
        ),
        RdfTriple::new_literal(
            &uri,
            format!("{}signatureAlgorithm", SOS_NS),
            &signature.signature_algorithm,
        ),
        RdfTriple::new_literal(&uri, format!("{}signature", SOS_NS), &signature.signature),
        RdfTriple::new_literal(&uri, format!("{}publicKey", SOS_NS), &signature.public_key),
        RdfTriple::new_literal(
            &uri,
            format!("{}keyFingerprint", SOS_NS),
            &signature.key_fingerprint,
        ),
        RdfTriple::new_literal(&uri, format!("{}signedBy", SOS_NS), &signature.signed_by),
        RdfTriple::new_typed(
            &uri,
            format!("{}signedAt", SOS_NS),
            signature.signed_at.to_rfc3339(),
            XSD_DATE_TIME,
        ),
        RdfTriple::new_typed(
            &uri,
            format!("{}signatureVerified", SOS_NS),
            verify_contract_signature(contract, signature).to_string(),
            XSD_BOOLEAN,
        ),
    ];

    if let Some(signing_key_ref) = &signature.signing_key_ref {
        triples.push(RdfTriple::new_literal(
            &uri,
            format!("{}signingKeyRef", SOS_NS),
            signing_key_ref,
        ));
    }
    if let Some(signing_key_version) = &signature.signing_key_version {
        triples.push(RdfTriple::new_literal(
            &uri,
            format!("{}signingKeyVersion", SOS_NS),
            signing_key_version,
        ));
    }
    triples.push(RdfTriple::new_literal(
        &uri,
        format!("{}signingKeySource", SOS_NS),
        &signature.signing_key_source,
    ));

    if let Some(approval_request_id) = &signature.approval_request_id {
        triples.push(RdfTriple::new(
            &uri,
            format!("{}forApprovalRequest", SOS_NS),
            contract_approval_request_uri(&signature.contract_id, approval_request_id),
        ));
    }
    for evidence_id in &signature.evidence_ids {
        let evidence_uri = signature
            .approval_request_id
            .as_ref()
            .map(|request_id| {
                contract_approval_evidence_uri(&signature.contract_id, request_id, evidence_id)
            })
            .unwrap_or_else(|| {
                format!(
                    "{}/evidence/{}",
                    contract_uri(&signature.contract_id),
                    evidence_id
                )
            });
        triples.push(RdfTriple::new(
            &uri,
            format!("{}usedApprovalEvidence", SOS_NS),
            evidence_uri,
        ));
    }
    for policy_ref in &signature.policy_refs {
        triples.push(RdfTriple::new(
            &uri,
            format!("{}governedByPolicy", SOS_NS),
            policy_ref_to_uri(policy_ref),
        ));
    }
    if !signature.metadata.is_empty() {
        triples.push(RdfTriple::new_literal(
            &uri,
            format!("{}metadataJson", SOS_NS),
            &serde_json::to_string(&signature.metadata)
                .expect("serializing contract signature metadata should not fail"),
        ));
    }

    triples
}

fn governance_policy_approval_request_triples(
    request: &PolicyApprovalRequestRecord,
) -> Vec<RdfTriple> {
    let uri = policy_approval_request_uri(&request.policy_id, &request.request_id);
    let mut triples = vec![
        RdfTriple::new(
            &uri,
            format!("{}type", RDF_NS),
            format!("{}PolicyApprovalRequest", SOS_NS),
        ),
        RdfTriple::new(
            &uri,
            format!("{}forPolicy", SOS_NS),
            policy_uri(&request.policy_id),
        ),
        RdfTriple::new(
            &uri,
            format!("{}forPolicyRevision", SOS_NS),
            policy_ref_to_uri(&format!(
                "policy:{}@{}",
                request.policy_id, request.policy_revision
            )),
        ),
        RdfTriple::new_literal(
            &uri,
            format!("{}approvalType", SOS_NS),
            &request.approval_type,
        ),
        RdfTriple::new_literal(
            &uri,
            format!("{}requestedLifecycleState", SOS_NS),
            &request.requested_lifecycle_state,
        ),
        RdfTriple::new_literal(&uri, format!("{}status", SOS_NS), &request.status),
        RdfTriple::new_literal(
            &uri,
            format!("{}requestedBy", SOS_NS),
            &request.requested_by,
        ),
        RdfTriple::new_typed(
            &uri,
            format!("{}requestedAt", SOS_NS),
            request.requested_at.to_rfc3339(),
            XSD_DATE_TIME,
        ),
    ];

    if let Some(note) = &request.note {
        triples.push(RdfTriple::new_literal(
            &uri,
            format!("{}note", SOS_NS),
            note,
        ));
    }
    if let Some(expires_at) = request.expires_at.as_ref() {
        triples.push(RdfTriple::new_typed(
            &uri,
            format!("{}expiresAt", SOS_NS),
            expires_at.to_rfc3339(),
            XSD_DATE_TIME,
        ));
    }
    if let Some(approved_by) = &request.approved_by {
        triples.push(RdfTriple::new_literal(
            &uri,
            format!("{}approvedBy", SOS_NS),
            approved_by,
        ));
    }
    if let Some(approved_at) = request.approved_at.as_ref() {
        triples.push(RdfTriple::new_typed(
            &uri,
            format!("{}approvedAt", SOS_NS),
            approved_at.to_rfc3339(),
            XSD_DATE_TIME,
        ));
    }
    if let Some(rejected_by) = &request.rejected_by {
        triples.push(RdfTriple::new_literal(
            &uri,
            format!("{}rejectedBy", SOS_NS),
            rejected_by,
        ));
    }
    if let Some(rejected_at) = request.rejected_at.as_ref() {
        triples.push(RdfTriple::new_typed(
            &uri,
            format!("{}rejectedAt", SOS_NS),
            rejected_at.to_rfc3339(),
            XSD_DATE_TIME,
        ));
    }
    if let Some(rejection_reason) = &request.rejection_reason {
        triples.push(RdfTriple::new_literal(
            &uri,
            format!("{}rejectionReason", SOS_NS),
            rejection_reason,
        ));
    }
    if let Some(policy_refs) = request
        .metadata
        .get("policy_refs")
        .and_then(|value| value.as_array())
    {
        for policy_ref in policy_refs.iter().filter_map(Value::as_str) {
            triples.push(RdfTriple::new(
                &uri,
                format!("{}governedByPolicy", SOS_NS),
                policy_ref_to_uri(policy_ref),
            ));
        }
    }
    if !request.metadata.is_empty() {
        triples.push(RdfTriple::new_literal(
            &uri,
            format!("{}metadataJson", SOS_NS),
            &serde_json::to_string(&request.metadata)
                .expect("serializing policy approval request metadata should not fail"),
        ));
    }

    triples
}

fn governance_policy_approval_evidence_triples(
    evidence: &PolicyApprovalEvidenceRecord,
) -> Vec<RdfTriple> {
    let uri = policy_approval_evidence_uri(
        &evidence.policy_id,
        &evidence.request_id,
        &evidence.evidence_id,
    );
    let mut triples = vec![
        RdfTriple::new(
            &uri,
            format!("{}type", RDF_NS),
            format!("{}PolicyApprovalEvidence", SOS_NS),
        ),
        RdfTriple::new(
            &uri,
            format!("{}forApprovalRequest", SOS_NS),
            policy_approval_request_uri(&evidence.policy_id, &evidence.request_id),
        ),
        RdfTriple::new(
            &uri,
            format!("{}forPolicyRevision", SOS_NS),
            policy_ref_to_uri(&format!(
                "policy:{}@{}",
                evidence.policy_id, evidence.policy_revision
            )),
        ),
        RdfTriple::new_literal(
            &uri,
            format!("{}evidenceType", SOS_NS),
            &evidence.evidence_type,
        ),
        RdfTriple::new_literal(&uri, format!("{}reportId", SOS_NS), &evidence.report_id),
        RdfTriple::new(
            &uri,
            format!("{}used", PROV_NS),
            validation_report_uri(&evidence.report_id),
        ),
        RdfTriple::new_literal(&uri, format!("{}addedBy", SOS_NS), &evidence.added_by),
        RdfTriple::new_typed(
            &uri,
            format!("{}addedAt", SOS_NS),
            evidence.added_at.to_rfc3339(),
            XSD_DATE_TIME,
        ),
    ];

    if let Some(note) = &evidence.note {
        triples.push(RdfTriple::new_literal(
            &uri,
            format!("{}note", SOS_NS),
            note,
        ));
    }
    if !evidence.metadata.is_empty() {
        triples.push(RdfTriple::new_literal(
            &uri,
            format!("{}metadataJson", SOS_NS),
            &serde_json::to_string(&evidence.metadata)
                .expect("serializing policy approval evidence metadata should not fail"),
        ));
    }

    triples
}

fn governance_policy_attestation_triples(
    policy: &SosPolicy,
    attestation: &PolicyAttestationRecord,
) -> Vec<RdfTriple> {
    let uri = policy_attestation_uri(
        &attestation.policy_id,
        attestation.policy_revision,
        &attestation.attestation_id,
    );
    let mut triples = vec![
        RdfTriple::new(
            &uri,
            format!("{}type", RDF_NS),
            format!("{}PolicyApprovalAttestation", SOS_NS),
        ),
        RdfTriple::new(
            &uri,
            format!("{}forPolicy", SOS_NS),
            policy_uri(&attestation.policy_id),
        ),
        RdfTriple::new(
            &uri,
            format!("{}forPolicyRevision", SOS_NS),
            policy_ref_to_uri(&attestation.policy_revision_ref),
        ),
        RdfTriple::new_literal(
            &uri,
            format!("{}policyRevisionRef", SOS_NS),
            &attestation.policy_revision_ref,
        ),
        RdfTriple::new_literal(
            &uri,
            format!("{}payloadHash", SOS_NS),
            &attestation.payload_hash,
        ),
        RdfTriple::new_literal(
            &uri,
            format!("{}payloadHashAlgorithm", SOS_NS),
            &attestation.payload_hash_algorithm,
        ),
        RdfTriple::new_literal(
            &uri,
            format!("{}signatureAlgorithm", SOS_NS),
            &attestation.signature_algorithm,
        ),
        RdfTriple::new_literal(&uri, format!("{}signature", SOS_NS), &attestation.signature),
        RdfTriple::new_literal(
            &uri,
            format!("{}publicKey", SOS_NS),
            &attestation.public_key,
        ),
        RdfTriple::new_literal(
            &uri,
            format!("{}keyFingerprint", SOS_NS),
            &attestation.key_fingerprint,
        ),
        RdfTriple::new_literal(
            &uri,
            format!("{}attestedBy", SOS_NS),
            &attestation.attested_by,
        ),
        RdfTriple::new_typed(
            &uri,
            format!("{}attestedAt", SOS_NS),
            attestation.attested_at.to_rfc3339(),
            XSD_DATE_TIME,
        ),
        RdfTriple::new_typed(
            &uri,
            format!("{}attestationVerified", SOS_NS),
            verify_policy_attestation(policy, attestation).to_string(),
            XSD_BOOLEAN,
        ),
    ];

    if let Some(signing_key_ref) = &attestation.signing_key_ref {
        triples.push(RdfTriple::new_literal(
            &uri,
            format!("{}signingKeyRef", SOS_NS),
            signing_key_ref,
        ));
    }
    if let Some(signing_key_version) = &attestation.signing_key_version {
        triples.push(RdfTriple::new_literal(
            &uri,
            format!("{}signingKeyVersion", SOS_NS),
            signing_key_version,
        ));
    }
    triples.push(RdfTriple::new_literal(
        &uri,
        format!("{}signingKeySource", SOS_NS),
        &attestation.signing_key_source,
    ));

    if let Some(approval_request_id) = &attestation.approval_request_id {
        triples.push(RdfTriple::new(
            &uri,
            format!("{}forApprovalRequest", SOS_NS),
            policy_approval_request_uri(&attestation.policy_id, approval_request_id),
        ));
    }
    for evidence_id in &attestation.evidence_ids {
        let evidence_uri = attestation
            .approval_request_id
            .as_ref()
            .map(|request_id| {
                policy_approval_evidence_uri(&attestation.policy_id, request_id, evidence_id)
            })
            .unwrap_or_else(|| {
                format!(
                    "{}/evidence/{}",
                    policy_uri(&attestation.policy_id),
                    evidence_id
                )
            });
        triples.push(RdfTriple::new(
            &uri,
            format!("{}usedApprovalEvidence", SOS_NS),
            evidence_uri,
        ));
    }
    for policy_ref in &attestation.policy_refs {
        triples.push(RdfTriple::new(
            &uri,
            format!("{}governedByPolicy", SOS_NS),
            policy_ref_to_uri(policy_ref),
        ));
    }
    if let Some(trust_mode) = metadata_string(
        &attestation.metadata,
        POLICY_ATTESTATION_TRUST_MODE_METADATA_KEY,
    ) {
        triples.push(RdfTriple::new_literal(
            &uri,
            format!("{}trustMode", SOS_NS),
            trust_mode,
        ));
    }
    if let Some(trust_provider) = metadata_string(
        &attestation.metadata,
        POLICY_ATTESTATION_TRUST_PROVIDER_METADATA_KEY,
    ) {
        triples.push(RdfTriple::new_literal(
            &uri,
            format!("{}trustProvider", SOS_NS),
            trust_provider,
        ));
    }
    if let Some(external_key_ref) = metadata_string(
        &attestation.metadata,
        POLICY_ATTESTATION_EXTERNAL_KEY_REF_METADATA_KEY,
    ) {
        triples.push(RdfTriple::new_literal(
            &uri,
            format!("{}externalKeyRef", SOS_NS),
            external_key_ref,
        ));
    }
    if let Some(trust_attestation_ref) = metadata_string(
        &attestation.metadata,
        POLICY_ATTESTATION_TRUST_ATTESTATION_REF_METADATA_KEY,
    ) {
        triples.push(RdfTriple::new_literal(
            &uri,
            format!("{}trustAttestationRef", SOS_NS),
            trust_attestation_ref,
        ));
    }
    if !attestation.metadata.is_empty() {
        triples.push(RdfTriple::new_literal(
            &uri,
            format!("{}metadataJson", SOS_NS),
            &serde_json::to_string(&attestation.metadata)
                .expect("serializing policy attestation metadata should not fail"),
        ));
    }

    triples
}

fn catalog_policy_latest_triples(policy: &SosPolicy) -> Vec<RdfTriple> {
    let uri = policy_uri(&policy.policy_id);
    let revision_uri = policy_revision_uri(policy);
    let lifecycle_state = policy_lifecycle_state(policy);
    let approval_status = policy_approval_status(policy);
    let mut triples = vec![
        RdfTriple::new(&uri, format!("{}type", RDF_NS), format!("{}Policy", SOS_NS)),
        RdfTriple::new_literal(&uri, format!("{}policyName", SOS_NS), &policy.policy_name),
        RdfTriple::new_literal(&uri, format!("{}lifecycleState", SOS_NS), lifecycle_state),
        RdfTriple::new_literal(&uri, format!("{}approvalStatus", SOS_NS), approval_status),
        RdfTriple::new_literal(&uri, format!("{}targetType", SOS_NS), &policy.target_type),
        RdfTriple::new_literal(
            &uri,
            format!("{}enforcementLevel", SOS_NS),
            &policy.enforcement_level,
        ),
        RdfTriple::new_literal(&uri, format!("{}severity", SOS_NS), &policy.severity),
        RdfTriple::new_typed(
            &uri,
            format!("{}active", SOS_NS),
            policy_is_automatic(policy).to_string(),
            XSD_BOOLEAN,
        ),
        RdfTriple::new_typed(
            &uri,
            format!("{}createdAt", SOS_NS),
            policy.created_at.to_rfc3339(),
            XSD_DATE_TIME,
        ),
        RdfTriple::new_typed(
            &uri,
            format!("{}updatedAt", SOS_NS),
            policy.updated_at.to_rfc3339(),
            XSD_DATE_TIME,
        ),
        RdfTriple::new_typed(
            &uri,
            format!("{}revision", SOS_NS),
            policy.revision.to_string(),
            "http://www.w3.org/2001/XMLSchema#integer",
        ),
        RdfTriple::new_literal(&uri, format!("{}createdBy", SOS_NS), &policy.created_by),
        RdfTriple::new_literal(&uri, format!("{}updatedBy", SOS_NS), &policy.updated_by),
        RdfTriple::new(&uri, format!("{}currentRevision", SOS_NS), revision_uri),
    ];

    if let Some(description) = &policy.description {
        triples.push(RdfTriple::new_literal(
            &uri,
            format!("{}description", SOS_NS),
            description,
        ));
    }
    if let Some(approval_requested_by) = &policy.approval_requested_by {
        triples.push(RdfTriple::new_literal(
            &uri,
            format!("{}approvalRequestedBy", SOS_NS),
            approval_requested_by,
        ));
    }
    if let Some(approval_requested_at) = policy.approval_requested_at.as_ref() {
        triples.push(RdfTriple::new_typed(
            &uri,
            format!("{}approvalRequestedAt", SOS_NS),
            approval_requested_at.to_rfc3339(),
            XSD_DATE_TIME,
        ));
    }
    if let Some(approved_by) = &policy.approved_by {
        triples.push(RdfTriple::new_literal(
            &uri,
            format!("{}approvedBy", SOS_NS),
            approved_by,
        ));
    }
    if let Some(approved_at) = policy.approved_at.as_ref() {
        triples.push(RdfTriple::new_typed(
            &uri,
            format!("{}approvedAt", SOS_NS),
            approved_at.to_rfc3339(),
            XSD_DATE_TIME,
        ));
    }
    if let Some(rejected_by) = &policy.rejected_by {
        triples.push(RdfTriple::new_literal(
            &uri,
            format!("{}rejectedBy", SOS_NS),
            rejected_by,
        ));
    }
    if let Some(rejected_at) = policy.rejected_at.as_ref() {
        triples.push(RdfTriple::new_typed(
            &uri,
            format!("{}rejectedAt", SOS_NS),
            rejected_at.to_rfc3339(),
            XSD_DATE_TIME,
        ));
    }
    if let Some(rejection_reason) = &policy.rejection_reason {
        triples.push(RdfTriple::new_literal(
            &uri,
            format!("{}rejectionReason", SOS_NS),
            rejection_reason,
        ));
    }
    if let Some(target_key) = &policy.target_key {
        triples.push(RdfTriple::new_literal(
            &uri,
            format!("{}targetKey", SOS_NS),
            target_key,
        ));
    }
    for stage in &policy.stages {
        triples.push(RdfTriple::new_literal(
            &uri,
            format!("{}stage", SOS_NS),
            stage,
        ));
    }
    for ontology_ref in &policy.ontology_refs {
        triples.push(RdfTriple::new(
            &uri,
            format!("{}usesOntology", SOS_NS),
            ontology_ref_to_uri(ontology_ref),
        ));
    }
    for shape_ref in &policy.shape_refs {
        triples.push(RdfTriple::new(
            &uri,
            format!("{}usesShape", SOS_NS),
            shape_ref_to_uri(shape_ref),
        ));
    }
    for subject_uri in subject_resource_uris(
        &policy.target_type,
        policy.target_key.as_deref().unwrap_or(""),
    ) {
        triples.push(RdfTriple::new(
            &uri,
            format!("{}appliesTo", SOS_NS),
            subject_uri,
        ));
    }

    triples
}

fn catalog_policy_revision_triples(policy: &SosPolicy) -> Vec<RdfTriple> {
    let latest_uri = policy_ref_to_uri(&format!("policy:{}", policy.policy_id));
    let revision_uri = policy_revision_uri(policy);
    let lifecycle_state = policy_lifecycle_state(policy);
    let approval_status = policy_approval_status(policy);
    let mut triples = vec![
        RdfTriple::new(
            &revision_uri,
            format!("{}type", RDF_NS),
            format!("{}PolicyRevision", SOS_NS),
        ),
        RdfTriple::new(
            &revision_uri,
            format!("{}specializationOf", PROV_NS),
            latest_uri,
        ),
        RdfTriple::new_literal(
            &revision_uri,
            format!("{}policyName", SOS_NS),
            &policy.policy_name,
        ),
        RdfTriple::new_literal(
            &revision_uri,
            format!("{}lifecycleState", SOS_NS),
            lifecycle_state,
        ),
        RdfTriple::new_literal(
            &revision_uri,
            format!("{}approvalStatus", SOS_NS),
            approval_status,
        ),
        RdfTriple::new_literal(
            &revision_uri,
            format!("{}targetType", SOS_NS),
            &policy.target_type,
        ),
        RdfTriple::new_literal(
            &revision_uri,
            format!("{}enforcementLevel", SOS_NS),
            &policy.enforcement_level,
        ),
        RdfTriple::new_literal(
            &revision_uri,
            format!("{}severity", SOS_NS),
            &policy.severity,
        ),
        RdfTriple::new_typed(
            &revision_uri,
            format!("{}active", SOS_NS),
            policy_is_automatic(policy).to_string(),
            XSD_BOOLEAN,
        ),
        RdfTriple::new_typed(
            &revision_uri,
            format!("{}revision", SOS_NS),
            policy.revision.to_string(),
            "http://www.w3.org/2001/XMLSchema#integer",
        ),
        RdfTriple::new_literal(
            &revision_uri,
            format!("{}createdBy", SOS_NS),
            &policy.created_by,
        ),
        RdfTriple::new_literal(
            &revision_uri,
            format!("{}updatedBy", SOS_NS),
            &policy.updated_by,
        ),
        RdfTriple::new_typed(
            &revision_uri,
            format!("{}createdAt", SOS_NS),
            policy.created_at.to_rfc3339(),
            XSD_DATE_TIME,
        ),
        RdfTriple::new_typed(
            &revision_uri,
            format!("{}updatedAt", SOS_NS),
            policy.updated_at.to_rfc3339(),
            XSD_DATE_TIME,
        ),
    ];

    if policy.revision > 1 {
        triples.push(RdfTriple::new(
            &revision_uri,
            format!("{}wasRevisionOf", PROV_NS),
            policy_ref_to_uri(&format!(
                "policy:{}@{}",
                policy.policy_id,
                policy.revision - 1
            )),
        ));
    }
    if let Some(superseded_by_revision) = policy.superseded_by_revision {
        triples.push(RdfTriple::new(
            &revision_uri,
            format!("{}supersededBy", SOS_NS),
            policy_ref_to_uri(&format!(
                "policy:{}@{}",
                policy.policy_id, superseded_by_revision
            )),
        ));
    }
    if let Some(description) = &policy.description {
        triples.push(RdfTriple::new_literal(
            &revision_uri,
            format!("{}description", SOS_NS),
            description,
        ));
    }
    if let Some(approval_requested_by) = &policy.approval_requested_by {
        triples.push(RdfTriple::new_literal(
            &revision_uri,
            format!("{}approvalRequestedBy", SOS_NS),
            approval_requested_by,
        ));
    }
    if let Some(approval_requested_at) = policy.approval_requested_at.as_ref() {
        triples.push(RdfTriple::new_typed(
            &revision_uri,
            format!("{}approvalRequestedAt", SOS_NS),
            approval_requested_at.to_rfc3339(),
            XSD_DATE_TIME,
        ));
    }
    if let Some(approved_by) = &policy.approved_by {
        triples.push(RdfTriple::new_literal(
            &revision_uri,
            format!("{}approvedBy", SOS_NS),
            approved_by,
        ));
    }
    if let Some(approved_at) = policy.approved_at.as_ref() {
        triples.push(RdfTriple::new_typed(
            &revision_uri,
            format!("{}approvedAt", SOS_NS),
            approved_at.to_rfc3339(),
            XSD_DATE_TIME,
        ));
    }
    if let Some(rejected_by) = &policy.rejected_by {
        triples.push(RdfTriple::new_literal(
            &revision_uri,
            format!("{}rejectedBy", SOS_NS),
            rejected_by,
        ));
    }
    if let Some(rejected_at) = policy.rejected_at.as_ref() {
        triples.push(RdfTriple::new_typed(
            &revision_uri,
            format!("{}rejectedAt", SOS_NS),
            rejected_at.to_rfc3339(),
            XSD_DATE_TIME,
        ));
    }
    if let Some(rejection_reason) = &policy.rejection_reason {
        triples.push(RdfTriple::new_literal(
            &revision_uri,
            format!("{}rejectionReason", SOS_NS),
            rejection_reason,
        ));
    }
    if let Some(target_key) = &policy.target_key {
        triples.push(RdfTriple::new_literal(
            &revision_uri,
            format!("{}targetKey", SOS_NS),
            target_key,
        ));
    }
    for stage in &policy.stages {
        triples.push(RdfTriple::new_literal(
            &revision_uri,
            format!("{}stage", SOS_NS),
            stage,
        ));
    }
    for ontology_ref in &policy.ontology_refs {
        triples.push(RdfTriple::new(
            &revision_uri,
            format!("{}usesOntology", SOS_NS),
            ontology_ref_to_uri(ontology_ref),
        ));
    }
    for shape_ref in &policy.shape_refs {
        triples.push(RdfTriple::new(
            &revision_uri,
            format!("{}usesShape", SOS_NS),
            shape_ref_to_uri(shape_ref),
        ));
    }
    for subject_uri in subject_resource_uris(
        &policy.target_type,
        policy.target_key.as_deref().unwrap_or(""),
    ) {
        triples.push(RdfTriple::new(
            &revision_uri,
            format!("{}appliesTo", SOS_NS),
            subject_uri,
        ));
    }

    triples
}

fn validation_report_triples(report: &ValidationReport) -> Vec<RdfTriple> {
    let activity_uri = validation_activity_uri(&report.validation_id);
    let report_uri = validation_report_uri(&report.report_id);
    let mut triples = vec![
        RdfTriple::new(
            &activity_uri,
            format!("{}type", RDF_NS),
            format!("{}Activity", PROV_NS),
        ),
        RdfTriple::new(
            &report_uri,
            format!("{}type", RDF_NS),
            format!("{}Entity", PROV_NS),
        ),
        RdfTriple::new(&activity_uri, format!("{}generated", PROV_NS), &report_uri),
        RdfTriple::new_literal(
            &report_uri,
            format!("{}subjectType", SOS_NS),
            &report.subject_type,
        ),
        RdfTriple::new_literal(
            &report_uri,
            format!("{}subjectKey", SOS_NS),
            &report.subject_key,
        ),
        RdfTriple::new_literal(
            &report_uri,
            format!("{}validationType", SOS_NS),
            &report.validation_type,
        ),
        RdfTriple::new_typed(
            &report_uri,
            format!("{}passed", SOS_NS),
            report.passed.to_string(),
            XSD_BOOLEAN,
        ),
        RdfTriple::new_typed(
            &report_uri,
            format!("{}confidence", SOS_NS),
            report.confidence.to_string(),
            XSD_DOUBLE,
        ),
        RdfTriple::new_typed(
            &activity_uri,
            format!("{}endedAtTime", PROV_NS),
            report.validated_at.to_rfc3339(),
            XSD_DATE_TIME,
        ),
    ];

    for used_uri in subject_resource_uris(&report.subject_type, &report.subject_key) {
        triples.push(RdfTriple::new(
            &activity_uri,
            format!("{}used", PROV_NS),
            used_uri,
        ));
    }

    for ontology_ref in &report.ontology_refs {
        triples.push(RdfTriple::new(
            &activity_uri,
            format!("{}used", PROV_NS),
            ontology_ref_to_uri(ontology_ref),
        ));
    }

    for shape_ref in &report.shape_refs {
        triples.push(RdfTriple::new(
            &activity_uri,
            format!("{}used", PROV_NS),
            shape_ref_to_uri(shape_ref),
        ));
    }

    for policy_ref in &report.policy_refs {
        triples.push(RdfTriple::new(
            &activity_uri,
            format!("{}used", PROV_NS),
            policy_ref_to_uri(policy_ref),
        ));
    }

    for contract_ref in &report.contract_refs {
        triples.push(RdfTriple::new(
            &activity_uri,
            format!("{}used", PROV_NS),
            contract_ref_to_uri(contract_ref),
        ));
    }

    if let Some(previous_report_id) = &report.previous_report_id {
        triples.push(RdfTriple::new(
            &report_uri,
            format!("{}wasRevisionOf", PROV_NS),
            validation_report_uri(previous_report_id),
        ));
    }

    if let (Some(workflow_execution_id), Some(workflow_step_id)) =
        (&report.workflow_execution_id, &report.workflow_step_id)
    {
        triples.push(RdfTriple::new(
            &activity_uri,
            format!("{}wasInformedBy", PROV_NS),
            workflow_step_uri(workflow_execution_id, workflow_step_id),
        ));
    }

    triples
}
