//! OpenLineage Standard Facets
//!
//! Implements the standard facets defined in the OpenLineage specification.
//! Facets are the extension mechanism for adding rich metadata to lineage events.
//!
//! See: https://openlineage.io/spec/facets/

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Base facet trait - all facets must have a schema URL and producer
pub trait Facet {
    /// Get the schema URL for this facet
    fn schema_url(&self) -> &str;

    /// Get the producer URI for this facet
    fn producer(&self) -> &str;
}

// ============================================================================
// Dataset Facets
// ============================================================================

/// Schema facet - describes the structure of a dataset
///
/// This is one of the most important facets, providing the schema definition
/// for datasets so consumers can understand the data structure.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SchemaDatasetFacet {
    /// Producer of this facet
    #[serde(rename = "_producer")]
    pub producer: String,

    /// Schema URL
    #[serde(rename = "_schemaURL")]
    pub schema_url: String,

    /// List of fields in the dataset
    pub fields: Vec<SchemaField>,
}

impl SchemaDatasetFacet {
    /// Create a new schema facet
    pub fn new(producer: String, fields: Vec<SchemaField>) -> Self {
        Self {
            producer,
            schema_url: "https://openlineage.io/spec/facets/1-0-0/SchemaDatasetFacet.json"
                .to_string(),
            fields,
        }
    }
}

impl Facet for SchemaDatasetFacet {
    fn schema_url(&self) -> &str {
        &self.schema_url
    }

    fn producer(&self) -> &str {
        &self.producer
    }
}

/// Schema field definition
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SchemaField {
    /// Field name
    pub name: String,

    /// Field data type (e.g., "INTEGER", "VARCHAR", "TIMESTAMP")
    #[serde(rename = "type")]
    pub field_type: String,

    /// Optional field description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Nested fields (for complex types like STRUCT)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<SchemaField>,
}

impl SchemaField {
    /// Create a new field
    pub fn new(name: String, field_type: String) -> Self {
        Self {
            name,
            field_type,
            description: None,
            fields: vec![],
        }
    }

    /// Add a description
    pub fn with_description(mut self, description: String) -> Self {
        self.description = Some(description);
        self
    }

    /// Add nested fields (for complex types)
    pub fn with_fields(mut self, fields: Vec<SchemaField>) -> Self {
        self.fields = fields;
        self
    }
}

/// Data Quality facet - metrics about dataset quality
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DataQualityMetricsDatasetFacet {
    /// Producer of this facet
    #[serde(rename = "_producer")]
    pub producer: String,

    /// Schema URL
    #[serde(rename = "_schemaURL")]
    pub schema_url: String,

    /// Row count
    #[serde(skip_serializing_if = "Option::is_none")]
    pub row_count: Option<i64>,

    /// Byte size
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes: Option<i64>,

    /// File count (for file-based datasets)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_count: Option<i64>,

    /// Per-column metrics
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub column_metrics: HashMap<String, ColumnMetrics>,
}

impl DataQualityMetricsDatasetFacet {
    /// Create a new data quality facet
    pub fn new(producer: String) -> Self {
        Self {
            producer,
            schema_url:
                "https://openlineage.io/spec/facets/1-0-0/DataQualityMetricsDatasetFacet.json"
                    .to_string(),
            row_count: None,
            bytes: None,
            file_count: None,
            column_metrics: Default::default(),
        }
    }

    /// Set row count
    pub fn with_row_count(mut self, count: i64) -> Self {
        self.row_count = Some(count);
        self
    }

    /// Set byte size
    pub fn with_bytes(mut self, bytes: i64) -> Self {
        self.bytes = Some(bytes);
        self
    }
}

impl Facet for DataQualityMetricsDatasetFacet {
    fn schema_url(&self) -> &str {
        &self.schema_url
    }

    fn producer(&self) -> &str {
        &self.producer
    }
}

/// Column-level quality metrics
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ColumnMetrics {
    /// Number of null values
    #[serde(skip_serializing_if = "Option::is_none")]
    pub null_count: Option<i64>,

    /// Number of distinct values
    #[serde(skip_serializing_if = "Option::is_none")]
    pub distinct_count: Option<i64>,

    /// Minimum value
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<serde_json::Value>,

    /// Maximum value
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<serde_json::Value>,

    /// Sum (for numeric columns)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sum: Option<f64>,

    /// Quantiles
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub quantiles: HashMap<String, f64>,
}

/// Ownership facet - who owns/manages this dataset
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OwnershipDatasetFacet {
    /// Producer of this facet
    #[serde(rename = "_producer")]
    pub producer: String,

    /// Schema URL
    #[serde(rename = "_schemaURL")]
    pub schema_url: String,

    /// List of owners
    pub owners: Vec<Owner>,
}

impl OwnershipDatasetFacet {
    /// Create a new ownership facet
    pub fn new(producer: String, owners: Vec<Owner>) -> Self {
        Self {
            producer,
            schema_url: "https://openlineage.io/spec/facets/1-0-0/OwnershipDatasetFacet.json"
                .to_string(),
            owners,
        }
    }
}

impl Facet for OwnershipDatasetFacet {
    fn schema_url(&self) -> &str {
        &self.schema_url
    }

    fn producer(&self) -> &str {
        &self.producer
    }
}

/// Owner information
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Owner {
    /// Owner name or identifier
    pub name: String,

    /// Owner type (e.g., "user", "team", "service")
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub owner_type: Option<String>,
}

impl Owner {
    /// Create a new owner
    pub fn new(name: String) -> Self {
        Self {
            name,
            owner_type: None,
        }
    }

    /// Set owner type
    pub fn with_type(mut self, owner_type: String) -> Self {
        self.owner_type = Some(owner_type);
        self
    }
}

/// Documentation facet - human-readable documentation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DocumentationDatasetFacet {
    /// Producer of this facet
    #[serde(rename = "_producer")]
    pub producer: String,

    /// Schema URL
    #[serde(rename = "_schemaURL")]
    pub schema_url: String,

    /// Markdown documentation
    pub description: String,
}

impl DocumentationDatasetFacet {
    /// Create a new documentation facet
    pub fn new(producer: String, description: String) -> Self {
        Self {
            producer,
            schema_url: "https://openlineage.io/spec/facets/1-0-0/DocumentationDatasetFacet.json"
                .to_string(),
            description,
        }
    }
}

impl Facet for DocumentationDatasetFacet {
    fn schema_url(&self) -> &str {
        &self.schema_url
    }

    fn producer(&self) -> &str {
        &self.producer
    }
}

// ============================================================================
// Job Facets
// ============================================================================

/// SQL facet - SQL query executed by the job
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SqlJobFacet {
    /// Producer of this facet
    #[serde(rename = "_producer")]
    pub producer: String,

    /// Schema URL
    #[serde(rename = "_schemaURL")]
    pub schema_url: String,

    /// SQL query text
    pub query: String,
}

impl SqlJobFacet {
    /// Create a new SQL facet
    pub fn new(producer: String, query: String) -> Self {
        Self {
            producer,
            schema_url: "https://openlineage.io/spec/facets/1-0-0/SqlJobFacet.json".to_string(),
            query,
        }
    }
}

impl Facet for SqlJobFacet {
    fn schema_url(&self) -> &str {
        &self.schema_url
    }

    fn producer(&self) -> &str {
        &self.producer
    }
}

/// Source Code facet - code that defines the job
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SourceCodeJobFacet {
    /// Producer of this facet
    #[serde(rename = "_producer")]
    pub producer: String,

    /// Schema URL
    #[serde(rename = "_schemaURL")]
    pub schema_url: String,

    /// Programming language
    pub language: String,

    /// Source code text
    pub source_code: String,
}

impl SourceCodeJobFacet {
    /// Create a new source code facet
    pub fn new(producer: String, language: String, source_code: String) -> Self {
        Self {
            producer,
            schema_url: "https://openlineage.io/spec/facets/1-0-0/SourceCodeJobFacet.json"
                .to_string(),
            language,
            source_code,
        }
    }
}

impl Facet for SourceCodeJobFacet {
    fn schema_url(&self) -> &str {
        &self.schema_url
    }

    fn producer(&self) -> &str {
        &self.producer
    }
}

// ============================================================================
// Run Facets
// ============================================================================

/// Nominal Time facet - scheduled/nominal time for the run
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NominalTimeRunFacet {
    /// Producer of this facet
    #[serde(rename = "_producer")]
    pub producer: String,

    /// Schema URL
    #[serde(rename = "_schemaURL")]
    pub schema_url: String,

    /// Nominal start time
    pub nominal_start_time: DateTime<Utc>,

    /// Nominal end time
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nominal_end_time: Option<DateTime<Utc>>,
}

impl NominalTimeRunFacet {
    /// Create a new nominal time facet
    pub fn new(producer: String, nominal_start_time: DateTime<Utc>) -> Self {
        Self {
            producer,
            schema_url: "https://openlineage.io/spec/facets/1-0-0/NominalTimeRunFacet.json"
                .to_string(),
            nominal_start_time,
            nominal_end_time: None,
        }
    }

    /// Set nominal end time
    pub fn with_end_time(mut self, end_time: DateTime<Utc>) -> Self {
        self.nominal_end_time = Some(end_time);
        self
    }
}

impl Facet for NominalTimeRunFacet {
    fn schema_url(&self) -> &str {
        &self.schema_url
    }

    fn producer(&self) -> &str {
        &self.producer
    }
}

/// Parent Run facet - links to parent job run (for DAG workflows)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ParentRunFacet {
    /// Producer of this facet
    #[serde(rename = "_producer")]
    pub producer: String,

    /// Schema URL
    #[serde(rename = "_schemaURL")]
    pub schema_url: String,

    /// Parent run details
    pub run: ParentRun,

    /// Parent job details
    pub job: ParentJob,
}

impl ParentRunFacet {
    /// Create a new parent run facet
    pub fn new(producer: String, run_id: String, job_namespace: String, job_name: String) -> Self {
        Self {
            producer,
            schema_url: "https://openlineage.io/spec/facets/1-0-0/ParentRunFacet.json".to_string(),
            run: ParentRun { run_id },
            job: ParentJob {
                namespace: job_namespace,
                name: job_name,
            },
        }
    }
}

impl Facet for ParentRunFacet {
    fn schema_url(&self) -> &str {
        &self.schema_url
    }

    fn producer(&self) -> &str {
        &self.producer
    }
}

/// Parent run information
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ParentRun {
    /// Parent run ID
    #[serde(rename = "runId")]
    pub run_id: String,
}

/// Parent job information
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ParentJob {
    /// Parent job namespace
    pub namespace: String,

    /// Parent job name
    pub name: String,
}

/// Error Message facet - details when a run fails
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ErrorMessageRunFacet {
    /// Producer of this facet
    #[serde(rename = "_producer")]
    pub producer: String,

    /// Schema URL
    #[serde(rename = "_schemaURL")]
    pub schema_url: String,

    /// Error message
    pub message: String,

    /// Programming language for stack trace
    #[serde(skip_serializing_if = "Option::is_none")]
    pub programming_language: Option<String>,

    /// Stack trace
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_trace: Option<String>,
}

impl ErrorMessageRunFacet {
    /// Create a new error message facet
    pub fn new(producer: String, message: String) -> Self {
        Self {
            producer,
            schema_url: "https://openlineage.io/spec/facets/1-0-0/ErrorMessageRunFacet.json"
                .to_string(),
            message,
            programming_language: None,
            stack_trace: None,
        }
    }

    /// Add stack trace
    pub fn with_stack_trace(mut self, language: String, stack_trace: String) -> Self {
        self.programming_language = Some(language);
        self.stack_trace = Some(stack_trace);
        self
    }
}

impl Facet for ErrorMessageRunFacet {
    fn schema_url(&self) -> &str {
        &self.schema_url
    }

    fn producer(&self) -> &str {
        &self.producer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_facet() {
        let fields = vec![
            SchemaField::new("id".to_string(), "INTEGER".to_string()),
            SchemaField::new("name".to_string(), "VARCHAR".to_string())
                .with_description("User name".to_string()),
        ];

        let facet = SchemaDatasetFacet::new("graphica".to_string(), fields);

        assert_eq!(facet.fields.len(), 2);
        assert_eq!(facet.fields[0].name, "id");
        assert_eq!(facet.fields[1].description, Some("User name".to_string()));
    }

    #[test]
    fn test_ownership_facet() {
        let owners = vec![
            Owner::new("data-team".to_string()).with_type("team".to_string()),
            Owner::new("alice@example.com".to_string()).with_type("user".to_string()),
        ];

        let facet = OwnershipDatasetFacet::new("graphica".to_string(), owners);

        assert_eq!(facet.owners.len(), 2);
        assert_eq!(facet.owners[0].name, "data-team");
        assert_eq!(facet.owners[0].owner_type, Some("team".to_string()));
    }

    #[test]
    fn test_sql_facet() {
        let query = "SELECT * FROM users WHERE active = true";
        let facet = SqlJobFacet::new("graphica".to_string(), query.to_string());

        assert_eq!(facet.query, query);
        assert_eq!(
            facet.schema_url(),
            "https://openlineage.io/spec/facets/1-0-0/SqlJobFacet.json"
        );
    }

    #[test]
    fn test_error_facet() {
        let facet =
            ErrorMessageRunFacet::new("graphica".to_string(), "Division by zero".to_string())
                .with_stack_trace("rust".to_string(), "at main.rs:42".to_string());

        assert_eq!(facet.message, "Division by zero");
        assert_eq!(facet.programming_language, Some("rust".to_string()));
    }

    #[test]
    fn test_data_quality_facet() {
        let facet = DataQualityMetricsDatasetFacet::new("graphica".to_string())
            .with_row_count(1000)
            .with_bytes(1024 * 1024);

        assert_eq!(facet.row_count, Some(1000));
        assert_eq!(facet.bytes, Some(1024 * 1024));
    }
}
