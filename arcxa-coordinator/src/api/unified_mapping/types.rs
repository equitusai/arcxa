//! Unified Mapping API Types
//!
//! Request and response types for unified mapping API endpoints.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::{IntoParams, ToSchema};

// Re-export types for OpenAPI schema generation
pub use crate::mapping::multi_source::types::{
    ConflictResolution, ForeignKeyConfig, MappingConflict, SourceFieldRef, TargetColumnConfig,
    TargetColumnRef, TargetDatabaseConfig, TargetTableConfig, UnifiedFieldMapping,
    UnifiedMappingSession, UnifiedSessionStatus,
};
pub use graphica_core::inference::mapping::{
    Cardinality, DataType, DatasetSchema, EvidenceType, FieldMetadata, FieldProfile,
    FieldSimilarity, JoinDirection, MappingEvidence, MappingSuggestions, RelationshipType,
    SimilarityScores, ValueDistribution,
};

// ============================================================================
// Request Types
// ============================================================================

/// Request to create a new unified mapping session
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateUnifiedSessionRequest {
    /// IDs of source mapping sessions to consolidate
    pub source_session_ids: Vec<String>,

    /// Target database configuration
    pub target_database: TargetDatabaseConfig,

    /// User creating the session
    pub created_by: String,
}

/// Request to update a unified mapping session
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateUnifiedSessionRequest {
    /// Updated target database configuration (optional)
    pub target_database: Option<TargetDatabaseConfig>,

    /// Updated field mappings (optional)
    pub field_mappings: Option<Vec<UnifiedFieldMapping>>,
}

/// Request to resolve conflicts in a unified session
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ResolveConflictsRequest {
    /// Map of conflict ID to resolution strategy
    pub resolutions: HashMap<String, ConflictResolutionChoice>,
}

/// Conflict resolution choice
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ConflictResolutionChoice {
    /// Resolution strategy to apply
    pub strategy: ConflictResolution,

    /// Optional parameters for the resolution (e.g., primary source ID, merge separator)
    pub parameters: Option<HashMap<String, String>>,
}

/// Request to load unified session to database
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LoadToDatabaseRequest {
    /// Target database type (PostgreSQL, DB2, Oracle, Databricks)
    pub database_type: DatabaseType,

    /// Connection string or configuration
    pub connection_config: DatabaseConnectionConfig,

    /// Batch size for bulk loading
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,

    /// Whether to create tables if they don't exist
    #[serde(default = "default_true")]
    pub create_tables: bool,

    /// Whether to validate data before loading
    #[serde(default = "default_true")]
    pub validate_data: bool,
}

/// Callback request from external executors (DB2/Oracle/Databricks)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ExternalLoadJobCallbackRequest {
    /// New status being reported by the external executor
    pub status: ExternalLoadJobCallbackStatus,

    /// Backend-native run/statement identifier
    pub external_run_id: Option<String>,

    /// Optional backend message
    pub message: Option<String>,

    /// Optional progress snapshot
    pub progress: Option<LoadProgress>,
}

/// Allowed callback statuses from external executors
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExternalLoadJobCallbackStatus {
    Running,
    Submitted,
    Completed,
    Failed,
    Cancelled,
}

/// Request to generate goal-driven SQL from ontology property requirements.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PlanGoalSqlRequest {
    /// Source data source ID used for schema introspection.
    pub source_id: String,

    /// Optional table filter during schema inference.
    pub table_name: Option<String>,

    /// Number of rows to sample during schema inference.
    #[serde(default = "default_sample_size")]
    pub sample_size: usize,

    /// Ontology entity URI for the goal.
    pub entity_uri: String,

    /// Required ontology properties for this goal.
    pub required_properties: Vec<String>,

    /// Optional property filters.
    #[serde(default)]
    pub filters: Vec<GoalSqlFilter>,

    /// Binding lookup strategy for planning.
    #[serde(default)]
    pub binding_strategy: GoalBindingStrategy,

    /// SQL dialect to render.
    #[serde(default)]
    pub sql_dialect: SqlDialect,

    /// Include dialect-specific explain-plan SQL hook in response.
    #[serde(default)]
    pub include_explain_plan: bool,

    /// Selected ontology->physical bindings used to build SQL.
    #[serde(default)]
    pub bindings: Vec<GoalSqlBinding>,

    /// Optional result limit.
    pub limit: Option<usize>,
}

/// Filter for a goal property.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GoalSqlFilter {
    pub ontology_uri: String,
    pub value: String,
}

/// Ontology property to physical field binding.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GoalSqlBinding {
    pub ontology_uri: String,
    pub table: String,
    pub column: String,
    pub confidence: f64,
}

/// Strategy for sourcing planner bindings.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum GoalBindingStrategy {
    Inline,
    Stored,
}

impl Default for GoalBindingStrategy {
    fn default() -> Self {
        Self::Inline
    }
}

/// SQL dialect hint for goal planning output.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SqlDialect {
    Postgresql,
    Edb,
    Oracle,
    Saphana,
    Db2,
    Databricks,
}

impl Default for SqlDialect {
    fn default() -> Self {
        Self::Postgresql
    }
}

fn default_batch_size() -> usize {
    1000
}

fn default_sample_size() -> usize {
    1000
}

fn default_true() -> bool {
    true
}

/// Database type enumeration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DatabaseType {
    PostgreSQL,
    DB2,
    Oracle,
    Databricks,
}

/// Database connection configuration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DatabaseConnectionConfig {
    /// Host or connection string
    pub host: String,

    /// Port number
    pub port: u16,

    /// Database name
    pub database: String,

    /// Username
    pub username: String,

    /// Password (will be encrypted in transit)
    pub password: String,

    /// Optional SSL/TLS configuration
    pub ssl_mode: Option<String>,

    /// Optional connection pool size
    pub pool_size: Option<u32>,
}

/// Query parameters for listing unified sessions
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, IntoParams)]
pub struct ListUnifiedSessionsQuery {
    /// Filter by status
    pub status: Option<UnifiedSessionStatus>,

    /// Filter by created_by user
    pub created_by: Option<String>,

    /// Page offset for pagination
    #[serde(default)]
    pub offset: usize,

    /// Page limit for pagination
    #[serde(default = "default_page_limit")]
    pub limit: usize,
}

fn default_page_limit() -> usize {
    50
}

// ============================================================================
// Response Types
// ============================================================================

/// Response for created unified session
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UnifiedSessionResponse {
    /// Session ID
    pub id: String,

    /// Source session IDs
    pub source_sessions: Vec<String>,

    /// Target database configuration
    pub target_database: TargetDatabaseConfig,

    /// Field mappings
    pub field_mappings: Vec<UnifiedFieldMappingDto>,

    /// Detected conflicts
    pub conflicts: Vec<MappingConflictDto>,

    /// Session status
    pub status: UnifiedSessionStatus,

    /// Created timestamp (Unix epoch)
    pub created_at: i64,

    /// Created by user
    pub created_by: String,

    /// Updated timestamp (Unix epoch)
    pub updated_at: i64,

    /// Statistics summary
    pub stats: SessionStatistics,
}

impl From<UnifiedMappingSession> for UnifiedSessionResponse {
    fn from(session: UnifiedMappingSession) -> Self {
        let conflicts: Vec<MappingConflictDto> =
            session.conflicts.iter().map(|c| c.clone().into()).collect();

        let field_mappings: Vec<UnifiedFieldMappingDto> = session
            .field_mappings
            .iter()
            .map(|m| m.clone().into())
            .collect();

        let stats = SessionStatistics {
            total_field_mappings: field_mappings.len(),
            total_conflicts: conflicts.len(),
            unresolved_conflicts: conflicts.iter().filter(|c| !c.is_resolved).count(),
            total_source_sessions: session.source_sessions.len(),
        };

        Self {
            id: session.id,
            source_sessions: session.source_sessions,
            target_database: session.target_database,
            field_mappings,
            conflicts,
            status: session.status,
            created_at: session.created_at,
            created_by: session.created_by,
            updated_at: session.updated_at,
            stats,
        }
    }
}

/// Unified field mapping DTO
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UnifiedFieldMappingDto {
    pub id: String,
    pub source_fields: Vec<SourceFieldRefDto>,
    pub ontology_term_uri: String,
    pub target_column: TargetColumnRefDto,
    pub conflict_resolution: ConflictResolution,
    pub transformation: Option<String>,
    pub confidence: f64,
}

impl From<UnifiedFieldMapping> for UnifiedFieldMappingDto {
    fn from(mapping: UnifiedFieldMapping) -> Self {
        Self {
            id: mapping.id,
            source_fields: mapping
                .source_fields
                .into_iter()
                .map(|s| s.into())
                .collect(),
            ontology_term_uri: mapping.ontology_term_uri,
            target_column: mapping.target_column.into(),
            conflict_resolution: mapping.conflict_resolution,
            transformation: mapping.transformation,
            confidence: mapping.confidence,
        }
    }
}

/// Source field reference DTO
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SourceFieldRefDto {
    pub session_id: String,
    pub datasource_id: String,
    pub table_name: String,
    pub field_name: String,
    pub source_data_type: String,
}

impl From<crate::mapping::multi_source::types::SourceFieldRef> for SourceFieldRefDto {
    fn from(r: crate::mapping::multi_source::types::SourceFieldRef) -> Self {
        Self {
            session_id: r.session_id,
            datasource_id: r.datasource_id,
            table_name: r.table_name,
            field_name: r.field_name,
            source_data_type: r.source_data_type,
        }
    }
}

/// Target column reference DTO
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TargetColumnRefDto {
    pub table_name: String,
    pub column_name: String,
    pub data_type: String,
}

impl From<crate::mapping::multi_source::types::TargetColumnRef> for TargetColumnRefDto {
    fn from(r: crate::mapping::multi_source::types::TargetColumnRef) -> Self {
        Self {
            table_name: r.table_name,
            column_name: r.column_name,
            data_type: r.data_type,
        }
    }
}

/// Mapping conflict DTO
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MappingConflictDto {
    pub id: String,
    pub ontology_term_uri: String,
    pub conflicting_source_fields: Vec<SourceFieldRefDto>,
    pub suggested_resolution: ConflictResolution,
    pub is_resolved: bool,
}

impl From<MappingConflict> for MappingConflictDto {
    fn from(conflict: MappingConflict) -> Self {
        Self {
            id: conflict.id,
            ontology_term_uri: conflict.ontology_term_uri,
            conflicting_source_fields: conflict
                .conflicting_sources
                .into_iter()
                .map(|s| s.into())
                .collect(),
            suggested_resolution: conflict.suggested_resolution,
            is_resolved: conflict.resolved,
        }
    }
}

/// Session statistics
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SessionStatistics {
    pub total_field_mappings: usize,
    pub total_conflicts: usize,
    pub unresolved_conflicts: usize,
    pub total_source_sessions: usize,
}

/// List of unified sessions response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ListUnifiedSessionsResponse {
    pub sessions: Vec<UnifiedSessionSummary>,
    pub total_count: usize,
    pub offset: usize,
    pub limit: usize,
}

/// Unified session summary (for list endpoints)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UnifiedSessionSummary {
    pub id: String,
    pub source_sessions: Vec<String>,
    pub status: UnifiedSessionStatus,
    pub created_at: i64,
    pub created_by: String,
    pub updated_at: i64,
    pub stats: SessionStatistics,
}

/// Response for conflict resolution operation
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ResolveConflictsResponse {
    pub session_id: String,
    pub conflicts_resolved: usize,
    pub remaining_conflicts: usize,
    pub new_status: UnifiedSessionStatus,
}

/// Response for database load operation
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LoadToDatabaseResponse {
    pub session_id: String,
    pub load_job_id: String,
    pub status: LoadJobStatus,
    pub message: String,
}

/// Load job status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum LoadJobStatus {
    /// Job queued for execution
    Queued,
    /// Job is running
    Running,
    /// Job has been submitted to an external executor and is awaiting completion callback
    Submitted,
    /// Job completed successfully
    Completed,
    /// Job failed with errors
    Failed,
    /// Job cancelled by user
    Cancelled,
}

/// Detailed load job status response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LoadJobStatusResponse {
    pub job_id: String,
    pub session_id: String,
    pub database_type: Option<String>,
    pub status: LoadJobStatus,
    pub progress: LoadProgress,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
    pub error_message: Option<String>,
    pub external_run_id: Option<String>,
}

/// Response after processing an external load-job callback.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ExternalLoadJobCallbackResponse {
    pub job_id: String,
    pub session_id: String,
    pub status: LoadJobStatus,
    pub message: String,
}

/// Load progress details
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LoadProgress {
    pub total_rows: usize,
    pub rows_processed: usize,
    pub rows_succeeded: usize,
    pub rows_failed: usize,
    pub percentage_complete: f64,
}

/// Response for goal-driven SQL planning.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PlanGoalSqlResponse {
    pub source_id: String,
    pub schema_name: String,
    pub sql_dialect: SqlDialect,
    pub binding_strategy: GoalBindingStrategy,
    pub sql: String,
    pub explain_sql: Option<String>,
    pub explain_metadata: Option<ExplainMetadataResponse>,
    pub selected_tables: Vec<String>,
    pub covered_properties: Vec<String>,
    pub missing_properties: Vec<String>,
    pub joins: Vec<PlannedJoinResponse>,
    pub parameters: Vec<PlannedSqlParameterResponse>,
}

/// Explain-plan metadata to guide engine-specific retrieval/execution behavior.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ExplainMetadataResponse {
    /// Explain execution mode for the target engine.
    pub mode: String,
    /// Optional follow-up query required to read explain output.
    pub follow_up_query: Option<String>,
    /// Additional notes for engine-specific behavior.
    pub notes: Vec<String>,
}

/// Request to diff ontology requirements against currently available physical bindings.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BindingCoverageRequest {
    pub source_id: String,
    pub entity_uri: String,
    pub required_properties: Vec<String>,
    pub table_name: Option<String>,
    #[serde(default = "default_sample_size")]
    pub sample_size: usize,
    #[serde(default = "default_true")]
    pub validate_schema: bool,
}

/// Coverage diff response for ontology requirement validation.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BindingCoverageResponse {
    pub source_id: String,
    pub entity_uri: String,
    pub required_properties: Vec<String>,
    pub covered_properties: Vec<String>,
    pub missing_properties: Vec<String>,
    pub stale_properties: Vec<String>,
    pub unmapped_properties: Vec<String>,
    pub coverage_ratio: f64,
}

/// Planned join details.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PlannedJoinResponse {
    pub from_table: String,
    pub to_table: String,
    pub condition: String,
}

/// Parameter metadata for parameterized SQL plans.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PlannedSqlParameterResponse {
    pub index: usize,
    pub placeholder: String,
    pub ontology_uri: String,
    pub value: String,
    pub data_type: Option<String>,
}

/// Upsert request for ontology->physical bindings.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpsertOntologyBindingsRequest {
    pub source_id: String,
    pub entity_uri: String,
    pub updated_by: String,
    pub bindings: Vec<OntologyBindingInput>,
}

/// A single ontology binding input entry.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OntologyBindingInput {
    pub ontology_uri: String,
    pub table: String,
    pub column: String,
    pub sql_dialect: SqlDialect,
    pub confidence: f64,
    pub provenance: Option<BindingProvenanceInput>,
}

/// Provenance metadata accepted from API clients.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BindingProvenanceInput {
    pub workflow_id: Option<String>,
    pub session_id: Option<String>,
    pub approved_by: Option<String>,
    pub approval_reason: Option<String>,
    pub observed_schema_hash: Option<String>,
}

/// Current/history response record for ontology bindings.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OntologyBindingResponse {
    pub id: String,
    pub source_id: String,
    pub entity_uri: String,
    pub ontology_uri: String,
    pub table: String,
    pub column: String,
    pub sql_dialect: SqlDialect,
    pub confidence: f64,
    pub status: String,
    pub version: u32,
    pub binding_hash: String,
    pub updated_at: i64,
    pub updated_by: String,
}

/// Response after upserting bindings.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpsertOntologyBindingsResponse {
    pub updated: Vec<OntologyBindingResponse>,
}

/// Query for listing current bindings by source/entity.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, IntoParams)]
pub struct ListOntologyBindingsQuery {
    pub source_id: String,
    pub entity_uri: String,
}

/// Query for fetching version history of a single ontology property binding.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, IntoParams)]
pub struct BindingHistoryQuery {
    pub source_id: String,
    pub entity_uri: String,
    pub ontology_uri: String,
}

/// Current bindings query response.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ListOntologyBindingsResponse {
    pub bindings: Vec<OntologyBindingResponse>,
}

/// Binding history query response.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BindingHistoryResponse {
    pub history: Vec<OntologyBindingResponse>,
}

/// Statistics response for all unified sessions
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GlobalStatisticsResponse {
    pub total_sessions: usize,
    pub sessions_by_status: HashMap<String, usize>,
    pub total_conflicts: usize,
    pub total_field_mappings: usize,
    pub database_types: HashMap<String, usize>,
}

// ============================================================================
// Error Response Type
// ============================================================================

/// API error response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ErrorResponse {
    pub error: String,
    pub status: u16,
    pub details: Option<HashMap<String, String>>,
}
