//! Example Pipeline Implementation
//!
//! This demonstrates how the new ETL architecture enables clean composition
//! of readers, transformers, and destinations into reusable pipelines.

use crate::etl::traits::{
    DataDestination, DataRecord, FormatReader, LoadConfig, LoadStats, PipelineConfig,
    PipelineExecutor, PipelineStats, PipelineStatus, RecordSchema, Transformer,
    TransformResult, EtlError,
};
use async_trait::async_trait;
use futures::stream::{Stream, StreamExt};
use std::pin::Pin;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tracing::{info, warn, error, instrument};

/// Simple pipeline executor implementation
pub struct SimplePipelineExecutor {
    status: PipelineStatus,
    cancel_tx: Option<mpsc::Sender<()>>,
}

impl SimplePipelineExecutor {
    pub fn new() -> Self {
        Self {
            status: PipelineStatus::NotStarted,
            cancel_tx: None,
        }
    }

    /// Apply transformers to a stream of records
    async fn apply_transformers(
        &self,
        mut stream: Pin<Box<dyn Stream<Item = Result<DataRecord, EtlError>> + Send>>,
        transformers: Vec<Box<dyn Transformer>>,
    ) -> Pin<Box<dyn Stream<Item = Result<DataRecord, EtlError>> + Send>> {
        // For simplicity, apply transformers sequentially
        // In production, this could be parallelized
        let transformed = stream.then(move |result| {
            let transformers = transformers.clone();
            async move {
                match result {
                    Ok(record) => {
                        let mut current = record;

                        for transformer in &transformers {
                            match transformer.transform(current.clone()).await? {
                                TransformResult::Record(transformed) => {
                                    current = transformed;
                                }
                                TransformResult::Multiple(records) => {
                                    // For now, just take the first record
                                    // In production, this would need better handling
                                    if let Some(first) = records.into_iter().next() {
                                        current = first;
                                    }
                                }
                                TransformResult::Filtered { reason } => {
                                    return Err(EtlError::ValidationError {
                                        message: format!("Record filtered: {}", reason),
                                        field: None,
                                        record_index: None,
                                    });
                                }
                                TransformResult::Error(e) => {
                                    return Err(e);
                                }
                            }
                        }

                        Ok(current)
                    }
                    Err(e) => Err(e),
                }
            }
        });

        Box::pin(transformed)
    }
}

#[async_trait]
impl PipelineExecutor for SimplePipelineExecutor {
    #[instrument(skip(self, source, transformers, destination))]
    async fn execute(
        &mut self,
        source: Box<dyn FormatReader>,
        transformers: Vec<Box<dyn Transformer>>,
        mut destination: Box<dyn DataDestination>,
        config: &PipelineConfig,
    ) -> Result<PipelineStats, EtlError> {
        info!("Starting pipeline: {}", config.name);
        let start_time = Instant::now();
        self.status = PipelineStatus::Running;

        // Create cancellation channel
        let (cancel_tx, mut cancel_rx) = mpsc::channel(1);
        self.cancel_tx = Some(cancel_tx);

        // Prepare destination with schema
        let schema = source.infer_schema().await?;
        destination.prepare(&schema, &config.load_config).await?;

        // Get source stream
        let source_stream = source.read_stream().await?;
        let total_records = source.get_stats().await?.total_records;

        info!(
            "Processing {} records from source",
            total_records.map_or("unknown".to_string(), |n| n.to_string())
        );

        // Apply transformers
        let transformed_stream = if transformers.is_empty() {
            source_stream
        } else {
            self.apply_transformers(source_stream, transformers).await
        };

        // Load to destination with cancellation support
        let load_future = destination.load_stream(transformed_stream, &config.load_config);

        let load_stats = tokio::select! {
            result = load_future => {
                match result {
                    Ok(stats) => stats,
                    Err(e) => {
                        error!("Load failed: {}", e);
                        self.status = PipelineStatus::Failed;
                        destination.rollback().await?;
                        return Err(e);
                    }
                }
            }
            _ = cancel_rx.recv() => {
                info!("Pipeline cancelled");
                self.status = PipelineStatus::Cancelled;
                destination.rollback().await?;
                return Err(EtlError::Cancelled {
                    reason: "User requested cancellation".to_string(),
                });
            }
        };

        // Finalize destination
        destination.finalize().await?;

        let duration_ms = start_time.elapsed().as_millis() as u64;
        self.status = PipelineStatus::Completed;

        info!(
            "Pipeline completed: {} records loaded in {}ms",
            load_stats.records_loaded, duration_ms
        );

        Ok(PipelineStats {
            status: "completed".to_string(),
            start_time: chrono::Utc::now(),
            end_time: Some(chrono::Utc::now()),
            duration_ms,
            records_read: load_stats.records_read,
            records_transformed: load_stats.records_read, // Simplified
            records_loaded: load_stats.records_loaded,
            records_failed: load_stats.records_failed,
            bytes_processed: load_stats.bytes_processed,
            errors: Vec::new(),
            checkpoints_created: 0,
            stage_stats: Vec::new(),
        })
    }

    async fn cancel(&mut self) -> Result<(), EtlError> {
        if let Some(tx) = &self.cancel_tx {
            tx.send(()).await.map_err(|_| EtlError::Internal {
                message: "Failed to send cancellation signal".to_string(),
                source: None,
            })?;
        }
        Ok(())
    }

    fn status(&self) -> PipelineStatus {
        self.status
    }
}

/// Pipeline builder for fluent API
pub struct PipelineBuilder {
    source: Option<Box<dyn FormatReader>>,
    transformers: Vec<Box<dyn Transformer>>,
    destination: Option<Box<dyn DataDestination>>,
    config: PipelineConfig,
}

impl PipelineBuilder {
    pub fn new(name: impl Into<String>) -> Self {
        let mut config = PipelineConfig::default();
        config.name = name.into();

        Self {
            source: None,
            transformers: Vec::new(),
            destination: None,
            config,
        }
    }

    pub fn source(mut self, source: impl FormatReader + 'static) -> Self {
        self.source = Some(Box::new(source));
        self
    }

    pub fn transform(mut self, transformer: impl Transformer + 'static) -> Self {
        self.transformers.push(Box::new(transformer));
        self
    }

    pub fn destination(mut self, destination: impl DataDestination + 'static) -> Self {
        self.destination = Some(Box::new(destination));
        self
    }

    pub fn batch_size(mut self, size: usize) -> Self {
        self.config.load_config.batch_size = size;
        self
    }

    pub fn parallelism(mut self, parallelism: usize) -> Self {
        self.config.parallelism = parallelism;
        self.config.load_config.parallelism = parallelism;
        self
    }

    pub fn with_config(mut self, f: impl FnOnce(&mut PipelineConfig)) -> Self {
        f(&mut self.config);
        self
    }

    pub async fn execute(self) -> Result<PipelineStats, EtlError> {
        let source = self.source.ok_or_else(|| EtlError::ConfigurationError {
            message: "Source not configured".to_string(),
        })?;

        let destination = self.destination.ok_or_else(|| EtlError::ConfigurationError {
            message: "Destination not configured".to_string(),
        })?;

        let mut executor = SimplePipelineExecutor::new();
        executor
            .execute(source, self.transformers, destination, &self.config)
            .await
    }
}

// ============================================================================
// Usage Examples
// ============================================================================

#[cfg(test)]
mod examples {
    use super::*;
    use crate::etl::formats::csv_example::CsvReader;
    use crate::etl::transformers::field_mapper::FieldMapper;
    use crate::etl::destinations::postgresql::PostgreSQLDestination;

    /// Example: CSV to PostgreSQL with field mapping
    async fn example_csv_to_postgres() -> Result<(), EtlError> {
        let stats = PipelineBuilder::new("customer_import")
            .source(CsvReader::new("/data/customers.csv")
                .with_delimiter(b',')
                .with_header(true))
            .transform(FieldMapper::new()
                .rename("cust_id", "customer_id")
                .rename("fname", "first_name")
                .rename("lname", "last_name"))
            .destination(PostgreSQLDestination::new(
                "postgresql://localhost/mydb",
                "customers",
            )?)
            .batch_size(5000)
            .parallelism(4)
            .with_config(|cfg| {
                cfg.load_config.mode = LoadMode::Upsert;
                cfg.load_config.key_fields = vec!["customer_id".to_string()];
            })
            .execute()
            .await?;

        println!("Imported {} customers in {}ms",
                 stats.records_loaded,
                 stats.duration_ms);

        Ok(())
    }

    /// Example: Multi-source pipeline with validation
    async fn example_multi_source_pipeline() -> Result<(), EtlError> {
        // First pipeline: load main customer data
        let main_stats = PipelineBuilder::new("main_customer_load")
            .source(CsvReader::new("/data/customers_main.csv"))
            .transform(DataValidator::new()
                .require_fields(vec!["customer_id", "email"])
                .validate_email("email"))
            .destination(PostgreSQLDestination::new(
                "postgresql://localhost/mydb",
                "customers",
            )?)
            .execute()
            .await?;

        // Second pipeline: load supplementary data
        let supp_stats = PipelineBuilder::new("supplementary_data_load")
            .source(ParquetReader::new("/data/customer_attributes.parquet"))
            .transform(FieldMapper::new()
                .rename("id", "customer_id"))
            .destination(PostgreSQLDestination::new(
                "postgresql://localhost/mydb",
                "customer_attributes",
            )?)
            .execute()
            .await?;

        println!("Loaded {} main + {} supplementary records",
                 main_stats.records_loaded,
                 supp_stats.records_loaded);

        Ok(())
    }

    /// Example: Database to database ETL with transformations
    async fn example_db_to_db_etl() -> Result<(), EtlError> {
        let stats = PipelineBuilder::new("warehouse_sync")
            .source(DatabaseExtractor::new(
                "postgresql://source_db",
                "SELECT * FROM orders WHERE created_at >= NOW() - INTERVAL '1 day'",
            )?)
            .transform(DataCleaner::default())
            .transform(FieldEncryptor::new(vec!["credit_card", "ssn"]))
            .transform(DataEnricher::new()
                .lookup("customer_id", "customers", vec!["name", "email"]))
            .destination(SnowflakeDestination::new(
                "snowflake://warehouse",
                "fact_orders",
            )?)
            .batch_size(10000)
            .with_config(|cfg| {
                cfg.checkpoint_interval = Some(Duration::from_secs(60));
                cfg.load_config.error_tolerance.max_errors = 100;
                cfg.load_config.error_tolerance.skip_on_error = true;
            })
            .execute()
            .await?;

        println!("Synchronized {} orders to warehouse", stats.records_loaded);

        if stats.records_failed > 0 {
            warn!("{} records failed during sync", stats.records_failed);
        }

        Ok(())
    }

    /// Example: Stream processing pipeline
    async fn example_streaming_pipeline() -> Result<(), EtlError> {
        let stats = PipelineBuilder::new("realtime_events")
            .source(KafkaSource::new(
                "kafka://localhost:9092",
                "events_topic",
            )?)
            .transform(JsonParser::new())
            .transform(EventValidator::new())
            .transform(EventEnricher::new())
            .destination(MultiDestination::new()
                .add(ElasticsearchDestination::new(
                    "http://localhost:9200",
                    "events_index",
                )?)
                .add(S3Destination::new(
                    "s3://my-bucket/events/",
                    ParquetFormat::default(),
                )?))
            .parallelism(8)
            .with_config(|cfg| {
                cfg.load_config.mode = LoadMode::Append;
                cfg.checkpoint_interval = Some(Duration::from_secs(30));
            })
            .execute()
            .await?;

        println!("Processed {} events", stats.records_loaded);

        Ok(())
    }
}

// Placeholder types for examples (would be actual implementations)
struct DataValidator;
struct ParquetReader;
struct DatabaseExtractor;
struct DataCleaner;
struct FieldEncryptor;
struct DataEnricher;
struct SnowflakeDestination;
struct KafkaSource;
struct JsonParser;
struct EventValidator;
struct EventEnricher;
struct MultiDestination;
struct ElasticsearchDestination;
struct S3Destination;
struct ParquetFormat;