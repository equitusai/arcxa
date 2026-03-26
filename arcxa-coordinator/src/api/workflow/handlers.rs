//! Workflow API handlers

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use serde::Deserialize;
use std::{collections::HashSet, sync::Arc};
use utoipa;

use super::types::*;
use crate::api::workflow::materialization::{
    finalize_execution_result, persist_execution_record_if_possible,
};
use crate::api::ApiState;
use crate::governance::WorkflowResultPersistence;
use crate::workflows::dataset_input::build_input_adapter;
use graphica_core::{
    catalog::{api_types::DataSourceCapabilities, DataSourceCatalog},
    errors::GraphicaError,
    orchestration::workflow::{
        definition::{DbExtractConfig, DbLoaderConfig, LoadMode, StepConfig},
        InputAdapter, WorkflowDefinition,
    },
};

#[derive(Debug, Default, Deserialize)]
pub struct ExecutionHistoryQuery {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

fn map_workflow_engine_error(action: &str, error: anyhow::Error) -> (StatusCode, String) {
    let message = error.to_string();

    if message.starts_with("Workflow not found:") {
        return (StatusCode::NOT_FOUND, message);
    }

    if message.starts_with("Workflow already exists:") {
        return (StatusCode::CONFLICT, message);
    }

    (
        StatusCode::INTERNAL_SERVER_ERROR,
        format!("{} failed: {}", action, message),
    )
}

const WORKFLOW_ISSUE_STEP_ID: &str = "$workflow";

fn validation_issue(
    level: WorkflowValidationIssueLevel,
    step_id: impl Into<String>,
    code: impl Into<String>,
    message: impl Into<String>,
    field: Option<&str>,
) -> WorkflowValidationIssue {
    WorkflowValidationIssue {
        level,
        step_id: step_id.into(),
        code: code.into(),
        message: message.into(),
        field: field.map(str::to_string),
    }
}

fn optional_non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn datasource_capabilities(capabilities: Option<DataSourceCapabilities>) -> DataSourceCapabilities {
    capabilities.unwrap_or(DataSourceCapabilities {
        can_test: false,
        can_infer_schema: false,
        can_query: false,
        can_read_workflow: false,
        can_write_workflow: false,
        supports_parameters: false,
        supports_tls: false,
        supports_incremental: false,
        supports_cancellation: false,
    })
}

fn normalize_identifier_variants(value: &str) -> Vec<String> {
    let cleaned_segments = value
        .split('.')
        .map(|segment| {
            segment
                .trim()
                .trim_matches('"')
                .trim_matches('`')
                .trim_matches('[')
                .trim_matches(']')
                .to_ascii_lowercase()
        })
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();

    if cleaned_segments.is_empty() {
        return Vec::new();
    }

    let mut variants = Vec::with_capacity(cleaned_segments.len());
    for start in 0..cleaned_segments.len() {
        variants.push(cleaned_segments[start..].join("."));
    }
    variants.sort();
    variants.dedup();
    variants
}

fn table_name_matches(actual: &str, expected: &str) -> bool {
    let actual_variants = normalize_identifier_variants(actual);
    let expected_variants = normalize_identifier_variants(expected);

    actual_variants
        .iter()
        .any(|actual_variant| expected_variants.contains(actual_variant))
}

fn validate_definition_shape(definition: &WorkflowDefinition) -> Vec<WorkflowValidationIssue> {
    let mut issues = Vec::new();

    if definition.steps.is_empty() {
        issues.push(validation_issue(
            WorkflowValidationIssueLevel::Error,
            WORKFLOW_ISSUE_STEP_ID,
            "no_steps",
            "Workflow must have at least one step",
            None,
        ));
    }

    let mut seen_ids = HashSet::new();
    let mut known_ids = HashSet::new();

    for step in &definition.steps {
        let step_id = step.id.trim();

        if step_id.is_empty() {
            issues.push(validation_issue(
                WorkflowValidationIssueLevel::Error,
                WORKFLOW_ISSUE_STEP_ID,
                "empty_step_id",
                "Step ID cannot be empty",
                Some("id"),
            ));
            continue;
        }

        if !seen_ids.insert(step_id.to_string()) {
            issues.push(validation_issue(
                WorkflowValidationIssueLevel::Error,
                step_id,
                "duplicate_step_id",
                format!("Duplicate step ID: {}", step_id),
                Some("id"),
            ));
        }

        known_ids.insert(step_id.to_string());
    }

    for step in &definition.steps {
        for dependency in &step.depends_on {
            if !known_ids.contains(dependency) {
                issues.push(validation_issue(
                    WorkflowValidationIssueLevel::Error,
                    step.id.clone(),
                    "missing_dependency",
                    format!(
                        "Step '{}' depends on non-existent step '{}'",
                        step.id, dependency
                    ),
                    Some("depends_on"),
                ));
            }
        }

        match &step.config {
            StepConfig::DbExtract(config) => {
                issues.extend(validate_db_extract_shape(step.id.as_str(), config));
            }
            StepConfig::DbLoader(config) => {
                issues.extend(validate_db_loader_shape(step.id.as_str(), config));
            }
            _ => {}
        }
    }

    issues
}

fn validate_db_extract_shape(
    step_id: &str,
    config: &DbExtractConfig,
) -> Vec<WorkflowValidationIssue> {
    let mut issues = Vec::new();
    let table_name = optional_non_empty(config.table_name.as_deref());
    let query = optional_non_empty(config.query.as_deref());

    if config.datasource_id.trim().is_empty() {
        issues.push(validation_issue(
            WorkflowValidationIssueLevel::Error,
            step_id,
            "missing_datasource_id",
            "db_extract requires a datasource_id",
            Some("datasource_id"),
        ));
    }

    if table_name.is_none() && query.is_none() {
        issues.push(validation_issue(
            WorkflowValidationIssueLevel::Error,
            step_id,
            "missing_source_selector",
            "db_extract requires either table_name or query",
            Some("table_name"),
        ));
    }

    if query.is_some() && config.incremental.unwrap_or(false) {
        issues.push(validation_issue(
            WorkflowValidationIssueLevel::Error,
            step_id,
            "incremental_query_mode_unsupported",
            "db_extract incremental extraction is only supported in table mode",
            Some("incremental"),
        ));
    }

    if config.incremental.unwrap_or(false)
        && optional_non_empty(config.incremental_column.as_deref()).is_none()
    {
        issues.push(validation_issue(
            WorkflowValidationIssueLevel::Error,
            step_id,
            "missing_incremental_column",
            "incremental_column is required when incremental extraction is enabled",
            Some("incremental_column"),
        ));
    }

    if config.incremental.unwrap_or(false) && config.last_value.is_none() {
        issues.push(validation_issue(
            WorkflowValidationIssueLevel::Error,
            step_id,
            "missing_last_value",
            "last_value is required when incremental extraction is enabled",
            Some("last_value"),
        ));
    }

    if config.include_schema.unwrap_or(false)
        && optional_non_empty(config.schema_table.as_deref()).is_none()
        && table_name.is_none()
    {
        issues.push(validation_issue(
            WorkflowValidationIssueLevel::Error,
            step_id,
            "missing_schema_table",
            "schema_table or table_name is required when include_schema is enabled",
            Some("schema_table"),
        ));
    }

    issues
}

fn validate_db_loader_shape(
    step_id: &str,
    config: &DbLoaderConfig,
) -> Vec<WorkflowValidationIssue> {
    let mut issues = Vec::new();

    if config.datasource_id.trim().is_empty() {
        issues.push(validation_issue(
            WorkflowValidationIssueLevel::Error,
            step_id,
            "missing_datasource_id",
            "db_loader requires a datasource_id",
            Some("datasource_id"),
        ));
    }

    if config.table_name.trim().is_empty() {
        issues.push(validation_issue(
            WorkflowValidationIssueLevel::Error,
            step_id,
            "missing_target_table",
            "db_loader requires a target table_name",
            Some("table_name"),
        ));
    }

    if matches!(config.mode, LoadMode::Upsert)
        && config
            .key_fields
            .as_ref()
            .map(|fields| fields.iter().any(|field| !field.trim().is_empty()))
            != Some(true)
    {
        issues.push(validation_issue(
            WorkflowValidationIssueLevel::Error,
            step_id,
            "missing_key_fields",
            "db_loader in upsert mode requires one or more key_fields",
            Some("key_fields"),
        ));
    }

    issues
}

async fn validate_datasource_backed_steps(
    catalog: &dyn DataSourceCatalog,
    definition: &WorkflowDefinition,
) -> Vec<WorkflowValidationIssue> {
    let mut issues = Vec::new();

    for step in &definition.steps {
        match &step.config {
            StepConfig::DbExtract(config) => {
                issues.extend(validate_db_extract_datasource(catalog, &step.id, config).await);
            }
            StepConfig::DbLoader(config) => {
                issues.extend(validate_db_loader_datasource(catalog, &step.id, config).await);
            }
            _ => {}
        }
    }

    issues
}

async fn validate_db_extract_datasource(
    catalog: &dyn DataSourceCatalog,
    step_id: &str,
    config: &DbExtractConfig,
) -> Vec<WorkflowValidationIssue> {
    let mut issues = Vec::new();
    let datasource_id = config.datasource_id.trim();

    if datasource_id.is_empty() {
        return issues;
    }

    let source = match catalog.get_source(datasource_id).await {
        Ok(source) => source,
        Err(GraphicaError::NotFound(_)) => {
            issues.push(validation_issue(
                WorkflowValidationIssueLevel::Error,
                step_id,
                "datasource_not_found",
                format!("Datasource '{}' was not found", datasource_id),
                Some("datasource_id"),
            ));
            return issues;
        }
        Err(error) => {
            issues.push(validation_issue(
                WorkflowValidationIssueLevel::Error,
                step_id,
                "datasource_lookup_failed",
                format!("Failed to load datasource '{}': {}", datasource_id, error),
                Some("datasource_id"),
            ));
            return issues;
        }
    };

    let capabilities = datasource_capabilities(source.capabilities);
    let query = optional_non_empty(config.query.as_deref());
    let table_name = optional_non_empty(config.table_name.as_deref());
    let schema_table = optional_non_empty(config.schema_table.as_deref());

    if !capabilities.can_read_workflow {
        issues.push(validation_issue(
            WorkflowValidationIssueLevel::Error,
            step_id,
            "datasource_not_workflow_readable",
            format!(
                "Datasource '{}' is not eligible for workflow extraction",
                datasource_id
            ),
            Some("datasource_id"),
        ));
    }

    if query.is_some() && !capabilities.can_query {
        issues.push(validation_issue(
            WorkflowValidationIssueLevel::Error,
            step_id,
            "query_not_supported",
            format!(
                "Datasource '{}' does not support workflow query mode",
                datasource_id
            ),
            Some("query"),
        ));
    }

    if config.incremental.unwrap_or(false) && !capabilities.supports_incremental {
        issues.push(validation_issue(
            WorkflowValidationIssueLevel::Error,
            step_id,
            "incremental_not_supported",
            format!(
                "Datasource '{}' does not support incremental extraction",
                datasource_id
            ),
            Some("incremental"),
        ));
    }

    if !capabilities.can_infer_schema {
        return issues;
    }

    let table_for_schema_check = if query.is_some() {
        schema_table
    } else {
        schema_table.or(table_name)
    };

    let Some(table_for_schema_check) = table_for_schema_check else {
        return issues;
    };

    let schema_field = if query.is_some() && schema_table.is_some() {
        "schema_table"
    } else if schema_table.is_some() {
        "schema_table"
    } else {
        "table_name"
    };

    match catalog
        .infer_schema(
            datasource_id,
            Some(table_for_schema_check),
            config.schema_sample_size.unwrap_or(1000),
        )
        .await
    {
        Ok(schema) => {
            if !schema
                .tables
                .iter()
                .any(|table| table_name_matches(&table.name, table_for_schema_check))
            {
                issues.push(validation_issue(
                    WorkflowValidationIssueLevel::Error,
                    step_id,
                    "source_table_not_found",
                    format!(
                        "Source table '{}' could not be found for datasource '{}'",
                        table_for_schema_check, datasource_id
                    ),
                    Some(schema_field),
                ));
            }
        }
        Err(error) => {
            issues.push(validation_issue(
                WorkflowValidationIssueLevel::Error,
                step_id,
                "source_schema_inference_failed",
                format!(
                    "Failed to verify source table '{}' for datasource '{}': {}",
                    table_for_schema_check, datasource_id, error
                ),
                Some(schema_field),
            ));
        }
    }

    issues
}

async fn validate_db_loader_datasource(
    catalog: &dyn DataSourceCatalog,
    step_id: &str,
    config: &DbLoaderConfig,
) -> Vec<WorkflowValidationIssue> {
    let mut issues = Vec::new();
    let datasource_id = config.datasource_id.trim();

    if datasource_id.is_empty() {
        return issues;
    }

    let source = match catalog.get_source(datasource_id).await {
        Ok(source) => source,
        Err(GraphicaError::NotFound(_)) => {
            issues.push(validation_issue(
                WorkflowValidationIssueLevel::Error,
                step_id,
                "datasource_not_found",
                format!("Datasource '{}' was not found", datasource_id),
                Some("datasource_id"),
            ));
            return issues;
        }
        Err(error) => {
            issues.push(validation_issue(
                WorkflowValidationIssueLevel::Error,
                step_id,
                "datasource_lookup_failed",
                format!("Failed to load datasource '{}': {}", datasource_id, error),
                Some("datasource_id"),
            ));
            return issues;
        }
    };

    let capabilities = datasource_capabilities(source.capabilities);

    if !capabilities.can_write_workflow {
        issues.push(validation_issue(
            WorkflowValidationIssueLevel::Error,
            step_id,
            "datasource_not_workflow_writable",
            format!(
                "Datasource '{}' is not eligible for workflow loading",
                datasource_id
            ),
            Some("datasource_id"),
        ));
    }

    if matches!(config.mode, LoadMode::Upsert)
        && config
            .key_fields
            .as_ref()
            .map(|fields| fields.iter().any(|field| !field.trim().is_empty()))
            != Some(true)
    {
        issues.push(validation_issue(
            WorkflowValidationIssueLevel::Error,
            step_id,
            "missing_key_fields",
            "db_loader in upsert mode requires one or more key_fields",
            Some("key_fields"),
        ));
    }

    if !capabilities.can_infer_schema || config.table_name.trim().is_empty() {
        return issues;
    }

    match catalog
        .infer_schema(datasource_id, Some(config.table_name.as_str()), 1000)
        .await
    {
        Ok(schema) => {
            if let Some(table) = schema
                .tables
                .iter()
                .find(|table| table_name_matches(&table.name, &config.table_name))
            {
                if matches!(config.mode, LoadMode::Upsert) {
                    let target_columns = table
                        .columns
                        .iter()
                        .map(|column| normalize_identifier_variants(&column.name))
                        .collect::<Vec<_>>();

                    let invalid_keys = config
                        .key_fields
                        .clone()
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|key_field| {
                            let key_variants = normalize_identifier_variants(key_field);
                            !target_columns.iter().any(|column_variants| {
                                column_variants
                                    .iter()
                                    .any(|variant| key_variants.contains(variant))
                            })
                        })
                        .collect::<Vec<_>>();

                    if !invalid_keys.is_empty() {
                        issues.push(validation_issue(
                            WorkflowValidationIssueLevel::Error,
                            step_id,
                            "invalid_key_fields",
                            format!(
                                "Key fields not found in target table '{}': {}",
                                config.table_name,
                                invalid_keys.join(", ")
                            ),
                            Some("key_fields"),
                        ));
                    }
                }
            } else {
                issues.push(validation_issue(
                    WorkflowValidationIssueLevel::Error,
                    step_id,
                    "target_table_not_found",
                    format!(
                        "Target table '{}' could not be found for datasource '{}'",
                        config.table_name, datasource_id
                    ),
                    Some("table_name"),
                ));
            }
        }
        Err(error) => {
            issues.push(validation_issue(
                WorkflowValidationIssueLevel::Error,
                step_id,
                "target_schema_inference_failed",
                format!(
                    "Failed to verify target table '{}' for datasource '{}': {}",
                    config.table_name, datasource_id, error
                ),
                Some("table_name"),
            ));
        }
    }

    issues
}

pub(crate) async fn build_workflow_validation_response(
    state: &ApiState,
    definition: &WorkflowDefinition,
) -> Result<ValidateWorkflowResponse, (StatusCode, String)> {
    let engine = state.workflow_engine.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Workflow engine not available".to_string(),
        )
    })?;

    let mut issues = validate_definition_shape(definition);
    let mut warnings = Vec::new();

    if let Err(error) = engine.validate_workflow(definition) {
        if issues.is_empty() {
            issues.push(validation_issue(
                WorkflowValidationIssueLevel::Error,
                WORKFLOW_ISSUE_STEP_ID,
                "workflow_validation_failed",
                error.to_string(),
                None,
            ));
        }
    }

    if let Some(catalog) = state.datasource_catalog.as_ref() {
        issues.extend(validate_datasource_backed_steps(catalog.as_ref(), definition).await);
    } else {
        warnings.push(
            "Datasource catalog unavailable; datasource-backed runtime validation was skipped"
                .to_string(),
        );
    }

    let valid = !issues
        .iter()
        .any(|issue| issue.level == WorkflowValidationIssueLevel::Error);

    Ok(ValidateWorkflowResponse {
        valid,
        message: if valid {
            "Workflow definition is valid".to_string()
        } else {
            "Workflow definition has validation issues".to_string()
        },
        warnings,
        step_count: definition.steps.len(),
        has_conditional_logic: false,
        has_error_handling: false,
        issues,
    })
}

fn summarize_validation_failure(action: &str, response: &ValidateWorkflowResponse) -> String {
    let mut messages = response
        .issues
        .iter()
        .filter(|issue| issue.level == WorkflowValidationIssueLevel::Error)
        .map(|issue| {
            if issue.step_id == WORKFLOW_ISSUE_STEP_ID {
                issue.message.clone()
            } else {
                format!("{}: {}", issue.step_id, issue.message)
            }
        })
        .take(3)
        .collect::<Vec<_>>();

    if messages.is_empty() && !response.message.trim().is_empty() {
        messages.push(response.message.clone());
    }

    if messages.is_empty() {
        format!("Workflow {} blocked by validation issues", action)
    } else {
        format!(
            "Workflow {} blocked by validation issues: {}",
            action,
            messages.join("; ")
        )
    }
}

pub(crate) async fn ensure_workflow_ready_for_action(
    state: &ApiState,
    definition: &WorkflowDefinition,
    action: &str,
) -> Result<(), (StatusCode, String)> {
    let response = build_workflow_validation_response(state, definition).await?;

    if response.valid {
        return Ok(());
    }

    Err((
        StatusCode::BAD_REQUEST,
        summarize_validation_failure(action, &response),
    ))
}

// ============================================================================
// Workflow CRUD Handlers
// ============================================================================

#[utoipa::path(
    post,
    path = "/api/v1/workflows",
    request_body = RegisterWorkflowRequest,
    responses(
        (status = 200, description = "Workflow registered successfully. Returns the generated workflow ID and creation timestamp. Use this ID for execution, scheduling, and management operations.", body = RegisterWorkflowResponse),
        (status = 409, description = "Workflow already exists. A workflow with the requested ID is already registered.", body = String),
        (status = 400, description = "Invalid workflow definition. The workflow validation failed due to malformed steps, invalid dependencies, missing required fields, or unsupported transformer types.", body = String),
        (status = 500, description = "Internal server error. Failed to persist the workflow to the workflow engine storage.", body = String),
    ),
    tag = "Workflow Orchestration"
)]
/// Register a new workflow
/// POST /api/v1/workflows
pub async fn register_workflow(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<RegisterWorkflowRequest>,
) -> Result<Json<RegisterWorkflowResponse>, (StatusCode, String)> {
    let engine = state.workflow_engine.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Workflow engine not available".to_string(),
        )
    })?;

    if request.name.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Workflow name cannot be empty".to_string(),
        ));
    }

    let workflow_id = request
        .id
        .unwrap_or_else(|| format!("wf_{}", uuid::Uuid::new_v4()));

    request
        .definition
        .validate()
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid workflow: {}", e)))?;

    engine
        .register_workflow(
            workflow_id.clone(),
            request.name.clone(),
            request.definition,
            request.description.clone(),
            request.tags.clone(),
        )
        .await
        .map_err(|e| map_workflow_engine_error("Registration", e))?;

    if let Some(rdf_store) = state.rdf_store.as_ref() {
        let persistence = WorkflowResultPersistence::new(rdf_store.clone());
        if let Err(error) = persistence
            .persist_workflow_definition(&workflow_id, &request.name, "1.0.0")
            .await
        {
            tracing::warn!(
                workflow_id = workflow_id,
                "Failed to persist workflow definition RDF during registration: {}",
                error
            );
        }
    }

    Ok(Json(RegisterWorkflowResponse {
        workflow_id,
        name: request.name,
        created_at: Utc::now(),
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/workflows",
    responses(
        (status = 200, description = "Successfully retrieved list of all registered workflows. Returns workflow summaries including ID, name, description, tags, and creation timestamp. Returns an empty array when no workflows are registered.", body = Vec<WorkflowSummaryDto>),
        (status = 503, description = "Workflow engine not available. The workflow service is not initialized.", body = String),
        (status = 500, description = "Internal server error. Failed to retrieve workflows from the workflow engine storage.", body = String),
    ),
    tag = "Workflow Orchestration"
)]
/// List all workflows
/// GET /api/v1/workflows
pub async fn list_workflows(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<Vec<WorkflowSummaryDto>>, (StatusCode, String)> {
    let engine = state.workflow_engine.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Workflow engine not available".to_string(),
        )
    })?;

    let workflows = engine.list_workflows().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to list workflows: {}", e),
        )
    })?;

    let summaries = workflows
        .into_iter()
        .map(|(_, metadata)| WorkflowSummaryDto {
            workflow_id: metadata.id,
            name: metadata.name,
            description: metadata.description,
            tags: metadata.tags,
            created_at: metadata.created_at,
        })
        .collect();

    Ok(Json(summaries))
}

#[utoipa::path(
    get,
    path = "/api/v1/workflows/{id}",
    params(
        ("id" = String, Path, description = "Unique workflow identifier")
    ),
    responses(
        (status = 200, description = "Successfully retrieved workflow definition. Returns complete workflow specification including all steps, dependencies, transformers, fusion threshold, and fallback strategy. Use this definition for inspection, cloning, or version comparison.", body = WorkflowDefinition),
        (status = 404, description = "Workflow not found. No workflow exists with the specified ID.", body = String),
        (status = 500, description = "Internal server error. Failed to retrieve workflow from storage.", body = String),
    ),
    tag = "Workflow Orchestration"
)]
/// Get workflow definition
/// GET /api/v1/workflows/:id
pub async fn get_workflow(
    State(state): State<Arc<ApiState>>,
    Path(workflow_id): Path<String>,
) -> Result<Json<WorkflowDefinition>, (StatusCode, String)> {
    let engine = state.workflow_engine.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Workflow engine not available".to_string(),
        )
    })?;

    let workflow = engine
        .get_workflow(&workflow_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to get workflow: {}", e),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("Workflow not found: {}", workflow_id),
            )
        })?;

    Ok(Json(workflow.definition))
}

#[utoipa::path(
    get,
    path = "/api/v1/workflows/{id}/details",
    params(
        ("id" = String, Path, description = "Unique workflow identifier")
    ),
    responses(
        (status = 200, description = "Successfully retrieved workflow metadata and definition.", body = WorkflowDetailsResponse),
        (status = 404, description = "Workflow not found. No workflow exists with the specified ID.", body = String),
        (status = 503, description = "Workflow engine not available. The workflow service is not initialized.", body = String),
        (status = 500, description = "Internal server error. Failed to retrieve workflow from storage.", body = String),
    ),
    tag = "Workflow Orchestration"
)]
/// Get workflow metadata and definition
/// GET /api/v1/workflows/:id/details
pub async fn get_workflow_details(
    State(state): State<Arc<ApiState>>,
    Path(workflow_id): Path<String>,
) -> Result<Json<WorkflowDetailsResponse>, (StatusCode, String)> {
    let engine = state.workflow_engine.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Workflow engine not available".to_string(),
        )
    })?;

    let workflow = engine
        .get_workflow(&workflow_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to get workflow details: {}", e),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("Workflow not found: {}", workflow_id),
            )
        })?;

    Ok(Json(WorkflowDetailsResponse {
        workflow_id: workflow.id,
        name: workflow.name,
        description: workflow.description,
        tags: workflow.tags,
        created_at: workflow.created_at,
        version: workflow.version,
        execution_count: workflow.execution_count,
        last_executed_at: workflow.last_executed_at,
        definition: workflow.definition,
    }))
}

#[utoipa::path(
    put,
    path = "/api/v1/workflows/{id}",
    params(
        ("id" = String, Path, description = "Unique workflow identifier to update")
    ),
    request_body = RegisterWorkflowRequest,
    responses(
        (status = 200, description = "Workflow updated successfully. Returns updated workflow metadata. Note: This replaces the entire workflow definition. Active schedules and executions continue using the updated definition.", body = RegisterWorkflowResponse),
        (status = 400, description = "Invalid workflow definition. The updated workflow failed validation due to malformed steps, invalid dependencies, or unsupported transformers.", body = String),
        (status = 404, description = "Workflow not found. No workflow exists with the specified ID to update.", body = String),
        (status = 500, description = "Internal server error. Failed to persist the updated workflow.", body = String),
    ),
    tag = "Workflow Orchestration"
)]
/// Update workflow
/// PUT /api/v1/workflows/:id
pub async fn update_workflow(
    State(state): State<Arc<ApiState>>,
    Path(workflow_id): Path<String>,
    Json(request): Json<RegisterWorkflowRequest>,
) -> Result<Json<RegisterWorkflowResponse>, (StatusCode, String)> {
    let engine = state.workflow_engine.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Workflow engine not available".to_string(),
        )
    })?;

    if request.name.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Workflow name cannot be empty".to_string(),
        ));
    }

    request
        .definition
        .validate()
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid workflow: {}", e)))?;

    let metadata = engine
        .update_workflow(
            &workflow_id,
            request.name,
            request.description,
            request.tags,
            request.definition,
        )
        .await
        .map_err(|e| map_workflow_engine_error("Update", e))?;

    Ok(Json(RegisterWorkflowResponse {
        workflow_id: metadata.id,
        name: metadata.name,
        created_at: metadata.created_at,
    }))
}

#[utoipa::path(
    delete,
    path = "/api/v1/workflows/{id}",
    params(
        ("id" = String, Path, description = "Unique workflow identifier to delete")
    ),
    responses(
        (status = 204, description = "Workflow deleted successfully. The workflow and all associated metadata have been removed. Note: Active schedules for this workflow should be deleted separately. Execution history may be retained depending on retention policy.", body = ()),
        (status = 404, description = "Workflow not found. No workflow exists with the specified ID.", body = String),
        (status = 500, description = "Internal server error. Failed to delete the workflow from storage.", body = String),
    ),
    tag = "Workflow Orchestration"
)]
/// Delete workflow
/// DELETE /api/v1/workflows/:id
pub async fn delete_workflow(
    State(state): State<Arc<ApiState>>,
    Path(workflow_id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let engine = state.workflow_engine.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Workflow engine not available".to_string(),
        )
    })?;

    engine
        .delete_workflow(&workflow_id)
        .await
        .map_err(|e| map_workflow_engine_error("Delete", e))?;

    Ok(StatusCode::NO_CONTENT)
}

// ============================================================================
// Workflow Execution Handlers
// ============================================================================

#[utoipa::path(
    post,
    path = "/api/v1/workflows/{id}/execute",
    params(
        ("id" = String, Path, description = "Unique workflow identifier to execute")
    ),
    request_body = ExecuteWorkflowRequest,
    responses(
        (status = 200, description = "Workflow executed successfully. Returns comprehensive execution results including per-step outputs, execution times, confidence scores, and final workflow output. For graph-native inputs (SPARQL, EntityFilter), may return multiple batched results if the input query produces multiple result sets. Check batch_count and overall_success for multi-batch execution status.", body = ExecuteWorkflowResponse),
        (status = 400, description = "Invalid execution request. Causes: unsupported input type (only sparql_query, entity_filter, json supported), malformed input specification, missing required context parameters, or invalid workflow input adapter configuration.", body = String),
        (status = 404, description = "Workflow not found. No workflow exists with the specified ID.", body = String),
        (status = 500, description = "Workflow execution failed. The workflow engine encountered an error during step execution, transformer invocation, or result persistence. Check the error message for specific failure details.", body = String),
        (status = 503, description = "Service unavailable. Either the workflow engine is not initialized or the query executor is unavailable (required for graph-native SPARQL/EntityFilter inputs in distributed mode with shards).", body = String),
    ),
    tag = "Workflow Orchestration"
)]
/// Execute a workflow and persist results
/// POST /api/v1/workflows/:id/execute
pub async fn execute_workflow(
    State(state): State<Arc<ApiState>>,
    Path(workflow_id): Path<String>,
    Json(request): Json<ExecuteWorkflowRequest>,
) -> Result<Json<ExecuteWorkflowResponse>, (StatusCode, String)> {
    let engine = state.workflow_engine.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Workflow engine not available".to_string(),
        )
    })?;

    // Get workflow definition
    let workflow = engine
        .get_workflow(&workflow_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to get workflow: {}", e),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("Workflow not found: {}", workflow_id),
            )
        })?;

    ensure_workflow_ready_for_action(state.as_ref(), &workflow.definition, "execution").await?;

    let execution_input =
        serde_json::to_value(&request.input).unwrap_or_else(|_| serde_json::Value::Null);
    let triggered_by = request.context.initiator.clone();

    // Convert input wrapper to WorkflowInput
    let workflow_input = request.input.into_workflow_input();

    // Create appropriate input adapter based on input type
    let adapter: Arc<dyn InputAdapter> = build_input_adapter(state.clone(), &workflow_input)?;

    // Execute workflow with input adapter
    let started_at = Utc::now();
    let context_map = request.context.to_hashmap();

    let results = engine
        .execute_workflow_with_input(&workflow_id, workflow_input, adapter, &context_map)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Workflow execution failed: {}", e),
            )
        })?;

    let completed_at = Utc::now();

    let batch_count = results.len();
    let mut result_dtos = Vec::with_capacity(batch_count);
    for (batch_index, result) in results.into_iter().enumerate() {
        let execution_result = finalize_execution_result(
            state.clone(),
            &workflow_id,
            &workflow.name,
            &result,
            request.output_dataset.as_ref(),
            batch_index,
            batch_count,
        )
        .await?;

        if let Err(error) = persist_execution_record_if_possible(
            state.as_ref(),
            &workflow_id,
            &workflow.name,
            &execution_input,
            triggered_by.as_deref(),
            &result,
            &execution_result,
        )
        .await
        {
            tracing::warn!(
                workflow_id = workflow_id,
                execution_id = result.execution_id,
                "Failed to persist workflow execution history: {}",
                error
            );
        }

        result_dtos.push(execution_result);
    }

    // Build batched response
    let response =
        ExecuteWorkflowResponse::batched(workflow_id, result_dtos, started_at, completed_at);

    Ok(Json(response))
}

// ============================================================================
// Workflow Validation & Testing Handlers
// ============================================================================

#[utoipa::path(
    post,
    path = "/api/v1/workflows/validate",
    request_body = WorkflowDefinition,
    responses(
        (status = 200, description = "Workflow definition validation completed. Returns shape validation plus datasource-aware issues for db_extract and db_loader steps when the datasource catalog is available. The response stays successful even when validation finds blocking issues; inspect valid=false and issues[].", body = ValidateWorkflowResponse),
        (status = 503, description = "Workflow engine not available. The validation service is not initialized.", body = String),
    ),
    tag = "Workflow Orchestration"
)]
/// Validate a workflow definition without registering
/// POST /api/v1/workflows/validate
pub async fn validate_workflow_definition(
    State(state): State<Arc<ApiState>>,
    Json(definition): Json<WorkflowDefinition>,
) -> Result<Json<ValidateWorkflowResponse>, (StatusCode, String)> {
    Ok(Json(
        build_workflow_validation_response(state.as_ref(), &definition).await?,
    ))
}

#[utoipa::path(
    post,
    path = "/api/v1/workflows/test-step",
    request_body = TestWorkflowStepRequest,
    responses(
        (status = 200, description = "Step test completed. Returns execution results including success status, output (if successful), error message (if failed), execution time in milliseconds, and step type. Use this endpoint to test individual workflow steps in isolation before integrating into full workflows, verify transformer configurations, or debug step failures with sample data.", body = TestWorkflowStepResponse),
        (status = 400, description = "Invalid step definition. The step failed validation due to malformed configuration, invalid transformer type, missing required parameters, or unsupported step type.", body = String),
        (status = 503, description = "Workflow engine not available. The step testing service is not initialized.", body = String),
    ),
    tag = "Workflow Orchestration"
)]
/// Test a single workflow step with sample input
/// POST /api/v1/workflows/test-step
pub async fn test_workflow_step(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<TestWorkflowStepRequest>,
) -> Result<Json<TestWorkflowStepResponse>, (StatusCode, String)> {
    let engine = state.workflow_engine.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Workflow engine not available".to_string(),
        )
    })?;

    let start = std::time::Instant::now();

    // Create a temporary single-step workflow for testing
    let test_workflow = WorkflowDefinition {
        steps: vec![request.step.clone()],
        fusion_threshold: 0.8,
        fallback:
            graphica_core::orchestration::workflow::definition::FallbackStrategy::ManualReview,
    };

    // Validate the step
    engine.validate_workflow(&test_workflow).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            format!("Invalid step definition: {}", e),
        )
    })?;

    // Execute the step (dry-run mode)
    let context_map = request.context.to_hashmap();
    let result = engine
        .execute_step(&request.step, request.input, &context_map)
        .await;

    let execution_time_ms = start.elapsed().as_millis() as u64;

    match result {
        Ok(output) => Ok(Json(TestWorkflowStepResponse {
            success: true,
            output: Some(output),
            error: None,
            execution_time_ms,
            step_type: request.step.step_type.to_string(),
        })),
        Err(e) => Ok(Json(TestWorkflowStepResponse {
            success: false,
            output: None,
            error: Some(e.to_string()),
            execution_time_ms,
            step_type: request.step.step_type.to_string(),
        })),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/workflows/dry-run",
    request_body = DryRunWorkflowRequest,
    responses(
        (status = 200, description = "Dry-run completed. Returns comprehensive execution simulation including success status, detailed results for each executed step (step_id, type, success, output, error, execution_time_ms), final workflow output (if all steps succeeded), total execution time, and collected error messages. No results are persisted to storage. Use this endpoint to test complete workflow definitions end-to-end before registration, validate data transformations, estimate execution time, or debug multi-step workflows with production-like data.", body = DryRunWorkflowResponse),
        (status = 400, description = "Invalid workflow definition or input. The workflow failed validation, input data is malformed, or required context parameters are missing.", body = String),
        (status = 503, description = "Workflow engine not available. The dry-run service is not initialized.", body = String),
    ),
    tag = "Workflow Orchestration"
)]
/// Dry-run a complete workflow without persisting results
/// POST /api/v1/workflows/dry-run
pub async fn dry_run_workflow(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<DryRunWorkflowRequest>,
) -> Result<Json<DryRunWorkflowResponse>, (StatusCode, String)> {
    let engine = state.workflow_engine.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Workflow engine not available".to_string(),
        )
    })?;

    ensure_workflow_ready_for_action(state.as_ref(), &request.definition, "dry-run").await?;

    let total_start = std::time::Instant::now();

    // Validate workflow definition
    request
        .definition
        .validate()
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid workflow: {}", e)))?;

    // Execute each step in sequence (simplified DAG execution)
    let mut steps_executed = Vec::new();
    let mut errors = Vec::new();
    let mut current_input = request.input.clone();

    let context_map = request.context.to_hashmap();

    for step in &request.definition.steps {
        let step_start = std::time::Instant::now();

        match engine
            .execute_step(step, current_input.clone(), &context_map)
            .await
        {
            Ok(output) => {
                steps_executed.push(StepExecutionResult {
                    step_id: step.id.clone(),
                    step_type: step.step_type.to_string(),
                    success: true,
                    output: Some(output.clone()),
                    error: None,
                    execution_time_ms: step_start.elapsed().as_millis() as u64,
                });
                current_input = output;
            }
            Err(e) => {
                errors.push(format!("Step '{}' failed: {}", step.id, e));
                steps_executed.push(StepExecutionResult {
                    step_id: step.id.clone(),
                    step_type: step.step_type.to_string(),
                    success: false,
                    output: None,
                    error: Some(e.to_string()),
                    execution_time_ms: step_start.elapsed().as_millis() as u64,
                });
                break; // Stop on first error
            }
        }
    }

    Ok(Json(DryRunWorkflowResponse {
        success: errors.is_empty(),
        steps_executed,
        final_output: if errors.is_empty() {
            Some(current_input)
        } else {
            None
        },
        total_execution_time_ms: total_start.elapsed().as_millis() as u64,
        errors,
    }))
}

// ============================================================================
// Workflow Scheduling Handlers
// ============================================================================

#[utoipa::path(
    post,
    path = "/api/v1/workflows/{id}/schedules",
    params(
        ("id" = String, Path, description = "Unique workflow identifier to schedule")
    ),
    request_body = ScheduleWorkflowRequest,
    responses(
        (status = 200, description = "Schedule created successfully. Returns schedule metadata including generated schedule_id, workflow_id, scheduling configuration (cron expression, interval, one-time scheduled_at), IANA timezone (e.g., 'America/New_York', 'UTC'), calculated next_execution timestamp in UTC, enabled status, and creation timestamp. Supports three scheduling modes: cron expressions for complex schedules, interval_seconds for simple recurring execution, or scheduled_at for one-time execution. Timezone support ensures schedules execute at correct local times across regions.", body = ScheduleWorkflowResponse),
        (status = 400, description = "Invalid schedule request. Causes: invalid cron expression syntax, invalid IANA timezone identifier, missing scheduling configuration (must specify at least one: cron_expression, interval_seconds, or scheduled_at), or failed to calculate next execution time.", body = String),
        (status = 404, description = "Workflow not found. No workflow exists with the specified ID to schedule.", body = String),
        (status = 500, description = "Internal server error. Failed to persist the schedule to storage.", body = String),
        (status = 503, description = "Service unavailable. Either the schedule store or workflow engine is not initialized.", body = String),
    ),
    tag = "Workflow Orchestration"
)]
/// Create a new schedule for a workflow (plural endpoint)
/// POST /api/v1/workflows/:id/schedules
pub async fn create_schedule(
    State(state): State<Arc<ApiState>>,
    Path(workflow_id): Path<String>,
    Json(request): Json<ScheduleWorkflowRequest>,
) -> Result<Json<ScheduleWorkflowResponse>, (StatusCode, String)> {
    use crate::workflows::domain::{calculate_next_execution, validate_timezone, WorkflowSchedule};

    let schedule_store = state.schedule_store.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Schedule store not available".to_string(),
        )
    })?;

    let engine = state.workflow_engine.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Workflow engine not available".to_string(),
        )
    })?;

    // Verify workflow exists
    let workflow = engine
        .get_workflow(&workflow_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to get workflow: {}", e),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("Workflow not found: {}", workflow_id),
            )
        })?;

    ensure_workflow_ready_for_action(state.as_ref(), &workflow.definition, "scheduling").await?;

    // Generate schedule ID
    let schedule_id = request
        .schedule_id
        .unwrap_or_else(|| format!("sched_{}", uuid::Uuid::new_v4()));

    // Validate timezone
    let timezone = request.timezone.unwrap_or_else(|| "UTC".to_string());
    if !validate_timezone(&timezone) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("Invalid timezone: {}", timezone),
        ));
    }

    // Calculate next execution time
    let now = Utc::now();
    let next_execution = if request.enabled {
        calculate_next_execution(
            request.cron_expression.as_deref(),
            request.interval_seconds,
            request.scheduled_at,
            &timezone,
            now,
        )
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                format!("Failed to calculate next execution: {}", e),
            )
        })?
    } else {
        None
    };

    // Create schedule
    let mut schedule = WorkflowSchedule::new(
        schedule_id.clone(),
        workflow_id.clone(),
        workflow.name.clone(),
        request.cron_expression.clone(),
        request.interval_seconds,
        request.scheduled_at,
        timezone.clone(),
        request.input,
        serde_json::to_value(&request.context).unwrap_or(serde_json::json!({})),
        request.enabled,
    );

    // Set next_run in the schedule
    schedule.set_next_run(next_execution);

    schedule_store.create(schedule.clone()).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to create schedule: {}", e),
        )
    })?;

    Ok(Json(ScheduleWorkflowResponse {
        schedule_id,
        workflow_id,
        cron_expression: request.cron_expression,
        interval_seconds: request.interval_seconds,
        scheduled_at: request.scheduled_at,
        timezone: Some(timezone),
        next_execution,
        enabled: request.enabled,
        created_at: schedule.created_at,
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/workflows/{id}/schedules",
    params(
        ("id" = String, Path, description = "Unique workflow identifier")
    ),
    responses(
        (status = 200, description = "Successfully retrieved all schedules for the workflow. Returns array of schedule information including schedule_id, workflow_id, scheduling configuration (cron/interval/scheduled_at), IANA timezone, next_execution timestamp, last_execution timestamp, enabled status, creation timestamp, and execution_count. Empty array if no schedules exist for the workflow.", body = Vec<WorkflowScheduleInfo>),
        (status = 404, description = "Workflow not found. No workflow exists with the specified ID.", body = String),
        (status = 500, description = "Internal server error. Failed to retrieve schedules from storage.", body = String),
        (status = 503, description = "Schedule store not available. The scheduling service is not initialized.", body = String),
    ),
    tag = "Workflow Orchestration"
)]
/// List all schedules for a workflow
/// GET /api/v1/workflows/:id/schedules
pub async fn list_schedules(
    State(state): State<Arc<ApiState>>,
    Path(workflow_id): Path<String>,
) -> Result<Json<Vec<WorkflowScheduleInfo>>, (StatusCode, String)> {
    let schedule_store = state.schedule_store.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Schedule store not available".to_string(),
        )
    })?;

    let schedules = schedule_store.list_by_workflow(&workflow_id).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to list schedules: {}", e),
        )
    })?;

    // Convert to WorkflowScheduleInfo
    let schedule_infos: Vec<WorkflowScheduleInfo> = schedules
        .into_iter()
        .map(|s| WorkflowScheduleInfo {
            schedule_id: s.schedule_id,
            workflow_id: s.workflow_id,
            cron_expression: s.cron_expression,
            interval_seconds: s.interval_seconds,
            scheduled_at: s.scheduled_at,
            timezone: Some(s.timezone),
            next_execution: s.next_run,
            last_execution: s.last_run,
            enabled: s.enabled,
            created_at: s.created_at,
            execution_count: s.execution_count,
        })
        .collect();

    Ok(Json(schedule_infos))
}

#[utoipa::path(
    get,
    path = "/api/v1/workflows/{id}/schedules/{schedule_id}",
    params(
        ("id" = String, Path, description = "Unique workflow identifier"),
        ("schedule_id" = String, Path, description = "Unique schedule identifier")
    ),
    responses(
        (status = 200, description = "Successfully retrieved schedule details. Returns complete schedule information including schedule_id, workflow_id, scheduling configuration (cron/interval/scheduled_at), IANA timezone, next_execution timestamp, last_execution timestamp, enabled status, creation timestamp, and execution_count. Use this endpoint to inspect schedule status, verify next execution time, or check execution history.", body = WorkflowScheduleInfo),
        (status = 404, description = "Schedule not found. No schedule exists with the specified schedule_id.", body = String),
        (status = 500, description = "Internal server error. Failed to retrieve schedule from storage.", body = String),
        (status = 503, description = "Schedule store not available. The scheduling service is not initialized.", body = String),
    ),
    tag = "Workflow Orchestration"
)]
/// Get a specific schedule by ID
/// GET /api/v1/workflows/:id/schedules/:schedule_id
pub async fn get_schedule(
    State(state): State<Arc<ApiState>>,
    Path((_workflow_id, schedule_id)): Path<(String, String)>,
) -> Result<Json<WorkflowScheduleInfo>, (StatusCode, String)> {
    let schedule_store = state.schedule_store.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Schedule store not available".to_string(),
        )
    })?;

    let schedule = schedule_store
        .get(&schedule_id)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to get schedule: {}", e),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("Schedule {} not found", schedule_id),
            )
        })?;

    Ok(Json(WorkflowScheduleInfo {
        schedule_id: schedule.schedule_id,
        workflow_id: schedule.workflow_id,
        cron_expression: schedule.cron_expression,
        interval_seconds: schedule.interval_seconds,
        scheduled_at: schedule.scheduled_at,
        timezone: Some(schedule.timezone),
        next_execution: schedule.next_run,
        last_execution: schedule.last_run,
        enabled: schedule.enabled,
        created_at: schedule.created_at,
        execution_count: schedule.execution_count,
    }))
}

#[utoipa::path(
    put,
    path = "/api/v1/workflows/{id}/schedules/{schedule_id}",
    params(
        ("id" = String, Path, description = "Unique workflow identifier"),
        ("schedule_id" = String, Path, description = "Unique schedule identifier to update")
    ),
    request_body = UpdateScheduleRequest,
    responses(
        (status = 200, description = "Schedule updated successfully. Returns updated schedule information with recalculated next_execution timestamp. All fields in UpdateScheduleRequest are optional - only provided fields will be updated. Updating scheduling configuration (cron/interval/scheduled_at) or timezone automatically recalculates next_execution. Disabling a schedule (enabled=false) clears next_execution. Use this endpoint to modify schedule timing, change input data or context, enable/disable schedules, or adjust timezone for cross-region execution.", body = WorkflowScheduleInfo),
        (status = 400, description = "Invalid update request. Causes: invalid cron expression syntax, invalid IANA timezone identifier, or failed to recalculate next execution time after update.", body = String),
        (status = 404, description = "Schedule not found. No schedule exists with the specified schedule_id.", body = String),
        (status = 500, description = "Internal server error. Failed to persist the updated schedule.", body = String),
        (status = 503, description = "Schedule store not available. The scheduling service is not initialized.", body = String),
    ),
    tag = "Workflow Orchestration"
)]
/// Update an existing schedule
/// PUT /api/v1/workflows/:id/schedules/:schedule_id
pub async fn update_schedule(
    State(state): State<Arc<ApiState>>,
    Path((_workflow_id, schedule_id)): Path<(String, String)>,
    Json(request): Json<UpdateScheduleRequest>,
) -> Result<Json<WorkflowScheduleInfo>, (StatusCode, String)> {
    use crate::workflows::domain::{calculate_next_execution, validate_timezone};

    let schedule_store = state.schedule_store.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Schedule store not available".to_string(),
        )
    })?;

    // Get existing schedule
    let mut schedule = schedule_store
        .get_required(&schedule_id)
        .map_err(|e| (StatusCode::NOT_FOUND, format!("Schedule not found: {}", e)))?;

    // Validate timezone if provided
    let timezone = request.timezone.unwrap_or(schedule.timezone.clone());
    if !validate_timezone(&timezone) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("Invalid timezone: {}", timezone),
        ));
    }

    let new_enabled = request.enabled.unwrap_or(schedule.enabled);

    // Update schedule
    schedule.update(
        request.cron_expression.or(schedule.cron_expression.clone()),
        request.interval_seconds.or(schedule.interval_seconds),
        request.scheduled_at.or(schedule.scheduled_at),
        timezone.clone(),
        request.input,
        request
            .context
            .map(|c| serde_json::to_value(&c).unwrap_or(serde_json::json!({}))),
        new_enabled,
    );

    // Recalculate next execution time after update
    let now = Utc::now();
    let next_execution = if new_enabled {
        calculate_next_execution(
            schedule.cron_expression.as_deref(),
            schedule.interval_seconds,
            schedule.scheduled_at,
            &timezone,
            now,
        )
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                format!("Failed to calculate next execution: {}", e),
            )
        })?
    } else {
        None
    };

    // Update next_run in the schedule
    schedule.set_next_run(next_execution);

    schedule_store.update(schedule.clone()).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to update schedule: {}", e),
        )
    })?;

    Ok(Json(WorkflowScheduleInfo {
        schedule_id: schedule.schedule_id,
        workflow_id: schedule.workflow_id,
        cron_expression: schedule.cron_expression,
        interval_seconds: schedule.interval_seconds,
        scheduled_at: schedule.scheduled_at,
        timezone: Some(schedule.timezone),
        next_execution: schedule.next_run,
        last_execution: schedule.last_run,
        enabled: schedule.enabled,
        created_at: schedule.created_at,
        execution_count: schedule.execution_count,
    }))
}

#[utoipa::path(
    delete,
    path = "/api/v1/workflows/{id}/schedules/{schedule_id}",
    params(
        ("id" = String, Path, description = "Unique workflow identifier"),
        ("schedule_id" = String, Path, description = "Unique schedule identifier to delete")
    ),
    responses(
        (status = 204, description = "Schedule deleted successfully. The schedule has been removed and will no longer trigger workflow executions. Any in-flight executions from this schedule will complete normally. Use this endpoint to remove obsolete schedules or cancel future workflow executions.", body = ()),
        (status = 404, description = "Schedule not found. No schedule exists with the specified schedule_id.", body = String),
        (status = 500, description = "Internal server error. Failed to delete the schedule from storage.", body = String),
        (status = 503, description = "Schedule store not available. The scheduling service is not initialized.", body = String),
    ),
    tag = "Workflow Orchestration"
)]
/// Delete a specific schedule
/// DELETE /api/v1/workflows/:id/schedules/:schedule_id
pub async fn delete_schedule(
    State(state): State<Arc<ApiState>>,
    Path((_workflow_id, schedule_id)): Path<(String, String)>,
) -> Result<StatusCode, (StatusCode, String)> {
    let schedule_store = state.schedule_store.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Schedule store not available".to_string(),
        )
    })?;

    schedule_store.delete(&schedule_id).map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            format!("Failed to delete schedule: {}", e),
        )
    })?;

    Ok(StatusCode::NO_CONTENT)
}

// ============================================================================
// Execution History Handlers
// ============================================================================

#[utoipa::path(
    get,
    path = "/api/v1/workflows/{id}/executions",
    params(
        ("id" = String, Path, description = "Unique workflow identifier"),
        ("limit" = Option<i64>, Query, description = "Maximum number of execution records to return (pagination)"),
        ("offset" = Option<i64>, Query, description = "Number of execution records to skip (pagination)")
    ),
    responses(
        (status = 200, description = "Successfully retrieved workflow execution history from the execution store. Returns execution summaries ordered by most recent start time. Supports limit/offset pagination.", body = Vec<WorkflowExecutionSummary>),
        (status = 404, description = "Workflow not found. No workflow exists with the specified ID.", body = String),
        (status = 500, description = "Internal server error. Failed to retrieve execution history from the execution store.", body = String),
    ),
    tag = "Workflow Orchestration"
)]
/// List workflow execution history
/// GET /api/v1/workflows/:id/executions
pub async fn list_workflow_executions(
    State(state): State<Arc<ApiState>>,
    Path(workflow_id): Path<String>,
    Query(query): Query<ExecutionHistoryQuery>,
) -> Result<Json<Vec<WorkflowExecutionSummary>>, (StatusCode, String)> {
    let engine = state.workflow_engine.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Workflow engine not available".to_string(),
        )
    })?;

    engine
        .get_workflow(&workflow_id)
        .await
        .map_err(|error| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to get workflow: {}", error),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("Workflow not found: {}", workflow_id),
            )
        })?;

    let execution_store = state.execution_store.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Execution store not available".to_string(),
        )
    })?;

    let mut executions = execution_store
        .list_by_workflow(&workflow_id)
        .await
        .map_err(|error| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to retrieve execution history: {}", error),
            )
        })?;

    executions.sort_by(|left, right| right.started_at.cmp(&left.started_at));

    let offset = query.offset.unwrap_or(0);
    let limit = query.limit.unwrap_or(usize::MAX);

    let summaries = executions
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|execution| {
            let success = matches!(
                execution.status,
                crate::workflows::domain::ExecutionStatus::Completed
            );

            WorkflowExecutionSummary {
                execution_id: execution.execution_id,
                workflow_id: execution.workflow_id,
                started_at: execution.started_at,
                completed_at: execution.completed_at.unwrap_or(execution.updated_at),
                success,
                confidence: execution
                    .confidence
                    .unwrap_or(if success { 1.0 } else { 0.0 }),
            }
        })
        .collect();

    Ok(Json(summaries))
}

// ============================================================================
// Workflow Progress & Monitoring (Phase 3)
// ============================================================================

#[utoipa::path(
    get,
    path = "/api/v1/workflows/executions/{execution_id}/progress",
    params(
        ("execution_id" = String, Path, description = "Unique execution identifier")
    ),
    responses(
        (status = 200, description = "Successfully retrieved workflow execution progress. Returns real-time progress information including execution status (queued/running/completed/failed/cancelled), current step being executed, rows processed, percent complete, estimated time remaining (ETA), and last update timestamp. Progress is updated in real-time during workflow execution.", body = graphica_core::orchestration::workflow::progress::WorkflowProgress),
        (status = 404, description = "Execution not found. No execution exists with the specified ID.", body = String),
        (status = 500, description = "Internal server error. Failed to retrieve progress from progress store.", body = String),
    ),
    tag = "Workflow Orchestration"
)]
/// Get workflow execution progress
/// GET /api/v1/workflows/executions/:execution_id/progress
pub async fn get_execution_progress(
    State(state): State<Arc<ApiState>>,
    Path(execution_id): Path<String>,
) -> Result<
    Json<graphica_core::orchestration::workflow::progress::WorkflowProgress>,
    (StatusCode, String),
> {
    let progress_store = state.progress_store.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Progress tracking not available".to_string(),
        )
    })?;

    let progress = progress_store
        .get_progress(&execution_id)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to retrieve progress: {}", e),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("Execution not found: {}", execution_id),
            )
        })?;

    Ok(Json(progress))
}

#[utoipa::path(
    get,
    path = "/api/v1/workflows/{id}/executions/progress",
    params(
        ("id" = String, Path, description = "Unique workflow identifier")
    ),
    responses(
        (status = 200, description = "Successfully retrieved all execution progress records for the specified workflow. Returns array of progress snapshots sorted by started_at timestamp (most recent first). Includes both active (running/queued) and completed (completed/failed/cancelled) executions.", body = Vec<graphica_core::orchestration::workflow::progress::WorkflowProgress>),
        (status = 500, description = "Internal server error. Failed to retrieve progress from progress store.", body = String),
    ),
    tag = "Workflow Orchestration"
)]
/// List all executions for a workflow with progress
/// GET /api/v1/workflows/:id/executions/progress
pub async fn list_workflow_execution_progress(
    State(state): State<Arc<ApiState>>,
    Path(workflow_id): Path<String>,
) -> Result<
    Json<Vec<graphica_core::orchestration::workflow::progress::WorkflowProgress>>,
    (StatusCode, String),
> {
    let progress_store = state.progress_store.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Progress tracking not available".to_string(),
        )
    })?;

    let executions = progress_store
        .get_workflow_executions(&workflow_id)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to retrieve executions: {}", e),
            )
        })?;

    Ok(Json(executions))
}

#[utoipa::path(
    get,
    path = "/api/v1/workflows/executions/active",
    responses(
        (status = 200, description = "Successfully retrieved all active workflow executions. Returns array of progress snapshots for executions with status 'running' or 'queued', sorted by started_at timestamp (oldest first). Use this endpoint to monitor all in-flight workflow executions across all workflows.", body = Vec<graphica_core::orchestration::workflow::progress::WorkflowProgress>),
        (status = 500, description = "Internal server error. Failed to retrieve active executions from progress store.", body = String),
    ),
    tag = "Workflow Orchestration"
)]
/// Get all active workflow executions
/// GET /api/v1/workflows/executions/active
pub async fn get_active_executions(
    State(state): State<Arc<ApiState>>,
) -> Result<
    Json<Vec<graphica_core::orchestration::workflow::progress::WorkflowProgress>>,
    (StatusCode, String),
> {
    let progress_store = state.progress_store.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Progress tracking not available".to_string(),
        )
    })?;

    let active = progress_store.get_active_executions().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to retrieve active executions: {}", e),
        )
    })?;

    Ok(Json(active))
}

#[utoipa::path(
    delete,
    path = "/api/v1/workflows/executions/{execution_id}",
    params(
        ("execution_id" = String, Path, description = "Unique execution identifier to cancel")
    ),
    responses(
        (status = 200, description = "Workflow execution cancelled successfully. The cancellation signal has been sent to the running workflow. The workflow will stop gracefully at the next cancellation checkpoint (typically between steps or at yield points during row processing). Final status will be 'cancelled'.", body = String),
        (status = 404, description = "Execution not found. No execution exists with the specified ID, or the execution is not currently running.", body = String),
        (status = 500, description = "Internal server error. Failed to send cancellation signal.", body = String),
    ),
    tag = "Workflow Orchestration"
)]
/// Cancel a running workflow execution
/// DELETE /api/v1/workflows/executions/:execution_id
pub async fn cancel_execution(
    State(state): State<Arc<ApiState>>,
    Path(execution_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let cancellation_manager = state.cancellation_manager.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Cancellation not available".to_string(),
        )
    })?;

    cancellation_manager
        .cancel_execution(&execution_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to cancel execution: {}", e),
            )
        })?;

    Ok(Json(serde_json::json!({
        "message": format!("Execution {} cancelled successfully", execution_id),
        "execution_id": execution_id
    })))
}

#[cfg(test)]
mod tests {
    use super::{
        delete_workflow, get_workflow_details, list_workflow_executions, list_workflows,
        register_workflow, update_workflow, validate_workflow_definition, ExecutionHistoryQuery,
    };
    use crate::api::auth::AuthConfig;
    use crate::api::import_jobs::ImportJobManager;
    use crate::api::setup_token::SetupTokenManager;
    use crate::api::workflow::types::RegisterWorkflowRequest;
    use crate::api::ApiState;
    use crate::storage::LineageStorage;
    use crate::workflows::domain::{ExecutionStatus, WorkflowExecution};
    use crate::workflows::storage::ExecutionStore;
    use async_trait::async_trait;
    use axum::{
        extract::{Path, Query, State},
        http::StatusCode,
        Json,
    };
    use chrono::{TimeZone, Utc};
    use graphica_core::catalog::{
        api_types::{
            ColumnDefinition, ConnectionTestResult, DataSourceCapabilities, DataSourceResponse,
            DataSourceStatus, ListDataSourcesRequest, ListDataSourcesResponse, QueryResult,
            SchemaDefinition, TableDefinition, UpdateDataSourcePatch,
        },
        types::{ConnectionDetails, DataSource, DatabricksConfig, PostgreSQLConfig, SourceConfig},
        DataSourceCatalog,
    };
    use graphica_core::errors::GraphicaError;
    use graphica_core::orchestration::workflow::definition::{
        ConfidenceGateConfig, DbExtractConfig, DbLoaderConfig, FallbackStrategy, LoadMode,
        StepConfig, StepType,
    };
    use graphica_core::orchestration::workflow::{
        WorkflowDefinition, WorkflowEngine, WorkflowStep,
    };
    use std::{collections::HashMap, sync::Arc};
    use tempfile::TempDir;

    #[test]
    fn table_name_matches_catalog_and_schema_suffixes() {
        assert!(super::table_name_matches(
            "bronze.customers",
            "main.bronze.customers"
        ));
        assert!(super::table_name_matches(
            "`main`.`bronze`.`customers`",
            "bronze.customers"
        ));
        assert!(!super::table_name_matches(
            "main.bronze.orders",
            "main.bronze.customers"
        ));
    }

    fn create_test_api_state() -> (Arc<ApiState>, Arc<WorkflowEngine>, Arc<ExecutionStore>) {
        let temp_dir = TempDir::new().unwrap();
        let rocks_path = temp_dir.path().join("rocksdb");
        let parquet_path = temp_dir.path().join("parquet");
        let cold_path = temp_dir.path().join("cold");

        std::fs::create_dir_all(&rocks_path).unwrap();
        std::fs::create_dir_all(&parquet_path).unwrap();
        std::fs::create_dir_all(&cold_path).unwrap();

        let lineage_storage = LineageStorage::new_for_tests(
            rocks_path.to_str().unwrap(),
            parquet_path.to_str().unwrap(),
            cold_path.to_str().unwrap(),
        )
        .unwrap();

        let workflow_engine = Arc::new(WorkflowEngine::new());
        let execution_store = Arc::new(ExecutionStore::new());

        let state = Arc::new(ApiState {
            lineage_storage: Arc::new(lineage_storage),
            governance_brain: None,
            rdf_store: None,
            shard_registry: None,
            query_executor: None,
            workflow_engine: Some(workflow_engine.clone()),
            model_registry: None,
            model_cache: None,
            rule_executor: None,
            user_service: None,
            setup_token_manager: Arc::new(SetupTokenManager::new()),
            audit_logger: None,
            datasource_catalog: None,
            datasource_catalog_impl: None,
            persisted_ontology_registry: None,
            ontology_registry: None,
            rdf_storage: None,
            connector_registry: None,
            resolved_entity_cache: None,
            metrics_registry: None,
            import_job_manager: Arc::new(ImportJobManager::new()),
            mapping_engine: None,
            secret_store_registry: None,
            loader_job_manager: None,
            unified_mapping_coordinator: None,
            binding_service: None,
            schedule_store: None,
            workflow_store: None,
            execution_store: Some(execution_store.clone()),
            file_library: None,
            transformer_registry: None,
            kafka_producer: None,
            http_client: None,
            lineage_generator: None,
            metrics: None,
            replay_coordinator: None,
            row_lineage_store: None,
            column_lineage_store: None,
            schema_evolution_store: None,
            manual_mapping_store: None,
            db2_pool: None,
            checkpoint_persistence: None,
            dlq_reader: None,
            dlq_reprocessor: None,
            dlq_stats_calculator: None,
            schema_version_store: None,
            policy_checker: None,
            execution_sync: None,
            approval_store: None,
            gdpr_coordinator: None,
            export_executor: None,
            progress_store: None,
            cancellation_manager: None,
            sos_storage_manager: None,
            discovery_state: None,
            discovery_orchestrator: None,
            circuit_breakers: None,
            auth_config: Arc::new(AuthConfig::disabled()),
        });

        (state, workflow_engine, execution_store)
    }

    fn create_test_api_state_with_catalog(
        catalog: Arc<dyn DataSourceCatalog>,
    ) -> (Arc<ApiState>, Arc<WorkflowEngine>, Arc<ExecutionStore>) {
        let (state, workflow_engine, execution_store) = create_test_api_state();
        let mut state_value = (*state).clone();
        state_value.datasource_catalog = Some(catalog);
        (Arc::new(state_value), workflow_engine, execution_store)
    }

    struct MockValidationCatalog {
        sources: HashMap<String, DataSourceResponse>,
        schemas: HashMap<(String, String), SchemaDefinition>,
    }

    impl MockValidationCatalog {
        fn new() -> Self {
            Self {
                sources: HashMap::new(),
                schemas: HashMap::new(),
            }
        }

        fn with_source(mut self, source: DataSourceResponse) -> Self {
            self.sources.insert(source.source.id.clone(), source);
            self
        }

        fn with_table_schema(
            mut self,
            datasource_id: &str,
            table_name: &str,
            columns: &[&str],
        ) -> Self {
            self.schemas.insert(
                (datasource_id.to_string(), table_name.to_string()),
                SchemaDefinition {
                    name: datasource_id.to_string(),
                    tables: vec![TableDefinition {
                        name: table_name.to_string(),
                        columns: columns
                            .iter()
                            .map(|column| ColumnDefinition {
                                name: (*column).to_string(),
                                data_type: "text".to_string(),
                                nullable: true,
                                primary_key: false,
                                default_value: None,
                                semantic_type: None,
                                statistics: None,
                            })
                            .collect(),
                        estimated_rows: None,
                    }],
                    relationships: Vec::new(),
                    indexes: Vec::new(),
                    inferred_at: Utc::now(),
                },
            );
            self
        }
    }

    #[async_trait]
    impl DataSourceCatalog for MockValidationCatalog {
        async fn register_source(
            &self,
            _source: DataSource,
        ) -> Result<DataSourceResponse, GraphicaError> {
            Err(GraphicaError::Internal("not implemented".to_string()))
        }

        async fn get_source(&self, id: &str) -> Result<DataSourceResponse, GraphicaError> {
            self.sources
                .get(id)
                .cloned()
                .ok_or_else(|| GraphicaError::NotFound(format!("Data source not found: {}", id)))
        }

        async fn update_source(
            &self,
            _id: &str,
            _updates: UpdateDataSourcePatch,
        ) -> Result<DataSourceResponse, GraphicaError> {
            Err(GraphicaError::Internal("not implemented".to_string()))
        }

        async fn delete_source(&self, _id: &str) -> Result<(), GraphicaError> {
            Err(GraphicaError::Internal("not implemented".to_string()))
        }

        async fn list_sources(
            &self,
            _request: &ListDataSourcesRequest,
        ) -> Result<ListDataSourcesResponse, GraphicaError> {
            Err(GraphicaError::Internal("not implemented".to_string()))
        }

        async fn test_connection(&self, _id: &str) -> Result<ConnectionTestResult, GraphicaError> {
            Err(GraphicaError::Internal("not implemented".to_string()))
        }

        async fn infer_schema(
            &self,
            id: &str,
            table_name: Option<&str>,
            _sample_size: usize,
        ) -> Result<SchemaDefinition, GraphicaError> {
            let table_name = table_name.unwrap_or_default().to_string();
            Ok(self
                .schemas
                .get(&(id.to_string(), table_name))
                .cloned()
                .unwrap_or(SchemaDefinition {
                    name: id.to_string(),
                    tables: Vec::new(),
                    relationships: Vec::new(),
                    indexes: Vec::new(),
                    inferred_at: Utc::now(),
                }))
        }

        async fn execute_query(
            &self,
            _id: &str,
            _query: &str,
            _parameters: HashMap<String, serde_json::Value>,
            _limit: Option<usize>,
        ) -> Result<QueryResult, GraphicaError> {
            Err(GraphicaError::Internal("not implemented".to_string()))
        }

        async fn mark_synced(&self, _id: &str) -> Result<(), GraphicaError> {
            Err(GraphicaError::Internal("not implemented".to_string()))
        }

        async fn update_status(
            &self,
            _id: &str,
            _status: DataSourceStatus,
            _error_message: Option<String>,
        ) -> Result<(), GraphicaError> {
            Err(GraphicaError::Internal("not implemented".to_string()))
        }

        async fn search_sources(
            &self,
            _query: &str,
            _limit: usize,
        ) -> Result<Vec<DataSourceResponse>, GraphicaError> {
            Err(GraphicaError::Internal("not implemented".to_string()))
        }

        async fn get_sources_by_tag(
            &self,
            _tag: &str,
        ) -> Result<Vec<DataSourceResponse>, GraphicaError> {
            Err(GraphicaError::Internal("not implemented".to_string()))
        }

        async fn get_usage_stats(
            &self,
            _id: &str,
        ) -> Result<graphica_core::catalog::client::UsageStatistics, GraphicaError> {
            Err(GraphicaError::Internal("not implemented".to_string()))
        }

        async fn get_source_by_title(
            &self,
            title: &str,
        ) -> Result<DataSourceResponse, GraphicaError> {
            self.sources
                .values()
                .find(|source| source.source.title == title)
                .cloned()
                .ok_or_else(|| {
                    GraphicaError::NotFound(format!("Data source not found with title: {}", title))
                })
        }
    }

    fn test_datasource_response(
        id: &str,
        capabilities: DataSourceCapabilities,
    ) -> DataSourceResponse {
        test_datasource_response_with_config(
            id,
            capabilities,
            SourceConfig::PostgreSQL(PostgreSQLConfig {
                host: "localhost".to_string(),
                port: 5432,
                database: "test".to_string(),
                schema: Some("public".to_string()),
                ssl_mode: None,
            }),
        )
    }

    fn test_datasource_response_with_config(
        id: &str,
        capabilities: DataSourceCapabilities,
        config: SourceConfig,
    ) -> DataSourceResponse {
        DataSourceResponse {
            source: DataSource {
                id: id.to_string(),
                title: format!("Datasource {}", id),
                description: None,
                source_type: config.source_type().to_string(),
                connection: ConnectionDetails {
                    secret_ref: "vault://test".to_string(),
                    config,
                    encryption_enabled: false,
                    credentials: HashMap::new(),
                },
                schema_ref: None,
                tags: Vec::new(),
                metadata: HashMap::new(),
                created_at: None,
                updated_at: None,
                last_synced_at: None,
            },
            status: DataSourceStatus::Active,
            last_test_result: None,
            capabilities: Some(capabilities),
        }
    }

    fn test_definition() -> WorkflowDefinition {
        WorkflowDefinition {
            steps: vec![WorkflowStep {
                id: "gate1".to_string(),
                step_type: StepType::ConfidenceGate,
                config: StepConfig::ConfidenceGate(ConfidenceGateConfig {
                    threshold: 0.5,
                    input_step: None,
                }),
                depends_on: vec![],
            }],
            fusion_threshold: 0.8,
            fallback: FallbackStrategy::ManualReview,
        }
    }

    #[tokio::test]
    async fn test_register_workflow_rejects_duplicate_ids() {
        let (state, _, _) = create_test_api_state();
        let request = RegisterWorkflowRequest {
            id: Some("wf_duplicate".to_string()),
            name: "Duplicate Workflow".to_string(),
            description: Some("test".to_string()),
            definition: test_definition(),
            tags: vec!["audit".to_string()],
        };

        let first = register_workflow(State(state.clone()), Json(request.clone())).await;
        assert!(first.is_ok());

        let duplicate = register_workflow(State(state), Json(request)).await;
        assert!(duplicate.is_err());
        assert_eq!(duplicate.unwrap_err().0, StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn test_update_workflow_updates_metadata_and_preserves_created_at() {
        let (state, workflow_engine, _) = create_test_api_state();

        let created = register_workflow(
            State(state.clone()),
            Json(RegisterWorkflowRequest {
                id: Some("wf_update_metadata".to_string()),
                name: "Original Workflow".to_string(),
                description: Some("original".to_string()),
                definition: test_definition(),
                tags: vec!["draft".to_string()],
            }),
        )
        .await
        .unwrap()
        .0;

        let mut updated_definition = test_definition();
        updated_definition.fusion_threshold = 0.95;

        let updated = update_workflow(
            State(state),
            Path("wf_update_metadata".to_string()),
            Json(RegisterWorkflowRequest {
                id: None,
                name: "Renamed Workflow".to_string(),
                description: Some("updated".to_string()),
                definition: updated_definition,
                tags: vec!["production".to_string()],
            }),
        )
        .await
        .unwrap()
        .0;

        assert_eq!(updated.name, "Renamed Workflow");
        assert_eq!(updated.created_at, created.created_at);

        let workflow = workflow_engine
            .get_workflow("wf_update_metadata")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(workflow.name, "Renamed Workflow");
        assert_eq!(workflow.description.as_deref(), Some("updated"));
        assert_eq!(workflow.tags, vec!["production".to_string()]);
        assert_eq!(workflow.definition.fusion_threshold, 0.95);
    }

    #[tokio::test]
    async fn test_update_workflow_returns_not_found_for_missing_workflow() {
        let (state, _, _) = create_test_api_state();

        let result = update_workflow(
            State(state),
            Path("wf_missing".to_string()),
            Json(RegisterWorkflowRequest {
                id: None,
                name: "Missing Workflow".to_string(),
                description: None,
                definition: test_definition(),
                tags: vec![],
            }),
        )
        .await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().0, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_delete_workflow_returns_not_found_for_missing_workflow() {
        let (state, _, _) = create_test_api_state();

        let result = delete_workflow(State(state), Path("wf_missing".to_string())).await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().0, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_validate_workflow_definition_flags_missing_extract_datasource() {
        let catalog = Arc::new(MockValidationCatalog::new());
        let (state, _, _) = create_test_api_state_with_catalog(catalog);

        let definition = WorkflowDefinition {
            steps: vec![WorkflowStep {
                id: "extract_customers".to_string(),
                step_type: StepType::DbExtract,
                config: StepConfig::DbExtract(DbExtractConfig {
                    datasource_id: "missing_source".to_string(),
                    table_name: Some("customers".to_string()),
                    schema_table: None,
                    query: None,
                    incremental: None,
                    incremental_column: None,
                    last_value: None,
                    batch_size: 50_000,
                    columns: None,
                    include_schema: None,
                    schema_sample_size: None,
                }),
                depends_on: Vec::new(),
            }],
            fusion_threshold: 0.8,
            fallback: FallbackStrategy::ManualReview,
        };

        let response = validate_workflow_definition(State(state), Json(definition))
            .await
            .unwrap()
            .0;

        assert!(!response.valid);
        assert!(response
            .issues
            .iter()
            .any(|issue| issue.code == "datasource_not_found"
                && issue.step_id == "extract_customers"));
    }

    #[tokio::test]
    async fn test_validate_workflow_definition_flags_unwritable_loader_datasource() {
        let catalog = Arc::new(
            MockValidationCatalog::new().with_source(test_datasource_response(
                "readonly_target",
                DataSourceCapabilities {
                    can_test: true,
                    can_infer_schema: true,
                    can_query: true,
                    can_read_workflow: true,
                    can_write_workflow: false,
                    supports_parameters: true,
                    supports_tls: true,
                    supports_incremental: true,
                    supports_cancellation: true,
                },
            )),
        );
        let (state, _, _) = create_test_api_state_with_catalog(catalog);

        let definition = WorkflowDefinition {
            steps: vec![WorkflowStep {
                id: "load_customers".to_string(),
                step_type: StepType::DbLoader,
                config: StepConfig::DbLoader(DbLoaderConfig {
                    datasource_id: "readonly_target".to_string(),
                    table_name: "customers".to_string(),
                    mode: LoadMode::Insert,
                    key_fields: None,
                    batch_size: 1_000,
                    create_table: false,
                    entity_uri: None,
                }),
                depends_on: Vec::new(),
            }],
            fusion_threshold: 0.8,
            fallback: FallbackStrategy::ManualReview,
        };

        let response = validate_workflow_definition(State(state), Json(definition))
            .await
            .unwrap()
            .0;

        assert!(!response.valid);
        assert!(response.issues.iter().any(|issue| {
            issue.code == "datasource_not_workflow_writable" && issue.step_id == "load_customers"
        }));
    }

    #[tokio::test]
    async fn test_validate_workflow_definition_flags_missing_upsert_key_fields() {
        let (state, _, _) = create_test_api_state();

        let definition = WorkflowDefinition {
            steps: vec![WorkflowStep {
                id: "upsert_customers".to_string(),
                step_type: StepType::DbLoader,
                config: StepConfig::DbLoader(DbLoaderConfig {
                    datasource_id: "target_ds".to_string(),
                    table_name: "customers".to_string(),
                    mode: LoadMode::Upsert,
                    key_fields: None,
                    batch_size: 1_000,
                    create_table: false,
                    entity_uri: None,
                }),
                depends_on: Vec::new(),
            }],
            fusion_threshold: 0.8,
            fallback: FallbackStrategy::ManualReview,
        };

        let response = validate_workflow_definition(State(state), Json(definition))
            .await
            .unwrap()
            .0;

        assert!(!response.valid);
        assert!(response
            .issues
            .iter()
            .any(|issue| issue.code == "missing_key_fields"));
    }

    #[tokio::test]
    async fn test_validate_workflow_definition_accepts_valid_datasource_workflow() {
        let capabilities = DataSourceCapabilities {
            can_test: true,
            can_infer_schema: true,
            can_query: true,
            can_read_workflow: true,
            can_write_workflow: true,
            supports_parameters: true,
            supports_tls: true,
            supports_incremental: true,
            supports_cancellation: true,
        };
        let catalog = Arc::new(
            MockValidationCatalog::new()
                .with_source(test_datasource_response("source_ds", capabilities.clone()))
                .with_source(test_datasource_response("target_ds", capabilities))
                .with_table_schema("source_ds", "customers", &["customer_id", "email"])
                .with_table_schema("target_ds", "customers_curated", &["customer_id", "email"]),
        );
        let (state, _, _) = create_test_api_state_with_catalog(catalog);

        let definition = WorkflowDefinition {
            steps: vec![
                WorkflowStep {
                    id: "extract_customers".to_string(),
                    step_type: StepType::DbExtract,
                    config: StepConfig::DbExtract(DbExtractConfig {
                        datasource_id: "source_ds".to_string(),
                        table_name: Some("customers".to_string()),
                        schema_table: None,
                        query: None,
                        incremental: Some(true),
                        incremental_column: Some("customer_id".to_string()),
                        last_value: None,
                        batch_size: 50_000,
                        columns: Some(vec!["customer_id".to_string(), "email".to_string()]),
                        include_schema: Some(true),
                        schema_sample_size: Some(250),
                    }),
                    depends_on: Vec::new(),
                },
                WorkflowStep {
                    id: "load_customers".to_string(),
                    step_type: StepType::DbLoader,
                    config: StepConfig::DbLoader(DbLoaderConfig {
                        datasource_id: "target_ds".to_string(),
                        table_name: "customers_curated".to_string(),
                        mode: LoadMode::Upsert,
                        key_fields: Some(vec!["customer_id".to_string()]),
                        batch_size: 1_000,
                        create_table: false,
                        entity_uri: None,
                    }),
                    depends_on: vec!["extract_customers".to_string()],
                },
            ],
            fusion_threshold: 0.8,
            fallback: FallbackStrategy::ManualReview,
        };

        let response = validate_workflow_definition(State(state), Json(definition))
            .await
            .unwrap()
            .0;

        assert!(response.valid, "{:?}", response.issues);
        assert!(response.issues.is_empty());
    }

    #[tokio::test]
    async fn test_validate_workflow_definition_accepts_valid_databricks_workflow() {
        let capabilities = DataSourceCapabilities {
            can_test: true,
            can_infer_schema: true,
            can_query: true,
            can_read_workflow: true,
            can_write_workflow: true,
            supports_parameters: true,
            supports_tls: true,
            supports_incremental: true,
            supports_cancellation: true,
        };
        let catalog = Arc::new(
            MockValidationCatalog::new()
                .with_source(test_datasource_response_with_config(
                    "dbx_source",
                    capabilities.clone(),
                    SourceConfig::Databricks(DatabricksConfig {
                        workspace_url: "https://adb-123.azuredatabricks.net".to_string(),
                        http_path: "/sql/1.0/warehouses/abc123".to_string(),
                        catalog: Some("main".to_string()),
                        schema: Some("bronze".to_string()),
                        warehouse_id: Some("abc123".to_string()),
                    }),
                ))
                .with_source(test_datasource_response_with_config(
                    "dbx_target",
                    capabilities,
                    SourceConfig::Databricks(DatabricksConfig {
                        workspace_url: "https://adb-123.azuredatabricks.net".to_string(),
                        http_path: "/sql/1.0/warehouses/abc123".to_string(),
                        catalog: Some("main".to_string()),
                        schema: Some("silver".to_string()),
                        warehouse_id: Some("abc123".to_string()),
                    }),
                ))
                .with_table_schema("dbx_source", "bronze.customers", &["customer_id", "email"])
                .with_table_schema(
                    "dbx_target",
                    "silver.customers_curated",
                    &["customer_id", "email"],
                ),
        );
        let (state, _, _) = create_test_api_state_with_catalog(catalog);

        let definition = WorkflowDefinition {
            steps: vec![
                WorkflowStep {
                    id: "extract_customers".to_string(),
                    step_type: StepType::DbExtract,
                    config: StepConfig::DbExtract(DbExtractConfig {
                        datasource_id: "dbx_source".to_string(),
                        table_name: Some("bronze.customers".to_string()),
                        schema_table: Some("bronze.customers".to_string()),
                        query: None,
                        incremental: Some(true),
                        incremental_column: Some("customer_id".to_string()),
                        last_value: None,
                        batch_size: 50_000,
                        columns: Some(vec!["customer_id".to_string(), "email".to_string()]),
                        include_schema: Some(true),
                        schema_sample_size: Some(250),
                    }),
                    depends_on: Vec::new(),
                },
                WorkflowStep {
                    id: "load_customers".to_string(),
                    step_type: StepType::DbLoader,
                    config: StepConfig::DbLoader(DbLoaderConfig {
                        datasource_id: "dbx_target".to_string(),
                        table_name: "silver.customers_curated".to_string(),
                        mode: LoadMode::Upsert,
                        key_fields: Some(vec!["customer_id".to_string()]),
                        batch_size: 1_000,
                        create_table: false,
                        entity_uri: None,
                    }),
                    depends_on: vec!["extract_customers".to_string()],
                },
            ],
            fusion_threshold: 0.8,
            fallback: FallbackStrategy::ManualReview,
        };

        let response = validate_workflow_definition(State(state), Json(definition))
            .await
            .unwrap()
            .0;

        assert!(response.valid, "{:?}", response.issues);
        assert!(response.issues.is_empty());
    }

    #[tokio::test]
    async fn test_validate_workflow_definition_accepts_catalog_qualified_databricks_workflow() {
        let capabilities = DataSourceCapabilities {
            can_test: true,
            can_infer_schema: true,
            can_query: true,
            can_read_workflow: true,
            can_write_workflow: true,
            supports_parameters: true,
            supports_tls: true,
            supports_incremental: true,
            supports_cancellation: true,
        };
        let catalog = Arc::new(
            MockValidationCatalog::new()
                .with_source(test_datasource_response_with_config(
                    "dbx_source",
                    capabilities.clone(),
                    SourceConfig::Databricks(DatabricksConfig {
                        workspace_url: "https://adb-123.azuredatabricks.net".to_string(),
                        http_path: "/sql/1.0/warehouses/abc123".to_string(),
                        catalog: Some("main".to_string()),
                        schema: Some("bronze".to_string()),
                        warehouse_id: Some("abc123".to_string()),
                    }),
                ))
                .with_source(test_datasource_response_with_config(
                    "dbx_target",
                    capabilities,
                    SourceConfig::Databricks(DatabricksConfig {
                        workspace_url: "https://adb-123.azuredatabricks.net".to_string(),
                        http_path: "/sql/1.0/warehouses/abc123".to_string(),
                        catalog: Some("main".to_string()),
                        schema: Some("silver".to_string()),
                        warehouse_id: Some("abc123".to_string()),
                    }),
                ))
                .with_table_schema("dbx_source", "bronze.customers", &["customer_id", "email"])
                .with_table_schema(
                    "dbx_target",
                    "silver.customers_curated",
                    &["customer_id", "email"],
                ),
        );
        let (state, _, _) = create_test_api_state_with_catalog(catalog);

        let definition = WorkflowDefinition {
            steps: vec![
                WorkflowStep {
                    id: "extract_customers".to_string(),
                    step_type: StepType::DbExtract,
                    config: StepConfig::DbExtract(DbExtractConfig {
                        datasource_id: "dbx_source".to_string(),
                        table_name: Some("main.bronze.customers".to_string()),
                        schema_table: Some("main.bronze.customers".to_string()),
                        query: None,
                        incremental: Some(true),
                        incremental_column: Some("customer_id".to_string()),
                        last_value: None,
                        batch_size: 50_000,
                        columns: Some(vec!["customer_id".to_string(), "email".to_string()]),
                        include_schema: Some(true),
                        schema_sample_size: Some(250),
                    }),
                    depends_on: Vec::new(),
                },
                WorkflowStep {
                    id: "load_customers".to_string(),
                    step_type: StepType::DbLoader,
                    config: StepConfig::DbLoader(DbLoaderConfig {
                        datasource_id: "dbx_target".to_string(),
                        table_name: "main.silver.customers_curated".to_string(),
                        mode: LoadMode::Upsert,
                        key_fields: Some(vec!["customer_id".to_string()]),
                        batch_size: 1_000,
                        create_table: false,
                        entity_uri: None,
                    }),
                    depends_on: vec!["extract_customers".to_string()],
                },
            ],
            fusion_threshold: 0.8,
            fallback: FallbackStrategy::ManualReview,
        };

        let response = validate_workflow_definition(State(state), Json(definition))
            .await
            .unwrap()
            .0;

        assert!(response.valid, "{:?}", response.issues);
        assert!(response.issues.is_empty());
    }

    #[tokio::test]
    async fn test_get_workflow_details_returns_metadata_and_definition() {
        let (state, workflow_engine, _) = create_test_api_state();
        let workflow_id = "wf_details".to_string();

        workflow_engine
            .register_workflow(
                workflow_id.clone(),
                "Detailed Workflow".to_string(),
                test_definition(),
                Some("workflow details".to_string()),
                vec!["graph".to_string()],
            )
            .await
            .unwrap();

        let Json(details) = get_workflow_details(State(state), Path(workflow_id.clone()))
            .await
            .unwrap();

        assert_eq!(details.workflow_id, workflow_id);
        assert_eq!(details.name, "Detailed Workflow");
        assert_eq!(details.description.as_deref(), Some("workflow details"));
        assert_eq!(details.tags, vec!["graph".to_string()]);
        assert_eq!(details.definition.steps.len(), 1);
    }

    #[tokio::test]
    async fn test_list_workflows_returns_service_unavailable_when_engine_missing() {
        let (state, _, _) = create_test_api_state();
        let state_without_engine = Arc::new(ApiState {
            workflow_engine: None,
            ..(*state).clone()
        });

        let result = list_workflows(State(state_without_engine)).await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().0, StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn test_list_workflow_executions_returns_persisted_history() {
        let (state, workflow_engine, execution_store) = create_test_api_state();
        let workflow_id = "wf_history".to_string();

        workflow_engine
            .register_workflow(
                workflow_id.clone(),
                "History Workflow".to_string(),
                test_definition(),
                None,
                vec![],
            )
            .await
            .unwrap();

        let mut execution = WorkflowExecution::new(
            "exec_history_1".to_string(),
            workflow_id.clone(),
            "History Workflow".to_string(),
            serde_json::json!({"type": "json", "data": {"record": 1}}),
            Some("tester".to_string()),
        );
        execution.status = ExecutionStatus::Completed;
        execution.confidence = Some(0.73);
        execution.started_at = Utc.with_ymd_and_hms(2026, 3, 9, 10, 0, 0).unwrap();
        execution.updated_at = Utc.with_ymd_and_hms(2026, 3, 9, 10, 1, 0).unwrap();
        execution.completed_at = Some(Utc.with_ymd_and_hms(2026, 3, 9, 10, 1, 0).unwrap());
        execution.duration_ms = Some(60_000);

        execution_store.save(execution).await.unwrap();

        let Json(summaries) = list_workflow_executions(
            State(state),
            Path(workflow_id),
            Query(ExecutionHistoryQuery::default()),
        )
        .await
        .unwrap();

        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].execution_id, "exec_history_1");
        assert_eq!(summaries[0].confidence, 0.73);
        assert!(summaries[0].success);
    }
}
