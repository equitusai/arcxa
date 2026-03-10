//! Request ID middleware
//!
//! Generates unique request IDs for correlation across logs and traces.
//! Injects X-Request-ID header into requests and responses.

use axum::{
    extract::Request,
    http::{header, HeaderValue},
    middleware::Next,
    response::Response,
};
use uuid::Uuid;

pub const REQUEST_ID_HEADER: &str = "x-request-id";

/// Middleware function to inject request IDs
pub async fn request_id_middleware(mut request: Request, next: Next) -> Response {
    // Check if request already has an ID (from client or proxy)
    let request_id = request
        .headers()
        .get(REQUEST_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    // Insert into request extensions for downstream handlers
    request
        .extensions_mut()
        .insert(RequestId(request_id.clone()));

    // Execute request
    let mut response = next.run(request).await;

    // Add request ID to response headers
    if let Ok(header_value) = HeaderValue::from_str(&request_id) {
        response
            .headers_mut()
            .insert(REQUEST_ID_HEADER, header_value);
    }

    response
}

/// Deprecated - use request_id_middleware function directly
#[deprecated(note = "Use request_id_middleware function directly")]
pub struct RequestIdLayer;

/// Request ID extension type
///
/// Stored in request extensions for access by handlers and other middleware.
#[derive(Clone, Debug)]
pub struct RequestId(pub String);

impl RequestId {
    /// Get request ID string
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Extract request ID from request extensions
pub fn get_request_id(request: &Request) -> Option<String> {
    request
        .extensions()
        .get::<RequestId>()
        .map(|id| id.0.clone())
}
