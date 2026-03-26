//! Row-Level Lineage Tracking
//!
//! Provides fine-grained lineage tracking at the individual row level for all ETL pipelines.
//! Supports CSV files, database extracts, Kafka/CDC streams, and S3 objects.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fmt;
use utoipa::ToSchema;

/// Unique identifier for a row across all data sources
///
/// # Examples
/// - CSV: `csv:customers_20241027.csv:12345` (row number)
/// - DB: `db2:prod.customers:pk=C123456` (primary key)
/// - Kafka: `kafka:orders:p5:o987654` (partition:offset)
/// - S3: `s3:bucket/path.parquet:r45678` (row index)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
pub struct RowId {
    /// Type of data source
    pub source_type: SourceType,
    /// Unique identifier of the source (file path, table name, topic)
    pub source_id: String,
    /// Position within the source
    pub position: RowPosition,
}

impl RowId {
    /// Create a new RowId for a CSV file
    pub fn csv(file_path: impl Into<String>, row_number: u64) -> Self {
        Self {
            source_type: SourceType::Csv,
            source_id: file_path.into(),
            position: RowPosition::RowNumber(row_number),
        }
    }

    /// Create a new RowId for a database record
    pub fn database(
        db_type: DatabaseType,
        table: impl Into<String>,
        primary_keys: BTreeMap<String, String>,
    ) -> Self {
        Self {
            source_type: SourceType::Database(db_type),
            source_id: table.into(),
            position: RowPosition::PrimaryKey(primary_keys),
        }
    }

    /// Create a new RowId for a Kafka message
    pub fn kafka(topic: impl Into<String>, partition: i32, offset: i64) -> Self {
        Self {
            source_type: SourceType::Kafka,
            source_id: topic.into(),
            position: RowPosition::KafkaOffset { partition, offset },
        }
    }

    /// Create a new RowId for an S3 object
    pub fn s3(bucket: impl Into<String>, key: impl Into<String>, index: u64) -> Self {
        Self {
            source_type: SourceType::S3,
            source_id: format!("{}/{}", bucket.into(), key.into()),
            position: RowPosition::ParquetIndex(index),
        }
    }

    /// Generate a unique string representation suitable for use as a key
    pub fn to_key(&self) -> String {
        match &self.position {
            RowPosition::RowNumber(n) => {
                format!("{}:{}:{}", self.source_type, self.source_id, n)
            }
            RowPosition::PrimaryKey(keys) => {
                let pk_str = keys
                    .iter()
                    .map(|(k, v)| format!("{}={}", k, v))
                    .collect::<Vec<_>>()
                    .join(",");
                format!("{}:{}:{}", self.source_type, self.source_id, pk_str)
            }
            RowPosition::KafkaOffset { partition, offset } => {
                format!(
                    "{}:{}:p{}:o{}",
                    self.source_type, self.source_id, partition, offset
                )
            }
            RowPosition::ParquetIndex(idx) => {
                format!("{}:{}:r{}", self.source_type, self.source_id, idx)
            }
        }
    }
}

impl fmt::Display for RowId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_key())
    }
}

/// Type of data source
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
pub enum SourceType {
    /// CSV file
    Csv,
    /// Database (with specific type)
    Database(DatabaseType),
    /// Kafka topic
    Kafka,
    /// S3 object
    S3,
}

impl fmt::Display for SourceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Csv => write!(f, "csv"),
            Self::Database(db) => write!(f, "{}", db),
            Self::Kafka => write!(f, "kafka"),
            Self::S3 => write!(f, "s3"),
        }
    }
}

/// Database type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
pub enum DatabaseType {
    Postgres,
    DB2,
    Oracle,
    SAPHANA,
    MySQL,
    Snowflake,
    Databricks,
}

impl fmt::Display for DatabaseType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Postgres => write!(f, "postgres"),
            Self::DB2 => write!(f, "db2"),
            Self::Oracle => write!(f, "oracle"),
            Self::SAPHANA => write!(f, "saphana"),
            Self::MySQL => write!(f, "mysql"),
            Self::Snowflake => write!(f, "snowflake"),
            Self::Databricks => write!(f, "databricks"),
        }
    }
}

/// Position of a row within its source
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
pub enum RowPosition {
    /// Row number (1-based) for CSV files
    RowNumber(u64),
    /// Primary key value(s) for database tables (BTreeMap for Hash support)
    PrimaryKey(BTreeMap<String, String>),
    /// Kafka partition and offset
    KafkaOffset { partition: i32, offset: i64 },
    /// Row index in Parquet file
    ParquetIndex(u64),
}

/// Lightweight row-level lineage event
///
/// Captures the processing of a single row through the ETL pipeline
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RowLineageEvent {
    /// Unique identifier for this row
    pub row_id: RowId,
    /// Batch context for grouping related rows
    pub batch_id: String,
    /// Job or workflow identifier
    pub job_id: String,
    /// Step identifier within the workflow
    pub step_id: Option<String>,
    /// Processing timestamp
    pub timestamp: DateTime<Utc>,
    /// Processing outcome (success, filtered, failed, etc.)
    pub outcome: ProcessingOutcome,
    /// Transformations applied to this row
    pub transformations: Vec<RowTransformation>,
    /// Output row ID if different from input
    pub output_row_id: Option<RowId>,
    /// Tenant identifier for multi-tenancy
    pub tenant_id: String,
    /// Optional correlation ID for distributed tracing
    pub correlation_id: Option<String>,
}

impl RowLineageEvent {
    /// Create a new successful processing event (backward compatible)
    pub fn success(
        row_id: RowId,
        batch_id: String,
        job_id: String,
        output_location: String,
        tenant_id: String,
    ) -> Self {
        Self::success_with_step(row_id, batch_id, job_id, None, output_location, tenant_id)
    }

    /// Create a new successful processing event with step tracking
    pub fn success_with_step(
        row_id: RowId,
        batch_id: String,
        job_id: String,
        step_id: Option<String>,
        output_location: String,
        tenant_id: String,
    ) -> Self {
        Self {
            row_id,
            batch_id,
            job_id,
            step_id,
            timestamp: Utc::now(),
            outcome: ProcessingOutcome::Processed { output_location },
            transformations: Vec::new(),
            output_row_id: None,
            tenant_id,
            correlation_id: None,
        }
    }

    /// Create a filtered row event (backward compatible)
    pub fn filtered(
        row_id: RowId,
        batch_id: String,
        job_id: String,
        reason: String,
        rule_id: String,
        tenant_id: String,
    ) -> Self {
        Self::filtered_with_step(row_id, batch_id, job_id, None, reason, rule_id, tenant_id)
    }

    /// Create a filtered row event with step tracking
    pub fn filtered_with_step(
        row_id: RowId,
        batch_id: String,
        job_id: String,
        step_id: Option<String>,
        reason: String,
        rule_id: String,
        tenant_id: String,
    ) -> Self {
        Self {
            row_id,
            batch_id,
            job_id,
            step_id,
            timestamp: Utc::now(),
            outcome: ProcessingOutcome::Filtered { reason, rule_id },
            transformations: Vec::new(),
            output_row_id: None,
            tenant_id,
            correlation_id: None,
        }
    }

    /// Add a transformation to this event
    pub fn add_transformation(&mut self, transformation: RowTransformation) {
        self.transformations.push(transformation);
    }

    /// Check if row was successfully processed
    pub fn is_success(&self) -> bool {
        matches!(self.outcome, ProcessingOutcome::Processed { .. })
    }

    /// Check if row was filtered out
    pub fn is_filtered(&self) -> bool {
        matches!(self.outcome, ProcessingOutcome::Filtered { .. })
    }
}

/// Processing outcome for a row
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub enum ProcessingOutcome {
    /// Row was successfully processed
    Processed {
        /// Location where the row was written
        output_location: String,
    },
    /// Row was filtered out
    Filtered {
        /// Reason for filtering
        reason: String,
        /// Rule that caused the filtering
        rule_id: String,
    },
    /// Row failed data quality validation
    ValidationFailed {
        /// List of quality violations
        violations: Vec<QualityViolation>,
    },
    /// Row processing failed with error
    Failed {
        /// Error message
        error: String,
    },
}

/// Transformation applied to a row
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RowTransformation {
    /// Type of transformation (e.g., "proper_case", "normalize_email")
    pub transform_type: String,
    /// Fields affected by this transformation
    pub fields: Vec<String>,
    /// Field values before transformation (optional for privacy)
    pub before_values: Option<HashMap<String, serde_json::Value>>,
    /// Field values after transformation
    pub after_values: Option<HashMap<String, serde_json::Value>>,
    /// When the transformation was applied
    pub applied_at: DateTime<Utc>,
}

impl RowTransformation {
    /// Create a new transformation record
    pub fn new(transform_type: impl Into<String>, fields: Vec<String>) -> Self {
        Self {
            transform_type: transform_type.into(),
            fields,
            before_values: None,
            after_values: None,
            applied_at: Utc::now(),
        }
    }

    /// Add before/after values for auditing
    pub fn with_values(
        mut self,
        before: HashMap<String, serde_json::Value>,
        after: HashMap<String, serde_json::Value>,
    ) -> Self {
        self.before_values = Some(before);
        self.after_values = Some(after);
        self
    }
}

/// Data quality violation at row level
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct QualityViolation {
    /// Rule that was violated
    pub rule_id: String,
    /// Field that violated the rule
    pub field: String,
    /// Constraint that was violated
    pub constraint: String,
    /// Human-readable violation message
    pub message: String,
}

/// Enhanced LineageSink trait with row-level support
#[async_trait::async_trait]
pub trait RowLevelLineageSink: Send + Sync {
    /// Write a single row lineage event
    async fn write_row(&self, event: RowLineageEvent) -> anyhow::Result<()>;

    /// Batch write multiple events for performance
    async fn write_rows_batch(&self, events: Vec<RowLineageEvent>) -> anyhow::Result<()>;

    /// Query lineage for a specific row
    async fn get_row_lineage(&self, row_id: &RowId) -> anyhow::Result<Vec<RowLineageEvent>>;

    /// Query all rows in a batch
    async fn get_batch_lineage(&self, batch_id: &str) -> anyhow::Result<Vec<RowLineageEvent>>;

    /// Query filtered rows with reasons
    async fn get_filtered_rows(
        &self,
        job_id: &str,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> anyhow::Result<Vec<(RowId, String)>>;

    /// Get transformation history for a row
    async fn get_row_transformations(
        &self,
        row_id: &RowId,
    ) -> anyhow::Result<Vec<RowTransformation>>;

    /// Trace complete journey of a row from source to destination
    async fn trace_row_journey(&self, row_id: &RowId) -> anyhow::Result<RowJourney>;

    /// Get statistics for a job
    async fn get_job_stats(&self, job_id: &str) -> anyhow::Result<JobStatistics>;

    /// Flush buffered events to storage (optional, for implementations with buffering)
    ///
    /// This is useful for testing to ensure events are persisted before querying.
    /// Implementations without buffering can simply return Ok(()).
    async fn flush_buffer(&self) -> anyhow::Result<()> {
        // Default implementation does nothing (no-op for stores without buffering)
        Ok(())
    }
}

/// Complete journey of a row through the system
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RowJourney {
    /// Original source row
    pub source: RowId,
    /// Processing steps in order
    pub steps: Vec<JourneyStep>,
    /// Final destination (if successfully processed)
    pub destination: Option<RowId>,
    /// Total processing duration in milliseconds
    pub total_duration_ms: u64,
}

/// Single step in a row's journey
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct JourneyStep {
    /// Activity performed
    pub activity: String,
    /// When it happened
    pub timestamp: DateTime<Utc>,
    /// Duration in milliseconds
    pub duration_ms: u64,
    /// Outcome of this step
    pub outcome: ProcessingOutcome,
}

/// Statistics for a job execution
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct JobStatistics {
    /// Job identifier
    pub job_id: String,
    /// Total rows processed
    pub total_rows: u64,
    /// Successfully processed rows
    pub success_count: u64,
    /// Filtered rows
    pub filtered_count: u64,
    /// Failed rows
    pub failed_count: u64,
    /// Filter reasons with counts
    pub filter_reasons: HashMap<String, u64>,
    /// Average processing time per row (ms)
    pub avg_processing_time_ms: f64,
    /// Job start time
    pub start_time: DateTime<Utc>,
    /// Job end time (if completed)
    pub end_time: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_row_id_csv() {
        let row_id = RowId::csv("/data/customers.csv", 12345);
        assert_eq!(row_id.to_key(), "csv:/data/customers.csv:12345");
    }

    #[test]
    fn test_row_id_database() {
        let mut pk = BTreeMap::new();
        pk.insert("customer_id".to_string(), "C123".to_string());
        pk.insert("order_id".to_string(), "O456".to_string());

        let row_id = RowId::database(DatabaseType::DB2, "orders", pk);
        let key = row_id.to_key();

        assert!(key.starts_with("db2:orders:"));
        assert!(key.contains("customer_id=C123"));
        assert!(key.contains("order_id=O456"));
    }

    #[test]
    fn test_row_id_databricks() {
        let mut pk = BTreeMap::new();
        pk.insert("event_id".to_string(), "evt-123".to_string());

        let row_id = RowId::database(DatabaseType::Databricks, "main.bronze.events", pk);
        assert_eq!(
            row_id.to_key(),
            "databricks:main.bronze.events:event_id=evt-123"
        );
    }

    #[test]
    fn test_row_id_kafka() {
        let row_id = RowId::kafka("orders-topic", 5, 987654);
        assert_eq!(row_id.to_key(), "kafka:orders-topic:p5:o987654");
    }

    #[test]
    fn test_row_lineage_event_success() {
        let row_id = RowId::csv("test.csv", 1);
        let event = RowLineageEvent::success(
            row_id.clone(),
            "batch-123".to_string(),
            "job-456".to_string(),
            "/output/test.csv".to_string(),
            "tenant-a".to_string(),
        );

        assert!(event.is_success());
        assert!(!event.is_filtered());
        assert_eq!(event.row_id, row_id);
    }

    #[test]
    fn test_row_transformation() {
        let mut transform =
            RowTransformation::new("proper_case", vec!["name".to_string(), "city".to_string()]);

        let mut before = HashMap::new();
        before.insert("name".to_string(), serde_json::json!("john doe"));
        before.insert("city".to_string(), serde_json::json!("new york"));

        let mut after = HashMap::new();
        after.insert("name".to_string(), serde_json::json!("John Doe"));
        after.insert("city".to_string(), serde_json::json!("New York"));

        transform = transform.with_values(before.clone(), after.clone());

        assert_eq!(transform.transform_type, "proper_case");
        assert_eq!(transform.fields.len(), 2);
        assert_eq!(transform.before_values.unwrap(), before);
        assert_eq!(transform.after_values.unwrap(), after);
    }
}
