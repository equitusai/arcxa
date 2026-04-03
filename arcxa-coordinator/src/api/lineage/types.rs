//! Lineage API Types
//!
//! Request and response types for lineage query endpoints.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::ToSchema;

// Re-export graphica-core types for OpenAPI schema generation
pub use graphica_core::core::lineage::row_level::{
    DatabaseType, JobStatistics, JourneyStep, ProcessingOutcome, QualityViolation, RowId,
    RowJourney, RowLineageEvent, RowPosition, RowTransformation, SourceType,
};

/// Response for lineage record query
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LineageRecordResponse {
    /// Record ID
    pub record_id: String,
    /// Dataset name
    pub dataset: String,
    /// Run ID
    pub run_id: String,
    /// Tenant ID
    pub tenant_id: String,
    /// Event timestamp
    pub timestamp: DateTime<Utc>,
    /// Source data references
    pub sources: Vec<DataRefDto>,
    /// Transformation steps
    pub transforms: Vec<TransformDto>,
    /// ML models applied
    pub models: Vec<ModelDto>,
    /// Output reference
    pub output: DataRefDto,
    /// Optional metadata
    pub metadata: HashMap<String, String>,
}

/// Data reference DTO
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DataRefDto {
    /// System name
    pub system: String,
    /// Resource path
    pub path: String,
    /// Optional version
    pub version: Option<String>,
    /// Extraction timestamp
    pub extracted_at: DateTime<Utc>,
    /// CDC position if available
    pub cdc_position: Option<CdcPositionDto>,
}

/// CDC position DTO
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CdcPositionDto {
    /// Kafka topic
    pub topic: String,
    /// Partition
    pub partition: i32,
    /// Offset
    pub offset: i64,
    /// LSN if available
    pub lsn: Option<String>,
}

/// Transform reference DTO
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TransformDto {
    /// Transform ID
    pub id: String,
    /// Transform type
    pub transform_type: String,
    /// Rule ID
    pub rule_id: String,
    /// Version
    pub version: String,
    /// Parameters
    pub parameters: HashMap<String, serde_json::Value>,
    /// Applied timestamp
    pub applied_at: DateTime<Utc>,
    /// Fields modified
    pub fields_modified: Vec<String>,
}

/// Model reference DTO
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ModelDto {
    /// Model ID
    pub model_id: String,
    /// Model version
    pub version: String,
    /// Model type
    pub model_type: String,
    /// Parameters hash
    pub params_hash: String,
    /// Training data references
    pub training_data: Vec<DataRefDto>,
    /// Metrics
    pub metrics: ModelMetricsDto,
    /// Registry URI
    pub registry_uri: String,
    /// Inference timestamp
    pub inference_at: DateTime<Utc>,
    /// Features used
    pub features_used: Vec<String>,
    /// Outputs generated
    pub outputs: Vec<String>,
}

/// Model metrics DTO
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ModelMetricsDto {
    pub accuracy: Option<f64>,
    pub precision: Option<f64>,
    pub recall: Option<f64>,
    pub f1_score: Option<f64>,
    pub rmse: Option<f64>,
    pub custom_metrics: HashMap<String, f64>,
}

/// Lineage graph response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LineageGraphResponse {
    /// Root record ID
    pub root_record_id: String,
    /// All lineage events in the graph
    pub events: Vec<LineageRecordResponse>,
    /// Upstream records (sources)
    pub upstream_records: Vec<String>,
    /// Downstream records (consumers)
    pub downstream_records: Vec<String>,
    /// Lineage depth (longest path from sources)
    pub lineage_depth: usize,
    /// Total event count
    pub total_events: usize,
    /// Graph statistics
    pub statistics: LineageStatistics,
}

/// Lineage statistics
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LineageStatistics {
    /// Total source systems
    pub source_systems: usize,
    /// Total transforms applied
    pub transform_count: usize,
    /// Total models in chain
    pub model_count: usize,
    /// Total output systems
    pub output_systems: usize,
    /// Has circular dependency
    pub has_circular_dependency: bool,
}

/// Model impact analysis response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ModelImpactResponse {
    /// Model ID
    pub model_id: String,
    /// Model version
    pub version: String,
    /// Affected records
    pub affected_records: Vec<AffectedRecordDto>,
    /// Total record count
    pub total_affected: usize,
    /// First impact timestamp
    pub first_impact: Option<DateTime<Utc>>,
    /// Last impact timestamp
    pub last_impact: Option<DateTime<Utc>>,
    /// Datasets affected
    pub datasets: Vec<String>,
}

/// Affected record DTO
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AffectedRecordDto {
    /// Record ID
    pub record_id: String,
    /// Dataset
    pub dataset: String,
    /// Run ID
    pub run_id: String,
    /// Impact timestamp
    pub timestamp: DateTime<Utc>,
    /// Output path
    pub output_path: String,
}

/// Run lineage response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RunLineageResponse {
    /// Run ID
    pub run_id: String,
    /// Total records processed
    pub total_records: usize,
    /// Lineage events
    pub events: Vec<LineageRecordResponse>,
    /// Datasets processed
    pub datasets: Vec<String>,
    /// Run start time
    pub start_time: Option<DateTime<Utc>>,
    /// Run end time
    pub end_time: Option<DateTime<Utc>>,
}

/// Time range lineage query request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TimeRangeLineageQuery {
    /// Start timestamp
    pub start: DateTime<Utc>,
    /// End timestamp
    pub end: DateTime<Utc>,
    /// Optional dataset filter
    pub dataset: Option<String>,
    /// Maximum results
    pub limit: Option<usize>,
}

/// Time range lineage response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TimeRangeLineageResponse {
    /// Query start time
    pub start: DateTime<Utc>,
    /// Query end time
    pub end: DateTime<Utc>,
    /// Total events found
    pub total_events: usize,
    /// Lineage events
    pub events: Vec<LineageRecordResponse>,
    /// Datasets involved
    pub datasets: Vec<String>,
}

// =============================================================================
// Row-Level Lineage Types
// =============================================================================

/// Row-level lineage response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RowLineageResponse {
    /// Row key (e.g., "csv:file.csv:123" or "db2:table:pk=value")
    pub row_key: String,
    /// Lineage events for this row
    pub events: Vec<RowLineageEvent>,
    /// Total event count
    pub total_count: usize,
}

/// Row key search query parameters
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RowKeySearchQuery {
    /// Partial row key or datasource/table prefix
    pub q: String,
    /// Maximum number of matches to return
    pub limit: Option<usize>,
}

/// Single row-key autocomplete match
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RowKeySearchMatch {
    /// Full row key suitable for lineage lookup
    pub row_key: String,
    /// Source type portion of the row key (for example `oracle` or `csv`)
    pub source_type: String,
    /// Source identifier portion of the row key (for example table or file path)
    pub source_id: String,
}

/// Row-key autocomplete response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RowKeySearchResponse {
    /// Original query string
    pub query: String,
    /// Matching row keys ordered by match strength
    pub matches: Vec<RowKeySearchMatch>,
    /// Total matches returned in this response
    pub total_count: usize,
}

/// Batch lineage response (all rows in a batch)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BatchLineageResponse {
    /// Batch ID
    pub batch_id: String,
    /// All row events in this batch
    pub events: Vec<RowLineageEvent>,
    /// Total rows processed
    pub total_rows: usize,
}

/// Filtered rows query parameters
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FilteredRowsQuery {
    /// Start time
    pub start_time: DateTime<Utc>,
    /// End time
    pub end_time: DateTime<Utc>,
}

/// Filtered rows response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FilteredRowsResponse {
    /// Job ID
    pub job_id: String,
    /// Filtered rows
    pub filtered_rows: Vec<FilteredRow>,
    /// Total count
    pub total_count: usize,
}

/// Filtered row information
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FilteredRow {
    /// Row key
    pub row_key: String,
    /// Reason for filtering
    pub reason: String,
}
