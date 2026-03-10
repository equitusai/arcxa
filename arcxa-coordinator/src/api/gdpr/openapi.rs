//! OpenAPI documentation for GDPR API

use crate::gdpr::export::{
    DataCategory, DataSource, ExportErrorCode, ExportErrorInfo, ExportFormat, ExportPhase,
    ExportProgressInfo, ExportRequest, ExportRequestResponse, ExportStatus, ExportStatusResponse,
    TimeRange,
};
use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    paths(
        // Article 17: Right to Erasure - Tenant Level
        super::handlers::erase_tenant_data,
        super::handlers::count_tenant_data,
        super::handlers::verify_erasure,
        // Article 17: Right to Erasure - User Level (Enhanced)
        super::handlers::erase_user_data,
        super::handlers::count_user_data,
        super::handlers::check_legal_holds,
        // Article 20: Right to Data Portability
        super::export_handlers::request_export,
        super::export_handlers::get_export_status,
        super::export_handlers::list_user_exports,
        super::export_handlers::download_export,
        super::export_handlers::cancel_export,
    ),
    components(
        schemas(
            // Tenant erasure types
            super::types::EraseTenantDataRequest,
            super::types::EraseTenantDataResponse,
            super::types::BackendErasureDetail,
            super::types::TenantDataCountResponse,
            super::types::VerifyErasureResponse,
            // User erasure types
            super::types::EraseUserDataRequest,
            super::types::EraseUserDataResponse,
            super::types::UserDataCountResponse,
            super::types::CheckLegalHoldResponse,
            super::types::LegalHoldInfo,
            // Export types
            ExportRequest,
            ExportFormat,
            ExportRequestResponse,
            ExportStatusResponse,
            ExportStatus,
            ExportPhase,
            ExportProgressInfo,
            ExportErrorInfo,
            ExportErrorCode,
            DataCategory,
            TimeRange,
            DataSource,
        )
    ),
    tags(
        (name = "GDPR", description = "GDPR compliance operations (Articles 17 & 20)")
    )
)]
pub struct GdprApiDoc;
