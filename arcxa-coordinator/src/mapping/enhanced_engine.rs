// Enhanced Mapping Engine with Manual Mapping Integration
use super::manual::{store::ManualMappingStore, types::*};
use crate::mapping::engine::MappingEngine;
use crate::mapping::types::{
    AnalyzeSchemaRequest, AnalyzeSchemaResponse, FieldMapping, GetCandidatesRequest,
    GetCandidatesResponse, MappingCandidate,
};
use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info};

/// Enhanced mapping engine that combines ML and manual mappings
pub struct EnhancedMappingEngine {
    /// Original ML-based mapping engine
    ml_engine: Arc<MappingEngine>,

    /// Manual mapping store
    manual_store: Arc<ManualMappingStore>,

    /// Configuration
    config: EnhancedMappingConfig,
}

#[derive(Debug, Clone)]
pub struct EnhancedMappingConfig {
    /// Minimum confidence threshold for ML mappings
    pub ml_confidence_threshold: f64,

    /// Weight for manual mappings (vs ML mappings)
    pub manual_mapping_weight: f64,

    /// Enable auto-learning from accepted mappings
    pub auto_learn: bool,

    /// Maximum suggestions to return
    pub max_suggestions: usize,
}

impl Default for EnhancedMappingConfig {
    fn default() -> Self {
        Self {
            ml_confidence_threshold: 0.5,
            manual_mapping_weight: 2.0, // Manual mappings weighted 2x ML
            auto_learn: true,
            max_suggestions: 5,
        }
    }
}

impl EnhancedMappingEngine {
    pub fn new(
        ml_engine: Arc<MappingEngine>,
        manual_store: Arc<ManualMappingStore>,
        config: EnhancedMappingConfig,
    ) -> Self {
        Self {
            ml_engine,
            manual_store,
            config,
        }
    }

    /// Analyze schema with manual mapping enhancement
    pub async fn analyze_schema(
        &self,
        request: AnalyzeSchemaRequest,
    ) -> Result<AnalyzeSchemaResponse> {
        // First get ML analysis
        let mut response = self.ml_engine.analyze_schema(request.clone()).await?;

        // Enhance with manual mappings
        for field in &mut response.fields {
            let context = SourceContext {
                source_id: request.source_id.clone(),
                table_name: request.table_name.clone(),
                field_name: field.name.clone(),
                field_metadata: Some(FieldCharacteristics {
                    data_type: Some(field.data_type.clone()),
                    sample_values: field.sample_values.clone().unwrap_or_default(),
                    detected_pattern: field.detected_pattern.clone(),
                    profile_hash: None, // TODO: Calculate profile hash
                }),
            };

            // Check for exact manual mapping
            if let Some(manual_mapping) = self.manual_store.find_by_source(&context).await? {
                // Override with manual mapping
                field.suggested_mapping = Some(FieldMapping {
                    target_field: manual_mapping.target_field_uri.clone(),
                    confidence: manual_mapping.confidence,
                    mapping_type: "manual".to_string(),
                    source: "user_defined".to_string(),
                });

                field.mapping_candidates.insert(
                    0,
                    MappingCandidate {
                        target_field: manual_mapping.target_field_uri,
                        confidence: manual_mapping.confidence,
                        mapping_type: "manual".to_string(),
                        reason: format!("User-defined mapping by {}", manual_mapping.created_by),
                        metadata: HashMap::from([
                            ("created_at".to_string(), manual_mapping.created_at.to_rfc3339()),
                            ("usage_count".to_string(), manual_mapping.usage_stats.apply_count.to_string()),
                        ]),
                    },
                );
            }
        }

        Ok(response)
    }

    /// Get mapping candidates with priority system
    pub async fn get_candidates(
        &self,
        request: GetCandidatesRequest,
    ) -> Result<GetCandidatesResponse> {
        let context = SourceContext {
            source_id: request.source_id.clone(),
            table_name: request.table_name.clone(),
            field_name: request.field_name.clone(),
            field_metadata: request.field_characteristics.as_ref().map(|fc| {
                FieldCharacteristics {
                    data_type: fc.data_type.clone(),
                    sample_values: fc.sample_values.clone(),
                    detected_pattern: fc.detected_pattern.clone(),
                    profile_hash: fc.profile_hash.clone(),
                }
            }),
        };

        let mut all_candidates = Vec::new();

        // 1. Get manual mapping candidates (highest priority)
        let manual_suggestions = self
            .manual_store
            .find_similar_mappings(&context, self.config.max_suggestions)
            .await?;

        for suggestion in manual_suggestions {
            let weighted_confidence = suggestion.relevance_score * self.config.manual_mapping_weight;

            all_candidates.push(MappingCandidate {
                target_field: suggestion.mapping.target_field_uri.clone(),
                confidence: weighted_confidence.min(1.0),
                mapping_type: "manual".to_string(),
                reason: match suggestion.suggestion_reason {
                    SuggestionReason::ExactFieldMatch { previous_source } => {
                        format!("Exact match from {}", previous_source)
                    }
                    SuggestionReason::SimilarFieldName { similarity } => {
                        format!("Similar field name ({}% match)", (similarity * 100.0) as u32)
                    }
                    SuggestionReason::SimilarDataProfile { profile_match } => {
                        format!("Similar data profile ({}% match)", (profile_match * 100.0) as u32)
                    }
                    SuggestionReason::FrequentPattern { usage_count } => {
                        format!("Frequently used pattern ({} times)", usage_count)
                    }
                    SuggestionReason::MLModel { model_name, confidence } => {
                        format!("ML model {} ({}% confidence)", model_name, (confidence * 100.0) as u32)
                    }
                },
                metadata: HashMap::from([
                    ("mapping_id".to_string(), suggestion.mapping.id),
                    ("created_by".to_string(), suggestion.mapping.created_by),
                    ("usage_count".to_string(), suggestion.mapping.usage_stats.apply_count.to_string()),
                ]),
            });
        }

        // 2. Get ML mapping candidates
        let ml_response = self.ml_engine.get_candidates(request).await?;
        for ml_candidate in ml_response.candidates {
            // Only include ML candidates above threshold
            if ml_candidate.confidence >= self.config.ml_confidence_threshold {
                // Check if this target is already in manual mappings
                let already_manual = all_candidates
                    .iter()
                    .any(|c| c.target_field == ml_candidate.target_field && c.mapping_type == "manual");

                if !already_manual {
                    all_candidates.push(ml_candidate);
                }
            }
        }

        // 3. Sort by confidence (manual mappings weighted higher)
        all_candidates.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // 4. Limit to max suggestions
        all_candidates.truncate(self.config.max_suggestions);

        Ok(GetCandidatesResponse {
            field_name: context.field_name,
            candidates: all_candidates,
            has_exact_match: all_candidates
                .first()
                .map(|c| c.confidence >= 0.95 && c.mapping_type == "manual")
                .unwrap_or(false),
        })
    }

    /// Record a mapping decision (for learning)
    pub async fn record_mapping_decision(
        &self,
        decision: MappingDecision,
    ) -> Result<()> {
        match decision.action {
            DecisionAction::Accept => {
                // If accepting a manual mapping, update its usage stats
                if let Some(mapping_id) = decision.mapping_id {
                    self.manual_store
                        .update_usage_stats(&mapping_id, UsageStatType::Accepted)
                        .await?;
                }

                // If auto-learn is enabled, create new manual mapping
                if self.config.auto_learn && decision.mapping_id.is_none() {
                    let new_mapping = ManualFieldMapping {
                        id: uuid::Uuid::new_v4().to_string(),
                        source_context: decision.source_context,
                        target_field_uri: decision.target_field_uri,
                        confidence: 1.0,
                        created_by: decision.user_id,
                        created_at: chrono::Utc::now(),
                        updated_at: chrono::Utc::now(),
                        notes: Some("Auto-learned from user acceptance".to_string()),
                        usage_stats: UsageStats {
                            apply_count: 1,
                            accept_count: 1,
                            reject_count: 0,
                            last_used: Some(chrono::Utc::now()),
                        },
                    };

                    self.manual_store.store_mapping(new_mapping).await?;
                    info!("Auto-learned new mapping from user acceptance");
                }
            }
            DecisionAction::Reject => {
                // Update rejection stats
                if let Some(mapping_id) = decision.mapping_id {
                    self.manual_store
                        .update_usage_stats(&mapping_id, UsageStatType::Rejected)
                        .await?;
                }
            }
            DecisionAction::Apply => {
                // Update apply stats
                if let Some(mapping_id) = decision.mapping_id {
                    self.manual_store
                        .update_usage_stats(&mapping_id, UsageStatType::Applied)
                        .await?;
                }
            }
        }

        Ok(())
    }

    /// Get mapping confidence score (combines manual and ML)
    pub async fn get_combined_confidence(
        &self,
        source_context: &SourceContext,
        target_field_uri: &str,
    ) -> Result<f64> {
        // Check manual mapping first
        if let Some(manual) = self.manual_store.find_by_source(source_context).await? {
            if manual.target_field_uri == target_field_uri {
                return Ok(manual.confidence);
            }
        }

        // Fall back to ML confidence
        let ml_request = GetCandidatesRequest {
            source_id: source_context.source_id.clone(),
            table_name: source_context.table_name.clone(),
            field_name: source_context.field_name.clone(),
            field_characteristics: source_context.field_metadata.as_ref().map(|fm| {
                crate::mapping::types::FieldCharacteristics {
                    data_type: fm.data_type.clone(),
                    sample_values: fm.sample_values.clone(),
                    detected_pattern: fm.detected_pattern.clone(),
                    profile_hash: fm.profile_hash.clone(),
                }
            }),
        };

        let ml_response = self.ml_engine.get_candidates(ml_request).await?;

        Ok(ml_response
            .candidates
            .iter()
            .find(|c| c.target_field == target_field_uri)
            .map(|c| c.confidence)
            .unwrap_or(0.0))
    }
}

/// Decision tracking for learning
#[derive(Debug, Clone)]
pub struct MappingDecision {
    pub source_context: SourceContext,
    pub target_field_uri: String,
    pub mapping_id: Option<String>, // ID if from existing mapping
    pub action: DecisionAction,
    pub user_id: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone)]
pub enum DecisionAction {
    Accept,  // User accepted suggestion
    Reject,  // User rejected suggestion
    Apply,   // Mapping was applied to data
}

/// Batch mapping optimizer
pub struct BatchMappingOptimizer {
    engine: Arc<EnhancedMappingEngine>,
}

impl BatchMappingOptimizer {
    pub fn new(engine: Arc<EnhancedMappingEngine>) -> Self {
        Self { engine }
    }

    /// Optimize mappings for a batch of fields
    pub async fn optimize_batch(
        &self,
        fields: Vec<SourceContext>,
    ) -> Result<Vec<OptimizedMapping>> {
        let mut optimized = Vec::new();

        for field_context in fields {
            let request = GetCandidatesRequest {
                source_id: field_context.source_id.clone(),
                table_name: field_context.table_name.clone(),
                field_name: field_context.field_name.clone(),
                field_characteristics: None, // TODO: Add if available
            };

            let candidates = self.engine.get_candidates(request).await?;

            // Select best candidate
            if let Some(best) = candidates.candidates.first() {
                optimized.push(OptimizedMapping {
                    source_context: field_context,
                    target_field_uri: best.target_field.clone(),
                    confidence: best.confidence,
                    mapping_type: best.mapping_type.clone(),
                    auto_applied: best.confidence >= 0.95 && best.mapping_type == "manual",
                });
            } else {
                optimized.push(OptimizedMapping {
                    source_context: field_context,
                    target_field_uri: String::new(),
                    confidence: 0.0,
                    mapping_type: "none".to_string(),
                    auto_applied: false,
                });
            }
        }

        Ok(optimized)
    }
}

#[derive(Debug, Clone)]
pub struct OptimizedMapping {
    pub source_context: SourceContext,
    pub target_field_uri: String,
    pub confidence: f64,
    pub mapping_type: String,
    pub auto_applied: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_manual_mapping_priority() {
        // Test that manual mappings take priority over ML mappings
        // TODO: Implement test
    }

    #[tokio::test]
    async fn test_auto_learning() {
        // Test that accepted mappings are auto-learned
        // TODO: Implement test
    }

    #[tokio::test]
    async fn test_confidence_weighting() {
        // Test that manual mappings are weighted higher
        // TODO: Implement test
    }
}