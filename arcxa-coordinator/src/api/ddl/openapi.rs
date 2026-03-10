//! OpenAPI documentation for DDL API
//!
//! This module aggregates all DDL generation endpoints into a single OpenAPI specification.

use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    paths(
        crate::api::ddl::handlers::generate_ddl,
        crate::api::ddl::handlers::generate_migration,
        crate::api::ddl::handlers::validate_ddl,
        crate::api::ddl::handlers::list_shapes,
        crate::api::ddl::handlers::execute_ddl,
    ),
    components(
        schemas(
            // DDL generation types
            crate::api::ddl::types::GenerateDdlRequest,
            crate::api::ddl::types::GenerateDdlResponse,
            // Migration types
            crate::api::ddl::types::GenerateMigrationRequest,
            crate::api::ddl::types::GenerateMigrationResponse,
            // Validation types
            crate::api::ddl::types::ValidateDdlRequest,
            crate::api::ddl::types::ValidateDdlResponse,
            // Shape listing types
            crate::api::ddl::types::ListShapesRequest,
            crate::api::ddl::types::ListShapesResponse,
            crate::api::ddl::types::ShapeInfo,
            // Execution types
            crate::api::ddl::types::ExecuteDdlRequest,
            crate::api::ddl::types::ExecuteDdlResponse,
            crate::api::ddl::types::DatabaseConnectionConfig,
            crate::api::ddl::types::DatabaseType,
            crate::api::ddl::types::DdlExecutionError,
        )
    ),
    tags(
        (name = "DDL Generation", description = "SHACL-driven DDL generation, migration, and database execution"),
    ),
    info(
        title = "ARCXA DDL API",
        version = "1.0.0",
        description = "REST API for generating database DDL from SHACL shapes with multi-dialect support",
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
pub struct DdlApiDoc;
