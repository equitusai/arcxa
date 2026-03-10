//! Converter from Graphica Internal Lineage to OpenLineage Format
//!
//! This module provides conversion utilities to translate Graphica's internal
//! lineage events into OpenLineage 1.0.0 format for interoperability with
//! external lineage tools.

use super::event::{Dataset, EventType, OpenLineageEvent};
use super::facets::{
    DataQualityMetricsDatasetFacet, NominalTimeRunFacet, SchemaDatasetFacet, SchemaField,
    SqlJobFacet,
};
use crate::core::lineage::{DataRef, LineageEvent, ModelRef, TransformRef};
use chrono::{DateTime, Utc};
use std::collections::HashMap;

/// Converter for translating Graphica lineage to OpenLineage format
pub struct LineageConverter {
    /// Producer URI to identify Graphica as the source
    producer: String,
    /// Namespace for jobs (typically the scheduler/orchestrator name)
    job_namespace: String,
}

impl LineageConverter {
    /// Create a new converter with default settings
    pub fn new() -> Self {
        Self {
            producer: "https://github.com/graphica/graphica".to_string(),
            job_namespace: "graphica".to_string(),
        }
    }

    /// Create a converter with custom producer and namespace
    pub fn with_config(producer: String, job_namespace: String) -> Self {
        Self {
            producer,
            job_namespace,
        }
    }

    /// Convert a Graphica LineageEvent to an OpenLineage event
    ///
    /// This creates a COMPLETE event by default, representing successful completion
    /// of a data transformation job.
    pub fn convert(&self, event: &LineageEvent) -> OpenLineageEvent {
        self.convert_with_type(event, EventType::Complete)
    }

    /// Convert with a specific event type (START, COMPLETE, FAIL, etc.)
    pub fn convert_with_type(
        &self,
        event: &LineageEvent,
        event_type: EventType,
    ) -> OpenLineageEvent {
        let job_name = self.create_job_name(event);

        let mut ol_event = OpenLineageEvent::new(
            event_type,
            event.run_id.clone(),
            self.job_namespace.clone(),
            job_name,
            self.producer.clone(),
        );

        // Add input datasets from source_refs
        for source in &event.source_refs {
            let input_dataset = self.convert_data_ref_to_dataset(source);
            ol_event = ol_event.with_input(input_dataset);
        }

        // Add output dataset from output_ref
        let output_dataset = self.convert_data_ref_to_dataset(&event.output_ref);
        ol_event = ol_event.with_output(output_dataset);

        // Add nominal time facet to run (when the data was logically created)
        if let Some(nominal_time_facet) = self.create_nominal_time_facet(event) {
            ol_event = ol_event.with_run_facet(
                "nominalTime".to_string(),
                serde_json::to_value(nominal_time_facet).unwrap(),
            );
        }

        // Add SQL facet if there are SQL-like transformations
        if let Some(sql_facet) = self.create_sql_facet(&event.transforms) {
            ol_event = ol_event
                .with_job_facet("sql".to_string(), serde_json::to_value(sql_facet).unwrap());
        }

        // Add metadata as custom facets
        if !event.metadata.is_empty() {
            let metadata_facet = self.create_metadata_facet(&event.metadata);
            ol_event = ol_event.with_run_facet(
                "graphica_metadata".to_string(),
                serde_json::to_value(metadata_facet).unwrap(),
            );
        }

        ol_event
    }

    /// Convert a DataRef to an OpenLineage Dataset
    fn convert_data_ref_to_dataset(&self, data_ref: &DataRef) -> Dataset {
        // Namespace is the system (e.g., "postgres-prod", "salesforce")
        let namespace = format!(
            "{}://{}",
            self.extract_protocol(&data_ref.system),
            data_ref.system
        );

        // Name is the path (e.g., "public.customers", "accounts/123")
        let name = data_ref.path.clone();

        let mut dataset = Dataset::new(namespace, name);

        // Add version information if available
        if let Some(version) = &data_ref.version {
            let version_facet = serde_json::json!({
                "_producer": self.producer,
                "_schemaURL": "https://openlineage.io/spec/facets/1-0-0/DatasetVersionDatasetFacet.json",
                "datasetVersion": version
            });
            dataset = dataset.with_facet("datasetVersion".to_string(), version_facet);
        }

        // Add CDC position if available (custom facet)
        if let Some(cdc_pos) = &data_ref.cdc_position {
            let cdc_facet = serde_json::json!({
                "_producer": self.producer,
                "_schemaURL": "https://github.com/graphica/graphica/openlineage/CdcPositionFacet.json",
                "topic": cdc_pos.topic,
                "partition": cdc_pos.partition,
                "offset": cdc_pos.offset,
                "lsn": cdc_pos.lsn
            });
            dataset = dataset.with_facet("cdcPosition".to_string(), cdc_facet);
        }

        dataset
    }

    /// Create a job name from the lineage event
    ///
    /// Format: {dataset}.{transform_types}
    fn create_job_name(&self, event: &LineageEvent) -> String {
        if event.transforms.is_empty() {
            // If no transforms, use dataset name
            format!("{}.load", event.dataset)
        } else {
            // Create name from transform types
            let transform_types: Vec<String> = event
                .transforms
                .iter()
                .map(|t| t.transform_type.clone())
                .collect();
            format!("{}.{}", event.dataset, transform_types.join("_"))
        }
    }

    /// Extract protocol from system identifier
    fn extract_protocol(&self, system: &str) -> String {
        if system.contains("postgres") {
            "postgres".to_string()
        } else if system.contains("mysql") {
            "mysql".to_string()
        } else if system.contains("kafka") {
            "kafka".to_string()
        } else if system.contains("s3") {
            "s3".to_string()
        } else if system.contains("salesforce") {
            "salesforce".to_string()
        } else {
            "custom".to_string()
        }
    }

    /// Create nominal time facet from lineage event
    fn create_nominal_time_facet(&self, event: &LineageEvent) -> Option<NominalTimeRunFacet> {
        // Use the earliest source extraction time as nominal time
        let nominal_time = event.source_refs.iter().map(|s| s.extracted_at).min()?;

        Some(NominalTimeRunFacet::new(
            self.producer.clone(),
            nominal_time,
        ))
    }

    /// Create SQL facet from transforms
    fn create_sql_facet(&self, transforms: &[TransformRef]) -> Option<SqlJobFacet> {
        // Look for SQL-like transforms
        for transform in transforms {
            if transform.transform_type.contains("sql")
                || transform.transform_type.contains("query")
            {
                // Try to extract SQL from parameters
                if let Some(sql_value) = transform.parameters.get("sql") {
                    if let Some(sql_string) = sql_value.as_str() {
                        return Some(SqlJobFacet::new(
                            self.producer.clone(),
                            sql_string.to_string(),
                        ));
                    }
                }
            }
        }
        None
    }

    /// Create custom metadata facet
    fn create_metadata_facet(&self, metadata: &HashMap<String, String>) -> serde_json::Value {
        serde_json::json!({
            "_producer": self.producer,
            "_schemaURL": "https://github.com/graphica/graphica/openlineage/GraphicaMetadataFacet.json",
            "metadata": metadata
        })
    }

    /// Convert a batch of LineageEvents to OpenLineage events
    pub fn convert_batch(&self, events: &[LineageEvent]) -> Vec<OpenLineageEvent> {
        events.iter().map(|e| self.convert(e)).collect()
    }

    /// Convert a model execution to an OpenLineage event
    ///
    /// Creates a separate event for ML model inference jobs
    pub fn convert_model_execution(
        &self,
        event: &LineageEvent,
        model: &ModelRef,
    ) -> OpenLineageEvent {
        let job_name = format!("{}.model.{}", event.dataset, model.model_id);

        let mut ol_event = OpenLineageEvent::new(
            EventType::Complete,
            event.run_id.clone(),
            self.job_namespace.clone(),
            job_name,
            self.producer.clone(),
        );

        // Inputs: source data
        for source in &event.source_refs {
            ol_event = ol_event.with_input(self.convert_data_ref_to_dataset(source));
        }

        // Output: predictions
        ol_event = ol_event.with_output(self.convert_data_ref_to_dataset(&event.output_ref));

        // Add model metadata as custom facet
        let model_facet = serde_json::json!({
            "_producer": self.producer,
            "_schemaURL": "https://github.com/graphica/graphica/openlineage/MLModelFacet.json",
            "modelId": model.model_id,
            "modelVersion": model.version,
            "modelType": model.model_type,
            "paramsHash": model.params_hash,
            "registryUri": model.registry_uri,
            "metrics": {
                "accuracy": model.metrics.accuracy,
                "precision": model.metrics.precision,
                "recall": model.metrics.recall,
                "f1_score": model.metrics.f1_score,
                "rmse": model.metrics.rmse,
                "custom_metrics": model.metrics.custom_metrics
            },
            "featuresUsed": model.features_used,
            "outputs": model.outputs
        });

        ol_event.with_job_facet("mlModel".to_string(), model_facet)
    }
}

impl Default for LineageConverter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::lineage::{CdcPosition, ModelMetrics};
    use chrono::Utc;
    use uuid::Uuid;

    fn create_test_lineage_event() -> LineageEvent {
        LineageEvent {
            id: Uuid::new_v4(),
            dataset: "customers".to_string(),
            record_id: "cust-123".to_string(),
            source_refs: vec![DataRef {
                system: "postgres-prod".to_string(),
                path: "public.raw_customers".to_string(),
                version: Some("v1.2.0".to_string()),
                extracted_at: Utc::now(),
                cdc_position: Some(CdcPosition {
                    topic: "db.public.raw_customers".to_string(),
                    partition: 0,
                    offset: 12345,
                    lsn: Some("0/16B3740".to_string()),
                }),
            }],
            transforms: vec![TransformRef {
                id: Uuid::new_v4(),
                transform_type: "dedupe".to_string(),
                rule_id: "dedupe-v2".to_string(),
                version: "2.0.0".to_string(),
                parameters: HashMap::new(),
                applied_at: Utc::now(),
                fields_modified: vec!["email".to_string()],
            }],
            model_refs: vec![],
            output_ref: DataRef {
                system: "postgres-prod".to_string(),
                path: "public.customers".to_string(),
                version: Some("v1.2.0".to_string()),
                extracted_at: Utc::now(),
                cdc_position: None,
            },
            ts: Utc::now(),
            run_id: "run-456".to_string(),
            tenant_id: "acme-corp".to_string(),
            correlation_id: Some("trace-789".to_string()),
            metadata: {
                let mut m = HashMap::new();
                m.insert("env".to_string(), "production".to_string());
                m
            },
        }
    }

    #[test]
    fn test_convert_basic_event() {
        let converter = LineageConverter::new();
        let event = create_test_lineage_event();

        let ol_event = converter.convert(&event);

        assert_eq!(ol_event.event_type, EventType::Complete);
        assert_eq!(ol_event.run.run_id, "run-456");
        assert_eq!(ol_event.job.namespace, "graphica");
        assert_eq!(ol_event.job.name, "customers.dedupe");
        assert_eq!(ol_event.inputs.len(), 1);
        assert_eq!(ol_event.outputs.len(), 1);
    }

    #[test]
    fn test_dataset_conversion() {
        let converter = LineageConverter::new();
        let data_ref = DataRef {
            system: "postgres-prod".to_string(),
            path: "public.orders".to_string(),
            version: Some("v2.1.0".to_string()),
            extracted_at: Utc::now(),
            cdc_position: None,
        };

        let dataset = converter.convert_data_ref_to_dataset(&data_ref);

        assert_eq!(dataset.namespace, "postgres://postgres-prod");
        assert_eq!(dataset.name, "public.orders");
        assert!(dataset.facets.contains_key("datasetVersion"));
    }

    #[test]
    fn test_cdc_position_facet() {
        let converter = LineageConverter::new();
        let data_ref = DataRef {
            system: "kafka-cluster-1".to_string(),
            path: "orders.events".to_string(),
            version: None,
            extracted_at: Utc::now(),
            cdc_position: Some(CdcPosition {
                topic: "orders.events".to_string(),
                partition: 5,
                offset: 99999,
                lsn: None,
            }),
        };

        let dataset = converter.convert_data_ref_to_dataset(&data_ref);

        assert!(dataset.facets.contains_key("cdcPosition"));
        let cdc_facet = &dataset.facets["cdcPosition"];
        assert_eq!(cdc_facet["partition"], 5);
        assert_eq!(cdc_facet["offset"], 99999);
    }

    #[test]
    fn test_model_execution_conversion() {
        let converter = LineageConverter::new();
        let mut event = create_test_lineage_event();

        let model = ModelRef {
            model_id: "fraud-detector-v3".to_string(),
            version: "3.2.1".to_string(),
            model_type: "sklearn.RandomForestClassifier".to_string(),
            params_hash: "sha256:abc123".to_string(),
            training_data: vec![],
            metrics: ModelMetrics {
                accuracy: Some(0.94),
                precision: Some(0.92),
                recall: Some(0.89),
                f1_score: Some(0.905),
                rmse: None,
                custom_metrics: HashMap::new(),
            },
            registry_uri: "s3://models/fraud-detector/v3".to_string(),
            inference_at: Utc::now(),
            features_used: vec!["amount".to_string(), "merchant".to_string()],
            outputs: vec!["fraud_score".to_string()],
        };

        event.model_refs.push(model.clone());

        let ol_event = converter.convert_model_execution(&event, &model);

        assert_eq!(ol_event.job.name, "customers.model.fraud-detector-v3");
        assert!(ol_event.job.facets.contains_key("mlModel"));

        let ml_facet = &ol_event.job.facets["mlModel"];
        assert_eq!(ml_facet["modelId"], "fraud-detector-v3");
        assert_eq!(ml_facet["modelType"], "sklearn.RandomForestClassifier");
        assert_eq!(ml_facet["metrics"]["accuracy"], 0.94);
    }

    #[test]
    fn test_batch_conversion() {
        let converter = LineageConverter::new();
        let events = vec![
            create_test_lineage_event(),
            create_test_lineage_event(),
            create_test_lineage_event(),
        ];

        let ol_events = converter.convert_batch(&events);

        assert_eq!(ol_events.len(), 3);
        for ol_event in ol_events {
            assert_eq!(ol_event.producer, "https://github.com/graphica/graphica");
            assert_eq!(ol_event.job.namespace, "graphica");
        }
    }

    #[test]
    fn test_custom_producer_and_namespace() {
        let converter = LineageConverter::with_config(
            "https://my-company.com/lineage".to_string(),
            "data-platform".to_string(),
        );
        let event = create_test_lineage_event();

        let ol_event = converter.convert(&event);

        assert_eq!(ol_event.producer, "https://my-company.com/lineage");
        assert_eq!(ol_event.job.namespace, "data-platform");
    }

    #[test]
    fn test_sql_facet_extraction() {
        let converter = LineageConverter::new();
        let mut event = create_test_lineage_event();

        // Add SQL transform
        let mut sql_params = HashMap::new();
        sql_params.insert(
            "sql".to_string(),
            serde_json::Value::String("SELECT * FROM customers WHERE active = true".to_string()),
        );

        event.transforms.push(TransformRef {
            id: Uuid::new_v4(),
            transform_type: "sql_transform".to_string(),
            rule_id: "filter-active".to_string(),
            version: "1.0.0".to_string(),
            parameters: sql_params,
            applied_at: Utc::now(),
            fields_modified: vec![],
        });

        let ol_event = converter.convert(&event);

        assert!(ol_event.job.facets.contains_key("sql"));
        let sql_facet = &ol_event.job.facets["sql"];
        assert!(sql_facet["query"]
            .as_str()
            .unwrap()
            .contains("SELECT * FROM customers"));
    }
}
