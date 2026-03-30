use crate::api::dto::datasets::{
    ColumnDefinition as DatasetColumnDefinition, ImportErrorResponse, SchemaDefinition,
};
use crate::api::handlers::datasets::{
    store_workflow_output_lineage, write_query_result_to_parquet,
};
use crate::api::workflow::types::{
    ExecutionResultDto, ExecutionRuntimeMetricsDto, RuntimeStepMetricsDto, StepResultDto,
    WorkflowOutputDatasetRef, WorkflowOutputDatasetRequest,
};
use crate::api::ApiState;
use crate::governance::WorkflowResultPersistence;
use crate::workflows::domain::{
    ExecutionRuntimeMetricsSummary, ExecutionStatus, PersistedStepResult, WorkflowExecution,
};
use axum::{http::StatusCode, Json};
use chrono::{DateTime, Utc};
use graphica_core::catalog::api_types::{ColumnDefinition as QueryColumnDefinition, QueryResult};
use graphica_core::orchestration::workflow::executor::WorkflowResult;
use serde_json::{json, Value as JsonValue};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

pub async fn finalize_execution_result(
    state: Arc<ApiState>,
    workflow_id: &str,
    workflow_name: &str,
    result: &WorkflowResult,
    output_dataset: Option<&WorkflowOutputDatasetRequest>,
    batch_index: usize,
    batch_count: usize,
) -> Result<ExecutionResultDto, (StatusCode, String)> {
    if output_dataset.is_none() {
        if let Err(error) = persist_workflow_execution_if_possible(
            state.as_ref(),
            workflow_id,
            workflow_name,
            &result,
        )
        .await
        {
            tracing::warn!(
                workflow_id = workflow_id,
                execution_id = result.execution_id,
                "Failed to persist workflow execution RDF: {}",
                error
            );
        }
    }

    let materialized_dataset = if let Some(request) = output_dataset {
        Some(
            materialize_workflow_output_dataset(
                state.as_ref(),
                workflow_id,
                workflow_name,
                &result,
                request,
                batch_index,
                batch_count,
            )
            .await?,
        )
    } else {
        None
    };

    let mut step_results: Vec<StepResultDto> = result
        .step_results
        .iter()
        .map(|(id, step)| StepResultDto {
            step_id: id.clone(),
            success: step.success,
            output: step.output.clone(),
            confidence: step.confidence,
            duration_ms: step
                .completed_at
                .signed_duration_since(step.started_at)
                .num_milliseconds()
                .max(0) as u64,
            runtime_metrics: step
                .runtime_metrics
                .as_ref()
                .map(RuntimeStepMetricsDto::from),
        })
        .collect();
    step_results.sort_by(|left, right| left.step_id.cmp(&right.step_id));
    let final_output = final_output_for_response(&result);
    let execution_id = result.execution_id.clone();
    let runtime_metrics = summarize_workflow_runtime_metrics(result.step_results.values());

    Ok(ExecutionResultDto {
        execution_id,
        success: result.success,
        step_results,
        final_output,
        confidence: result.confidence,
        runtime_metrics: runtime_metrics
            .as_ref()
            .map(ExecutionRuntimeMetricsDto::from),
        materialized_dataset,
    })
}

pub async fn persist_execution_record_if_possible(
    state: &ApiState,
    workflow_id: &str,
    workflow_name: &str,
    execution_input: &JsonValue,
    triggered_by: Option<&str>,
    result: &WorkflowResult,
    execution_result: &ExecutionResultDto,
) -> Result<(), String> {
    let Some(execution_store) = state.execution_store.as_ref() else {
        return Ok(());
    };

    let mut execution = WorkflowExecution::new(
        result.execution_id.clone(),
        workflow_id.to_string(),
        workflow_name.to_string(),
        execution_input.clone(),
        triggered_by.map(ToOwned::to_owned),
    );

    execution.started_at = result.started_at;
    execution.updated_at = result.completed_at;
    execution.completed_at = Some(result.completed_at);
    execution.duration_ms = Some(
        result
            .completed_at
            .signed_duration_since(result.started_at)
            .num_milliseconds()
            .max(0) as u64,
    );
    execution.status = if result.success {
        ExecutionStatus::Completed
    } else {
        ExecutionStatus::Failed
    };
    execution.error = result.error.clone();
    execution.actions_executed = result.step_results.len();
    execution.confidence = Some(result.confidence);
    execution.step_results = result
        .step_results
        .iter()
        .map(|(step_id, step)| PersistedStepResult {
            step_id: step_id.clone(),
            success: step.success,
            output: step.output.clone(),
            confidence: step.confidence,
            duration_ms: step
                .completed_at
                .signed_duration_since(step.started_at)
                .num_milliseconds()
                .max(0) as u64,
            runtime_metrics: step.runtime_metrics.clone(),
        })
        .collect();
    execution.runtime_metrics = summarize_workflow_runtime_metrics(result.step_results.values());
    execution.output = Some(build_persisted_execution_output(execution_result));

    if execution_store
        .exists(&execution.execution_id)
        .await
        .map_err(|error| error.to_string())?
    {
        execution_store
            .update(execution)
            .await
            .map_err(|error| error.to_string())
    } else {
        execution_store
            .save(execution)
            .await
            .map_err(|error| error.to_string())
    }
}

async fn persist_workflow_execution_if_possible(
    state: &ApiState,
    workflow_id: &str,
    workflow_name: &str,
    result: &WorkflowResult,
) -> Result<(), String> {
    let Some(rdf_store) = state.rdf_store.as_ref() else {
        return Ok(());
    };

    let persistence = WorkflowResultPersistence::new(rdf_store.clone());
    persistence
        .persist_workflow_definition(workflow_id, workflow_name, "1.0.0")
        .await
        .map_err(|error| error.to_string())?;
    persistence
        .persist_result(workflow_id, result)
        .await
        .map_err(|error| error.to_string())
}

async fn materialize_workflow_output_dataset(
    state: &ApiState,
    workflow_id: &str,
    workflow_name: &str,
    result: &WorkflowResult,
    request: &WorkflowOutputDatasetRequest,
    batch_index: usize,
    batch_count: usize,
) -> Result<WorkflowOutputDatasetRef, (StatusCode, String)> {
    if state.rdf_store.is_none() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "RDF store not available; cannot register workflow output dataset".to_string(),
        ));
    }

    persist_workflow_execution_if_possible(state, workflow_id, workflow_name, result)
        .await
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?;

    let rows = extract_output_rows(result).ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            "Workflow output does not contain materializable rows".to_string(),
        )
    })?;

    let schema = infer_schema_from_rows(&rows).map_err(|error| (StatusCode::BAD_REQUEST, error))?;
    let query_columns = schema
        .columns
        .iter()
        .map(|column| QueryColumnDefinition {
            name: column.name.clone(),
            data_type: column.data_type.clone(),
            nullable: column.nullable,
            primary_key: false,
            default_value: None,
            semantic_type: None,
            statistics: None,
        })
        .collect();

    let storage_root =
        std::env::var("PARQUET_PATH").unwrap_or_else(|_| "./data/parquet".to_string());
    std::fs::create_dir_all(&storage_root).map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!(
                "Failed to create workflow dataset storage directory: {}",
                error
            ),
        )
    })?;

    let dataset_id = format!("ds_workflow_{}", uuid::Uuid::new_v4().simple());
    let dataset_name = resolve_dataset_name(
        request.name.as_deref(),
        workflow_name,
        &result.completed_at,
        batch_index,
        batch_count,
    );
    let parquet_path = format!("{}/{}.parquet", storage_root, dataset_id);
    let row_count = rows.len() as u64;
    let query_result = QueryResult {
        rows,
        row_count: row_count as usize,
        execution_time_ms: result
            .completed_at
            .signed_duration_since(result.started_at)
            .num_milliseconds()
            .max(0) as u64,
        truncated: false,
        columns: Some(query_columns),
    };

    let file_size_bytes =
        write_query_result_to_parquet(&query_result, &parquet_path).map_err(map_import_error)?;

    store_workflow_output_lineage(
        state,
        &dataset_id,
        &dataset_name,
        row_count,
        &schema,
        workflow_id,
        &result.execution_id,
        workflow_name,
        &result.completed_at.to_rfc3339(),
        "parquet",
        &parquet_path,
        file_size_bytes,
    )
    .await
    .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?;

    Ok(WorkflowOutputDatasetRef {
        dataset_id,
        name: dataset_name,
        dataset_type: "workflow_output".to_string(),
        asset_kind: "materialized_dataset".to_string(),
        record_count: row_count,
        file_size_bytes,
        created_at: result.completed_at,
    })
}

fn final_output_for_response(result: &WorkflowResult) -> JsonValue {
    let mut output = if result.final_output.is_null() {
        JsonValue::Null
    } else {
        result.final_output.clone()
    };

    if let Some(object) = output.as_object_mut() {
        if let Some(row_count) = object
            .get("_rows")
            .and_then(|value| value.as_array())
            .map(|rows| rows.len())
        {
            if row_count > 10_000 {
                object.remove("_rows");
                object.insert("_row_count".to_string(), json!(row_count));
            }
        }

        if let Some(row_count) = object
            .get("rows")
            .and_then(|value| value.as_array())
            .map(|rows| rows.len())
        {
            if row_count > 10_000 {
                object.remove("rows");
                object.insert("row_count".to_string(), json!(row_count));
            }
        }
    }

    if output.is_null() {
        result
            .step_results
            .values()
            .last()
            .map(|step| step.output.clone())
            .unwrap_or(JsonValue::Null)
    } else {
        output
    }
}

fn build_persisted_execution_output(execution_result: &ExecutionResultDto) -> JsonValue {
    if let Some(materialized_dataset) = execution_result.materialized_dataset.as_ref() {
        json!({
            "final_output": execution_result.final_output.clone(),
            "materialized_dataset": materialized_dataset,
        })
    } else {
        execution_result.final_output.clone()
    }
}

fn summarize_workflow_runtime_metrics<'a, I>(
    step_results: I,
) -> Option<ExecutionRuntimeMetricsSummary>
where
    I: IntoIterator<Item = &'a graphica_core::orchestration::workflow::executor::StepResult>,
{
    ExecutionRuntimeMetricsSummary::from_runtime_metrics(
        step_results
            .into_iter()
            .filter_map(|step| step.runtime_metrics.as_ref()),
    )
}

fn extract_output_rows(result: &WorkflowResult) -> Option<Vec<JsonValue>> {
    result.output_rows.clone().or_else(|| {
        result
            .final_output
            .get("_rows")
            .and_then(|value| value.as_array())
            .or_else(|| {
                result
                    .final_output
                    .get("rows")
                    .and_then(|value| value.as_array())
            })
            .cloned()
    })
}

fn resolve_dataset_name(
    requested_name: Option<&str>,
    workflow_name: &str,
    completed_at: &DateTime<Utc>,
    batch_index: usize,
    batch_count: usize,
) -> String {
    let base_name = requested_name
        .map(|name| name.trim())
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            format!(
                "{} Output {}",
                workflow_name,
                completed_at.format("%Y-%m-%d %H:%M:%S UTC")
            )
        });

    if batch_count > 1 {
        format!(
            "{} (batch {} of {})",
            base_name,
            batch_index + 1,
            batch_count
        )
    } else {
        base_name
    }
}

fn infer_schema_from_rows(rows: &[JsonValue]) -> Result<SchemaDefinition, String> {
    if rows.is_empty() {
        return Err(
            "Workflow output contained zero rows; schema inference is unavailable".to_string(),
        );
    }

    let mut ordered_columns = Vec::new();
    let mut seen_columns = HashSet::new();
    for row in rows {
        let object = row
            .as_object()
            .ok_or_else(|| "Workflow output rows must be JSON objects".to_string())?;
        for key in object.keys() {
            if seen_columns.insert(key.clone()) {
                ordered_columns.push(key.clone());
            }
        }
    }

    if ordered_columns.is_empty() {
        return Err("Workflow output rows did not contain any columns".to_string());
    }

    let mut column_state: HashMap<String, ColumnState> = ordered_columns
        .iter()
        .map(|name| (name.clone(), ColumnState::default()))
        .collect();

    for row in rows {
        let object = row
            .as_object()
            .ok_or_else(|| "Workflow output rows must be JSON objects".to_string())?;
        for column in &ordered_columns {
            let state = column_state.get_mut(column).expect("column state exists");
            match object.get(column) {
                None | Some(JsonValue::Null) => {
                    state.nullable = true;
                }
                Some(value) => {
                    let observed_kind = infer_value_kind(value);
                    state.kind = Some(match state.kind {
                        Some(existing) => merge_kinds(existing, observed_kind),
                        None => observed_kind,
                    });
                }
            }
        }
    }

    let columns = ordered_columns
        .into_iter()
        .map(|name| {
            let state = column_state
                .remove(&name)
                .ok_or_else(|| format!("Missing inferred schema for column {}", name))?;
            Ok(DatasetColumnDefinition {
                name,
                data_type: state
                    .kind
                    .unwrap_or(ValueKind::Text)
                    .as_type_name()
                    .to_string(),
                nullable: state.nullable,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;

    Ok(SchemaDefinition {
        primary_key: None,
        columns,
    })
}

fn map_import_error(error: (StatusCode, Json<ImportErrorResponse>)) -> (StatusCode, String) {
    let (status, body) = error;
    (status, body.0.message)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ValueKind {
    Bool,
    Int,
    Float,
    Text,
    Json,
}

impl ValueKind {
    fn as_type_name(self) -> &'static str {
        match self {
            ValueKind::Bool => "BOOLEAN",
            ValueKind::Int => "BIGINT",
            ValueKind::Float => "DOUBLE",
            ValueKind::Text => "TEXT",
            ValueKind::Json => "JSON",
        }
    }
}

#[derive(Debug, Default)]
struct ColumnState {
    nullable: bool,
    kind: Option<ValueKind>,
}

fn infer_value_kind(value: &JsonValue) -> ValueKind {
    match value {
        JsonValue::Bool(_) => ValueKind::Bool,
        JsonValue::Number(number) => {
            if number.is_i64() || number.is_u64() {
                ValueKind::Int
            } else {
                ValueKind::Float
            }
        }
        JsonValue::String(_) => ValueKind::Text,
        JsonValue::Array(_) | JsonValue::Object(_) => ValueKind::Json,
        JsonValue::Null => ValueKind::Text,
    }
}

fn merge_kinds(existing: ValueKind, observed: ValueKind) -> ValueKind {
    use ValueKind::{Float, Int, Json, Text};

    match (existing, observed) {
        (Json, _) | (_, Json) => Json,
        (Text, _) | (_, Text) => Text,
        (Float, Int) | (Int, Float) => Float,
        (left, right) if left == right => left,
        _ => Text,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_persisted_execution_output, final_output_for_response, infer_schema_from_rows,
        resolve_dataset_name, summarize_workflow_runtime_metrics,
    };
    use crate::api::workflow::types::ExecutionResultDto;
    use chrono::{TimeZone, Utc};
    use graphica_core::orchestration::workflow::executor::{
        FinalDecision, StepResult, WorkflowResult,
    };
    use graphica_core::orchestration::workflow::runtime::metrics::RuntimeStepMetrics;
    use serde_json::{json, Value as JsonValue};
    use std::collections::HashMap;

    #[test]
    fn test_infer_schema_from_rows_marks_nullable_and_json() {
        let rows = vec![
            json!({"id": 1, "active": true, "payload": {"nested": true}}),
            json!({"id": 2, "active": null}),
        ];

        let schema = infer_schema_from_rows(&rows).unwrap();
        assert_eq!(schema.columns.len(), 3);
        let columns = schema
            .columns
            .iter()
            .map(|column| (column.name.as_str(), column))
            .collect::<std::collections::HashMap<_, _>>();

        assert_eq!(columns.get("id").unwrap().data_type, "BIGINT");
        assert!(columns.get("active").unwrap().nullable);
        assert_eq!(columns.get("payload").unwrap().data_type, "JSON");
    }

    #[test]
    fn test_resolve_dataset_name_appends_batch_suffix() {
        let completed_at = Utc.with_ymd_and_hms(2026, 3, 9, 12, 0, 0).unwrap();
        let name = resolve_dataset_name(Some("Curated Customers"), "ignored", &completed_at, 1, 3);
        assert_eq!(name, "Curated Customers (batch 2 of 3)");
    }

    #[test]
    fn test_final_output_for_response_strips_large_row_arrays() {
        let rows: Vec<JsonValue> = (0..10001).map(|idx| json!({"id": idx})).collect();
        let result = WorkflowResult {
            execution_id: "exec_test".to_string(),
            success: true,
            final_decision: FinalDecision::Accept,
            confidence: 1.0,
            step_results: HashMap::new(),
            started_at: Utc.with_ymd_and_hms(2026, 3, 9, 11, 0, 0).unwrap(),
            completed_at: Utc.with_ymd_and_hms(2026, 3, 9, 11, 5, 0).unwrap(),
            error: None,
            final_output: json!({"_rows": rows}),
            output_rows: None,
        };

        let final_output = final_output_for_response(&result);
        assert!(final_output.get("_rows").is_none());
        assert_eq!(
            final_output
                .get("_row_count")
                .and_then(|value| value.as_u64()),
            Some(10001)
        );
    }

    #[test]
    fn test_build_persisted_execution_output_wraps_materialized_dataset() {
        let execution_result = ExecutionResultDto {
            execution_id: "exec_test".to_string(),
            success: true,
            step_results: vec![],
            final_output: json!({"records": 12}),
            confidence: 0.92,
            runtime_metrics: None,
            materialized_dataset: Some(super::WorkflowOutputDatasetRef {
                dataset_id: "ds_workflow_test".to_string(),
                name: "Workflow Output".to_string(),
                dataset_type: "workflow_output".to_string(),
                asset_kind: "materialized_dataset".to_string(),
                record_count: 12,
                file_size_bytes: 2048,
                created_at: Utc.with_ymd_and_hms(2026, 3, 9, 12, 30, 0).unwrap(),
            }),
        };

        let persisted = build_persisted_execution_output(&execution_result);
        assert_eq!(persisted.get("final_output"), Some(&json!({"records": 12})));
        assert_eq!(
            persisted
                .get("materialized_dataset")
                .and_then(|value| value.get("dataset_id"))
                .and_then(|value| value.as_str()),
            Some("ds_workflow_test")
        );
    }

    #[test]
    fn test_summarize_workflow_runtime_metrics_rolls_up_storage_signals() {
        let started_at = Utc.with_ymd_and_hms(2026, 3, 9, 11, 0, 0).unwrap();
        let completed_at = Utc.with_ymd_and_hms(2026, 3, 9, 11, 5, 0).unwrap();
        let step_results = HashMap::from([
            (
                "spill_step".to_string(),
                StepResult {
                    step_id: "spill_step".to_string(),
                    success: true,
                    output: json!({"_row_count": 120000}),
                    confidence: 0.91,
                    started_at,
                    completed_at,
                    batch_metadata: None,
                    runtime_metrics: Some(RuntimeStepMetrics {
                        input_rows: 120000,
                        output_rows: 120000,
                        materialization_count: 1,
                        spill_events: 2,
                        spill_bytes: 8192,
                        memory_high_water_mark: 16384,
                        storage_type: Some("parquet".to_string()),
                        storage_operation: Some("set_rows".to_string()),
                        planned_tier: Some("parquet".to_string()),
                        storage_decision_reason: Some("planned".to_string()),
                        reserved_spill_bytes: 4096,
                        execution_reserved_spill_bytes: 4096,
                        total_reserved_spill_bytes: 4096,
                        storage_location: Some("spill/spill_step.parquet".to_string()),
                        pushdown_applied: false,
                    }),
                    batch_frame: None,
                },
            ),
            (
                "memory_step".to_string(),
                StepResult {
                    step_id: "memory_step".to_string(),
                    success: true,
                    output: json!({"_row_count": 2}),
                    confidence: 1.0,
                    started_at,
                    completed_at,
                    batch_metadata: None,
                    runtime_metrics: Some(RuntimeStepMetrics {
                        input_rows: 2,
                        output_rows: 2,
                        materialization_count: 0,
                        spill_events: 0,
                        spill_bytes: 0,
                        memory_high_water_mark: 1024,
                        storage_type: Some("in_memory".to_string()),
                        storage_operation: Some("set_rows".to_string()),
                        planned_tier: Some("in_memory".to_string()),
                        storage_decision_reason: Some("planned".to_string()),
                        reserved_spill_bytes: 0,
                        execution_reserved_spill_bytes: 0,
                        total_reserved_spill_bytes: 0,
                        storage_location: None,
                        pushdown_applied: false,
                    }),
                    batch_frame: None,
                },
            ),
        ]);

        let summary = summarize_workflow_runtime_metrics(step_results.values())
            .expect("runtime metrics summary should be produced");

        assert_eq!(summary.steps_with_runtime_metrics, 2);
        assert_eq!(summary.steps_with_disk_storage, 1);
        assert_eq!(summary.total_spill_events, 2);
        assert_eq!(summary.total_spill_bytes, 8192);
        assert_eq!(summary.max_memory_high_water_mark, 16384);
        assert_eq!(summary.storage_backends, vec!["in_memory", "parquet"]);
        assert_eq!(summary.planned_tiers, vec!["in_memory", "parquet"]);
        assert_eq!(summary.storage_decision_reasons, vec!["planned"]);
    }
}
