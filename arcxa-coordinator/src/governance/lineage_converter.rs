//! LineageEvent to RDF Triple Conversion
//!
//! Converts LineageEvent domain objects to W3C PROV-based RDF triples for storage
//! in the governance brain. Follows RDF-First architecture principles.

use crate::governance::ontology::{GRAPHICA_NS, ML_NS, PROV_NS, RDF_NS};
use anyhow::Result;
use chrono::{DateTime, Utc};
use graphica_core::core::lineage::{CdcPosition, DataRef, LineageEvent, ModelMetrics, ModelRef};

/// Comprehensive LineageEvent converter with CDC positions and model details
pub struct LineageConverter;

impl LineageConverter {
    /// Convert a LineageEvent to RDF triples with full provenance
    ///
    /// Generates W3C PROV-compliant triples including:
    /// - CDC positions for replay capability
    /// - Transform details with parameters
    /// - Model metadata with metrics
    /// - Full source-to-output lineage chain
    pub fn to_rdf_triples_detailed(event: &LineageEvent) -> Result<Vec<(String, String, String)>> {
        let mut triples = Vec::new();

        // 1. Core event (PROV Activity)
        let event_uri = format!("{}/lineage/{}", GRAPHICA_NS, event.id);
        let activity_uri = format!("{}/activity/{}", GRAPHICA_NS, event.id);

        triples.push((
            activity_uri.clone(),
            format!("{}type", RDF_NS),
            format!("{}Activity", PROV_NS),
        ));

        triples.push((
            activity_uri.clone(),
            format!("{}startedAtTime", PROV_NS),
            Self::datetime_literal(&event.ts),
        ));

        triples.push((
            activity_uri.clone(),
            format!("{}dataset", GRAPHICA_NS),
            Self::string_literal(&event.dataset),
        ));

        triples.push((
            activity_uri.clone(),
            format!("{}recordId", GRAPHICA_NS),
            Self::string_literal(&event.record_id),
        ));

        triples.push((
            activity_uri.clone(),
            format!("{}runId", GRAPHICA_NS),
            Self::string_literal(&event.run_id),
        ));

        triples.push((
            activity_uri.clone(),
            format!("{}tenantId", GRAPHICA_NS),
            Self::string_literal(&event.tenant_id),
        ));

        // 2. Output entity (PROV Entity)
        let output_uri = Self::data_ref_uri(&event.output_ref);

        triples.push((
            output_uri.clone(),
            format!("{}type", RDF_NS),
            format!("{}Entity", PROV_NS),
        ));

        triples.push((
            output_uri.clone(),
            format!("{}wasGeneratedBy", PROV_NS),
            activity_uri.clone(),
        ));

        Self::add_data_ref_triples(&mut triples, &output_uri, &event.output_ref)?;

        // 3. Source entities with CDC positions
        for (idx, source) in event.source_refs.iter().enumerate() {
            let source_uri = Self::data_ref_uri(source);

            triples.push((
                source_uri.clone(),
                format!("{}type", RDF_NS),
                format!("{}Entity", PROV_NS),
            ));

            triples.push((
                activity_uri.clone(),
                format!("{}used", PROV_NS),
                source_uri.clone(),
            ));

            triples.push((
                source_uri.clone(),
                format!("{}sourceIndex", GRAPHICA_NS),
                Self::int_literal(idx as i64),
            ));

            Self::add_data_ref_triples(&mut triples, &source_uri, source)?;

            // CDC position (critical for replay)
            if let Some(ref cdc) = source.cdc_position {
                Self::add_cdc_position_triples(&mut triples, &source_uri, cdc)?;
            }
        }

        // 4. Transform steps
        for (idx, transform) in event.transforms.iter().enumerate() {
            let transform_uri = format!("{}/transform/{}", GRAPHICA_NS, transform.id);

            triples.push((
                transform_uri.clone(),
                format!("{}type", RDF_NS),
                format!("{}Activity", PROV_NS),
            ));

            triples.push((
                activity_uri.clone(),
                format!("{}hadTransform", GRAPHICA_NS),
                transform_uri.clone(),
            ));

            triples.push((
                transform_uri.clone(),
                format!("{}transformIndex", GRAPHICA_NS),
                Self::int_literal(idx as i64),
            ));

            triples.push((
                transform_uri.clone(),
                format!("{}transformType", GRAPHICA_NS),
                Self::string_literal(&transform.transform_type),
            ));

            triples.push((
                transform_uri.clone(),
                format!("{}ruleId", GRAPHICA_NS),
                Self::string_literal(&transform.rule_id),
            ));

            triples.push((
                transform_uri.clone(),
                format!("{}version", GRAPHICA_NS),
                Self::string_literal(&transform.version),
            ));

            triples.push((
                transform_uri.clone(),
                format!("{}appliedAt", GRAPHICA_NS),
                Self::datetime_literal(&transform.applied_at),
            ));

            // Parameters as JSON string
            if !transform.parameters.is_empty() {
                let params_json = serde_json::to_string(&transform.parameters)
                    .unwrap_or_else(|_| "{}".to_string());
                triples.push((
                    transform_uri.clone(),
                    format!("{}parameters", GRAPHICA_NS),
                    Self::string_literal(&params_json),
                ));
            }

            // Fields modified
            for field in &transform.fields_modified {
                triples.push((
                    transform_uri.clone(),
                    format!("{}modifiedField", GRAPHICA_NS),
                    Self::string_literal(field),
                ));
            }
        }

        // 5. Model references with metrics
        for model_ref in &event.model_refs {
            Self::add_model_ref_triples(&mut triples, &activity_uri, model_ref)?;
        }

        // 6. Correlation ID
        if let Some(ref correlation_id) = event.correlation_id {
            triples.push((
                event_uri.clone(),
                format!("{}correlationId", GRAPHICA_NS),
                Self::string_literal(correlation_id),
            ));
        }

        // 7. Metadata
        for (key, value) in &event.metadata {
            triples.push((
                event_uri.clone(),
                format!("{}{}", GRAPHICA_NS, key),
                Self::string_literal(value),
            ));
        }

        Ok(triples)
    }

    /// Add DataRef triples (system, path, version, timestamp)
    fn add_data_ref_triples(
        triples: &mut Vec<(String, String, String)>,
        uri: &str,
        data_ref: &DataRef,
    ) -> Result<()> {
        triples.push((
            uri.to_string(),
            format!("{}system", GRAPHICA_NS),
            Self::string_literal(&data_ref.system),
        ));

        triples.push((
            uri.to_string(),
            format!("{}path", GRAPHICA_NS),
            Self::string_literal(&data_ref.path),
        ));

        if let Some(ref version) = data_ref.version {
            triples.push((
                uri.to_string(),
                format!("{}version", GRAPHICA_NS),
                Self::string_literal(version),
            ));
        }

        triples.push((
            uri.to_string(),
            format!("{}extractedAt", GRAPHICA_NS),
            Self::datetime_literal(&data_ref.extracted_at),
        ));

        Ok(())
    }

    /// Add CDC position triples for replay capability
    fn add_cdc_position_triples(
        triples: &mut Vec<(String, String, String)>,
        source_uri: &str,
        cdc: &CdcPosition,
    ) -> Result<()> {
        let cdc_uri = format!(
            "{}/cdc/{}/{}_{}",
            GRAPHICA_NS,
            Self::sanitize(&cdc.topic),
            cdc.partition,
            cdc.offset
        );

        triples.push((
            cdc_uri.clone(),
            format!("{}type", RDF_NS),
            format!("{}CdcPosition", GRAPHICA_NS),
        ));

        triples.push((
            cdc_uri.clone(),
            format!("{}topic", GRAPHICA_NS),
            Self::string_literal(&cdc.topic),
        ));

        triples.push((
            cdc_uri.clone(),
            format!("{}partition", GRAPHICA_NS),
            Self::int_literal(cdc.partition as i64),
        ));

        triples.push((
            cdc_uri.clone(),
            format!("{}offset", GRAPHICA_NS),
            Self::int_literal(cdc.offset),
        ));

        if let Some(ref lsn) = cdc.lsn {
            triples.push((
                cdc_uri.clone(),
                format!("{}lsn", GRAPHICA_NS),
                Self::string_literal(lsn),
            ));
        }

        // Link source to CDC position
        triples.push((
            source_uri.to_string(),
            format!("{}cdcPosition", GRAPHICA_NS),
            cdc_uri,
        ));

        Ok(())
    }

    /// Add model reference triples with full metadata and metrics
    fn add_model_ref_triples(
        triples: &mut Vec<(String, String, String)>,
        activity_uri: &str,
        model: &ModelRef,
    ) -> Result<()> {
        let model_uri = format!("{}/model/{}/{}", ML_NS, model.model_id, model.version);

        triples.push((
            model_uri.clone(),
            format!("{}type", RDF_NS),
            format!("{}Model", ML_NS),
        ));

        triples.push((
            model_uri.clone(),
            format!("{}type", RDF_NS),
            format!("{}Agent", PROV_NS),
        ));

        triples.push((
            activity_uri.to_string(),
            format!("{}wasAssociatedWith", PROV_NS),
            model_uri.clone(),
        ));

        triples.push((
            model_uri.clone(),
            format!("{}modelId", ML_NS),
            Self::string_literal(&model.model_id),
        ));

        triples.push((
            model_uri.clone(),
            format!("{}version", ML_NS),
            Self::string_literal(&model.version),
        ));

        triples.push((
            model_uri.clone(),
            format!("{}modelType", ML_NS),
            Self::string_literal(&model.model_type),
        ));

        triples.push((
            model_uri.clone(),
            format!("{}paramsHash", ML_NS),
            Self::string_literal(&model.params_hash),
        ));

        triples.push((
            model_uri.clone(),
            format!("{}registryUri", ML_NS),
            Self::string_literal(&model.registry_uri),
        ));

        triples.push((
            model_uri.clone(),
            format!("{}inferenceAt", ML_NS),
            Self::datetime_literal(&model.inference_at),
        ));

        // Metrics
        Self::add_model_metrics_triples(triples, &model_uri, &model.metrics)?;

        // Features used
        for feature in &model.features_used {
            triples.push((
                model_uri.clone(),
                format!("{}featureUsed", ML_NS),
                Self::string_literal(feature),
            ));
        }

        // Outputs
        for output in &model.outputs {
            triples.push((
                model_uri.clone(),
                format!("{}output", ML_NS),
                Self::string_literal(output),
            ));
        }

        // Training data
        for (idx, training_data) in model.training_data.iter().enumerate() {
            let training_uri = Self::data_ref_uri(training_data);
            triples.push((
                model_uri.clone(),
                format!("{}trainingData", ML_NS),
                training_uri.clone(),
            ));

            triples.push((
                training_uri,
                format!("{}trainingDataIndex", ML_NS),
                Self::int_literal(idx as i64),
            ));
        }

        Ok(())
    }

    /// Add model metrics triples
    fn add_model_metrics_triples(
        triples: &mut Vec<(String, String, String)>,
        model_uri: &str,
        metrics: &ModelMetrics,
    ) -> Result<()> {
        if let Some(accuracy) = metrics.accuracy {
            triples.push((
                model_uri.to_string(),
                format!("{}accuracy", ML_NS),
                Self::double_literal(accuracy),
            ));
        }

        if let Some(precision) = metrics.precision {
            triples.push((
                model_uri.to_string(),
                format!("{}precision", ML_NS),
                Self::double_literal(precision),
            ));
        }

        if let Some(recall) = metrics.recall {
            triples.push((
                model_uri.to_string(),
                format!("{}recall", ML_NS),
                Self::double_literal(recall),
            ));
        }

        if let Some(f1_score) = metrics.f1_score {
            triples.push((
                model_uri.to_string(),
                format!("{}f1Score", ML_NS),
                Self::double_literal(f1_score),
            ));
        }

        if let Some(rmse) = metrics.rmse {
            triples.push((
                model_uri.to_string(),
                format!("{}rmse", ML_NS),
                Self::double_literal(rmse),
            ));
        }

        // Custom metrics
        for (key, value) in &metrics.custom_metrics {
            triples.push((
                model_uri.to_string(),
                format!("{}metric_{}", ML_NS, key),
                Self::double_literal(*value),
            ));
        }

        Ok(())
    }

    // URI generation helpers

    fn data_ref_uri(data_ref: &DataRef) -> String {
        let sanitized_path = Self::sanitize(&data_ref.path);
        let sanitized_system = Self::sanitize(&data_ref.system);
        format!(
            "{}/data/{}/{}",
            GRAPHICA_NS, sanitized_system, sanitized_path
        )
    }

    fn sanitize(s: &str) -> String {
        s.replace('/', "_").replace(':', "_").replace(' ', "_")
    }

    // Literal helpers

    fn string_literal(value: &str) -> String {
        // Simple string literal (proper RDF serialization handled by store)
        value.to_string()
    }

    fn datetime_literal(dt: &DateTime<Utc>) -> String {
        dt.to_rfc3339()
    }

    fn int_literal(value: i64) -> String {
        value.to_string()
    }

    fn double_literal(value: f64) -> String {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::collections::HashMap;

    #[test]
    fn test_lineage_event_to_rdf() {
        let event = LineageEvent {
            id: uuid::Uuid::new_v4(),
            dataset: "customers".to_string(),
            record_id: "rec_123".to_string(),
            source_refs: vec![DataRef {
                system: "postgres".to_string(),
                path: "public.customers".to_string(),
                version: Some("v1".to_string()),
                extracted_at: Utc::now(),
                cdc_position: Some(CdcPosition {
                    topic: "dbserver1.inventory.customers".to_string(),
                    partition: 0,
                    offset: 12345,
                    lsn: Some("0/123456".to_string()),
                }),
            }],
            transforms: vec![],
            model_refs: vec![],
            output_ref: DataRef {
                system: "graphica".to_string(),
                path: "governance.lineage".to_string(),
                version: None,
                extracted_at: Utc::now(),
                cdc_position: None,
            },
            ts: Utc::now(),
            run_id: "run_456".to_string(),
            tenant_id: "tenant_1".to_string(),
            correlation_id: None,
            metadata: HashMap::new(),
        };

        let triples = LineageConverter::to_rdf_triples_detailed(&event).unwrap();
        assert!(!triples.is_empty(), "Should generate triples");

        // Verify some key triples
        let has_activity = triples
            .iter()
            .any(|(_, p, o)| p.contains("type") && o.contains("Activity"));
        assert!(has_activity, "Should have Activity type");

        let has_cdc = triples.iter().any(|(_, p, _)| p.contains("cdcPosition"));
        assert!(has_cdc, "Should have CDC position");
    }
}
