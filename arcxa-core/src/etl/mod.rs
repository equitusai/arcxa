//! ETL Core Module - Timely/Differential Dataflow-based Processing
//!
//! This module provides the core ETL abstractions for Graphica's data governance
//! and quality platform, using Timely Dataflow and Differential Dataflow for
//! streaming transformations with comprehensive lineage tracking.

pub mod dataflow;
pub mod lineage;
pub mod profiling;
pub mod quality;
pub mod readers;
pub mod destinations;
pub mod pipeline;

use anyhow::{Result, Error};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::collections::HashMap;
use chrono::{DateTime, Utc};

// Re-exports
pub use lineage::{LineageEvent, DataRef, TransformRef, ModelRef, LineageSink};
pub use pipeline::{Pipeline, PipelineBuilder, PipelineStage};
pub use quality::{QualityRule, QualityViolation, QualityScorecard};

/// Record represents a single data record flowing through the pipeline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
    /// Unique identifier for this record
    pub id: String,

    /// Dataset this record belongs to
    pub dataset: String,

    /// Actual data as key-value pairs
    pub data: HashMap<String, serde_json::Value>,

    /// File library ID if sourced from file
    pub file_library_id: Option<String>,

    /// CDC position for replay
    pub cdc_position: Option<String>,

    /// Source URI
    pub source_uri: String,

    /// Source version
    pub source_version: String,

    /// Timestamp when record was ingested
    pub ingested_at: DateTime<Utc>,

    /// Quality score (0.0 to 1.0)
    pub quality_score: f64,

    /// Detected quality violations
    pub violations: Vec<QualityViolation>,

    /// Transformation history
    pub transform_refs: Vec<TransformRef>,

    /// Model predictions applied
    pub model_refs: Vec<ModelRef>,
}

impl Record {
    /// Create a new record with basic fields
    pub fn new(id: String, dataset: String, data: HashMap<String, serde_json::Value>) -> Self {
        Self {
            id,
            dataset,
            data,
            file_library_id: None,
            cdc_position: None,
            source_uri: String::new(),
            source_version: "1.0.0".to_string(),
            ingested_at: Utc::now(),
            quality_score: 1.0,
            violations: Vec::new(),
            transform_refs: Vec::new(),
            model_refs: Vec::new(),
        }
    }

    /// Add a transformation reference
    pub fn add_transform(&mut self, transform: TransformRef) {
        self.transform_refs.push(transform);
    }

    /// Add a model reference
    pub fn add_model(&mut self, model: ModelRef) {
        self.model_refs.push(model);
    }

    /// Update quality score based on violations
    pub fn update_quality_score(&mut self) {
        if self.violations.is_empty() {
            self.quality_score = 1.0;
        } else {
            let total_severity: f64 = self.violations.iter()
                .map(|v| match v.severity.as_str() {
                    "critical" => 1.0,
                    "high" => 0.7,
                    "medium" => 0.4,
                    "low" => 0.1,
                    _ => 0.0,
                })
                .sum();

            self.quality_score = (1.0 - (total_severity / self.violations.len() as f64)).max(0.0);
        }
    }
}

/// Stream of records
pub type RecordStream = std::pin::Pin<Box<dyn futures::Stream<Item = Result<Record>> + Send>>;

/// Schema information for structured data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schema {
    pub fields: Vec<Field>,
    pub version: String,
    pub source: String,
}

/// Field in a schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Field {
    pub name: String,
    pub data_type: DataType,
    pub nullable: bool,
    pub description: Option<String>,
    pub ontology_uri: Option<String>,
}

/// Data types supported by the platform
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DataType {
    String,
    Integer,
    Float,
    Boolean,
    Date,
    Timestamp,
    Json,
    Binary,
}

/// Metadata about a data source
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceMetadata {
    pub source_type: String,
    pub location: String,
    pub size_bytes: Option<u64>,
    pub record_count: Option<u64>,
    pub created_at: Option<DateTime<Utc>>,
    pub modified_at: Option<DateTime<Utc>>,
    pub file_library_id: Option<String>,
    pub tags: HashMap<String, String>,
}

/// Core trait for data sources
#[async_trait]
pub trait FormatReader: Send + Sync {
    /// Read data as a stream of records
    async fn read_stream(&self) -> Result<RecordStream>;

    /// Get schema if available
    fn schema(&self) -> Option<Schema>;

    /// Get source metadata
    fn source_metadata(&self) -> SourceMetadata;

    /// Validate source is accessible
    async fn validate(&self) -> Result<()>;
}

/// Core trait for data destinations
#[async_trait]
pub trait DataDestination: Send + Sync {
    /// Write a batch of records
    async fn write_batch(&self, records: Vec<Record>) -> Result<WriteResult>;

    /// Commit pending writes
    async fn commit(&self) -> Result<()>;

    /// Rollback pending writes
    async fn rollback(&self) -> Result<()>;

    /// Get destination metadata
    fn destination_metadata(&self) -> DestinationMetadata;

    /// Validate destination is accessible
    async fn validate(&self) -> Result<()>;
}

/// Result of a write operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteResult {
    pub records_written: u64,
    pub records_failed: u64,
    pub duration_ms: u64,
    pub errors: Vec<String>,
}

/// Metadata about a destination
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DestinationMetadata {
    pub destination_type: String,
    pub location: String,
    pub supports_transactions: bool,
    pub supports_upsert: bool,
    pub max_batch_size: Option<usize>,
}

/// Execution mode for pipelines
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExecutionMode {
    /// Batch processing mode
    Batch,
    /// Streaming with Timely Dataflow
    Streaming,
    /// Micro-batch with configurable interval
    MicroBatch { interval_ms: u64 },
}

/// Pipeline configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineConfig {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub execution_mode: ExecutionMode,
    pub max_parallelism: usize,
    pub checkpoint_interval_ms: Option<u64>,
    pub lineage_config: LineageConfig,
}

/// Lineage tracking configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageConfig {
    /// Enable lineage tracking
    pub enabled: bool,

    /// Capture field-level lineage
    pub field_level: bool,

    /// Include transformation parameters
    pub include_params: bool,

    /// Include model metadata
    pub include_models: bool,

    /// Retention period in days
    pub retention_days: u32,
}

impl Default for LineageConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            field_level: true,
            include_params: true,
            include_models: true,
            retention_days: 365,
        }
    }
}

/// Registry for format readers
pub struct FormatReaderRegistry {
    readers: HashMap<String, Arc<dyn FormatReader>>,
}

impl FormatReaderRegistry {
    pub fn new() -> Self {
        Self {
            readers: HashMap::new(),
        }
    }

    pub fn register(&mut self, format: impl Into<String>, reader: Arc<dyn FormatReader>) {
        self.readers.insert(format.into(), reader);
    }

    pub fn get(&self, format: &str) -> Option<Arc<dyn FormatReader>> {
        self.readers.get(format).cloned()
    }
}

/// Registry for data destinations
pub struct DataDestinationRegistry {
    destinations: HashMap<String, Arc<dyn DataDestination>>,
}

impl DataDestinationRegistry {
    pub fn new() -> Self {
        Self {
            destinations: HashMap::new(),
        }
    }

    pub fn register(&mut self, name: impl Into<String>, destination: Arc<dyn DataDestination>) {
        self.destinations.insert(name.into(), destination);
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn DataDestination>> {
        self.destinations.get(name).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_record_quality_score() {
        let mut record = Record::new(
            "rec_1".to_string(),
            "customers".to_string(),
            HashMap::new(),
        );

        assert_eq!(record.quality_score, 1.0);

        // Add violations
        record.violations.push(QualityViolation {
            rule_id: "completeness".to_string(),
            field: Some("email".to_string()),
            severity: "medium".to_string(),
            message: "Missing email".to_string(),
        });

        record.violations.push(QualityViolation {
            rule_id: "validity".to_string(),
            field: Some("phone".to_string()),
            severity: "low".to_string(),
            message: "Invalid phone format".to_string(),
        });

        record.update_quality_score();
        assert!(record.quality_score < 1.0);
        assert!(record.quality_score > 0.5);
    }

    #[test]
    fn test_record_lineage() {
        let mut record = Record::new(
            "rec_1".to_string(),
            "customers".to_string(),
            HashMap::new(),
        );

        record.add_transform(TransformRef {
            transform_id: "dedupe".to_string(),
            transform_type: "deduplication".to_string(),
            params_hash: "abc123".to_string(),
            execution_time_ms: 45,
        });

        record.add_model(ModelRef {
            model_id: "gender_predictor".to_string(),
            version: "2.1.0".to_string(),
            params_hash: "xyz789".to_string(),
            training_data: vec![],
            confidence_threshold: 0.8,
        });

        assert_eq!(record.transform_refs.len(), 1);
        assert_eq!(record.model_refs.len(), 1);
    }
}