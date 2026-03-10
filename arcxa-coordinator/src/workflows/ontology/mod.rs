//! Workflow Execution RDF Ontology
//!
//! Converts workflow executions to W3C PROV-based RDF triples for storage in shard.
//! Also includes data transformation and relationship resolution for ontology-driven ETL.

pub mod cache;
pub mod data_transformer;
pub mod ddl_generator;
pub mod loader;
pub mod normalization;
pub mod relationship_resolver;
pub mod schema_provider;
pub mod type_mapper;
pub mod types;

pub use cache::*;
pub use data_transformer::*;
pub use ddl_generator::*;
pub use loader::*;
pub use normalization::*;
pub use relationship_resolver::*;
pub use schema_provider::*;
pub use type_mapper::*;
pub use types::*;

use crate::workflows::domain::{ExecutionLog, ExecutionStatus, WorkflowExecution};
use anyhow::Result;

/// RDF namespaces
pub mod ns {
    pub const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
    pub const RDFS: &str = "http://www.w3.org/2000/01/rdf-schema#";
    pub const XSD: &str = "http://www.w3.org/2001/XMLSchema#";
    pub const PROV: &str = "http://www.w3.org/ns/prov#";
    pub const WF: &str = "http://graphica.io/ontology/workflow#";
    pub const GPH: &str = "http://graphica.io/ontology#";
}

/// RDF triple representation
#[derive(Debug, Clone)]
pub struct Triple {
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub object_type: ObjectType,
}

/// Type of RDF object
#[derive(Debug, Clone)]
pub enum ObjectType {
    Uri,
    Literal(Option<String>), // Optional datatype
    TypedLiteral(String),    // With datatype
}

impl Triple {
    /// Create a URI triple
    pub fn uri(
        subject: impl Into<String>,
        predicate: impl Into<String>,
        object: impl Into<String>,
    ) -> Self {
        Self {
            subject: subject.into(),
            predicate: predicate.into(),
            object: object.into(),
            object_type: ObjectType::Uri,
        }
    }

    /// Create a literal triple
    pub fn literal(
        subject: impl Into<String>,
        predicate: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        Self {
            subject: subject.into(),
            predicate: predicate.into(),
            object: value.into(),
            object_type: ObjectType::Literal(None),
        }
    }

    /// Create a typed literal triple
    pub fn typed_literal(
        subject: impl Into<String>,
        predicate: impl Into<String>,
        value: impl Into<String>,
        datatype: impl Into<String>,
    ) -> Self {
        Self {
            subject: subject.into(),
            predicate: predicate.into(),
            object: value.into(),
            object_type: ObjectType::TypedLiteral(datatype.into()),
        }
    }

    /// Convert to Turtle format
    pub fn to_turtle(&self) -> String {
        let obj = match &self.object_type {
            ObjectType::Uri => format!("<{}>", self.object),
            ObjectType::Literal(None) => format!("\"{}\"", self.object.replace('\"', "\\\"")),
            ObjectType::Literal(Some(lang)) => {
                format!("\"{}\"@{}", self.object.replace('\"', "\\\""), lang)
            }
            ObjectType::TypedLiteral(dtype) => {
                format!("\"{}\"^^<{}>", self.object.replace('\"', "\\\""), dtype)
            }
        };

        format!("<{}> <{}> {} .", self.subject, self.predicate, obj)
    }
}

/// Convert WorkflowExecution to RDF triples
pub fn execution_to_triples(execution: &WorkflowExecution) -> Result<Vec<Triple>> {
    let mut triples = Vec::new();

    // Execution URI
    let exec_uri = format!("{}/execution/{}", ns::GPH, execution.execution_id);

    // Type declaration
    triples.push(Triple::uri(
        &exec_uri,
        format!("{}type", ns::RDF),
        format!("{}WorkflowExecution", ns::WF),
    ));

    // Also subclass of prov:Activity
    triples.push(Triple::uri(
        &exec_uri,
        format!("{}type", ns::RDF),
        format!("{}Activity", ns::PROV),
    ));

    // Basic properties
    triples.push(Triple::literal(
        &exec_uri,
        format!("{}executionId", ns::WF),
        &execution.execution_id,
    ));
    triples.push(Triple::literal(
        &exec_uri,
        format!("{}workflowId", ns::WF),
        &execution.workflow_id,
    ));
    triples.push(Triple::literal(
        &exec_uri,
        format!("{}workflowName", ns::WF),
        &execution.workflow_name,
    ));

    // Status
    let status_str = match &execution.status {
        ExecutionStatus::Pending => "Pending",
        ExecutionStatus::Running => "Running",
        ExecutionStatus::Paused => "Paused",
        ExecutionStatus::Completed => "Completed",
        ExecutionStatus::Failed => "Failed",
        ExecutionStatus::Stopped => "Stopped",
        ExecutionStatus::Aborted => "Aborted",
    };
    triples.push(Triple::literal(
        &exec_uri,
        format!("{}status", ns::WF),
        status_str,
    ));

    // Timestamps
    let started_at = execution.started_at.to_rfc3339();
    triples.push(Triple::typed_literal(
        &exec_uri,
        format!("{}startedAt", ns::WF),
        &started_at,
        format!("{}dateTime", ns::XSD),
    ));

    // PROV mapping
    triples.push(Triple::typed_literal(
        &exec_uri,
        format!("{}startedAtTime", ns::PROV),
        &started_at,
        format!("{}dateTime", ns::XSD),
    ));

    if let Some(completed_at) = execution.completed_at {
        let completed_str = completed_at.to_rfc3339();
        triples.push(Triple::typed_literal(
            &exec_uri,
            format!("{}completedAt", ns::WF),
            &completed_str,
            format!("{}dateTime", ns::XSD),
        ));

        triples.push(Triple::typed_literal(
            &exec_uri,
            format!("{}endedAtTime", ns::PROV),
            &completed_str,
            format!("{}dateTime", ns::XSD),
        ));

        // Duration
        let duration = completed_at.signed_duration_since(execution.started_at);
        triples.push(Triple::typed_literal(
            &exec_uri,
            format!("{}duration", ns::WF),
            duration.num_milliseconds().to_string(),
            format!("{}integer", ns::XSD),
        ));
    }

    // Triggered by (user/system)
    if let Some(ref triggered_by) = execution.triggered_by {
        triples.push(Triple::literal(
            &exec_uri,
            format!("{}triggeredBy", ns::WF),
            triggered_by,
        ));

        // PROV agent association
        let agent_uri = format!("{}/agent/{}", ns::GPH, triggered_by);
        triples.push(Triple::uri(
            &exec_uri,
            format!("{}wasAssociatedWith", ns::PROV),
            &agent_uri,
        ));
    }

    // Input (JSON serialized)
    let input_json = serde_json::to_string(&execution.input)?;
    triples.push(Triple::literal(
        &exec_uri,
        format!("{}hasInput", ns::WF),
        input_json,
    ));

    // Output (JSON serialized)
    if let Some(ref output) = execution.output {
        let output_json = serde_json::to_string(output)?;
        triples.push(Triple::literal(
            &exec_uri,
            format!("{}hasOutput", ns::WF),
            output_json,
        ));
    }

    // Error
    if let Some(ref error) = execution.error {
        triples.push(Triple::literal(
            &exec_uri,
            format!("{}hasError", ns::WF),
            error,
        ));
    }

    // Logs
    for (idx, log) in execution.logs.iter().enumerate() {
        let log_uri = format!("{}/log/{}", exec_uri, idx);
        triples.push(Triple::uri(
            &exec_uri,
            format!("{}hasLog", ns::WF),
            &log_uri,
        ));
        triples.push(Triple::uri(
            &log_uri,
            format!("{}type", ns::RDF),
            format!("{}LogEntry", ns::WF),
        ));

        triples.push(Triple::literal(
            &log_uri,
            format!("{}logMessage", ns::WF),
            &log.message,
        ));

        let level_str = format!("{:?}", log.level);
        triples.push(Triple::literal(
            &log_uri,
            format!("{}logLevel", ns::WF),
            level_str,
        ));

        let timestamp_str = log.timestamp.to_rfc3339();
        triples.push(Triple::typed_literal(
            &log_uri,
            format!("{}logTimestamp", ns::WF),
            timestamp_str,
            format!("{}dateTime", ns::XSD),
        ));
    }

    Ok(triples)
}

/// Convert triples to Turtle format
pub fn triples_to_turtle(triples: &[Triple]) -> String {
    let mut turtle = String::new();

    // Add prefixes
    turtle.push_str(&format!("@prefix rdf: <{}> .\n", ns::RDF));
    turtle.push_str(&format!("@prefix rdfs: <{}> .\n", ns::RDFS));
    turtle.push_str(&format!("@prefix xsd: <{}> .\n", ns::XSD));
    turtle.push_str(&format!("@prefix prov: <{}> .\n", ns::PROV));
    turtle.push_str(&format!("@prefix wf: <{}> .\n", ns::WF));
    turtle.push_str(&format!("@prefix gph: <{}> .\n\n", ns::GPH));

    // Add triples
    for triple in triples {
        turtle.push_str(&triple.to_turtle());
        turtle.push('\n');
    }

    turtle
}

/// SPARQL query templates
pub mod sparql {
    /// Query all in-progress executions
    pub const QUERY_IN_PROGRESS: &str = r#"
PREFIX wf: <http://graphica.io/ontology/workflow#>
PREFIX prov: <http://www.w3.org/ns/prov#>

SELECT ?execution ?executionId ?workflowId ?status ?startedAt
WHERE {
    ?execution a wf:WorkflowExecution ;
               wf:executionId ?executionId ;
               wf:workflowId ?workflowId ;
               wf:status ?status ;
               wf:startedAt ?startedAt .
    FILTER (?status = "Running" || ?status = "Paused")
}
ORDER BY DESC(?startedAt)
"#;

    /// Query execution history for a workflow
    pub const QUERY_WORKFLOW_HISTORY: &str = r#"
PREFIX wf: <http://graphica.io/ontology/workflow#>

SELECT ?execution ?executionId ?status ?startedAt ?completedAt ?duration
WHERE {
    ?execution a wf:WorkflowExecution ;
               wf:executionId ?executionId ;
               wf:workflowId ?workflowId ;
               wf:status ?status ;
               wf:startedAt ?startedAt .
    OPTIONAL { ?execution wf:completedAt ?completedAt }
    OPTIONAL { ?execution wf:duration ?duration }
    FILTER (?workflowId = ?1)
}
ORDER BY DESC(?startedAt)
LIMIT ?2
"#;

    /// Query checkpoints for an execution
    pub const QUERY_CHECKPOINTS: &str = r#"
PREFIX wf: <http://graphica.io/ontology/workflow#>
PREFIX prov: <http://www.w3.org/ns/prov#>

SELECT ?checkpoint ?sequence ?createdAt ?checksum
WHERE {
    ?execution wf:executionId ?executionId ;
               wf:hasCheckpoint ?checkpoint .
    ?checkpoint wf:checkpointSequence ?sequence ;
                wf:checkpointCreatedAt ?createdAt ;
                wf:stateChecksum ?checksum .
    FILTER (?executionId = ?1)
}
ORDER BY DESC(?sequence)
"#;

    /// Query execution lineage (PROV-based)
    pub const QUERY_LINEAGE: &str = r#"
PREFIX prov: <http://www.w3.org/ns/prov#>
PREFIX wf: <http://graphica.io/ontology/workflow#>

SELECT ?entity ?relationship ?relatedEntity
WHERE {
    ?execution wf:executionId ?executionId .
    {
        ?execution prov:used ?entity .
        BIND("used" AS ?relationship)
        BIND(?entity AS ?relatedEntity)
    } UNION {
        ?entity prov:wasGeneratedBy ?execution .
        BIND("generated" AS ?relationship)
        BIND(?entity AS ?relatedEntity)
    } UNION {
        ?execution prov:wasAssociatedWith ?agent .
        BIND("associatedWith" AS ?relationship)
        BIND(?agent AS ?relatedEntity)
    }
    FILTER (?executionId = ?1)
}
"#;

    /// Query failed executions in time range
    pub const QUERY_FAILED_EXECUTIONS: &str = r#"
PREFIX wf: <http://graphica.io/ontology/workflow#>
PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>

SELECT ?execution ?executionId ?workflowName ?error ?startedAt
WHERE {
    ?execution a wf:WorkflowExecution ;
               wf:executionId ?executionId ;
               wf:workflowName ?workflowName ;
               wf:status "Failed" ;
               wf:startedAt ?startedAt ;
               wf:hasError ?error .
    FILTER (?startedAt >= ?1 && ?startedAt <= ?2)
}
ORDER BY DESC(?startedAt)
"#;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflows::domain::LogLevel;
    use serde_json::json;

    fn create_test_execution() -> WorkflowExecution {
        WorkflowExecution::new(
            "exec_001".to_string(),
            "wf_001".to_string(),
            "Test Workflow".to_string(),
            json!({"input": "test"}),
            Some("test@example.com".to_string()),
        )
    }

    #[test]
    fn test_execution_to_triples() {
        let execution = create_test_execution();
        let triples = execution_to_triples(&execution).unwrap();

        assert!(triples.len() > 5);

        // Check for required triples
        let has_type = triples.iter().any(|t| {
            t.predicate == format!("{}type", ns::RDF)
                && t.object == format!("{}WorkflowExecution", ns::WF)
        });
        assert!(has_type, "Should have WorkflowExecution type triple");

        let has_exec_id = triples
            .iter()
            .any(|t| t.predicate == format!("{}executionId", ns::WF) && t.object == "exec_001");
        assert!(has_exec_id, "Should have executionId triple");
    }

    #[test]
    fn test_triple_to_turtle() {
        let triple = Triple::uri(
            "http://example.org/subject",
            "http://example.org/predicate",
            "http://example.org/object",
        );

        let turtle = triple.to_turtle();
        assert!(turtle.contains("<http://example.org/subject>"));
        assert!(turtle.contains("<http://example.org/predicate>"));
        assert!(turtle.contains("<http://example.org/object>"));
    }

    #[test]
    fn test_typed_literal_triple() {
        let triple = Triple::typed_literal(
            "http://example.org/subject",
            "http://example.org/date",
            "2024-10-25T10:00:00Z",
            format!("{}dateTime", ns::XSD),
        );

        let turtle = triple.to_turtle();
        assert!(turtle.contains("\"2024-10-25T10:00:00Z\""));
        assert!(turtle.contains("^^<http://www.w3.org/2001/XMLSchema#dateTime>"));
    }

    #[test]
    fn test_full_turtle_generation() {
        let mut execution = create_test_execution();
        execution.add_log(ExecutionLog {
            timestamp: chrono::Utc::now(),
            level: LogLevel::Info,
            message: "Test log".to_string(),
            step_id: None,
            details: None,
        });

        let triples = execution_to_triples(&execution).unwrap();
        let turtle = triples_to_turtle(&triples);

        // Should have prefixes
        assert!(turtle.contains("@prefix wf:"));
        assert!(turtle.contains("@prefix prov:"));

        // Should have execution data
        assert!(turtle.contains("WorkflowExecution"));
        assert!(turtle.contains("exec_001"));
        assert!(turtle.contains("Test Workflow"));
    }
}
