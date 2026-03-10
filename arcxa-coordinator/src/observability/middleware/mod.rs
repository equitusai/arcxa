//! Middleware module
//!
//! HTTP middleware components for observability:
//! - `request_id`: Request ID generation and propagation
//! - `metrics_layer`: HTTP metrics collection

pub mod metrics_layer;
pub mod request_id;

pub use metrics_layer::metrics_middleware;
pub use request_id::{request_id_middleware, RequestId};

// Deprecated re-exports
#[allow(deprecated)]
pub use metrics_layer::MetricsLayer;
#[allow(deprecated)]
pub use request_id::RequestIdLayer;
