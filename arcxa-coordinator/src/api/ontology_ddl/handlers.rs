//! API Handlers for Ontology-Driven DDL Generation
//!
//! Exposes the ontology-driven DDL generation via REST API.

use super::types::*;
use crate::api::dto::ApiError;
use crate::api::ApiState;
use crate::mapping::discovery::types::{ColumnStatistics, DiscoveredColumn, DiscoveredTable};
use crate::mapping::ontology_ddl::{
    generate_ontology_ddl, generate_ontology_ddl_with_config, OntologyDdlConfig,
    OntologyDdlOrchestrator,
};
use crate::mapping::ontology_registry::RegistryClient;
use axum::{extract::State, Json};
use std::sync::Arc;
use std::time::Instant;
use tracing::{info, warn};

/// Generate ontology-driven DDL from discovered schema
///
/// POST /api/v1/ontology-ddl/generate
pub async fn generate_ontology_ddl_handler(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<GenerateOntologyDdlRequest>,
) -> Result<Json<GenerateOntologyDdlResponse>, ApiError> {
    let start_time = Instant::now();

    info!(
        "Generating ontology-driven DDL for table: {}, columns: {}, dialect: {}",
        request.table_name,
        request.columns.len(),
        request.dialect
    );

    // Convert API request to DiscoveredTable
    let discovered_table = convert_request_to_discovered_table(&request)
        .map_err(|e| ApiError::bad_request(format!("Invalid request: {}", e)))?;

    // Convert API config to internal config
    let config = convert_config_input_to_config(&request.config);

    // Create orchestrator with custom ontologies if available
    let orchestrator = create_orchestrator_with_custom_ontologies(&state, config)?;

    // Generate DDL using orchestrator
    let result = orchestrator
        .generate_ddl(&discovered_table, &request.dialect)
        .await
        .map_err(|e| ApiError::internal(format!("DDL generation failed: {}", e)))?;

    let execution_time_ms = start_time.elapsed().as_millis() as u64;

    info!(
        "Successfully generated ontology-driven DDL: {} statements, {} mappings, {}ms",
        result.ddl_statements.len(),
        result.ontology_mappings.len(),
        execution_time_ms
    );

    // Convert result to API response
    let response = convert_result_to_response(result, &request.table_name, execution_time_ms);

    Ok(Json(response))
}

/// Get ontology mappings for a table without generating DDL
///
/// POST /api/v1/ontology-ddl/analyze
pub async fn analyze_ontology_mappings_handler(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<GenerateOntologyDdlRequest>,
) -> Result<Json<Vec<OntologyMappingInfo>>, ApiError> {
    info!(
        "Analyzing ontology mappings for table: {}, columns: {}",
        request.table_name,
        request.columns.len()
    );

    // Convert API request to DiscoveredTable
    let discovered_table = convert_request_to_discovered_table(&request)
        .map_err(|e| ApiError::bad_request(format!("Invalid request: {}", e)))?;

    // Convert API config to internal config
    let config = convert_config_input_to_config(&request.config);

    // Create orchestrator with custom ontologies if available
    let orchestrator = create_orchestrator_with_custom_ontologies(&state, config)?;

    // Generate result (we need full generation to get mappings)
    let result = orchestrator
        .generate_ddl(&discovered_table, &request.dialect)
        .await
        .map_err(|e| ApiError::internal(format!("Analysis failed: {}", e)))?;

    // Convert mappings to API format
    let mappings: Vec<OntologyMappingInfo> =
        result.ontology_mappings.iter().map(|m| m.into()).collect();

    info!("Found {} ontology mappings", mappings.len());

    Ok(Json(mappings))
}

/// Get default configuration
///
/// GET /api/v1/ontology-ddl/config/default
pub async fn get_default_config_handler(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<OntologyDdlConfigInput>, ApiError> {
    Ok(Json(OntologyDdlConfigInput::default()))
}

// Helper functions

/// Create an orchestrator with custom ontologies if available
///
/// This function checks if a PersistedOntologyRegistry is available in ApiState.
/// If it is, it creates an orchestrator that loads custom ontologies. Otherwise,
/// it falls back to the default orchestrator with only schema.org terms.
fn create_orchestrator_with_custom_ontologies(
    state: &ApiState,
    config: OntologyDdlConfig,
) -> Result<OntologyDdlOrchestrator, ApiError> {
    // Check if persisted ontology registry is available
    if let Some(persisted_registry) = &state.persisted_ontology_registry {
        info!("Creating orchestrator with custom ontologies from registry");

        // Create RegistryClient from persisted registry
        let registry_client = RegistryClient::new(Some(persisted_registry.registry()));

        // Create orchestrator with custom ontologies
        OntologyDdlOrchestrator::with_custom_ontologies(config, &registry_client).map_err(|e| {
            warn!(
                "Failed to load custom ontologies, falling back to defaults: {}",
                e
            );
            // Fall back to default orchestrator if loading custom ontologies fails
            ApiError::internal(format!(
                "Failed to create orchestrator with custom ontologies: {}",
                e
            ))
        })
    } else {
        info!("No custom ontology registry available, using default schema.org terms");
        Ok(OntologyDdlOrchestrator::new(config))
    }
}

fn convert_request_to_discovered_table(
    request: &GenerateOntologyDdlRequest,
) -> anyhow::Result<DiscoveredTable> {
    let columns: Vec<DiscoveredColumn> = request
        .columns
        .iter()
        .map(|col| DiscoveredColumn {
            name: col.name.clone(),
            data_type: col.data_type.clone(),
            nullable: col.nullable,
            primary_key: col.primary_key,
            semantic_type: None,
            confidence: 0.0,
            patterns: vec![],
            statistics: ColumnStatistics {
                distinct_count: col.distinct_count.unwrap_or(0),
                null_fraction: if col.nullable { 0.1 } else { 0.0 },
                sample_count: col.sample_values.len(),
                most_common_values: None,
                avg_length: col.avg_length,
                min_value: col.min_value.clone(),
                max_value: col.max_value.clone(),
            },
            sample_values: col.sample_values.clone(),
        })
        .collect();

    Ok(DiscoveredTable {
        name: request.table_name.clone(),
        columns,
        row_count: None,
    })
}

fn convert_config_input_to_config(input: &OntologyDdlConfigInput) -> OntologyDdlConfig {
    OntologyDdlConfig {
        skip_ontology_mapping: input.skip_ontology_mapping,
        min_mapping_confidence: input.min_mapping_confidence,
        strict_constraints: input.strict_constraints,
        record_lineage: input.record_lineage,
        max_candidates: input.max_candidates,
    }
}

fn convert_result_to_response(
    result: crate::mapping::ontology_ddl::types::OntologyDdlResult,
    table_name: &str,
    execution_time_ms: u64,
) -> GenerateOntologyDdlResponse {
    // Convert ontology mappings
    let ontology_mappings: Vec<OntologyMappingInfo> =
        result.ontology_mappings.iter().map(|m| m.into()).collect();

    // Create SHACL shape summary
    let shacl_shape_summary = ShaclShapeSummary {
        shape_uri: result.shacl_shape.uri.clone(),
        target_class: result.shacl_shape.target_class.clone(),
        property_count: result.shacl_shape.properties.len(),
        closed: result.shacl_shape.closed,
    };

    // Create lineage summary if available
    let lineage_summary = result.rdf_triples.as_ref().map(|triples| {
        // Count entity and activity types
        let entity_count = triples
            .iter()
            .filter(|(_, p, o)| p.ends_with("type") && o.contains("Entity"))
            .count();

        let activity_count = triples
            .iter()
            .filter(|(_, p, o)| p.ends_with("type") && o.contains("Activity"))
            .count();

        let derivation_count = triples
            .iter()
            .filter(|(_, p, _)| p.contains("wasDerivedFrom") || p.contains("wasGeneratedBy"))
            .count();

        LineageSummaryInfo {
            total_triples: triples.len(),
            entity_count,
            activity_count,
            derivation_count,
        }
    });

    GenerateOntologyDdlResponse {
        ddl_statements: result.ddl_statements,
        table_name: table_name.to_string(),
        column_count: result.table_definition.columns.len(),
        ontology_mappings,
        shacl_shape_summary,
        lineage_summary,
        execution_time_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_request_to_discovered_table() {
        let request = GenerateOntologyDdlRequest {
            table_name: "customers".to_string(),
            columns: vec![ColumnDiscoveryInput {
                name: "email".to_string(),
                data_type: "VARCHAR(255)".to_string(),
                nullable: false,
                primary_key: false,
                sample_values: vec![
                    "john@example.com".to_string(),
                    "jane@example.com".to_string(),
                ],
                min_value: None,
                max_value: None,
                avg_length: Some(20.0),
                distinct_count: Some(1000),
            }],
            dialect: "postgresql".to_string(),
            config: OntologyDdlConfigInput::default(),
        };

        let table = convert_request_to_discovered_table(&request).unwrap();

        assert_eq!(table.name, "customers");
        assert_eq!(table.columns.len(), 1);
        assert_eq!(table.columns[0].name, "email");
        assert_eq!(table.columns[0].sample_values.len(), 2);
    }

    #[test]
    fn test_convert_config_input_to_config() {
        let input = OntologyDdlConfigInput {
            skip_ontology_mapping: true,
            min_mapping_confidence: 0.8,
            strict_constraints: false,
            record_lineage: false,
            max_candidates: 10,
        };

        let config = convert_config_input_to_config(&input);

        assert_eq!(config.skip_ontology_mapping, true);
        assert_eq!(config.min_mapping_confidence, 0.8);
        assert_eq!(config.strict_constraints, false);
        assert_eq!(config.record_lineage, false);
        assert_eq!(config.max_candidates, 10);
    }

    #[test]
    fn test_default_config() {
        let config = OntologyDdlConfigInput::default();

        assert_eq!(config.skip_ontology_mapping, false);
        assert_eq!(config.min_mapping_confidence, 0.7);
        assert_eq!(config.strict_constraints, true);
        assert_eq!(config.record_lineage, true);
        assert_eq!(config.max_candidates, 5);
    }
}
