//! OpenAPI documentation for R2RML Mapping API
//!
//! This module aggregates all R2RML mapping endpoints (Stage 2: Semantic Mapping)
//! including CRUD operations, execution, validation, and AI-powered suggestion.

use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    paths(
        // Mapping CRUD operations
        crate::api::r2rml::handlers::create_mapping,
        crate::api::r2rml::handlers::list_mappings,
        crate::api::r2rml::handlers::get_mapping,
        crate::api::r2rml::handlers::update_mapping,
        crate::api::r2rml::handlers::delete_mapping,
        // Mapping execution
        crate::api::r2rml::handlers::execute_mapping,
        // AI-powered suggestion
        crate::api::r2rml::handlers::suggest_mapping_from_profile,
        // Validation
        crate::api::r2rml::handlers::validate_mapping,
    ),
    components(
        schemas(
            // Request types
            crate::api::r2rml::types::CreateMappingRequest,
            crate::api::r2rml::types::UpdateMappingRequest,
            crate::api::r2rml::types::ExecuteMappingRequest,
            crate::api::r2rml::types::SuggestMappingRequest,
            crate::api::r2rml::types::ValidateMappingRequest,
            // Response types
            crate::api::r2rml::types::CreateMappingResponse,
            crate::api::r2rml::types::UpdateMappingResponse,
            crate::api::r2rml::types::GetMappingResponse,
            crate::api::r2rml::types::ListMappingsResponse,
            crate::api::r2rml::types::MappingSummary,
            crate::api::r2rml::types::DeleteMappingResponse,
            crate::api::r2rml::types::ExecuteMappingResponse,
            crate::api::r2rml::types::SuggestMappingResponse,
            crate::api::r2rml::types::ColumnSuggestion,
            crate::api::r2rml::types::ValidateMappingResponse,
            crate::api::r2rml::types::ValidationError,
            crate::api::r2rml::types::ValidationWarning,
            // R2RML core types
            crate::mapping::semantic_mapping::rdf::r2rml_types::R2rmlMapping,
            crate::mapping::semantic_mapping::rdf::r2rml_types::TriplesMap,
            crate::mapping::semantic_mapping::rdf::r2rml_types::SubjectMap,
            crate::mapping::semantic_mapping::rdf::r2rml_types::PredicateObjectMap,
            crate::mapping::semantic_mapping::rdf::r2rml_types::ObjectMap,
            crate::mapping::semantic_mapping::rdf::r2rml_types::LogicalTable,
            crate::mapping::semantic_mapping::rdf::r2rml_types::TermType,
            crate::mapping::semantic_mapping::rdf::r2rml_types::GraphMap,
            crate::mapping::semantic_mapping::rdf::r2rml_types::PredicateSpec,
            crate::mapping::semantic_mapping::rdf::r2rml_types::JoinCondition,
            // Profiling types
            crate::mapping::profiling::types::ProfileResult,
            crate::mapping::profiling::types::ColumnProfile,
        )
    ),
    tags(
        (name = "R2RML Mappings", description = "W3C R2RML mapping management for CSV-to-RDF transformation with AI-powered suggestions"),
    ),
    info(
        title = "ARCXA R2RML Mapping API",
        version = "1.0.0",
        description = "REST API for managing W3C R2RML mappings (Stage 2: Semantic Mapping). Create, validate, and execute CSV-to-RDF transformations with AI-powered mapping suggestions from dataset profiles.",
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
pub struct R2rmlApiDoc;
