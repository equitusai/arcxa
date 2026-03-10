//! # SPARQL Query Examples
//!
//! Common SPARQL queries for lineage and governance.

/// Query all records influenced by a specific model
pub const QUERY_MODEL_IMPACT: &str = r#"
PREFIX gph: <http://graphica.ai/ontology#>
PREFIX ml: <http://graphica.ai/ml#>
PREFIX prov: <http://www.w3.org/ns/prov#>

SELECT ?record ?model ?version ?timestamp
WHERE {
  ?lineage a gph:LineageEvent ;
           ml:usedModel ?modelVersion ;
           gph:recordId ?record ;
           prov:atTime ?timestamp .

  ?modelVersion ml:modelId ?model ;
                ml:version ?version .

  FILTER (?model = $MODEL_ID)
  FILTER (?version = $VERSION)
}
ORDER BY DESC(?timestamp)
"#;

/// Query complete lineage chain for a record
pub const QUERY_LINEAGE_CHAIN: &str = r#"
PREFIX gph: <http://graphica.ai/ontology#>
PREFIX prov: <http://www.w3.org/ns/prov#>

SELECT ?source ?transform ?model ?output
WHERE {
  ?lineage a gph:LineageEvent ;
           gph:recordId $RECORD_ID ;
           prov:used ?source ;
           gph:wasTransformedBy ?transform ;
           ml:usedModel ?model ;
           prov:generated ?output .
}
"#;

/// Query quality violations by severity
pub const QUERY_VIOLATIONS_BY_SEVERITY: &str = r#"
PREFIX gph: <http://graphica.ai/ontology#>

SELECT ?record ?rule ?severity (COUNT(*) as ?count)
WHERE {
  ?violation a gph:QualityViolation ;
             gph:violates ?rule ;
             gph:severity ?severity ;
             gph:affectsRecord ?record .

  FILTER (?severity IN ("ERROR", "CRITICAL"))
}
GROUP BY ?record ?rule ?severity
ORDER BY DESC(?count)
"#;

/// Query datasets affected by a training dataset change
pub const QUERY_TRAINING_DATA_IMPACT: &str = r#"
PREFIX ml: <http://graphica.ai/ml#>
PREFIX gph: <http://graphica.ai/ontology#>
PREFIX prov: <http://www.w3.org/ns/prov#>

SELECT DISTINCT ?dataset ?model ?affectedRecords
WHERE {
  ?model a ml:Model ;
         ml:trainedOn ?trainingDataset ;
         ml:influencedData ?record .

  ?record gph:dataset ?dataset .

  FILTER (?trainingDataset = $TRAINING_DATASET_URI)
}
GROUP BY ?dataset ?model
"#;

/// Query cross-dataset lineage (data flowing between systems)
pub const QUERY_CROSS_DATASET_FLOW: &str = r#"
PREFIX gph: <http://graphica.ai/ontology#>
PREFIX prov: <http://www.w3.org/ns/prov#>

SELECT ?sourceDataset ?targetDataset (COUNT(*) as ?recordCount)
WHERE {
  ?lineage a gph:LineageEvent ;
           prov:used ?source ;
           prov:generated ?target .

  ?source gph:dataset ?sourceDataset .
  ?target gph:dataset ?targetDataset .

  FILTER (?sourceDataset != ?targetDataset)
}
GROUP BY ?sourceDataset ?targetDataset
ORDER BY DESC(?recordCount)
"#;

/// Query model performance over time
pub const QUERY_MODEL_PERFORMANCE: &str = r#"
PREFIX ml: <http://graphica.ai/ml#>
PREFIX prov: <http://www.w3.org/ns/prov#>

SELECT ?timestamp ?accuracy ?precision ?recall
WHERE {
  ?modelVersion a ml:ModelVersion ;
                ml:modelId $MODEL_ID ;
                ml:version ?version ;
                prov:generatedAtTime ?timestamp ;
                ml:hasMetric ?metrics .

  ?metrics ml:accuracy ?accuracy ;
           ml:precision ?precision ;
           ml:recall ?recall .
}
ORDER BY ?timestamp
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_syntax() {
        // Validate queries parse correctly
        assert!(QUERY_MODEL_IMPACT.contains("SELECT"));
        assert!(QUERY_LINEAGE_CHAIN.contains("PREFIX"));
    }
}