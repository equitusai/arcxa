//! # Reliability Module
//!
//! Circuit breakers, retry policies, and fault tolerance patterns.

pub mod async_retry;
pub mod circuit_breaker;
pub mod retry_strategy;

pub use async_retry::{retry_async, retry_async_with, RetryMetrics, RetryPolicy};
pub use circuit_breaker::{CircuitBreaker, CircuitBreakerConfig, CircuitBreakerError};
pub use retry_strategy::{
    handle_retry_outcome, DlqWriter, RetryConfig, RetryExecutor, RetryOutcome,
};
