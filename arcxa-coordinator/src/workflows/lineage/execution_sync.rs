//! Execution State to RDF Synchronization (Phase 3.1)
//!
//! Bridges the operational execution store with the RDF lineage store,
//! ensuring execution state changes are automatically reflected in the
//! W3C PROV knowledge graph for unified querying and analysis.

use anyhow::Result;
use chrono::{DateTime, Utc};
use std::sync::Arc;
use tracing::{debug, warn};
use uuid::Uuid;

use crate::governance::rdf_store::{GraphicaRdfStore, NamedGraph, RdfStore, RdfTriple};
use crate::workflows::domain::{ExecutionStatus, WorkflowExecution};

// RDF namespace prefixes
const WORKFLOW_NS: &str = "http://graphica.io/workflow#";
const PROV_NS: &str = "http://www.w3.org/ns/prov#";
const RDF_NS: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
#[allow(dead_code)]
const XSD_NS: &str = "http://www.w3.org/2001/XMLSchema#";

/// Synchronizes WorkflowExecution state to RDF triples
///
/// This bridge ensures that operational execution data is available
/// in the RDF knowledge graph for:
/// - Unified SPARQL queries across execution state and lineage
/// - Historical analysis and audit trails
/// - Cross-workflow impact analysis
/// - Compliance reporting
pub struct ExecutionStateSynchronizer {
    rdf_store: Arc<GraphicaRdfStore>,
}

impl ExecutionStateSynchronizer {
    /// Create a new execution state synchronizer
    pub fn new(rdf_store: Arc<GraphicaRdfStore>) -> Self {
        Self { rdf_store }
    }

    /// Sync a workflow execution to RDF
    ///
    /// Creates/updates RDF triples representing the execution state.
    /// This is called whenever execution state changes (status updates, completion, etc.).
    ///
    /// # Arguments
    /// * `execution` - The workflow execution to sync
    pub fn sync_execution(&self, execution: &WorkflowExecution) -> Result<()> {
        let exec_uri = format!("{}{}", WORKFLOW_NS, execution.execution_id);
        let workflow_uri = format!("{}{}", WORKFLOW_NS, execution.workflow_id);

        let mut triples = vec![
            // Basic execution entity
            RdfTriple::new(
                &exec_uri,
                &format!("{}type", RDF_NS),
                &format!("{}Activity", PROV_NS),
            ),
            RdfTriple::new_literal(
                &exec_uri,
                &format!("{}activityType", WORKFLOW_NS),
                "WorkflowExecution",
            ),
            // Link to workflow definition
            RdfTriple::new(&exec_uri, &format!("{}wasPartOf", PROV_NS), &workflow_uri),
            RdfTriple::new_literal(
                &exec_uri,
                &format!("{}workflowName", WORKFLOW_NS),
                &execution.workflow_name,
            ),
            // Execution status
            RdfTriple::new_literal(
                &exec_uri,
                &format!("{}status", WORKFLOW_NS),
                &execution.status.to_string(),
            ),
            // Timestamps
            RdfTriple::new_typed(
                &exec_uri,
                &format!("{}startedAtTime", PROV_NS),
                &execution.started_at.to_rfc3339(),
                "xsd:dateTime",
            ),
            RdfTriple::new_typed(
                &exec_uri,
                &format!("{}updatedAt", WORKFLOW_NS),
                &execution.updated_at.to_rfc3339(),
                "xsd:dateTime",
            ),
            // Execution metrics
            RdfTriple::new_typed(
                &exec_uri,
                &format!("{}actionsExecuted", WORKFLOW_NS),
                &execution.actions_executed.to_string(),
                "xsd:integer",
            ),
        ];

        // Add completed timestamp if available
        if let Some(ref completed_at) = execution.completed_at {
            triples.push(RdfTriple::new_typed(
                &exec_uri,
                &format!("{}endedAtTime", PROV_NS),
                &completed_at.to_rfc3339(),
                "xsd:dateTime",
            ));
        }

        // Add duration if available
        if let Some(duration_ms) = execution.duration_ms {
            triples.push(RdfTriple::new_typed(
                &exec_uri,
                &format!("{}durationMs", WORKFLOW_NS),
                &duration_ms.to_string(),
                "xsd:long",
            ));
        }

        // Add matched route info if available
        if let Some(ref route_id) = execution.matched_route {
            let route_uri = format!("{}/route/{}", exec_uri, route_id);
            triples.push(RdfTriple::new(
                &exec_uri,
                &format!("{}selectedRoute", WORKFLOW_NS),
                &route_uri,
            ));
        }

        if let Some(ref route_name) = execution.matched_route_name {
            triples.push(RdfTriple::new_literal(
                &exec_uri,
                &format!("{}selectedRouteName", WORKFLOW_NS),
                route_name,
            ));
        }

        // Add error message if failed
        if let Some(ref error) = execution.error {
            triples.push(RdfTriple::new_literal(
                &exec_uri,
                &format!("{}errorMessage", WORKFLOW_NS),
                error,
            ));
        }

        // Add triggered_by (user attribution)
        if let Some(ref user) = execution.triggered_by {
            let user_uri = format!("http://graphica.io/user/{}", user);
            triples.push(RdfTriple::new(
                &exec_uri,
                &format!("{}wasAssociatedWith", PROV_NS),
                &user_uri,
            ));
        }

        // Add logs as separate activities
        for (idx, log) in execution.logs.iter().enumerate() {
            let log_uri = format!("{}/log/{}", exec_uri, idx);
            triples.push(RdfTriple::new(
                &log_uri,
                &format!("{}type", RDF_NS),
                &format!("{}Activity", PROV_NS),
            ));
            triples.push(RdfTriple::new_literal(
                &log_uri,
                &format!("{}logLevel", WORKFLOW_NS),
                &log.level.to_string(),
            ));
            triples.push(RdfTriple::new_literal(
                &log_uri,
                &format!("{}message", WORKFLOW_NS),
                &log.message,
            ));
            triples.push(RdfTriple::new_typed(
                &log_uri,
                &format!("{}timestamp", WORKFLOW_NS),
                &log.timestamp.to_rfc3339(),
                "xsd:dateTime",
            ));
            triples.push(RdfTriple::new(
                &log_uri,
                &format!("{}wasPartOf", PROV_NS),
                &exec_uri,
            ));
        }

        // Insert into workflow-executions named graph
        self.rdf_store
            .insert_batch(&triples, Some(&NamedGraph::workflow_executions()))?;

        debug!(
            "Synced execution {} to RDF: status={}, actions={}",
            execution.execution_id, execution.status, execution.actions_executed
        );

        Ok(())
    }

    /// Sync execution status change to RDF
    ///
    /// Lightweight update that only modifies status-related triples.
    /// More efficient than full sync when only status changes.
    ///
    /// # Arguments
    /// * `execution_id` - Execution identifier
    /// * `status` - New status
    /// * `updated_at` - Status update timestamp
    pub fn sync_status_change(
        &self,
        execution_id: &str,
        status: ExecutionStatus,
        updated_at: DateTime<Utc>,
    ) -> Result<()> {
        let exec_uri = format!("{}{}", WORKFLOW_NS, execution_id);

        let triples = vec![
            RdfTriple::new_literal(
                &exec_uri,
                &format!("{}status", WORKFLOW_NS),
                &status.to_string(),
            ),
            RdfTriple::new_typed(
                &exec_uri,
                &format!("{}updatedAt", WORKFLOW_NS),
                &updated_at.to_rfc3339(),
                "xsd:dateTime",
            ),
        ];

        self.rdf_store
            .insert_batch(&triples, Some(&NamedGraph::workflow_executions()))?;

        debug!("Synced status change for {}: {}", execution_id, status);

        Ok(())
    }

    /// Record execution completion to RDF
    ///
    /// Updates completion timestamp and final duration.
    ///
    /// # Arguments
    /// * `execution_id` - Execution identifier
    /// * `completed_at` - Completion timestamp
    /// * `duration_ms` - Total execution duration in milliseconds
    pub fn sync_completion(
        &self,
        execution_id: &str,
        completed_at: DateTime<Utc>,
        duration_ms: u64,
    ) -> Result<()> {
        let exec_uri = format!("{}{}", WORKFLOW_NS, execution_id);

        let triples = vec![
            RdfTriple::new_typed(
                &exec_uri,
                &format!("{}endedAtTime", PROV_NS),
                &completed_at.to_rfc3339(),
                "xsd:dateTime",
            ),
            RdfTriple::new_typed(
                &exec_uri,
                &format!("{}durationMs", WORKFLOW_NS),
                &duration_ms.to_string(),
                "xsd:long",
            ),
        ];

        self.rdf_store
            .insert_batch(&triples, Some(&NamedGraph::workflow_executions()))?;

        debug!(
            "Synced completion for {}: duration={}ms",
            execution_id, duration_ms
        );

        Ok(())
    }

    /// Query executions by status via SPARQL
    ///
    /// Demonstrates unified querying across execution state and lineage.
    ///
    /// # Arguments
    /// * `status` - Status to filter by
    ///
    /// # Returns
    /// List of execution IDs matching the status
    pub fn query_by_status(&self, status: ExecutionStatus) -> Result<Vec<String>> {
        let status_literal = status.to_string();
        let sparql = format!(
            r#"
PREFIX wf: <{}>
PREFIX prov: <{}>

SELECT ?exec
WHERE {{
  ?exec a prov:Activity ;
        wf:activityType "WorkflowExecution" ;
        wf:status "{}" .
}}
"#,
            WORKFLOW_NS, PROV_NS, status
        );

        let results = self.rdf_store.query(&sparql)?;

        let execution_ids: Vec<String> = results
            .into_iter()
            .filter_map(|row| row.get("exec").and_then(|v| v.as_str()).map(String::from))
            .collect();

        if !execution_ids.is_empty() {
            return Ok(execution_ids);
        }

        // Fallback for simplified in-memory query engines used in tests:
        // filter raw triples by status + activityType and recover execution IDs.
        let raw_results = self.rdf_store.query("SELECT ?s ?p ?o WHERE { ?s ?p ?o }")?;
        let status_predicate = format!("{}status", WORKFLOW_NS);
        let activity_type_predicate = format!("{}activityType", WORKFLOW_NS);
        let mut status_matches = std::collections::HashSet::new();
        let mut activity_matches = std::collections::HashSet::new();

        for row in raw_results {
            let subject = row
                .get("subject")
                .and_then(|v| v.as_str())
                .or_else(|| row.get("s").and_then(|v| v.as_str()));
            let predicate = row
                .get("predicate")
                .and_then(|v| v.as_str())
                .or_else(|| row.get("p").and_then(|v| v.as_str()));
            let object = row
                .get("object")
                .and_then(|v| v.as_str())
                .or_else(|| row.get("o").and_then(|v| v.as_str()));

            let (Some(subject), Some(predicate), Some(object)) = (subject, predicate, object)
            else {
                continue;
            };

            let normalized_object = object.trim_matches('"');
            if predicate == status_predicate && normalized_object == status_literal {
                status_matches.insert(subject.to_string());
            } else if predicate == activity_type_predicate
                && normalized_object == "WorkflowExecution"
            {
                activity_matches.insert(subject.to_string());
            }
        }

        let mut fallback_ids: Vec<String> = status_matches
            .into_iter()
            .filter(|s| activity_matches.contains(s))
            .collect();
        fallback_ids.sort();

        Ok(fallback_ids)
    }

    /// Query execution lineage graph
    ///
    /// Retrieves complete lineage graph for an execution including:
    /// - Execution state
    /// - All actions executed
    /// - External system interactions (Kafka, HTTP)
    /// - Quality validations
    /// - Route matching decisions
    ///
    /// # Arguments
    /// * `execution_id` - Execution identifier
    ///
    /// # Returns
    /// SPARQL query results as JSON
    pub fn query_execution_lineage(&self, execution_id: &str) -> Result<Vec<serde_json::Value>> {
        let exec_uri = format!("{}{}", WORKFLOW_NS, execution_id);

        let sparql = format!(
            r#"
PREFIX wf: <{}>
PREFIX prov: <{}>

SELECT ?activity ?activityType ?timestamp ?status ?details
WHERE {{
  {{
    # Main execution
    BIND(<{}> AS ?activity)
    ?activity wf:activityType ?activityType ;
              wf:status ?status ;
              prov:startedAtTime ?timestamp .
    OPTIONAL {{ ?activity wf:errorMessage ?details }}
  }}
  UNION
  {{
    # All sub-activities (actions, validations, etc.)
    ?activity prov:wasPartOf+ <{}> ;
              wf:activityType ?activityType ;
              prov:atTime ?timestamp .
    OPTIONAL {{ ?activity wf:status ?status }}
    OPTIONAL {{ ?activity wf:errorMessage ?details }}
  }}
}}
ORDER BY ?timestamp
"#,
            WORKFLOW_NS, PROV_NS, exec_uri, exec_uri
        );

        self.rdf_store.query(&sparql)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflows::domain::ExecutionLog;
    use serde_json::json;

    #[test]
    fn test_sync_execution_basic() {
        let rdf_store = Arc::new(GraphicaRdfStore::new_in_memory().unwrap());
        let sync = ExecutionStateSynchronizer::new(rdf_store);

        let execution = WorkflowExecution::new(
            "exec_123".to_string(),
            "wf_001".to_string(),
            "Test Workflow".to_string(),
            json!({"test": "data"}),
            Some("user@example.com".to_string()),
        );

        assert!(sync.sync_execution(&execution).is_ok());
    }

    #[test]
    fn test_query_by_status() {
        let rdf_store = Arc::new(GraphicaRdfStore::new_in_memory().unwrap());
        let sync = ExecutionStateSynchronizer::new(rdf_store);

        let mut execution = WorkflowExecution::new(
            "exec_456".to_string(),
            "wf_002".to_string(),
            "Query Test".to_string(),
            json!({}),
            None,
        );
        execution.status = ExecutionStatus::Running;

        sync.sync_execution(&execution).unwrap();

        let results = sync.query_by_status(ExecutionStatus::Running).unwrap();
        assert!(results.len() >= 1);
    }
}
