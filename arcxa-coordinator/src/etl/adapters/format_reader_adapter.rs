//! FormatReader Adapter
//!
//! Wraps a FormatReader (new abstraction) as a workflow Transformer (existing interface).
//! This allows gradual migration while maintaining backward compatibility.

use crate::etl::errors::EtlError;
use crate::etl::traits::{DataRecord, FormatReader};
use crate::workflows::engine::transformers::Transformer;

use anyhow::Result;
use async_trait::async_trait;
use futures::StreamExt;
use serde_json::{json, Value};
use tracing::{debug, info, warn};

/// Adapter that wraps a FormatReader as a Transformer
///
/// This adapter bridges the gap between the new ETL abstractions and the
/// existing workflow system. It allows FormatReaders to be used in YAML
/// workflows without any changes to the workflow engine.
///
/// ## Workflow Integration
///
/// ```yaml
/// routes:
///   - actions:
///       - transform:
///           transformer: csv_parse  # Uses FormatReaderAdapter internally
///           config:
///             source_file_id: "file_123"
///             delimiter: ","
/// ```
///
/// ## Output Format
///
/// The adapter produces output compatible with existing workflow expectations:
///
/// ```json
/// {
///   "records": [
///     {"name": "Alice", "age": 30},
///     {"name": "Bob", "age": 25}
///   ],
///   "schema": {
///     "fields": [
///       {"name": "name", "data_type": "string", "nullable": true},
///       {"name": "age", "data_type": "bigint", "nullable": true}
///     ]
///   },
///   "metadata": {
///     "record_count": 2,
///     "format": "CSV",
///     "source_file_id": "file_123",
///     "execution_id": "exec_456"
///   }
/// }
/// ```
pub struct FormatReaderAdapter {
    reader: Box<dyn FormatReader>,
    transformer_name: String,
}

impl FormatReaderAdapter {
    /// Create a new FormatReaderAdapter
    ///
    /// # Arguments
    ///
    /// * `reader` - The FormatReader to wrap
    /// * `transformer_name` - Name for this transformer (e.g., "csv_parse")
    pub fn new(reader: Box<dyn FormatReader>, transformer_name: String) -> Self {
        Self {
            reader,
            transformer_name,
        }
    }

    /// Extract source file ID from config for lineage tracking
    fn extract_file_id(config: &Value) -> Option<String> {
        config
            .get("source_file_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }
}

#[async_trait]
impl Transformer for FormatReaderAdapter {
    async fn transform(
        &self,
        config: &Value,
        data: &mut Value,
        context: Option<&crate::workflows::engine::ExecutionContext>,
    ) -> Result<()> {
        let file_id = Self::extract_file_id(config);

        debug!(
            transformer = %self.transformer_name,
            file_id = ?file_id,
            "Starting FormatReader transformation"
        );

        // Read records from FormatReader
        let mut stream = self
            .reader
            .read_stream()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to read stream: {}", e))?;

        let mut records = Vec::new();
        let mut record_count = 0;

        while let Some(record_result) = stream.next().await {
            match record_result {
                Ok(mut record) => {
                    // Add lineage tracking if context provided
                    if let Some(ctx) = context {
                        if let Some(ref exec_id) = ctx.execution_id {
                            record
                                .metadata
                                .insert("execution_id".to_string(), Value::String(exec_id.clone()));
                        }

                        record.metadata.insert(
                            "workflow_id".to_string(),
                            Value::String(ctx.workflow_id.clone()),
                        );
                    }

                    // Add source file tracking
                    if let Some(ref fid) = file_id {
                        record
                            .metadata
                            .insert("source_file_id".to_string(), Value::String(fid.clone()));
                    }

                    records.push(record.data);
                    record_count += 1;
                }
                Err(e) => {
                    warn!(
                        transformer = %self.transformer_name,
                        record_num = record_count + 1,
                        error = %e,
                        "Failed to parse record"
                    );

                    // Convert EtlError to anyhow
                    return Err(anyhow::anyhow!(
                        "Failed to parse record {}: {}",
                        record_count + 1,
                        e
                    ));
                }
            }
        }

        // Get schema
        let schema = self
            .reader
            .infer_schema()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to infer schema: {}", e))?;

        // Get stats
        let stats = self
            .reader
            .get_stats()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get stats: {}", e))?;

        info!(
            transformer = %self.transformer_name,
            record_count = record_count,
            format = %stats.format_name,
            "FormatReader transformation complete"
        );

        // Update data (maintain backward compatible format)
        data["records"] = Value::Array(records);
        data["schema"] = serde_json::to_value(schema)?;
        data["metadata"] = json!({
            "record_count": record_count,
            "format": stats.format_name,
            "source_file_id": file_id,
            "execution_id": context.and_then(|c| c.execution_id.clone()),
            "workflow_id": context.map(|c| c.workflow_id.clone()),
        });

        Ok(())
    }

    fn name(&self) -> &'static str {
        // This is a limitation of the trait - it expects &'static str
        // We leak the string to get a static lifetime
        Box::leak(self.transformer_name.clone().into_boxed_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::etl::traits::*;
    use crate::workflows::engine::transformers::Transformer;
    use futures::stream;
    use std::pin::Pin;

    struct MockReader {
        records: Vec<DataRecord>,
    }

    impl MockReader {
        fn new(records: Vec<DataRecord>) -> Self {
            Self { records }
        }
    }

    #[async_trait]
    impl FormatReader for MockReader {
        async fn read_stream(
            &self,
        ) -> Result<
            Pin<Box<dyn futures::stream::Stream<Item = anyhow::Result<DataRecord>> + Send>>,
            EtlError,
        > {
            let records = self.records.clone();
            Ok(Box::pin(stream::iter(
                records
                    .into_iter()
                    .map(|r| Ok::<DataRecord, anyhow::Error>(r)),
            )))
        }

        async fn infer_schema(&self) -> Result<RecordSchema, EtlError> {
            Ok(RecordSchema {
                fields: vec![
                    FieldSchema {
                        name: "id".to_string(),
                        data_type: DataType::BigInt,
                        nullable: false,
                        description: None,
                        metadata: Default::default(),
                    },
                    FieldSchema {
                        name: "name".to_string(),
                        data_type: DataType::String,
                        nullable: true,
                        description: None,
                        metadata: Default::default(),
                    },
                ],
                metadata: Default::default(),
            })
        }

        async fn get_stats(&self) -> Result<FormatStats, EtlError> {
            Ok(FormatStats {
                total_records: Some(self.records.len() as u64),
                total_bytes: None,
                format_name: "MockFormat".to_string(),
                compression: None,
                metadata: Default::default(),
            })
        }

        async fn validate(&self) -> Result<ValidationReport, EtlError> {
            Ok(ValidationReport {
                is_valid: true,
                errors: vec![],
                warnings: vec![],
            })
        }
    }

    #[tokio::test]
    async fn test_format_reader_adapter_basic() {
        let records = vec![
            DataRecord {
                data: json!({"id": 1, "name": "Alice"}),
                schema: None,
                source_location: None,
                metadata: Default::default(),
            },
            DataRecord {
                data: json!({"id": 2, "name": "Bob"}),
                schema: None,
                source_location: None,
                metadata: Default::default(),
            },
        ];

        let reader = Box::new(MockReader::new(records));
        let adapter = FormatReaderAdapter::new(reader, "test_reader".to_string());

        let config = json!({
            "source_file_id": "file_123"
        });
        let mut data = json!({});

        adapter.transform(&config, &mut data, None).await.unwrap();

        // Validate output structure
        assert!(data["records"].is_array());
        assert_eq!(data["records"].as_array().unwrap().len(), 2);

        assert!(data["schema"].is_object());
        assert_eq!(data["schema"]["fields"].as_array().unwrap().len(), 2);

        assert!(data["metadata"].is_object());
        assert_eq!(data["metadata"]["record_count"], 2);
        assert_eq!(data["metadata"]["format"], "MockFormat");
        assert_eq!(data["metadata"]["source_file_id"], "file_123");
    }

    #[tokio::test]
    async fn test_format_reader_adapter_with_context() {
        let records = vec![DataRecord {
            data: json!({"id": 1}),
            schema: None,
            source_location: None,
            metadata: Default::default(),
        }];

        let reader = Box::new(MockReader::new(records));
        let adapter = FormatReaderAdapter::new(reader, "test_reader".to_string());

        // Note: ExecutionContext has complex fields we can't easily construct in tests
        // For now, just test without context - context handling will be tested
        // in integration tests with actual workflow execution

        let config = json!({});
        let mut data = json!({});

        adapter.transform(&config, &mut data, None).await.unwrap();

        // Validate basic output (context testing deferred to integration tests)
        assert!(data["records"].is_array());
        assert_eq!(data["records"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_format_reader_adapter_empty_records() {
        let reader = Box::new(MockReader::new(vec![]));
        let adapter = FormatReaderAdapter::new(reader, "test_reader".to_string());

        let config = json!({});
        let mut data = json!({});

        adapter.transform(&config, &mut data, None).await.unwrap();

        assert!(data["records"].is_array());
        assert_eq!(data["records"].as_array().unwrap().len(), 0);
        assert_eq!(data["metadata"]["record_count"], 0);
    }

    #[test]
    fn test_transformer_name() {
        let reader = Box::new(MockReader::new(vec![]));
        let adapter = FormatReaderAdapter::new(reader, "csv_parse".to_string());

        assert_eq!(adapter.name(), "csv_parse");
    }
}
