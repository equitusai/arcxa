// Graphica Ontology Definitions
//
// This module defines the RDF ontology for Graphica, extending W3C PROV
// with custom classes and properties for data governance.

/// Graphica namespace
pub const GRAPHICA_NS: &str = "http://graphica.io/ontology#";

/// ML namespace
pub const ML_NS: &str = "http://graphica.io/ml#";

/// Workflow namespace
pub const WORKFLOW_NS: &str = "http://graphica.io/workflow#";

/// GDPR namespace
pub const GDPR_NS: &str = "http://graphica.io/gdpr#";

/// W3C PROV namespace
pub const PROV_NS: &str = "http://www.w3.org/ns/prov#";

/// RDF namespace
pub const RDF_NS: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";

/// RDFS namespace
pub const RDFS_NS: &str = "http://www.w3.org/2000/01/rdf-schema#";

/// XSD namespace
pub const XSD_NS: &str = "http://www.w3.org/2001/XMLSchema#";

/// Graphica Ontology
pub struct GraphicaOntology {
    pub turtle: String,
}

impl GraphicaOntology {
    /// Create ontology with all definitions
    pub fn new() -> Self {
        let turtle = format!(
            r#"
@prefix gph: <{GRAPHICA_NS}> .
@prefix ml: <{ML_NS}> .
@prefix workflow: <{WORKFLOW_NS}> .
@prefix prov: <{PROV_NS}> .
@prefix rdf: <{RDF_NS}> .
@prefix rdfs: <{RDFS_NS}> .
@prefix xsd: <{XSD_NS}> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .

# ============================================================================
# CORE CLASSES
# ============================================================================

# Entity - Domain-agnostic business entity
gph:Entity a rdfs:Class ;
    rdfs:subClassOf prov:Entity ;
    rdfs:label "Business Entity" ;
    rdfs:comment "Domain-agnostic entity representing customers, products, patients, targets, etc." .

# DerivedAttribute - ML-predicted or inferred attribute
gph:DerivedAttribute a rdfs:Class ;
    rdfs:label "Derived Attribute" ;
    rdfs:comment "Attribute derived from ML models or inference, with confidence score" .

# FusionOperation - Entity resolution/merge
gph:FusionOperation a rdfs:Class ;
    rdfs:subClassOf prov:Activity ;
    rdfs:label "Fusion Operation" ;
    rdfs:comment "Entity resolution or merge operation with reversibility support" .

# FusionCandidate - Proposed entity match for review
gph:FusionCandidate a rdfs:Class ;
    rdfs:label "Fusion Candidate" ;
    rdfs:comment "Proposed entity match awaiting human review and approval" .

# Dataset - Logical collection of records
gph:Dataset a rdfs:Class ;
    rdfs:subClassOf prov:Entity ;
    rdfs:label "Dataset" ;
    rdfs:comment "Logical collection of records with schema and lineage" .

# Dataset Column - Schema metadata for dataset columns
gph:DatasetColumn a rdfs:Class ;
    rdfs:label "Dataset Column" ;
    rdfs:comment "Column metadata for dataset schema" .

# Mapping Session - Schema-to-ontology mapping workflow
gph:MappingSession a rdfs:Class ;
    rdfs:subClassOf prov:Activity ;
    rdfs:label "Mapping Session" ;
    rdfs:comment "A mapping session tracking schema-to-ontology field mappings" .

# Field Mapping - Individual field-to-term mapping
gph:FieldMapping a rdfs:Class ;
    rdfs:label "Field Mapping" ;
    rdfs:comment "Maps a source field to an ontology term with confidence score" .

# ML Model
ml:Model a rdfs:Class ;
    rdfs:subClassOf prov:Agent ;
    rdfs:label "ML Model" ;
    rdfs:comment "External machine learning model used for predictions" .

ml:ModelVersion a rdfs:Class ;
    rdfs:label "Model Version" ;
    rdfs:comment "Specific version of a model with training metadata" .

# ============================================================================
# ENTITY PROPERTIES
# ============================================================================

gph:entityId a rdf:Property ;
    rdfs:domain gph:Entity ;
    rdfs:range xsd:string ;
    rdfs:label "Entity ID" ;
    rdfs:comment "Unique identifier for the entity" .

gph:entityType a rdf:Property ;
    rdfs:domain gph:Entity ;
    rdfs:range xsd:string ;
    rdfs:label "Entity Type" ;
    rdfs:comment "Type of entity: customer, product, patient, target, etc." .

gph:hasDerivedAttribute a rdf:Property ;
    rdfs:domain gph:Entity ;
    rdfs:range gph:DerivedAttribute ;
    rdfs:label "Has Derived Attribute" ;
    rdfs:comment "Links entity to its derived attributes" .

# ============================================================================
# DERIVED ATTRIBUTE PROPERTIES
# ============================================================================

gph:attributeName a rdf:Property ;
    rdfs:domain gph:DerivedAttribute ;
    rdfs:range xsd:string ;
    rdfs:label "Attribute Name" ;
    rdfs:comment "Name of the derived attribute" .

gph:value a rdf:Property ;
    rdfs:domain gph:DerivedAttribute ;
    rdfs:label "Attribute Value" ;
    rdfs:comment "Polymorphic value of the attribute" .

gph:confidence a rdf:Property ;
    rdfs:domain gph:DerivedAttribute ;
    rdfs:range xsd:double ;
    rdfs:label "Confidence Score" ;
    rdfs:comment "Confidence of the prediction (0.0-1.0)" .

gph:inputFeatures a rdf:Property ;
    rdfs:domain gph:DerivedAttribute ;
    rdfs:label "Input Features" ;
    rdfs:comment "Features used by model for this prediction (JSON)" .

# ============================================================================
# MODEL PROPERTIES
# ============================================================================

ml:modelName a rdf:Property ;
    rdfs:domain ml:Model ;
    rdfs:range xsd:string ;
    rdfs:label "Model Name" ;
    rdfs:comment "Human-readable name of the model" .

ml:version a rdf:Property ;
    rdfs:domain ml:Model ;
    rdfs:range xsd:string ;
    rdfs:label "Model Version" ;
    rdfs:comment "Semantic version of the model" .

ml:modelType a rdf:Property ;
    rdfs:domain ml:Model ;
    rdfs:range xsd:string ;
    rdfs:label "Model Type" ;
    rdfs:comment "Type: classification, regression, clustering, embedding" .

ml:trainingDataRef a rdf:Property ;
    rdfs:domain ml:Model ;
    rdfs:range prov:Entity ;
    rdfs:label "Training Data Reference" ;
    rdfs:comment "Reference to training dataset" .

ml:paramHash a rdf:Property ;
    rdfs:domain ml:Model ;
    rdfs:range xsd:string ;
    rdfs:label "Parameter Hash" ;
    rdfs:comment "Hash of model parameters for reproducibility" .

ml:accuracy a rdf:Property ;
    rdfs:domain ml:Model ;
    rdfs:range xsd:double ;
    rdfs:label "Model Accuracy" ;
    rdfs:comment "Overall accuracy metric" .

ml:precision a rdf:Property ;
    rdfs:domain ml:Model ;
    rdfs:range xsd:double ;
    rdfs:label "Model Precision" .

ml:recall a rdf:Property ;
    rdfs:domain ml:Model ;
    rdfs:range xsd:double ;
    rdfs:label "Model Recall" .

ml:confidenceThreshold a rdf:Property ;
    rdfs:domain ml:Model ;
    rdfs:range xsd:double ;
    rdfs:label "Confidence Threshold" ;
    rdfs:comment "Minimum confidence for accepting predictions" .

ml:endpoint a rdf:Property ;
    rdfs:domain ml:Model ;
    rdfs:range xsd:string ;
    rdfs:label "Model Endpoint" ;
    rdfs:comment "URL for model serving endpoint" .

# ============================================================================
# FUSION PROPERTIES
# ============================================================================

gph:mergedEntity a rdf:Property ;
    rdfs:domain gph:FusionOperation ;
    rdfs:range gph:Entity ;
    rdfs:label "Merged Entity" ;
    rdfs:comment "The canonical entity after fusion" .

gph:sourceEntity a rdf:Property ;
    rdfs:domain gph:FusionOperation ;
    rdfs:range gph:Entity ;
    rdfs:label "Source Entity" ;
    rdfs:comment "Entity that was merged into the canonical entity" .

gph:fusionRule a rdf:Property ;
    rdfs:domain gph:FusionOperation ;
    rdfs:range xsd:string ;
    rdfs:label "Fusion Rule" ;
    rdfs:comment "Rule ID used for entity resolution" .

gph:fusionConfidence a rdf:Property ;
    rdfs:domain gph:FusionOperation ;
    rdfs:range xsd:double ;
    rdfs:label "Fusion Confidence" ;
    rdfs:comment "Confidence in the fusion match (0.0-1.0)" .

gph:reversedAt a rdf:Property ;
    rdfs:domain gph:FusionOperation ;
    rdfs:range xsd:dateTime ;
    rdfs:label "Reversed At" ;
    rdfs:comment "Timestamp when fusion was reversed (null if active)" .

gph:reversalReason a rdf:Property ;
    rdfs:domain gph:FusionOperation ;
    rdfs:range xsd:string ;
    rdfs:label "Reversal Reason" ;
    rdfs:comment "Reason for reversing the fusion" .

# ============================================================================
# FUSION CANDIDATE PROPERTIES
# ============================================================================

gph:matchRule a rdf:Property ;
    rdfs:domain gph:FusionCandidate ;
    rdfs:range xsd:string ;
    rdfs:label "Match Rule" ;
    rdfs:comment "Rule used to identify matching entities (email, phone, ssn, etc.)" .

gph:matchValue a rdf:Property ;
    rdfs:domain gph:FusionCandidate ;
    rdfs:range xsd:string ;
    rdfs:label "Match Value" ;
    rdfs:comment "The value that matched across entities" .

gph:hasEntity a rdf:Property ;
    rdfs:domain gph:FusionCandidate ;
    rdfs:range gph:Entity ;
    rdfs:label "Has Entity" ;
    rdfs:comment "Links candidate to entity that is part of the match group" .

gph:proposedAt a rdf:Property ;
    rdfs:domain gph:FusionCandidate ;
    rdfs:range xsd:dateTime ;
    rdfs:label "Proposed At" ;
    rdfs:comment "Timestamp when candidate was proposed" .

gph:status a rdf:Property ;
    rdfs:domain gph:FusionCandidate ;
    rdfs:range xsd:string ;
    rdfs:label "Candidate Status" ;
    rdfs:comment "Status: proposed, approved, rejected, committed" .

gph:reviewedBy a rdf:Property ;
    rdfs:domain gph:FusionCandidate ;
    rdfs:range prov:Agent ;
    rdfs:label "Reviewed By" ;
    rdfs:comment "User or system that reviewed the candidate" .

gph:reviewedAt a rdf:Property ;
    rdfs:domain gph:FusionCandidate ;
    rdfs:range xsd:dateTime ;
    rdfs:label "Reviewed At" ;
    rdfs:comment "Timestamp when candidate was reviewed" .

gph:reviewNotes a rdf:Property ;
    rdfs:domain gph:FusionCandidate ;
    rdfs:range xsd:string ;
    rdfs:label "Review Notes" ;
    rdfs:comment "Notes from human reviewer" .

# ============================================================================
# LINEAGE PROPERTIES (extending PROV)
# ============================================================================

gph:recordId a rdf:Property ;
    rdfs:domain prov:Entity ;
    rdfs:range xsd:string ;
    rdfs:label "Record ID" ;
    rdfs:comment "Unique record identifier in lineage" .

gph:dataset a rdf:Property ;
    rdfs:domain prov:Entity ;
    rdfs:range xsd:string ;
    rdfs:label "Dataset" ;
    rdfs:comment "Dataset name or identifier" .

gph:cdcPosition a rdf:Property ;
    rdfs:domain prov:Entity ;
    rdfs:range xsd:string ;
    rdfs:label "CDC Position" ;
    rdfs:comment "Change Data Capture position for replay" .

gph:transformType a rdf:Property ;
    rdfs:domain prov:Activity ;
    rdfs:range xsd:string ;
    rdfs:label "Transform Type" ;
    rdfs:comment "Type of transformation: standardize, dedupe, enrich, etc." .

# ============================================================================
# DATASET PROPERTIES
# ============================================================================

gph:datasetName a rdf:Property ;
    rdfs:domain gph:Dataset ;
    rdfs:range xsd:string ;
    rdfs:label "Dataset Name" ;
    rdfs:comment "Human-readable dataset name" .

gph:datasetType a rdf:Property ;
    rdfs:domain gph:Dataset ;
    rdfs:range xsd:string ;
    rdfs:label "Dataset Type" ;
    rdfs:comment "Type: source, workflow_output, training_data, fusion_result" .

gph:recordCount a rdf:Property ;
    rdfs:domain gph:Dataset ;
    rdfs:range xsd:integer ;
    rdfs:label "Record Count" ;
    rdfs:comment "Total number of records in dataset" .

gph:schemaHash a rdf:Property ;
    rdfs:domain gph:Dataset ;
    rdfs:range xsd:string ;
    rdfs:label "Schema Hash" ;
    rdfs:comment "Hash of schema for change detection" .

gph:hasColumn a rdf:Property ;
    rdfs:domain gph:Dataset ;
    rdfs:range gph:DatasetColumn ;
    rdfs:label "Has Column" ;
    rdfs:comment "Links dataset to its columns" .

gph:sourceDataSource a rdf:Property ;
    rdfs:domain gph:Dataset ;
    rdfs:range xsd:string ;
    rdfs:label "Source Data Source" ;
    rdfs:comment "Link to data source catalog ID" .

gph:producedByWorkflow a rdf:Property ;
    rdfs:domain gph:Dataset ;
    rdfs:range workflow:Execution ;
    rdfs:label "Produced By Workflow" ;
    rdfs:comment "Workflow execution that produced this dataset" .

gph:createdAt a rdf:Property ;
    rdfs:range xsd:dateTime ;
    rdfs:label "Created At" ;
    rdfs:comment "Timestamp when resource was created" .

gph:updatedAt a rdf:Property ;
    rdfs:range xsd:dateTime ;
    rdfs:label "Updated At" ;
    rdfs:comment "Timestamp when resource was last updated" .

gph:lastIngestedAt a rdf:Property ;
    rdfs:domain gph:Dataset ;
    rdfs:range xsd:dateTime ;
    rdfs:label "Last Ingested At" ;
    rdfs:comment "Timestamp of last data ingestion" .

# Dataset Column properties
gph:columnName a rdf:Property ;
    rdfs:domain gph:DatasetColumn ;
    rdfs:range xsd:string ;
    rdfs:label "Column Name" .

gph:columnType a rdf:Property ;
    rdfs:domain gph:DatasetColumn ;
    rdfs:range xsd:string ;
    rdfs:label "Column Type" ;
    rdfs:comment "Data type: INTEGER, VARCHAR, TIMESTAMP, etc." .

gph:nullable a rdf:Property ;
    rdfs:domain gph:DatasetColumn ;
    rdfs:range xsd:boolean ;
    rdfs:label "Nullable" ;
    rdfs:comment "Whether column can contain null values" .

gph:distinctCount a rdf:Property ;
    rdfs:domain gph:DatasetColumn ;
    rdfs:range xsd:integer ;
    rdfs:label "Distinct Count" ;
    rdfs:comment "Number of distinct values in column" .

gph:nullPercentage a rdf:Property ;
    rdfs:domain gph:DatasetColumn ;
    rdfs:range xsd:double ;
    rdfs:label "Null Percentage" ;
    rdfs:comment "Percentage of null values (0.0-1.0)" .

# ============================================================================
# MAPPING PROPERTIES
# ============================================================================

gph:sessionId a rdf:Property ;
    rdfs:domain gph:MappingSession ;
    rdfs:range xsd:string ;
    rdfs:label "Session ID" ;
    rdfs:comment "Unique identifier for the mapping session" .

gph:forDataSource a rdf:Property ;
    rdfs:domain gph:MappingSession ;
    rdfs:range xsd:string ;
    rdfs:label "For Data Source" ;
    rdfs:comment "Data source being mapped to the ontology" .

gph:hasMapping a rdf:Property ;
    rdfs:domain gph:MappingSession ;
    rdfs:range gph:FieldMapping ;
    rdfs:label "Has Mapping" ;
    rdfs:comment "Links session to field mappings" .

gph:sourceTable a rdf:Property ;
    rdfs:domain gph:FieldMapping ;
    rdfs:range xsd:string ;
    rdfs:label "Source Table" ;
    rdfs:comment "Source table name" .

gph:sourceField a rdf:Property ;
    rdfs:domain gph:FieldMapping ;
    rdfs:range xsd:string ;
    rdfs:label "Source Field" ;
    rdfs:comment "Source field name" .

gph:mapsToOntologyTerm a rdf:Property ;
    rdfs:domain gph:FieldMapping ;
    rdfs:label "Maps To Ontology Term" ;
    rdfs:comment "Target ontology term URI" .

gph:approvalStatus a rdf:Property ;
    rdfs:domain gph:FieldMapping ;
    rdfs:range xsd:string ;
    rdfs:label "Approval Status" ;
    rdfs:comment "Status: pending, autoapproved, approved, modified, rejected" .

gph:wasTopCandidate a rdf:Property ;
    rdfs:domain gph:FieldMapping ;
    rdfs:range xsd:boolean ;
    rdfs:label "Was Top Candidate" ;
    rdfs:comment "Whether the selected mapping was the top-ranked candidate" .

gph:transformation a rdf:Property ;
    rdfs:domain gph:FieldMapping ;
    rdfs:range xsd:string ;
    rdfs:label "Transformation" ;
    rdfs:comment "Optional transformation rule applied to the mapping" .

gph:notes a rdf:Property ;
    rdfs:label "Notes" ;
    rdfs:range xsd:string ;
    rdfs:comment "Human-readable notes or comments" .

# ============================================================================
# QUALITY PROPERTIES
# ============================================================================

gph:qualityScore a rdf:Property ;
    rdfs:domain gph:Entity ;
    rdfs:range xsd:double ;
    rdfs:label "Quality Score" ;
    rdfs:comment "Overall data quality score (0.0-1.0)" .

gph:hasViolation a rdf:Property ;
    rdfs:domain gph:Entity ;
    rdfs:range gph:QualityViolation ;
    rdfs:label "Has Violation" ;
    rdfs:comment "Links entity to quality violations" .

gph:QualityViolation a rdfs:Class ;
    rdfs:label "Quality Violation" ;
    rdfs:comment "Data quality rule violation" .

gph:ruleId a rdf:Property ;
    rdfs:domain gph:QualityViolation ;
    rdfs:range xsd:string ;
    rdfs:label "Rule ID" .

gph:severity a rdf:Property ;
    rdfs:domain gph:QualityViolation ;
    rdfs:range xsd:string ;
    rdfs:label "Severity" ;
    rdfs:comment "Severity: info, warning, error, critical" .

# ============================================================================
# TEMPORAL PROPERTIES
# ============================================================================

gph:validFrom a rdf:Property ;
    rdfs:range xsd:dateTime ;
    rdfs:label "Valid From" ;
    rdfs:comment "Start of validity period" .

gph:validTo a rdf:Property ;
    rdfs:range xsd:dateTime ;
    rdfs:label "Valid To" ;
    rdfs:comment "End of validity period" .

gph:capturedAt a rdf:Property ;
    rdfs:range xsd:dateTime ;
    rdfs:label "Captured At" ;
    rdfs:comment "Timestamp when data was captured" .

# ============================================================================
# WORKFLOW CLASSES
# ============================================================================

workflow:Workflow a rdfs:Class ;
    rdfs:subClassOf prov:Plan ;
    rdfs:label "Workflow Definition" ;
    rdfs:comment "A workflow orchestration plan with steps" .

workflow:Execution a rdfs:Class ;
    rdfs:subClassOf prov:Activity ;
    rdfs:label "Workflow Execution" ;
    rdfs:comment "An execution of a workflow" .

workflow:Step a rdfs:Class ;
    rdfs:label "Workflow Step" ;
    rdfs:comment "A step definition in a workflow" .

workflow:StepExecution a rdfs:Class ;
    rdfs:subClassOf prov:Activity ;
    rdfs:label "Step Execution" ;
    rdfs:comment "An execution of a workflow step" .

# ============================================================================
# WORKFLOW PROPERTIES
# ============================================================================

workflow:workflowId a rdf:Property ;
    rdfs:domain workflow:Workflow ;
    rdfs:range xsd:string ;
    rdfs:label "Workflow ID" ;
    rdfs:comment "Unique identifier for the workflow" .

workflow:workflowName a rdf:Property ;
    rdfs:domain workflow:Workflow ;
    rdfs:range xsd:string ;
    rdfs:label "Workflow Name" ;
    rdfs:comment "Human-readable workflow name" .

workflow:version a rdf:Property ;
    rdfs:domain workflow:Workflow ;
    rdfs:range xsd:string ;
    rdfs:label "Workflow Version" ;
    rdfs:comment "Semantic version of the workflow" .

workflow:executedWorkflow a rdf:Property ;
    rdfs:domain workflow:Execution ;
    rdfs:range workflow:Workflow ;
    rdfs:label "Executed Workflow" ;
    rdfs:comment "Links execution to workflow definition" .

workflow:success a rdf:Property ;
    rdfs:domain workflow:Execution ;
    rdfs:range xsd:boolean ;
    rdfs:label "Execution Success" ;
    rdfs:comment "Whether the execution succeeded" .

workflow:confidence a rdf:Property ;
    rdfs:domain workflow:Execution ;
    rdfs:range xsd:double ;
    rdfs:label "Execution Confidence" ;
    rdfs:comment "Confidence score of the execution result (0.0-1.0)" .

workflow:finalDecision a rdf:Property ;
    rdfs:domain workflow:Execution ;
    rdfs:range xsd:string ;
    rdfs:label "Final Decision" ;
    rdfs:comment "Final decision from workflow execution" .

workflow:error a rdf:Property ;
    rdfs:domain workflow:Execution ;
    rdfs:range xsd:string ;
    rdfs:label "Error Message" ;
    rdfs:comment "Error message if execution failed" .

workflow:partOfExecution a rdf:Property ;
    rdfs:domain workflow:StepExecution ;
    rdfs:range workflow:Execution ;
    rdfs:label "Part Of Execution" ;
    rdfs:comment "Links step execution to parent workflow execution" .

workflow:stepId a rdf:Property ;
    rdfs:domain workflow:StepExecution ;
    rdfs:range xsd:string ;
    rdfs:label "Step ID" ;
    rdfs:comment "Unique identifier for the step" .

workflow:stepType a rdf:Property ;
    rdfs:domain workflow:StepExecution ;
    rdfs:range xsd:string ;
    rdfs:label "Step Type" ;
    rdfs:comment "Type of step: transform, model_invoke, quality_check, etc." .

workflow:output a rdf:Property ;
    rdfs:domain workflow:StepExecution ;
    rdfs:range xsd:string ;
    rdfs:label "Step Output" ;
    rdfs:comment "JSON-encoded output from step execution" .

# ============================================================================
# GDPR COMPLIANCE ONTOLOGY
# ============================================================================

@prefix gdpr: <{GDPR_NS}> .

# ----------------------------------------------------------------------------
# GDPR Classes
# ----------------------------------------------------------------------------

# Data Subject - Individual whose personal data is being processed
gdpr:DataSubject a rdfs:Class ;
    rdfs:subClassOf prov:Agent ;
    rdfs:label "Data Subject" ;
    rdfs:comment "Individual (natural person) whose personal data is being processed under GDPR" .

# Consent Record - Tracks consent given by data subject
gdpr:ConsentRecord a rdfs:Class ;
    rdfs:subClassOf prov:Entity ;
    rdfs:label "Consent Record" ;
    rdfs:comment "Record of consent given by data subject for specific processing purposes" .

# Processing Activity - Data processing operation
gdpr:ProcessingActivity a rdfs:Class ;
    rdfs:subClassOf prov:Activity ;
    rdfs:label "Processing Activity" ;
    rdfs:comment "Activity that processes personal data, requiring legal basis under GDPR Article 6" .

# Legal Hold - Litigation/investigation hold preventing deletion
gdpr:LegalHold a rdfs:Class ;
    rdfs:label "Legal Hold" ;
    rdfs:comment "Litigation or investigation hold that prevents data deletion" .

# Retention Policy - Data retention rules
gdpr:RetentionPolicy a rdfs:Class ;
    rdfs:label "Retention Policy" ;
    rdfs:comment "Policy defining minimum/maximum data retention periods for legal compliance" .

# Erasure Request - Right to be forgotten request
gdpr:ErasureRequest a rdfs:Class ;
    rdfs:subClassOf prov:Activity ;
    rdfs:label "Erasure Request" ;
    rdfs:comment "GDPR Article 17 request for data erasure (right to be forgotten)" .

# ----------------------------------------------------------------------------
# GDPR Properties
# ----------------------------------------------------------------------------

# Data Subject Properties
gdpr:subjectId a rdf:Property ;
    rdfs:domain gdpr:DataSubject ;
    rdfs:range xsd:string ;
    rdfs:label "Subject ID" ;
    rdfs:comment "Unique identifier for the data subject" .

gdpr:subjectType a rdf:Property ;
    rdfs:domain gdpr:DataSubject ;
    rdfs:range xsd:string ;
    rdfs:label "Subject Type" ;
    rdfs:comment "Type of subject identifier: user_id, email, tenant_id" .

# Consent Record Properties
gdpr:hasConsent a rdf:Property ;
    rdfs:domain gdpr:DataSubject ;
    rdfs:range gdpr:ConsentRecord ;
    rdfs:label "Has Consent" ;
    rdfs:comment "Links data subject to their consent records" .

gdpr:consentId a rdf:Property ;
    rdfs:domain gdpr:ConsentRecord ;
    rdfs:range xsd:string ;
    rdfs:label "Consent ID" ;
    rdfs:comment "Unique identifier for this consent record" .

gdpr:consentPurpose a rdf:Property ;
    rdfs:domain gdpr:ConsentRecord ;
    rdfs:range xsd:string ;
    rdfs:label "Consent Purpose" ;
    rdfs:comment "Purpose for which consent was given (marketing, analytics, etc.)" .

gdpr:consentStatus a rdf:Property ;
    rdfs:domain gdpr:ConsentRecord ;
    rdfs:range xsd:string ;
    rdfs:label "Consent Status" ;
    rdfs:comment "Current status: granted, denied, withdrawn, pending" .

gdpr:grantedAt a rdf:Property ;
    rdfs:domain gdpr:ConsentRecord ;
    rdfs:range xsd:dateTime ;
    rdfs:label "Granted At" ;
    rdfs:comment "Timestamp when consent was granted" .

gdpr:withdrawnAt a rdf:Property ;
    rdfs:domain gdpr:ConsentRecord ;
    rdfs:range xsd:dateTime ;
    rdfs:label "Withdrawn At" ;
    rdfs:comment "Timestamp when consent was withdrawn" .

# Processing Activity Properties
gdpr:lawfulBasis a rdf:Property ;
    rdfs:domain gdpr:ProcessingActivity ;
    rdfs:range xsd:string ;
    rdfs:label "Lawful Basis" ;
    rdfs:comment "Legal basis for processing under GDPR Article 6: consent, contract, legal_obligation, vital_interests, public_task, legitimate_interests" .

gdpr:processingPurpose a rdf:Property ;
    rdfs:domain gdpr:ProcessingActivity ;
    rdfs:range xsd:string ;
    rdfs:label "Processing Purpose" ;
    rdfs:comment "Specific purpose for which data is being processed" .

gdpr:processesData a rdf:Property ;
    rdfs:domain gdpr:ProcessingActivity ;
    rdfs:range prov:Entity ;
    rdfs:label "Processes Data" ;
    rdfs:comment "Links processing activity to data being processed" .

gdpr:involvesSubject a rdf:Property ;
    rdfs:domain gdpr:ProcessingActivity ;
    rdfs:range gdpr:DataSubject ;
    rdfs:label "Involves Subject" ;
    rdfs:comment "Links processing activity to affected data subject" .

# Legal Hold Properties
gdpr:holdId a rdf:Property ;
    rdfs:domain gdpr:LegalHold ;
    rdfs:range xsd:string ;
    rdfs:label "Hold ID" ;
    rdfs:comment "Unique identifier for the legal hold" .

gdpr:holdName a rdf:Property ;
    rdfs:domain gdpr:LegalHold ;
    rdfs:range xsd:string ;
    rdfs:label "Hold Name" ;
    rdfs:comment "Name/case number for the legal hold" .

gdpr:holdReason a rdf:Property ;
    rdfs:domain gdpr:LegalHold ;
    rdfs:range xsd:string ;
    rdfs:label "Hold Reason" ;
    rdfs:comment "Reason for the legal hold (litigation, investigation)" .

gdpr:holdPlacedAt a rdf:Property ;
    rdfs:domain gdpr:LegalHold ;
    rdfs:range xsd:dateTime ;
    rdfs:label "Hold Placed At" ;
    rdfs:comment "When the legal hold was placed" .

gdpr:holdPlacedBy a rdf:Property ;
    rdfs:domain gdpr:LegalHold ;
    rdfs:range xsd:string ;
    rdfs:label "Hold Placed By" ;
    rdfs:comment "Who placed the legal hold" .

gdpr:holdsData a rdf:Property ;
    rdfs:domain gdpr:LegalHold ;
    rdfs:range gdpr:DataSubject ;
    rdfs:label "Holds Data" ;
    rdfs:comment "Links legal hold to affected data subjects" .

# Retention Policy Properties
gdpr:dataCategory a rdf:Property ;
    rdfs:domain gdpr:RetentionPolicy ;
    rdfs:range xsd:string ;
    rdfs:label "Data Category" ;
    rdfs:comment "Category of data this policy applies to" .

gdpr:minRetentionDays a rdf:Property ;
    rdfs:domain gdpr:RetentionPolicy ;
    rdfs:range xsd:integer ;
    rdfs:label "Minimum Retention Days" ;
    rdfs:comment "Minimum number of days data must be retained" .

gdpr:maxRetentionDays a rdf:Property ;
    rdfs:domain gdpr:RetentionPolicy ;
    rdfs:range xsd:integer ;
    rdfs:label "Maximum Retention Days" ;
    rdfs:comment "Maximum number of days data may be retained (storage limitation)" .

gdpr:legalBasis a rdf:Property ;
    rdfs:domain gdpr:RetentionPolicy ;
    rdfs:range xsd:string ;
    rdfs:label "Legal Basis" ;
    rdfs:comment "Legal basis for the retention policy (tax law, GDPR, etc.)" .

# Erasure Request Properties
gdpr:erasureReason a rdf:Property ;
    rdfs:domain gdpr:ErasureRequest ;
    rdfs:range xsd:string ;
    rdfs:label "Erasure Reason" ;
    rdfs:comment "Reason for erasure: user_request, consent_withdrawn, unlawful_processing, legal_obligation" .

gdpr:erasureStrategy a rdf:Property ;
    rdfs:domain gdpr:ErasureRequest ;
    rdfs:range xsd:string ;
    rdfs:label "Erasure Strategy" ;
    rdfs:comment "Strategy used: hard_delete, anonymize, tombstone, archive_then_delete" .

gdpr:recordsErased a rdf:Property ;
    rdfs:domain gdpr:ErasureRequest ;
    rdfs:range xsd:integer ;
    rdfs:label "Records Erased" ;
    rdfs:comment "Number of records erased" .

gdpr:erasedAt a rdf:Property ;
    rdfs:domain gdpr:ErasureRequest ;
    rdfs:range xsd:dateTime ;
    rdfs:label "Erased At" ;
    rdfs:comment "When the erasure was completed" .

# ============================================================================
# METADATA
# ============================================================================

# Ontology metadata
<{GRAPHICA_NS}> a owl:Ontology ;
    rdfs:label "Graphica Data Governance Ontology" ;
    rdfs:comment "RDF ontology for data governance, lineage, quality management, and GDPR compliance" ;
    owl:versionInfo "1.1.0" ;
    prov:wasGeneratedBy <http://graphica.io> .
"#
        );

        Self { turtle }
    }

    /// Get ontology as Turtle format
    pub fn to_turtle(&self) -> String {
        self.turtle.clone()
    }

    /// Get namespace URIs
    pub fn namespaces() -> Vec<(&'static str, &'static str)> {
        vec![
            ("gph", GRAPHICA_NS),
            ("ml", ML_NS),
            ("workflow", WORKFLOW_NS),
            ("gdpr", GDPR_NS),
            ("prov", PROV_NS),
            ("rdf", RDF_NS),
            ("rdfs", RDFS_NS),
            ("xsd", XSD_NS),
        ]
    }
}

impl Default for GraphicaOntology {
    fn default() -> Self {
        Self::new()
    }
}

/// Utility functions for building RDF URIs
pub mod uris {
    use super::*;

    /// Create entity URI
    pub fn entity(id: &str) -> String {
        format!("{}entity/{}", GRAPHICA_NS, id)
    }

    /// Create attribute URI
    pub fn attribute(id: &str) -> String {
        format!("{}attr/{}", GRAPHICA_NS, id)
    }

    /// Create model URI
    pub fn model(id: &str) -> String {
        format!("{}model/{}", ML_NS, id)
    }

    /// Create fusion operation URI
    pub fn fusion(id: &str) -> String {
        format!("{}fusion/{}", GRAPHICA_NS, id)
    }

    /// Create fusion candidate URI
    pub fn fusion_candidate(id: &str) -> String {
        format!("{}fusion/candidate/{}", GRAPHICA_NS, id)
    }

    /// Create lineage event URI
    pub fn lineage(id: &str) -> String {
        format!("{}lineage/{}", GRAPHICA_NS, id)
    }

    /// Create dataset URI
    pub fn dataset(id: &str) -> String {
        format!("{}dataset/{}", GRAPHICA_NS, id)
    }

    /// Create workflow URI
    pub fn workflow(id: &str) -> String {
        format!("{}workflow/{}", WORKFLOW_NS, id)
    }

    /// Create workflow execution URI
    pub fn workflow_execution(id: &str) -> String {
        format!("{}execution/{}", WORKFLOW_NS, id)
    }

    /// Create step execution URI
    pub fn step_execution(id: &str) -> String {
        format!("{}step/{}", WORKFLOW_NS, id)
    }

    // ========================================================================
    // GDPR URI Builders
    // ========================================================================

    /// Create data subject URI
    pub fn data_subject(subject_id: &str) -> String {
        format!("{}subject/{}", GDPR_NS, subject_id)
    }

    /// Create consent record URI
    pub fn consent_record(consent_id: &str) -> String {
        format!("{}consent/{}", GDPR_NS, consent_id)
    }

    /// Create processing activity URI
    pub fn processing_activity(activity_id: &str) -> String {
        format!("{}activity/{}", GDPR_NS, activity_id)
    }

    /// Create legal hold URI
    pub fn legal_hold(hold_id: &str) -> String {
        format!("{}hold/{}", GDPR_NS, hold_id)
    }

    /// Create retention policy URI
    pub fn retention_policy(policy_id: &str) -> String {
        format!("{}policy/{}", GDPR_NS, policy_id)
    }

    /// Create erasure request URI
    pub fn erasure_request(request_id: &str) -> String {
        format!("{}erasure/{}", GDPR_NS, request_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ontology_creation() {
        let onto = GraphicaOntology::new();
        assert!(onto.to_turtle().contains("gph:Entity"));
        assert!(onto.to_turtle().contains("gph:Dataset"));
        assert!(onto.to_turtle().contains("ml:Model"));
        assert!(onto.to_turtle().contains("gph:FusionOperation"));
        assert!(onto.to_turtle().contains("gph:FusionCandidate"));
        assert!(onto.to_turtle().contains("workflow:Workflow"));
        assert!(onto.to_turtle().contains("workflow:Execution"));
    }

    #[test]
    fn test_uri_builders() {
        assert_eq!(
            uris::entity("123"),
            "http://graphica.io/ontology#entity/123"
        );
        assert_eq!(
            uris::model("mdl_456"),
            "http://graphica.io/ml#model/mdl_456"
        );
        assert_eq!(
            uris::fusion("fus_789"),
            "http://graphica.io/ontology#fusion/fus_789"
        );
        assert_eq!(
            uris::workflow("wf_001"),
            "http://graphica.io/workflow#workflow/wf_001"
        );
        assert_eq!(
            uris::workflow_execution("exec_123"),
            "http://graphica.io/workflow#execution/exec_123"
        );
        assert_eq!(
            uris::step_execution("step_456"),
            "http://graphica.io/workflow#step/step_456"
        );
    }

    #[test]
    fn test_namespaces() {
        let ns = GraphicaOntology::namespaces();
        assert_eq!(ns.len(), 8); // Updated for GDPR namespace
        assert!(ns.iter().any(|(prefix, _)| *prefix == "gph"));
        assert!(ns.iter().any(|(prefix, _)| *prefix == "ml"));
        assert!(ns.iter().any(|(prefix, _)| *prefix == "workflow"));
        assert!(ns.iter().any(|(prefix, _)| *prefix == "gdpr"));
    }

    #[test]
    fn test_gdpr_ontology_classes() {
        let onto = GraphicaOntology::new();
        assert!(onto.to_turtle().contains("gdpr:DataSubject"));
        assert!(onto.to_turtle().contains("gdpr:ConsentRecord"));
        assert!(onto.to_turtle().contains("gdpr:ProcessingActivity"));
        assert!(onto.to_turtle().contains("gdpr:LegalHold"));
        assert!(onto.to_turtle().contains("gdpr:RetentionPolicy"));
        assert!(onto.to_turtle().contains("gdpr:ErasureRequest"));
    }

    #[test]
    fn test_gdpr_uri_builders() {
        assert_eq!(
            uris::data_subject("user123"),
            "http://graphica.io/gdpr#subject/user123"
        );
        assert_eq!(
            uris::consent_record("consent456"),
            "http://graphica.io/gdpr#consent/consent456"
        );
        assert_eq!(
            uris::processing_activity("act789"),
            "http://graphica.io/gdpr#activity/act789"
        );
        assert_eq!(
            uris::legal_hold("hold123"),
            "http://graphica.io/gdpr#hold/hold123"
        );
        assert_eq!(
            uris::retention_policy("pol456"),
            "http://graphica.io/gdpr#policy/pol456"
        );
        assert_eq!(
            uris::erasure_request("req789"),
            "http://graphica.io/gdpr#erasure/req789"
        );
    }
}
