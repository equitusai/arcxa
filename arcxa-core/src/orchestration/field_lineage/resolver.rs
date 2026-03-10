//! Field Resolution Engine
//!
//! High-level API for resolving field values from multiple sources
//! using voting strategies and persisting to RDF.

use super::storage::FieldLineageStore;
use super::types::*;
use super::voting::VotingEngine;
use anyhow::{Context, Result};
use chrono::Utc;
use std::collections::HashMap;

/// Field resolver for resolved entity creation
pub struct FieldResolver {
    /// Voting engine
    voting_engine: VotingEngine,

    /// Storage layer
    storage: FieldLineageStore,

    /// Default confidence threshold
    min_confidence: f64,
}

impl FieldResolver {
    /// Create a new field resolver with default frequency voting
    pub fn new() -> Self {
        Self {
            voting_engine: VotingEngine::new(StrategyType::Frequency),
            storage: FieldLineageStore::new(),
            min_confidence: 0.70,
        }
    }

    /// Create with custom strategy
    pub fn with_strategy(strategy: StrategyType) -> Self {
        Self {
            voting_engine: VotingEngine::new(strategy),
            storage: FieldLineageStore::new(),
            min_confidence: strategy.default_confidence_threshold(),
        }
    }

    /// Set minimum confidence threshold
    pub fn with_min_confidence(mut self, min_confidence: f64) -> Self {
        self.min_confidence = min_confidence;
        self
    }

    /// Resolve a single field from multiple sources
    pub fn resolve_field(
        &self,
        entity_id: &str,
        field_name: &str,
        source_values: Vec<SourceValue>,
        strategy: Option<VotingStrategy>,
    ) -> Result<FieldResolution> {
        if source_values.is_empty() {
            anyhow::bail!("No source values provided for field '{}'", field_name);
        }

        // Resolve using voting engine
        let resolution =
            self.voting_engine
                .resolve_field(entity_id, field_name, source_values, strategy)?;

        // Check confidence threshold
        let confidence = resolution.selected_value.vote_weight
            / resolution
                .source_values
                .iter()
                .map(|s| s.vote_weight.max(1.0))
                .sum::<f64>();

        if confidence < self.min_confidence {
            // Low confidence - may require review
            tracing::warn!(
                "Low confidence ({:.2}) for field '{}'.'{}' (threshold: {:.2})",
                confidence,
                entity_id,
                field_name,
                self.min_confidence
            );
        }

        Ok(resolution)
    }

    /// Resolve multiple fields for an entity
    pub fn resolve_fields(
        &self,
        entity_id: &str,
        fields: HashMap<String, Vec<SourceValue>>,
        strategy: Option<VotingStrategy>,
    ) -> Result<Vec<FieldResolution>> {
        let mut resolutions = Vec::new();

        for (field_name, source_values) in fields {
            match self.resolve_field(entity_id, &field_name, source_values, strategy.clone()) {
                Ok(resolution) => resolutions.push(resolution),
                Err(e) => {
                    tracing::error!("Failed to resolve field '{}': {}", field_name, e);
                    // Continue with other fields
                }
            }
        }

        Ok(resolutions)
    }

    /// Create a resolved entity from field resolutions
    pub fn create_resolved_entity(
        &self,
        entity_id: &str,
        resolutions: Vec<FieldResolution>,
    ) -> Result<ResolvedEntity> {
        let mut fields = HashMap::new();
        let mut total_confidence = 0.0;
        let mut conflict_count = 0;
        let mut review_required = false;

        for resolution in &resolutions {
            // Create field value
            let field_value = FieldValue {
                entity_id: entity_id.to_string(),
                field_name: resolution.field_name.clone(),
                value: resolution.selected_value.value.clone(),
                value_type: infer_value_type(&resolution.selected_value.value),
                confidence: resolution.selected_value.vote_weight
                    / resolution
                        .source_values
                        .iter()
                        .map(|s| s.vote_weight.max(1.0))
                        .sum::<f64>(),
                resolved_at: resolution.resolved_at,
                valid_from: resolution.resolved_at,
                valid_to: None,
                supersedes: None,
                explanation: Some(resolution.explanation.clone()),
                resolution_id: resolution.id.clone(),
            };

            total_confidence += field_value.confidence;
            fields.insert(resolution.field_name.clone(), field_value);

            // Check for conflicts
            if resolution.conflict.is_some() {
                conflict_count += 1;
                if resolution.conflict.as_ref().unwrap().requires_review {
                    review_required = true;
                }
            }
        }

        let avg_confidence = if !resolutions.is_empty() {
            total_confidence / resolutions.len() as f64
        } else {
            0.0
        };

        Ok(ResolvedEntity {
            entity_id: entity_id.to_string(),
            fields,
            resolutions,
            overall_confidence: avg_confidence,
            created_at: Utc::now(),
            conflict_count,
            requires_review: review_required,
        })
    }

    /// Generate SPARQL insert query for resolved entity
    pub fn resolved_entity_to_sparql(&self, resolved_entity: &ResolvedEntity) -> String {
        let mut queries = Vec::new();

        // Insert each resolution
        for resolution in &resolved_entity.resolutions {
            queries.push(self.storage.insert_field_resolution_query(resolution));
        }

        // Insert field values
        for field_value in resolved_entity.fields.values() {
            let triples = self.storage.field_value_to_triples(field_value);
            queries.push(format!(
                r#"
PREFIX field: <http://graphica.io/field#>
PREFIX prov: <http://www.w3.org/ns/prov#>

INSERT DATA {{
    GRAPH <http://graphica.io/graph/field-lineage> {{
        {}
    }}
}}
"#,
                triples
            ));
        }

        queries.join("\n\n")
    }

    /// Get field lineage query
    pub fn get_field_lineage_query(&self, entity_id: &str, field_name: &str) -> String {
        self.storage.query_field_lineage(entity_id, field_name)
    }

    /// Get field history query
    pub fn get_field_history_query(&self, entity_id: &str, field_name: &str) -> String {
        self.storage.query_field_history(entity_id, field_name)
    }

    /// Get conflicts requiring review query
    pub fn get_conflicts_query(&self) -> String {
        self.storage.query_conflicts_requiring_review()
    }
}

impl Default for FieldResolver {
    fn default() -> Self {
        Self::new()
    }
}

/// Resolved entity with consolidated fields from multiple sources
#[derive(Debug, Clone)]
pub struct ResolvedEntity {
    /// Entity ID
    pub entity_id: String,

    /// Resolved fields
    pub fields: HashMap<String, FieldValue>,

    /// All resolutions
    pub resolutions: Vec<FieldResolution>,

    /// Overall confidence (average of all fields)
    pub overall_confidence: f64,

    /// When resolved entity was created
    pub created_at: chrono::DateTime<Utc>,

    /// Number of conflicts encountered
    pub conflict_count: usize,

    /// Whether any conflict requires human review
    pub requires_review: bool,
}

impl ResolvedEntity {
    /// Get field value
    pub fn get_field(&self, field_name: &str) -> Option<&FieldValue> {
        self.fields.get(field_name)
    }

    /// Get field value as JSON
    pub fn get_field_value(&self, field_name: &str) -> Option<&serde_json::Value> {
        self.fields.get(field_name).map(|fv| &fv.value)
    }

    /// Get all field names
    pub fn field_names(&self) -> Vec<&str> {
        self.fields.keys().map(|s| s.as_str()).collect()
    }

    /// Get fields below confidence threshold
    pub fn low_confidence_fields(&self, threshold: f64) -> Vec<&str> {
        self.fields
            .iter()
            .filter(|(_, fv)| fv.confidence < threshold)
            .map(|(name, _)| name.as_str())
            .collect()
    }

    /// Get conflicting fields
    pub fn conflicting_fields(&self) -> Vec<&str> {
        self.resolutions
            .iter()
            .filter(|r| r.conflict.is_some())
            .map(|r| r.field_name.as_str())
            .collect()
    }

    /// Convert to JSON representation
    pub fn to_json(&self) -> serde_json::Value {
        let mut fields_json = serde_json::Map::new();
        for (name, field_value) in &self.fields {
            fields_json.insert(
                name.clone(),
                serde_json::json!({
                    "value": field_value.value,
                    "confidence": field_value.confidence,
                    "explanation": field_value.explanation,
                }),
            );
        }

        serde_json::json!({
            "entity_id": self.entity_id,
            "fields": fields_json,
            "overall_confidence": self.overall_confidence,
            "created_at": self.created_at.to_rfc3339(),
            "conflict_count": self.conflict_count,
            "requires_review": self.requires_review,
        })
    }
}

/// Infer JSON value type
fn infer_value_type(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::Bool(_) => "boolean".to_string(),
        serde_json::Value::Number(_) => "number".to_string(),
        serde_json::Value::String(_) => "string".to_string(),
        serde_json::Value::Array(_) => "array".to_string(),
        serde_json::Value::Object(_) => "object".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_sources(count: usize, value_prefix: &str) -> Vec<SourceValue> {
        (0..count)
            .map(|i| SourceValue {
                id: format!("src_{}", i),
                value: serde_json::json!(format!("{}{}", value_prefix, i % 2)),
                source_system: format!("System{}", i),
                source_timestamp: Utc::now(),
                source_authority: 0.8,
                confidence: Some(0.9),
                vote_count: 0,
                vote_weight: 1.0,
                metadata: HashMap::new(),
            })
            .collect()
    }

    #[test]
    fn test_resolve_field() {
        let resolver = FieldResolver::new();
        let sources = create_test_sources(5, "value");

        let result = resolver.resolve_field("cust_001", "email", sources, None);
        assert!(result.is_ok());

        let resolution = result.unwrap();
        assert_eq!(resolution.entity_id, "cust_001");
        assert_eq!(resolution.field_name, "email");
        assert!(!resolution.selected_value.value.is_null());
    }

    #[test]
    fn test_resolve_fields_multiple() {
        let resolver = FieldResolver::new();
        let mut fields = HashMap::new();

        fields.insert("email".to_string(), create_test_sources(3, "test@"));
        fields.insert("phone".to_string(), create_test_sources(4, "555-"));

        let result = resolver.resolve_fields("cust_001", fields, None);
        assert!(result.is_ok());

        let resolutions = result.unwrap();
        assert_eq!(resolutions.len(), 2);
    }

    #[test]
    fn test_create_resolved_entity() {
        let resolver = FieldResolver::new();
        let mut fields = HashMap::new();

        fields.insert(
            "email".to_string(),
            create_test_sources(5, "test@example.com"),
        );
        fields.insert("name".to_string(), create_test_sources(5, "John Smith"));

        let resolutions = resolver.resolve_fields("cust_001", fields, None).unwrap();
        let resolved_entity = resolver.create_resolved_entity("cust_001", resolutions);

        assert!(resolved_entity.is_ok());
        let re = resolved_entity.unwrap();

        assert_eq!(re.entity_id, "cust_001");
        assert_eq!(re.fields.len(), 2);
        assert!(re.overall_confidence > 0.0);
    }

    #[test]
    fn test_resolved_entity_field_access() {
        let resolver = FieldResolver::new();
        let mut fields = HashMap::new();

        fields.insert(
            "email".to_string(),
            create_test_sources(3, "test@example.com"),
        );

        let resolutions = resolver.resolve_fields("cust_001", fields, None).unwrap();
        let re = resolver
            .create_resolved_entity("cust_001", resolutions)
            .unwrap();

        assert!(re.get_field("email").is_some());
        assert!(re.get_field("nonexistent").is_none());

        let field_names = re.field_names();
        assert_eq!(field_names.len(), 1);
        assert!(field_names.contains(&"email"));
    }

    #[test]
    fn test_resolved_entity_to_json() {
        let resolver = FieldResolver::new();
        let mut fields = HashMap::new();

        fields.insert("email".to_string(), create_test_sources(2, "test@"));

        let resolutions = resolver.resolve_fields("cust_001", fields, None).unwrap();
        let re = resolver
            .create_resolved_entity("cust_001", resolutions)
            .unwrap();

        let json = re.to_json();
        assert!(json["entity_id"].as_str().unwrap() == "cust_001");
        assert!(json["fields"].is_object());
        assert!(json["overall_confidence"].is_number());
        assert!(json["created_at"].is_string());
    }

    #[test]
    fn test_infer_value_type() {
        assert_eq!(infer_value_type(&serde_json::json!(null)), "null");
        assert_eq!(infer_value_type(&serde_json::json!(true)), "boolean");
        assert_eq!(infer_value_type(&serde_json::json!(42)), "number");
        assert_eq!(infer_value_type(&serde_json::json!("text")), "string");
        assert_eq!(infer_value_type(&serde_json::json!([])), "array");
        assert_eq!(infer_value_type(&serde_json::json!({})), "object");
    }
}
