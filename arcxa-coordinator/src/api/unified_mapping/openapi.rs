//! OpenAPI documentation for Unified Mapping API
//!
//! This module aggregates all unified mapping, conflict resolution, and database loading
//! endpoints into a single OpenAPI specification.

use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    paths(
        // AI/ML field mapping suggestions
        crate::api::unified_mapping::field_similarity::suggest_field_mappings,
        crate::api::unified_mapping::handlers::plan_goal_sql,
        crate::api::unified_mapping::handlers::upsert_ontology_bindings,
        crate::api::unified_mapping::handlers::list_ontology_bindings,
        crate::api::unified_mapping::handlers::binding_history,
        crate::api::unified_mapping::handlers::binding_coverage,
        // Session management
        crate::api::unified_mapping::handlers::create_unified_session,
        crate::api::unified_mapping::handlers::list_unified_sessions,
        crate::api::unified_mapping::handlers::get_unified_session,
        crate::api::unified_mapping::handlers::update_unified_session,
        crate::api::unified_mapping::handlers::delete_unified_session,
        // Conflict resolution
        crate::api::unified_mapping::handlers::resolve_conflicts,
        // Database loading
        crate::api::unified_mapping::handlers::load_to_database,
        crate::api::unified_mapping::handlers::get_load_job_status,
        crate::api::unified_mapping::handlers::external_load_job_callback,
        // Statistics
        crate::api::unified_mapping::handlers::get_global_statistics,
    ),
    components(
        schemas(
            // Request types
            crate::api::unified_mapping::types::CreateUnifiedSessionRequest,
            crate::api::unified_mapping::types::UpdateUnifiedSessionRequest,
            crate::api::unified_mapping::types::ResolveConflictsRequest,
            crate::api::unified_mapping::types::ConflictResolutionChoice,
            crate::api::unified_mapping::types::LoadToDatabaseRequest,
            crate::api::unified_mapping::types::ExternalLoadJobCallbackRequest,
            crate::api::unified_mapping::types::ExternalLoadJobCallbackStatus,
            crate::api::unified_mapping::types::PlanGoalSqlRequest,
            crate::api::unified_mapping::types::GoalSqlFilter,
            crate::api::unified_mapping::types::GoalSqlBinding,
            crate::api::unified_mapping::types::GoalBindingStrategy,
            crate::api::unified_mapping::types::SqlDialect,
            crate::api::unified_mapping::types::UpsertOntologyBindingsRequest,
            crate::api::unified_mapping::types::OntologyBindingInput,
            crate::api::unified_mapping::types::BindingProvenanceInput,
            crate::api::unified_mapping::types::ListOntologyBindingsQuery,
            crate::api::unified_mapping::types::BindingHistoryQuery,
            crate::api::unified_mapping::types::BindingCoverageRequest,
            crate::api::unified_mapping::types::DatabaseType,
            crate::api::unified_mapping::types::DatabaseConnectionConfig,
            crate::api::unified_mapping::types::ListUnifiedSessionsQuery,
            // Response types
            crate::api::unified_mapping::types::UnifiedSessionResponse,
            crate::api::unified_mapping::types::UnifiedFieldMappingDto,
            crate::api::unified_mapping::types::SourceFieldRefDto,
            crate::api::unified_mapping::types::TargetColumnRefDto,
            crate::api::unified_mapping::types::MappingConflictDto,
            crate::api::unified_mapping::types::SessionStatistics,
            crate::api::unified_mapping::types::ListUnifiedSessionsResponse,
            crate::api::unified_mapping::types::UnifiedSessionSummary,
            crate::api::unified_mapping::types::ResolveConflictsResponse,
            crate::api::unified_mapping::types::LoadToDatabaseResponse,
            crate::api::unified_mapping::types::LoadJobStatus,
            crate::api::unified_mapping::types::LoadJobStatusResponse,
            crate::api::unified_mapping::types::ExternalLoadJobCallbackResponse,
            crate::api::unified_mapping::types::LoadProgress,
            crate::api::unified_mapping::types::PlanGoalSqlResponse,
            crate::api::unified_mapping::types::ExplainMetadataResponse,
            crate::api::unified_mapping::types::PlannedJoinResponse,
            crate::api::unified_mapping::types::PlannedSqlParameterResponse,
            crate::api::unified_mapping::types::OntologyBindingResponse,
            crate::api::unified_mapping::types::UpsertOntologyBindingsResponse,
            crate::api::unified_mapping::types::ListOntologyBindingsResponse,
            crate::api::unified_mapping::types::BindingHistoryResponse,
            crate::api::unified_mapping::types::BindingCoverageResponse,
            crate::api::unified_mapping::types::GlobalStatisticsResponse,
            crate::api::unified_mapping::types::ErrorResponse,
            // Field similarity types
            crate::api::unified_mapping::field_similarity::SuggestMappingsRequest,
            crate::api::unified_mapping::field_similarity::DatasetInput,
            crate::api::unified_mapping::field_similarity::SuggestMappingsResponse,
            crate::api::unified_mapping::field_similarity::ApiErrorResponse,
            // Re-exported types from multi_source
            crate::api::unified_mapping::types::ConflictResolution,
            crate::api::unified_mapping::types::UnifiedSessionStatus,
            crate::api::unified_mapping::types::TargetDatabaseConfig,
            crate::api::unified_mapping::types::UnifiedFieldMapping,
            crate::api::unified_mapping::types::TargetTableConfig,
            crate::api::unified_mapping::types::TargetColumnConfig,
            crate::api::unified_mapping::types::ForeignKeyConfig,
            crate::api::unified_mapping::types::SourceFieldRef,
            crate::api::unified_mapping::types::TargetColumnRef,
            // Re-exported types from graphica-core
            crate::api::unified_mapping::types::DatasetSchema,
            crate::api::unified_mapping::types::MappingSuggestions,
            crate::api::unified_mapping::types::FieldSimilarity,
            crate::api::unified_mapping::types::FieldMetadata,
            crate::api::unified_mapping::types::SimilarityScores,
            crate::api::unified_mapping::types::RelationshipType,
            crate::api::unified_mapping::types::FieldProfile,
            crate::api::unified_mapping::types::MappingEvidence,
            crate::api::unified_mapping::types::EvidenceType,
            crate::api::unified_mapping::types::DataType,
            crate::api::unified_mapping::types::JoinDirection,
            crate::api::unified_mapping::types::Cardinality,
            crate::api::unified_mapping::types::ValueDistribution,
        )
    ),
    tags(
        (name = "Unified Mapping", description = "Multi-source CSV consolidation with AI/ML field similarity, conflict resolution, and batch loading to PostgreSQL/DB2/Oracle/Databricks. External executor callbacks apply only to DB2; PostgreSQL, Oracle, and Databricks run through internal catalog-backed loader paths."),
    ),
    info(
        title = "ARCXA Unified Mapping API",
        version = "1.0.0",
        description = "REST API for consolidating multiple CSV sources into unified normalized schemas with AI-powered field matching and multi-database support",
        contact(
            name = "ARCXA Team",
            email = "avinam@equitus.us"
        )
    ),
    servers(
        (url = "http://localhost:8080", description = "Development server"),
        (url = "https://api.graphica.io", description = "Production server")
    )
)]
pub struct UnifiedMappingApiDoc;
