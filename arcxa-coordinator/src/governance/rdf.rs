//! # RDF Schema Extensions
//!
//! Extended RDF vocabulary for data lineage and ML models.

/// Graphica RDF ontology in Turtle format
pub const GRAPHICA_ONTOLOGY: &str = r#"
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
@prefix prov: <http://www.w3.org/ns/prov#> .
@prefix dcat: <http://www.w3.org/ns/dcat#> .
@prefix gph: <http://graphica.ai/ontology#> .
@prefix ml: <http://graphica.ai/ml#> .

# ===== Core Classes =====

gph:Dataset a rdfs:Class ;
    rdfs:label "Dataset" ;
    rdfs:comment "A collection of data records" ;
    rdfs:subClassOf dcat:Dataset .

gph:DataRecord a rdfs:Class ;
    rdfs:label "Data Record" ;
    rdfs:comment "An individual data record with lineage" .

gph:LineageEvent a rdfs:Class ;
    rdfs:label "Lineage Event" ;
    rdfs:comment "A provenance event capturing data transformation" ;
    rdfs:subClassOf prov:Activity .

gph:Transform a rdfs:Class ;
    rdfs:label "Transform" ;
    rdfs:comment "A data transformation operation" ;
    rdfs:subClassOf prov:Activity .

gph:QualityRule a rdfs:Class ;
    rdfs:label "Quality Rule" ;
    rdfs:comment "A data quality validation rule" .

gph:QualityViolation a rdfs:Class ;
    rdfs:label "Quality Violation" ;
    rdfs:comment "A detected quality rule violation" .

# ===== ML Model Classes =====

ml:Model a rdfs:Class ;
    rdfs:label "ML Model" ;
    rdfs:comment "A machine learning model" ;
    rdfs:subClassOf prov:Agent .

ml:ModelVersion a rdfs:Class ;
    rdfs:label "Model Version" ;
    rdfs:comment "A specific version of an ML model" ;
    rdfs:subClassOf ml:Model .

ml:TrainingDataset a rdfs:Class ;
    rdfs:label "Training Dataset" ;
    rdfs:comment "Dataset used to train a model" ;
    rdfs:subClassOf gph:Dataset .

ml:Inference a rdfs:Class ;
    rdfs:label "Inference" ;
    rdfs:comment "Application of a model to data" ;
    rdfs:subClassOf prov:Activity .

# ===== Properties =====

gph:hasSource a rdf:Property ;
    rdfs:label "has source" ;
    rdfs:domain gph:DataRecord ;
    rdfs:range gph:DataRecord ;
    rdfs:subPropertyOf prov:wasDerivedFrom .

gph:wasTransformedBy a rdf:Property ;
    rdfs:label "was transformed by" ;
    rdfs:domain gph:DataRecord ;
    rdfs:range gph:Transform ;
    rdfs:subPropertyOf prov:wasGeneratedBy .

gph:usedRule a rdf:Property ;
    rdfs:label "used rule" ;
    rdfs:domain gph:Transform ;
    rdfs:range gph:QualityRule ;
    rdfs:subPropertyOf prov:used .

gph:violates a rdf:Property ;
    rdfs:label "violates" ;
    rdfs:domain gph:DataRecord ;
    rdfs:range gph:QualityRule .

gph:hasViolation a rdf:Property ;
    rdfs:label "has violation" ;
    rdfs:domain gph:DataRecord ;
    rdfs:range gph:QualityViolation .

# ===== ML Properties =====

ml:usedModel a rdf:Property ;
    rdfs:label "used model" ;
    rdfs:domain prov:Activity ;
    rdfs:range ml:Model ;
    rdfs:subPropertyOf prov:used .

ml:trainedOn a rdf:Property ;
    rdfs:label "trained on" ;
    rdfs:domain ml:Model ;
    rdfs:range ml:TrainingDataset .

ml:hasVersion a rdf:Property ;
    rdfs:label "has version" ;
    rdfs:domain ml:Model ;
    rdfs:range ml:ModelVersion .

ml:influencedData a rdf:Property ;
    rdfs:label "influenced data" ;
    rdfs:domain ml:Model ;
    rdfs:range gph:DataRecord ;
    rdfs:subPropertyOf prov:influenced .

ml:hasMetric a rdf:Property ;
    rdfs:label "has metric" ;
    rdfs:domain ml:Model ;
    rdfs:range xsd:double .

ml:paramsHash a rdf:Property ;
    rdfs:label "parameters hash" ;
    rdfs:domain ml:ModelVersion ;
    rdfs:range xsd:string .
"#;

/// Convert lineage event to RDF triples
pub fn lineage_to_rdf(event: &graphica_core::core::lineage::LineageEvent) -> String {
    let base_uri = format!("http://graphica.ai/data/lineage/{}", event.id);

    let mut turtle = format!(
        r#"@prefix gph: <http://graphica.ai/ontology#> .
@prefix prov: <http://www.w3.org/ns/prov#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
@prefix ml: <http://graphica.ai/ml#> .

<{base_uri}> a gph:LineageEvent ;
    gph:dataset "{dataset}" ;
    gph:recordId "{record_id}" ;
    prov:atTime "{timestamp}"^^xsd:dateTime ;
    prov:runId "{run_id}" .
"#,
        base_uri = base_uri,
        dataset = event.dataset,
        record_id = event.record_id,
        timestamp = event.ts.to_rfc3339(),
        run_id = event.run_id
    );

    // Add source references
    for (i, source) in event.source_refs.iter().enumerate() {
        turtle.push_str(&format!(
            "\n<{base_uri}> prov:used <{source_uri}> .\n<{source_uri}> a prov:Entity ;\n    gph:system \"{system}\" ;\n    gph:path \"{path}\" .",
            base_uri = base_uri,
            source_uri = format!("http://graphica.ai/data/source/{}", i),
            system = source.system,
            path = source.path
        ));
    }

    // Add model references
    for model_ref in &event.model_refs {
        turtle.push_str(&format!(
            "\n<{base_uri}> ml:usedModel <{model_uri}> .\n<{model_uri}> a ml:ModelVersion ;\n    ml:modelId \"{model_id}\" ;\n    ml:version \"{version}\" ;\n    ml:paramsHash \"{params_hash}\" .",
            base_uri = base_uri,
            model_uri = format!("http://graphica.ai/ml/model/{}/{}", model_ref.model_id, model_ref.version),
            model_id = model_ref.model_id,
            version = model_ref.version,
            params_hash = model_ref.params_hash
        ));
    }

    turtle
}