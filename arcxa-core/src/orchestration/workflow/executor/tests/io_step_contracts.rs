use super::*;
use crate::orchestration::workflow::definition::{ConfidenceGateConfig, FallbackStrategy};

fn create_test_workflow() -> WorkflowDefinition {
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
async fn test_execute_db_extract_falls_back_to_stub_output_contract() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;
    use crate::orchestration::workflow::definition::DbExtractConfig;

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let context = ExecutionContext::new(serde_json::json!({}));
    let config = DbExtractConfig {
        datasource_id: "stub_ds".to_string(),
        table_name: Some("customers".to_string()),
        schema_table: None,
        query: Some("SELECT * FROM customers".to_string()),
        incremental: None,
        incremental_column: None,
        last_value: None,
        batch_size: 1000,
        columns: None,
        include_schema: Some(false),
        schema_sample_size: None,
    };

    let result = executor
        .execute_db_extract(&config, &context)
        .await
        .expect("stub db_extract path should execute");

    assert!(result.success);
    assert!(result.batch_frame.is_none());
    assert_eq!(result.confidence, 1.0);
    assert_eq!(result.output["_row_count"], serde_json::json!(0));
    assert_eq!(result.output["_rows"], serde_json::json!([]));
    assert_eq!(result.output["_datasource_id"], "stub_ds");
    assert_eq!(result.output["_table_name"], "customers");
    assert_eq!(result.output["_query"], "SELECT * FROM customers");
    assert_eq!(result.output["_status"], "stub_implementation");
}

#[tokio::test]
async fn test_execute_csv_source_preserves_legacy_output_contract() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;
    use crate::orchestration::workflow::definition::CsvSourceConfig;
    use std::io::Write;
    use tempfile::NamedTempFile;

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let mut csv_file = NamedTempFile::new().unwrap();
    writeln!(csv_file, "id,name").unwrap();
    writeln!(csv_file, "1,Alice").unwrap();
    writeln!(csv_file, "2,Bob").unwrap();
    csv_file.flush().unwrap();

    let config = CsvSourceConfig {
        file_path: csv_file.path().to_string_lossy().into_owned(),
        delimiter: Some(','),
        has_header: Some(true),
        encoding: None,
        skip_rows: None,
        max_rows: None,
    };
    let context = ExecutionContext::new(serde_json::json!({}));

    let (success, output, confidence) = executor
        .execute_csv_source(&config, &context)
        .await
        .expect("csv source should execute");

    assert!(success);
    assert_eq!(confidence, 1.0);
    assert_eq!(output["_row_count"], serde_json::json!(2));
    assert_eq!(output["_columns"], serde_json::json!(["id", "name"]));
    assert_eq!(output["_source_file"], config.file_path);
    assert_eq!(output["_rows"][0]["id"], "1");
    assert_eq!(output["_rows"][0]["name"], "Alice");
    assert_eq!(output["_rows"][1]["id"], "2");
    assert_eq!(output["_rows"][1]["name"], "Bob");
    assert_eq!(output["_modifications"].as_array().unwrap().len(), 1);
    assert_eq!(output["_modifications"][0]["field_name"], "_source");
}

#[tokio::test]
async fn test_execute_db_loader_falls_back_to_stub_output_contract() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;
    use crate::orchestration::workflow::definition::{DbLoaderConfig, LoadMode};

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let context = ExecutionContext::new(serde_json::json!({
        "_rows": [
            {"id": 1, "name": "Alice"},
            {"id": 2, "name": "Bob"}
        ]
    }));
    let config = DbLoaderConfig {
        datasource_id: "stub_target".to_string(),
        table_name: "customers".to_string(),
        mode: LoadMode::Insert,
        key_fields: None,
        batch_size: 1000,
        create_table: false,
        entity_uri: None,
    };

    let (success, output, confidence) = executor
        .execute_db_loader(&config, &context)
        .await
        .expect("stub db_loader path should execute");

    assert!(success);
    assert_eq!(confidence, 1.0);
    assert_eq!(output["_datasource_id"], "stub_target");
    assert_eq!(output["_table_name"], "customers");
    assert_eq!(output["_rows_to_load"], serde_json::json!(2));
    assert_eq!(output["_mode"], "Insert");
    assert_eq!(output["_status"], "stub_implementation");
}

#[tokio::test]
async fn test_execute_db_loader_uses_callback_output_contract() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;
    use crate::orchestration::workflow::definition::{DbLoaderConfig, LoadMode};

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor)
        .unwrap()
        .with_db_loader_callback(Arc::new(Box::new(
            |datasource_id, table_name, rows, mode, key_fields| {
                let datasource_id = datasource_id.to_string();
                let table_name = table_name.to_string();
                let mode = mode.to_string();
                Box::pin(async move {
                    assert_eq!(datasource_id, "target_ds");
                    assert_eq!(table_name, "customers");
                    assert_eq!(mode, "Upsert");
                    assert_eq!(key_fields, Some(vec!["id".to_string()]));
                    Ok(rows.len() as u64)
                })
            },
        )));

    let context = ExecutionContext::new(serde_json::json!({
        "_rows": [
            {"id": 1, "name": "Alice"},
            {"id": 2, "name": "Bob"}
        ]
    }));
    let config = DbLoaderConfig {
        datasource_id: "target_ds".to_string(),
        table_name: "customers".to_string(),
        mode: LoadMode::Upsert,
        key_fields: Some(vec!["id".to_string()]),
        batch_size: 1000,
        create_table: false,
        entity_uri: None,
    };

    let (success, output, confidence) = executor
        .execute_db_loader(&config, &context)
        .await
        .expect("callback db_loader path should execute");

    assert!(success);
    assert_eq!(confidence, 1.0);
    assert_eq!(output["_datasource_id"], "target_ds");
    assert_eq!(output["_table_name"], "customers");
    assert_eq!(output["_rows_loaded"], serde_json::json!(2));
    assert_eq!(output["_mode"], "Upsert");
    assert_eq!(output["_status"], "success");
}

#[tokio::test]
async fn test_execute_semantic_mapper_falls_back_to_stub_output_contract() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;
    use crate::orchestration::workflow::definition::{MappingMode, SemanticMapperConfig};

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let context = ExecutionContext::new(serde_json::json!({
        "_rows": [
            {"id": 1, "name": "Alice"},
            {"id": 2, "name": "Bob"}
        ]
    }));
    let config = SemanticMapperConfig {
        target_ontology: vec!["Customer".to_string()],
        auto_approve_threshold: 0.95,
        mapping_mode: MappingMode::Manual,
        mapping_session_id: None,
        source_id: None,
        table_name: None,
        entity_uri: None,
    };

    let (success, output, confidence) = executor
        .execute_semantic_mapper(&config, &context)
        .await
        .expect("stub semantic mapper path should execute");

    assert!(success);
    assert_eq!(confidence, 0.0);
    assert_eq!(output["_target_ontology"], serde_json::json!(["Customer"]));
    assert_eq!(output["_mapping_mode"], "Manual");
    assert_eq!(output["_status"], "stub_implementation");
    assert_eq!(output["_rows"][0]["name"], "Alice");
    assert_eq!(output["_rows"][1]["name"], "Bob");
}

#[tokio::test]
async fn test_execute_semantic_mapper_uses_transformer_callback_output_contract() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;
    use crate::orchestration::workflow::definition::{MappingMode, SemanticMapperConfig};

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor)
        .unwrap()
        .with_transformer_callback(Arc::new(Box::new(|name, config, data| {
            let transformer_name = name.to_string();
            let config = config.clone();

            if let Some(rows) = data.get_mut("rows").and_then(|value| value.as_array_mut()) {
                for row in rows {
                    if let Some(object) = row.as_object_mut() {
                        object.insert("mapped_entity".to_string(), serde_json::json!("Customer"));
                    }
                }
            }

            if let Some(object) = data.as_object_mut() {
                object.insert(
                    "ontology_mapping".to_string(),
                    serde_json::json!({"accepted": 2}),
                );
                object.insert(
                    "_modifications".to_string(),
                    serde_json::json!([
                        {
                            "field_name": "mapped_entity",
                            "metadata": {"rows_modified": 2}
                        }
                    ]),
                );
            }

            Box::pin(async move {
                assert_eq!(transformer_name, "ontology_map");
                assert_eq!(config["source_id"], "source_ds");
                assert_eq!(config["table_name"], "customers");
                assert_eq!(config["session_id"], "session_123");
                assert!(
                    config.get("mapping_session_id").is_none(),
                    "semantic mapper callback config should preserve the legacy session_id key"
                );
                assert_eq!(config["entity_uri"], "urn:test:customer");
                Ok(())
            })
        })));

    let context = ExecutionContext::new(serde_json::json!({
        "_rows": [
            {"id": 1, "name": "Alice"},
            {"id": 2, "name": "Bob"}
        ],
        "_schema": {
            "columns": [
                {"name": "id", "type": "integer"},
                {"name": "name", "type": "string"}
            ]
        }
    }));
    let config = SemanticMapperConfig {
        target_ontology: vec!["Customer".to_string()],
        auto_approve_threshold: 0.9,
        mapping_mode: MappingMode::Hybrid,
        mapping_session_id: Some("session_123".to_string()),
        source_id: Some("source_ds".to_string()),
        table_name: Some("customers".to_string()),
        entity_uri: Some("urn:test:customer".to_string()),
    };

    let (success, output, confidence) = executor
        .execute_semantic_mapper(&config, &context)
        .await
        .expect("transformer-backed semantic mapper path should execute");

    assert!(success);
    assert_eq!(confidence, 1.0);
    assert_eq!(output["_row_count"], serde_json::json!(2));
    assert_eq!(output["_rows"][0]["mapped_entity"], "Customer");
    assert_eq!(output["_rows"][1]["mapped_entity"], "Customer");
    assert_eq!(
        output["ontology_mapping"],
        serde_json::json!({"accepted": 2})
    );
    assert_eq!(output["_modifications"].as_array().unwrap().len(), 1);
    assert_eq!(
        output["_modifications"][0]["field_name"],
        serde_json::json!("mapped_entity")
    );
}

#[tokio::test]
async fn test_execute_step_db_extract_attaches_batch_frame_sidecar() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;
    use crate::orchestration::workflow::definition::{
        DbExtractConfig, StepConfig, StepType, WorkflowStep,
    };

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor)
        .unwrap()
        .with_db_extract_callback(Arc::new(Box::new(|_config, _context| {
            Box::pin(async move {
                Ok(DbExtractResult {
                    rows: vec![
                        serde_json::Map::from_iter([("id".to_string(), serde_json::json!(1))]),
                        serde_json::Map::from_iter([
                            ("id".to_string(), serde_json::json!(2)),
                            ("name".to_string(), serde_json::json!("Alice")),
                        ]),
                    ],
                    row_count: 2,
                    schema: Some(serde_json::json!({
                        "columns": [
                            {"name": "id", "type": "integer"},
                            {"name": "name", "type": "string"}
                        ]
                    })),
                })
            })
        })));

    let step = WorkflowStep {
        id: "extract_step".to_string(),
        step_type: StepType::DbExtract,
        config: StepConfig::DbExtract(DbExtractConfig {
            datasource_id: "ds_postgres".to_string(),
            table_name: Some("customers".to_string()),
            schema_table: None,
            query: None,
            incremental: None,
            incremental_column: None,
            last_value: None,
            batch_size: 1000,
            columns: None,
            include_schema: Some(true),
            schema_sample_size: None,
        }),
        depends_on: vec![],
    };
    let context = ExecutionContext::new(serde_json::json!({}));

    let step_result = executor.execute_step(&step, &context).await.unwrap();

    assert!(step_result.success);
    assert_eq!(step_result.output["_row_count"], 2);
    assert_eq!(step_result.output["_rows"][1]["name"], "Alice");
    let batch_frame = step_result
        .batch_frame
        .expect("db_extract callback path should attach a frame sidecar");
    assert_eq!(batch_frame.row_count(), 2);
    assert_eq!(
        batch_frame.metadata().source_step_id.as_deref(),
        Some("extract_step")
    );
    assert_eq!(
        batch_frame.metadata().source_kind.as_deref(),
        Some("db_extract")
    );
}

#[test]
fn test_finalize_step_result_stamps_db_extract_batch_metadata() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;
    use crate::orchestration::workflow::definition::{
        DbExtractConfig, StepConfig, StepType, WorkflowStep,
    };
    use crate::orchestration::workflow::runtime::frame::BatchFrame;

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let step = WorkflowStep {
        id: "extract_step".to_string(),
        step_type: StepType::DbExtract,
        config: StepConfig::DbExtract(DbExtractConfig {
            datasource_id: "ds_postgres".to_string(),
            table_name: Some("customers".to_string()),
            schema_table: None,
            query: None,
            incremental: None,
            incremental_column: None,
            last_value: None,
            batch_size: 1000,
            columns: None,
            include_schema: Some(true),
            schema_sample_size: None,
        }),
        depends_on: vec![],
    };

    let frame =
        BatchFrame::from_json_values(&[serde_json::json!({"id": 1}), serde_json::json!({"id": 2})])
            .unwrap();

    let step_result = executor.finalize_step_result(
        &step,
        chrono::Utc::now(),
        chrono::Utc::now(),
        BatchStepExecutionResult::with_frame(
            true,
            build_rows_output(
                vec![serde_json::json!({"id": 1}), serde_json::json!({"id": 2})],
                2,
                vec![],
            ),
            1.0,
            frame,
        ),
    );

    assert_eq!(
        step_result
            .batch_metadata
            .as_ref()
            .and_then(|metadata| metadata.source_step_id.as_deref()),
        Some("extract_step")
    );
    assert_eq!(
        step_result
            .batch_metadata
            .as_ref()
            .and_then(|metadata| metadata.source_kind.as_deref()),
        Some("db_extract")
    );
    assert!(step_result.batch_frame.is_some());
}

#[test]
fn test_csv_export_batch_path_falls_back_for_unsupported_rows() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;
    use crate::orchestration::workflow::definition::CsvExporterConfig;
    use tempfile::tempdir;

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let temp_dir = tempdir().unwrap();
    let output_path = temp_dir.path().join("rows.csv");
    let config = CsvExporterConfig {
        output_path: output_path.to_string_lossy().into_owned(),
        delimiter: Some(','),
        include_header: true,
        encoding: None,
    };
    let rows = vec![serde_json::json!("not an object row")];
    let context = ExecutionContext::new(serde_json::json!({
        "_rows": rows.clone()
    }));

    let result = executor
        .try_execute_csv_export_batch(&context, &config, &config.output_path, &rows)
        .unwrap();
    assert!(
        result.is_none(),
        "non-object rows should cleanly decline the batch CSV export path"
    );
}

#[test]
fn test_write_csv_export_rows_preserves_nulls_and_stringifies_scalars() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;
    use crate::orchestration::workflow::definition::CsvExporterConfig;
    use tempfile::tempdir;

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let temp_dir = tempdir().unwrap();
    let output_path = temp_dir.path().join("legacy_rows.csv");
    let config = CsvExporterConfig {
        output_path: output_path.to_string_lossy().into_owned(),
        delimiter: Some(','),
        include_header: true,
        encoding: None,
    };
    let columns = vec!["id".to_string(), "active".to_string(), "name".to_string()];
    let rows = vec![
        serde_json::json!({"id": 1, "active": true, "name": "Alice"}),
        serde_json::json!({"id": 2, "active": false, "name": null}),
    ];

    let rows_written = executor
        .write_csv_export_rows(&config, &config.output_path, &rows, &columns)
        .expect("legacy CSV row writer should succeed");

    assert_eq!(rows_written, 2);
    let content = std::fs::read_to_string(&config.output_path).unwrap();
    assert!(content.contains("id,active,name"));
    assert!(content.contains("1,true,Alice"));
    assert!(content.contains("2,false,"));
}

#[tokio::test]
async fn test_execute_csv_exporter_falls_back_to_legacy_row_writer_contract() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;
    use crate::orchestration::workflow::definition::CsvExporterConfig;
    use tempfile::tempdir;

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let temp_dir = tempdir().unwrap();
    let requested_path = temp_dir.path().join("legacy_rows.csv");
    let config = CsvExporterConfig {
        output_path: requested_path.to_string_lossy().into_owned(),
        delimiter: Some(','),
        include_header: false,
        encoding: None,
    };
    let context = ExecutionContext::new(serde_json::json!({
        "_rows": [123, 456]
    }))
    .with_workflow_id("wf_csv_legacy".to_string());

    let (success, output, confidence) = executor
        .execute_csv_exporter(&config, &context)
        .await
        .expect("legacy CSV exporter path should execute");

    assert!(success);
    assert_eq!(confidence, 1.0);
    assert_eq!(output["_requested_path"], config.output_path);
    assert_eq!(output["_unique_id"], "wf_csv_legacy");
    assert_eq!(output["_rows_written"], 2);
    assert_eq!(output["_columns"], serde_json::json!([]));
    assert_eq!(output["_modifications"].as_array().unwrap().len(), 1);

    let output_path = output["_output_path"].as_str().unwrap();
    assert!(output_path.ends_with("legacy_rows_wf_csv_legacy.csv"));
    assert!(std::path::Path::new(output_path).exists());
}

#[tokio::test]
async fn test_csv_exporter_uses_batch_path_and_preserves_output_contract() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;
    use crate::orchestration::workflow::definition::CsvExporterConfig;
    use tempfile::tempdir;

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let temp_dir = tempdir().unwrap();
    let requested_path = temp_dir.path().join("rows.csv");
    let config = CsvExporterConfig {
        output_path: requested_path.to_string_lossy().into_owned(),
        delimiter: Some(','),
        include_header: true,
        encoding: None,
    };
    let context = ExecutionContext::new(serde_json::json!({
        "_rows": [
            {"id": 1, "name": "Alice", "active": true},
            {"id": 2, "name": "Bob", "active": false}
        ]
    }))
    .with_workflow_id("wf_csv".to_string());

    let (success, output, confidence) = executor
        .execute_csv_exporter(&config, &context)
        .await
        .unwrap();

    assert!(success);
    assert_eq!(confidence, 1.0);
    assert_eq!(output["_requested_path"], config.output_path);
    assert_eq!(output["_unique_id"], "wf_csv");
    assert_eq!(output["_rows_written"], 2);
    assert_eq!(output["_columns"].as_array().unwrap().len(), 3);
    assert_eq!(output["_modifications"].as_array().unwrap().len(), 1);

    let output_path = output["_output_path"].as_str().unwrap();
    assert!(output_path.ends_with("rows_wf_csv.csv"));

    let content = std::fs::read_to_string(output_path).unwrap();
    assert!(content.contains("active,id,name"));
    assert!(content.contains("true,1,Alice"));
    assert!(content.contains("false,2,Bob"));
}
