//! OpenAPI documentation for Field Lineage API
//!
//! This module aggregates all field-level provenance, golden record creation,
//! and voting-based conflict resolution endpoints into a single OpenAPI specification.

use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    paths(
        // Field lineage and history
        crate::api::field_lineage::handlers::get_field_lineage,
        crate::api::field_lineage::handlers::get_field_history,
        // Golden record operations
        crate::api::field_lineage::handlers::create_resolved_entity,
        crate::api::field_lineage::handlers::get_resolved_entity,
        // Conflict management
        crate::api::field_lineage::handlers::list_conflicts_requiring_review,
        crate::api::field_lineage::handlers::resolve_field_conflict,
        // Cache metrics
        crate::api::field_lineage::handlers::get_cache_metrics,
    ),
    components(
        schemas(
            // Request types
            crate::api::field_lineage::types::CreateResolvedEntityRequest,
            crate::api::field_lineage::types::SourceValueInput,
            crate::api::field_lineage::types::VotingStrategyInput,
            crate::api::field_lineage::types::ResolveFieldConflictRequest,
            // Response types
            crate::api::field_lineage::types::ResolvedEntityResponse,
            crate::api::field_lineage::types::FieldValueResponse,
            crate::api::field_lineage::types::FieldLineageResponse,
            crate::api::field_lineage::types::FieldResolutionResponse,
            crate::api::field_lineage::types::VotingStrategyResponse,
            crate::api::field_lineage::types::SourceValueResponse,
            crate::api::field_lineage::types::FieldConflictResponse,
            crate::api::field_lineage::types::FieldHistoryResponse,
            crate::api::field_lineage::types::ConflictListItem,
            crate::api::field_lineage::types::ConflictsListResponse,
            // External types from graphica-core
            graphica_core::orchestration::field_lineage::StrategyType,
            graphica_core::orchestration::field_lineage::ConflictSeverity,
        )
    ),
    tags(
        (name = "Field Lineage", description = "Field-level provenance tracking, golden record creation, and voting-based conflict resolution"),
    ),
    info(
        title = "ARCXA Field Lineage API",
        version = "1.0.0",
        description = "REST API for field-level data lineage, golden record management, and multi-source conflict resolution with voting strategies",
        contact(
            name = "ARCXA Team",
            email = "support@graphica.io"
        ),
        license(
            name = "Apache 2.0",
            url = "https://www.apache.org/licenses/LICENSE-2.0"
        )
    ),
    servers(
        (url = "http://localhost:8080", description = "Development server"),
        (url = "https://api.graphica.io", description = "Production server")
    )
)]
pub struct FieldLineageApiDoc;
