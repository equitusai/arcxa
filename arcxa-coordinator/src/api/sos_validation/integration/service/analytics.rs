use super::*;
use crate::api::sos_validation::contract_governance::{
    effective_contract_lifecycle_state, CONTRACT_LIFECYCLE_DRAFT,
};
use crate::api::sos_validation::types::{
    CompatibilityMatrixMetadata, DependencyGraphEdge, DependencyGraphMetadata, DependencyGraphNode,
    DependencyGraphQuery, DependencyGraphResponse, WhatIfAnalysisMetadata,
};

const DEFAULT_COMPATIBILITY_MATRIX_MAX_PAIRS: usize = 10_000;
const DEFAULT_DEPENDENCY_GRAPH_MAX_NODES: usize = 10_000;
const DEFAULT_DEPENDENCY_GRAPH_MAX_EDGES: usize = 20_000;
const DEFAULT_WHAT_IF_MAX_EVALUATIONS: usize = 1_000;

#[derive(Debug, Clone, Copy)]
struct CompatibilityMatrixBudget {
    requested: Option<usize>,
    applied: usize,
    server_cap: usize,
}

#[derive(Debug, Clone, Copy)]
struct DependencyGraphBudget {
    requested_nodes: Option<usize>,
    applied_nodes: usize,
    server_node_cap: usize,
    requested_edges: Option<usize>,
    applied_edges: usize,
    server_edge_cap: usize,
}

#[derive(Debug, Clone, Copy)]
struct WhatIfBudget {
    requested: Option<usize>,
    applied: usize,
    server_cap: usize,
}

pub(super) fn build_compatibility_matrix(
    service: &SosValidationService,
    requested_budget: Option<usize>,
) -> Result<CompatibilityMatrixResponse, SosValidationServiceError> {
    let budget = compatibility_matrix_budget(requested_budget)?;
    let mut interfaces = service
        .storage_manager
        .list_all_interfaces(0, usize::MAX)
        .map_err(map_storage_error)?;
    interfaces.sort_by(|left, right| left.interface_id.cmp(&right.interface_id));

    let total_interfaces = interfaces.len();
    let total_candidate_pairs = ordered_candidate_pair_count(&interfaces);
    let mut matrix = Vec::with_capacity(total_candidate_pairs.min(budget.applied));
    let mut evaluated_pairs = 0usize;

    'provider_loop: for provider in &interfaces {
        for consumer in &interfaces {
            if evaluated_pairs >= budget.applied {
                break 'provider_loop;
            }

            if provider.interface_id == consumer.interface_id
                || provider.system_id == consumer.system_id
            {
                continue;
            }

            let contract = super::lookup::find_contract_between(
                service,
                &provider.interface_id,
                &consumer.interface_id,
            )?;
            let subject_key = format!(
                "interface_pair:{}:{}",
                provider.interface_id, consumer.interface_id
            );
            let expected_hashes = service.current_interface_pair_schema_hashes(
                provider,
                consumer,
                contract.as_ref(),
            )?;
            let latest_report = service
                .storage_manager
                .get_latest_validation_report(&subject_key)
                .map_err(map_storage_error)?;

            if let Some(report) = latest_report.filter(|report| {
                report.validation_type == "interface_compatibility"
                    && report.schema_hashes == expected_hashes
            }) {
                matrix.push(CompatibilityScore {
                    provider_interface_id: provider.interface_id.clone(),
                    consumer_interface_id: consumer.interface_id.clone(),
                    score: report.confidence,
                    compatibility_state: super::derive_interface_compatibility_state(&report.checks),
                    details: report
                        .checks
                        .iter()
                        .map(|check| CompatibilityDetail {
                            aspect: check.check_name.clone(),
                            compatible: check.passed,
                            explanation: check.description.clone(),
                        })
                        .collect(),
                });
                evaluated_pairs += 1;
                continue;
            }

            let execution =
                service.validate_interface_pair(provider, consumer, contract.as_ref())?;
            matrix.push(CompatibilityScore {
                provider_interface_id: provider.interface_id.clone(),
                consumer_interface_id: consumer.interface_id.clone(),
                score: execution.confidence,
                compatibility_state: super::derive_interface_compatibility_state(&execution.checks),
                details: execution
                    .checks
                    .iter()
                    .map(|check| CompatibilityDetail {
                        aspect: check.check_name.clone(),
                        compatible: check.passed,
                        explanation: check.description.clone(),
                    })
                    .collect(),
            });
            evaluated_pairs += 1;
        }
    }

    let remaining_candidate_pairs = total_candidate_pairs.saturating_sub(evaluated_pairs);
    let truncated = remaining_candidate_pairs > 0;
    if truncated {
        tracing::warn!(
            total_interfaces,
            total_candidate_pairs,
            evaluated_pairs,
            remaining_candidate_pairs,
            requested_evaluation_budget = ?budget.requested,
            applied_evaluation_budget = budget.applied,
            server_evaluation_budget = budget.server_cap,
            "compatibility matrix response truncated by evaluation budget"
        );
    }

    Ok(CompatibilityMatrixResponse {
        matrix,
        metadata: CompatibilityMatrixMetadata {
            total_interfaces,
            total_candidate_pairs,
            evaluated_pairs,
            remaining_candidate_pairs,
            truncated,
            requested_evaluation_budget: budget.requested,
            applied_evaluation_budget: budget.applied,
            server_evaluation_budget: budget.server_cap,
        },
        generated_at: Utc::now().to_rfc3339(),
    })
}

fn compatibility_matrix_budget(
    requested_budget: Option<usize>,
) -> Result<CompatibilityMatrixBudget, SosValidationServiceError> {
    if matches!(requested_budget, Some(0)) {
        return Err(SosValidationServiceError::InvalidRequest(
            "compatibility matrix evaluation_budget must be greater than zero".to_string(),
        ));
    }

    let server_cap = std::env::var("SOS_COMPATIBILITY_MATRIX_MAX_PAIRS")
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_COMPATIBILITY_MATRIX_MAX_PAIRS);

    let applied = requested_budget.unwrap_or(server_cap).min(server_cap);

    Ok(CompatibilityMatrixBudget {
        requested: requested_budget,
        applied,
        server_cap,
    })
}

fn ordered_candidate_pair_count(interfaces: &[Interface]) -> usize {
    let total_interfaces = interfaces.len();
    if total_interfaces < 2 {
        return 0;
    }

    let mut interfaces_per_system: HashMap<String, usize> = HashMap::new();
    for provider in interfaces {
        *interfaces_per_system
            .entry(provider.system_id.clone())
            .or_insert(0) += 1;
    }

    let total_ordered_pairs = total_interfaces * (total_interfaces - 1);
    let same_system_pairs: usize = interfaces_per_system
        .into_values()
        .map(|count| count * count.saturating_sub(1))
        .sum();

    total_ordered_pairs.saturating_sub(same_system_pairs)
}

pub(super) fn build_dependency_graph(
    service: &SosValidationService,
    query: DependencyGraphQuery,
) -> Result<DependencyGraphResponse, SosValidationServiceError> {
    let budget = dependency_graph_budget(query.node_budget, query.edge_budget)?;
    let mut systems = service
        .storage_manager
        .list_all_systems(0, usize::MAX)
        .map_err(map_storage_error)?;
    systems.sort_by(|left, right| left.system_id.cmp(&right.system_id));
    let mut interfaces = service
        .storage_manager
        .list_all_interfaces(0, usize::MAX)
        .map_err(map_storage_error)?;
    interfaces.sort_by(|left, right| left.interface_id.cmp(&right.interface_id));
    let mut contracts = service
        .storage_manager
        .list_all_contracts(0, usize::MAX)
        .map_err(map_storage_error)?;
    contracts.sort_by(|left, right| left.contract_id.cmp(&right.contract_id));

    let mut all_nodes = Vec::new();
    let mut all_edges = Vec::new();
    let interface_by_id: HashMap<_, _> = interfaces
        .iter()
        .map(|interface| (interface.interface_id.clone(), interface.clone()))
        .collect();

    for system in &systems {
        all_nodes.push(DependencyGraphNode {
            id: system.system_id.clone(),
            kind: "system".to_string(),
            label: system.system_name.clone(),
            system_type: Some(system.system_type.clone()),
            system_id: None,
        });
    }

    for interface in &interfaces {
        all_nodes.push(DependencyGraphNode {
            id: interface.interface_id.clone(),
            kind: "interface".to_string(),
            label: interface.interface_name.clone(),
            system_type: None,
            system_id: Some(interface.system_id.clone()),
        });
        all_edges.push(DependencyGraphEdge {
            from: interface.system_id.clone(),
            to: interface.interface_id.clone(),
            kind: "exposes".to_string(),
            contract_id: None,
        });
    }

    for contract in &contracts {
        all_nodes.push(DependencyGraphNode {
            id: contract.contract_id.clone(),
            kind: "contract".to_string(),
            label: contract.contract_name.clone(),
            system_type: None,
            system_id: None,
        });
        all_edges.push(DependencyGraphEdge {
            from: contract.provider_interface_id.clone(),
            to: contract.contract_id.clone(),
            kind: "governs_provider".to_string(),
            contract_id: None,
        });
        all_edges.push(DependencyGraphEdge {
            from: contract.contract_id.clone(),
            to: contract.consumer_interface_id.clone(),
            kind: "governs_consumer".to_string(),
            contract_id: None,
        });

        if let (Some(provider), Some(consumer)) = (
            interface_by_id.get(&contract.provider_interface_id),
            interface_by_id.get(&contract.consumer_interface_id),
        ) {
            all_edges.push(DependencyGraphEdge {
                from: provider.system_id.clone(),
                to: consumer.system_id.clone(),
                kind: "integrates_with".to_string(),
                contract_id: Some(contract.contract_id.clone()),
            });
        }
    }

    let total_nodes = all_nodes.len();
    let total_edges = all_edges.len();
    let returned_nodes = total_nodes.min(budget.applied_nodes);
    let returned_edges = total_edges.min(budget.applied_edges);
    let remaining_nodes = total_nodes.saturating_sub(returned_nodes);
    let remaining_edges = total_edges.saturating_sub(returned_edges);
    let truncated = remaining_nodes > 0 || remaining_edges > 0;

    if truncated {
        tracing::warn!(
            total_nodes,
            total_edges,
            returned_nodes,
            returned_edges,
            remaining_nodes,
            remaining_edges,
            requested_node_budget = ?budget.requested_nodes,
            applied_node_budget = budget.applied_nodes,
            server_node_budget = budget.server_node_cap,
            requested_edge_budget = ?budget.requested_edges,
            applied_edge_budget = budget.applied_edges,
            server_edge_budget = budget.server_edge_cap,
            "dependency graph response truncated by node/edge budgets"
        );
    }

    Ok(DependencyGraphResponse {
        nodes: all_nodes.into_iter().take(returned_nodes).collect(),
        edges: all_edges.into_iter().take(returned_edges).collect(),
        metadata: DependencyGraphMetadata {
            total_nodes,
            total_edges,
            returned_nodes,
            returned_edges,
            remaining_nodes,
            remaining_edges,
            truncated,
            requested_node_budget: budget.requested_nodes,
            applied_node_budget: budget.applied_nodes,
            server_node_budget: budget.server_node_cap,
            requested_edge_budget: budget.requested_edges,
            applied_edge_budget: budget.applied_edges,
            server_edge_budget: budget.server_edge_cap,
        },
        generated_at: Utc::now().to_rfc3339(),
    })
}

pub(super) fn run_what_if_analysis(
    service: &SosValidationService,
    request: WhatIfRequest,
) -> Result<WhatIfResponse, SosValidationServiceError> {
    let budget = what_if_budget(request.evaluation_budget)?;
    let current_systems = service
        .storage_manager
        .list_all_systems(0, usize::MAX)
        .map_err(map_storage_error)?;
    let current_interfaces = service
        .storage_manager
        .list_all_interfaces(0, usize::MAX)
        .map_err(map_storage_error)?;
    let current_contracts = service
        .storage_manager
        .list_all_contracts(0, usize::MAX)
        .map_err(map_storage_error)?;

    let current_system_map: HashMap<_, _> = current_systems
        .into_iter()
        .map(|system| (system.system_id.clone(), system))
        .collect();
    let current_interface_map: HashMap<_, _> = current_interfaces
        .into_iter()
        .map(|interface| (interface.interface_id.clone(), interface))
        .collect();
    let current_contract_map: HashMap<_, _> = current_contracts
        .into_iter()
        .map(|contract| (contract.contract_id.clone(), contract))
        .collect();

    let mut projected_systems = current_system_map.clone();
    let mut projected_interfaces = current_interface_map.clone();
    let mut projected_contracts = current_contract_map.clone();

    let mut affected_system_ids = HashSet::new();
    let mut affected_interface_ids = HashSet::new();
    let mut affected_contract_ids = HashSet::new();
    let mut impact = Vec::new();
    let mut recommendations = Vec::new();

    for change in &request.changes {
        apply_what_if_change(
            service,
            change,
            &mut projected_systems,
            &mut projected_interfaces,
            &mut projected_contracts,
            &mut affected_system_ids,
            &mut affected_interface_ids,
            &mut affected_contract_ids,
            &mut impact,
            &mut recommendations,
        )?;
    }

    let mut interface_universe = current_interface_map.clone();
    interface_universe.extend(projected_interfaces.clone());
    let mut system_universe = current_system_map.clone();
    system_universe.extend(projected_systems.clone());

    let mut interface_pairs = HashSet::new();
    for interface_id in &affected_interface_ids {
        if let Some(changed_interface) = interface_universe.get(interface_id) {
            for other_interface in interface_universe.values() {
                if other_interface.interface_id == changed_interface.interface_id
                    || other_interface.system_id == changed_interface.system_id
                {
                    continue;
                }

                interface_pairs.insert((
                    changed_interface.interface_id.clone(),
                    other_interface.interface_id.clone(),
                ));
                interface_pairs.insert((
                    other_interface.interface_id.clone(),
                    changed_interface.interface_id.clone(),
                ));
            }
        }
    }

    let mut system_pairs = HashSet::new();
    for (provider_interface_id, consumer_interface_id) in &interface_pairs {
        if let (Some(provider), Some(consumer)) = (
            interface_universe.get(provider_interface_id),
            interface_universe.get(consumer_interface_id),
        ) {
            if provider.system_id != consumer.system_id {
                system_pairs.insert((provider.system_id.clone(), consumer.system_id.clone()));
            }
        }
    }

    for contract_id in &affected_contract_ids {
        if let Some(contract) = current_contract_map
            .get(contract_id)
            .or_else(|| projected_contracts.get(contract_id))
        {
            interface_pairs.insert((
                contract.provider_interface_id.clone(),
                contract.consumer_interface_id.clone(),
            ));

            if let (Some(provider), Some(consumer)) = (
                interface_universe.get(&contract.provider_interface_id),
                interface_universe.get(&contract.consumer_interface_id),
            ) {
                if provider.system_id != consumer.system_id {
                    system_pairs.insert((provider.system_id.clone(), consumer.system_id.clone()));
                }
            }
        }
    }

    for system_id in &affected_system_ids {
        for other_system in system_universe.values() {
            if other_system.system_id != *system_id {
                system_pairs.insert((system_id.clone(), other_system.system_id.clone()));
            }
        }
    }

    let mut ordered_contract_ids: Vec<_> = affected_contract_ids.iter().cloned().collect();
    ordered_contract_ids.sort();

    let mut ordered_interface_pairs: Vec<_> = interface_pairs.into_iter().collect();
    ordered_interface_pairs.sort();

    let mut ordered_system_pairs: Vec<_> = system_pairs.into_iter().collect();
    ordered_system_pairs.sort();

    let total_candidate_evaluations =
        ordered_contract_ids.len() + ordered_interface_pairs.len() + ordered_system_pairs.len();
    let mut evaluated_candidate_evaluations = 0usize;

    for contract_id in &ordered_contract_ids {
        if evaluated_candidate_evaluations >= budget.applied {
            break;
        }

        let current_execution = validate_contract_compliance_with_catalog(
            service,
            contract_id,
            &current_contract_map,
            &current_interface_map,
        )?;
        let projected_execution = validate_contract_compliance_with_catalog(
            service,
            contract_id,
            &projected_contracts,
            &projected_interfaces,
        )?;

        if let Some(message) = build_execution_delta_message(
            &format!("Contract compliance '{}'", contract_id),
            current_execution.as_ref(),
            projected_execution.as_ref(),
        ) {
            impact.push(message);
        }

        if let Some(execution) = projected_execution.as_ref() {
            append_execution_recommendations(
                &format!("contract '{}'", contract_id),
                execution,
                &mut recommendations,
            );
        }

        evaluated_candidate_evaluations += 1;
    }

    for (provider_interface_id, consumer_interface_id) in &ordered_interface_pairs {
        if evaluated_candidate_evaluations >= budget.applied {
            break;
        }

        let current_execution = validate_interface_pair_with_catalog(
            service,
            provider_interface_id,
            consumer_interface_id,
            &current_interface_map,
            &current_contract_map,
        )?;
        let projected_execution = validate_interface_pair_with_catalog(
            service,
            provider_interface_id,
            consumer_interface_id,
            &projected_interfaces,
            &projected_contracts,
        )?;

        if let Some(message) = build_execution_delta_message(
            &format!(
                "Interface compatibility '{} -> {}'",
                provider_interface_id, consumer_interface_id
            ),
            current_execution.as_ref(),
            projected_execution.as_ref(),
        ) {
            impact.push(message);
        }

        if let Some(execution) = projected_execution.as_ref() {
            append_execution_recommendations(
                &format!(
                    "interface pair '{} -> {}'",
                    provider_interface_id, consumer_interface_id
                ),
                execution,
                &mut recommendations,
            );
        }

        evaluated_candidate_evaluations += 1;
    }

    for (source_system_id, target_system_id) in &ordered_system_pairs {
        if evaluated_candidate_evaluations >= budget.applied {
            break;
        }

        let current_execution = validate_system_integration_with_catalog(
            service,
            source_system_id,
            target_system_id,
            &current_system_map,
            &current_interface_map,
            &current_contract_map,
        )?;
        let projected_execution = validate_system_integration_with_catalog(
            service,
            source_system_id,
            target_system_id,
            &projected_systems,
            &projected_interfaces,
            &projected_contracts,
        )?;

        if let Some(message) = build_execution_delta_message(
            &format!(
                "System integration '{} -> {}'",
                source_system_id, target_system_id
            ),
            current_execution.as_ref(),
            projected_execution.as_ref(),
        ) {
            impact.push(message);
        }

        if let Some(execution) = projected_execution.as_ref() {
            append_execution_recommendations(
                &format!("system pair '{} -> {}'", source_system_id, target_system_id),
                execution,
                &mut recommendations,
            );
        }

        evaluated_candidate_evaluations += 1;
    }

    let mut affected_entities: Vec<String> = affected_system_ids
        .into_iter()
        .chain(affected_interface_ids)
        .chain(affected_contract_ids)
        .collect();
    affected_entities.sort();
    affected_entities.dedup();

    if impact.is_empty() {
        impact.push(
            "The hypothetical changes did not materially alter any SoS compatibility, contract, or system-integration outcomes".to_string(),
        );
    }

    recommendations.sort();
    recommendations.dedup();

    if recommendations.is_empty() {
        recommendations
            .push("No corrective actions are required for the projected state".to_string());
    }

    let remaining_candidate_evaluations =
        total_candidate_evaluations.saturating_sub(evaluated_candidate_evaluations);
    let truncated = remaining_candidate_evaluations > 0;
    if truncated {
        impact.push(format!(
            "What-if analysis evaluated {} of {} candidate checks; rerun with a higher evaluation budget for a complete result",
            evaluated_candidate_evaluations, total_candidate_evaluations
        ));
        recommendations.push(
            "Increase the what-if evaluation budget or narrow the scenario scope to inspect the full affected surface".to_string(),
        );
        tracing::warn!(
            total_candidate_evaluations,
            evaluated_candidate_evaluations,
            remaining_candidate_evaluations,
            requested_evaluation_budget = ?budget.requested,
            applied_evaluation_budget = budget.applied,
            server_evaluation_budget = budget.server_cap,
            "what-if response truncated by evaluation budget"
        );
    }

    Ok(WhatIfResponse {
        scenario_id: Uuid::new_v4().to_string(),
        impact,
        affected_entities,
        recommendations,
        metadata: WhatIfAnalysisMetadata {
            total_candidate_evaluations,
            evaluated_candidate_evaluations,
            remaining_candidate_evaluations,
            truncated,
            requested_evaluation_budget: budget.requested,
            applied_evaluation_budget: budget.applied,
            server_evaluation_budget: budget.server_cap,
        },
    })
}

fn dependency_graph_budget(
    requested_nodes: Option<usize>,
    requested_edges: Option<usize>,
) -> Result<DependencyGraphBudget, SosValidationServiceError> {
    if matches!(requested_nodes, Some(0)) {
        return Err(SosValidationServiceError::InvalidRequest(
            "dependency graph node_budget must be greater than zero".to_string(),
        ));
    }
    if matches!(requested_edges, Some(0)) {
        return Err(SosValidationServiceError::InvalidRequest(
            "dependency graph edge_budget must be greater than zero".to_string(),
        ));
    }

    let server_node_cap = std::env::var("SOS_DEPENDENCY_GRAPH_MAX_NODES")
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_DEPENDENCY_GRAPH_MAX_NODES);
    let server_edge_cap = std::env::var("SOS_DEPENDENCY_GRAPH_MAX_EDGES")
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_DEPENDENCY_GRAPH_MAX_EDGES);

    Ok(DependencyGraphBudget {
        requested_nodes,
        applied_nodes: requested_nodes
            .unwrap_or(server_node_cap)
            .min(server_node_cap),
        server_node_cap,
        requested_edges,
        applied_edges: requested_edges
            .unwrap_or(server_edge_cap)
            .min(server_edge_cap),
        server_edge_cap,
    })
}

fn what_if_budget(
    requested_budget: Option<usize>,
) -> Result<WhatIfBudget, SosValidationServiceError> {
    if matches!(requested_budget, Some(0)) {
        return Err(SosValidationServiceError::InvalidRequest(
            "what-if evaluation_budget must be greater than zero".to_string(),
        ));
    }

    let server_cap = std::env::var("SOS_WHAT_IF_MAX_EVALUATIONS")
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_WHAT_IF_MAX_EVALUATIONS);

    Ok(WhatIfBudget {
        requested: requested_budget,
        applied: requested_budget.unwrap_or(server_cap).min(server_cap),
        server_cap,
    })
}

fn validate_interface_pair_with_catalog(
    service: &SosValidationService,
    provider_interface_id: &str,
    consumer_interface_id: &str,
    interfaces: &HashMap<String, Interface>,
    contracts: &HashMap<String, Contract>,
) -> Result<Option<ValidationExecution>, SosValidationServiceError> {
    let Some(provider) = interfaces.get(provider_interface_id) else {
        return Ok(None);
    };
    let Some(consumer) = interfaces.get(consumer_interface_id) else {
        return Ok(None);
    };

    if provider.system_id == consumer.system_id {
        return Ok(None);
    }

    let contract = super::lookup::find_contract_between_catalog(
        contracts,
        provider_interface_id,
        consumer_interface_id,
    );
    service
        .validate_interface_pair(provider, consumer, contract)
        .map(Some)
}

fn validate_contract_compliance_with_catalog(
    service: &SosValidationService,
    contract_id: &str,
    contracts: &HashMap<String, Contract>,
    interfaces: &HashMap<String, Interface>,
) -> Result<Option<ValidationExecution>, SosValidationServiceError> {
    let Some(contract) = contracts.get(contract_id) else {
        return Ok(None);
    };
    let Some(provider) = interfaces.get(&contract.provider_interface_id) else {
        return Ok(None);
    };
    let Some(consumer) = interfaces.get(&contract.consumer_interface_id) else {
        return Ok(None);
    };

    service
        .validate_contract_compliance_for_entities(contract, provider, consumer)
        .map(Some)
}

fn validate_system_integration_with_catalog(
    service: &SosValidationService,
    source_system_id: &str,
    target_system_id: &str,
    systems: &HashMap<String, crate::api::sos_validation::storage::System>,
    interfaces: &HashMap<String, Interface>,
    contracts: &HashMap<String, Contract>,
) -> Result<Option<ValidationExecution>, SosValidationServiceError> {
    if !systems.contains_key(source_system_id) || !systems.contains_key(target_system_id) {
        return Ok(None);
    }

    let source_interfaces: Vec<_> = interfaces
        .values()
        .filter(|interface| interface.system_id == source_system_id)
        .cloned()
        .collect();
    let target_interfaces: Vec<_> = interfaces
        .values()
        .filter(|interface| interface.system_id == target_system_id)
        .cloned()
        .collect();

    service
        .validate_system_integration_for_catalog(
            source_system_id,
            target_system_id,
            &source_interfaces,
            &target_interfaces,
            |provider_interface_id, consumer_interface_id| {
                Ok(super::lookup::find_contract_between_catalog(
                    contracts,
                    provider_interface_id,
                    consumer_interface_id,
                )
                .cloned())
            },
        )
        .map(Some)
}

#[allow(clippy::too_many_arguments)]
fn apply_what_if_change(
    service: &SosValidationService,
    change: &Value,
    systems: &mut HashMap<String, crate::api::sos_validation::storage::System>,
    interfaces: &mut HashMap<String, Interface>,
    contracts: &mut HashMap<String, Contract>,
    affected_system_ids: &mut HashSet<String>,
    affected_interface_ids: &mut HashSet<String>,
    affected_contract_ids: &mut HashSet<String>,
    impact: &mut Vec<String>,
    recommendations: &mut Vec<String>,
) -> Result<(), SosValidationServiceError> {
    let kind = infer_change_kind(change);
    let action = extract_change_string(change, &["operation", "action"])
        .unwrap_or_else(|| "upsert".to_string())
        .to_ascii_lowercase();

    match kind {
        Some("system") => {
            let Some(system_id) = extract_change_string(change, &["system_id", "id"]) else {
                impact.push("Ignored what-if system change without a system_id or id".to_string());
                return Ok(());
            };

            match action.as_str() {
                "delete" | "remove" => {
                    if systems.remove(&system_id).is_some() {
                        affected_system_ids.insert(system_id.clone());

                        let interfaces_to_remove: Vec<_> = interfaces
                            .values()
                            .filter(|interface| interface.system_id == system_id)
                            .map(|interface| interface.interface_id.clone())
                            .collect();
                        for interface_id in interfaces_to_remove {
                            interfaces.remove(&interface_id);
                            affected_interface_ids.insert(interface_id.clone());

                            let contracts_to_remove: Vec<_> = contracts
                                .values()
                                .filter(|contract| {
                                    contract.provider_interface_id == interface_id
                                        || contract.consumer_interface_id == interface_id
                                })
                                .map(|contract| contract.contract_id.clone())
                                .collect();
                            for contract_id in contracts_to_remove {
                                contracts.remove(&contract_id);
                                affected_contract_ids.insert(contract_id);
                            }
                        }

                        impact.push(format!(
                            "Scenario removes system '{}' and its dependent interfaces/contracts from the projected catalog",
                            system_id
                        ));
                    } else {
                        impact.push(format!(
                            "Scenario tried to remove system '{}' but it does not exist in the current catalog",
                            system_id
                        ));
                    }
                }
                _ => {
                    let now = Utc::now();
                    let mut system = systems.get(&system_id).cloned().unwrap_or(
                        crate::api::sos_validation::storage::System {
                            system_id: system_id.clone(),
                            system_name: extract_change_string(change, &["system_name"])
                                .unwrap_or_else(|| system_id.clone()),
                            system_type: extract_change_string(change, &["system_type"])
                                .unwrap_or_else(|| "unknown".to_string()),
                            vendor: extract_change_string(change, &["vendor"])
                                .unwrap_or_else(|| "unknown".to_string()),
                            version: extract_change_string(change, &["version"])
                                .unwrap_or_else(|| "what-if".to_string()),
                            classification: extract_change_string(change, &["classification"])
                                .unwrap_or_else(|| "UNSPECIFIED".to_string()),
                            description: None,
                            deployment: HashMap::new(),
                            capabilities: HashMap::new(),
                            tags: Vec::new(),
                            active: true,
                            created_at: now,
                            updated_at: now,
                        },
                    );

                    if let Some(system_name) = extract_change_string(change, &["system_name"]) {
                        system.system_name = system_name;
                    }
                    if let Some(system_type) = extract_change_string(change, &["system_type"]) {
                        system.system_type = system_type;
                    }
                    if let Some(vendor) = extract_change_string(change, &["vendor"]) {
                        system.vendor = vendor;
                    }
                    if let Some(version) = extract_change_string(change, &["version"]) {
                        system.version = version;
                    }
                    if let Some(classification) = extract_change_string(change, &["classification"])
                    {
                        system.classification = classification;
                    }
                    if let Some(description) = extract_change_string(change, &["description"]) {
                        system.description = Some(description);
                    }
                    if let Some(active) = extract_change_bool(change, &["active"]) {
                        system.active = active;
                    }
                    if let Some(tags) = extract_change_string_list(change, "tags") {
                        system.tags = tags;
                    }
                    if let Some(deployment) = extract_change_object(change, "deployment") {
                        system.deployment = deployment;
                    }
                    if let Some(capabilities) = extract_change_object(change, "capabilities") {
                        system.capabilities = capabilities;
                    }

                    system.updated_at = now;
                    systems.insert(system_id.clone(), system);
                    affected_system_ids.insert(system_id.clone());
                    impact.push(format!(
                        "Scenario updates system '{}'; downstream interfaces and integrations will be re-evaluated in-memory",
                        system_id
                    ));
                }
            }
        }
        Some("interface") => {
            let Some(interface_id) = extract_change_string(change, &["interface_id", "id"]) else {
                impact.push(
                    "Ignored what-if interface change without an interface_id or id".to_string(),
                );
                return Ok(());
            };

            match action.as_str() {
                "delete" | "remove" => {
                    if let Some(existing) = interfaces.remove(&interface_id) {
                        affected_interface_ids.insert(interface_id.clone());
                        affected_system_ids.insert(existing.system_id.clone());

                        let contracts_to_remove: Vec<_> = contracts
                            .values()
                            .filter(|contract| {
                                contract.provider_interface_id == interface_id
                                    || contract.consumer_interface_id == interface_id
                            })
                            .map(|contract| contract.contract_id.clone())
                            .collect();
                        for contract_id in contracts_to_remove {
                            contracts.remove(&contract_id);
                            affected_contract_ids.insert(contract_id);
                        }

                        impact.push(format!(
                            "Scenario removes interface '{}' from system '{}'",
                            interface_id, existing.system_id
                        ));
                    } else {
                        impact.push(format!(
                            "Scenario tried to remove interface '{}' but it does not exist in the current catalog",
                            interface_id
                        ));
                    }
                }
                _ => {
                    let now = Utc::now();
                    let existing = interfaces.get(&interface_id).cloned();
                    let system_id = extract_change_string(change, &["system_id"])
                        .or_else(|| existing.as_ref().map(|interface| interface.system_id.clone()))
                        .ok_or_else(|| {
                            SosValidationServiceError::InvalidRequest(format!(
                                "What-if interface change for '{}' must include system_id when the interface does not already exist",
                                interface_id
                            ))
                        })?;
                    let old_system_id = existing
                        .as_ref()
                        .map(|interface| interface.system_id.clone());

                    let mut interface = existing.unwrap_or(Interface {
                        interface_id: interface_id.clone(),
                        system_id: system_id.clone(),
                        interface_name: extract_change_string(change, &["interface_name"])
                            .unwrap_or_else(|| interface_id.clone()),
                        direction: extract_change_string(change, &["direction"])
                            .unwrap_or_else(|| "bidirectional".to_string()),
                        protocol: extract_change_string(change, &["protocol"])
                            .unwrap_or_else(|| "unknown".to_string()),
                        data_format: extract_change_string(change, &["data_format"])
                            .unwrap_or_else(|| "json".to_string()),
                        schema: change
                            .get("schema")
                            .cloned()
                            .unwrap_or_else(|| json!({"type": "object"})),
                        coordinate_system: None,
                        unit_system: None,
                        metadata: HashMap::new(),
                        created_at: now,
                        updated_at: now,
                    });

                    interface.system_id = system_id.clone();
                    if let Some(interface_name) = extract_change_string(change, &["interface_name"])
                    {
                        interface.interface_name = interface_name;
                    }
                    if let Some(direction) = extract_change_string(change, &["direction"]) {
                        interface.direction = direction;
                    }
                    if let Some(protocol) = extract_change_string(change, &["protocol"]) {
                        interface.protocol = protocol;
                    }
                    if let Some(data_format) = extract_change_string(change, &["data_format"]) {
                        interface.data_format = data_format;
                    }
                    if let Some(schema) = change.get("schema") {
                        interface.schema = schema.clone();
                    }
                    if change.get("coordinate_system").is_some() {
                        interface.coordinate_system =
                            extract_change_string(change, &["coordinate_system"]);
                    }
                    if change.get("unit_system").is_some() {
                        interface.unit_system = extract_change_string(change, &["unit_system"]);
                    }
                    if let Some(metadata) = extract_change_object(change, "metadata") {
                        interface.metadata = metadata;
                    }

                    refresh_interface_overlay_metadata(&mut interface)?;
                    interface.updated_at = now;

                    interfaces.insert(interface_id.clone(), interface);
                    affected_interface_ids.insert(interface_id.clone());
                    affected_system_ids.insert(system_id.clone());
                    if let Some(old_system_id) = old_system_id {
                        affected_system_ids.insert(old_system_id);
                    }

                    impact.push(format!(
                        "Scenario updates interface '{}' on system '{}'",
                        interface_id, system_id
                    ));
                }
            }
        }
        Some("contract") => {
            let Some(contract_id) = extract_change_string(change, &["contract_id", "id"]) else {
                impact.push(
                    "Ignored what-if contract change without a contract_id or id".to_string(),
                );
                return Ok(());
            };

            match action.as_str() {
                "delete" | "remove" => {
                    if let Some(existing) = contracts.remove(&contract_id) {
                        affected_contract_ids.insert(contract_id.clone());
                        affected_interface_ids.insert(existing.provider_interface_id.clone());
                        affected_interface_ids.insert(existing.consumer_interface_id.clone());
                        if let Some(provider) = interfaces.get(&existing.provider_interface_id) {
                            affected_system_ids.insert(provider.system_id.clone());
                        }
                        if let Some(consumer) = interfaces.get(&existing.consumer_interface_id) {
                            affected_system_ids.insert(consumer.system_id.clone());
                        }

                        impact.push(format!(
                            "Scenario removes contract '{}', which will change contract-backed compatibility and integration paths",
                            contract_id
                        ));
                    } else {
                        impact.push(format!(
                            "Scenario tried to remove contract '{}' but it does not exist in the current catalog",
                            contract_id
                        ));
                    }
                }
                _ => {
                    let now = Utc::now();
                    let existing = contracts.get(&contract_id).cloned();
                    let provider_interface_id = extract_change_string(change, &["provider_interface_id"])
                        .or_else(|| {
                            existing
                                .as_ref()
                                .map(|contract| contract.provider_interface_id.clone())
                        })
                        .ok_or_else(|| {
                            SosValidationServiceError::InvalidRequest(format!(
                                "What-if contract change for '{}' must include provider_interface_id when the contract does not already exist",
                                contract_id
                            ))
                        })?;
                    let consumer_interface_id = extract_change_string(change, &["consumer_interface_id"])
                        .or_else(|| {
                            existing
                                .as_ref()
                                .map(|contract| contract.consumer_interface_id.clone())
                        })
                        .ok_or_else(|| {
                            SosValidationServiceError::InvalidRequest(format!(
                                "What-if contract change for '{}' must include consumer_interface_id when the contract does not already exist",
                                contract_id
                            ))
                        })?;

                    let mut contract = existing.unwrap_or(Contract {
                        contract_id: contract_id.clone(),
                        revision: 1,
                        contract_name: extract_change_string(change, &["contract_name"])
                            .unwrap_or_else(|| contract_id.clone()),
                        provider_interface_id: provider_interface_id.clone(),
                        consumer_interface_id: consumer_interface_id.clone(),
                        sla_metrics: Vec::new(),
                        transformation_rules: HashMap::new(),
                        description: None,
                        tags: Vec::new(),
                        approved: false,
                        signed: false,
                        lifecycle_state: Some(CONTRACT_LIFECYCLE_DRAFT.to_string()),
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
                        created_by: "scenario".to_string(),
                        updated_by: "scenario".to_string(),
                        superseded_by_revision: None,
                        created_at: now,
                        updated_at: now,
                    });

                    contract.provider_interface_id = provider_interface_id.clone();
                    contract.consumer_interface_id = consumer_interface_id.clone();
                    if let Some(contract_name) = extract_change_string(change, &["contract_name"]) {
                        contract.contract_name = contract_name;
                    }
                    if let Some(sla_metrics) = extract_sla_metrics(change)? {
                        contract.sla_metrics = sla_metrics;
                    }
                    if let Some(transformation_rules) =
                        extract_change_object(change, "transformation_rules")
                    {
                        contract.transformation_rules = transformation_rules;
                    }
                    if let Some(description) = extract_change_string(change, &["description"]) {
                        contract.description = Some(description);
                    }
                    if let Some(tags) = extract_change_string_list(change, "tags") {
                        contract.tags = tags;
                    }
                    if let Some(approved) = extract_change_bool(change, &["approved"]) {
                        contract.approved = approved;
                        if !approved {
                            contract.signed = false;
                        }
                    }
                    if let Some(signed) = extract_change_bool(change, &["signed"]) {
                        contract.signed = signed;
                        if signed {
                            contract.approved = true;
                        }
                    }

                    contract.lifecycle_state =
                        Some(effective_contract_lifecycle_state(&contract).to_string());
                    contract.updated_by = "scenario".to_string();
                    contract.updated_at = now;
                    contracts.insert(contract_id.clone(), contract);
                    affected_contract_ids.insert(contract_id.clone());
                    affected_interface_ids.insert(provider_interface_id.clone());
                    affected_interface_ids.insert(consumer_interface_id.clone());
                    if let Some(provider) = interfaces.get(&provider_interface_id) {
                        affected_system_ids.insert(provider.system_id.clone());
                    }
                    if let Some(consumer) = interfaces.get(&consumer_interface_id) {
                        affected_system_ids.insert(consumer.system_id.clone());
                    }

                    impact.push(format!(
                        "Scenario updates contract '{}' for interface path '{} -> {}'",
                        contract_id, provider_interface_id, consumer_interface_id
                    ));
                }
            }
        }
        _ => {
            impact.push(format!(
                "Ignored unsupported what-if change payload: {}",
                change
            ));
            recommendations.push(
                "Provide an entity_type/kind (system, interface, contract) or include identifying keys such as system_id, interface_id, or contract_id".to_string(),
            );
        }
    }

    Ok(())
}

fn refresh_interface_overlay_metadata(
    interface: &mut Interface,
) -> Result<(), SosValidationServiceError> {
    let schema_hash = hash_json(&interface.schema)?;
    interface.metadata.insert(
        "schema_hash".to_string(),
        Value::String(schema_hash.clone()),
    );
    interface.metadata.insert(
        "shape_ref".to_string(),
        Value::String(format!(
            "http://graphica.io/sos/interface/{}/shape/{}",
            interface.interface_id, schema_hash
        )),
    );
    Ok(())
}

fn build_execution_delta_message(
    label: &str,
    current: Option<&ValidationExecution>,
    projected: Option<&ValidationExecution>,
) -> Option<String> {
    match (current, projected) {
        (Some(current), Some(projected)) => {
            let current_passed = execution_passed(&current.checks);
            let projected_passed = execution_passed(&projected.checks);
            let (resolved_checks, new_failures) =
                compare_execution_checks(&current.checks, &projected.checks);
            let confidence_delta = projected.confidence - current.confidence;

            if current_passed != projected_passed {
                Some(format!(
                    "{} would change from {} to {} (confidence {:.2} -> {:.2}; resolved checks: {}; new failures: {})",
                    label,
                    execution_state_label(current_passed),
                    execution_state_label(projected_passed),
                    current.confidence,
                    projected.confidence,
                    format_check_list(&resolved_checks),
                    format_check_list(&new_failures),
                ))
            } else if !resolved_checks.is_empty()
                || !new_failures.is_empty()
                || confidence_delta.abs() >= 0.01
            {
                Some(format!(
                    "{} would remain {} but change materially (confidence {:.2} -> {:.2}; resolved checks: {}; new failures: {})",
                    label,
                    execution_state_label(projected_passed),
                    current.confidence,
                    projected.confidence,
                    format_check_list(&resolved_checks),
                    format_check_list(&new_failures),
                ))
            } else {
                None
            }
        }
        (None, Some(projected)) => Some(format!(
            "{} would be introduced with projected state {} (confidence {:.2})",
            label,
            execution_state_label(execution_passed(&projected.checks)),
            projected.confidence,
        )),
        (Some(_), None) => Some(format!(
            "{} would be removed from the active SoS catalog context",
            label
        )),
        (None, None) => None,
    }
}

fn compare_execution_checks(
    current: &[ValidationCheckRecord],
    projected: &[ValidationCheckRecord],
) -> (Vec<String>, Vec<String>) {
    let current_map: HashMap<_, _> = current
        .iter()
        .map(|check| (check.check_name.as_str(), check))
        .collect();
    let projected_map: HashMap<_, _> = projected
        .iter()
        .map(|check| (check.check_name.as_str(), check))
        .collect();

    let resolved_checks = current_map
        .iter()
        .filter_map(|(name, current_check)| {
            let projected_check = projected_map.get(name)?;
            (!current_check.passed && projected_check.passed).then(|| (*name).to_string())
        })
        .collect();

    let new_failures = projected_map
        .iter()
        .filter_map(|(name, projected_check)| {
            let current_failed = current_map
                .get(name)
                .map(|current_check| !current_check.passed)
                .unwrap_or(false);
            (!projected_check.passed && !current_failed).then(|| (*name).to_string())
        })
        .collect();

    (resolved_checks, new_failures)
}

fn append_execution_recommendations(
    label: &str,
    execution: &ValidationExecution,
    recommendations: &mut Vec<String>,
) {
    let blocking_failures: Vec<_> = execution
        .checks
        .iter()
        .filter(|check| !check.passed && check.severity.eq_ignore_ascii_case("error"))
        .map(|check| check.check_name.as_str())
        .collect();

    if blocking_failures.is_empty() {
        return;
    }

    recommendations.push(format!(
        "Address blocking validation issues for {}: {}",
        label,
        blocking_failures.join(", ")
    ));

    if blocking_failures.contains(&"schema_compatibility") {
        recommendations.push(format!(
            "Align provider/consumer schemas for {} before promoting the scenario",
            label
        ));
    }
    if blocking_failures.contains(&"unit_compatibility") {
        recommendations.push(format!(
            "Add an explicit unit transformation rule or align declared unit systems for {}",
            label
        ));
    }
    if blocking_failures.contains(&"coordinate_compatibility") {
        recommendations.push(format!(
            "Add an explicit coordinate transformation rule or align coordinate systems for {}",
            label
        ));
    }
    if blocking_failures.contains(&"contract_approved")
        || blocking_failures.contains(&"contract_signed")
    {
        recommendations.push(format!(
            "Approve and sign the governing contract for {} before deployment",
            label
        ));
    }
    if blocking_failures.contains(&"contracted_integration_path") {
        recommendations.push(format!(
            "Create or repair a contract-backed integration path for {}",
            label
        ));
    }
}

fn execution_state_label(passed: bool) -> &'static str {
    if passed {
        "passing"
    } else {
        "failing"
    }
}

fn format_check_list(checks: &[String]) -> String {
    if checks.is_empty() {
        "none".to_string()
    } else {
        checks.join(", ")
    }
}

fn infer_change_kind(change: &Value) -> Option<&'static str> {
    let explicit_kind = change
        .get("entity_type")
        .or_else(|| change.get("kind"))
        .or_else(|| change.get("entity"))
        .or_else(|| change.get("resource_type"))
        .and_then(Value::as_str)
        .map(|value| value.to_ascii_lowercase());

    match explicit_kind.as_deref() {
        Some("system") => Some("system"),
        Some("interface") => Some("interface"),
        Some("contract") => Some("contract"),
        Some(_) | None => {
            if change.get("contract_id").is_some()
                || (change.get("provider_interface_id").is_some()
                    && change.get("consumer_interface_id").is_some())
            {
                Some("contract")
            } else if change.get("interface_id").is_some()
                || change.get("schema").is_some()
                || change.get("protocol").is_some()
                || change.get("data_format").is_some()
            {
                Some("interface")
            } else if change.get("system_id").is_some()
                || change.get("system_name").is_some()
                || change.get("system_type").is_some()
            {
                Some("system")
            } else {
                None
            }
        }
    }
}

fn extract_change_string(change: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        change
            .get(*key)
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
    })
}

fn extract_change_bool(change: &Value, keys: &[&str]) -> Option<bool> {
    keys.iter()
        .find_map(|key| change.get(*key).and_then(Value::as_bool))
}

fn extract_change_string_list(change: &Value, key: &str) -> Option<Vec<String>> {
    change.get(key).and_then(Value::as_array).map(|values| {
        values
            .iter()
            .filter_map(Value::as_str)
            .map(ToOwned::to_owned)
            .collect()
    })
}

fn extract_change_object(change: &Value, key: &str) -> Option<HashMap<String, Value>> {
    change.get(key).and_then(Value::as_object).map(|object| {
        object
            .iter()
            .map(|(inner_key, inner_value)| (inner_key.clone(), inner_value.clone()))
            .collect()
    })
}

fn extract_sla_metrics(
    change: &Value,
) -> Result<Option<Vec<crate::api::sos_validation::storage::SlaMetric>>, SosValidationServiceError>
{
    let Some(raw_metrics) = change.get("sla_metrics") else {
        return Ok(None);
    };

    serde_json::from_value(raw_metrics.clone())
        .map(Some)
        .map_err(|error| {
            SosValidationServiceError::InvalidRequest(format!(
                "What-if change contains invalid sla_metrics: {}",
                error
            ))
        })
}
