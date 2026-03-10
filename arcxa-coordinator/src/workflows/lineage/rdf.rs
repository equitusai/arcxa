//! RDF-First Workflow Lineage Generation
//!
//! Generates comprehensive RDF triples for every workflow execution,
//! capturing field-level lineage, transformations, and model predictions.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use crate::governance::ontology::{GRAPHICA_NS, ML_NS, PROV_NS, RDF_NS, WORKFLOW_NS};
use crate::governance::rdf_store::{GraphicaRdfStore, NamedGraph, RdfStore, RdfTriple};

/// Comprehensive workflow lineage generator
pub struct WorkflowLineageGenerator {
    rdf_store: Arc<GraphicaRdfStore>,
}

impl WorkflowLineageGenerator {
    pub fn new(rdf_store: Arc<GraphicaRdfStore>) -> Self {
        Self { rdf_store }
    }

    /// Record workflow execution start
    pub fn record_workflow_start(
        &self,
        execution_id: &str,
        workflow_id: &str,
        started_at: DateTime<Utc>,
    ) -> Result<()> {
        let exec_uri = self.execution_uri(execution_id);
        let workflow_uri = self.workflow_uri(workflow_id);

        let triples = vec![
            RdfTriple::new(
                &exec_uri,
                &format!("{}type", RDF_NS),
                &format!("{}Activity", PROV_NS),
            ),
            RdfTriple::new(
                &exec_uri,
                &format!("{}executedWorkflow", WORKFLOW_NS),
                &workflow_uri,
            ),
            RdfTriple::new_typed(
                &exec_uri,
                &format!("{}startedAtTime", PROV_NS),
                &started_at.to_rfc3339(),
                "xsd:dateTime",
            ),
        ];

        self.rdf_store
            .insert_batch(&triples, Some(&NamedGraph::workflow_executions()))?;

        tracing::debug!("Recorded workflow start: {}", execution_id);
        Ok(())
    }

    /// Record step execution with field modifications
    pub fn record_step_execution(
        &self,
        execution_id: &str,
        step_id: &str,
        step_type: &str,
        modifications: Vec<FieldModification>,
        started_at: DateTime<Utc>,
        completed_at: DateTime<Utc>,
    ) -> Result<()> {
        let exec_uri = self.execution_uri(execution_id);
        let step_uri = format!("{}#{}", exec_uri, step_id);

        let mut triples = vec![
            RdfTriple::new(
                &step_uri,
                &format!("{}type", RDF_NS),
                &format!("{}Activity", PROV_NS),
            ),
            RdfTriple::new_literal(&step_uri, &format!("{}stepId", WORKFLOW_NS), step_id),
            RdfTriple::new_literal(&step_uri, &format!("{}stepType", WORKFLOW_NS), step_type),
            RdfTriple::new(&step_uri, &format!("{}wasPartOf", PROV_NS), &exec_uri),
            RdfTriple::new_typed(
                &step_uri,
                &format!("{}startedAtTime", PROV_NS),
                &started_at.to_rfc3339(),
                "xsd:dateTime",
            ),
            RdfTriple::new_typed(
                &step_uri,
                &format!("{}endedAtTime", PROV_NS),
                &completed_at.to_rfc3339(),
                "xsd:dateTime",
            ),
        ];

        // Add field modification triples
        for (idx, modification) in modifications.iter().enumerate() {
            let mod_uri = format!("{}/mod_{}", step_uri, idx);

            triples.push(RdfTriple::new(
                &step_uri,
                &format!("{}generated", PROV_NS),
                &mod_uri,
            ));
            triples.push(RdfTriple::new(
                &mod_uri,
                &format!("{}type", RDF_NS),
                &format!("{}FieldModification", WORKFLOW_NS),
            ));
            triples.push(RdfTriple::new_literal(
                &mod_uri,
                &format!("{}fieldName", WORKFLOW_NS),
                &modification.field_name,
            ));
            triples.push(RdfTriple::new_literal(
                &mod_uri,
                &format!("{}oldValue", WORKFLOW_NS),
                &modification.old_value.to_string(),
            ));
            triples.push(RdfTriple::new_literal(
                &mod_uri,
                &format!("{}newValue", WORKFLOW_NS),
                &modification.new_value.to_string(),
            ));
            triples.push(RdfTriple::new_typed(
                &mod_uri,
                &format!("{}confidence", WORKFLOW_NS),
                &modification.confidence.to_string(),
                "xsd:double",
            ));
            triples.push(RdfTriple::new_typed(
                &mod_uri,
                &format!("{}isReversible", WORKFLOW_NS),
                &modification.is_reversible.to_string(),
                "xsd:boolean",
            ));
        }

        tracing::warn!(
            "✓ RDF_GENERATOR: Inserting {} triples for step '{}' (modifications: {}) into workflow_executions graph",
            triples.len(),
            step_id,
            modifications.len()
        );

        let result = self
            .rdf_store
            .insert_batch(&triples, Some(&NamedGraph::workflow_executions()));

        match &result {
            Ok(_) => tracing::warn!(
                "✓ RDF_STORE: Successfully inserted {} triples for step '{}'",
                triples.len(),
                step_id
            ),
            Err(e) => tracing::error!(
                "✗ RDF_STORE: Failed to insert triples for step '{}': {}",
                step_id,
                e
            ),
        }

        result?;

        tracing::warn!(
            "✓ LINEAGE_COMPLETE: Recorded step execution: {} (modifications: {})",
            step_id,
            modifications.len()
        );
        Ok(())
    }

    /// Record workflow completion
    pub fn record_workflow_complete(
        &self,
        execution_id: &str,
        success: bool,
        completed_at: DateTime<Utc>,
    ) -> Result<()> {
        let exec_uri = self.execution_uri(execution_id);

        let triples = vec![
            RdfTriple::new_typed(
                &exec_uri,
                &format!("{}endedAtTime", PROV_NS),
                &completed_at.to_rfc3339(),
                "xsd:dateTime",
            ),
            RdfTriple::new_typed(
                &exec_uri,
                &format!("{}success", WORKFLOW_NS),
                &success.to_string(),
                "xsd:boolean",
            ),
        ];

        self.rdf_store
            .insert_batch(&triples, Some(&NamedGraph::workflow_executions()))?;

        tracing::debug!(
            "Recorded workflow completion: {} (success: {})",
            execution_id,
            success
        );
        Ok(())
    }

    /// Record ML predictions from workflow step
    ///
    /// Generates DerivedAttribute RDF triples following same structure as direct model API calls,
    /// plus additional workflow attribution for governance queries.
    pub fn record_ml_predictions(
        &self,
        execution_id: &str,
        step_id: &str,
        model_id: &str,
        model_version: &str,
        predictions: Vec<ModelPrediction>,
        started_at: DateTime<Utc>,
        completed_at: DateTime<Utc>,
    ) -> Result<()> {
        let exec_uri = self.execution_uri(execution_id);
        let step_uri = format!("{}#{}", exec_uri, step_id);
        let model_uri = format!("{}/model/{}", ML_NS, model_id);

        let mut triples = Vec::new();

        // 1. Step execution metadata
        triples.push(RdfTriple::new(
            &step_uri,
            &format!("{}type", RDF_NS),
            &format!("{}Activity", PROV_NS),
        ));
        triples.push(RdfTriple::new_literal(
            &step_uri,
            &format!("{}stepId", WORKFLOW_NS),
            step_id,
        ));
        triples.push(RdfTriple::new_literal(
            &step_uri,
            &format!("{}stepType", WORKFLOW_NS),
            "ml_predict",
        ));
        triples.push(RdfTriple::new(
            &step_uri,
            &format!("{}wasPartOf", PROV_NS),
            &exec_uri,
        ));
        triples.push(RdfTriple::new(
            &step_uri,
            &format!("{}usedModel", WORKFLOW_NS),
            &model_uri,
        ));
        triples.push(RdfTriple::new_typed(
            &step_uri,
            &format!("{}startedAtTime", PROV_NS),
            &started_at.to_rfc3339(),
            "xsd:dateTime",
        ));
        triples.push(RdfTriple::new_typed(
            &step_uri,
            &format!("{}endedAtTime", PROV_NS),
            &completed_at.to_rfc3339(),
            "xsd:dateTime",
        ));

        // 2. Generate DerivedAttribute triples for each prediction
        let prediction_count = predictions.len();
        for prediction in &predictions {
            let attr_id = if prediction.attribute_id.is_empty() {
                Uuid::new_v4().to_string()
            } else {
                prediction.attribute_id.clone()
            };
            let attr_uri = format!("{}/attr/{}", GRAPHICA_NS, attr_id);

            // Core attribute triples (same as direct API would create)
            triples.push(RdfTriple::new(
                &attr_uri,
                &format!("{}type", RDF_NS),
                &format!("{}DerivedAttribute", GRAPHICA_NS),
            ));
            triples.push(RdfTriple::new_literal(
                &attr_uri,
                &format!("{}attributeName", GRAPHICA_NS),
                &prediction.attribute_name,
            ));
            triples.push(RdfTriple::new_literal(
                &attr_uri,
                &format!("{}value", GRAPHICA_NS),
                &prediction.value.to_string(),
            ));
            triples.push(RdfTriple::new_typed(
                &attr_uri,
                &format!("{}confidence", GRAPHICA_NS),
                &prediction.confidence.to_string(),
                "xsd:double",
            ));

            // Provenance: link to model
            triples.push(RdfTriple::new(
                &attr_uri,
                &format!("{}wasGeneratedBy", PROV_NS),
                &model_uri,
            ));

            // Model version for lineage
            triples.push(RdfTriple::new_literal(
                &attr_uri,
                &format!("{}modelVersion", ML_NS),
                model_version,
            ));

            // Workflow attribution (CRITICAL: links prediction to workflow)
            triples.push(RdfTriple::new(
                &attr_uri,
                &format!("{}wasPartOf", PROV_NS),
                &step_uri,
            ));
            triples.push(RdfTriple::new(
                &attr_uri,
                &format!("{}wasPartOf", PROV_NS),
                &exec_uri,
            ));

            // Timestamp
            triples.push(RdfTriple::new_typed(
                &attr_uri,
                &format!("{}generatedAtTime", PROV_NS),
                &completed_at.to_rfc3339(),
                "xsd:dateTime",
            ));

            // Link step to attribute
            triples.push(RdfTriple::new(
                &step_uri,
                &format!("{}generated", PROV_NS),
                &attr_uri,
            ));
        }

        // Insert into RDF store
        self.rdf_store
            .insert_batch(&triples, Some(&NamedGraph::workflow_executions()))?;

        tracing::debug!(
            "Recorded ML predictions: step={}, predictions={}, triples={}",
            step_id,
            prediction_count,
            triples.len()
        );

        Ok(())
    }

    /// Generate complete lineage for workflow execution
    pub async fn generate_execution_lineage(
        &self,
        execution: &WorkflowExecution,
        entity_id: &str,
    ) -> Result<WorkflowLineageResult> {
        let mut triples = Vec::new();
        let mut field_modifications = Vec::new();
        let mut predictions = Vec::new();

        // 1. Workflow execution as PROV Activity
        let exec_uri = self.execution_uri(&execution.id);
        let workflow_uri = self.workflow_uri(&execution.workflow_id);

        triples.push(RdfTriple::new(
            &exec_uri,
            &format!("{}type", RDF_NS),
            &format!("{}Activity", PROV_NS),
        ));

        triples.push(RdfTriple::new(
            &exec_uri,
            &format!("{}executedWorkflow", WORKFLOW_NS),
            &workflow_uri,
        ));

        triples.push(RdfTriple::new(
            &exec_uri,
            &format!("{}startedAtTime", PROV_NS),
            &execution.started_at.to_rfc3339(),
        ));

        triples.push(RdfTriple::new(
            &exec_uri,
            &format!("{}endedAtTime", PROV_NS),
            &execution.completed_at.to_rfc3339(),
        ));

        // Confidence tracking
        triples.push(RdfTriple::new(
            &exec_uri,
            &format!("{}inputConfidence", WORKFLOW_NS),
            &execution.input_confidence.to_string(),
        ));

        triples.push(RdfTriple::new(
            &exec_uri,
            &format!("{}outputConfidence", WORKFLOW_NS),
            &execution.output_confidence.to_string(),
        ));

        // 2. Entity was generated/influenced by workflow
        let entity_uri = format!("{}/entity/{}", GRAPHICA_NS, entity_id);

        triples.push(RdfTriple::new(
            &entity_uri,
            &format!("{}wasGeneratedBy", PROV_NS),
            &exec_uri,
        ));

        triples.push(RdfTriple::new(
            &entity_uri,
            &format!("{}wasAttributedTo", PROV_NS),
            &workflow_uri,
        ));

        // 3. Process each step execution
        for step in &execution.steps {
            let step_triples = self
                .generate_step_lineage(step, &exec_uri, entity_id)
                .await?;

            // Collect modifications and predictions for summary
            match &step.action_type {
                ActionType::Transform {
                    modifications: mods,
                } => {
                    field_modifications.extend(mods.clone());
                }
                ActionType::MLPredict { prediction } => {
                    predictions.push(prediction.clone());
                }
                _ => {}
            }

            triples.extend(step_triples);
        }

        // 4. Store in time-partitioned named graph
        let graph = NamedGraph::new(format!(
            "{}/executions/{}",
            WORKFLOW_NS,
            execution.started_at.date()
        ));

        self.rdf_store.insert_batch(&triples, Some(&graph))?;

        Ok(WorkflowLineageResult {
            execution_uri: exec_uri,
            triples_generated: triples.len(),
            field_modifications,
            predictions,
            lineage_depth: self.calculate_lineage_depth(&execution),
        })
    }

    /// Generate lineage for individual step execution
    async fn generate_step_lineage(
        &self,
        step: &StepExecution,
        exec_uri: &str,
        entity_id: &str,
    ) -> Result<Vec<RdfTriple>> {
        let mut triples = Vec::new();
        let step_uri = self.step_execution_uri(&step.id);

        // Step execution metadata
        triples.push(RdfTriple::new(
            &step_uri,
            &format!("{}type", RDF_NS),
            &format!("{}Activity", PROV_NS),
        ));

        triples.push(RdfTriple::new(
            &step_uri,
            &format!("{}partOfExecution", WORKFLOW_NS),
            exec_uri,
        ));

        triples.push(RdfTriple::new(
            &step_uri,
            &format!("{}stepType", WORKFLOW_NS),
            &step.step_type,
        ));

        triples.push(RdfTriple::new(
            &step_uri,
            &format!("{}startedAtTime", PROV_NS),
            &step.started_at.to_rfc3339(),
        ));

        // Process based on action type
        match &step.action_type {
            ActionType::Transform { modifications } => {
                let transform_triples = self
                    .generate_transform_lineage(&step_uri, entity_id, modifications)
                    .await?;
                triples.extend(transform_triples);
            }
            ActionType::MLPredict { prediction } => {
                let predict_triples = self
                    .generate_prediction_lineage(&step_uri, entity_id, prediction)
                    .await?;
                triples.extend(predict_triples);
            }
            ActionType::Validate { rules_applied } => {
                let validate_triples = self
                    .generate_validation_lineage(&step_uri, entity_id, rules_applied)
                    .await?;
                triples.extend(validate_triples);
            }
        }

        Ok(triples)
    }

    /// Generate field-level transformation lineage
    async fn generate_transform_lineage(
        &self,
        step_uri: &str,
        entity_id: &str,
        modifications: &[FieldModification],
    ) -> Result<Vec<RdfTriple>> {
        let mut triples = Vec::new();
        let transform_uri = format!("{}/transform/{}", WORKFLOW_NS, Uuid::new_v4());

        // Transform action
        triples.push(RdfTriple::new(
            &transform_uri,
            &format!("{}type", RDF_NS),
            &format!("{}TransformAction", WORKFLOW_NS),
        ));

        triples.push(RdfTriple::new(
            &transform_uri,
            &format!("{}wasPartOf", PROV_NS),
            step_uri,
        ));

        // Track each field modification with before/after values
        for modification in modifications {
            let mod_uri = format!("{}/field_mod/{}", WORKFLOW_NS, Uuid::new_v4());

            triples.push(RdfTriple::new(
                &mod_uri,
                &format!("{}type", RDF_NS),
                &format!("{}FieldModification", WORKFLOW_NS),
            ));

            triples.push(RdfTriple::new(
                &mod_uri,
                &format!("{}fieldName", WORKFLOW_NS),
                &modification.field_name,
            ));

            triples.push(RdfTriple::new(
                &mod_uri,
                &format!("{}oldValue", WORKFLOW_NS),
                &self.serialize_value(&modification.old_value),
            ));

            triples.push(RdfTriple::new(
                &mod_uri,
                &format!("{}newValue", WORKFLOW_NS),
                &self.serialize_value(&modification.new_value),
            ));

            triples.push(RdfTriple::new(
                &mod_uri,
                &format!("{}fieldConfidence", WORKFLOW_NS),
                &modification.confidence.to_string(),
            ));

            // Link modification to transform
            triples.push(RdfTriple::new(
                &transform_uri,
                &format!("{}modifiedField", WORKFLOW_NS),
                &mod_uri,
            ));

            // Timestamp for time-travel queries
            triples.push(RdfTriple::new(
                &mod_uri,
                &format!("{}atTime", PROV_NS),
                &Utc::now().to_rfc3339(),
            ));

            // Store reversal information if applicable
            if modification.is_reversible {
                triples.push(RdfTriple::new(
                    &transform_uri,
                    &format!("{}isReversible", WORKFLOW_NS),
                    "true",
                ));

                triples.push(RdfTriple::new(
                    &transform_uri,
                    &format!("{}reversalData", WORKFLOW_NS),
                    &base64::encode(serde_json::to_vec(&modification.old_value)?),
                ));
            }
        }

        // Link entity to transformation
        let entity_uri = format!("{}/entity/{}", GRAPHICA_NS, entity_id);
        triples.push(RdfTriple::new(
            &entity_uri,
            &format!("{}wasInfluencedBy", PROV_NS),
            &transform_uri,
        ));

        Ok(triples)
    }

    /// Generate ML prediction lineage (integrates with existing model service)
    async fn generate_prediction_lineage(
        &self,
        step_uri: &str,
        entity_id: &str,
        prediction: &ModelPrediction,
    ) -> Result<Vec<RdfTriple>> {
        let mut triples = Vec::new();

        // NOTE: The actual prediction triples are created by the Model Service
        // via POST /api/v1/models/{id}/predictions
        // Here we only add workflow-specific lineage

        // Link the prediction to the workflow step
        let attr_uri = format!("{}/attr/{}", GRAPHICA_NS, prediction.attribute_id);

        triples.push(RdfTriple::new(
            &attr_uri,
            &format!("{}wasPartOf", PROV_NS),
            step_uri,
        ));

        // Add workflow context to prediction
        triples.push(RdfTriple::new(
            &attr_uri,
            &format!("{}workflowContext", WORKFLOW_NS),
            step_uri,
        ));

        // Track confidence propagation
        triples.push(RdfTriple::new(
            step_uri,
            &format!("{}predictionConfidence", WORKFLOW_NS),
            &prediction.confidence.to_string(),
        ));

        Ok(triples)
    }

    /// Generate validation lineage
    async fn generate_validation_lineage(
        &self,
        step_uri: &str,
        entity_id: &str,
        rules_applied: &[RuleApplication],
    ) -> Result<Vec<RdfTriple>> {
        let mut triples = Vec::new();

        for rule in rules_applied {
            let rule_uri = format!("{}/rule_app/{}", WORKFLOW_NS, Uuid::new_v4());

            triples.push(RdfTriple::new(
                &rule_uri,
                &format!("{}type", RDF_NS),
                &format!("{}RuleApplication", WORKFLOW_NS),
            ));

            triples.push(RdfTriple::new(
                &rule_uri,
                &format!("{}wasPartOf", PROV_NS),
                step_uri,
            ));

            triples.push(RdfTriple::new(
                &rule_uri,
                &format!("{}ruleId", WORKFLOW_NS),
                &rule.rule_id,
            ));

            triples.push(RdfTriple::new(
                &rule_uri,
                &format!("{}passed", WORKFLOW_NS),
                &rule.passed.to_string(),
            ));

            if !rule.passed {
                triples.push(RdfTriple::new(
                    &rule_uri,
                    &format!("{}violationReason", WORKFLOW_NS),
                    &rule.violation_reason,
                ));
            }
        }

        Ok(triples)
    }

    /// Query workflow impact using SPARQL
    pub async fn query_workflow_impact(
        &self,
        workflow_id: &str,
        start_date: Option<DateTime<Utc>>,
        end_date: Option<DateTime<Utc>>,
    ) -> Result<WorkflowImpact> {
        let workflow_uri = self.workflow_uri(workflow_id);

        let date_filter = match (start_date, end_date) {
            (Some(start), Some(end)) => format!(
                "FILTER(?timestamp >= \"{}\"^^xsd:dateTime && ?timestamp <= \"{}\"^^xsd:dateTime)",
                start.to_rfc3339(),
                end.to_rfc3339()
            ),
            (Some(start), None) => {
                format!(
                    "FILTER(?timestamp >= \"{}\"^^xsd:dateTime)",
                    start.to_rfc3339()
                )
            }
            (None, Some(end)) => {
                format!(
                    "FILTER(?timestamp <= \"{}\"^^xsd:dateTime)",
                    end.to_rfc3339()
                )
            }
            _ => String::new(),
        };

        let sparql = format!(
            r#"
            PREFIX wf: <{WORKFLOW_NS}>
            PREFIX gph: <{GRAPHICA_NS}>
            PREFIX prov: <http://www.w3.org/ns/prov#>

            SELECT
                (COUNT(DISTINCT ?entity) as ?total_entities)
                (COUNT(DISTINCT ?field_mod) as ?total_fields_modified)
                (COUNT(DISTINCT ?prediction) as ?total_predictions)
                (AVG(?confidence) as ?avg_confidence)
            WHERE {{
                ?execution wf:executedWorkflow <{workflow_uri}> ;
                          prov:endedAtTime ?timestamp ;
                          wf:outputConfidence ?confidence .

                ?entity prov:wasGeneratedBy ?execution .

                OPTIONAL {{
                    ?transform prov:wasPartOf ?step .
                    ?step wf:partOfExecution ?execution .
                    ?transform wf:modifiedField ?field_mod .
                }}

                OPTIONAL {{
                    ?prediction prov:wasPartOf ?step2 .
                    ?step2 wf:partOfExecution ?execution .
                    ?prediction a gph:DerivedAttribute .
                }}

                {date_filter}
            }}
            "#
        );

        let results = self.rdf_store.query(&sparql)?;

        // Parse SPARQL results
        Ok(WorkflowImpact {
            total_entities: self.extract_count(&results, "total_entities"),
            total_fields_modified: self.extract_count(&results, "total_fields_modified"),
            total_predictions: self.extract_count(&results, "total_predictions"),
            average_confidence: self.extract_double(&results, "avg_confidence"),
        })
    }

    /// Find all transformations for a specific field
    pub async fn query_field_history(
        &self,
        entity_id: &str,
        field_name: &str,
    ) -> Result<Vec<FieldHistory>> {
        let entity_uri = format!("{}/entity/{}", GRAPHICA_NS, entity_id);

        let sparql = format!(
            r#"
            PREFIX wf: <{WORKFLOW_NS}>
            PREFIX prov: <http://www.w3.org/ns/prov#>

            SELECT ?timestamp ?old_value ?new_value ?confidence ?workflow ?transform_id
            WHERE {{
                ?modification wf:fieldName "{field_name}" ;
                             wf:oldValue ?old_value ;
                             wf:newValue ?new_value ;
                             wf:fieldConfidence ?confidence ;
                             prov:atTime ?timestamp .

                ?transform wf:modifiedField ?modification .

                ?step prov:generated ?transform .
                ?execution wf:hasStepExecution ?step ;
                          wf:executedWorkflow ?workflow .

                <{entity_uri}> prov:wasInfluencedBy ?transform .

                BIND(STR(?transform) AS ?transform_id)
            }}
            ORDER BY ?timestamp
            "#
        );

        let results = self.rdf_store.query(&sparql)?;

        // Parse results into FieldHistory structs
        let mut history = Vec::new();
        for binding in results {
            history.push(FieldHistory {
                timestamp: self.parse_datetime(&binding["timestamp"])?,
                old_value: self.parse_json_value(&binding["old_value"]),
                new_value: self.parse_json_value(&binding["new_value"]),
                confidence: self.extract_double_from_binding(&binding, "confidence"),
                workflow_id: self.extract_uri_id(&binding["workflow"]),
                transform_id: self.extract_uri_id(&binding["transform_id"]),
            });
        }

        Ok(history)
    }

    // Helper methods

    fn execution_uri(&self, id: &str) -> String {
        format!("{}/execution/{}", WORKFLOW_NS, id)
    }

    fn workflow_uri(&self, id: &str) -> String {
        format!("{}/workflow/{}", WORKFLOW_NS, id)
    }

    fn step_execution_uri(&self, id: &str) -> String {
        format!("{}/step_exec/{}", WORKFLOW_NS, id)
    }

    fn serialize_value(&self, value: &JsonValue) -> String {
        serde_json::to_string(value).unwrap_or_else(|_| "null".to_string())
    }

    fn calculate_lineage_depth(&self, execution: &WorkflowExecution) -> usize {
        // Count the maximum depth of transformations + predictions
        execution.steps.len()
    }

    fn extract_count(&self, results: &[JsonValue], field: &str) -> usize {
        results
            .first()
            .and_then(|r| r.get(field))
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize
    }

    fn extract_double(&self, results: &[JsonValue], field: &str) -> f64 {
        results
            .first()
            .and_then(|r| r.get(field))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0)
    }

    fn parse_datetime(&self, value: &JsonValue) -> Result<DateTime<Utc>> {
        let date_str = value
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid datetime"))?;
        DateTime::parse_from_rfc3339(date_str)
            .map(|dt| dt.with_timezone(&Utc))
            .context("Failed to parse datetime")
    }

    fn parse_json_value(&self, value: &JsonValue) -> JsonValue {
        if let Some(s) = value.as_str() {
            serde_json::from_str(s).unwrap_or(JsonValue::String(s.to_string()))
        } else {
            value.clone()
        }
    }

    fn extract_double_from_binding(&self, binding: &JsonValue, field: &str) -> f64 {
        binding.get(field).and_then(|v| v.as_f64()).unwrap_or(0.0)
    }

    fn extract_uri_id(&self, value: &JsonValue) -> String {
        value
            .as_str()
            .and_then(|s| s.split('/').last())
            .unwrap_or("")
            .to_string()
    }

    /// Record a custom lineage event with arbitrary metadata
    ///
    /// Used by the RecordLineage action to capture workflow-specific lineage events
    /// that don't fit the standard patterns.
    pub fn record_custom_event(
        &self,
        execution_id: &str,
        workflow_id: &str,
        route_id: &str,
        event_type: &str,
        metadata: &JsonValue,
    ) -> Result<()> {
        let exec_uri = self.execution_uri(execution_id);
        let event_id = Uuid::new_v4().to_string();
        let event_uri = format!("{}/event/{}", exec_uri, event_id);

        let mut triples = vec![
            RdfTriple::new(
                &event_uri,
                &format!("{}type", RDF_NS),
                &format!("{}Activity", PROV_NS),
            ),
            RdfTriple::new_literal(&event_uri, &format!("{}eventType", WORKFLOW_NS), event_type),
            RdfTriple::new(&event_uri, &format!("{}wasPartOf", PROV_NS), &exec_uri),
            RdfTriple::new_literal(
                &event_uri,
                &format!("{}workflowId", WORKFLOW_NS),
                workflow_id,
            ),
            RdfTriple::new_literal(&event_uri, &format!("{}routeId", WORKFLOW_NS), route_id),
            RdfTriple::new_typed(
                &event_uri,
                &format!("{}recordedAt", PROV_NS),
                &chrono::Utc::now().to_rfc3339(),
                "xsd:dateTime",
            ),
        ];

        // Add metadata as JSON literal
        if !metadata.is_null() && metadata != &serde_json::json!({}) {
            let metadata_json =
                serde_json::to_string(metadata).context("Failed to serialize lineage metadata")?;
            triples.push(RdfTriple::new_literal(
                &event_uri,
                &format!("{}metadata", WORKFLOW_NS),
                &metadata_json,
            ));
        }

        self.rdf_store
            .insert_batch(&triples, Some(&NamedGraph::workflow_executions()))?;

        tracing::debug!(
            "Recorded custom lineage event: {} (type: {})",
            event_id,
            event_type
        );

        Ok(())
    }

    /// Record Kafka message delivery (Phase 1.3)
    ///
    /// Captures lineage for data sent to Kafka topics, including delivery metadata.
    pub fn record_kafka_delivery(
        &self,
        execution_id: &str,
        workflow_id: &str,
        route_id: &str,
        topic: &str,
        partition: i32,
        offset: i64,
        latency_ms: u64,
    ) -> Result<()> {
        let exec_uri = self.execution_uri(execution_id);
        let delivery_id = Uuid::new_v4().to_string();
        let delivery_uri = format!("{}/kafka/{}", exec_uri, delivery_id);

        let triples = vec![
            RdfTriple::new(
                &delivery_uri,
                &format!("{}type", RDF_NS),
                &format!("{}Activity", PROV_NS),
            ),
            RdfTriple::new_literal(
                &delivery_uri,
                &format!("{}activityType", WORKFLOW_NS),
                "KafkaDelivery",
            ),
            RdfTriple::new(&delivery_uri, &format!("{}wasPartOf", PROV_NS), &exec_uri),
            RdfTriple::new_literal(
                &delivery_uri,
                &format!("{}workflowId", WORKFLOW_NS),
                workflow_id,
            ),
            RdfTriple::new_literal(&delivery_uri, &format!("{}routeId", WORKFLOW_NS), route_id),
            RdfTriple::new_literal(&delivery_uri, &format!("{}topic", WORKFLOW_NS), topic),
            RdfTriple::new_typed(
                &delivery_uri,
                &format!("{}partition", WORKFLOW_NS),
                &partition.to_string(),
                "xsd:integer",
            ),
            RdfTriple::new_typed(
                &delivery_uri,
                &format!("{}offset", WORKFLOW_NS),
                &offset.to_string(),
                "xsd:long",
            ),
            RdfTriple::new_typed(
                &delivery_uri,
                &format!("{}latencyMs", WORKFLOW_NS),
                &latency_ms.to_string(),
                "xsd:long",
            ),
            RdfTriple::new_typed(
                &delivery_uri,
                &format!("{}deliveredAt", PROV_NS),
                &chrono::Utc::now().to_rfc3339(),
                "xsd:dateTime",
            ),
        ];

        self.rdf_store
            .insert_batch(&triples, Some(&NamedGraph::workflow_executions()))?;

        tracing::debug!(
            "Recorded Kafka delivery lineage: topic={}, partition={}, offset={}",
            topic,
            partition,
            offset
        );

        Ok(())
    }

    /// Record HTTP API call (Phase 1.3)
    ///
    /// Captures lineage for data sent to external HTTP endpoints.
    pub fn record_http_call(
        &self,
        execution_id: &str,
        workflow_id: &str,
        route_id: &str,
        url: &str,
        method: &str,
        status_code: u16,
        latency_ms: u64,
        retries: u32,
    ) -> Result<()> {
        let exec_uri = self.execution_uri(execution_id);
        let call_id = Uuid::new_v4().to_string();
        let call_uri = format!("{}/http/{}", exec_uri, call_id);

        let triples = vec![
            RdfTriple::new(
                &call_uri,
                &format!("{}type", RDF_NS),
                &format!("{}Activity", PROV_NS),
            ),
            RdfTriple::new_literal(
                &call_uri,
                &format!("{}activityType", WORKFLOW_NS),
                "HttpCall",
            ),
            RdfTriple::new(&call_uri, &format!("{}wasPartOf", PROV_NS), &exec_uri),
            RdfTriple::new_literal(
                &call_uri,
                &format!("{}workflowId", WORKFLOW_NS),
                workflow_id,
            ),
            RdfTriple::new_literal(&call_uri, &format!("{}routeId", WORKFLOW_NS), route_id),
            RdfTriple::new_literal(&call_uri, &format!("{}url", WORKFLOW_NS), url),
            RdfTriple::new_literal(&call_uri, &format!("{}method", WORKFLOW_NS), method),
            RdfTriple::new_typed(
                &call_uri,
                &format!("{}statusCode", WORKFLOW_NS),
                &status_code.to_string(),
                "xsd:integer",
            ),
            RdfTriple::new_typed(
                &call_uri,
                &format!("{}latencyMs", WORKFLOW_NS),
                &latency_ms.to_string(),
                "xsd:long",
            ),
            RdfTriple::new_typed(
                &call_uri,
                &format!("{}retries", WORKFLOW_NS),
                &retries.to_string(),
                "xsd:integer",
            ),
            RdfTriple::new_typed(
                &call_uri,
                &format!("{}calledAt", PROV_NS),
                &chrono::Utc::now().to_rfc3339(),
                "xsd:dateTime",
            ),
        ];

        self.rdf_store
            .insert_batch(&triples, Some(&NamedGraph::workflow_executions()))?;

        tracing::debug!(
            "Recorded HTTP call lineage: {} {} (status={})",
            method,
            url,
            status_code
        );

        Ok(())
    }

    /// Record action execution (Phase 2.1)
    ///
    /// Captures lineage for individual workflow actions with timing and status.
    pub fn record_action_execution(
        &self,
        execution_id: &str,
        workflow_id: &str,
        route_id: &str,
        action_type: &str,
        action_index: usize,
        status: &str,
        duration_ms: u64,
        error_message: Option<&str>,
    ) -> Result<()> {
        let exec_uri = self.execution_uri(execution_id);
        let action_id = format!("action_{}", action_index);
        let action_uri = format!("{}/action/{}", exec_uri, action_id);

        let mut triples = vec![
            RdfTriple::new(
                &action_uri,
                &format!("{}type", RDF_NS),
                &format!("{}Activity", PROV_NS),
            ),
            RdfTriple::new_literal(
                &action_uri,
                &format!("{}activityType", WORKFLOW_NS),
                action_type,
            ),
            RdfTriple::new(&action_uri, &format!("{}wasPartOf", PROV_NS), &exec_uri),
            RdfTriple::new_literal(
                &action_uri,
                &format!("{}workflowId", WORKFLOW_NS),
                workflow_id,
            ),
            RdfTriple::new_literal(&action_uri, &format!("{}routeId", WORKFLOW_NS), route_id),
            RdfTriple::new_typed(
                &action_uri,
                &format!("{}actionIndex", WORKFLOW_NS),
                &action_index.to_string(),
                "xsd:integer",
            ),
            RdfTriple::new_literal(&action_uri, &format!("{}status", WORKFLOW_NS), status),
            RdfTriple::new_typed(
                &action_uri,
                &format!("{}durationMs", WORKFLOW_NS),
                &duration_ms.to_string(),
                "xsd:long",
            ),
            RdfTriple::new_typed(
                &action_uri,
                &format!("{}executedAt", PROV_NS),
                &chrono::Utc::now().to_rfc3339(),
                "xsd:dateTime",
            ),
        ];

        // Add error message if present
        if let Some(error) = error_message {
            triples.push(RdfTriple::new_literal(
                &action_uri,
                &format!("{}errorMessage", WORKFLOW_NS),
                error,
            ));
        }

        self.rdf_store
            .insert_batch(&triples, Some(&NamedGraph::workflow_executions()))?;

        tracing::debug!(
            "Recorded action execution lineage: {} (index={}, status={}, duration={}ms)",
            action_type,
            action_index,
            status,
            duration_ms
        );

        Ok(())
    }

    /// Record route matching decision (Phase 2.3)
    ///
    /// Captures which route matched (or didn't match) and why, for audit trail.
    pub fn record_route_matching(
        &self,
        execution_id: &str,
        workflow_id: &str,
        matched_route_id: Option<&str>,
        matched_route_name: Option<&str>,
        evaluation_time_ms: u64,
        routes_evaluated: usize,
        match_reason: &str,
    ) -> Result<()> {
        let exec_uri = self.execution_uri(execution_id);
        let routing_id = Uuid::new_v4().to_string();
        let routing_uri = format!("{}/routing/{}", exec_uri, routing_id);

        let mut triples = vec![
            RdfTriple::new(
                &routing_uri,
                &format!("{}type", RDF_NS),
                &format!("{}Activity", PROV_NS),
            ),
            RdfTriple::new_literal(
                &routing_uri,
                &format!("{}activityType", WORKFLOW_NS),
                "RouteMatching",
            ),
            RdfTriple::new(&routing_uri, &format!("{}wasPartOf", PROV_NS), &exec_uri),
            RdfTriple::new_literal(
                &routing_uri,
                &format!("{}workflowId", WORKFLOW_NS),
                workflow_id,
            ),
            RdfTriple::new_typed(
                &routing_uri,
                &format!("{}evaluationTimeMs", WORKFLOW_NS),
                &evaluation_time_ms.to_string(),
                "xsd:long",
            ),
            RdfTriple::new_typed(
                &routing_uri,
                &format!("{}routesEvaluated", WORKFLOW_NS),
                &routes_evaluated.to_string(),
                "xsd:integer",
            ),
            RdfTriple::new_literal(
                &routing_uri,
                &format!("{}matchReason", WORKFLOW_NS),
                match_reason,
            ),
            RdfTriple::new_typed(
                &routing_uri,
                &format!("{}evaluatedAt", PROV_NS),
                &chrono::Utc::now().to_rfc3339(),
                "xsd:dateTime",
            ),
        ];

        // Add matched route info if present
        if let Some(route_id) = matched_route_id {
            triples.push(RdfTriple::new_literal(
                &routing_uri,
                &format!("{}matchedRouteId", WORKFLOW_NS),
                route_id,
            ));
        }
        if let Some(route_name) = matched_route_name {
            triples.push(RdfTriple::new_literal(
                &routing_uri,
                &format!("{}matchedRouteName", WORKFLOW_NS),
                route_name,
            ));
        }

        self.rdf_store
            .insert_batch(&triples, Some(&NamedGraph::workflow_executions()))?;

        tracing::debug!(
            "Recorded route matching lineage: matched={}, reason={}, evaluation_time={}ms",
            matched_route_id.unwrap_or("none"),
            match_reason,
            evaluation_time_ms
        );

        Ok(())
    }

    /// Record data quality validation lineage (Phase 2.2)
    ///
    /// Captures data quality rule execution results including:
    /// - Rule ID and type
    /// - Validation result (passed/failed)
    /// - Confidence score
    /// - Violations detected (if any)
    /// - Fields validated
    ///
    /// # Arguments
    /// * `execution_id` - Unique execution identifier
    /// * `workflow_id` - Workflow identifier
    /// * `route_id` - Route identifier
    /// * `action_index` - Action index in the route
    /// * `rule_id` - Quality rule identifier
    /// * `rule_type` - Type of quality rule (completeness, validity, uniqueness, etc.)
    /// * `passed` - Whether validation passed
    /// * `confidence` - Confidence score (0.0 to 1.0)
    /// * `violation_count` - Number of violations detected
    /// * `fields_validated` - Fields that were validated
    /// * `error_message` - Error message if validation failed
    pub fn record_quality_validation(
        &self,
        execution_id: &str,
        workflow_id: &str,
        route_id: &str,
        action_index: usize,
        rule_id: &str,
        rule_type: &str,
        passed: bool,
        confidence: f64,
        violation_count: u32,
        fields_validated: Vec<String>,
        error_message: Option<&str>,
    ) -> Result<()> {
        let exec_uri = format!("{}{}", WORKFLOW_NS, execution_id);
        let route_uri = format!("{}/route/{}", exec_uri, route_id);
        let validation_uri = format!("{}/quality/validation_{}", exec_uri, Uuid::new_v4());
        let action_uri = format!("{}/action/action_{}", exec_uri, action_index);

        let now = Utc::now().to_rfc3339();

        let mut triples = vec![
            // Basic validation activity
            RdfTriple::new(
                &validation_uri,
                &format!("{}type", RDF_NS),
                &format!("{}Activity", PROV_NS),
            ),
            RdfTriple::new_literal(
                &validation_uri,
                &format!("{}activityType", WORKFLOW_NS),
                "QualityValidation",
            ),
            // Link to workflow execution hierarchy
            RdfTriple::new(
                &validation_uri,
                &format!("{}wasPartOf", PROV_NS),
                &action_uri,
            ),
            // Rule metadata
            RdfTriple::new_literal(&validation_uri, &format!("{}ruleId", WORKFLOW_NS), rule_id),
            RdfTriple::new_literal(
                &validation_uri,
                &format!("{}ruleType", WORKFLOW_NS),
                rule_type,
            ),
            // Validation results
            RdfTriple::new_typed(
                &validation_uri,
                &format!("{}passed", WORKFLOW_NS),
                &passed.to_string(),
                "xsd:boolean",
            ),
            RdfTriple::new_typed(
                &validation_uri,
                &format!("{}confidence", WORKFLOW_NS),
                &confidence.to_string(),
                "xsd:double",
            ),
            RdfTriple::new_typed(
                &validation_uri,
                &format!("{}violationCount", WORKFLOW_NS),
                &violation_count.to_string(),
                "xsd:integer",
            ),
            // Timestamp
            RdfTriple::new_typed(
                &validation_uri,
                &format!("{}atTime", PROV_NS),
                &now,
                "xsd:dateTime",
            ),
        ];

        // Add fields validated (as separate triples for SPARQL queries)
        for field in fields_validated {
            triples.push(RdfTriple::new_literal(
                &validation_uri,
                &format!("{}validatedField", WORKFLOW_NS),
                &field,
            ));
        }

        // Add error message if validation failed
        if let Some(error) = error_message {
            triples.push(RdfTriple::new_literal(
                &validation_uri,
                &format!("{}errorMessage", WORKFLOW_NS),
                error,
            ));
        }

        self.rdf_store
            .insert_batch(&triples, Some(&NamedGraph::workflow_executions()))?;

        tracing::debug!(
            "Recorded quality validation lineage: rule={}, type={}, passed={}, confidence={}, violations={}",
            rule_id,
            rule_type,
            passed,
            confidence,
            violation_count
        );

        Ok(())
    }

    /// Record field-level transformation lineage (Phase 3.2)
    ///
    /// Captures individual field modifications during Transform actions including:
    /// - Field name
    /// - Old and new values
    /// - Transformation type (e.g., rename, map, format, derive)
    /// - Confidence score
    /// - Reversibility flag
    ///
    /// # Arguments
    /// * `execution_id` - Unique execution identifier
    /// * `workflow_id` - Workflow identifier
    /// * `route_id` - Route identifier
    /// * `action_index` - Action index in the route
    /// * `transformer_name` - Name of the transformer applied
    /// * `field_modifications` - List of field-level changes
    pub fn record_field_transformations(
        &self,
        execution_id: &str,
        workflow_id: &str,
        route_id: &str,
        action_index: usize,
        transformer_name: &str,
        field_modifications: Vec<FieldModification>,
    ) -> Result<()> {
        if field_modifications.is_empty() {
            return Ok(());
        }

        let exec_uri = format!("{}{}", WORKFLOW_NS, execution_id);
        let action_uri = format!("{}/action/action_{}", exec_uri, action_index);

        let now = Utc::now().to_rfc3339();

        let mut triples = Vec::new();

        // Create triples for each field modification
        for (idx, modification) in field_modifications.iter().enumerate() {
            let field_uri = format!("{}/field/{}", action_uri, idx);

            triples.push(RdfTriple::new(
                &field_uri,
                &format!("{}type", RDF_NS),
                &format!("{}Activity", PROV_NS),
            ));
            triples.push(RdfTriple::new_literal(
                &field_uri,
                &format!("{}activityType", WORKFLOW_NS),
                "FieldTransformation",
            ));

            // Link to parent action
            triples.push(RdfTriple::new(
                &field_uri,
                &format!("{}wasPartOf", PROV_NS),
                &action_uri,
            ));

            // Field identification
            triples.push(RdfTriple::new_literal(
                &field_uri,
                &format!("{}fieldName", WORKFLOW_NS),
                &modification.field_name,
            ));
            triples.push(RdfTriple::new_literal(
                &field_uri,
                &format!("{}transformerName", WORKFLOW_NS),
                transformer_name,
            ));

            // Value changes (as JSON strings for complex types)
            let old_value_str = serde_json::to_string(&modification.old_value)
                .unwrap_or_else(|_| "null".to_string());
            let new_value_str = serde_json::to_string(&modification.new_value)
                .unwrap_or_else(|_| "null".to_string());

            triples.push(RdfTriple::new_literal(
                &field_uri,
                &format!("{}oldValue", WORKFLOW_NS),
                &old_value_str,
            ));
            triples.push(RdfTriple::new_literal(
                &field_uri,
                &format!("{}newValue", WORKFLOW_NS),
                &new_value_str,
            ));

            // Metadata
            triples.push(RdfTriple::new_typed(
                &field_uri,
                &format!("{}confidence", WORKFLOW_NS),
                &modification.confidence.to_string(),
                "xsd:double",
            ));
            triples.push(RdfTriple::new_typed(
                &field_uri,
                &format!("{}isReversible", WORKFLOW_NS),
                &modification.is_reversible.to_string(),
                "xsd:boolean",
            ));

            // Timestamp
            triples.push(RdfTriple::new_typed(
                &field_uri,
                &format!("{}atTime", PROV_NS),
                &now,
                "xsd:dateTime",
            ));

            // Determine transformation type from value comparison
            let transform_type = Self::infer_transformation_type(
                &modification.field_name,
                &modification.old_value,
                &modification.new_value,
            );
            triples.push(RdfTriple::new_literal(
                &field_uri,
                &format!("{}transformationType", WORKFLOW_NS),
                &transform_type,
            ));
        }

        self.rdf_store
            .insert_batch(&triples, Some(&NamedGraph::workflow_executions()))?;

        tracing::debug!(
            "Recorded field-level transformation lineage: transformer={}, fields_modified={}",
            transformer_name,
            field_modifications.len()
        );

        Ok(())
    }

    /// Infer transformation type from field name and value changes
    fn infer_transformation_type(
        field_name: &str,
        old_value: &JsonValue,
        new_value: &JsonValue,
    ) -> String {
        // Null to value = derive
        if old_value.is_null() && !new_value.is_null() {
            return "derive".to_string();
        }

        // Value to null = remove
        if !old_value.is_null() && new_value.is_null() {
            return "remove".to_string();
        }

        // Type change = cast
        let old_type = Self::json_type(old_value);
        let new_type = Self::json_type(new_value);
        if old_type != new_type {
            return format!("cast_{}_{}", old_type, new_type);
        }

        // String transformations
        if old_value.is_string() && new_value.is_string() {
            let old_str = old_value.as_str().unwrap_or("");
            let new_str = new_value.as_str().unwrap_or("");

            // Format detection (common patterns)
            if old_str.to_lowercase() != old_str && new_str == old_str.to_lowercase() {
                return "format_lowercase".to_string();
            }
            if old_str.to_uppercase() != old_str && new_str == old_str.to_uppercase() {
                return "format_uppercase".to_string();
            }
            if old_str.trim() != old_str && new_str == old_str.trim() {
                return "format_trim".to_string();
            }

            return "map_value".to_string();
        }

        // Number transformations
        if old_value.is_number() && new_value.is_number() {
            return "map_value".to_string();
        }

        // Default: generic mapping
        "map_value".to_string()
    }

    /// Get JSON value type as string
    fn json_type(value: &JsonValue) -> &str {
        match value {
            JsonValue::Null => "null",
            JsonValue::Bool(_) => "boolean",
            JsonValue::Number(_) => "number",
            JsonValue::String(_) => "string",
            JsonValue::Array(_) => "array",
            JsonValue::Object(_) => "object",
        }
    }
}

// Domain types

#[derive(Debug, Clone)]
pub struct WorkflowExecution {
    pub id: String,
    pub workflow_id: String,
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
    pub input_confidence: f64,
    pub output_confidence: f64,
    pub steps: Vec<StepExecution>,
}

#[derive(Debug, Clone)]
pub struct StepExecution {
    pub id: String,
    pub step_type: String,
    pub started_at: DateTime<Utc>,
    pub action_type: ActionType,
}

#[derive(Debug, Clone)]
pub enum ActionType {
    Transform {
        modifications: Vec<FieldModification>,
    },
    MLPredict {
        prediction: ModelPrediction,
    },
    Validate {
        rules_applied: Vec<RuleApplication>,
    },
}

#[derive(Debug, Clone)]
pub struct FieldModification {
    pub field_name: String,
    pub old_value: JsonValue,
    pub new_value: JsonValue,
    pub confidence: f64,
    pub is_reversible: bool,
}

#[derive(Debug, Clone)]
pub struct ModelPrediction {
    pub attribute_id: String,
    pub attribute_name: String,
    pub value: JsonValue,
    pub confidence: f64,
    pub model_id: String,
    pub model_version: String,
}

#[derive(Debug, Clone)]
pub struct RuleApplication {
    pub rule_id: String,
    pub passed: bool,
    pub violation_reason: String,
}

#[derive(Debug)]
pub struct WorkflowLineageResult {
    pub execution_uri: String,
    pub triples_generated: usize,
    pub field_modifications: Vec<FieldModification>,
    pub predictions: Vec<ModelPrediction>,
    pub lineage_depth: usize,
}

#[derive(Debug)]
pub struct WorkflowImpact {
    pub total_entities: usize,
    pub total_fields_modified: usize,
    pub total_predictions: usize,
    pub average_confidence: f64,
}

#[derive(Debug)]
pub struct FieldHistory {
    pub timestamp: DateTime<Utc>,
    pub old_value: JsonValue,
    pub new_value: JsonValue,
    pub confidence: f64,
    pub workflow_id: String,
    pub transform_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_workflow_lineage_generation() {
        let rdf_store = Arc::new(GraphicaRdfStore::new_in_memory().unwrap());
        let generator = WorkflowLineageGenerator::new(rdf_store);

        let execution = WorkflowExecution {
            id: "exec_123".to_string(),
            workflow_id: "wf_customer_enrichment".to_string(),
            started_at: Utc::now(),
            completed_at: Utc::now(),
            input_confidence: 0.95,
            output_confidence: 0.82,
            steps: vec![StepExecution {
                id: "step_001".to_string(),
                step_type: "transform".to_string(),
                started_at: Utc::now(),
                action_type: ActionType::Transform {
                    modifications: vec![FieldModification {
                        field_name: "email".to_string(),
                        old_value: json!("John.Doe@Example.com"),
                        new_value: json!("john.doe@example.com"),
                        confidence: 1.0,
                        is_reversible: true,
                    }],
                },
            }],
        };

        let result = generator
            .generate_execution_lineage(&execution, "cust_456")
            .await
            .unwrap();

        assert!(result.triples_generated > 10);
        assert_eq!(result.field_modifications.len(), 1);
        assert_eq!(result.lineage_depth, 1);
    }
}
