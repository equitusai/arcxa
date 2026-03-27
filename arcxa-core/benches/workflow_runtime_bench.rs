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

use anyhow::Result;
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use graphica_core::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry, ModelInvoker};
use graphica_core::orchestration::rules::RuleExecutor;
use graphica_core::orchestration::workflow::definition::{
    DataValidatorConfig, DbExtractConfig, DbLoaderConfig, FallbackStrategy, FieldTransformation,
    FieldTransformerConfig, LoadMode, RuleType, Severity, StepConfig, StepType,
    TransformOperation, ValidationRule, WorkflowDefinition, WorkflowStep,
};
use graphica_core::orchestration::workflow::executor::{
    DbExtractCallback, DbExtractResult, DbLoaderCallback, ExecutionContext,
};
use graphica_core::orchestration::workflow::input::{DatasetResolver, WorkflowInput};
use graphica_core::orchestration::workflow::{DatasetInputAdapter, WorkflowEngine};
use serde_json::{json, Map, Value as JsonValue};

fn benchmark_sizes() -> Vec<usize> {
    match env::var("ARCXA_WORKFLOW_BENCH_PROFILE")
        .unwrap_or_else(|_| "quick".to_string())
        .as_str()
    {
        "baseline" => vec![10_000, 100_000, 1_000_000],
        _ => vec![10_000, 100_000],
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
        .map(|row| row.as_object().cloned().expect("benchmark rows to be objects"))
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
    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    let mut group = c.benchmark_group("workflow_runtime/dataset_transform");
    let workflow_input_batch_size = 10_000;

    for size in benchmark_sizes() {
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
        if size >= 100_000 {
            group.sample_size(10);
        }

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
    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    let mut group = c.benchmark_group("workflow_runtime/dataset_transform_validate");
    let workflow_input_batch_size = 10_000;

    for size in benchmark_sizes() {
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
        if size >= 100_000 {
            group.sample_size(10);
        }

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
    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    let mut group = c.benchmark_group("workflow_runtime/extract_transform_load");

    for size in benchmark_sizes() {
        let rows = Arc::new(rows_as_maps(&generate_rows(size)));

        let extract_rows = rows.clone();
        let extract_callback: Arc<DbExtractCallback> = Arc::new(Box::new(
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

        let load_callback: Arc<DbLoaderCallback> = Arc::new(Box::new(
            move |_datasource_id: &str,
                  _table_name: &str,
                  rows: Vec<Map<String, JsonValue>>,
                  _mode: &str,
                  _key_fields: Option<Vec<String>>|
                  -> Pin<Box<dyn std::future::Future<Output = Result<u64>> + Send>> {
                Box::pin(async move { Ok(rows.len() as u64) })
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
        if size >= 100_000 {
            group.sample_size(10);
        }

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

criterion_group!(
    benches,
    benchmark_dataset_transform,
    benchmark_dataset_transform_validate,
    benchmark_extract_transform_load
);
criterion_main!(benches);
