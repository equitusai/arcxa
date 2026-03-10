// Enhanced Ontology Mapper Transformer with Manual Mapping Support
use crate::mapping::enhanced_engine::{EnhancedMappingEngine, MappingDecision, DecisionAction};
use crate::mapping::manual::types::SourceContext;
use crate::workflows::engine::{
    Transformer, TransformerConfig, TransformerContext, TransformerResult,
};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Configuration for enhanced ontology mapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnhancedOntologyMapperConfig {
    /// Source system identifier
    pub source_id: Option<String>,

    /// Table/entity name
    pub table_name: String,

    /// Field mapping overrides (field_name -> target_uri)
    pub field_overrides: HashMap<String, String>,

    /// Auto-apply mappings above this confidence
    pub auto_apply_threshold: f64,

    /// Enable interactive mode (ask user for low confidence)
    pub interactive_mode: bool,

    /// Record decisions for learning
    pub record_decisions: bool,

    /// Bulk mapping file path (optional)
    pub bulk_mapping_file: Option<String>,

    /// Mapping strategy
    pub strategy: MappingStrategy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MappingStrategy {
    /// Use manual mappings only
    ManualOnly,

    /// Use ML mappings only
    MLOnly,

    /// Prefer manual, fallback to ML (default)
    ManualWithMLFallback,

    /// Combine both with weighted scoring
    Weighted { manual_weight: f64, ml_weight: f64 },
}

impl Default for MappingStrategy {
    fn default() -> Self {
        Self::ManualWithMLFallback
    }
}

/// Enhanced ontology mapper transformer
pub struct EnhancedOntologyMapperTransformer {
    engine: Arc<EnhancedMappingEngine>,
    config: EnhancedOntologyMapperConfig,
    decision_buffer: Vec<MappingDecision>,
}

impl EnhancedOntologyMapperTransformer {
    pub fn new(
        engine: Arc<EnhancedMappingEngine>,
        config: EnhancedOntologyMapperConfig,
    ) -> Self {
        Self {
            engine,
            config,
            decision_buffer: Vec::new(),
        }
    }

    /// Load bulk mappings from file
    async fn load_bulk_mappings(&self) -> Result<HashMap<String, String>> {
        if let Some(ref file_path) = self.config.bulk_mapping_file {
            // Load CSV or JSON file with mappings
            let content = tokio::fs::read_to_string(file_path).await?;

            // Parse as JSON for now
            let mappings: HashMap<String, String> = serde_json::from_str(&content)?;

            info!("Loaded {} bulk mappings from {}", mappings.len(), file_path);
            return Ok(mappings);
        }

        Ok(HashMap::new())
    }

    /// Apply field mapping with strategy
    async fn apply_mapping(
        &mut self,
        field_name: &str,
        field_value: &serde_json::Value,
        context: &TransformerContext,
    ) -> Result<MappedField> {
        // 1. Check for explicit override
        if let Some(target_uri) = self.config.field_overrides.get(field_name) {
            return Ok(MappedField {
                source_field: field_name.to_string(),
                target_field: target_uri.clone(),
                value: field_value.clone(),
                confidence: 1.0,
                mapping_source: "override".to_string(),
                auto_applied: true,
            });
        }

        // 2. Build source context
        let source_context = SourceContext {
            source_id: self.config.source_id.clone(),
            table_name: self.config.table_name.clone(),
            field_name: field_name.to_string(),
            field_metadata: None, // TODO: Extract from value
        };

        // 3. Get mapping candidates based on strategy
        let candidates = match self.config.strategy {
            MappingStrategy::ManualOnly => {
                self.get_manual_candidates(&source_context).await?
            }
            MappingStrategy::MLOnly => {
                self.get_ml_candidates(&source_context).await?
            }
            MappingStrategy::ManualWithMLFallback => {
                let mut manual = self.get_manual_candidates(&source_context).await?;
                if manual.is_empty() {
                    self.get_ml_candidates(&source_context).await?
                } else {
                    manual
                }
            }
            MappingStrategy::Weighted { manual_weight, ml_weight } => {
                self.get_weighted_candidates(&source_context, manual_weight, ml_weight).await?
            }
        };

        // 4. Select best candidate
        if let Some(best) = candidates.first() {
            let auto_apply = best.confidence >= self.config.auto_apply_threshold;

            // 5. Handle interactive mode
            if !auto_apply && self.config.interactive_mode {
                let user_choice = self.prompt_user_for_mapping(field_name, &candidates).await?;

                if let Some(chosen) = user_choice {
                    // Record decision for learning
                    if self.config.record_decisions {
                        self.decision_buffer.push(MappingDecision {
                            source_context: source_context.clone(),
                            target_field_uri: chosen.target_field.clone(),
                            mapping_id: chosen.metadata.get("mapping_id").cloned(),
                            action: DecisionAction::Accept,
                            user_id: context.user_id.clone().unwrap_or_else(|| "system".to_string()),
                            timestamp: chrono::Utc::now(),
                        });
                    }

                    return Ok(MappedField {
                        source_field: field_name.to_string(),
                        target_field: chosen.target_field,
                        value: field_value.clone(),
                        confidence: chosen.confidence,
                        mapping_source: chosen.mapping_type,
                        auto_applied: false,
                    });
                }
            }

            // 6. Apply best candidate
            if auto_apply && self.config.record_decisions {
                self.decision_buffer.push(MappingDecision {
                    source_context,
                    target_field_uri: best.target_field.clone(),
                    mapping_id: best.metadata.get("mapping_id").cloned(),
                    action: DecisionAction::Apply,
                    user_id: context.user_id.clone().unwrap_or_else(|| "system".to_string()),
                    timestamp: chrono::Utc::now(),
                });
            }

            return Ok(MappedField {
                source_field: field_name.to_string(),
                target_field: best.target_field.clone(),
                value: field_value.clone(),
                confidence: best.confidence,
                mapping_source: best.mapping_type.clone(),
                auto_applied,
            });
        }

        // 7. No mapping found - passthrough
        warn!("No mapping found for field: {}", field_name);
        Ok(MappedField {
            source_field: field_name.to_string(),
            target_field: field_name.to_string(),
            value: field_value.clone(),
            confidence: 0.0,
            mapping_source: "passthrough".to_string(),
            auto_applied: false,
        })
    }

    async fn get_manual_candidates(
        &self,
        context: &SourceContext,
    ) -> Result<Vec<crate::mapping::types::MappingCandidate>> {
        let request = crate::mapping::types::GetCandidatesRequest {
            source_id: context.source_id.clone(),
            table_name: context.table_name.clone(),
            field_name: context.field_name.clone(),
            field_characteristics: None,
        };

        let response = self.engine.get_candidates(request).await?;

        Ok(response.candidates
            .into_iter()
            .filter(|c| c.mapping_type == "manual")
            .collect())
    }

    async fn get_ml_candidates(
        &self,
        context: &SourceContext,
    ) -> Result<Vec<crate::mapping::types::MappingCandidate>> {
        let request = crate::mapping::types::GetCandidatesRequest {
            source_id: context.source_id.clone(),
            table_name: context.table_name.clone(),
            field_name: context.field_name.clone(),
            field_characteristics: None,
        };

        let response = self.engine.get_candidates(request).await?;

        Ok(response.candidates
            .into_iter()
            .filter(|c| c.mapping_type != "manual")
            .collect())
    }

    async fn get_weighted_candidates(
        &self,
        context: &SourceContext,
        manual_weight: f64,
        ml_weight: f64,
    ) -> Result<Vec<crate::mapping::types::MappingCandidate>> {
        let request = crate::mapping::types::GetCandidatesRequest {
            source_id: context.source_id.clone(),
            table_name: context.table_name.clone(),
            field_name: context.field_name.clone(),
            field_characteristics: None,
        };

        let mut response = self.engine.get_candidates(request).await?;

        // Apply weighting
        for candidate in &mut response.candidates {
            if candidate.mapping_type == "manual" {
                candidate.confidence = (candidate.confidence * manual_weight).min(1.0);
            } else {
                candidate.confidence = (candidate.confidence * ml_weight).min(1.0);
            }
        }

        // Re-sort by weighted confidence
        response.candidates.sort_by(|a, b| {
            b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(response.candidates)
    }

    async fn prompt_user_for_mapping(
        &self,
        field_name: &str,
        candidates: &[crate::mapping::types::MappingCandidate],
    ) -> Result<Option<crate::mapping::types::MappingCandidate>> {
        // TODO: Implement interactive prompt
        // For now, return first candidate
        Ok(candidates.first().cloned())
    }

    /// Flush decision buffer (record all decisions)
    async fn flush_decisions(&mut self) -> Result<()> {
        for decision in self.decision_buffer.drain(..) {
            self.engine.record_mapping_decision(decision).await?;
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl Transformer for EnhancedOntologyMapperTransformer {
    fn name(&self) -> &str {
        "enhanced_ontology_mapper"
    }

    fn description(&self) -> &str {
        "Maps fields to ontology with manual mapping support"
    }

    async fn transform(
        &mut self,
        input: serde_json::Value,
        context: &TransformerContext,
    ) -> Result<TransformerResult> {
        // Load bulk mappings if configured
        let bulk_mappings = self.load_bulk_mappings().await?;

        // Apply bulk mappings to overrides
        let mut config = self.config.clone();
        config.field_overrides.extend(bulk_mappings);
        self.config = config;

        // Process input based on type
        let output = match input {
            serde_json::Value::Object(obj) => {
                let mut mapped = serde_json::Map::new();

                for (field_name, field_value) in obj {
                    let mapped_field = self.apply_mapping(&field_name, &field_value, context).await?;

                    // Store in output with target field name
                    mapped.insert(mapped_field.target_field, field_value);
                }

                serde_json::Value::Object(mapped)
            }
            serde_json::Value::Array(arr) => {
                let mut mapped_array = Vec::new();

                for item in arr {
                    if let serde_json::Value::Object(obj) = item {
                        let mut mapped = serde_json::Map::new();

                        for (field_name, field_value) in obj {
                            let mapped_field = self.apply_mapping(&field_name, &field_value, context).await?;
                            mapped.insert(mapped_field.target_field, field_value);
                        }

                        mapped_array.push(serde_json::Value::Object(mapped));
                    } else {
                        mapped_array.push(item);
                    }
                }

                serde_json::Value::Array(mapped_array)
            }
            _ => input,
        };

        // Flush decisions if recording
        if self.config.record_decisions {
            self.flush_decisions().await?;
        }

        Ok(TransformerResult {
            output,
            metadata: HashMap::from([
                ("transformer".to_string(), self.name().to_string()),
                ("strategy".to_string(), format!("{:?}", self.config.strategy)),
            ]),
        })
    }

    fn validate_config(&self, _config: &TransformerConfig) -> Result<()> {
        // Validate configuration
        if self.config.auto_apply_threshold < 0.0 || self.config.auto_apply_threshold > 1.0 {
            anyhow::bail!("auto_apply_threshold must be between 0.0 and 1.0");
        }

        if let MappingStrategy::Weighted { manual_weight, ml_weight } = self.config.strategy {
            if manual_weight < 0.0 || ml_weight < 0.0 {
                anyhow::bail!("Weights must be non-negative");
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MappedField {
    source_field: String,
    target_field: String,
    value: serde_json::Value,
    confidence: f64,
    mapping_source: String,
    auto_applied: bool,
}

/// Factory for creating enhanced ontology mapper
pub struct EnhancedOntologyMapperFactory {
    engine: Arc<EnhancedMappingEngine>,
}

impl EnhancedOntologyMapperFactory {
    pub fn new(engine: Arc<EnhancedMappingEngine>) -> Self {
        Self { engine }
    }

    pub fn create(&self, config: EnhancedOntologyMapperConfig) -> Box<dyn Transformer> {
        Box::new(EnhancedOntologyMapperTransformer::new(
            self.engine.clone(),
            config,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_field_override() {
        // Test that field overrides take precedence
        // TODO: Implement test
    }

    #[tokio::test]
    async fn test_strategy_application() {
        // Test different mapping strategies
        // TODO: Implement test
    }

    #[tokio::test]
    async fn test_decision_recording() {
        // Test that decisions are recorded for learning
        // TODO: Implement test
    }
}