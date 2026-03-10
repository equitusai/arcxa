//! Prometheus metrics exporter
//!
//! HTTP handler for exposing metrics in Prometheus text format.

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use std::sync::Arc;

use super::MetricsRegistry;

/// Metrics export handler
///
/// Returns metrics in Prometheus text exposition format.
/// Endpoint: GET /metrics
pub async fn metrics_handler(State(registry): State<Arc<MetricsRegistry>>) -> Response {
    match registry.gather() {
        Ok(buffer) => {
            // Return with correct content type for Prometheus
            (
                StatusCode::OK,
                [("Content-Type", "text/plain; version=0.0.4; charset=utf-8")],
                buffer,
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!("Failed to gather metrics: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to gather metrics: {}", e),
            )
                .into_response()
        }
    }
}
