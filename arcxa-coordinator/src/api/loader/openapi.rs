//! OpenAPI documentation for Loader API
//!
//! This module aggregates all ETL loader endpoints into a single OpenAPI specification.

use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    paths(
        // Job management
        crate::api::loader::handlers::create_loader_job,
        crate::api::loader::handlers::get_job_status,
        crate::api::loader::handlers::list_loader_jobs,
        crate::api::loader::handlers::cancel_loader_job,
        // Job control
        crate::api::loader::handlers::resume_loader_job,
        // Checkpoint management
        crate::api::loader::handlers::get_checkpoint_status,
        // DLQ management
        crate::api::loader::handlers::get_dlq_stats,
        crate::api::loader::handlers::get_dlq_rows,
        crate::api::loader::handlers::reprocess_dlq_rows,
        // Health
        crate::api::loader::handlers::get_loader_health,
    ),
    components(
        schemas(
            // Job management types
            crate::api::loader::types::CreateLoaderJobRequest,
            crate::api::loader::types::TargetDatabaseConfig,
            crate::api::loader::types::DatabaseType,
            crate::api::loader::types::ColumnMappingDto,
            crate::api::loader::types::TransformationDto,
            crate::api::loader::types::LoaderConfigDto,
            crate::api::loader::types::CheckpointConfigDto,
            crate::api::loader::types::DlqConfigDto,
            crate::api::loader::types::DlqFormatDto,
            crate::api::loader::types::CreateLoaderJobResponse,
            crate::api::loader::types::JobStatus,
            // Job query/status types
            crate::api::loader::types::JobStatusResponse,
            crate::api::loader::types::JobProgressDto,
            crate::api::loader::types::CheckpointStatusDto,
            crate::api::loader::types::ErrorSummaryDto,
            crate::api::loader::types::ErrorRecordDto,
            crate::api::loader::types::DlqStatsDto,
            crate::api::loader::types::ListJobsResponse,
            crate::api::loader::types::JobSummaryDto,
            // Job control types
            crate::api::loader::types::ResumeJobRequest,
            crate::api::loader::types::ResumeJobResponse,
            crate::api::loader::types::CancelJobResponse,
            // DLQ query types
            crate::api::loader::types::GetDlqRowsQuery,
            crate::api::loader::types::GetDlqRowsResponse,
            crate::api::loader::types::FailedRowDto,
            crate::api::loader::types::ReprocessDlqRequest,
            crate::api::loader::types::ReprocessDlqResponse,
            // Health/stats types
            crate::api::loader::types::LoaderHealthResponse,
            crate::api::loader::types::HealthStatus,
        )
    ),
    tags(
        (name = "ETL Loader", description = "Enterprise ETL loader for CSV/DB data with checkpointing and DLQ"),
    ),
    info(
        title = "ARCXA Loader API",
        version = "1.0.0",
        description = "REST API for ETL data loading with checkpoint recovery and dead letter queue management",
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
pub struct LoaderApiDoc;
