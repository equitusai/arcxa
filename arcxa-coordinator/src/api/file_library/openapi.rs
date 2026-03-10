//! OpenAPI documentation for File Library API
//!
//! This module aggregates all file library endpoints into a single OpenAPI specification.

use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    paths(
        // File operations
        crate::api::file_library::handlers::list_files,
        crate::api::file_library::handlers::get_file,
        crate::api::file_library::handlers::create_file,
        crate::api::file_library::handlers::update_file,
        crate::api::file_library::handlers::delete_file,
        crate::api::file_library::handlers::scan_file,
        crate::api::file_library::handlers::validate_file_for_registration,
        crate::api::file_library::handlers::download_file,
        crate::api::file_library::handlers::preview_file,
        // Bulk operations
        crate::api::file_library::handlers::bulk_upload,
        crate::api::file_library::handlers::get_job_status,
        crate::api::file_library::handlers::bulk_update,
        crate::api::file_library::handlers::bulk_delete,
        crate::api::file_library::handlers::bulk_scan,
        // Folder operations
        crate::api::file_library::handlers::list_folders,
        crate::api::file_library::handlers::create_folder,
        crate::api::file_library::handlers::update_folder,
        crate::api::file_library::handlers::delete_folder,
        // Search & Tags
        crate::api::file_library::handlers::list_tags,
        crate::api::file_library::handlers::search_files,
        // Statistics
        crate::api::file_library::handlers::get_library_stats,
        crate::api::file_library::handlers::get_file_usage_stats,
        // Lineage & Impact Analysis
        crate::api::file_library::handlers::get_file_lineage,
        crate::api::file_library::handlers::get_impact_analysis,
        // Admin operations
        crate::api::file_library::handlers::recover_orphaned_files,
    ),
    components(
        schemas(
            // Core domain types
            crate::api::file_library::types::DataFile,
            crate::api::file_library::types::FileOwner,
            crate::api::file_library::types::FileSchema,
            crate::api::file_library::types::SchemaField,
            crate::api::file_library::types::FieldOntologyMapping,
            crate::api::file_library::types::FileStatus,
            crate::api::file_library::types::FieldType,
            crate::api::file_library::types::PiiType,
            crate::api::file_library::types::SensitivityLevel,
            crate::api::file_library::types::AccessControl,
            // Folder types
            crate::api::file_library::types::Folder,
            // Scan results
            crate::api::file_library::types::ScanResult,
            // Import jobs
            crate::api::file_library::types::ImportJob,
            crate::api::file_library::types::JobStatus,
            crate::api::file_library::types::ImportResult,
            crate::api::file_library::types::ImportFileStatus,
            // Request types
            crate::api::file_library::types::ListFilesRequest,
            crate::api::file_library::types::SortField,
            crate::api::file_library::types::SortOrder,
            crate::api::file_library::types::CreateFileRequest,
            crate::api::file_library::types::UpdateFileRequest,
            crate::api::file_library::types::ScanFileRequest,
            crate::api::file_library::types::BulkUploadRequest,
            crate::api::file_library::types::BulkUploadDefaults,
            crate::api::file_library::types::BulkImportDirectoryRequest,
            crate::api::file_library::types::DirectoryFilters,
            crate::api::file_library::types::BulkUpdateRequest,
            crate::api::file_library::types::BulkUpdates,
            crate::api::file_library::types::TagOperation,
            crate::api::file_library::types::TagAction,
            crate::api::file_library::types::BulkDeleteRequest,
            crate::api::file_library::types::BulkScanRequest,
            crate::api::file_library::types::CreateFolderRequest,
            crate::api::file_library::types::UpdateFolderRequest,
            crate::api::file_library::types::SearchRequest,
            crate::api::file_library::types::SearchFilters,
            crate::api::file_library::types::DateRange,
            crate::api::file_library::types::SearchSort,
            // Response types
            crate::api::file_library::types::ListFilesResponse,
            crate::api::file_library::types::CreateFileResponse,
            crate::api::file_library::types::FilePreviewResponse,
            crate::api::file_library::types::BulkUploadResponse,
            crate::api::file_library::types::BulkUpdateResponse,
            crate::api::file_library::types::BulkOperationError,
            crate::api::file_library::types::BulkDeleteResponse,
            crate::api::file_library::types::BulkDeleteError,
            crate::api::file_library::types::Dependencies,
            crate::api::file_library::types::BulkScanResponse,
            crate::api::file_library::types::FolderResponse,
            crate::api::file_library::types::ListFoldersResponse,
            crate::api::file_library::types::ListTagsResponse,
            crate::api::file_library::types::TagInfo,
            crate::api::file_library::types::SearchResponse,
            crate::api::file_library::types::SearchFacets,
            crate::api::file_library::types::LineageResponse,
            crate::api::file_library::types::LineageNode,
            crate::api::file_library::types::LineageNodeType,
            crate::api::file_library::types::ImpactAnalysisResponse,
            crate::api::file_library::types::ImpactDetails,
            crate::api::file_library::types::LibraryStatsResponse,
            crate::api::file_library::types::UsageStatsResponse,
            crate::api::file_library::types::UserUsage,
            crate::api::file_library::types::ValidateFileForRegistrationResponse,
            crate::api::file_library::types::InferredConfig,
        )
    ),
    tags(
        (name = "File Library", description = "Enterprise file management system for data files"),
    ),
    info(
        title = "ARCXA File Library API",
        version = "1.0.0",
        description = "REST API for managing data files with schema detection, PII governance, and lineage tracking",
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
pub struct FileLibraryApiDoc;
