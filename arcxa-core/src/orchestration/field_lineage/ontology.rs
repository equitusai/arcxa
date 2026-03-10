//! Field-Level Lineage RDF Ontology Extensions
//!
//! Extends W3C PROV with field-level provenance for golden record creation.
//! Tracks individual field values, their sources, voting decisions, and confidence scores.

/// Graphica Field Lineage namespace
pub const FIELD_NS: &str = "http://graphica.io/field#";

/// W3C PROV namespace
pub const PROV_NS: &str = "http://www.w3.org/ns/prov#";

/// XSD namespace for datatypes
pub const XSD_NS: &str = "http://www.w3.org/2001/XMLSchema#";

/// Complete RDF ontology for field-level lineage
///
/// This ontology enables:
/// - Field-level provenance tracking
/// - Multi-source value aggregation
/// - Voting decision explanation
/// - Confidence propagation
/// - Temporal field evolution
pub fn field_lineage_ontology() -> String {
    format!(
        r#"
@prefix field: <{FIELD_NS}> .
@prefix prov: <{PROV_NS}> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix xsd: <{XSD_NS}> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .

# ============================================================================
# Field-Level Provenance Classes
# ============================================================================

field:FieldValue a owl:Class ;
    rdfs:subClassOf prov:Entity ;
    rdfs:label "Field Value" ;
    rdfs:comment "A specific value for a field in a golden record with full provenance" .

field:SourceValue a owl:Class ;
    rdfs:subClassOf prov:Entity ;
    rdfs:label "Source Value" ;
    rdfs:comment "A candidate value from a source system for field resolution" .

field:FieldResolution a owl:Class ;
    rdfs:subClassOf prov:Activity ;
    rdfs:label "Field Resolution Activity" ;
    rdfs:comment "The activity of resolving a field value from multiple sources using voting" .

field:VotingStrategy a owl:Class ;
    rdfs:subClassOf prov:Plan ;
    rdfs:label "Voting Strategy" ;
    rdfs:comment "The strategy used to resolve conflicting field values" .

field:FieldConflict a owl:Class ;
    rdfs:label "Field Conflict" ;
    rdfs:comment "A conflict between multiple candidate values for the same field" .

# ============================================================================
# Field-Level Properties
# ============================================================================

# Field identity and metadata
field:fieldName a owl:DatatypeProperty ;
    rdfs:domain field:FieldValue ;
    rdfs:range xsd:string ;
    rdfs:label "Field Name" ;
    rdfs:comment "The name of the field (e.g., 'address', 'email', 'phone')" .

field:fieldValue a owl:DatatypeProperty ;
    rdfs:domain field:FieldValue ;
    rdfs:label "Field Value" ;
    rdfs:comment "The actual value chosen for the field" .

field:valueType a owl:DatatypeProperty ;
    rdfs:domain field:FieldValue ;
    rdfs:range xsd:string ;
    rdfs:label "Value Type" ;
    rdfs:comment "Data type of the value (string, number, date, etc.)" .

# Provenance linkage
field:resolvedFrom a owl:ObjectProperty ;
    rdfs:subPropertyOf prov:wasDerivedFrom ;
    rdfs:domain field:FieldValue ;
    rdfs:range field:SourceValue ;
    rdfs:label "Resolved From" ;
    rdfs:comment "Links golden record field to source values considered" .

field:hadSourceValue a owl:ObjectProperty ;
    rdfs:domain field:FieldResolution ;
    rdfs:range field:SourceValue ;
    rdfs:label "Had Source Value" ;
    rdfs:comment "Source value considered during resolution" .

field:selectedValue a owl:ObjectProperty ;
    rdfs:domain field:FieldResolution ;
    rdfs:range field:SourceValue ;
    rdfs:label "Selected Value" ;
    rdfs:comment "The source value that was chosen by voting" .

field:rejectedValue a owl:ObjectProperty ;
    rdfs:domain field:FieldResolution ;
    rdfs:range field:SourceValue ;
    rdfs:label "Rejected Value" ;
    rdfs:comment "A source value that was not chosen" .

# Source metadata
field:sourceSystem a owl:DatatypeProperty ;
    rdfs:domain field:SourceValue ;
    rdfs:range xsd:string ;
    rdfs:label "Source System" ;
    rdfs:comment "The system that provided this value (e.g., 'CRM', 'ERP', 'Website')" .

field:sourceTimestamp a owl:DatatypeProperty ;
    rdfs:domain field:SourceValue ;
    rdfs:range xsd:dateTime ;
    rdfs:label "Source Timestamp" ;
    rdfs:comment "When this value was recorded in the source system" .

field:sourceAuthority a owl:DatatypeProperty ;
    rdfs:domain field:SourceValue ;
    rdfs:range xsd:double ;
    rdfs:label "Source Authority" ;
    rdfs:comment "Authority/trust weight of the source system (0.0-1.0)" .

# Voting mechanism
field:usedStrategy a owl:ObjectProperty ;
    rdfs:domain field:FieldResolution ;
    rdfs:range field:VotingStrategy ;
    rdfs:label "Used Strategy" ;
    rdfs:comment "The voting strategy used for resolution" .

field:strategyType a owl:DatatypeProperty ;
    rdfs:domain field:VotingStrategy ;
    rdfs:range xsd:string ;
    rdfs:label "Strategy Type" ;
    rdfs:comment "Type of voting: frequency, time-decay, authority, ensemble" .

field:strategyParameters a owl:DatatypeProperty ;
    rdfs:domain field:VotingStrategy ;
    rdfs:range xsd:string ;
    rdfs:label "Strategy Parameters" ;
    rdfs:comment "JSON parameters for the voting strategy" .

# Confidence tracking
field:confidence a owl:DatatypeProperty ;
    rdfs:range xsd:double ;
    rdfs:label "Confidence" ;
    rdfs:comment "Confidence score (0.0-1.0) for this value" .

field:voteCount a owl:DatatypeProperty ;
    rdfs:domain field:SourceValue ;
    rdfs:range xsd:integer ;
    rdfs:label "Vote Count" ;
    rdfs:comment "Number of votes this value received" .

field:voteWeight a owl:DatatypeProperty ;
    rdfs:domain field:SourceValue ;
    rdfs:range xsd:double ;
    rdfs:label "Vote Weight" ;
    rdfs:comment "Weighted vote score for this value" .

# Conflict metadata
field:hasConflict a owl:ObjectProperty ;
    rdfs:domain field:FieldResolution ;
    rdfs:range field:FieldConflict ;
    rdfs:label "Has Conflict" ;
    rdfs:comment "Indicates this resolution had conflicting values" .

field:conflictSeverity a owl:DatatypeProperty ;
    rdfs:domain field:FieldConflict ;
    rdfs:range xsd:string ;
    rdfs:label "Conflict Severity" ;
    rdfs:comment "Severity: low, medium, high, critical" .

field:conflictReason a owl:DatatypeProperty ;
    rdfs:domain field:FieldConflict ;
    rdfs:range xsd:string ;
    rdfs:label "Conflict Reason" ;
    rdfs:comment "Explanation of why values conflicted" .

field:requiresReview a owl:DatatypeProperty ;
    rdfs:domain field:FieldConflict ;
    rdfs:range xsd:boolean ;
    rdfs:label "Requires Review" ;
    rdfs:comment "Whether this conflict needs human review" .

# Temporal tracking
field:validFrom a owl:DatatypeProperty ;
    rdfs:range xsd:dateTime ;
    rdfs:label "Valid From" ;
    rdfs:comment "Start of validity period for this field value" .

field:validTo a owl:DatatypeProperty ;
    rdfs:range xsd:dateTime ;
    rdfs:label "Valid To" ;
    rdfs:comment "End of validity period for this field value" .

field:supersedes a owl:ObjectProperty ;
    rdfs:domain field:FieldValue ;
    rdfs:range field:FieldValue ;
    rdfs:label "Supersedes" ;
    rdfs:comment "This field value replaces a previous value" .

# Explanation and audit
field:explanation a owl:DatatypeProperty ;
    rdfs:range xsd:string ;
    rdfs:label "Explanation" ;
    rdfs:comment "Human-readable explanation of why this value was chosen" .

field:reviewedBy a owl:ObjectProperty ;
    rdfs:subPropertyOf prov:wasAttributedTo ;
    rdfs:label "Reviewed By" ;
    rdfs:comment "Agent (human or ML model) who reviewed this resolution" .

field:reviewNotes a owl:DatatypeProperty ;
    rdfs:range xsd:string ;
    rdfs:label "Review Notes" ;
    rdfs:comment "Notes from reviewer about this field resolution" .

# ============================================================================
# Voting Strategy Individuals
# ============================================================================

field:FrequencyVoting a field:VotingStrategy ;
    rdfs:label "Frequency Voting" ;
    rdfs:comment "Most common value wins (majority vote)" ;
    field:strategyType "frequency" .

field:TimeDecayVoting a field:VotingStrategy ;
    rdfs:label "Time-Decay Voting" ;
    rdfs:comment "Recent values weighted higher (exponential decay)" ;
    field:strategyType "time-decay" .

field:AuthorityVoting a field:VotingStrategy ;
    rdfs:label "Authority Voting" ;
    rdfs:comment "Trusted sources weighted higher" ;
    field:strategyType "authority" .

field:EnsembleVoting a field:VotingStrategy ;
    rdfs:label "Ensemble Voting" ;
    rdfs:comment "Combine multiple strategies (weighted average)" ;
    field:strategyType "ensemble" .

field:MLPredictionVoting a field:VotingStrategy ;
    rdfs:label "ML Prediction Voting" ;
    rdfs:comment "Use ML model to predict correct value" ;
    field:strategyType "ml-prediction" .

# ============================================================================
# SHACL Constraints
# ============================================================================

field:FieldValueShape a owl:Class ;
    rdfs:comment "Validation shape for field values" .

# Example: Confidence must be between 0.0 and 1.0
field:confidenceConstraint a owl:Restriction ;
    rdfs:comment "Confidence must be in range [0.0, 1.0]" .

# Example: Field resolution must use exactly one strategy
field:singleStrategyConstraint a owl:Restriction ;
    rdfs:comment "Field resolution must use exactly one voting strategy" .

"#
    )
}

/// Generate URI for a field value
pub fn field_value_uri(entity_id: &str, field_name: &str) -> String {
    format!("{FIELD_NS}value/{entity_id}/{field_name}")
}

/// Generate URI for a source value
pub fn source_value_uri(
    source_system: &str,
    entity_id: &str,
    field_name: &str,
    timestamp: i64,
) -> String {
    format!("{FIELD_NS}source/{source_system}/{entity_id}/{field_name}/{timestamp}")
}

/// Generate URI for a field resolution activity
pub fn field_resolution_uri(entity_id: &str, field_name: &str, timestamp: i64) -> String {
    format!("{FIELD_NS}resolution/{entity_id}/{field_name}/{timestamp}")
}

/// Generate URI for a voting strategy
pub fn voting_strategy_uri(strategy_type: &str) -> String {
    format!("{FIELD_NS}strategy/{strategy_type}")
}

/// Generate URI for a field conflict
pub fn field_conflict_uri(entity_id: &str, field_name: &str, timestamp: i64) -> String {
    format!("{FIELD_NS}conflict/{entity_id}/{field_name}/{timestamp}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_field_value_uri() {
        let uri = field_value_uri("cust_001", "address");
        assert_eq!(uri, "http://graphica.io/field#value/cust_001/address");
    }

    #[test]
    fn test_source_value_uri() {
        let uri = source_value_uri("CRM", "cust_001", "email", 1234567890);
        assert_eq!(
            uri,
            "http://graphica.io/field#source/CRM/cust_001/email/1234567890"
        );
    }

    #[test]
    fn test_field_resolution_uri() {
        let uri = field_resolution_uri("cust_001", "phone", 1234567890);
        assert_eq!(
            uri,
            "http://graphica.io/field#resolution/cust_001/phone/1234567890"
        );
    }

    #[test]
    fn test_voting_strategy_uri() {
        let uri = voting_strategy_uri("frequency");
        assert_eq!(uri, "http://graphica.io/field#strategy/frequency");
    }

    #[test]
    fn test_ontology_generation() {
        let ontology = field_lineage_ontology();

        // Verify key classes
        assert!(ontology.contains("field:FieldValue"));
        assert!(ontology.contains("field:SourceValue"));
        assert!(ontology.contains("field:FieldResolution"));
        assert!(ontology.contains("field:VotingStrategy"));

        // Verify key properties
        assert!(ontology.contains("field:fieldName"));
        assert!(ontology.contains("field:resolvedFrom"));
        assert!(ontology.contains("field:usedStrategy"));
        assert!(ontology.contains("field:confidence"));

        // Verify voting strategies
        assert!(ontology.contains("field:FrequencyVoting"));
        assert!(ontology.contains("field:TimeDecayVoting"));
        assert!(ontology.contains("field:AuthorityVoting"));
        assert!(ontology.contains("field:EnsembleVoting"));
    }
}
