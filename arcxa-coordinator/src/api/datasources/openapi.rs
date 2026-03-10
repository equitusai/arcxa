//! OpenAPI documentation for Data Source Catalog API
//!
//! This module aggregates all data source catalog endpoints into a single OpenAPI specification.

use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    paths(
        // CRUD operations
        crate::api::datasources::handlers::register_datasource,
        crate::api::datasources::handlers::list_datasources,
        crate::api::datasources::handlers::get_datasource,
        crate::api::datasources::handlers::update_datasource,
        crate::api::datasources::handlers::delete_datasource,
        // Operations
        crate::api::datasources::handlers::test_connection,
        crate::api::datasources::handlers::infer_schema,
        crate::api::datasources::handlers::infer_schema_enhanced,
        crate::api::datasources::handlers::execute_query,
        crate::api::datasources::handlers::search_datasources,
        // Admin
        crate::api::datasources::handlers::sync_datasources_to_rdf,
    ),
    components(
        schemas(
            // Request/Response types
            graphica_core::catalog::CreateDataSourceRequest,
            graphica_core::catalog::DataSourceResponse,
            graphica_core::catalog::DataSourceStatus,
            graphica_core::catalog::UpdateDataSourceRequest,
            graphica_core::catalog::ListDataSourcesRequest,
            graphica_core::catalog::ListDataSourcesResponse,
            graphica_core::catalog::TestConnectionRequest,
            graphica_core::catalog::ConnectionTestResult,
            graphica_core::catalog::InferSchemaRequest,
            graphica_core::catalog::SchemaDefinition,
            graphica_core::catalog::TableDefinition,
            graphica_core::catalog::ColumnDefinition,
            graphica_core::catalog::ExecuteQueryRequest,
            graphica_core::catalog::QueryResult,
            graphica_core::catalog::CatalogErrorResponse,
            // Core types
            graphica_core::catalog::types::DataSource,
            graphica_core::catalog::types::ConnectionDetails,
            graphica_core::catalog::types::SourceConfig,
            graphica_core::catalog::types::PostgreSQLConfig,
            graphica_core::catalog::types::MySQLConfig,
            graphica_core::catalog::types::OracleConfig,
            graphica_core::catalog::types::DB2Config,
            graphica_core::catalog::types::SAPHANAConfig,
            graphica_core::catalog::types::SnowflakeConfig,
            graphica_core::catalog::types::S3ParquetConfig,
            graphica_core::catalog::types::CsvFileConfig,
            graphica_core::catalog::types::RDFNTriplesConfig,
            // Inference types
            graphica_core::inference::types::SemanticType,
            graphica_core::inference::types::ColumnStatistics,
            graphica_core::inference::types::CardinalityClass,
            graphica_core::inference::types::Histogram,
            graphica_core::inference::types::HistogramBucket,
            graphica_core::inference::types::HistogramMethod,
            graphica_core::inference::types::ValueFrequency,
        )
    ),
    tags(
        (name = "datasources", description = "Data Source Catalog API for managing database and file connections"),
    ),
    info(
        title = "ARCXA Data Source Catalog API",
        version = "1.0.0",
        description = "REST API for managing data source connections with schema inference and connection testing",
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
pub struct DataSourceApiDoc;
