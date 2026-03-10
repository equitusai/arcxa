// RDF-star Converters for Graphica Domain Types
//
// This module provides RDF-star conversion implementations for domain types,
// enabling statement-level metadata like confidence, provenance, and temporal info.

use super::ontology::uris;
use super::rdf_star::{
    predicates::*, AnnotatedTriple, AnnotatedTripleBuilder, Annotation, ToRdfStarTriples,
    TripleValue,
};
use anyhow::Result;
use graphica_core::core::lineage::LineageEvent;
use graphica_core::core::quality::{QualityScorecard, QualityViolation};
use std::collections::HashMap;
use uuid::Uuid;

/// Convert LineageEvent to RDF-star annotated triples
impl ToRdfStarTriples for LineageEvent {
    fn to_rdf_star_triples(&self) -> Result<Vec<AnnotatedTriple>> {
        let mut triples = Vec::new();
        let event_uri = uris::lineage(&self.id.to_string());

        // 1. Base lineage activity (with bitemporal annotation)
        let activity_triple = AnnotatedTripleBuilder::new(
            &event_uri,
            format!("{}type", PROV_NS),
            format!("{}Activity", PROV_NS),
        )
        .valid_time(self.ts, None) // Business time: when lineage became valid
        .timestamp(self.ts) // Keep for backward compat
        .transaction(&self.run_id)
        .build();
        triples.push(activity_triple);

        // 2. Source references with confidence and CDC position
        for source_ref in &self.source_refs {
            let source_uri = format!("{}source/{}", GRAPHICA_NS, Uuid::new_v4());

            // Main lineage->source relationship with annotations
            let mut builder =
                AnnotatedTripleBuilder::new(&event_uri, format!("{}used", PROV_NS), &source_uri)
                    .confidence(0.95) // Default high confidence for direct sources
                    .valid_time(source_ref.extracted_at, None) // Business time: when data was extracted
                    .timestamp(source_ref.extracted_at); // Keep for backward compat

            // Add CDC position if available
            if let Some(cdc_pos) = &source_ref.cdc_position {
                builder = builder.annotation(
                    format!("{}cdcOffset", GRAPHICA_NS),
                    TripleValue::integer(cdc_pos.offset),
                );
                builder = builder.annotation(
                    format!("{}cdcPartition", GRAPHICA_NS),
                    TripleValue::integer(cdc_pos.partition as i64),
                );
                if let Some(lsn) = &cdc_pos.lsn {
                    builder = builder
                        .annotation(format!("{}cdcLsn", GRAPHICA_NS), TripleValue::literal(lsn));
                }
            }

            triples.push(builder.build());

            // Source entity details (plain triples, no annotations needed)
            triples.push(AnnotatedTriple::new(
                &source_uri,
                format!("{}type", PROV_NS),
                format!("{}Entity", PROV_NS),
            ));

            triples.push(AnnotatedTriple::new(
                &source_uri,
                format!("{}system", GRAPHICA_NS),
                &source_ref.system,
            ));

            triples.push(AnnotatedTriple::new(
                &source_uri,
                format!("{}path", GRAPHICA_NS),
                &source_ref.path,
            ));
        }

        // 3. Transform steps with confidence decay
        let mut cumulative_confidence = 0.95;
        for (idx, transform_ref) in self.transforms.iter().enumerate() {
            let transform_uri = format!("{}transform/{}", GRAPHICA_NS, transform_ref.id);

            // Apply confidence decay per transform (0.98 factor per step)
            cumulative_confidence *= 0.98;

            let transform_triple = AnnotatedTripleBuilder::new(
                &event_uri,
                format!("{}hadStep", PROV_NS),
                &transform_uri,
            )
            .confidence(cumulative_confidence)
            .annotation(
                format!("{}stepOrder", GRAPHICA_NS),
                TripleValue::integer(idx as i64),
            )
            .annotation(
                format!("{}transformType", GRAPHICA_NS),
                TripleValue::literal(&transform_ref.transform_type),
            )
            .valid_time(transform_ref.applied_at, None) // Business time: when transform was applied
            .timestamp(transform_ref.applied_at) // Keep for backward compat
            .build();

            triples.push(transform_triple);

            // Transform metadata
            if !transform_ref.fields_modified.is_empty() {
                for field in &transform_ref.fields_modified {
                    triples.push(AnnotatedTriple::new(
                        &transform_uri,
                        format!("{}modifiedField", GRAPHICA_NS),
                        field,
                    ));
                }
            }
        }

        // 4. Model references with confidence and training metadata
        for model_ref in &self.model_refs {
            let model_uri = uris::model(&model_ref.model_id);

            // Extract confidence from model metrics (use accuracy as proxy for confidence)
            let model_confidence = model_ref
                .metrics
                .accuracy
                .or(model_ref.metrics.f1_score)
                .or(model_ref.metrics.precision)
                .unwrap_or(0.75);

            let model_triple = AnnotatedTripleBuilder::new(
                &event_uri,
                format!("{}wasAssociatedWith", PROV_NS),
                &model_uri,
            )
            .confidence(model_confidence)
            .annotation(MODEL_VERSION, TripleValue::literal(&model_ref.version))
            .annotation(
                format!("{}paramsHash", ML_NS),
                TripleValue::literal(&model_ref.params_hash),
            )
            .valid_time(chrono::Utc::now(), None) // Business time: when model was associated
            .timestamp(chrono::Utc::now()) // Keep for backward compat
            .build();

            triples.push(model_triple);

            // Model training data references (with confidence about training data quality)
            for training_ref in &model_ref.training_data {
                let training_triple = AnnotatedTripleBuilder::new(
                    &model_uri,
                    format!("{}trainedOn", ML_NS),
                    format!("{}dataset/{}", GRAPHICA_NS, training_ref.path),
                )
                .confidence(0.9) // High confidence in training data provenance
                .annotation(
                    format!("{}trainingSystem", ML_NS),
                    TripleValue::literal(&training_ref.system),
                )
                .build();

                triples.push(training_triple);
            }

            // Model metrics as triples
            if let Some(accuracy) = model_ref.metrics.accuracy {
                triples.push(AnnotatedTriple::new(
                    &model_uri,
                    format!("{}accuracy", ML_NS),
                    accuracy.to_string(),
                ));
            }
            if let Some(precision) = model_ref.metrics.precision {
                triples.push(AnnotatedTriple::new(
                    &model_uri,
                    format!("{}precision", ML_NS),
                    precision.to_string(),
                ));
            }
            if let Some(recall) = model_ref.metrics.recall {
                triples.push(AnnotatedTriple::new(
                    &model_uri,
                    format!("{}recall", ML_NS),
                    recall.to_string(),
                ));
            }
            if let Some(f1_score) = model_ref.metrics.f1_score {
                triples.push(AnnotatedTriple::new(
                    &model_uri,
                    format!("{}f1_score", ML_NS),
                    f1_score.to_string(),
                ));
            }
            // Custom metrics
            for (metric_name, metric_value) in &model_ref.metrics.custom_metrics {
                triples.push(AnnotatedTriple::new(
                    &model_uri,
                    format!("{}metric_{}", ML_NS, metric_name),
                    metric_value.to_string(),
                ));
            }
        }

        // 5. Output entity with generation confidence
        let output_uri = uris::entity(&self.record_id);
        let output_triple = AnnotatedTripleBuilder::new(
            &output_uri,
            format!("{}wasGeneratedBy", PROV_NS),
            &event_uri,
        )
        .confidence(cumulative_confidence)
        .valid_time(self.ts, None) // Business time: when output was generated
        .timestamp(self.ts) // Keep for backward compat
        .transaction(&self.run_id)
        .build();

        triples.push(output_triple);

        // 6. Dataset and record metadata
        triples.push(AnnotatedTriple::new(
            &event_uri,
            format!("{}dataset", GRAPHICA_NS),
            &self.dataset,
        ));

        triples.push(AnnotatedTriple::new(
            &event_uri,
            format!("{}recordId", GRAPHICA_NS),
            &self.record_id,
        ));

        // 7. Tenant isolation
        triples.push(AnnotatedTriple::new(
            &event_uri,
            format!("{}tenantId", GRAPHICA_NS),
            &self.tenant_id,
        ));

        // 8. Correlation tracking (if present)
        if let Some(corr_id) = &self.correlation_id {
            let corr_triple = AnnotatedTripleBuilder::new(&event_uri, CORRELATION_ID, corr_id)
                .valid_time(self.ts, None) // Business time: when correlation was established
                .timestamp(self.ts) // Keep for backward compat
                .build();
            triples.push(corr_triple);
        }

        Ok(triples)
    }
}

/// Model prediction with confidence as RDF-star
pub struct ModelPrediction {
    pub entity_id: String,
    pub model_id: String,
    pub model_version: String,
    pub attribute_name: String,
    pub value: String,
    pub confidence: f64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub explanation: Option<HashMap<String, f64>>,
}

impl ToRdfStarTriples for ModelPrediction {
    fn to_rdf_star_triples(&self) -> Result<Vec<AnnotatedTriple>> {
        let mut triples = Vec::new();

        let entity_uri = uris::entity(&self.entity_id);
        let attr_uri = uris::attribute(&Uuid::new_v4().to_string());

        // Main prediction with confidence
        let prediction_triple = AnnotatedTripleBuilder::new(
            &entity_uri,
            format!("{}hasDerivedAttribute", GRAPHICA_NS),
            &attr_uri,
        )
        .confidence(self.confidence)
        .model(&self.model_id, &self.model_version)
        .valid_time(self.timestamp, None) // Business time: when prediction was made
        .timestamp(self.timestamp) // Keep for backward compat
        .build();

        triples.push(prediction_triple);

        // Attribute details
        triples.push(AnnotatedTriple::new(
            &attr_uri,
            format!("{}attributeName", GRAPHICA_NS),
            &self.attribute_name,
        ));

        triples.push(AnnotatedTriple::new(
            &attr_uri,
            format!("{}value", GRAPHICA_NS),
            &self.value,
        ));

        // Model explanation (feature importances) if available
        if let Some(explanation) = &self.explanation {
            for (feature, importance) in explanation {
                let explanation_triple = AnnotatedTripleBuilder::new(
                    &attr_uri,
                    format!("{}explainedBy", ML_NS),
                    feature,
                )
                .annotation(
                    format!("{}featureImportance", ML_NS),
                    TripleValue::decimal(*importance),
                )
                .build();

                triples.push(explanation_triple);
            }
        }

        Ok(triples)
    }
}

/// Fusion operation as RDF-star with confidence and reversal support
pub struct FusionOperation {
    pub fusion_id: String,
    pub merged_entity_id: String,
    pub source_entity_ids: Vec<String>,
    pub rule_id: String,
    pub method: String,
    pub confidence: f64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub reversed_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl ToRdfStarTriples for FusionOperation {
    fn to_rdf_star_triples(&self) -> Result<Vec<AnnotatedTriple>> {
        let mut triples = Vec::new();

        let fusion_uri = uris::fusion(&self.fusion_id);
        let merged_uri = uris::entity(&self.merged_entity_id);

        // Fusion operation metadata
        triples.push(AnnotatedTriple::new(
            &fusion_uri,
            format!("{}type", PROV_NS),
            format!("{}FusionOperation", GRAPHICA_NS),
        ));

        // Each source entity relationship with fusion metadata
        for source_id in &self.source_entity_ids {
            let source_uri = uris::entity(source_id);

            let mut builder = AnnotatedTripleBuilder::new(
                &merged_uri,
                "http://www.w3.org/2002/07/owl#sameAs",
                &source_uri,
            )
            .annotation(FUSION_METHOD, TripleValue::literal(&self.method))
            .annotation(FUSION_CONFIDENCE, TripleValue::decimal(self.confidence))
            .annotation(
                format!("{}fusionRule", GRAPHICA_NS),
                TripleValue::literal(&self.rule_id),
            )
            .valid_time(self.timestamp, self.reversed_at) // Business time: when fusion was valid
            .timestamp(self.timestamp); // Keep for backward compat

            // Add reversal timestamp if fusion was reversed
            if let Some(reversed) = self.reversed_at {
                builder = builder.annotation(REVERSAL_TIMESTAMP, TripleValue::datetime(&reversed));
            } else {
                builder = builder.annotation(REVERSAL_TIMESTAMP, TripleValue::literal("null"));
            }

            triples.push(builder.build());

            // Track fusion operation for audit
            let fusion_link = AnnotatedTripleBuilder::new(
                &fusion_uri,
                format!("{}mergedEntities", GRAPHICA_NS),
                &source_uri,
            )
            .confidence(self.confidence)
            .build();

            triples.push(fusion_link);
        }

        Ok(triples)
    }
}

/// Quality violation as RDF-star
impl ToRdfStarTriples for QualityViolation {
    fn to_rdf_star_triples(&self) -> Result<Vec<AnnotatedTriple>> {
        let mut triples = Vec::new();

        let violation_uri = format!("{}violation/{}", GRAPHICA_NS, Uuid::new_v4());
        let dataset_uri = format!("{}dataset/{}", GRAPHICA_NS, self.dataset);

        // Violation detected on dataset with severity
        let violation_triple = AnnotatedTripleBuilder::new(
            &dataset_uri,
            format!("{}hasViolation", GRAPHICA_NS),
            &violation_uri,
        )
        .annotation(
            format!("{}severity", GRAPHICA_NS),
            TripleValue::literal(format!("{:?}", self.severity)),
        )
        .annotation(
            format!("{}ruleId", GRAPHICA_NS),
            TripleValue::literal(&self.rule_id),
        )
        .valid_time(self.detected_at, None) // Business time: when violation was detected
        .timestamp(self.detected_at) // Keep for backward compat
        .build();

        triples.push(violation_triple);

        // Violation message
        triples.push(AnnotatedTriple::new(
            &violation_uri,
            format!("{}message", GRAPHICA_NS),
            &self.message,
        ));

        let record_triple = AnnotatedTripleBuilder::new(
            &violation_uri,
            format!("{}affectedRecord", GRAPHICA_NS),
            &self.record_id,
        )
        .confidence(1.0) // Full confidence in violation detection
        .build();

        triples.push(record_triple);

        if let Some(field) = &self.field {
            triples.push(AnnotatedTriple::new(
                &violation_uri,
                format!("{}affectedField", GRAPHICA_NS),
                field,
            ));
        }

        // Link to lineage if available
        if let Some(lineage_ref) = &self.lineage_ref {
            let lineage_triple = AnnotatedTripleBuilder::new(
                &violation_uri,
                format!("{}detectedInLineage", GRAPHICA_NS),
                uris::lineage(&lineage_ref.to_string()),
            )
            .valid_time(self.detected_at, None) // Business time: when linkage was detected
            .timestamp(self.detected_at) // Keep for backward compat
            .build();

            triples.push(lineage_triple);
        }

        Ok(triples)
    }
}

/// Quality scorecard as RDF-star
impl ToRdfStarTriples for QualityScorecard {
    fn to_rdf_star_triples(&self) -> Result<Vec<AnnotatedTriple>> {
        let mut triples = Vec::new();

        let scorecard_uri = format!("{}scorecard/{}", GRAPHICA_NS, Uuid::new_v4());
        let dataset_uri = format!("{}dataset/{}", GRAPHICA_NS, self.dataset);

        // Overall quality score with confidence in the assessment
        let score_triple = AnnotatedTripleBuilder::new(
            &dataset_uri,
            format!("{}hasQualityScore", GRAPHICA_NS),
            self.overall_score.to_string(),
        )
        .confidence(0.95) // High confidence in quality assessment
        .annotation(
            format!("{}totalRecords", GRAPHICA_NS),
            TripleValue::integer(self.total_records as i64),
        )
        .annotation(
            format!("{}periodStart", GRAPHICA_NS),
            TripleValue::datetime(&self.period_start),
        )
        .annotation(
            format!("{}periodEnd", GRAPHICA_NS),
            TripleValue::datetime(&self.period_end),
        )
        .build();

        triples.push(score_triple);

        // Individual dimension scores
        for (rule_type, score) in &self.dimension_scores {
            let dimension_triple = AnnotatedTripleBuilder::new(
                &scorecard_uri,
                format!("{}dimension_{:?}", GRAPHICA_NS, rule_type),
                score.to_string(),
            )
            .confidence(0.9)
            .build();

            triples.push(dimension_triple);
        }

        // Violation counts by severity
        for (severity, count) in &self.violation_counts {
            let violation_triple = AnnotatedTripleBuilder::new(
                &scorecard_uri,
                format!("{}violationCount", GRAPHICA_NS),
                count.to_string(),
            )
            .annotation(
                format!("{}severity", GRAPHICA_NS),
                TripleValue::literal(format!("{:?}", severity)),
            )
            .build();

            triples.push(violation_triple);
        }

        // Rule results
        for rule_result in &self.rule_results {
            let rule_triple = AnnotatedTripleBuilder::new(
                &scorecard_uri,
                format!("{}ruleResult", GRAPHICA_NS),
                &rule_result.rule_id,
            )
            .annotation(
                format!("{}passed", GRAPHICA_NS),
                TripleValue::integer(rule_result.passed as i64),
            )
            .annotation(
                format!("{}failed", GRAPHICA_NS),
                TripleValue::integer(rule_result.failed as i64),
            )
            .build();

            triples.push(rule_triple);
        }

        Ok(triples)
    }
}

/// Entity resolution result as RDF-star
pub struct EntityResolution {
    pub entity_id: String,
    pub candidate_ids: Vec<String>,
    pub match_scores: HashMap<String, f64>,
    pub selected_match: Option<String>,
    pub resolution_method: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl ToRdfStarTriples for EntityResolution {
    fn to_rdf_star_triples(&self) -> Result<Vec<AnnotatedTriple>> {
        let mut triples = Vec::new();

        let entity_uri = uris::entity(&self.entity_id);

        // Each candidate match with score
        for candidate_id in &self.candidate_ids {
            let candidate_uri = uris::entity(candidate_id);
            let match_score = self.match_scores.get(candidate_id).copied().unwrap_or(0.0);

            let mut builder = AnnotatedTripleBuilder::new(
                &entity_uri,
                format!("{}candidateMatch", GRAPHICA_NS),
                &candidate_uri,
            )
            .confidence(match_score)
            .annotation(
                format!("{}resolutionMethod", GRAPHICA_NS),
                TripleValue::literal(&self.resolution_method),
            )
            .valid_time(self.timestamp, None) // Business time: when match was computed
            .timestamp(self.timestamp); // Keep for backward compat

            // Mark selected match
            if self.selected_match.as_ref() == Some(candidate_id) {
                builder = builder.annotation(
                    format!("{}selectedMatch", GRAPHICA_NS),
                    TripleValue::boolean(true),
                );
            }

            triples.push(builder.build());
        }

        Ok(triples)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use graphica_core::core::lineage::{CdcPosition, DataRef, ModelRef, TransformRef};
    use graphica_core::core::quality::Severity;

    #[test]
    fn test_lineage_to_rdf_star() {
        let now = Utc::now();
        let model_metrics = graphica_core::core::lineage::ModelMetrics {
            accuracy: Some(0.87),
            precision: Some(0.85),
            recall: Some(0.90),
            f1_score: Some(0.875),
            rmse: None,
            custom_metrics: HashMap::new(),
        };

        let event = LineageEvent {
            id: Uuid::new_v4(),
            dataset: "customers".to_string(),
            record_id: "cust_456".to_string(),
            source_refs: vec![DataRef {
                system: "kafka".to_string(),
                path: "topic/customers".to_string(),
                version: None,
                extracted_at: now,
                cdc_position: Some(CdcPosition {
                    topic: "customers".to_string(),
                    partition: 2,
                    offset: 12345,
                    lsn: Some("0/1F2E3D4C".to_string()),
                }),
            }],
            transforms: vec![TransformRef {
                id: Uuid::new_v4(),
                transform_type: "standardize".to_string(),
                rule_id: "std_001".to_string(),
                version: "1.0.0".to_string(),
                parameters: HashMap::new(),
                applied_at: now,
                fields_modified: vec!["name".to_string(), "email".to_string()],
            }],
            model_refs: vec![ModelRef {
                model_id: "gender_classifier".to_string(),
                version: "2.1.0".to_string(),
                model_type: "classification".to_string(),
                params_hash: "abc123def456".to_string(),
                training_data: vec![],
                metrics: model_metrics,
                registry_uri: "mlflow://models/gender_classifier".to_string(),
                inference_at: now,
                features_used: vec!["name".to_string(), "email".to_string()],
                outputs: vec!["gender".to_string()],
            }],
            output_ref: DataRef {
                system: "output".to_string(),
                path: "processed/customers".to_string(),
                version: None,
                extracted_at: now,
                cdc_position: None,
            },
            ts: now,
            run_id: "run_789".to_string(),
            tenant_id: "tenant_001".to_string(),
            correlation_id: Some("corr_xyz".to_string()),
            metadata: HashMap::new(),
        };

        let triples = event.to_rdf_star_triples().unwrap();

        // Check for key annotations
        let has_confidence = triples.iter().any(|t| {
            t.annotations
                .iter()
                .any(|a| a.predicate.contains("confidence"))
        });
        assert!(has_confidence, "Should have confidence annotations");

        let has_cdc = triples.iter().any(|t| {
            t.annotations
                .iter()
                .any(|a| a.predicate.contains("cdcOffset"))
        });
        assert!(has_cdc, "Should have CDC position annotations");

        let has_model = triples
            .iter()
            .any(|t| t.predicate.contains("wasAssociatedWith"));
        assert!(has_model, "Should have model association");
    }

    #[test]
    fn test_fusion_with_reversal() {
        let fusion = FusionOperation {
            fusion_id: "fus_123".to_string(),
            merged_entity_id: "entity_final".to_string(),
            source_entity_ids: vec!["entity_a".to_string(), "entity_b".to_string()],
            rule_id: "email_match".to_string(),
            method: "exact_match".to_string(),
            confidence: 0.95,
            timestamp: Utc::now(),
            reversed_at: Some(Utc::now() + chrono::Duration::hours(2)),
        };

        let triples = fusion.to_rdf_star_triples().unwrap();

        // Check for reversal timestamp
        let has_reversal = triples.iter().any(|t| {
            t.annotations.iter().any(|a| {
                a.predicate.contains("reversalTimestamp")
                    && !matches!(&a.object, TripleValue::Literal(s) if s == "null")
            })
        });
        assert!(has_reversal, "Should have reversal timestamp");

        // Check fusion confidence
        let has_fusion_conf = triples.iter().any(|t| {
            t.annotations
                .iter()
                .any(|a| a.predicate.contains("fusionConfidence"))
        });
        assert!(has_fusion_conf, "Should have fusion confidence");
    }

    #[test]
    fn test_model_prediction_with_explanation() {
        let mut explanation = HashMap::new();
        explanation.insert("age".to_string(), 0.45);
        explanation.insert("income".to_string(), 0.35);
        explanation.insert("history".to_string(), 0.20);

        let prediction = ModelPrediction {
            entity_id: "cust_123".to_string(),
            model_id: "churn_model".to_string(),
            model_version: "3.0.1".to_string(),
            attribute_name: "churn_risk".to_string(),
            value: "high".to_string(),
            confidence: 0.82,
            timestamp: Utc::now(),
            explanation: Some(explanation),
        };

        let triples = prediction.to_rdf_star_triples().unwrap();

        // Check for explanation
        let explanation_count = triples
            .iter()
            .filter(|t| t.predicate.contains("explainedBy"))
            .count();
        assert_eq!(explanation_count, 3, "Should have 3 explanation features");

        // Check confidence annotation
        let has_confidence = triples.iter().any(|t| {
            t.annotations.iter().any(|a| {
                if let TripleValue::TypedLiteral { value, .. } = &a.object {
                    a.predicate.contains("confidence") && value == "0.82"
                } else {
                    false
                }
            })
        });
        assert!(has_confidence, "Should have correct confidence value");
    }

    #[test]
    fn test_quality_violation_rdf_star() {
        let violation = QualityViolation {
            id: Uuid::new_v4(),
            dataset: "orders".to_string(),
            rule_id: "completeness_01".to_string(),
            severity: Severity::Error,
            record_id: "order_789".to_string(),
            field: Some("customer_email".to_string()),
            expected_value: None,
            actual_value: None,
            message: "Missing required field".to_string(),
            detected_at: Utc::now(),
            resolved_at: None,
            lineage_ref: Some(Uuid::new_v4()),
        };

        let triples = violation.to_rdf_star_triples().unwrap();

        // Check severity annotation
        let has_severity = triples.iter().any(|t| {
            t.annotations
                .iter()
                .any(|a| a.predicate.contains("severity"))
        });
        assert!(has_severity, "Should have severity annotation");

        // Check lineage link
        let has_lineage = triples
            .iter()
            .any(|t| t.predicate.contains("detectedInLineage"));
        assert!(has_lineage, "Should link to lineage");
    }
}
