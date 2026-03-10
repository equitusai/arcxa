//! Workflow Execution Ontology Extension
//!
//! Extends the Graphica ontology with workflow execution concepts,
//! enabling SPARQL queries over workflow state and lineage.

use crate::governance::ontology::{GRAPHICA_NS, PROV_NS, RDFS_NS, RDF_NS, WORKFLOW_NS, XSD_NS};

/// Workflow ontology definitions as Turtle
pub fn workflow_ontology() -> String {
    format!(
        r#"
@prefix workflow: <{WORKFLOW_NS}> .
@prefix gph: <{GRAPHICA_NS}> .
@prefix prov: <{PROV_NS}> .
@prefix rdf: <{RDF_NS}> .
@prefix rdfs: <{RDFS_NS}> .
@prefix xsd: <{XSD_NS}> .

# ============================================================================
# WORKFLOW EXECUTION CLASSES
# ============================================================================

# WorkflowExecution - Instance of a workflow run
workflow:Execution a rdfs:Class ;
    rdfs:subClassOf prov:Activity ;
    rdfs:label "Workflow Execution" ;
    rdfs:comment "An instance of a workflow execution with state and lineage" .

# WorkflowDefinition - The template/definition
workflow:Definition a rdfs:Class ;
    rdfs:subClassOf prov:Plan ;
    rdfs:label "Workflow Definition" ;
    rdfs:comment "Template defining workflow steps, routes, and actions" .

# ExecutionCheckpoint - State snapshot for recovery
workflow:Checkpoint a rdfs:Class ;
    rdfs:label "Execution Checkpoint" ;
    rdfs:comment "Snapshot of execution state for recovery and auditing" .

# WorkflowStep - Individual step in execution
workflow:Step a rdfs:Class ;
    rdfs:subClassOf prov:Activity ;
    rdfs:label "Workflow Step" ;
    rdfs:comment "Individual step within a workflow execution" .

# BatchJob - Coordinated execution of multiple workflows
workflow:BatchJob a rdfs:Class ;
    rdfs:subClassOf prov:Activity ;
    rdfs:label "Batch Job" ;
    rdfs:comment "Coordinates multiple workflow executions as a unit" .

# ExecutionLog - Log entry for execution
workflow:LogEntry a rdfs:Class ;
    rdfs:label "Execution Log Entry" ;
    rdfs:comment "Log message from workflow execution" .

# ============================================================================
# EXECUTION PROPERTIES
# ============================================================================

# Basic execution properties
workflow:executionId a rdf:Property ;
    rdfs:domain workflow:Execution ;
    rdfs:range xsd:string ;
    rdfs:label "Execution ID" ;
    rdfs:comment "Unique identifier for the execution" .

workflow:workflowId a rdf:Property ;
    rdfs:domain workflow:Execution ;
    rdfs:range xsd:string ;
    rdfs:label "Workflow ID" ;
    rdfs:comment "ID of the workflow definition being executed" .

workflow:status a rdf:Property ;
    rdfs:domain workflow:Execution ;
    rdfs:range xsd:string ;
    rdfs:label "Execution Status" ;
    rdfs:comment "Current status: Pending, Running, Completed, Failed, etc." .

workflow:startedAt a rdf:Property ;
    rdfs:domain workflow:Execution ;
    rdfs:range xsd:dateTime ;
    rdfs:label "Started At" ;
    rdfs:comment "Timestamp when execution started" .

workflow:completedAt a rdf:Property ;
    rdfs:domain workflow:Execution ;
    rdfs:range xsd:dateTime ;
    rdfs:label "Completed At" ;
    rdfs:comment "Timestamp when execution completed" .

workflow:triggeredBy a rdf:Property ;
    rdfs:domain workflow:Execution ;
    rdfs:range xsd:string ;
    rdfs:label "Triggered By" ;
    rdfs:comment "User or system that triggered the execution" .

# Relationships
workflow:hasCheckpoint a rdf:Property ;
    rdfs:domain workflow:Execution ;
    rdfs:range workflow:Checkpoint ;
    rdfs:label "Has Checkpoint" ;
    rdfs:comment "Links execution to its checkpoints" .

workflow:hasStep a rdf:Property ;
    rdfs:domain workflow:Execution ;
    rdfs:range workflow:Step ;
    rdfs:label "Has Step" ;
    rdfs:comment "Links execution to its steps" .

workflow:partOfBatch a rdf:Property ;
    rdfs:domain workflow:Execution ;
    rdfs:range workflow:BatchJob ;
    rdfs:label "Part of Batch" ;
    rdfs:comment "Links execution to parent batch job" .

workflow:hasLog a rdf:Property ;
    rdfs:domain workflow:Execution ;
    rdfs:range workflow:LogEntry ;
    rdfs:label "Has Log" ;
    rdfs:comment "Links execution to log entries" .

# ============================================================================
# CHECKPOINT PROPERTIES
# ============================================================================

workflow:checkpointId a rdf:Property ;
    rdfs:domain workflow:Checkpoint ;
    rdfs:range xsd:string ;
    rdfs:label "Checkpoint ID" ;
    rdfs:comment "Unique identifier for checkpoint" .

workflow:sequence a rdf:Property ;
    rdfs:domain workflow:Checkpoint ;
    rdfs:range xsd:integer ;
    rdfs:label "Sequence Number" ;
    rdfs:comment "Monotonic sequence number of checkpoint" .

workflow:stepNumber a rdf:Property ;
    rdfs:domain workflow:Checkpoint ;
    rdfs:range xsd:integer ;
    rdfs:label "Step Number" ;
    rdfs:comment "Number of completed steps at checkpoint" .

workflow:compressedState a rdf:Property ;
    rdfs:domain workflow:Checkpoint ;
    rdfs:range xsd:base64Binary ;
    rdfs:label "Compressed State" ;
    rdfs:comment "Compressed execution state snapshot" .

workflow:stateChecksum a rdf:Property ;
    rdfs:domain workflow:Checkpoint ;
    rdfs:range xsd:string ;
    rdfs:label "State Checksum" ;
    rdfs:comment "SHA-256 hash of uncompressed state" .

workflow:createdAt a rdf:Property ;
    rdfs:domain workflow:Checkpoint ;
    rdfs:range xsd:dateTime ;
    rdfs:label "Created At" ;
    rdfs:comment "Timestamp of checkpoint creation" .

workflow:previousCheckpoint a rdf:Property ;
    rdfs:domain workflow:Checkpoint ;
    rdfs:range workflow:Checkpoint ;
    rdfs:label "Previous Checkpoint" ;
    rdfs:comment "Link to previous checkpoint in chain" .

# ============================================================================
# STEP PROPERTIES
# ============================================================================

workflow:stepId a rdf:Property ;
    rdfs:domain workflow:Step ;
    rdfs:range xsd:string ;
    rdfs:label "Step ID" ;
    rdfs:comment "Unique identifier for step" .

workflow:stepName a rdf:Property ;
    rdfs:domain workflow:Step ;
    rdfs:range xsd:string ;
    rdfs:label "Step Name" ;
    rdfs:comment "Human-readable step name" .

workflow:stepType a rdf:Property ;
    rdfs:domain workflow:Step ;
    rdfs:range xsd:string ;
    rdfs:label "Step Type" ;
    rdfs:comment "Type of step: Action, Condition, Route, etc." .

workflow:stepStatus a rdf:Property ;
    rdfs:domain workflow:Step ;
    rdfs:range xsd:string ;
    rdfs:label "Step Status" ;
    rdfs:comment "Status: Pending, Running, Completed, Failed" .

workflow:stepStartedAt a rdf:Property ;
    rdfs:domain workflow:Step ;
    rdfs:range xsd:dateTime ;
    rdfs:label "Step Started At" ;
    rdfs:comment "When step started executing" .

workflow:stepCompletedAt a rdf:Property ;
    rdfs:domain workflow:Step ;
    rdfs:range xsd:dateTime ;
    rdfs:label "Step Completed At" ;
    rdfs:comment "When step completed" .

workflow:stepOutput a rdf:Property ;
    rdfs:domain workflow:Step ;
    rdfs:range xsd:string ;
    rdfs:label "Step Output" ;
    rdfs:comment "JSON output from step execution" .

workflow:stepError a rdf:Property ;
    rdfs:domain workflow:Step ;
    rdfs:range xsd:string ;
    rdfs:label "Step Error" ;
    rdfs:comment "Error message if step failed" .

# Step relationships
workflow:follows a rdf:Property ;
    rdfs:domain workflow:Step ;
    rdfs:range workflow:Step ;
    rdfs:label "Follows" ;
    rdfs:comment "This step follows another step" .

workflow:dependsOn a rdf:Property ;
    rdfs:domain workflow:Step ;
    rdfs:range workflow:Step ;
    rdfs:label "Depends On" ;
    rdfs:comment "This step depends on completion of another" .

# ============================================================================
# BATCH JOB PROPERTIES
# ============================================================================

workflow:batchJobId a rdf:Property ;
    rdfs:domain workflow:BatchJob ;
    rdfs:range xsd:string ;
    rdfs:label "Batch Job ID" ;
    rdfs:comment "Unique identifier for batch job" .

workflow:batchJobName a rdf:Property ;
    rdfs:domain workflow:BatchJob ;
    rdfs:range xsd:string ;
    rdfs:label "Batch Job Name" ;
    rdfs:comment "Human-readable batch job name" .

workflow:totalExecutions a rdf:Property ;
    rdfs:domain workflow:BatchJob ;
    rdfs:range xsd:integer ;
    rdfs:label "Total Executions" ;
    rdfs:comment "Total number of executions in batch" .

workflow:completedExecutions a rdf:Property ;
    rdfs:domain workflow:BatchJob ;
    rdfs:range xsd:integer ;
    rdfs:label "Completed Executions" ;
    rdfs:comment "Number of completed executions" .

workflow:failedExecutions a rdf:Property ;
    rdfs:domain workflow:BatchJob ;
    rdfs:range xsd:integer ;
    rdfs:label "Failed Executions" ;
    rdfs:comment "Number of failed executions" .

workflow:batchStatus a rdf:Property ;
    rdfs:domain workflow:BatchJob ;
    rdfs:range xsd:string ;
    rdfs:label "Batch Status" ;
    rdfs:comment "Overall batch status" .

# ============================================================================
# LOG ENTRY PROPERTIES
# ============================================================================

workflow:logLevel a rdf:Property ;
    rdfs:domain workflow:LogEntry ;
    rdfs:range xsd:string ;
    rdfs:label "Log Level" ;
    rdfs:comment "Log level: DEBUG, INFO, WARN, ERROR" .

workflow:logMessage a rdf:Property ;
    rdfs:domain workflow:LogEntry ;
    rdfs:range xsd:string ;
    rdfs:label "Log Message" ;
    rdfs:comment "Log message text" .

workflow:logTimestamp a rdf:Property ;
    rdfs:domain workflow:LogEntry ;
    rdfs:range xsd:dateTime ;
    rdfs:label "Log Timestamp" ;
    rdfs:comment "When log entry was created" .

workflow:logStepId a rdf:Property ;
    rdfs:domain workflow:LogEntry ;
    rdfs:range xsd:string ;
    rdfs:label "Log Step ID" ;
    rdfs:comment "Step that generated the log" .

# ============================================================================
# PROVENANCE EXTENSIONS (W3C PROV)
# ============================================================================

# Link execution to data it used
workflow:usedDataset a rdf:Property ;
    rdfs:subPropertyOf prov:used ;
    rdfs:domain workflow:Execution ;
    rdfs:range gph:Dataset ;
    rdfs:label "Used Dataset" ;
    rdfs:comment "Dataset consumed by this execution" .

# Link execution to data it generated
workflow:generatedDataset a rdf:Property ;
    rdfs:subPropertyOf prov:generated ;
    rdfs:domain workflow:Execution ;
    rdfs:range gph:Dataset ;
    rdfs:label "Generated Dataset" ;
    rdfs:comment "Dataset produced by this execution" .

# Link execution to models it used
workflow:usedModel a rdf:Property ;
    rdfs:subPropertyOf prov:used ;
    rdfs:domain workflow:Execution ;
    rdfs:range ml:Model ;
    rdfs:label "Used Model" ;
    rdfs:comment "ML model used during execution" .

# Attribution
workflow:executedBy a rdf:Property ;
    rdfs:subPropertyOf prov:wasAttributedTo ;
    rdfs:domain workflow:Execution ;
    rdfs:range prov:Agent ;
    rdfs:label "Executed By" ;
    rdfs:comment "Agent (user/system) that executed workflow" .

# Derivation chain
workflow:derivedFromExecution a rdf:Property ;
    rdfs:subPropertyOf prov:wasDerivedFrom ;
    rdfs:domain workflow:Execution ;
    rdfs:range workflow:Execution ;
    rdfs:label "Derived From Execution" ;
    rdfs:comment "This execution was derived from another" .

# ============================================================================
# EXAMPLE RDF INSTANCE
# ============================================================================

# Example execution with checkpoint
workflow:exec_abc123 a workflow:Execution ;
    workflow:executionId "exec_abc123" ;
    workflow:workflowId "wf_csv_import" ;
    workflow:status "Running" ;
    workflow:startedAt "2024-10-25T10:00:00Z"^^xsd:dateTime ;
    workflow:triggeredBy "user@example.com" ;
    workflow:hasCheckpoint workflow:checkpoint_001 ;
    workflow:hasStep workflow:step_001, workflow:step_002 ;
    workflow:usedDataset gph:dataset_customers ;
    prov:wasStartedBy <http://graphica.io/users/alice> .

workflow:checkpoint_001 a workflow:Checkpoint ;
    workflow:checkpointId "checkpoint_001" ;
    workflow:sequence 5 ;
    workflow:stepNumber 42 ;
    workflow:createdAt "2024-10-25T10:30:00Z"^^xsd:dateTime ;
    workflow:stateChecksum "sha256:abcd1234..." ;
    workflow:previousCheckpoint workflow:checkpoint_000 .

workflow:step_001 a workflow:Step ;
    workflow:stepId "step_001" ;
    workflow:stepName "Validate CSV" ;
    workflow:stepType "Action" ;
    workflow:stepStatus "Completed" ;
    workflow:stepStartedAt "2024-10-25T10:01:00Z"^^xsd:dateTime ;
    workflow:stepCompletedAt "2024-10-25T10:02:00Z"^^xsd:dateTime .

workflow:step_002 a workflow:Step ;
    workflow:stepId "step_002" ;
    workflow:stepName "Import to Database" ;
    workflow:stepType "Action" ;
    workflow:stepStatus "Running" ;
    workflow:stepStartedAt "2024-10-25T10:02:00Z"^^xsd:dateTime ;
    workflow:follows workflow:step_001 .
"#
    )
}

/// Example SPARQL queries for workflow state
pub mod sparql_examples {
    /// Get all running executions
    pub const RUNNING_EXECUTIONS: &str = r#"
PREFIX workflow: <http://graphica.io/workflow#>
PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>

SELECT ?exec ?workflow ?started
WHERE {
    ?exec a workflow:Execution ;
          workflow:status "Running" ;
          workflow:workflowId ?workflow ;
          workflow:startedAt ?started .
}
ORDER BY DESC(?started)
LIMIT 100
"#;

    /// Get latest checkpoint for execution
    pub const LATEST_CHECKPOINT: &str = r#"
PREFIX workflow: <http://graphica.io/workflow#>

SELECT ?checkpoint ?sequence ?created
WHERE {
    <http://graphica.io/executions/exec_abc123> workflow:hasCheckpoint ?checkpoint .
    ?checkpoint workflow:sequence ?sequence ;
                workflow:createdAt ?created .
}
ORDER BY DESC(?sequence)
LIMIT 1
"#;

    /// Get execution lineage
    pub const EXECUTION_LINEAGE: &str = r#"
PREFIX workflow: <http://graphica.io/workflow#>
PREFIX prov: <http://www.w3.org/ns/prov#>
PREFIX gph: <http://graphica.io/ontology#>

CONSTRUCT {
    ?exec a workflow:Execution ;
          workflow:usedDataset ?input ;
          workflow:generatedDataset ?output ;
          workflow:usedModel ?model ;
          prov:wasStartedBy ?user .
}
WHERE {
    BIND(<http://graphica.io/executions/exec_abc123> as ?exec)

    ?exec workflow:usedDataset ?input .
    OPTIONAL { ?exec workflow:generatedDataset ?output }
    OPTIONAL { ?exec workflow:usedModel ?model }
    OPTIONAL { ?exec prov:wasStartedBy ?user }
}
"#;

    /// Get incomplete executions for recovery
    pub const INCOMPLETE_EXECUTIONS: &str = r#"
PREFIX workflow: <http://graphica.io/workflow#>
PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>

SELECT ?exec ?status ?started ?checkpoint
WHERE {
    ?exec a workflow:Execution ;
          workflow:status ?status ;
          workflow:startedAt ?started .

    FILTER (?status IN ("Pending", "Running", "Paused"))

    OPTIONAL {
        ?exec workflow:hasCheckpoint ?checkpoint .
        ?checkpoint workflow:sequence ?seq .
    }
}
ORDER BY ?started
"#;

    /// Get batch job progress
    pub const BATCH_JOB_PROGRESS: &str = r#"
PREFIX workflow: <http://graphica.io/workflow#>

SELECT ?batch ?total ?completed ?failed
WHERE {
    ?batch a workflow:BatchJob ;
           workflow:batchJobId "batch_xyz" ;
           workflow:totalExecutions ?total ;
           workflow:completedExecutions ?completed ;
           workflow:failedExecutions ?failed .
}
"#;
}
