//! Canonical workflow runtime benchmarks for the performance hardening roadmap.
//!
//! These benchmarks focus on workflow-runtime paths we are actively migrating:
//! - dataset input -> transform
//! - dataset input -> transform -> validate
//! - db_extract -> transform -> db_loader
//!
//! Use the repo wrapper to run this bench in a Conda-safe environment:
//! `./benchmark-workflow-runtime.sh`

use std::collections::HashMap;
use std::env;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use criterion::measurement::WallTime;
use criterion::{
    black_box, criterion_group, criterion_main, BenchmarkGroup, BenchmarkId, Criterion,
    SamplingMode, Throughput,
};
use graphica_core::orchestration::ml::{CacheConfig, ModelCache, ModelInvoker, ModelRegistry};
use graphica_core::orchestration::rules::RuleExecutor;
use graphica_core::orchestration::workflow::definition::{
    DataValidatorConfig, DbExtractConfig, DbLoaderConfig, FallbackStrategy, FieldTransformation,
    FieldTransformerConfig, LoadMode, RuleType, Severity, StepConfig, StepType, TransformOperation,
    ValidationRule, WorkflowDefinition, WorkflowStep,
};
use graphica_core::orchestration::workflow::executor::{
    DbExtractCallback, DbExtractResult, DbLoadResult, DbLoaderCallback, ExecutionContext,
};
use graphica_core::orchestration::workflow::input::{
    DatasetResolver, JsonInputAdapter, WorkflowInput,
};
#[cfg(feature = "workflow-storage")]
use graphica_core::orchestration::workflow::{
    BatchFrame, ExecutionContextV2, StorageManager, StorageType as WorkflowStorageType,
};
use graphica_core::orchestration::workflow::{DatasetInputAdapter, WorkflowEngine};
use serde_json::{json, Map, Value as JsonValue};
#[cfg(feature = "workflow-storage")]
use tempfile::tempdir;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BenchProfile {
    Quick,
    Baseline,
}

impl BenchProfile {
    fn from_env() -> Self {
        match env::var("ARCXA_WORKFLOW_BENCH_PROFILE")
            .unwrap_or_else(|_| "quick".to_string())
            .as_str()
        {
            "baseline" => Self::Baseline,
            _ => Self::Quick,
        }
    }

    fn dataset_sizes(self) -> Vec<usize> {
        match self {
            Self::Baseline => vec![10_000, 100_000, 1_000_000],
            Self::Quick => vec![10_000, 100_000],
        }
    }

    fn stream_micro_batch_sizes(self) -> Vec<usize> {
        match self {
            Self::Baseline => vec![1_000, 5_000, 10_000],
            Self::Quick => vec![1_000, 5_000],
        }
    }

    #[cfg(feature = "workflow-storage")]
    fn storage_tiering_cases(self) -> Vec<(&'static str, usize, WorkflowStorageType)> {
        match self {
            Self::Baseline => vec![
                ("rocksdb_round_trip", 150_000, WorkflowStorageType::RocksDB),
                ("rocksdb_round_trip", 500_000, WorkflowStorageType::RocksDB),
                (
                    "parquet_round_trip",
                    1_200_000,
                    WorkflowStorageType::Parquet,
                ),
                (
                    "parquet_round_trip",
                    2_000_000,
                    WorkflowStorageType::Parquet,
                ),
            ],
            Self::Quick => vec![],
        }
    }
}

fn configure_group(group: &mut BenchmarkGroup<'_, WallTime>, profile: BenchProfile, rows: usize) {
    group.sampling_mode(SamplingMode::Flat);

    match (profile, rows) {
        (BenchProfile::Quick, 0..=5_000) => {
            group.sample_size(10);
            group.warm_up_time(Duration::from_millis(750));
            group.measurement_time(Duration::from_secs(4));
        }
        (BenchProfile::Quick, 5_001..=100_000) => {
            group.sample_size(10);
            group.warm_up_time(Duration::from_secs(1));
            group.measurement_time(Duration::from_secs(30));
        }
        (BenchProfile::Quick, _) => {
            group.sample_size(10);
            group.warm_up_time(Duration::from_secs(2));
            group.measurement_time(Duration::from_secs(40));
        }
        (BenchProfile::Baseline, 0..=10_000) => {
            group.sample_size(15);
            group.warm_up_time(Duration::from_secs(1));
            group.measurement_time(Duration::from_secs(6));
        }
        (BenchProfile::Baseline, 10_001..=100_000) => {
            group.sample_size(10);
            group.warm_up_time(Duration::from_secs(2));
            group.measurement_time(Duration::from_secs(30));
        }
        (BenchProfile::Baseline, _) => {
            group.sample_size(10);
            group.warm_up_time(Duration::from_secs(2));
            group.measurement_time(Duration::from_secs(40));
        }
    }
}

#[cfg(feature = "workflow-storage")]
fn configure_storage_group(
    group: &mut BenchmarkGroup<'_, WallTime>,
    profile: BenchProfile,
    rows: usize,
) {
    group.sampling_mode(SamplingMode::Flat);

    match (profile, rows) {
        (BenchProfile::Baseline, 0..=200_000) => {
            group.sample_size(10);
            group.warm_up_time(Duration::from_secs(1));
            group.measurement_time(Duration::from_secs(10));
        }
        (BenchProfile::Baseline, 200_001..=1_500_000) => {
            group.sample_size(10);
            group.warm_up_time(Duration::from_secs(1));
            group.measurement_time(Duration::from_secs(40));
        }
        (BenchProfile::Baseline, _) => {
            group.sample_size(10);
            group.warm_up_time(Duration::from_secs(1));
            group.measurement_time(Duration::from_secs(120));
        }
        (BenchProfile::Quick, _) => {
            group.sample_size(10);
            group.warm_up_time(Duration::from_secs(1));
            group.measurement_time(Duration::from_secs(8));
        }
    }
}

fn generate_rows(count: usize) -> Vec<JsonValue> {
    (0..count)
        .map(|index| {
            json!({
                "id": index as i64,
                "status": if index % 2 == 0 { "ACTIVE" } else { "PENDING" },
                "amount": ((index % 1000) as f64) + 0.25,
            })
        })
        .collect()
}

fn rows_as_maps(rows: &[JsonValue]) -> Vec<Map<String, JsonValue>> {
    rows.iter()
        .map(|row| {
            row.as_object()
                .cloned()
                .expect("benchmark rows to be objects")
        })
        .collect()
}

#[derive(Clone)]
struct BenchDatasetResolver {
    rows: Arc<Vec<JsonValue>>,
}

#[async_trait::async_trait]
impl DatasetResolver for BenchDatasetResolver {
    async fn load_rows(&self, _dataset_id: &str, limit: Option<usize>) -> Result<Vec<JsonValue>> {
        Ok(match limit {
            Some(limit) => self.rows.iter().take(limit).cloned().collect(),
            None => self.rows.as_ref().clone(),
        })
    }
}

fn create_runtime_engine() -> WorkflowEngine {
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).expect("model invoker"));
    let rule_executor = Arc::new(RuleExecutor::new());
    WorkflowEngine::new_with_execution(invoker, rule_executor)
}

fn transform_definition() -> WorkflowDefinition {
    WorkflowDefinition {
        steps: vec![WorkflowStep {
            id: "transform1".to_string(),
            step_type: StepType::FieldTransformer,
            config: StepConfig::FieldTransformer(FieldTransformerConfig {
                transformations: vec![FieldTransformation {
                    field: "status".to_string(),
                    operations: vec![TransformOperation::Lower],
                }],
            }),
            depends_on: vec![],
        }],
        fusion_threshold: 0.8,
        fallback: FallbackStrategy::ManualReview,
    }
}

fn transform_validate_definition() -> WorkflowDefinition {
    WorkflowDefinition {
        steps: vec![
            WorkflowStep {
                id: "transform1".to_string(),
                step_type: StepType::FieldTransformer,
                config: StepConfig::FieldTransformer(FieldTransformerConfig {
                    transformations: vec![FieldTransformation {
                        field: "status".to_string(),
                        operations: vec![TransformOperation::Lower],
                    }],
                }),
                depends_on: vec![],
            },
            WorkflowStep {
                id: "validate1".to_string(),
                step_type: StepType::DataValidator,
                config: StepConfig::DataValidator(DataValidatorConfig {
                    rules: vec![ValidationRule {
                        field: "status".to_string(),
                        rule_type: RuleType::InSet {
                            values: vec!["active".to_string(), "pending".to_string()],
                        },
                        params: None,
                        severity: Severity::Error,
                    }],
                    fail_on_error: false,
                }),
                depends_on: vec!["transform1".to_string()],
            },
        ],
        fusion_threshold: 0.8,
        fallback: FallbackStrategy::ManualReview,
    }
}

fn extract_transform_load_definition() -> WorkflowDefinition {
    WorkflowDefinition {
        steps: vec![
            WorkflowStep {
                id: "extract1".to_string(),
                step_type: StepType::DbExtract,
                config: StepConfig::DbExtract(DbExtractConfig {
                    datasource_id: "bench_source".to_string(),
                    table_name: Some("bench_source_table".to_string()),
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
                depends_on: vec![],
            },
            WorkflowStep {
                id: "transform1".to_string(),
                step_type: StepType::FieldTransformer,
                config: StepConfig::FieldTransformer(FieldTransformerConfig {
                    transformations: vec![FieldTransformation {
                        field: "status".to_string(),
                        operations: vec![TransformOperation::Lower],
                    }],
                }),
                depends_on: vec!["extract1".to_string()],
            },
            WorkflowStep {
                id: "load1".to_string(),
                step_type: StepType::DbLoader,
                config: StepConfig::DbLoader(DbLoaderConfig {
                    datasource_id: "bench_target".to_string(),
                    table_name: "bench_target_table".to_string(),
                    mode: LoadMode::Insert,
                    key_fields: None,
                    batch_size: 50_000,
                    create_table: false,
                    entity_uri: None,
                }),
                depends_on: vec!["transform1".to_string()],
            },
        ],
        fusion_threshold: 0.8,
        fallback: FallbackStrategy::ManualReview,
    }
}

fn benchmark_dataset_transform(c: &mut Criterion) {
    let profile = BenchProfile::from_env();
    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    let mut group = c.benchmark_group("workflow_runtime/dataset_transform");
    let workflow_input_batch_size = 10_000;

    for size in profile.dataset_sizes() {
        let rows = Arc::new(generate_rows(size));
        let engine = Arc::new(create_runtime_engine());
        let workflow_id = runtime
            .block_on(async {
                engine
                    .register_workflow(
                        format!("bench_dataset_transform_{}", size),
                        "bench_dataset_transform".to_string(),
                        transform_definition(),
                        None,
                        vec![],
                    )
                    .await
            })
            .expect("workflow registration");
        let adapter = Arc::new(DatasetInputAdapter::new(Arc::new(BenchDatasetResolver {
            rows: rows.clone(),
        })));
        let context = Arc::new(HashMap::new());

        group.throughput(Throughput::Elements(size as u64));
        configure_group(&mut group, profile, size);

        group.bench_with_input(BenchmarkId::new("execute", size), &size, |b, _| {
            let engine = engine.clone();
            let workflow_id = workflow_id.clone();
            let adapter = adapter.clone();
            let context = context.clone();

            b.to_async(&runtime).iter(|| {
                let engine = engine.clone();
                let workflow_id = workflow_id.clone();
                let adapter = adapter.clone();
                let context = context.clone();

                async move {
                    let results = engine
                        .execute_workflow_with_input(
                            &workflow_id,
                            WorkflowInput::Dataset {
                                dataset_id: "bench_dataset".to_string(),
                                batch_size: Some(workflow_input_batch_size),
                                limit: None,
                            },
                            adapter,
                            context.as_ref(),
                        )
                        .await
                        .expect("workflow execution");
                    black_box(results[0].step_results.len())
                }
            });
        });
    }

    group.finish();
}

fn benchmark_dataset_transform_validate(c: &mut Criterion) {
    let profile = BenchProfile::from_env();
    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    let mut group = c.benchmark_group("workflow_runtime/dataset_transform_validate");
    let workflow_input_batch_size = 10_000;

    for size in profile.dataset_sizes() {
        let rows = Arc::new(generate_rows(size));
        let engine = Arc::new(create_runtime_engine());
        let workflow_id = runtime
            .block_on(async {
                engine
                    .register_workflow(
                        format!("bench_dataset_transform_validate_{}", size),
                        "bench_dataset_transform_validate".to_string(),
                        transform_validate_definition(),
                        None,
                        vec![],
                    )
                    .await
            })
            .expect("workflow registration");
        let adapter = Arc::new(DatasetInputAdapter::new(Arc::new(BenchDatasetResolver {
            rows: rows.clone(),
        })));
        let context = Arc::new(HashMap::new());

        group.throughput(Throughput::Elements(size as u64));
        configure_group(&mut group, profile, size);

        group.bench_with_input(BenchmarkId::new("execute", size), &size, |b, _| {
            let engine = engine.clone();
            let workflow_id = workflow_id.clone();
            let adapter = adapter.clone();
            let context = context.clone();

            b.to_async(&runtime).iter(|| {
                let engine = engine.clone();
                let workflow_id = workflow_id.clone();
                let adapter = adapter.clone();
                let context = context.clone();

                async move {
                    let results = engine
                        .execute_workflow_with_input(
                            &workflow_id,
                            WorkflowInput::Dataset {
                                dataset_id: "bench_dataset".to_string(),
                                batch_size: Some(workflow_input_batch_size),
                                limit: None,
                            },
                            adapter,
                            context.as_ref(),
                        )
                        .await
                        .expect("workflow execution");
                    black_box(results[0].step_results.len())
                }
            });
        });
    }

    group.finish();
}

fn benchmark_extract_transform_load(c: &mut Criterion) {
    let profile = BenchProfile::from_env();
    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    let mut group = c.benchmark_group("workflow_runtime/extract_transform_load");

    for size in profile.dataset_sizes() {
        let rows = Arc::new(rows_as_maps(&generate_rows(size)));

        let extract_rows = rows.clone();
        let extract_callback: Arc<DbExtractCallback> =
            Arc::new(Box::new(
                move |_config: &DbExtractConfig,
                      _context: &ExecutionContext|
                      -> Pin<
                    Box<dyn std::future::Future<Output = Result<DbExtractResult>> + Send>,
                > {
                    let extract_rows = extract_rows.clone();
                    Box::pin(async move {
                        Ok(DbExtractResult {
                            row_count: extract_rows.len(),
                            rows: extract_rows.as_ref().clone(),
                            schema: None,
                        })
                    })
                },
            ));

        let load_callback: Arc<DbLoaderCallback> =
            Arc::new(Box::new(
                move |_datasource_id: &str,
                      _table_name: &str,
                      rows: Vec<Map<String, JsonValue>>,
                      _mode: &str,
                      _key_fields: Option<Vec<String>>|
                      -> Pin<
                    Box<dyn std::future::Future<Output = Result<DbLoadResult>> + Send>,
                > {
                    Box::pin(async move {
                        Ok(DbLoadResult {
                            rows_loaded: rows.len() as u64,
                            output_row_ids: vec![None; rows.len()],
                        })
                    })
                },
            ));

        let engine = Arc::new(
            create_runtime_engine()
                .with_db_extract_callback(extract_callback)
                .with_db_loader_callback(load_callback),
        );
        let workflow_id = runtime
            .block_on(async {
                engine
                    .register_workflow(
                        format!("bench_extract_transform_load_{}", size),
                        "bench_extract_transform_load".to_string(),
                        extract_transform_load_definition(),
                        None,
                        vec![],
                    )
                    .await
            })
            .expect("workflow registration");
        let context = Arc::new(HashMap::new());

        group.throughput(Throughput::Elements(size as u64));
        configure_group(&mut group, profile, size);

        group.bench_with_input(BenchmarkId::new("execute", size), &size, |b, _| {
            let engine = engine.clone();
            let workflow_id = workflow_id.clone();
            let context = context.clone();

            b.to_async(&runtime).iter(|| {
                let engine = engine.clone();
                let workflow_id = workflow_id.clone();
                let context = context.clone();

                async move {
                    let result = engine
                        .execute_workflow(&workflow_id, json!({}), context.as_ref())
                        .await
                        .expect("workflow execution");
                    black_box(result.step_results.len())
                }
            });
        });
    }

    group.finish();
}

fn benchmark_stream_micro_batch_transform_validate(c: &mut Criterion) {
    let profile = BenchProfile::from_env();
    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    let mut group = c.benchmark_group("workflow_runtime/stream_micro_batch_transform_validate");
    let adapter = Arc::new(JsonInputAdapter);

    for size in profile.stream_micro_batch_sizes() {
        let rows = generate_rows(size);
        let input = WorkflowInput::Json {
            data: json!({
                "_rows": rows,
                "_stream_source": "benchmark",
                "_micro_batch_size": size,
            }),
        };
        let engine = Arc::new(create_runtime_engine());
        let workflow_id = runtime
            .block_on(async {
                engine
                    .register_workflow(
                        format!("bench_stream_micro_batch_transform_validate_{}", size),
                        "bench_stream_micro_batch_transform_validate".to_string(),
                        transform_validate_definition(),
                        None,
                        vec![],
                    )
                    .await
            })
            .expect("workflow registration");
        let context = Arc::new(HashMap::new());

        group.throughput(Throughput::Elements(size as u64));
        configure_group(&mut group, profile, size);

        group.bench_with_input(BenchmarkId::new("execute", size), &size, |b, _| {
            let engine = engine.clone();
            let workflow_id = workflow_id.clone();
            let adapter = adapter.clone();
            let input = input.clone();
            let context = context.clone();

            b.to_async(&runtime).iter(|| {
                let engine = engine.clone();
                let workflow_id = workflow_id.clone();
                let adapter = adapter.clone();
                let input = input.clone();
                let context = context.clone();

                async move {
                    let results = engine
                        .execute_workflow_with_input(&workflow_id, input, adapter, context.as_ref())
                        .await
                        .expect("workflow execution");
                    black_box(results[0].step_results.len())
                }
            });
        });
    }

    group.finish();
}

#[cfg(feature = "workflow-storage")]
fn benchmark_storage_tiering_round_trip(c: &mut Criterion) {
    let profile = BenchProfile::from_env();
    let cases = profile.storage_tiering_cases();
    if cases.is_empty() {
        return;
    }

    let rocks_dir = tempdir().expect("rocks tempdir");
    let temp_dir = tempdir().expect("spill tempdir");
    let storage_manager =
        Arc::new(StorageManager::new(rocks_dir.path(), temp_dir.path()).expect("storage manager"));
    let mut group = c.benchmark_group("workflow_runtime/storage_tiering_round_trip");

    for (case_name, size, expected_storage_type) in cases {
        let rows = Arc::new(generate_rows(size));
        group.throughput(Throughput::Elements(size as u64));
        configure_storage_group(&mut group, profile, size);

        group.bench_with_input(BenchmarkId::new(case_name, size), &size, |b, _| {
            let rows = rows.clone();
            let storage_manager = storage_manager.clone();

            b.iter(|| {
                let mut ctx =
                    ExecutionContextV2::with_storage_manager(json!({}), storage_manager.clone());
                ctx.resource_limits.max_row_count = size.saturating_mul(2);
                ctx.resource_limits.max_memory_bytes = usize::MAX / 4;
                ctx.set_current_step(case_name.to_string());
                ctx.set_rows(rows.as_ref().clone())
                    .expect("storage placement to succeed");

                let actual_storage_type = ctx
                    .row_storage
                    .as_ref()
                    .expect("row storage to be present")
                    .storage_type();
                assert_eq!(actual_storage_type, expected_storage_type);

                let row_count = ctx
                    .get_rows()
                    .expect("row accessor")
                    .to_vec()
                    .expect("materialize")
                    .len();
                let reserved_bytes = ctx.get_metrics().total_spill_reserved_bytes_current;
                let storage_location = ctx
                    .get_metrics()
                    .recent_storage_decisions
                    .last()
                    .and_then(|decision| decision.storage_location.clone());

                ctx.cleanup().expect("cleanup");

                black_box((row_count, reserved_bytes, storage_location))
            });
        });
    }

    group.finish();
}

#[cfg(not(feature = "workflow-storage"))]
fn benchmark_storage_tiering_round_trip(_: &mut Criterion) {}

#[cfg(feature = "workflow-storage")]
fn benchmark_storage_tiering_batch_frame_round_trip(c: &mut Criterion) {
    let profile = BenchProfile::from_env();
    let cases = profile.storage_tiering_cases();
    if cases.is_empty() {
        return;
    }

    let rocks_dir = tempdir().expect("rocks tempdir");
    let temp_dir = tempdir().expect("spill tempdir");
    let storage_manager =
        Arc::new(StorageManager::new(rocks_dir.path(), temp_dir.path()).expect("storage manager"));
    let mut group = c.benchmark_group("workflow_runtime/storage_tiering_batch_frame_round_trip");

    for (case_name, size, expected_storage_type) in cases {
        let frame = Arc::new(BatchFrame::from_json_values(&generate_rows(size)).expect("frame"));
        group.throughput(Throughput::Elements(size as u64));
        configure_storage_group(&mut group, profile, size);

        group.bench_with_input(BenchmarkId::new(case_name, size), &size, |b, _| {
            let frame = frame.clone();
            let storage_manager = storage_manager.clone();

            b.iter(|| {
                let mut ctx =
                    ExecutionContextV2::with_storage_manager(json!({}), storage_manager.clone());
                ctx.resource_limits.max_row_count = size.saturating_mul(2);
                ctx.resource_limits.max_memory_bytes = usize::MAX / 4;
                ctx.set_current_step(case_name.to_string());
                ctx.set_batch_frame((*frame).clone())
                    .expect("storage placement to succeed");

                let actual_storage_type = ctx
                    .row_storage
                    .as_ref()
                    .expect("row storage to be present")
                    .storage_type();
                assert_eq!(actual_storage_type, expected_storage_type);

                let frame = ctx.get_batch_frame().expect("batch frame");
                let row_count = frame.row_count();
                let column_count = frame.schema().fields.len();
                let reserved_bytes = ctx.get_metrics().total_spill_reserved_bytes_current;
                let storage_location = ctx
                    .get_metrics()
                    .recent_storage_decisions
                    .last()
                    .and_then(|decision| decision.storage_location.clone());

                ctx.cleanup().expect("cleanup");

                black_box((row_count, column_count, reserved_bytes, storage_location))
            });
        });
    }

    group.finish();
}

#[cfg(not(feature = "workflow-storage"))]
fn benchmark_storage_tiering_batch_frame_round_trip(_: &mut Criterion) {}

criterion_group!(
    benches,
    benchmark_dataset_transform,
    benchmark_dataset_transform_validate,
    benchmark_extract_transform_load,
    benchmark_stream_micro_batch_transform_validate,
    benchmark_storage_tiering_round_trip,
    benchmark_storage_tiering_batch_frame_round_trip
);
criterion_main!(benches);
