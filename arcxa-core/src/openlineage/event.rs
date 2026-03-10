//! OpenLineage Event Model
//!
//! Implements the OpenLineage 1.0.0 specification for lineage event interchange.
//! See: https://openlineage.io/spec/1-0-0/

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// OpenLineage event according to spec 1.0.0
///
/// This is the root type for all OpenLineage events. Every event must include:
/// - Event metadata (type, time, producer)
/// - Run context (unique run ID)
/// - Job information (namespace + name)
/// - Input/Output datasets
///
/// # Example
/// ```
/// use graphica_core::openlineage::{OpenLineageEvent, EventType, Run, Job};
/// use chrono::Utc;
///
/// let event = OpenLineageEvent {
///     event_type: EventType::Complete,
///     event_time: Utc::now(),
///     run: Run {
///         run_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
///         facets: Default::default(),
///     },
///     job: Job {
///         namespace: "my-scheduler".to_string(),
///         name: "myjob.mytask".to_string(),
///         facets: Default::default(),
///     },
///     inputs: vec![],
///     outputs: vec![],
///     producer: "https://github.com/graphica/graphica".to_string(),
///     schema_url: "https://openlineage.io/spec/1-0-0/OpenLineage.json".to_string(),
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OpenLineageEvent {
    /// Type of the event (START, RUNNING, COMPLETE, FAIL, ABORT, OTHER)
    pub event_type: EventType,

    /// Time the event occurred (ISO 8601 timestamp)
    pub event_time: DateTime<Utc>,

    /// Run information - unique identifier for this job execution
    pub run: Run,

    /// Job information - identifies the job being run
    pub job: Job,

    /// Input datasets consumed by this job
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<Dataset>,

    /// Output datasets produced by this job
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<Dataset>,

    /// URI identifying the producer of this event
    /// Example: "https://github.com/my-org/my-scheduler"
    pub producer: String,

    /// URL to the OpenLineage schema version
    /// Should be: "https://openlineage.io/spec/1-0-0/OpenLineage.json"
    #[serde(rename = "schemaURL")]
    pub schema_url: String,
}

impl OpenLineageEvent {
    /// Create a new OpenLineage event
    pub fn new(
        event_type: EventType,
        run_id: String,
        job_namespace: String,
        job_name: String,
        producer: String,
    ) -> Self {
        Self {
            event_type,
            event_time: Utc::now(),
            run: Run {
                run_id,
                facets: Default::default(),
            },
            job: Job {
                namespace: job_namespace,
                name: job_name,
                facets: Default::default(),
            },
            inputs: vec![],
            outputs: vec![],
            producer,
            schema_url: "https://openlineage.io/spec/1-0-0/OpenLineage.json".to_string(),
        }
    }

    /// Add an input dataset
    pub fn with_input(mut self, dataset: Dataset) -> Self {
        self.inputs.push(dataset);
        self
    }

    /// Add an output dataset
    pub fn with_output(mut self, dataset: Dataset) -> Self {
        self.outputs.push(dataset);
        self
    }

    /// Add a run facet
    pub fn with_run_facet(mut self, key: String, facet: serde_json::Value) -> Self {
        self.run.facets.insert(key, facet);
        self
    }

    /// Add a job facet
    pub fn with_job_facet(mut self, key: String, facet: serde_json::Value) -> Self {
        self.job.facets.insert(key, facet);
        self
    }
}

/// Event type enumeration
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "UPPERCASE")]
pub enum EventType {
    /// Job started
    Start,

    /// Job is running (progress update)
    Running,

    /// Job completed successfully
    Complete,

    /// Job failed
    Fail,

    /// Job was aborted/cancelled
    Abort,

    /// Other event type
    Other,
}

/// Run information
///
/// Identifies a specific execution of a job. The run_id should be unique and immutable.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    /// Unique identifier for this run (typically a UUID)
    /// This ID is used to correlate all events for a single job execution
    pub run_id: String,

    /// Run facets - additional metadata about the run
    /// Common facets: nominalTime, parent, errorMessage, etc.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub facets: HashMap<String, serde_json::Value>,
}

/// Job information
///
/// Identifies a job within a namespace. The combination of namespace + name
/// should uniquely identify a job across the organization.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    /// Job namespace - typically the orchestrator or scheduler name
    /// Examples: "airflow", "prefect", "dagster", "my-scheduler"
    pub namespace: String,

    /// Job name - unique within the namespace
    /// Examples: "etl.daily_sales", "analytics.user_metrics"
    pub name: String,

    /// Job facets - additional metadata about the job
    /// Common facets: sql, sourceCode, documentation, ownership, etc.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub facets: HashMap<String, serde_json::Value>,
}

/// Dataset information
///
/// Represents a dataset that is input to or output from a job.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Dataset {
    /// Dataset namespace - typically the data platform
    /// Examples: "postgres://prod", "s3://my-bucket", "kafka://cluster"
    pub namespace: String,

    /// Dataset name - unique within the namespace
    /// Examples: "public.users", "sales/daily/2024-01-15.parquet"
    pub name: String,

    /// Dataset facets - additional metadata about the dataset
    /// Common facets: schema, dataQuality, lifecycle, ownership, etc.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub facets: HashMap<String, serde_json::Value>,
}

impl Dataset {
    /// Create a new dataset reference
    pub fn new(namespace: String, name: String) -> Self {
        Self {
            namespace,
            name,
            facets: Default::default(),
        }
    }

    /// Add a facet to the dataset
    pub fn with_facet(mut self, key: String, facet: serde_json::Value) -> Self {
        self.facets.insert(key, facet);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_creation() {
        let event = OpenLineageEvent::new(
            EventType::Complete,
            "run-123".to_string(),
            "my-scheduler".to_string(),
            "my-job".to_string(),
            "https://github.com/graphica/graphica".to_string(),
        );

        assert_eq!(event.event_type, EventType::Complete);
        assert_eq!(event.run.run_id, "run-123");
        assert_eq!(event.job.namespace, "my-scheduler");
        assert_eq!(event.job.name, "my-job");
        assert_eq!(
            event.schema_url,
            "https://openlineage.io/spec/1-0-0/OpenLineage.json"
        );
    }

    #[test]
    fn test_event_with_datasets() {
        let input = Dataset::new("postgres://prod".to_string(), "public.orders".to_string());

        let output = Dataset::new(
            "s3://data-lake".to_string(),
            "analytics/orders.parquet".to_string(),
        );

        let event = OpenLineageEvent::new(
            EventType::Complete,
            "run-456".to_string(),
            "airflow".to_string(),
            "etl.process_orders".to_string(),
            "https://airflow.example.com".to_string(),
        )
        .with_input(input)
        .with_output(output);

        assert_eq!(event.inputs.len(), 1);
        assert_eq!(event.outputs.len(), 1);
        assert_eq!(event.inputs[0].namespace, "postgres://prod");
        assert_eq!(event.outputs[0].namespace, "s3://data-lake");
    }

    #[test]
    fn test_event_serialization() {
        let event = OpenLineageEvent::new(
            EventType::Start,
            "550e8400-e29b-41d4-a716-446655440000".to_string(),
            "graphica".to_string(),
            "data_pipeline".to_string(),
            "https://github.com/graphica/graphica".to_string(),
        );

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"eventType\":\"START\""));
        assert!(json.contains("\"runId\":\"550e8400"));

        // Test round-trip
        let deserialized: OpenLineageEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.event_type, EventType::Start);
    }

    #[test]
    fn test_dataset_with_facets() {
        let facet = serde_json::json!({
            "_producer": "graphica",
            "_schemaURL": "https://openlineage.io/spec/facets/1-0-0/SchemaDatasetFacet.json",
            "fields": []
        });

        let dataset = Dataset::new("postgres://prod".to_string(), "public.users".to_string())
            .with_facet("schema".to_string(), facet);

        assert_eq!(dataset.facets.len(), 1);
        assert!(dataset.facets.contains_key("schema"));
    }
}
