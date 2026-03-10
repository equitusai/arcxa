//! ML Model Integration
//!
//! Provides HTTP/gRPC clients for invoking external ML models with connection pooling,
//! caching, and circuit breaker patterns.

pub mod cache;
pub mod health;
pub mod invoker;
pub mod metrics;
pub mod registry;
pub mod retry;

pub use cache::{CacheConfig, ModelCache};
pub use health::{HealthConfig, HealthMetrics, HealthMonitor, HealthStatus};
pub use invoker::{InvocationError, ModelInvoker, ModelRequest, ModelResponse};
pub use metrics::{MlMetrics, ML_METRICS};
pub use registry::{ModelEndpoint, ModelMetadata, ModelRegistry};
pub use retry::{CircuitBreaker, CircuitBreakerConfig, CircuitState, RetryPolicy};

use serde::{Deserialize, Serialize};

/// ML model invocation protocol
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ModelProtocol {
    Http,
    Grpc,
    Lambda,
}

/// Model serving framework
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ServingFramework {
    TensorFlowServing,
    TorchServe,
    SageMaker,
    Custom,
}
