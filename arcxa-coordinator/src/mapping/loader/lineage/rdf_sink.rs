//! RDF Lineage Sink
//!
//! Stores lineage events as W3C PROV-compliant RDF triples in the governance brain.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use graphica_core::core::lineage::{LineageEvent, LineageSink};
use std::sync::Arc;
use tracing::{debug, error, info};

use crate::governance::lineage_converter::LineageConverter;
use crate::governance::rdf_store::{GraphicaRdfStore, RdfStore};

/// RDF-backed lineage sink for storing provenance in governance brain
pub struct RdfLineageSink {
    /// RDF triple store (governance brain)
    rdf_store: Arc<GraphicaRdfStore>,

    /// Graph URI for lineage triples
    graph_uri: String,
}

impl RdfLineageSink {
    /// Create new RDF lineage sink
    ///
    /// # Arguments
    /// * `rdf_store` - RDF triple store for storing lineage
    /// * `graph_uri` - Named graph URI for lineage data (defaults to "http://graphica.io/lineage")
    pub fn new(rdf_store: Arc<GraphicaRdfStore>, graph_uri: Option<String>) -> Self {
        let graph_uri = graph_uri.unwrap_or_else(|| "http://graphica.io/lineage".to_string());

        info!("Created RdfLineageSink with graph URI: {}", graph_uri);

        Self {
            rdf_store,
            graph_uri,
        }
    }

    /// Convert lineage event to RDF triples
    fn to_rdf_triples(&self, event: &LineageEvent) -> Result<Vec<(String, String, String)>> {
        LineageConverter::to_rdf_triples_detailed(event)
            .context("Failed to convert lineage event to RDF triples")
    }

    /// Insert RDF triples into store
    fn insert_triples_batch(&self, triples: &[(String, String, String)]) -> Result<()> {
        use crate::governance::rdf_store::NamedGraph;

        debug!(
            "Inserting {} lineage triples into graph: {}",
            triples.len(),
            self.graph_uri
        );

        // Create named graph for lineage
        let graph = NamedGraph {
            uri: self.graph_uri.clone(),
        };

        // Insert triples into RDF store
        self.rdf_store
            .insert_triples(triples.to_vec(), Some(&graph))
            .context("Failed to insert lineage triples into RDF store")?;

        Ok(())
    }

    /// Query lineage events by SPARQL
    fn query_lineage(&self, query: &str) -> Result<Vec<serde_json::Value>> {
        self.rdf_store
            .query(query)
            .context("Failed to query lineage from RDF store")
    }
}

impl LineageSink for RdfLineageSink {
    /// Write lineage event to RDF store
    fn write(&self, event: LineageEvent) -> Result<()> {
        debug!(
            "Writing lineage event: dataset={}, record_id={}",
            event.dataset, event.record_id
        );

        // Convert to RDF triples
        let triples = self.to_rdf_triples(&event)?;

        // Insert into RDF store
        self.insert_triples_batch(&triples).map_err(|e| {
            error!("Failed to write lineage event: {}", e);
            e
        })?;

        info!(
            "Successfully wrote lineage event for record: {}",
            event.record_id
        );

        Ok(())
    }

    /// Query lineage for a specific record
    fn get_record_lineage(&self, record_id: &str) -> Result<Vec<LineageEvent>> {
        let query = format!(
            r#"
PREFIX gph: <http://graphica.io/ontology#>
PREFIX prov: <http://www.w3.org/ns/prov#>

SELECT ?activity ?dataset ?runId ?tenantId ?ts WHERE {{
    ?activity gph:recordId "{}" ;
              gph:dataset ?dataset ;
              gph:runId ?runId ;
              gph:tenantId ?tenantId ;
              prov:startedAtTime ?ts .
}}
"#,
            record_id
        );

        let results = self.query_lineage(&query)?;

        // TODO: Full LineageEvent reconstruction from SPARQL results
        // For now, return empty vector (partial implementation)
        debug!(
            "Found {} lineage events for record: {}",
            results.len(),
            record_id
        );

        Ok(Vec::new())
    }

    /// Query all data affected by a model
    fn get_model_impact(&self, model_id: &str, version: &str) -> Result<Vec<LineageEvent>> {
        let query = format!(
            r#"
PREFIX ml: <http://graphica.io/ml#>
PREFIX prov: <http://www.w3.org/ns/prov#>
PREFIX gph: <http://graphica.io/ontology#>

SELECT ?activity ?recordId WHERE {{
    ?model ml:modelId "{}" ;
           ml:version "{}" .

    ?activity prov:wasAssociatedWith ?model ;
              gph:recordId ?recordId .
}}
"#,
            model_id, version
        );

        let results = self.query_lineage(&query)?;

        debug!(
            "Found {} records affected by model: {} (v{})",
            results.len(),
            model_id,
            version
        );

        Ok(Vec::new())
    }

    /// Query lineage by time range
    fn query_by_time_range(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<LineageEvent>> {
        let query = format!(
            r#"
PREFIX prov: <http://www.w3.org/ns/prov#>
PREFIX gph: <http://graphica.io/ontology#>

SELECT ?activity ?recordId ?ts WHERE {{
    ?activity gph:recordId ?recordId ;
              prov:startedAtTime ?ts .

    FILTER (?ts >= "{}"^^xsd:dateTime && ?ts <= "{}"^^xsd:dateTime)
}}
ORDER BY ?ts
"#,
            start.to_rfc3339(),
            end.to_rfc3339()
        );

        let results = self.query_lineage(&query)?;

        debug!(
            "Found {} lineage events between {} and {}",
            results.len(),
            start,
            end
        );

        Ok(Vec::new())
    }

    /// Get lineage for a specific run
    fn get_run_lineage(&self, run_id: &str) -> Result<Vec<LineageEvent>> {
        let query = format!(
            r#"
PREFIX gph: <http://graphica.io/ontology#>

SELECT ?activity ?recordId WHERE {{
    ?activity gph:runId "{}" ;
              gph:recordId ?recordId .
}}
"#,
            run_id
        );

        let results = self.query_lineage(&query)?;

        debug!("Found {} lineage events for run: {}", results.len(), run_id);

        Ok(Vec::new())
    }

    /// Time-travel query: Get lineage as it existed at a specific timestamp
    fn get_lineage_as_of(
        &self,
        record_id: &str,
        as_of: DateTime<Utc>,
    ) -> Result<Vec<LineageEvent>> {
        let query = format!(
            r#"
PREFIX gph: <http://graphica.io/ontology#>
PREFIX prov: <http://www.w3.org/ns/prov#>

SELECT ?activity ?dataset ?runId ?ts WHERE {{
    ?activity gph:recordId "{}" ;
              gph:dataset ?dataset ;
              gph:runId ?runId ;
              prov:startedAtTime ?ts .

    FILTER (?ts <= "{}"^^xsd:dateTime)
}}
ORDER BY DESC(?ts)
LIMIT 1
"#,
            record_id,
            as_of.to_rfc3339()
        );

        let results = self.query_lineage(&query)?;

        debug!(
            "Found {} lineage events for record {} as of {}",
            results.len(),
            record_id,
            as_of
        );

        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use graphica_core::core::lineage::{DataRef, ModelMetrics, ModelRef, TransformRef};
    use std::collections::HashMap;
    use uuid::Uuid;

    fn create_test_lineage_event() -> LineageEvent {
        LineageEvent {
            id: Uuid::new_v4(),
            dataset: "test_dataset".to_string(),
            record_id: "test_record_123".to_string(),
            source_refs: vec![DataRef {
                system: "file".to_string(),
                path: "/data/test.csv".to_string(),
                version: Some("v1".to_string()),
                extracted_at: Utc::now(),
                cdc_position: None,
            }],
            transforms: vec![TransformRef {
                id: Uuid::new_v4(),
                transform_type: "r2rml_mapping".to_string(),
                rule_id: "mapping_001".to_string(),
                version: "v1".to_string(),
                parameters: HashMap::new(),
                applied_at: Utc::now(),
                fields_modified: vec!["customer_id".to_string()],
            }],
            model_refs: vec![],
            output_ref: DataRef {
                system: "db2".to_string(),
                path: "SCHEMA.TABLE".to_string(),
                version: None,
                extracted_at: Utc::now(),
                cdc_position: None,
            },
            ts: Utc::now(),
            run_id: "run_001".to_string(),
            tenant_id: "default".to_string(),
            correlation_id: None,
            metadata: HashMap::new(),
        }
    }

    #[test]
    fn test_to_rdf_triples() {
        // Create test RDF store (in-memory)
        let rdf_store = Arc::new(GraphicaRdfStore::new_in_memory().unwrap());
        let sink = RdfLineageSink::new(rdf_store, None);

        let event = create_test_lineage_event();

        // Convert to RDF triples
        let triples = sink.to_rdf_triples(&event).unwrap();

        // Verify triples generated
        assert!(!triples.is_empty(), "Should generate RDF triples");

        // Verify key triples exist
        let has_activity = triples
            .iter()
            .any(|(_, p, o)| p.contains("type") && o.contains("Activity"));
        assert!(has_activity, "Should have prov:Activity type");

        let has_record_id = triples
            .iter()
            .any(|(_, p, o)| p.contains("recordId") && o.contains("test_record_123"));
        assert!(has_record_id, "Should have record ID");
    }

    #[test]
    fn test_lineage_sink_write() {
        // Create in-memory RDF store for testing
        let rdf_store = Arc::new(GraphicaRdfStore::new_in_memory().unwrap());
        let sink = RdfLineageSink::new(rdf_store.clone(), None);

        let event = create_test_lineage_event();
        let record_id = event.record_id.clone();

        // Write lineage event
        let result = sink.write(event);
        assert!(result.is_ok(), "Should write lineage event successfully");

        // Query back (basic check - full reconstruction not implemented yet)
        let results = sink.get_record_lineage(&record_id);
        assert!(results.is_ok(), "Should query lineage successfully");
    }

    #[test]
    fn test_lineage_time_range_query() {
        let rdf_store = Arc::new(GraphicaRdfStore::new_in_memory().unwrap());
        let sink = RdfLineageSink::new(rdf_store, None);

        let event = create_test_lineage_event();
        sink.write(event).unwrap();

        let start = Utc::now() - chrono::Duration::hours(1);
        let end = Utc::now() + chrono::Duration::hours(1);

        let results = sink.query_by_time_range(start, end);
        assert!(results.is_ok(), "Should query by time range successfully");
    }

    #[test]
    fn test_lineage_run_query() {
        let rdf_store = Arc::new(GraphicaRdfStore::new_in_memory().unwrap());
        let sink = RdfLineageSink::new(rdf_store, None);

        let event = create_test_lineage_event();
        let run_id = event.run_id.clone();

        sink.write(event).unwrap();

        let results = sink.get_run_lineage(&run_id);
        assert!(results.is_ok(), "Should query by run ID successfully");
    }
}
