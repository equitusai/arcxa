//! RDF persistence for workflow execution results
//!
//! Converts workflow results to RDF triples using W3C PROV ontology
//! and stores them in the governance brain's RDF store.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;

use super::ontology::{PROV_NS, WORKFLOW_NS};
use super::rdf_store::{GraphicaRdfStore, NamedGraph, RdfStore};
use graphica_core::orchestration::workflow::executor::{FinalDecision, StepResult, WorkflowResult};

const RDF_NS: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
const XSD_NS: &str = "http://www.w3.org/2001/XMLSchema#";

/// Workflow result persistence layer
pub struct WorkflowResultPersistence {
    rdf_store: Arc<GraphicaRdfStore>,
}

impl WorkflowResultPersistence {
    /// Create new persistence layer with RDF store
    pub fn new(rdf_store: Arc<GraphicaRdfStore>) -> Self {
        Self { rdf_store }
    }

    /// Persist workflow definition to RDF store
    pub async fn persist_workflow_definition(
        &self,
        workflow_id: &str,
        name: &str,
        version: &str,
    ) -> Result<()> {
        let workflow_uri = self.workflow_uri(workflow_id);
        let graph = Some(NamedGraph::new(format!("{}workflows", WORKFLOW_NS)));

        // Type declaration
        self.rdf_store.insert_triple(
            &workflow_uri,
            &format!("{}type", RDF_NS),
            &format!("{}Workflow", WORKFLOW_NS),
            graph.as_ref(),
        )?;

        // Workflow metadata
        self.rdf_store.insert_triple(
            &workflow_uri,
            &format!("{}workflowId", WORKFLOW_NS),
            &format!("\"{}\"", workflow_id),
            graph.as_ref(),
        )?;

        self.rdf_store.insert_triple(
            &workflow_uri,
            &format!("{}workflowName", WORKFLOW_NS),
            &format!("\"{}\"", name),
            graph.as_ref(),
        )?;

        self.rdf_store.insert_triple(
            &workflow_uri,
            &format!("{}version", WORKFLOW_NS),
            &format!("\"{}\"", version),
            graph.as_ref(),
        )?;

        Ok(())
    }

    /// Persist workflow result to RDF store
    pub async fn persist_result(&self, workflow_id: &str, result: &WorkflowResult) -> Result<()> {
        // Create URIs
        let execution_uri = self.execution_uri(&result.execution_id);
        let workflow_uri = self.workflow_uri(workflow_id);
        let graph = Some(NamedGraph::new(format!("{}executions", WORKFLOW_NS)));

        // Collect all triples for batch insert
        let mut all_triples = Vec::new();

        // Type declarations
        all_triples.push((
            execution_uri.clone(),
            format!("{}type", RDF_NS),
            format!("{}Execution", WORKFLOW_NS),
        ));

        all_triples.push((
            execution_uri.clone(),
            format!("{}type", RDF_NS),
            format!("{}Activity", PROV_NS),
        ));

        // Link to workflow definition
        all_triples.push((
            execution_uri.clone(),
            format!("{}executedWorkflow", WORKFLOW_NS),
            workflow_uri.clone(),
        ));

        // Execution metadata
        all_triples.push((
            execution_uri.clone(),
            format!("{}startedAtTime", PROV_NS),
            format!(
                "\"{}\"^^<{}dateTime>",
                result.started_at.to_rfc3339(),
                XSD_NS
            ),
        ));

        all_triples.push((
            execution_uri.clone(),
            format!("{}endedAtTime", PROV_NS),
            format!(
                "\"{}\"^^<{}dateTime>",
                result.completed_at.to_rfc3339(),
                XSD_NS
            ),
        ));

        all_triples.push((
            execution_uri.clone(),
            format!("{}success", WORKFLOW_NS),
            format!("\"{}\"^^<{}boolean>", result.success, XSD_NS),
        ));

        all_triples.push((
            execution_uri.clone(),
            format!("{}confidence", WORKFLOW_NS),
            format!("\"{}\"^^<{}double>", result.confidence, XSD_NS),
        ));

        let decision_str = match result.final_decision {
            FinalDecision::Accept => "accept",
            FinalDecision::Reject => "reject",
            FinalDecision::ManualReview => "manual_review",
        };

        all_triples.push((
            execution_uri.clone(),
            format!("{}finalDecision", WORKFLOW_NS),
            format!("\"{}\"", decision_str),
        ));

        if let Some(error) = &result.error {
            let escaped_error = error.replace('\\', "\\\\").replace('"', "\\\"");
            all_triples.push((
                execution_uri.clone(),
                format!("{}error", WORKFLOW_NS),
                format!("\"{}\"", escaped_error),
            ));
        }

        // Persist step results
        for (step_id, step_result) in &result.step_results {
            let step_triples = self.step_result_to_triples(&execution_uri, step_id, step_result)?;
            all_triples.extend(step_triples);
        }

        // Batch insert all triples
        self.rdf_store.insert_triples(all_triples, graph.as_ref())?;

        Ok(())
    }

    /// Convert step result to RDF triples
    fn step_result_to_triples(
        &self,
        execution_uri: &str,
        step_id: &str,
        step_result: &StepResult,
    ) -> Result<Vec<(String, String, String)>> {
        let mut triples = Vec::new();
        let step_uri = self.step_execution_uri(&step_result.step_id);

        // Type: step execution
        triples.push((
            step_uri.clone(),
            format!("{}type", RDF_NS),
            format!("{}StepExecution", WORKFLOW_NS),
        ));

        // Also mark as PROV Activity
        triples.push((
            step_uri.clone(),
            format!("{}type", RDF_NS),
            format!("{}Activity", PROV_NS),
        ));

        // Link to parent execution
        triples.push((
            step_uri.clone(),
            format!("{}partOfExecution", WORKFLOW_NS),
            execution_uri.to_string(),
        ));

        // Step metadata
        triples.push((
            step_uri.clone(),
            format!("{}stepId", WORKFLOW_NS),
            format!("\"{}\"", step_id),
        ));

        triples.push((
            step_uri.clone(),
            format!("{}success", WORKFLOW_NS),
            format!("\"{}\"^^<{}boolean>", step_result.success, XSD_NS),
        ));

        triples.push((
            step_uri.clone(),
            format!("{}confidence", WORKFLOW_NS),
            format!("\"{}\"^^<{}double>", step_result.confidence, XSD_NS),
        ));

        triples.push((
            step_uri.clone(),
            format!("{}startedAtTime", PROV_NS),
            format!(
                "\"{}\"^^<{}dateTime>",
                step_result.started_at.to_rfc3339(),
                XSD_NS
            ),
        ));

        triples.push((
            step_uri.clone(),
            format!("{}endedAtTime", PROV_NS),
            format!(
                "\"{}\"^^<{}dateTime>",
                step_result.completed_at.to_rfc3339(),
                XSD_NS
            ),
        ));

        // Store output as JSON string
        let output_json = serde_json::to_string(&step_result.output)
            .context("Failed to serialize step output")?;
        let escaped_output = output_json.replace('\\', "\\\\").replace('"', "\\\"");
        triples.push((
            step_uri.clone(),
            format!("{}output", WORKFLOW_NS),
            format!("\"{}\"", escaped_output),
        ));

        Ok(triples)
    }

    /// Query workflow execution history
    pub async fn query_execution_history(
        &self,
        workflow_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<ExecutionSummary>> {
        let workflow_uri = self.workflow_uri(workflow_id);

        let sparql = format!(
            r#"
            PREFIX workflow: <{WORKFLOW_NS}>
            PREFIX prov: <{PROV_NS}>

            SELECT ?execution ?success ?confidence ?started ?ended
            WHERE {{
                ?execution workflow:executedWorkflow <{workflow_uri}> ;
                          workflow:success ?success ;
                          workflow:confidence ?confidence ;
                          prov:startedAtTime ?started ;
                          prov:endedAtTime ?ended .
            }}
            ORDER BY DESC(?started)
            {limit_clause}
            "#,
            limit_clause = limit.map(|l| format!("LIMIT {}", l)).unwrap_or_default()
        );

        // Execute SPARQL query
        let results = self.rdf_store.query(&sparql)?;

        // Parse results into ExecutionSummary
        let mut summaries = Vec::new();
        for result_json in results {
            if let Some(obj) = result_json.as_object() {
                if let (
                    Some(exec_uri),
                    Some(success),
                    Some(confidence),
                    Some(started),
                    Some(ended),
                ) = (
                    obj.get("execution").and_then(|v| v.as_str()),
                    obj.get("success").and_then(|v| v.as_bool()),
                    obj.get("confidence").and_then(|v| v.as_f64()),
                    obj.get("started").and_then(|v| v.as_str()),
                    obj.get("ended").and_then(|v| v.as_str()),
                ) {
                    let execution_id = exec_uri
                        .trim_start_matches(&format!("{}execution/", WORKFLOW_NS))
                        .to_string();
                    let started_at = DateTime::parse_from_rfc3339(started)?.with_timezone(&Utc);
                    let completed_at = DateTime::parse_from_rfc3339(ended)?.with_timezone(&Utc);

                    summaries.push(ExecutionSummary {
                        execution_id,
                        workflow_id: workflow_id.to_string(),
                        success,
                        confidence,
                        started_at,
                        completed_at,
                    });
                }
            }
        }

        Ok(summaries)
    }

    /// Get detailed execution result by ID
    pub async fn get_execution_details(
        &self,
        execution_id: &str,
    ) -> Result<Option<serde_json::Value>> {
        let execution_uri = self.execution_uri(execution_id);

        let sparql = format!(
            r#"
            PREFIX workflow: <{WORKFLOW_NS}>
            PREFIX prov: <{PROV_NS}>

            SELECT ?p ?o
            WHERE {{
                <{execution_uri}> ?p ?o .
            }}
            "#
        );

        let results = self.rdf_store.query(&sparql)?;

        if results.is_empty() {
            return Ok(None);
        }

        // Convert SPARQL results to execution details JSON
        let mut details = serde_json::json!({
            "execution_id": execution_id,
            "execution_uri": execution_uri,
            "properties": {}
        });

        for result in results {
            if let (Some(p), Some(o)) = (result.get("p").and_then(|v| v.as_str()), result.get("o"))
            {
                details["properties"][p] = o.clone();
            }
        }

        Ok(Some(details))
    }

    // Helper methods for URI construction

    fn execution_uri(&self, execution_id: &str) -> String {
        format!("{}execution/{}", WORKFLOW_NS, execution_id)
    }

    fn workflow_uri(&self, workflow_id: &str) -> String {
        format!("{}workflow/{}", WORKFLOW_NS, workflow_id)
    }

    fn step_execution_uri(&self, step_id: &str) -> String {
        format!("{}step/{}", WORKFLOW_NS, step_id)
    }
}

/// Execution summary for query results
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExecutionSummary {
    pub execution_id: String,
    pub workflow_id: String,
    pub success: bool,
    pub confidence: f64,
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uri_construction() {
        let persistence = WorkflowResultPersistence {
            rdf_store: Arc::new(GraphicaRdfStore::default()),
        };

        let execution_uri = persistence.execution_uri("exec_123");
        assert_eq!(
            execution_uri,
            "http://graphica.io/workflow#execution/exec_123"
        );

        let workflow_uri = persistence.workflow_uri("wf_456");
        assert_eq!(workflow_uri, "http://graphica.io/workflow#workflow/wf_456");
    }
}
