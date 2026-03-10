//! Field-Level Lineage Core Types
//!
//! Rust types for tracking field-level provenance in golden record creation.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::ToSchema;

/// A specific value for a field in a golden record with full provenance
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FieldValue {
    /// Entity ID this field belongs to
    pub entity_id: String,

    /// Field name (e.g., "address", "email", "phone")
    pub field_name: String,

    /// The actual value chosen
    pub value: serde_json::Value,

    /// Data type of the value
    pub value_type: String,

    /// Confidence score (0.0-1.0)
    pub confidence: f64,

    /// When this value was resolved
    pub resolved_at: DateTime<Utc>,

    /// Valid from (bitemporal support)
    pub valid_from: DateTime<Utc>,

    /// Valid to (None = current)
    pub valid_to: Option<DateTime<Utc>>,

    /// Previous field value this supersedes
    pub supersedes: Option<String>,

    /// Human-readable explanation
    pub explanation: Option<String>,

    /// Provenance: which resolution activity created this
    pub resolution_id: String,
}

/// A candidate value from a source system for field resolution
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SourceValue {
    /// Unique ID for this source value
    pub id: String,

    /// The value from the source
    pub value: serde_json::Value,

    /// Source system name (e.g., "CRM", "ERP", "Website")
    pub source_system: String,

    /// When this value was recorded in source
    pub source_timestamp: DateTime<Utc>,

    /// Authority/trust weight of source (0.0-1.0)
    pub source_authority: f64,

    /// Confidence from source (if available)
    pub confidence: Option<f64>,

    /// Vote count (for frequency voting)
    pub vote_count: u32,

    /// Weighted vote score
    pub vote_weight: f64,

    /// Additional metadata from source
    pub metadata: HashMap<String, String>,
}

/// The activity of resolving a field value from multiple sources
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldResolution {
    /// Unique ID for this resolution
    pub id: String,

    /// Entity ID
    pub entity_id: String,

    /// Field name
    pub field_name: String,

    /// All source values considered
    pub source_values: Vec<SourceValue>,

    /// The selected source value
    pub selected_value: SourceValue,

    /// Rejected source values
    pub rejected_values: Vec<SourceValue>,

    /// Voting strategy used
    pub strategy: VotingStrategy,

    /// When resolution occurred
    pub resolved_at: DateTime<Utc>,

    /// Conflict information (if any)
    pub conflict: Option<FieldConflict>,

    /// Who/what performed the resolution
    pub resolved_by: String,

    /// Human-readable explanation
    pub explanation: String,

    /// Review information (if reviewed)
    pub review: Option<FieldReview>,
}

/// Voting strategy for field resolution
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VotingStrategy {
    /// Strategy type
    pub strategy_type: StrategyType,

    /// Strategy-specific parameters (JSON)
    pub parameters: serde_json::Value,

    /// Human-readable description
    pub description: String,
}

/// Types of voting strategies
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, ToSchema)]
#[serde(rename_all = "kebab-case")]
pub enum StrategyType {
    /// Most common value wins (majority vote)
    Frequency,

    /// Recent values weighted higher (exponential decay)
    TimeDecay,

    /// Trusted sources weighted higher
    Authority,

    /// Combine multiple strategies
    Ensemble,

    /// Use ML model to predict correct value
    MlPrediction,

    /// Custom user-defined strategy
    Custom,
}

impl StrategyType {
    /// Get default confidence threshold for this strategy
    pub fn default_confidence_threshold(&self) -> f64 {
        match self {
            StrategyType::Frequency => 0.60,    // 60% of votes
            StrategyType::TimeDecay => 0.70,    // Higher for time-based
            StrategyType::Authority => 0.80,    // High for authority-based
            StrategyType::Ensemble => 0.75,     // Combined threshold
            StrategyType::MlPrediction => 0.85, // ML should be confident
            StrategyType::Custom => 0.70,       // Conservative default
        }
    }

    /// Check if this strategy requires parameters
    pub fn requires_parameters(&self) -> bool {
        match self {
            StrategyType::TimeDecay => true,    // Decay rate
            StrategyType::Authority => true,    // Source weights
            StrategyType::Ensemble => true,     // Strategy weights
            StrategyType::MlPrediction => true, // Model endpoint
            StrategyType::Custom => true,       // Custom logic
            StrategyType::Frequency => false,   // No params needed
        }
    }
}

/// A conflict between multiple candidate values
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldConflict {
    /// Conflict ID
    pub id: String,

    /// Conflicting values
    pub conflicting_values: Vec<SourceValue>,

    /// Severity: low, medium, high, critical
    pub severity: ConflictSeverity,

    /// Explanation of conflict
    pub reason: String,

    /// Whether human review is required
    pub requires_review: bool,

    /// Suggested resolution (if available)
    pub suggested_resolution: Option<String>,
}

/// Conflict severity levels
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ConflictSeverity {
    Low,
    Medium,
    High,
    Critical,
}

impl ConflictSeverity {
    /// Determine if this severity requires human review
    pub fn requires_human_review(&self) -> bool {
        matches!(self, ConflictSeverity::High | ConflictSeverity::Critical)
    }

    /// Get confidence penalty for this severity
    pub fn confidence_penalty(&self) -> f64 {
        match self {
            ConflictSeverity::Low => 0.05,
            ConflictSeverity::Medium => 0.10,
            ConflictSeverity::High => 0.20,
            ConflictSeverity::Critical => 0.40,
        }
    }
}

/// Human or ML review of field resolution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldReview {
    /// Reviewer (user ID or model name)
    pub reviewed_by: String,

    /// When reviewed
    pub reviewed_at: DateTime<Utc>,

    /// Review decision: approved, rejected, modified
    pub decision: ReviewDecision,

    /// Review notes
    pub notes: Option<String>,

    /// Modified value (if decision = modified)
    pub modified_value: Option<serde_json::Value>,
}

/// Review decision types
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ReviewDecision {
    Approved,
    Rejected,
    Modified,
}

/// Field lineage query parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldLineageQuery {
    /// Entity ID
    pub entity_id: String,

    /// Field name
    pub field_name: String,

    /// Point-in-time query (None = current)
    pub as_of: Option<DateTime<Utc>>,

    /// Include source values
    pub include_sources: bool,

    /// Include voting details
    pub include_voting: bool,

    /// Include conflict information
    pub include_conflicts: bool,

    /// Maximum depth of lineage tree
    pub max_depth: Option<usize>,
}

/// Field lineage response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldLineageResponse {
    /// The field value
    pub field_value: FieldValue,

    /// Resolution activity
    pub resolution: FieldResolution,

    /// Lineage tree (if max_depth > 0)
    pub lineage_tree: Option<LineageTree>,
}

/// Lineage tree for recursive field dependencies
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageTree {
    /// Current field value
    pub field_value: FieldValue,

    /// Source values that contributed
    pub sources: Vec<SourceValue>,

    /// Child lineage (for derived fields)
    pub children: Vec<LineageTree>,
}

/// Statistics about field resolutions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldResolutionStats {
    /// Total resolutions
    pub total_resolutions: u64,

    /// Resolutions by strategy
    pub by_strategy: HashMap<StrategyType, u64>,

    /// Average confidence by strategy
    pub avg_confidence: HashMap<StrategyType, f64>,

    /// Conflict count
    pub total_conflicts: u64,

    /// Conflicts requiring review
    pub conflicts_requiring_review: u64,

    /// Average resolution time (milliseconds)
    pub avg_resolution_time_ms: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_field_value_creation() {
        let value = FieldValue {
            entity_id: "cust_001".to_string(),
            field_name: "address".to_string(),
            value: serde_json::json!("123 Main St"),
            value_type: "string".to_string(),
            confidence: 0.95,
            resolved_at: Utc::now(),
            valid_from: Utc::now(),
            valid_to: None,
            supersedes: None,
            explanation: Some("Selected by frequency voting".to_string()),
            resolution_id: "res_123".to_string(),
        };

        assert_eq!(value.entity_id, "cust_001");
        assert_eq!(value.confidence, 0.95);
        assert!(value.valid_to.is_none());
    }

    #[test]
    fn test_source_value_creation() {
        let source = SourceValue {
            id: "src_1".to_string(),
            value: serde_json::json!("john@example.com"),
            source_system: "CRM".to_string(),
            source_timestamp: Utc::now(),
            source_authority: 0.9,
            confidence: Some(0.85),
            vote_count: 3,
            vote_weight: 2.7,
            metadata: HashMap::new(),
        };

        assert_eq!(source.source_system, "CRM");
        assert_eq!(source.vote_count, 3);
        assert_eq!(source.vote_weight, 2.7);
    }

    #[test]
    fn test_strategy_type_confidence_thresholds() {
        assert_eq!(StrategyType::Frequency.default_confidence_threshold(), 0.60);
        assert_eq!(StrategyType::TimeDecay.default_confidence_threshold(), 0.70);
        assert_eq!(StrategyType::Authority.default_confidence_threshold(), 0.80);
        assert_eq!(
            StrategyType::MlPrediction.default_confidence_threshold(),
            0.85
        );
    }

    #[test]
    fn test_conflict_severity_review() {
        assert!(!ConflictSeverity::Low.requires_human_review());
        assert!(!ConflictSeverity::Medium.requires_human_review());
        assert!(ConflictSeverity::High.requires_human_review());
        assert!(ConflictSeverity::Critical.requires_human_review());
    }

    #[test]
    fn test_conflict_severity_penalty() {
        assert_eq!(ConflictSeverity::Low.confidence_penalty(), 0.05);
        assert_eq!(ConflictSeverity::Medium.confidence_penalty(), 0.10);
        assert_eq!(ConflictSeverity::High.confidence_penalty(), 0.20);
        assert_eq!(ConflictSeverity::Critical.confidence_penalty(), 0.40);
    }

    #[test]
    fn test_voting_strategy_serialization() {
        let strategy = VotingStrategy {
            strategy_type: StrategyType::Frequency,
            parameters: serde_json::json!({}),
            description: "Majority vote wins".to_string(),
        };

        let json = serde_json::to_string(&strategy).unwrap();
        assert!(json.contains("frequency"));

        let deserialized: VotingStrategy = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.strategy_type, StrategyType::Frequency);
    }

    #[test]
    fn test_field_lineage_query() {
        let query = FieldLineageQuery {
            entity_id: "cust_001".to_string(),
            field_name: "address".to_string(),
            as_of: None,
            include_sources: true,
            include_voting: true,
            include_conflicts: false,
            max_depth: Some(3),
        };

        assert_eq!(query.entity_id, "cust_001");
        assert!(query.include_sources);
        assert_eq!(query.max_depth, Some(3));
    }
}
