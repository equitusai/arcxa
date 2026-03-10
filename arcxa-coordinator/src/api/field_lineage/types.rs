//! Field Lineage API Types
//!
//! Request and response types for field lineage endpoints.

use chrono::{DateTime, Utc};
use graphica_core::orchestration::field_lineage::{ConflictSeverity, StrategyType};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::ToSchema;

/// Request to create a golden record
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateResolvedEntityRequest {
    /// Entity ID
    pub entity_id: String,

    /// Fields with their source values
    /// Map of field_name -> list of source values
    pub fields: HashMap<String, Vec<SourceValueInput>>,

    /// Optional voting strategy override
    pub voting_strategy: Option<VotingStrategyInput>,

    /// Minimum confidence threshold (default: 0.70)
    pub min_confidence: Option<f64>,
}

/// Source value input
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SourceValueInput {
    /// Source system identifier
    pub source_system: String,

    /// Field value (JSON)
    pub value: serde_json::Value,

    /// Source authority weight (0.0-1.0)
    pub source_authority: f64,

    /// When this value was captured
    pub source_timestamp: DateTime<Utc>,

    /// Optional confidence score
    pub confidence: Option<f64>,

    /// Optional metadata
    pub metadata: Option<HashMap<String, String>>,
}

/// Voting strategy input
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct VotingStrategyInput {
    /// Strategy type
    pub strategy_type: StrategyType,

    /// Time decay rate (for TimeDecay strategy)
    pub decay_rate: Option<f64>,

    /// Reference time (for TimeDecay strategy)
    pub reference_time: Option<DateTime<Utc>>,
}

/// Golden record response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ResolvedEntityResponse {
    /// Entity ID
    pub entity_id: String,

    /// Resolved fields
    pub fields: HashMap<String, FieldValueResponse>,

    /// Overall confidence score
    pub overall_confidence: f64,

    /// Number of conflicts detected
    pub conflict_count: usize,

    /// Whether any field requires human review
    pub requires_review: bool,

    /// When the golden record was created
    pub created_at: DateTime<Utc>,

    /// Low confidence fields (below threshold)
    pub low_confidence_fields: Vec<String>,

    /// Conflicting fields
    pub conflicting_fields: Vec<String>,
}

/// Field value in response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FieldValueResponse {
    /// Field name
    pub field_name: String,

    /// Resolved value
    pub value: serde_json::Value,

    /// Value type
    pub value_type: String,

    /// Confidence score
    pub confidence: f64,

    /// When this value was resolved
    pub resolved_at: DateTime<Utc>,

    /// Validity period start
    pub valid_from: DateTime<Utc>,

    /// Validity period end (if superseded)
    pub valid_to: Option<DateTime<Utc>>,

    /// Explanation of how this value was selected
    pub explanation: Option<String>,
}

/// Field lineage response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FieldLineageResponse {
    /// Entity ID
    pub entity_id: String,

    /// Field name
    pub field_name: String,

    /// Current field value
    pub current_value: FieldValueResponse,

    /// Resolution details
    pub resolution: FieldResolutionResponse,
}

/// Field resolution details
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FieldResolutionResponse {
    /// Resolution ID
    pub id: String,

    /// When resolved
    pub resolved_at: DateTime<Utc>,

    /// Who/what resolved it
    pub resolved_by: String,

    /// Voting strategy used
    pub strategy: VotingStrategyResponse,

    /// Source values considered
    pub source_values: Vec<SourceValueResponse>,

    /// Selected value
    pub selected_value: SourceValueResponse,

    /// Rejected values
    pub rejected_values: Vec<SourceValueResponse>,

    /// Explanation
    pub explanation: String,

    /// Conflict (if any)
    pub conflict: Option<FieldConflictResponse>,
}

/// Voting strategy in response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct VotingStrategyResponse {
    /// Strategy type
    pub strategy_type: StrategyType,

    /// Configuration (JSON)
    pub config: serde_json::Value,
}

/// Source value in response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SourceValueResponse {
    /// Source ID
    pub id: String,

    /// Value
    pub value: serde_json::Value,

    /// Source system
    pub source_system: String,

    /// Source timestamp
    pub source_timestamp: DateTime<Utc>,

    /// Source authority
    pub source_authority: f64,

    /// Confidence (if available)
    pub confidence: Option<f64>,

    /// Vote count
    pub vote_count: u32,

    /// Vote weight
    pub vote_weight: f64,

    /// Metadata
    pub metadata: HashMap<String, String>,
}

/// Field conflict in response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FieldConflictResponse {
    /// Conflict ID
    pub id: String,

    /// Severity
    pub severity: ConflictSeverity,

    /// Reason
    pub reason: String,

    /// Requires human review
    pub requires_review: bool,

    /// Conflicting values
    pub conflicting_values: Vec<SourceValueResponse>,
}

/// Field history response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FieldHistoryResponse {
    /// Entity ID
    pub entity_id: String,

    /// Field name
    pub field_name: String,

    /// Historical values (ordered by valid_from DESC)
    pub history: Vec<FieldValueResponse>,
}

/// Conflict list item
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ConflictListItem {
    /// Entity ID
    pub entity_id: String,

    /// Field name
    pub field_name: String,

    /// Conflict severity
    pub severity: ConflictSeverity,

    /// Reason
    pub reason: String,

    /// When the conflict was detected
    pub resolved_at: DateTime<Utc>,
}

/// Conflicts list response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ConflictsListResponse {
    /// Conflicts requiring review
    pub conflicts: Vec<ConflictListItem>,

    /// Total count
    pub total: usize,
}

/// Request to resolve a field conflict
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ResolveFieldConflictRequest {
    /// Source values to resolve
    pub source_values: Vec<SourceValueInput>,

    /// Voting strategy to use
    pub voting_strategy: Option<VotingStrategyInput>,
}
