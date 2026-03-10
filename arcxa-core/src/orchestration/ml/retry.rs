//! Retry logic with exponential backoff and circuit breaker

use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::time::sleep;

/// Retry policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicy {
    /// Maximum number of retry attempts
    pub max_attempts: u32,
    /// Initial backoff duration
    pub initial_backoff_ms: u64,
    /// Maximum backoff duration
    pub max_backoff_ms: u64,
    /// Backoff multiplier
    pub backoff_multiplier: f64,
    /// Jitter factor (0.0 - 1.0)
    pub jitter_factor: f64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_backoff_ms: 100,
            max_backoff_ms: 5000,
            backoff_multiplier: 2.0,
            jitter_factor: 0.1,
        }
    }
}

impl RetryPolicy {
    /// Calculate backoff duration for attempt number
    pub fn backoff_duration(&self, attempt: u32) -> Duration {
        let base_backoff =
            self.initial_backoff_ms as f64 * self.backoff_multiplier.powi(attempt as i32);

        let backoff = base_backoff.min(self.max_backoff_ms as f64);

        // Add jitter to prevent thundering herd
        let jitter = backoff * self.jitter_factor * rand::random::<f64>();
        let final_backoff = backoff + jitter;

        Duration::from_millis(final_backoff as u64)
    }

    /// Execute function with retry logic
    pub async fn execute_with_retry<F, Fut, T, E>(&self, mut operation: F) -> Result<T, E>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<T, E>>,
        E: std::fmt::Display + IsRetryable,
    {
        let mut attempt = 0;

        loop {
            match operation().await {
                Ok(result) => return Ok(result),
                Err(err) => {
                    attempt += 1;

                    // Check if error is retryable
                    if !err.is_retryable() {
                        return Err(err);
                    }

                    // Check if we've exhausted retries
                    if attempt >= self.max_attempts {
                        return Err(err);
                    }

                    // Calculate backoff and sleep
                    let backoff = self.backoff_duration(attempt - 1);
                    tracing::debug!(
                        "Retry attempt {}/{} after error: {}. Backing off for {:?}",
                        attempt,
                        self.max_attempts,
                        err,
                        backoff
                    );
                    sleep(backoff).await;
                }
            }
        }
    }
}

/// Trait for determining if an error is retryable
pub trait IsRetryable {
    fn is_retryable(&self) -> bool;
}

/// Circuit breaker state
#[derive(Debug, Clone, PartialEq)]
pub enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

/// Circuit breaker configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    /// Failure threshold to open circuit
    pub failure_threshold: u32,
    /// Success threshold to close circuit from half-open
    pub success_threshold: u32,
    /// Timeout before moving from open to half-open
    pub timeout_ms: u64,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            success_threshold: 2,
            timeout_ms: 30_000, // 30 seconds
        }
    }
}

/// Circuit breaker for preventing cascading failures
pub struct CircuitBreaker {
    config: CircuitBreakerConfig,
    state: parking_lot::RwLock<CircuitState>,
    failure_count: parking_lot::RwLock<u32>,
    success_count: parking_lot::RwLock<u32>,
    last_failure_time: parking_lot::RwLock<Option<std::time::Instant>>,
}

impl CircuitBreaker {
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            config,
            state: parking_lot::RwLock::new(CircuitState::Closed),
            failure_count: parking_lot::RwLock::new(0),
            success_count: parking_lot::RwLock::new(0),
            last_failure_time: parking_lot::RwLock::new(None),
        }
    }

    /// Check if request should be allowed
    pub fn is_request_allowed(&self) -> bool {
        let state = self.state.read();

        match *state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // Check if timeout has elapsed
                if let Some(last_failure) = *self.last_failure_time.read() {
                    let timeout = Duration::from_millis(self.config.timeout_ms);
                    if last_failure.elapsed() >= timeout {
                        // Move to half-open
                        drop(state);
                        *self.state.write() = CircuitState::HalfOpen;
                        *self.success_count.write() = 0;
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            CircuitState::HalfOpen => true,
        }
    }

    /// Record successful request
    pub fn record_success(&self) {
        let state = self.state.read();

        match *state {
            CircuitState::Closed => {
                *self.failure_count.write() = 0;
            }
            CircuitState::HalfOpen => {
                let mut success_count = self.success_count.write();
                *success_count += 1;

                if *success_count >= self.config.success_threshold {
                    drop(state);
                    drop(success_count);
                    *self.state.write() = CircuitState::Closed;
                    *self.failure_count.write() = 0;
                    tracing::info!("Circuit breaker closed after successful recovery");
                }
            }
            CircuitState::Open => {}
        }
    }

    /// Record failed request
    pub fn record_failure(&self) {
        let state = self.state.read();

        match *state {
            CircuitState::Closed => {
                let mut failure_count = self.failure_count.write();
                *failure_count += 1;

                if *failure_count >= self.config.failure_threshold {
                    drop(state);
                    drop(failure_count);
                    *self.state.write() = CircuitState::Open;
                    *self.last_failure_time.write() = Some(std::time::Instant::now());
                    tracing::warn!(
                        "Circuit breaker opened after {} failures",
                        self.config.failure_threshold
                    );
                }
            }
            CircuitState::HalfOpen => {
                drop(state);
                *self.state.write() = CircuitState::Open;
                *self.last_failure_time.write() = Some(std::time::Instant::now());
                *self.success_count.write() = 0;
                tracing::warn!("Circuit breaker reopened after failure in half-open state");
            }
            CircuitState::Open => {}
        }
    }

    /// Get current circuit state
    pub fn state(&self) -> CircuitState {
        self.state.read().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backoff_calculation() {
        let policy = RetryPolicy {
            max_attempts: 3,
            initial_backoff_ms: 100,
            max_backoff_ms: 5000,
            backoff_multiplier: 2.0,
            jitter_factor: 0.0, // No jitter for deterministic test
        };

        let backoff1 = policy.backoff_duration(0);
        assert_eq!(backoff1.as_millis(), 100);

        let backoff2 = policy.backoff_duration(1);
        assert_eq!(backoff2.as_millis(), 200);

        let backoff3 = policy.backoff_duration(2);
        assert_eq!(backoff3.as_millis(), 400);
    }

    #[test]
    fn test_circuit_breaker_transitions() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            success_threshold: 2,
            timeout_ms: 100,
        };

        let breaker = CircuitBreaker::new(config);

        // Initially closed
        assert_eq!(breaker.state(), CircuitState::Closed);
        assert!(breaker.is_request_allowed());

        // Record failures to open circuit
        breaker.record_failure();
        breaker.record_failure();
        breaker.record_failure();

        assert_eq!(breaker.state(), CircuitState::Open);
        assert!(!breaker.is_request_allowed());
    }

    #[tokio::test]
    async fn test_circuit_breaker_recovery() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            success_threshold: 2,
            timeout_ms: 50,
        };

        let breaker = CircuitBreaker::new(config);

        // Open circuit
        breaker.record_failure();
        breaker.record_failure();
        assert_eq!(breaker.state(), CircuitState::Open);

        // Wait for timeout
        tokio::time::sleep(Duration::from_millis(60)).await;

        // Should move to half-open
        assert!(breaker.is_request_allowed());
        assert_eq!(breaker.state(), CircuitState::HalfOpen);

        // Record successes to close
        breaker.record_success();
        breaker.record_success();

        assert_eq!(breaker.state(), CircuitState::Closed);
    }
}
