//! OpenAPI documentation for Ontology Management API
//!
//! This module aggregates all custom ontology management endpoints into a single OpenAPI specification.

use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    paths(
        crate::api::ontology::handlers::register_ontology,
        crate::api::ontology::handlers::get_ontology,
        crate::api::ontology::handlers::list_ontologies,
        crate::api::ontology::handlers::update_ontology,
        crate::api::ontology::handlers::delete_ontology,
        crate::api::ontology::handlers::get_merged_ontology,
        crate::api::ontology::handlers::validate_ontology,
        crate::api::ontology::handlers::get_ontology_tree,
        crate::api::ontology::handlers::activate_ontology,
    ),
    components(
        schemas(
            // Registration types
            crate::api::ontology::types::RegisterOntologyRequest,
            crate::api::ontology::types::RegisterOntologyResponse,
            // Retrieval types
            crate::api::ontology::types::OntologyResponse,
            crate::api::ontology::types::ListOntologiesResponse,
            // Update types
            crate::api::ontology::types::UpdateOntologyRequest,
            // Merge types
            crate::api::ontology::types::GetMergedOntologyRequest,
            crate::api::ontology::types::MergedOntologyResponse,
            // Validation types
            crate::api::ontology::types::ValidateOntologyRequest,
            crate::api::ontology::types::ValidateOntologyResponse,
            // Tree structure types
            crate::api::ontology::types::GetOntologyTreeRequest,
            crate::api::ontology::types::OntologyTreeResponse,
            crate::api::ontology::types::ClassNode,
            crate::api::ontology::types::PropertyNode,
            crate::api::ontology::types::IndividualNode,
            crate::api::ontology::types::TreeStats,
            crate::api::ontology::types::PropertyType,
            // Statistics
            crate::api::ontology::types::OntologyStats,
            // Error types
            crate::api::ontology::types::OntologyErrorResponse,
            // External types from graphica_core
            graphica_core::catalog::OntologyMetadata,
            graphica_core::catalog::ValidationStatus,
        )
    ),
    tags(
        (name = "Ontology Management", description = "Custom domain ontology registration, validation, and tree visualization with RDF/Turtle support"),
    ),
    info(
        title = "ARCXA Ontology Management API",
        version = "1.0.0",
        description = "REST API for managing custom domain ontologies with RDF/Turtle storage, SHACL validation, and hierarchical tree visualization",
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
pub struct OntologyApiDoc;
