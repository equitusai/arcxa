use anyhow::{anyhow, bail, Context, Result};
use arrow2::array::{Array, BooleanArray, PrimitiveArray, Utf8Array};
use arrow2::datatypes::TimeUnit;
use arrow2::io::parquet::read as parquet_read;
use axum::http::StatusCode;
use chrono::{Duration, NaiveDate, Utc};
use graphica_core::orchestration::workflow::{
    DataSourceInputAdapter, DatasetInputAdapter, DatasetResolver, EntityFilterAdapter,
    InputAdapter, JsonInputAdapter, SparqlInputAdapter, WorkflowInput,
};
use serde_json::{Map as JsonMap, Number as JsonNumber, Value as JsonValue};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::api::ApiState;
use crate::governance::rdf_store::RdfStore;
use crate::governance::sparql_templates::SparqlTemplates;
use crate::governance::workflow_query_adapter::WorkflowQueryAdapter;

pub fn build_input_adapter(
    state: Arc<ApiState>,
    workflow_input: &WorkflowInput,
) -> Result<Arc<dyn InputAdapter>, (StatusCode, String)> {
    match workflow_input {
        WorkflowInput::SparqlQuery { .. } => {
            let query_executor = state.query_executor.as_ref().ok_or_else(|| {
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    "Query executor not available. Ensure coordinator is running in distributed mode with shards.".to_string(),
                )
            })?;

            let workflow_adapter = Arc::new(WorkflowQueryAdapter::new(query_executor.clone()));
            Ok(Arc::new(SparqlInputAdapter::new(workflow_adapter)))
        }
        WorkflowInput::EntityFilter { .. } => {
            let query_executor = state.query_executor.as_ref().ok_or_else(|| {
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    "Query executor not available. Ensure coordinator is running in distributed mode with shards.".to_string(),
                )
            })?;

            let workflow_adapter = Arc::new(WorkflowQueryAdapter::new(query_executor.clone()));
            Ok(Arc::new(EntityFilterAdapter::new(workflow_adapter)))
        }
        WorkflowInput::Json { .. } => Ok(Arc::new(JsonInputAdapter)),
        WorkflowInput::DataSourceQuery { .. } => {
            let catalog = state.datasource_catalog.clone().ok_or_else(|| {
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    "Datasource catalog is not available".to_string(),
                )
            })?;

            Ok(Arc::new(DataSourceInputAdapter::new(catalog)))
        }
        WorkflowInput::Dataset { .. } => Ok(Arc::new(DatasetInputAdapter::new(Arc::new(
            MaterializedDatasetResolver::new(state),
        )))),
        WorkflowInput::GraphStream { .. } => Err((
            StatusCode::BAD_REQUEST,
            "Graph stream workflow input is not yet supported".to_string(),
        )),
    }
}

struct MaterializedDatasetResolver {
    state: Arc<ApiState>,
}

impl MaterializedDatasetResolver {
    fn new(state: Arc<ApiState>) -> Self {
        Self { state }
    }

    fn resolve_storage_location(&self, dataset_id: &str) -> Result<DatasetStorageLocation> {
        resolve_storage_location_from_state(&self.state, dataset_id)
    }
}

pub(crate) fn resolve_materialized_dataset_path(
    state: &ApiState,
    dataset_id: &str,
) -> Result<String> {
    Ok(resolve_storage_location_from_state(state, dataset_id)?.path)
}

fn resolve_storage_location_from_state(
    state: &ApiState,
    dataset_id: &str,
) -> Result<DatasetStorageLocation> {
    let mut dataset_type = None;
    let mut storage_path = None;
    let mut storage_format = None;

    if let Some(rdf_store) = &state.rdf_store {
        let query = SparqlTemplates::get_dataset_by_id(dataset_id);
        if let Ok(results) = rdf_store.query(&query) {
            for result in results {
                let Some(property_uri) = result.get("property").and_then(|value| value.as_str())
                else {
                    continue;
                };

                let property_name = property_uri
                    .rsplit('#')
                    .next()
                    .or_else(|| property_uri.rsplit('/').next())
                    .unwrap_or(property_uri);

                match property_name {
                    "datasetType" => {
                        dataset_type = result
                            .get("value")
                            .and_then(|value| value.as_str())
                            .map(ToOwned::to_owned);
                    }
                    "storagePath" => {
                        storage_path = result
                            .get("value")
                            .and_then(|value| value.as_str())
                            .map(ToOwned::to_owned);
                    }
                    "storageFormat" => {
                        storage_format = result
                            .get("value")
                            .and_then(|value| value.as_str())
                            .map(ToOwned::to_owned);
                    }
                    _ => {}
                }
            }
        }
    }

    if matches!(dataset_type.as_deref(), Some("source")) {
        bail!(
            "Dataset {} is a source asset. Materialize it before using it as workflow input",
            dataset_id
        );
    }

    if storage_path.is_none() {
        let legacy_path = legacy_parquet_path(dataset_id);
        if legacy_path.exists() {
            storage_path = Some(legacy_path.to_string_lossy().to_string());
            storage_format.get_or_insert_with(|| "parquet".to_string());
        }
    }

    let path = storage_path.ok_or_else(|| {
        anyhow!(
            "Dataset {} does not have materialized storage metadata or a readable Parquet file",
            dataset_id
        )
    })?;

    if !Path::new(&path).exists() {
        bail!(
            "Dataset {} storage file does not exist at {}",
            dataset_id,
            path
        );
    }

    let format = storage_format
        .unwrap_or_else(|| infer_storage_format(&path))
        .to_lowercase();

    if format != "parquet" {
        bail!(
            "Dataset {} uses unsupported storage format {}. Only Parquet-backed datasets are supported right now",
            dataset_id,
            format
        );
    }

    Ok(DatasetStorageLocation { path })
}

#[async_trait::async_trait]
impl DatasetResolver for MaterializedDatasetResolver {
    async fn load_rows(&self, dataset_id: &str, limit: Option<usize>) -> Result<Vec<JsonValue>> {
        let storage = self.resolve_storage_location(dataset_id)?;
        let path = storage.path.clone();

        tokio::task::spawn_blocking(move || read_parquet_rows(&path, limit))
            .await
            .context("Dataset read task failed")?
    }
}

struct DatasetStorageLocation {
    path: String,
}

fn legacy_parquet_path(dataset_id: &str) -> PathBuf {
    let base_path = std::env::var("PARQUET_PATH").unwrap_or_else(|_| "./data/parquet".to_string());
    PathBuf::from(base_path).join(format!("{}.parquet", dataset_id))
}

fn infer_storage_format(path: &str) -> String {
    Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("parquet")
        .to_string()
}

pub(crate) fn read_parquet_rows(
    parquet_path: &str,
    limit: Option<usize>,
) -> Result<Vec<JsonValue>> {
    let mut file = File::open(parquet_path)
        .with_context(|| format!("Failed to open dataset Parquet file {}", parquet_path))?;

    let metadata = parquet_read::read_metadata(&mut file)
        .with_context(|| format!("Failed to read Parquet metadata from {}", parquet_path))?;
    let schema = parquet_read::infer_schema(&metadata)
        .with_context(|| format!("Failed to infer Parquet schema for {}", parquet_path))?;
    let field_names: Vec<String> = schema
        .fields
        .iter()
        .map(|field| field.name.clone())
        .collect();

    let mut rows = Vec::new();

    'row_groups: for row_group in &metadata.row_groups {
        let mut reader = parquet_read::FileReader::new(
            file.try_clone()?,
            vec![row_group.clone()],
            schema.clone(),
            Some(10_000),
            None,
            None,
        );

        while let Some(chunk) = reader.next() {
            let chunk = chunk?;

            for row_idx in 0..chunk.len() {
                let mut row = JsonMap::with_capacity(field_names.len());

                for (column_idx, array) in chunk.arrays().iter().enumerate() {
                    row.insert(
                        field_names[column_idx].clone(),
                        parquet_value_to_json(array.as_ref(), row_idx)?,
                    );
                }

                rows.push(JsonValue::Object(row));

                if limit
                    .map(|row_limit| rows.len() >= row_limit)
                    .unwrap_or(false)
                {
                    break 'row_groups;
                }
            }
        }
    }

    Ok(rows)
}

fn parquet_value_to_json(array: &dyn Array, row_idx: usize) -> Result<JsonValue> {
    if array.is_null(row_idx) {
        return Ok(JsonValue::Null);
    }

    match array.data_type() {
        arrow2::datatypes::DataType::Boolean => {
            let values = array
                .as_any()
                .downcast_ref::<BooleanArray>()
                .ok_or_else(|| anyhow!("Expected BooleanArray"))?;
            Ok(JsonValue::Bool(values.value(row_idx)))
        }
        arrow2::datatypes::DataType::Int64 => {
            let values = array
                .as_any()
                .downcast_ref::<PrimitiveArray<i64>>()
                .ok_or_else(|| anyhow!("Expected Int64 array"))?;
            Ok(JsonValue::Number(values.value(row_idx).into()))
        }
        arrow2::datatypes::DataType::Int32 => {
            let values = array
                .as_any()
                .downcast_ref::<PrimitiveArray<i32>>()
                .ok_or_else(|| anyhow!("Expected Int32 array"))?;
            Ok(JsonValue::Number((values.value(row_idx) as i64).into()))
        }
        arrow2::datatypes::DataType::UInt64 => {
            let values = array
                .as_any()
                .downcast_ref::<PrimitiveArray<u64>>()
                .ok_or_else(|| anyhow!("Expected UInt64 array"))?;
            Ok(JsonValue::Number(values.value(row_idx).into()))
        }
        arrow2::datatypes::DataType::UInt32 => {
            let values = array
                .as_any()
                .downcast_ref::<PrimitiveArray<u32>>()
                .ok_or_else(|| anyhow!("Expected UInt32 array"))?;
            Ok(JsonValue::Number((values.value(row_idx) as u64).into()))
        }
        arrow2::datatypes::DataType::Float64 => {
            let values = array
                .as_any()
                .downcast_ref::<PrimitiveArray<f64>>()
                .ok_or_else(|| anyhow!("Expected Float64 array"))?;
            let value = values.value(row_idx);
            Ok(JsonValue::Number(JsonNumber::from_f64(value).ok_or_else(
                || anyhow!("Float64 value {} cannot be represented in JSON", value),
            )?))
        }
        arrow2::datatypes::DataType::Float32 => {
            let values = array
                .as_any()
                .downcast_ref::<PrimitiveArray<f32>>()
                .ok_or_else(|| anyhow!("Expected Float32 array"))?;
            let value = values.value(row_idx) as f64;
            Ok(JsonValue::Number(JsonNumber::from_f64(value).ok_or_else(
                || anyhow!("Float32 value {} cannot be represented in JSON", value),
            )?))
        }
        arrow2::datatypes::DataType::Utf8 => {
            let values = array
                .as_any()
                .downcast_ref::<Utf8Array<i32>>()
                .ok_or_else(|| anyhow!("Expected Utf8 array"))?;
            Ok(JsonValue::String(values.value(row_idx).to_string()))
        }
        arrow2::datatypes::DataType::LargeUtf8 => {
            let values = array
                .as_any()
                .downcast_ref::<Utf8Array<i64>>()
                .ok_or_else(|| anyhow!("Expected LargeUtf8 array"))?;
            Ok(JsonValue::String(values.value(row_idx).to_string()))
        }
        arrow2::datatypes::DataType::Date32 => {
            let values = array
                .as_any()
                .downcast_ref::<PrimitiveArray<i32>>()
                .ok_or_else(|| anyhow!("Expected Date32 array"))?;
            let epoch = NaiveDate::from_ymd_opt(1970, 1, 1).unwrap();
            let date = epoch + Duration::days(values.value(row_idx) as i64);
            Ok(JsonValue::String(date.format("%Y-%m-%d").to_string()))
        }
        arrow2::datatypes::DataType::Timestamp(unit, _) => {
            let values = array
                .as_any()
                .downcast_ref::<PrimitiveArray<i64>>()
                .ok_or_else(|| anyhow!("Expected Timestamp array"))?;
            Ok(JsonValue::String(timestamp_to_rfc3339(
                values.value(row_idx),
                *unit,
            )?))
        }
        data_type => bail!(
            "Unsupported Parquet column type for workflow dataset input: {:?}",
            data_type
        ),
    }
}

fn timestamp_to_rfc3339(value: i64, unit: TimeUnit) -> Result<String> {
    let (seconds, nanos) = match unit {
        TimeUnit::Second => (value, 0),
        TimeUnit::Millisecond => (
            value.div_euclid(1_000),
            (value.rem_euclid(1_000) as u32) * 1_000_000,
        ),
        TimeUnit::Microsecond => (
            value.div_euclid(1_000_000),
            (value.rem_euclid(1_000_000) as u32) * 1_000,
        ),
        TimeUnit::Nanosecond => (
            value.div_euclid(1_000_000_000),
            value.rem_euclid(1_000_000_000) as u32,
        ),
    };

    let timestamp = chrono::DateTime::<Utc>::from_timestamp(seconds, nanos)
        .ok_or_else(|| anyhow!("Invalid timestamp value {}", value))?;

    Ok(timestamp.to_rfc3339())
}

#[cfg(test)]
mod tests {
    use super::read_parquet_rows;
    use crate::api::handlers::datasets::write_query_result_to_parquet;
    use graphica_core::catalog::api_types::{
        ColumnDefinition as CatalogColumnDefinition, QueryResult,
    };
    use serde_json::json;
    use tempfile::TempDir;

    #[test]
    fn test_read_parquet_rows_round_trip() {
        let temp_dir = TempDir::new().unwrap();
        let parquet_path = temp_dir.path().join("dataset.parquet");
        let parquet_path_str = parquet_path.to_str().unwrap();

        let query_result = QueryResult {
            rows: vec![
                json!({"id": 1, "name": "Alice", "active": true}),
                json!({"id": 2, "name": "Bob", "active": false}),
            ],
            row_count: 2,
            execution_time_ms: 5,
            truncated: false,
            columns: Some(vec![
                CatalogColumnDefinition {
                    name: "id".to_string(),
                    data_type: "INTEGER".to_string(),
                    nullable: false,
                    primary_key: true,
                    default_value: None,
                    semantic_type: None,
                    statistics: None,
                },
                CatalogColumnDefinition {
                    name: "name".to_string(),
                    data_type: "VARCHAR".to_string(),
                    nullable: false,
                    primary_key: false,
                    default_value: None,
                    semantic_type: None,
                    statistics: None,
                },
                CatalogColumnDefinition {
                    name: "active".to_string(),
                    data_type: "BOOLEAN".to_string(),
                    nullable: false,
                    primary_key: false,
                    default_value: None,
                    semantic_type: None,
                    statistics: None,
                },
            ]),
        };

        write_query_result_to_parquet(&query_result, parquet_path_str).unwrap();

        let rows = read_parquet_rows(parquet_path_str, None).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["id"], 1);
        assert_eq!(rows[0]["name"], "Alice");
        assert_eq!(rows[1]["active"], false);
    }
}
