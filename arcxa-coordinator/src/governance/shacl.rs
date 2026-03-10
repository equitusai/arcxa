//! # SHACL Constraints
//!
//! SHACL shapes for data governance and quality validation.

/// SHACL shapes for Graphica ontology
pub const GRAPHICA_SHAPES: &str = r#"
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix gph: <http://graphica.ai/ontology#> .
@prefix ml: <http://graphica.ai/ml#> .
@prefix prov: <http://www.w3.org/ns/prov#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

# ===== LineageEvent Constraints =====

gph:LineageEventShape a sh:NodeShape ;
    sh:targetClass gph:LineageEvent ;
    sh:property [
        sh:path gph:dataset ;
        sh:datatype xsd:string ;
        sh:minCount 1 ;
        sh:maxCount 1 ;
        sh:message "LineageEvent must have exactly one dataset"
    ] ;
    sh:property [
        sh:path gph:recordId ;
        sh:datatype xsd:string ;
        sh:minCount 1 ;
        sh:maxCount 1 ;
        sh:message "LineageEvent must have exactly one recordId"
    ] ;
    sh:property [
        sh:path prov:atTime ;
        sh:datatype xsd:dateTime ;
        sh:minCount 1 ;
        sh:maxCount 1 ;
        sh:message "LineageEvent must have a timestamp"
    ] ;
    sh:property [
        sh:path prov:used ;
        sh:minCount 1 ;
        sh:message "LineageEvent must reference at least one source"
    ] .

# ===== ML Model Constraints =====

ml:ModelShape a sh:NodeShape ;
    sh:targetClass ml:Model ;
    sh:property [
        sh:path ml:modelId ;
        sh:datatype xsd:string ;
        sh:minCount 1 ;
        sh:maxCount 1 ;
        sh:pattern "^[a-zA-Z0-9_-]+$" ;
        sh:message "Model must have a valid modelId"
    ] ;
    sh:property [
        sh:path ml:version ;
        sh:datatype xsd:string ;
        sh:minCount 1 ;
        sh:pattern "^[0-9]+\\.[0-9]+\\.[0-9]+$" ;
        sh:message "Model version must be semantic versioning (e.g., 1.0.0)"
    ] ;
    sh:property [
        sh:path ml:paramsHash ;
        sh:datatype xsd:string ;
        sh:minCount 1 ;
        sh:minLength 32 ;
        sh:message "Model must have a parameters hash for reproducibility"
    ] .

# ===== Model Training Constraints =====

ml:ModelVersionShape a sh:NodeShape ;
    sh:targetClass ml:ModelVersion ;
    sh:property [
        sh:path ml:trainedOn ;
        sh:class ml:TrainingDataset ;
        sh:minCount 1 ;
        sh:message "Model must reference training data"
    ] ;
    sh:property [
        sh:path ml:hasMetric ;
        sh:minCount 1 ;
        sh:message "Model must have at least one performance metric"
    ] .

# ===== Quality Rule Constraints =====

gph:QualityRuleShape a sh:NodeShape ;
    sh:targetClass gph:QualityRule ;
    sh:property [
        sh:path gph:ruleId ;
        sh:datatype xsd:string ;
        sh:minCount 1 ;
        sh:maxCount 1 ;
        sh:message "Quality rule must have a unique ruleId"
    ] ;
    sh:property [
        sh:path gph:severity ;
        sh:in ("INFO" "WARNING" "ERROR" "CRITICAL") ;
        sh:minCount 1 ;
        sh:maxCount 1 ;
        sh:message "Quality rule must have valid severity"
    ] ;
    sh:property [
        sh:path gph:expression ;
        sh:datatype xsd:string ;
        sh:minCount 1 ;
        sh:message "Quality rule must have an expression"
    ] .

# ===== Data Source Constraints =====

gph:DataRefShape a sh:NodeShape ;
    sh:targetClass prov:Entity ;
    sh:property [
        sh:path gph:system ;
        sh:datatype xsd:string ;
        sh:minCount 1 ;
        sh:message "Data source must specify system"
    ] ;
    sh:property [
        sh:path gph:path ;
        sh:datatype xsd:string ;
        sh:minCount 1 ;
        sh:message "Data source must specify path"
    ] ;
    sh:property [
        sh:path prov:generatedAtTime ;
        sh:datatype xsd:dateTime ;
        sh:minCount 1 ;
        sh:message "Data source must have extraction timestamp"
    ] .

# ===== Model Governance Constraints =====

ml:ModelGovernanceShape a sh:NodeShape ;
    sh:targetClass ml:Model ;
    sh:sparql [
        sh:message "Model must be retrained if training data is older than 90 days" ;
        sh:prefixes gph: , ml: , prov: ;
        sh:select """
            SELECT $this ?trainingData ?lastTrained
            WHERE {
                $this ml:trainedOn ?trainingData .
                ?trainingData prov:generatedAtTime ?lastTrained .
                FILTER (NOW() - ?lastTrained > "P90D"^^xsd:duration)
            }
        """
    ] ;
    sh:sparql [
        sh:message "Model accuracy must be >= 0.80 in production" ;
        sh:select """
            SELECT $this ?accuracy
            WHERE {
                $this ml:hasMetric ?metrics .
                ?metrics ml:accuracy ?accuracy .
                FILTER (?accuracy < 0.80)
            }
        """
    ] .
"#;
