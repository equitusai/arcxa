//! # Unified Ontology Mapping Engine
//!
//! Main orchestrator for the unified mapping system.

use anyhow::{Context, Result};
use futures::future::join_all;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tracing::{debug, info, instrument, warn};

use super::scoring::ConfidenceScorer;
use super::shared::{OntologyCacheImpl, PatternDetectorImpl};
use super::strategies::{
    HeuristicStrategy, LexicalStrategy, ManualStrategy, PatternStrategy, RegistryStrategy,
    SemanticStrategy, StatisticalStrategy,
};
use super::types::*;
use crate::mapping::manual::ManualMappingStore;

/// Main unified ontology mapping engine
pub struct UnifiedOntologyMappingEngine {
    /// Registered matching strategies
    strategies: HashMap<String, Box<dyn MatchingStrategy>>,

    /// Confidence scorer
    scorer: ConfidenceScorer,

    /// Configuration
    config: UnifiedMappingConfig,

    /// Ontology cache
    ontology_cache: Arc<dyn OntologyCache>,

    /// Pattern detector
    pattern_detector: Arc<dyn PatternDetector>,

    /// Embedding cache (if semantic strategy enabled)
    embedding_cache: Option<Arc<dyn EmbeddingCache>>,
}

impl UnifiedOntologyMappingEngine {
    /// Create a new unified mapping engine
    pub async fn new(
        config: UnifiedMappingConfig,
        registry_client: Option<Arc<crate::mapping::ontology_registry::RegistryClient>>,
        manual_mapping_store: Option<Arc<crate::mapping::manual::ManualMappingStore>>,
        // semantic_client: Option<Arc<crate::mapping::semantic::SemanticMatcherClient>>, // PRE-EXISTING ISSUE
    ) -> Result<Self> {
        info!("Initializing Unified Ontology Mapping Engine");

        // Initialize shared components
        let ontology_cache = Arc::new(OntologyCacheImpl::new(
            registry_client,
            config.caching.ontology_ttl,
        )?);

        let pattern_detector = Arc::new(PatternDetectorImpl::new());

        // Initialize strategies based on configuration
        let mut strategies: HashMap<String, Box<dyn MatchingStrategy>> = HashMap::new();

        // Manual Strategy (HIGHEST PRIORITY - always checked first)
        if let Some(store) = manual_mapping_store {
            info!("  Enabling Manual Strategy (100% confidence)");
            strategies.insert("manual".to_string(), Box::new(ManualStrategy::new(store)));
        }

        // Pattern Strategy
        if config
            .strategies
            .get("pattern")
            .map_or(false, |s| s.enabled)
        {
            info!("  Enabling Pattern Strategy");
            strategies.insert(
                "pattern".to_string(),
                Box::new(PatternStrategy::new(pattern_detector.clone())),
            );
        }

        // Semantic Strategy (PRE-EXISTING ISSUE: semantic module doesn't exist)
        // if config.strategies.get("semantic").map_or(false, |s| s.enabled) {
        //     if let Some(client) = semantic_client {
        //         info!("  Enabling Semantic Strategy");
        //         strategies.insert(
        //             "semantic".to_string(),
        //             Box::new(SemanticStrategy::new(client)),
        //         );
        //     } else {
        if config
            .strategies
            .get("semantic")
            .map_or(false, |s| s.enabled)
        {
            {
                warn!("  Semantic Strategy enabled but no client available");
            }
        }

        // Statistical Strategy
        if config
            .strategies
            .get("statistical")
            .map_or(false, |s| s.enabled)
        {
            info!("  Enabling Statistical Strategy");
            strategies.insert(
                "statistical".to_string(),
                Box::new(StatisticalStrategy::new()),
            );
        }

        // Lexical Strategy
        if config
            .strategies
            .get("lexical")
            .map_or(false, |s| s.enabled)
        {
            info!("  Enabling Lexical Strategy");
            strategies.insert("lexical".to_string(), Box::new(LexicalStrategy::new()));
        }

        // Registry Strategy
        if config
            .strategies
            .get("registry")
            .map_or(false, |s| s.enabled)
        {
            info!("  Enabling Registry Strategy");
            strategies.insert("registry".to_string(), Box::new(RegistryStrategy::new()));
        }

        // Heuristic Strategy
        if config
            .strategies
            .get("heuristic")
            .map_or(false, |s| s.enabled)
        {
            info!("  Enabling Heuristic Strategy");
            strategies.insert("heuristic".to_string(), Box::new(HeuristicStrategy::new()));
        }

        info!("  {} strategies enabled", strategies.len());

        // Initialize confidence scorer with strategy weights
        let mut strategy_weights = HashMap::new();

        // Manual strategy always gets highest weight (2.0) since it's user-defined
        if strategies.contains_key("manual") {
            strategy_weights.insert("manual".to_string(), 2.0);
        }

        for (name, cfg) in &config.strategies {
            if cfg.enabled {
                strategy_weights.insert(name.clone(), cfg.weight);
            }
        }
        let scorer = ConfidenceScorer::new(strategy_weights);

        Ok(Self {
            strategies,
            scorer,
            config,
            ontology_cache,
            pattern_detector,
            embedding_cache: None, // TODO: Initialize from semantic client
        })
    }

    /// Map a single field to ontology terms
    #[instrument(skip(self, field, options))]
    pub async fn map_field(
        &self,
        field: &FieldDescriptor,
        options: &MappingOptions,
    ) -> Result<Vec<MappingCandidate>> {
        let start = Instant::now();

        debug!(
            "Mapping field: {} (type: {}, table: {})",
            field.name, field.data_type, field.table_name
        );

        // Get ontology terms (filtered by namespace if specified)
        let ontology_terms = if let Some(namespaces) = &options.ontology_namespaces {
            let mut terms = Vec::new();
            for ns in namespaces {
                terms.extend(self.ontology_cache.get_terms_by_namespace(ns));
            }
            terms
        } else {
            self.ontology_cache.get_terms()
        };

        debug!(
            "Found {} ontology terms to match against",
            ontology_terms.len()
        );

        // Build matching context
        let context = MatchingContext {
            embedding_cache: self.embedding_cache.clone(),
            ontology_cache: Some(self.ontology_cache.clone()),
            pattern_detector: Some(self.pattern_detector.clone()),
            metadata: HashMap::new(),
        };

        // Execute strategies
        let all_matches = if self.config.parallel_execution.enabled {
            self.execute_strategies_parallel(field, &ontology_terms, &context, options)
                .await?
        } else {
            self.execute_strategies_sequential(field, &ontology_terms, &context, options)
                .await?
        };

        debug!(
            "Collected {} raw matches from strategies",
            all_matches.len()
        );

        // Check if there are any manual mappings - if so, use ONLY those (manual overrides all)
        let has_manual_mapping = all_matches.iter().any(|m| m.strategy_name == "manual");
        let filtered_matches = if has_manual_mapping {
            debug!("Manual mapping found - filtering out all non-manual matches");
            all_matches
                .into_iter()
                .filter(|m| m.strategy_name == "manual")
                .collect()
        } else {
            all_matches
        };

        // Score and rank candidates
        let mut candidates = self.scorer.score_candidates(filtered_matches);

        // Filter by minimum confidence
        candidates.retain(|c| c.confidence >= options.min_confidence);

        // Sort by confidence (descending)
        candidates.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());

        // Limit to max candidates
        candidates.truncate(options.max_candidates);

        let elapsed = start.elapsed();
        info!(
            "Mapped field '{}' to {} candidates in {:?}",
            field.name,
            candidates.len(),
            elapsed
        );

        Ok(candidates)
    }

    /// Map multiple fields in batch
    #[instrument(skip(self, fields, options))]
    pub async fn map_fields(
        &self,
        fields: &[FieldDescriptor],
        options: &MappingOptions,
    ) -> Result<Vec<FieldMappingResult>> {
        info!("Batch mapping {} fields", fields.len());

        // Map each field concurrently
        let futures: Vec<_> = fields
            .iter()
            .map(|field| {
                let field = field.clone();
                let options = options.clone();
                async move {
                    let start = Instant::now();
                    let candidates = match self.map_field(&field, &options).await {
                        Ok(c) => c,
                        Err(e) => {
                            warn!("Error mapping field {}: {}", field.name, e);
                            return FieldMappingResult {
                                field,
                                candidates: vec![],
                                errors: vec![e.to_string()],
                                processing_time_ms: start.elapsed().as_millis() as u64,
                            };
                        }
                    };

                    FieldMappingResult {
                        field,
                        candidates,
                        errors: vec![],
                        processing_time_ms: start.elapsed().as_millis() as u64,
                    }
                }
            })
            .collect();

        let results = join_all(futures).await;

        Ok(results)
    }

    /// Execute strategies in parallel
    async fn execute_strategies_parallel(
        &self,
        field: &FieldDescriptor,
        ontology_terms: &[OntologyTerm],
        context: &MatchingContext,
        options: &MappingOptions,
    ) -> Result<Vec<StrategyMatch>> {
        let mut futures = Vec::new();

        for (name, strategy) in &self.strategies {
            // Skip if strategy is not enabled in options
            if let Some(enabled) = &options.enabled_strategies {
                if !enabled.contains(name) {
                    continue;
                }
            }

            // Check if strategy applies to this field
            if !strategy.applies_to(field) {
                debug!("Strategy {} does not apply to field {}", name, field.name);
                continue;
            }

            let strategy = strategy.as_ref();
            let field = field.clone();
            let terms = ontology_terms.to_vec();
            let ctx = context.clone();

            futures.push(async move {
                match strategy.find_matches(&field, &terms, &ctx).await {
                    Ok(matches) => matches,
                    Err(e) => {
                        warn!("Strategy {} failed: {}", strategy.name(), e);
                        vec![]
                    }
                }
            });
        }

        // Limit concurrency
        let max_concurrent = self.config.parallel_execution.max_concurrent_strategies;
        let mut all_matches = Vec::new();

        // Process futures in chunks (owned, not borrowed)
        let mut futures_iter = futures.into_iter();
        loop {
            let chunk: Vec<_> = futures_iter.by_ref().take(max_concurrent).collect();
            if chunk.is_empty() {
                break;
            }
            let chunk_results = join_all(chunk).await;
            for matches in chunk_results {
                all_matches.extend(matches);
            }
        }

        Ok(all_matches)
    }

    /// Execute strategies sequentially
    async fn execute_strategies_sequential(
        &self,
        field: &FieldDescriptor,
        ontology_terms: &[OntologyTerm],
        context: &MatchingContext,
        options: &MappingOptions,
    ) -> Result<Vec<StrategyMatch>> {
        let mut all_matches = Vec::new();

        for (name, strategy) in &self.strategies {
            // Skip if strategy is not enabled in options
            if let Some(enabled) = &options.enabled_strategies {
                if !enabled.contains(name) {
                    continue;
                }
            }

            // Check if strategy applies to this field
            if !strategy.applies_to(field) {
                debug!("Strategy {} does not apply to field {}", name, field.name);
                continue;
            }

            match strategy.find_matches(field, ontology_terms, context).await {
                Ok(matches) => {
                    debug!("Strategy {} found {} matches", name, matches.len());
                    all_matches.extend(matches);
                }
                Err(e) => {
                    warn!("Strategy {} failed: {}", name, e);
                }
            }
        }

        Ok(all_matches)
    }

    /// Register a custom matching strategy
    pub fn register_strategy(&mut self, name: String, strategy: Box<dyn MatchingStrategy>) {
        info!("Registering custom strategy: {}", name);
        self.strategies.insert(name, strategy);
    }

    /// Get list of enabled strategies
    pub fn enabled_strategies(&self) -> Vec<String> {
        self.strategies.keys().cloned().collect()
    }

    /// Update strategy configuration
    pub fn update_strategy_config(&mut self, name: &str, config: StrategyConfig) -> Result<()> {
        if !self.strategies.contains_key(name) {
            return Err(anyhow::anyhow!("Strategy {} not found", name));
        }

        self.config
            .strategies
            .insert(name.to_string(), config.clone());

        // Update scorer weights
        if config.enabled {
            self.scorer.update_weight(name, config.weight);
        } else {
            self.scorer.remove_weight(name);
        }

        Ok(())
    }

    /// Refresh ontology cache
    pub async fn refresh_ontologies(&self) -> Result<()> {
        info!("Refreshing ontology cache");
        self.ontology_cache.refresh()?;
        Ok(())
    }
}
