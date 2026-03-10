//! HTTP metrics middleware
//!
//! Collects HTTP request metrics:
//! - Request count
//! - Request duration
//! - In-flight requests
//! - Request body size

use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use std::sync::Arc;
use std::time::Instant;

use crate::observability::MetricsRegistry;

/// Middleware function to collect HTTP metrics
pub async fn metrics_middleware(
    State(registry): State<Option<Arc<MetricsRegistry>>>,
    request: Request,
    next: Next,
) -> Response {
    if let Some(ref reg) = registry {
        // Record request start
        reg.api.request_started();
        let start = Instant::now();

        // Extract request metadata
        let method = request.method().to_string();
        let path = request.uri().path().to_string();
        let endpoint = normalize_endpoint(&path);

        // Record request size if available
        if let Some(content_length) = request
            .headers()
            .get(axum::http::header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<usize>().ok())
        {
            reg.api
                .record_request_size(&method, &endpoint, content_length);
        }

        // Execute request
        let response = next.run(request).await;

        // Record request completion
        let duration = start.elapsed().as_secs_f64();
        let status = response.status().as_u16();

        reg.api.record_request(&method, &endpoint, status, duration);
        reg.api.request_finished();

        response
    } else {
        // Metrics not available, just pass through
        next.run(request).await
    }
}

/// Deprecated - use metrics_middleware function directly
#[deprecated(note = "Use metrics_middleware function directly")]
pub struct MetricsLayer;

/// Normalize endpoint path for metrics
///
/// Converts dynamic path segments to templates to avoid cardinality explosion.
/// Example: `/api/v1/users/123` -> `/api/v1/users/:id`
fn normalize_endpoint(path: &str) -> String {
    let segments: Vec<&str> = path.split('/').collect();
    let mut normalized = Vec::new();

    for (i, segment) in segments.iter().enumerate() {
        if segment.is_empty() {
            continue;
        }

        // Detect UUID patterns
        if is_uuid_like(segment) {
            normalized.push(":id");
        }
        // Detect numeric IDs
        else if segment.parse::<u64>().is_ok() && i > 2 {
            normalized.push(":id");
        }
        // Detect hash-like patterns (32+ hex chars)
        else if segment.len() >= 32 && segment.chars().all(|c| c.is_ascii_hexdigit()) {
            normalized.push(":hash");
        }
        // Keep literal segments
        else {
            normalized.push(segment);
        }
    }

    format!("/{}", normalized.join("/"))
}

/// Check if string looks like a UUID
fn is_uuid_like(s: &str) -> bool {
    s.len() == 36 && s.chars().filter(|&c| c == '-').count() == 4
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_endpoint() {
        assert_eq!(normalize_endpoint("/api/v1/users/123"), "/api/v1/users/:id");

        assert_eq!(
            normalize_endpoint("/api/v1/users/550e8400-e29b-41d4-a716-446655440000"),
            "/api/v1/users/:id"
        );

        assert_eq!(
            normalize_endpoint("/api/v1/auth/login"),
            "/api/v1/auth/login"
        );

        // Test hash detection (requires 32+ hex chars)
        assert_eq!(
            normalize_endpoint("/api/v1/entities/abc123def456789012345678901234567890/lineage"),
            "/api/v1/entities/:hash/lineage"
        );
    }
}
