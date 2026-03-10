//! # Unified Mapping Types
//!
//! Core data structures for the unified ontology mapping system.

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Unified field descriptor that works across all mapping scenarios
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldDescriptor {
    /// Field identifier
    pub id: String,

    /// Field name
    pub name: String,

    /// Normalized field name (lowercase, no special chars)
    pub normalized_name: String,

    /// Data type (VARCHAR, INTEGER, etc.)
    pub data_type: String,

    /// Whether the field is nullable
    pub nullable: bool,

    /// Whether this is a primary key
    pub primary_key: bool,

    /// Sample values for pattern detection
    pub sample_values: Vec<String>,

    /// Optional field description
    pub description: Option<String>,

    /// Source information
    pub source_id: String,
    pub table_name: String,

    /// Statistical properties (optional)
    pub statistics: Option<FieldStatistics>,
}

/// Statistical properties of a field
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldStatistics {
    pub distinct_count: Option<u64>,
    pub null_count: Option<u64>,
    pub total_count: Option<u64>,
    pub min_length: Option<usize>,
    pub max_length: Option<usize>,
    pub avg_length: Option<f64>,
}

/// Mapping options for controlling the mapping process
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MappingOptions {
    /// Minimum confidence threshold
    pub min_confidence: f64,

    /// Maximum number of candidates to return
    pub max_candidates: usize,

    /// Ontology namespaces to consider (None = all)
    pub ontology_namespaces: Option<Vec<String>>,

    /// Strategies to enable (None = all)
    pub enabled_strategies: Option<Vec<String>>,

    /// Whether to use caching
    pub use_cache: bool,

    /// Timeout for external services (milliseconds)
    pub timeout_ms: Option<u64>,
}

impl Default for MappingOptions {
    fn default() -> Self {
        Self {
            min_confidence: 0.5,
            max_candidates: 10,
            ontology_namespaces: None,
            enabled_strategies: None,
            use_cache: true,
            timeout_ms: Some(5000),
        }
    }
}

/// A mapping candidate with confidence and evidence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MappingCandidate {
    /// The ontology URI this field maps to
    pub ontology_uri: String,

    /// Overall confidence score (0.0 - 1.0)
    pub confidence: f64,

    /// Evidence from different strategies
    pub evidence: Vec<StrategyMatch>,

    /// Suggested transformation (if any)
    pub transformation: Option<String>,

    /// Human-readable explanation
    pub explanation: String,
}

/// A match from a specific strategy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyMatch {
    /// Name of the strategy that found this match
    pub strategy_name: String,

    /// The ontology URI matched
    pub ontology_uri: String,

    /// Strategy-specific confidence (0.0 - 1.0)
    pub confidence: f64,

    /// Strategy-specific explanation
    pub explanation: String,

    /// Additional metadata from the strategy
    pub metadata: HashMap<String, String>,
}

/// Result of mapping a single field
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldMappingResult {
    /// The field that was mapped
    pub field: FieldDescriptor,

    /// Mapping candidates (sorted by confidence)
    pub candidates: Vec<MappingCandidate>,

    /// Any errors encountered during mapping
    pub errors: Vec<String>,

    /// Processing time in milliseconds
    pub processing_time_ms: u64,
}

/// Context passed to matching strategies
#[derive(Clone)]
pub struct MatchingContext {
    /// Cache for embeddings
    pub embedding_cache: Option<std::sync::Arc<dyn EmbeddingCache>>,

    /// Cache for ontology terms
    pub ontology_cache: Option<std::sync::Arc<dyn OntologyCache>>,

    /// Pattern detector instance
    pub pattern_detector: Option<std::sync::Arc<dyn PatternDetector>>,

    /// Additional context data
    pub metadata: HashMap<String, String>,
}

/// Trait for matching strategies
#[async_trait]
pub trait MatchingStrategy: Send + Sync {
    /// Strategy name for debugging and configuration
    fn name(&self) -> &str;

    /// Minimum confidence this strategy can produce
    fn min_confidence(&self) -> f64;

    /// Maximum confidence this strategy can produce
    fn max_confidence(&self) -> f64;

    /// Check if this strategy applies to the given field
    fn applies_to(&self, field: &FieldDescriptor) -> bool;

    /// Find matches for the given field
    async fn find_matches(
        &self,
        field: &FieldDescriptor,
        ontology_terms: &[OntologyTerm],
        context: &MatchingContext,
    ) -> Result<Vec<StrategyMatch>>;
}

/// An ontology term (unified representation)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OntologyTerm {
    /// Full URI of the ontology term
    pub uri: String,

    /// Human-readable label
    pub label: String,

    /// Namespace (e.g., "schema.org", "custom.retail")
    pub namespace: String,

    /// Term type (Class, Property, etc.)
    pub term_type: OntologyTermType,

    /// Optional description
    pub description: Option<String>,

    /// Data type constraints (for properties)
    pub data_type: Option<String>,

    /// Alternative labels/synonyms
    pub alt_labels: Vec<String>,
}

/// Type of ontology term
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OntologyTermType {
    Class,
    Property,
    DataProperty,
    ObjectProperty,
    Individual,
}

/// Trait for embedding cache
pub trait EmbeddingCache: Send + Sync {
    /// Get embedding from cache
    fn get(&self, text: &str) -> Option<Vec<f32>>;

    /// Store embedding in cache
    fn put(&self, text: &str, embedding: Vec<f32>);
}

/// Trait for ontology cache
pub trait OntologyCache: Send + Sync {
    /// Get all ontology terms
    fn get_terms(&self) -> Vec<OntologyTerm>;

    /// Get terms by namespace
    fn get_terms_by_namespace(&self, namespace: &str) -> Vec<OntologyTerm>;

    /// Refresh cache from registry
    fn refresh(&self) -> Result<()>;
}

/// Trait for pattern detection
pub trait PatternDetector: Send + Sync {
    /// Analyze samples and detect patterns
    fn detect_patterns(&self, samples: &[String]) -> Vec<DetectedPattern>;

    /// Check if samples match a specific pattern
    fn matches_pattern(&self, samples: &[String], pattern_type: PatternType) -> bool;
}

/// Detected pattern in field values
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedPattern {
    /// Type of pattern detected
    pub pattern_type: PatternType,

    /// Confidence in the pattern (0.0 - 1.0)
    pub confidence: f64,

    /// Number of samples that match
    pub match_count: usize,

    /// Total number of samples analyzed
    pub total_count: usize,

    /// Example matching value
    pub example: Option<String>,
}

/// Types of patterns that can be detected
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum PatternType {
    Email,
    Phone,
    URL,
    PostalCode,
    SSN,
    CreditCard,
    UUID,
    IPv4,
    IPv6,
    Date,
    Time,
    DateTime,
    Currency,
    Percentage,
    Custom(String),
}

/// Configuration for the unified mapping engine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedMappingConfig {
    /// Strategy configurations
    pub strategies: HashMap<String, StrategyConfig>,

    /// Caching configuration
    pub caching: CachingConfig,

    /// Parallel execution settings
    pub parallel_execution: ParallelConfig,

    /// External service configurations
    pub services: ServicesConfig,
}

/// Configuration for a single strategy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyConfig {
    /// Whether this strategy is enabled
    pub enabled: bool,

    /// Weight for confidence scoring (1.0 = normal)
    pub weight: f64,

    /// Minimum confidence override
    pub min_confidence: Option<f64>,

    /// Maximum confidence override
    pub max_confidence: Option<f64>,

    /// Strategy-specific settings
    pub settings: HashMap<String, String>,
}

/// Caching configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachingConfig {
    /// Ontology cache TTL in seconds
    pub ontology_ttl: u64,

    /// Maximum embedding cache entries
    pub embedding_cache_size: usize,

    /// Maximum pattern cache entries
    pub pattern_cache_size: usize,
}

/// Parallel execution configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParallelConfig {
    /// Whether to execute strategies in parallel
    pub enabled: bool,

    /// Maximum concurrent strategies
    pub max_concurrent_strategies: usize,
}

/// External services configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServicesConfig {
    /// GraphicaModel service URL
    pub model_service_url: Option<String>,

    /// Connection timeout in milliseconds
    pub connect_timeout_ms: u64,

    /// Request timeout in milliseconds
    pub request_timeout_ms: u64,
}

impl Default for UnifiedMappingConfig {
    fn default() -> Self {
        Self {
            strategies: Self::default_strategies(),
            caching: CachingConfig {
                ontology_ttl: 300,
                embedding_cache_size: 10000,
                pattern_cache_size: 1000,
            },
            parallel_execution: ParallelConfig {
                enabled: true,
                max_concurrent_strategies: 4,
            },
            services: ServicesConfig {
                model_service_url: None,
                connect_timeout_ms: 5000,
                request_timeout_ms: 10000,
            },
        }
    }
}

impl UnifiedMappingConfig {
    fn default_strategies() -> HashMap<String, StrategyConfig> {
        let mut strategies = HashMap::new();

        strategies.insert(
            "pattern".to_string(),
            StrategyConfig {
                enabled: true,
                weight: 1.5,
                min_confidence: Some(0.85),
                max_confidence: Some(0.95),
                settings: HashMap::new(),
            },
        );

        strategies.insert(
            "semantic".to_string(),
            StrategyConfig {
                enabled: true,
                weight: 1.2,
                min_confidence: Some(0.80),
                max_confidence: Some(0.90),
                settings: HashMap::new(),
            },
        );

        strategies.insert(
            "statistical".to_string(),
            StrategyConfig {
                enabled: true,
                weight: 1.0,
                min_confidence: Some(0.70),
                max_confidence: Some(0.85),
                settings: HashMap::new(),
            },
        );

        strategies.insert(
            "lexical".to_string(),
            StrategyConfig {
                enabled: true,
                weight: 0.8,
                min_confidence: Some(0.65),
                max_confidence: Some(0.80),
                settings: HashMap::new(),
            },
        );

        strategies.insert(
            "registry".to_string(),
            StrategyConfig {
                enabled: true,
                weight: 1.1,
                min_confidence: Some(0.75),
                max_confidence: Some(0.90),
                settings: HashMap::new(),
            },
        );

        strategies.insert(
            "heuristic".to_string(),
            StrategyConfig {
                enabled: true,
                weight: 0.6,
                min_confidence: Some(0.60),
                max_confidence: Some(0.75),
                settings: HashMap::new(),
            },
        );

        strategies
    }
}
