//! # Adapters for Existing Systems
//!
//! Adapter implementations that allow existing code to use the unified mapping engine
//! without breaking changes.

use anyhow::Result;
use std::sync::Arc;

use super::{FieldDescriptor, FieldStatistics, MappingOptions, UnifiedOntologyMappingEngine};
use crate::mapping::discovery::types::DiscoveredColumn;
use crate::mapping::ontology_ddl::types::{FieldOntologyMapping, MappingMethod};

// ============================================================================
// Adapter for OntologyDdlOrchestrator
// ============================================================================

/// Adapter that allows OntologyDdlOrchestrator to use the unified mapper
pub struct OntologyDdlAdapter {
    unified_engine: Arc<UnifiedOntologyMappingEngine>,
}

impl OntologyDdlAdapter {
    pub fn new(unified_engine: Arc<UnifiedOntologyMappingEngine>) -> Self {
        Self { unified_engine }
    }

    /// Resolve mappings for discovered columns
    pub async fn resolve_mappings(
        &self,
        table_name: &str,
        columns: &[DiscoveredColumn],
        min_confidence: f64,
    ) -> Result<Vec<FieldOntologyMapping>> {
        let mut mappings = Vec::new();

        for column in columns {
            // Convert DiscoveredColumn to FieldDescriptor
            let descriptor = convert_discovered_column_to_field_descriptor(table_name, column);

            // Create mapping options
            let options = MappingOptions {
                min_confidence,
                max_candidates: 1, // Only need top candidate
                ontology_namespaces: None,
                enabled_strategies: None,
                use_cache: true,
                timeout_ms: Some(5000),
            };

            // Get candidates from unified engine
            let candidates = self.unified_engine.map_field(&descriptor, &options).await?;

            // Convert top candidate to FieldOntologyMapping
            if let Some(top) = candidates.first() {
                // Determine mapping method from evidence
                let method = determine_mapping_method_from_evidence(&top.evidence);

                mappings.push(FieldOntologyMapping {
                    field_id: descriptor.id,
                    field_name: column.name.clone(),
                    table_name: table_name.to_string(),
                    ontology_uri: top.ontology_uri.clone(),
                    confidence: top.confidence,
                    mapping_method: method,
                    mapped_at: chrono::Utc::now().timestamp(),
                });
            }
        }

        Ok(mappings)
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Convert DiscoveredColumn to FieldDescriptor for unified mapping
fn convert_discovered_column_to_field_descriptor(
    table_name: &str,
    column: &DiscoveredColumn,
) -> FieldDescriptor {
    // Calculate statistics from discovered column
    let statistics = Some(FieldStatistics {
        distinct_count: Some(column.statistics.distinct_count as u64),
        null_count: Some(
            (column.statistics.null_fraction * column.statistics.sample_count as f64) as u64,
        ),
        total_count: Some(column.statistics.sample_count as u64),
        min_length: None,
        max_length: None,
        avg_length: column.statistics.avg_length,
    });

    // Normalize field name (lowercase, remove special chars)
    let normalized_name = column
        .name
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect();

    FieldDescriptor {
        id: format!("{}_{}", table_name, column.name),
        name: column.name.clone(),
        normalized_name,
        data_type: column.data_type.clone(),
        nullable: column.nullable,
        primary_key: column.primary_key,
        sample_values: column.sample_values.clone(),
        description: column.semantic_type.clone(),
        source_id: "discovery".to_string(),
        table_name: table_name.to_string(),
        statistics,
    }
}

/// Determine mapping method from evidence
fn determine_mapping_method_from_evidence(
    evidence: &[super::types::StrategyMatch],
) -> MappingMethod {
    // Find the strategy with the highest confidence
    let primary_strategy = evidence
        .iter()
        .max_by(|a, b| a.confidence.partial_cmp(&b.confidence).unwrap())
        .map(|e| e.strategy_name.as_str());

    match primary_strategy {
        Some("pattern") => MappingMethod::PatternInference,
        Some("registry") => MappingMethod::RegistryMatching,
        Some("semantic") => MappingMethod::Semantic,
        Some("statistical") => MappingMethod::Statistical,
        Some("lexical") | Some("heuristic") => MappingMethod::Hybrid,
        _ => MappingMethod::Hybrid,
    }
}

// ============================================================================
// Factory Functions
// ============================================================================

/// Create OntologyDdlAdapter with unified engine
pub async fn create_ontology_ddl_adapter(
    config: super::types::UnifiedMappingConfig,
    registry_client: Option<Arc<crate::mapping::ontology_registry::RegistryClient>>,
    // semantic_client: Option<Arc<crate::mapping::semantic::SemanticMatcherClient>>, // PRE-EXISTING ISSUE
) -> Result<OntologyDdlAdapter> {
    // Create unified engine (PRE-EXISTING ISSUE: semantic_client parameter removed)
    let unified_engine =
        Arc::new(UnifiedOntologyMappingEngine::new(config, registry_client, None).await?);

    // Create adapter
    Ok(OntologyDdlAdapter::new(unified_engine))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::discovery::types::ColumnStatistics;

    #[test]
    fn test_convert_discovered_column_to_field_descriptor() {
        let column = DiscoveredColumn {
            name: "user_email".to_string(),
            data_type: "VARCHAR".to_string(),
            nullable: false,
            primary_key: false,
            semantic_type: Some("email".to_string()),
            confidence: 0.95,
            patterns: vec![],
            statistics: ColumnStatistics {
                distinct_count: 100,
                null_fraction: 0.0,
                sample_count: 100,
                most_common_values: None,
                avg_length: Some(25.0),
                min_value: None,
                max_value: None,
            },
            sample_values: vec!["test@example.com".to_string()],
        };

        let field = convert_discovered_column_to_field_descriptor("users", &column);

        assert_eq!(field.id, "users_user_email");
        assert_eq!(field.name, "user_email");
        assert_eq!(field.normalized_name, "useremail");
        assert_eq!(field.data_type, "VARCHAR");
        assert!(!field.nullable);
        assert!(!field.primary_key);
        assert_eq!(field.sample_values, vec!["test@example.com"]);
        assert_eq!(field.table_name, "users");
        assert!(field.statistics.is_some());
        assert_eq!(field.statistics.as_ref().unwrap().distinct_count, Some(100));
    }

    #[test]
    fn test_determine_mapping_method_from_evidence() {
        use super::super::types::StrategyMatch;
        use std::collections::HashMap;

        // Test pattern strategy
        let evidence = vec![StrategyMatch {
            strategy_name: "pattern".to_string(),
            ontology_uri: "http://schema.org/email".to_string(),
            confidence: 0.9,
            explanation: "".to_string(),
            metadata: HashMap::new(),
        }];
        assert_eq!(
            determine_mapping_method_from_evidence(&evidence),
            MappingMethod::PatternInference
        );

        // Test registry strategy
        let evidence = vec![StrategyMatch {
            strategy_name: "registry".to_string(),
            ontology_uri: "http://example.com/CustomTerm".to_string(),
            confidence: 0.85,
            explanation: "".to_string(),
            metadata: HashMap::new(),
        }];
        assert_eq!(
            determine_mapping_method_from_evidence(&evidence),
            MappingMethod::RegistryMatching
        );

        // Test semantic strategy
        let evidence = vec![StrategyMatch {
            strategy_name: "semantic".to_string(),
            ontology_uri: "http://schema.org/name".to_string(),
            confidence: 0.8,
            explanation: "".to_string(),
            metadata: HashMap::new(),
        }];
        assert_eq!(
            determine_mapping_method_from_evidence(&evidence),
            MappingMethod::Semantic
        );

        // Test statistical strategy
        let evidence = vec![StrategyMatch {
            strategy_name: "statistical".to_string(),
            ontology_uri: "http://schema.org/name".to_string(),
            confidence: 0.75,
            explanation: "".to_string(),
            metadata: HashMap::new(),
        }];
        assert_eq!(
            determine_mapping_method_from_evidence(&evidence),
            MappingMethod::Statistical
        );

        // Test hybrid (lexical/heuristic)
        let evidence = vec![StrategyMatch {
            strategy_name: "lexical".to_string(),
            ontology_uri: "http://schema.org/name".to_string(),
            confidence: 0.7,
            explanation: "".to_string(),
            metadata: HashMap::new(),
        }];
        assert_eq!(
            determine_mapping_method_from_evidence(&evidence),
            MappingMethod::Hybrid
        );
    }
}
