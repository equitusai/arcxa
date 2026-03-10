// SPARQL Query Templates for Graphica
//
// Pre-built SPARQL queries for common governance operations

use super::ontology::{GRAPHICA_NS, ML_NS, PROV_NS, WORKFLOW_NS};

pub struct SparqlTemplates;

impl SparqlTemplates {
    fn dataset_uri_candidates(dataset_id: &str) -> [String; 3] {
        [
            format!("http://graphica.io/ontology/dataset/{}", dataset_id),
            format!("{GRAPHICA_NS}dataset/{}", dataset_id),
            format!("gph:dataset/{}", dataset_id),
        ]
    }

    /// Get all derived attributes for an entity
    pub fn get_entity_attributes(entity_id: &str) -> String {
        format!(
            r#"
PREFIX gph: <{GRAPHICA_NS}>
PREFIX prov: <{PROV_NS}>

SELECT ?attrName ?value ?confidence ?model ?timestamp
WHERE {{
    <{GRAPHICA_NS}entity/{entity_id}> gph:hasDerivedAttribute ?attr .
    ?attr gph:attributeName ?attrName ;
          gph:value ?value ;
          gph:confidence ?confidence ;
          prov:wasGeneratedBy ?model ;
          prov:generatedAtTime ?timestamp .
}}
ORDER BY DESC(?timestamp)
"#
        )
    }

    /// Get model impact (all entities affected by a model)
    pub fn get_model_impact(model_id: &str) -> String {
        format!(
            r#"
PREFIX gph: <{GRAPHICA_NS}>
PREFIX prov: <{PROV_NS}>
PREFIX ml: <{ML_NS}>

SELECT ?entity ?attrName ?confidence ?timestamp
WHERE {{
    ?attr a gph:DerivedAttribute ;
          prov:wasGeneratedBy <{ML_NS}model/{model_id}> ;
          gph:attributeName ?attrName ;
          gph:confidence ?confidence ;
          prov:generatedAtTime ?timestamp .
    ?entity gph:hasDerivedAttribute ?attr .
}}
ORDER BY ?entity ?timestamp
"#
        )
    }

    /// Find low confidence attributes
    pub fn find_low_confidence_attributes(threshold: f64) -> String {
        format!(
            r#"
PREFIX gph: <{GRAPHICA_NS}>
PREFIX prov: <{PROV_NS}>

SELECT ?entity ?attrName ?confidence ?model
WHERE {{
    ?entity gph:hasDerivedAttribute ?attr .
    ?attr gph:attributeName ?attrName ;
          gph:confidence ?confidence ;
          prov:wasGeneratedBy ?model .
    FILTER (?confidence < {threshold})
}}
ORDER BY ?confidence
LIMIT 100
"#
        )
    }

    /// Get fusion history for an entity
    pub fn get_fusion_history(entity_id: &str) -> String {
        format!(
            r#"
PREFIX gph: <{GRAPHICA_NS}>
PREFIX prov: <{PROV_NS}>

SELECT ?fusion ?sourceEntity ?rule ?confidence ?timestamp ?reversed
FROM <http://graphica.io/graph/fusion>
WHERE {{
    ?fusion a gph:FusionOperation ;
            gph:mergedEntity <{GRAPHICA_NS}entity/{entity_id}> ;
            gph:sourceEntity ?sourceEntity ;
            gph:fusionRule ?rule ;
            gph:fusionConfidence ?confidence ;
            prov:atTime ?timestamp .
    OPTIONAL {{ ?fusion gph:reversedAt ?reversed }}
}}
ORDER BY DESC(?timestamp)
"#
        )
    }

    /// Time-travel query (entity state as-of date)
    pub fn get_entity_as_of(entity_id: &str, date: &str) -> String {
        format!(
            r#"
PREFIX gph: <{GRAPHICA_NS}>

SELECT ?attrName ?value ?confidence
FROM <http://graphica.io/graph/{date}>
WHERE {{
    <{GRAPHICA_NS}entity/{entity_id}> gph:hasDerivedAttribute ?attr .
    ?attr gph:attributeName ?attrName ;
          gph:value ?value ;
          gph:confidence ?confidence .
}}
"#
        )
    }

    /// Get attribute evolution over time
    pub fn get_attribute_evolution(entity_id: &str, attribute_name: &str) -> String {
        format!(
            r#"
PREFIX gph: <{GRAPHICA_NS}>
PREFIX prov: <{PROV_NS}>

SELECT ?timestamp ?value ?confidence ?model
WHERE {{
    <{GRAPHICA_NS}entity/{entity_id}> gph:hasDerivedAttribute ?attr .
    ?attr gph:attributeName "{attribute_name}" ;
          gph:value ?value ;
          gph:confidence ?confidence ;
          prov:wasGeneratedBy ?model ;
          prov:generatedAtTime ?timestamp .
}}
ORDER BY ?timestamp
"#
        )
    }

    /// Get lineage graph for an entity (W3C PROV)
    pub fn get_entity_lineage(entity_id: &str) -> String {
        format!(
            r#"
PREFIX gph: <{GRAPHICA_NS}>
PREFIX prov: <{PROV_NS}>

CONSTRUCT {{
    ?s ?p ?o .
}}
WHERE {{
    {{
        <{GRAPHICA_NS}entity/{entity_id}> ?p ?o .
        BIND(<{GRAPHICA_NS}entity/{entity_id}> AS ?s)
    }}
    UNION
    {{
        <{GRAPHICA_NS}entity/{entity_id}> prov:wasGeneratedBy ?activity .
        ?activity ?p ?o .
        BIND(?activity AS ?s)
    }}
    UNION
    {{
        <{GRAPHICA_NS}entity/{entity_id}> prov:wasGeneratedBy/prov:used ?used .
        ?used ?p ?o .
        BIND(?used AS ?s)
    }}
}}
"#
        )
    }

    /// Count entities by type
    pub fn count_entities_by_type() -> String {
        format!(
            r#"
PREFIX gph: <{GRAPHICA_NS}>

SELECT ?entityType (COUNT(?entity) AS ?count)
WHERE {{
    ?entity a gph:Entity ;
            gph:entityType ?entityType .
}}
GROUP BY ?entityType
ORDER BY DESC(?count)
"#
        )
    }

    /// Find entities affected by model retraining
    pub fn find_stale_predictions(model_id: &str, since: &str) -> String {
        format!(
            r#"
PREFIX gph: <{GRAPHICA_NS}>
PREFIX prov: <{PROV_NS}>
PREFIX ml: <{ML_NS}>

SELECT DISTINCT ?entity
WHERE {{
    ?attr prov:wasGeneratedBy <{ML_NS}model/{model_id}> ;
          prov:generatedAtTime ?timestamp .
    ?entity gph:hasDerivedAttribute ?attr .
    FILTER (?timestamp < "{since}"^^xsd:dateTime)
}}
"#
        )
    }

    /// List all datasets with metadata
    pub fn list_datasets(filter_clause: Option<&str>, limit: usize, offset: usize) -> String {
        let filter_clause = filter_clause.unwrap_or_default();

        format!(
            r#"
PREFIX gph: <{GRAPHICA_NS}>
PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>

SELECT ?dataset ?name ?type ?recordCount ?qualityScore ?createdAt ?updatedAt ?sourceDataSource ?workflow
WHERE {{
    ?dataset a gph:Dataset ;
             gph:datasetName ?name ;
             gph:datasetType ?type ;
             gph:recordCount ?recordCount ;
             gph:createdAt ?createdAt ;
             gph:updatedAt ?updatedAt .
    OPTIONAL {{ ?dataset gph:qualityScore ?qualityScore }}
    OPTIONAL {{ ?dataset gph:sourceDataSource ?sourceDataSource }}
    OPTIONAL {{ ?dataset gph:producedByWorkflow ?workflow }}
    {filter_clause}
}}
ORDER BY DESC(?updatedAt)
LIMIT {limit}
OFFSET {offset}
"#
        )
    }

    /// Get dataset by ID with full metadata
    pub fn get_dataset_by_id(dataset_id: &str) -> String {
        let [slash_uri, fragment_uri, legacy_uri] = Self::dataset_uri_candidates(dataset_id);
        format!(
            r#"
PREFIX gph: <{GRAPHICA_NS}>
PREFIX workflow: <{WORKFLOW_NS}>

SELECT ?dataset ?property ?value
WHERE {{
    VALUES ?dataset {{ <{slash_uri}> <{fragment_uri}> <{legacy_uri}> }}
    ?dataset ?property ?value .
}}
"#
        )
    }

    /// Get dataset schema (columns)
    pub fn get_dataset_schema(dataset_id: &str) -> String {
        let [slash_uri, fragment_uri, legacy_uri] = Self::dataset_uri_candidates(dataset_id);
        format!(
            r#"
PREFIX gph: <{GRAPHICA_NS}>

SELECT ?columnName ?columnType ?nullable ?distinctCount ?nullPercentage
WHERE {{
    VALUES ?dataset {{ <{slash_uri}> <{fragment_uri}> <{legacy_uri}> }}
    ?dataset gph:hasColumn ?column .
    ?column gph:columnName ?columnName ;
            gph:columnType ?columnType ;
            gph:nullable ?nullable .
    OPTIONAL {{ ?column gph:distinctCount ?distinctCount }}
    OPTIONAL {{ ?column gph:nullPercentage ?nullPercentage }}
}}
ORDER BY ?columnName
"#
        )
    }

    /// Get dataset lineage (workflow executions, sources)
    pub fn get_dataset_lineage(dataset_id: &str) -> String {
        let [slash_uri, fragment_uri, legacy_uri] = Self::dataset_uri_candidates(dataset_id);
        format!(
            r#"
PREFIX gph: <{GRAPHICA_NS}>
PREFIX workflow: <{WORKFLOW_NS}>
PREFIX prov: <{PROV_NS}>

SELECT ?workflow ?workflowName ?executedAt ?sourceDataSource
WHERE {{
    VALUES ?dataset {{ <{slash_uri}> <{fragment_uri}> <{legacy_uri}> }}
    ?dataset a gph:Dataset .
    OPTIONAL {{
        ?dataset gph:producedByWorkflow ?workflow .
        OPTIONAL {{ ?workflow workflow:workflowName ?executionWorkflowName }}
        OPTIONAL {{
            ?workflow workflow:executedWorkflow ?workflowDefinition .
            ?workflowDefinition workflow:workflowName ?definitionWorkflowName .
        }}
        OPTIONAL {{ ?workflow prov:endedAtTime ?executedAt . }}
        BIND(COALESCE(?executionWorkflowName, ?definitionWorkflowName) AS ?workflowName)
    }}
    OPTIONAL {{
        ?dataset gph:sourceDataSource ?sourceDataSource .
    }}
}}
"#
        )
    }

    /// Get datasets by workflow execution
    pub fn get_datasets_by_workflow(workflow_execution_id: &str) -> String {
        format!(
            r#"
PREFIX gph: <{GRAPHICA_NS}>
PREFIX workflow: <{WORKFLOW_NS}>

SELECT ?dataset ?name ?recordCount
WHERE {{
    ?dataset a gph:Dataset ;
             gph:datasetName ?name ;
             gph:recordCount ?recordCount ;
             gph:producedByWorkflow <{WORKFLOW_NS}execution/{workflow_execution_id}> .
}}
"#
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entity_attributes_query() {
        let query = SparqlTemplates::get_entity_attributes("cust_123");
        assert!(query.contains("cust_123"));
        assert!(query.contains("hasDerivedAttribute"));
    }

    #[test]
    fn test_model_impact_query() {
        let query = SparqlTemplates::get_model_impact("mdl_456");
        assert!(query.contains("mdl_456"));
        assert!(query.contains("wasGeneratedBy"));
    }

    #[test]
    fn test_fusion_history_query() {
        let query = SparqlTemplates::get_fusion_history("cust_789");
        assert!(query.contains("FusionOperation"));
        assert!(query.contains("graph/fusion"));
    }

    #[test]
    fn test_list_datasets_with_filter_clause() {
        let query =
            SparqlTemplates::list_datasets(Some("FILTER (?datasetType != \"source\")"), 50, 0);
        assert!(query.contains("FILTER (?datasetType != \"source\")"));
    }

    #[test]
    fn test_list_datasets_without_filter_clause() {
        let query = SparqlTemplates::list_datasets(None, 50, 0);
        assert!(!query.contains("datasetType !="));
    }
}
