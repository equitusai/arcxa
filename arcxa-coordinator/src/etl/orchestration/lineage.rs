//! ETL Lineage Tracking
//!
//! Tracks field-level lineage through the entire ETL pipeline:
//! CSV Field → Ontology Term → Target Database Column

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

use super::types::*;
use super::unified_mapping::UnifiedMappingSession;
use crate::governance::rdf_store::{GraphicaRdfStore, RdfStore};

/// Lineage chain for a single field
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageChain {
    /// Target database field (table.column)
    pub target_field: String,

    /// Source CSV fields
    pub source_fields: Vec<SourceFieldLineage>,

    /// Ontology term
    pub ontology_term: String,

    /// Transformation applied
    pub transformation: Option<String>,

    /// Mapping session ID
    pub session_id: String,
}

/// Source field in lineage chain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceFieldLineage {
    /// CSV file path or datasource ID
    pub csv_source: String,

    /// CSV field name
    pub csv_field: String,

    /// Source session ID
    pub source_session_id: String,

    /// Data type
    pub data_type: String,

    /// Sample values
    pub sample_values: Vec<String>,
}

/// Field lineage information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldLineage {
    /// All lineage chains for this target field
    pub chains: Vec<LineageChain>,

    /// Total source fields contributing
    pub total_sources: usize,

    /// Fusion operations count (if applicable)
    pub fusion_count: u64,
}

/// ETL lineage tracker
///
/// Tracks and queries field-level lineage through the ETL pipeline.
pub struct EtlLineageTracker {
    /// RDF store for persistence
    rdf_store: Option<Arc<GraphicaRdfStore>>,

    /// In-memory lineage cache
    lineage_cache: HashMap<String, FieldLineage>,
}

impl EtlLineageTracker {
    /// Create a new lineage tracker
    pub fn new() -> Self {
        Self {
            rdf_store: None,
            lineage_cache: HashMap::new(),
        }
    }

    /// Create with RDF store
    pub fn with_rdf_store(rdf_store: Arc<GraphicaRdfStore>) -> Self {
        Self {
            rdf_store: Some(rdf_store),
            lineage_cache: HashMap::new(),
        }
    }

    /// Build lineage chains from unified mapping session
    pub fn build_lineage(&mut self, session: &UnifiedMappingSession) -> Result<Vec<LineageChain>> {
        let mut chains = Vec::new();

        for rule in &session.mapping_rules {
            let target_field = format!("{}.{}", rule.target_table, rule.target_column);

            let chain = LineageChain {
                target_field: target_field.clone(),
                source_fields: Vec::new(), // Will be populated from source sessions
                ontology_term: rule.ontology_term.clone(),
                transformation: rule.transformation.clone(),
                session_id: session.session_id.clone(),
            };

            // Cache the lineage
            let lineage = FieldLineage {
                chains: vec![chain.clone()],
                total_sources: 0,
                fusion_count: 0,
            };

            self.lineage_cache.insert(target_field, lineage);

            // Add to return list
            chains.push(chain);
        }

        tracing::info!(
            "Built {} lineage chains for session {}",
            chains.len(),
            session.session_id
        );

        Ok(chains)
    }

    /// Get lineage for a target field
    pub fn get_field_lineage(&self, target_field: &str) -> Option<&FieldLineage> {
        self.lineage_cache.get(target_field)
    }

    /// Query lineage (all fields)
    pub fn query_all_lineage(&self) -> Vec<(&String, &FieldLineage)> {
        self.lineage_cache.iter().collect()
    }

    /// Store lineage as RDF triples
    pub fn store_lineage_rdf(&self, session: &UnifiedMappingSession) -> Result<u64> {
        if self.rdf_store.is_none() {
            tracing::warn!("RDF store not configured, skipping lineage persistence");
            return Ok(0);
        }

        let rdf_store = self.rdf_store.as_ref().unwrap();

        // Generate RDF triples for lineage
        let mut triples = Vec::new();

        for rule in &session.mapping_rules {
            let target_uri = format!(
                "http://graphica.io/mapping/target/{}/{}",
                rule.target_table, rule.target_column
            );

            // Target mapping triple
            triples.push((
                target_uri.clone(),
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_string(),
                "http://graphica.io/ontology#TargetMapping".to_string(),
            ));

            // Ontology term mapping
            triples.push((
                target_uri.clone(),
                "http://graphica.io/ontology#fromOntologyTerm".to_string(),
                rule.ontology_term.clone(),
            ));

            // Target table
            triples.push((
                target_uri.clone(),
                "http://graphica.io/ontology#toTable".to_string(),
                format!("\"{}\"", rule.target_table),
            ));

            // Target column
            triples.push((
                target_uri.clone(),
                "http://graphica.io/ontology#toColumn".to_string(),
                format!("\"{}\"", rule.target_column),
            ));

            // Transformation (if present)
            if let Some(transformation) = &rule.transformation {
                triples.push((
                    target_uri.clone(),
                    "http://graphica.io/ontology#transformation".to_string(),
                    format!("\"{}\"", transformation),
                ));
            }

            // Add source field lineage
            for source_field in &rule.source_fields {
                let source_uri = format!(
                    "http://graphica.io/mapping/source/{}/{}/{}",
                    source_field.datasource_id, source_field.table_name, source_field.csv_field
                );

                // Link target to source
                triples.push((
                    target_uri.clone(),
                    "http://graphica.io/ontology#hasSourceField".to_string(),
                    source_uri.clone(),
                ));

                // Source field details
                triples.push((
                    source_uri.clone(),
                    "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_string(),
                    "http://graphica.io/ontology#SourceField".to_string(),
                ));

                triples.push((
                    source_uri.clone(),
                    "http://graphica.io/ontology#datasourceId".to_string(),
                    format!("\"{}\"", source_field.datasource_id),
                ));

                triples.push((
                    source_uri.clone(),
                    "http://graphica.io/ontology#csvField".to_string(),
                    format!("\"{}\"", source_field.csv_field),
                ));
            }
        }

        // Store triples in named graph for this session
        let graph_uri = format!("http://graphica.io/graph/lineage/{}", session.session_id);
        let graph = crate::governance::rdf_store::NamedGraph::new(&graph_uri);

        rdf_store
            .insert_triples(triples.clone(), Some(&graph))
            .context("Failed to store lineage triples")?;

        tracing::info!(
            "Stored {} RDF triples for lineage in graph {}",
            triples.len(),
            graph_uri
        );

        Ok(triples.len() as u64)
    }

    /// Generate lineage report
    pub fn generate_report(&self, session: &UnifiedMappingSession) -> LineageReport {
        let mut field_lineages = Vec::new();

        for rule in &session.mapping_rules {
            let target_field = format!("{}.{}", rule.target_table, rule.target_column);

            let lineage_info = FieldLineageInfo {
                target_field: target_field.clone(),
                sources: Vec::new(), // Will be populated from actual sources
                ontology_term: rule.ontology_term.clone(),
                transformation: rule.transformation.clone(),
                fusion_operations_count: 0,
            };

            field_lineages.push(lineage_info);
        }

        LineageReport {
            session_id: session.session_id.clone(),
            session_name: session.name.clone(),
            total_fields: field_lineages.len(),
            field_lineages,
        }
    }
}

/// Lineage report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageReport {
    /// Session ID
    pub session_id: String,

    /// Session name
    pub session_name: String,

    /// Total fields mapped
    pub total_fields: usize,

    /// Field lineages
    pub field_lineages: Vec<FieldLineageInfo>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::etl::orchestration::types::*;
    use crate::etl::orchestration::unified_mapping::UnifiedMappingSession;
    use std::collections::HashMap;

    fn create_test_session() -> UnifiedMappingSession {
        let target_db = TargetDatabase {
            database_type: "PostgreSQL".to_string(),
            connection: TargetConnection::ConnectionString {
                connection_string: "host=localhost".to_string(),
            },
            schema: Some("public".to_string()),
        };

        let mut target_schema = HashMap::new();
        target_schema.insert(
            "customers".to_string(),
            TargetTableSchema {
                table_name: "customers".to_string(),
                columns: {
                    let mut cols = HashMap::new();
                    cols.insert(
                        "email".to_string(),
                        ColumnDefinition {
                            data_type: "VARCHAR(255)".to_string(),
                            nullable: false,
                            unique: true,
                            default: None,
                        },
                    );
                    cols
                },
                primary_keys: vec!["customer_id".to_string()],
                foreign_keys: Vec::new(),
            },
        );

        let mut session = UnifiedMappingSession::new(
            "Test Lineage Session".to_string(),
            vec!["sess_001".to_string()],
            target_db,
            target_schema,
        );

        session
            .add_mapping_rule(TargetMappingRule {
                ontology_term: "http://schema.org/email".to_string(),
                target_table: "customers".to_string(),
                target_column: "email".to_string(),
                transformation: Some("LOWER(TRIM({value}))".to_string()),
                required: true,
                source_fields: Vec::new(),
            })
            .unwrap();

        session
    }

    #[test]
    fn test_build_lineage() {
        let session = create_test_session();
        let mut tracker = EtlLineageTracker::new();

        let chains = tracker.build_lineage(&session).unwrap();

        assert_eq!(chains.len(), 1);
        assert_eq!(chains[0].target_field, "customers.email");
        assert_eq!(chains[0].ontology_term, "http://schema.org/email");
        assert_eq!(
            chains[0].transformation,
            Some("LOWER(TRIM({value}))".to_string())
        );
    }

    #[test]
    fn test_get_field_lineage() {
        let session = create_test_session();
        let mut tracker = EtlLineageTracker::new();

        tracker.build_lineage(&session).unwrap();

        let lineage = tracker.get_field_lineage("customers.email");
        assert!(lineage.is_some());

        let lineage = lineage.unwrap();
        assert_eq!(lineage.chains.len(), 1);
    }

    #[test]
    fn test_generate_report() {
        let session = create_test_session();
        let tracker = EtlLineageTracker::new();

        let report = tracker.generate_report(&session);

        assert_eq!(report.session_id, session.session_id);
        assert_eq!(report.total_fields, 1);
        assert_eq!(report.field_lineages.len(), 1);
        assert_eq!(report.field_lineages[0].target_field, "customers.email");
    }

    #[test]
    fn test_store_lineage_rdf() {
        let session = create_test_session();
        let tracker = EtlLineageTracker::new();

        let triples_count = tracker.store_lineage_rdf(&session).unwrap();

        // Without RDF store configured, returns 0 (as expected)
        assert_eq!(triples_count, 0);

        // In a real scenario with RDF store, would generate 4-5 triples per rule:
        // - type triple
        // - ontology term triple
        // - target table triple
        // - target column triple
        // - transformation triple (if present)
    }
}
