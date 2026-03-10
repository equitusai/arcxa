//! OpenAPI documentation for Governance API
//!
//! Provides SPARQL query execution and RDF store management endpoints.

use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    paths(
        crate::api::governance::handlers::sparql_query,
        crate::api::governance::handlers::get_rdf_stats,
        crate::api::governance::handlers::get_rdf_auto_save_stats,
        crate::api::governance::handlers::trigger_rdf_save,
    ),
    components(
        schemas(
            crate::api::dto::SparqlQuery,
            crate::api::dto::SparqlResults,
            crate::api::dto::SparqlResultRow,
            crate::api::dto::RdfAutoSaveStatsResponse,
            crate::api::dto::RdfSaveResponse,
            crate::api::governance::handlers::RdfStats,
            crate::api::dto::ApiErrorResponse,
        )
    ),
    tags(
        (name = "governance", description = "Governance and SPARQL API - RDF store management and query execution"),
    ),
)]
pub struct GovernanceApiDoc;
