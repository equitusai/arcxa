//! Common DTOs
//!
//! Shared request and response types used across multiple API endpoints.

use axum::{http::StatusCode, response::Json};
use chrono::{DateTime, Utc};
use serde::Serialize;
use utoipa::ToSchema;

// =============================================================================
// Health Check DTOs
// =============================================================================

#[derive(Serialize, ToSchema)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub timestamp: DateTime<Utc>,
    pub components: Option<std::collections::HashMap<String, ComponentHealth>>,
}

#[derive(Serialize, ToSchema)]
pub struct StorageHealthResponse {
    pub healthy: bool,
    pub rocksdb: bool,
    pub rdf_store: bool,
    pub timestamp: DateTime<Utc>,
}

#[derive(Serialize, ToSchema)]
pub struct ReadinessResponse {
    pub ready: bool,
    pub version: String,
    pub timestamp: DateTime<Utc>,
    pub components: std::collections::HashMap<String, ComponentHealth>,
}

#[derive(Serialize, ToSchema)]
pub struct ComponentHealth {
    pub status: String,
    pub message: Option<String>,
}

// =============================================================================
// API Error Type
// =============================================================================

/// API error response schema for OpenAPI documentation
#[derive(Serialize, ToSchema)]
pub struct ApiErrorResponse {
    /// Error message
    pub error: String,
    /// HTTP status code
    pub status: u16,
}

/// API error type with proper status codes
#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    pub fn internal(message: String) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message,
        }
    }

    pub fn bad_request(message: String) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message,
        }
    }

    pub fn not_found(message: String) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message,
        }
    }

    pub fn service_unavailable(message: String) -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            message,
        }
    }

    pub fn conflict(message: String) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            message,
        }
    }

    #[cfg(test)]
    pub fn message(&self) -> &str {
        &self.message
    }

    #[cfg(test)]
    pub fn status(&self) -> StatusCode {
        self.status
    }
}

impl axum::response::IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        (
            self.status,
            Json(serde_json::json!({
                "error": self.message,
                "status": self.status.as_u16()
            })),
        )
            .into_response()
    }
}
