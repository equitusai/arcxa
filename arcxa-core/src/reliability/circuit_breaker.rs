//! # Circuit Breaker Pattern
//!
//! Prevents cascading failures by failing fast when a service is unhealthy.
//!
//! States:
//! - Closed: Normal operation, all requests pass through
//! - Open: Service unhealthy, fail fast without trying
//! - HalfOpen: Test with single request to see if service recovered

use parking_lot::Mutex;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Circuit breaker state
#[derive(Debug, Clone)]
enum BreakerState {
    /// Normal operation - requests pass through
    Closed,

    /// Service unhealthy - fail fast
    Open { opened_at: Instant },

    /// Testing if service recovered
    HalfOpen,
}

/// Circuit breaker configuration
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Number of consecutive failures before opening circuit
    pub failure_threshold: u32,

    /// Time to wait before attempting recovery (half-open)
    pub timeout: Duration,

    /// Number of consecutive successes needed to close circuit from half-open
    pub success_threshold: u32,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            timeout: Duration::from_secs(30),
            success_threshold: 2,
        }
    }
}

/// Circuit breaker for preventing cascading failures
#[derive(Clone)]
pub struct CircuitBreaker {
    state: Arc<Mutex<BreakerState>>,
    consecutive_failures: Arc<Mutex<u32>>,
    consecutive_successes: Arc<Mutex<u32>>,
    config: CircuitBreakerConfig,
    name: String,
}

impl CircuitBreaker {
    pub fn new(name: impl Into<String>, config: CircuitBreakerConfig) -> Self {
        Self {
            state: Arc::new(Mutex::new(BreakerState::Closed)),
            consecutive_failures: Arc::new(Mutex::new(0)),
            consecutive_successes: Arc::new(Mutex::new(0)),
            config,
            name: name.into(),
        }
    }

    pub fn with_defaults(name: impl Into<String>) -> Self {
        Self::new(name, CircuitBreakerConfig::default())
    }

    /// Get the circuit breaker name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Execute a function through the circuit breaker
    pub fn call<F, T, E>(&self, f: F) -> Result<T, CircuitBreakerError<E>>
    where
        F: FnOnce() -> Result<T, E>,
    {
        // Check current state
        let current_state = self.state.lock().clone();

        match current_state {
            BreakerState::Open { opened_at } => {
                // Check if timeout elapsed
                if opened_at.elapsed() >= self.config.timeout {
                    tracing::info!(
                        "Circuit breaker '{}': Attempting recovery (half-open)",
                        self.name
                    );
                    *self.state.lock() = BreakerState::HalfOpen;
                    *self.consecutive_successes.lock() = 0;
                } else {
                    // Still open - fail fast
                    tracing::debug!("Circuit breaker '{}': Open, failing fast", self.name);
                    crate::ingestion::metrics::CIRCUIT_BREAKER_OPEN
                        .with_label_values(&[&self.name])
                        .inc();
                    return Err(CircuitBreakerError::Open);
                }
            }
            BreakerState::Closed | BreakerState::HalfOpen => {}
        }

        // Execute the function
        match f() {
            Ok(result) => {
                self.on_success();
                Ok(result)
            }
            Err(e) => {
                self.on_failure();
                Err(CircuitBreakerError::CallFailed(e))
            }
        }
    }

    /// Record a successful call
    ///
    /// Use this in async contexts where the callback-based `call()` method isn't suitable
    pub fn record_success(&self) {
        self.on_success();
    }

    /// Record a failed call
    ///
    /// Use this in async contexts where the callback-based `call()` method isn't suitable
    pub fn record_failure(&self) {
        self.on_failure();
    }

    /// Record a successful call (internal)
    fn on_success(&self) {
        let current_state = self.state.lock().clone();

        match current_state {
            BreakerState::Closed => {
                // Reset failure counter
                *self.consecutive_failures.lock() = 0;
            }
            BreakerState::HalfOpen => {
                // Count successes in half-open state
                let mut successes = self.consecutive_successes.lock();
                *successes += 1;

                if *successes >= self.config.success_threshold {
                    tracing::info!(
                        "Circuit breaker '{}': Closing (service recovered)",
                        self.name
                    );
                    *self.state.lock() = BreakerState::Closed;
                    *self.consecutive_failures.lock() = 0;
                    *successes = 0;

                    crate::ingestion::metrics::CIRCUIT_BREAKER_CLOSED
                        .with_label_values(&[&self.name])
                        .inc();
                }
            }
            BreakerState::Open { .. } => {
                // Should not happen, but reset if it does
                *self.state.lock() = BreakerState::Closed;
            }
        }
    }

    /// Record a failed call
    fn on_failure(&self) {
        let current_state = self.state.lock().clone();

        match current_state {
            BreakerState::Closed => {
                // Count consecutive failures
                let mut failures = self.consecutive_failures.lock();
                *failures += 1;

                if *failures >= self.config.failure_threshold {
                    tracing::warn!(
                        "Circuit breaker '{}': Opening after {} consecutive failures",
                        self.name,
                        *failures
                    );
                    *self.state.lock() = BreakerState::Open {
                        opened_at: Instant::now(),
                    };
                    *failures = 0;

                    crate::ingestion::metrics::CIRCUIT_BREAKER_OPENED
                        .with_label_values(&[&self.name])
                        .inc();
                }
            }
            BreakerState::HalfOpen => {
                // Single failure in half-open → re-open
                tracing::warn!(
                    "Circuit breaker '{}': Re-opening (recovery failed)",
                    self.name
                );
                *self.state.lock() = BreakerState::Open {
                    opened_at: Instant::now(),
                };
                *self.consecutive_successes.lock() = 0;

                crate::ingestion::metrics::CIRCUIT_BREAKER_OPENED
                    .with_label_values(&[&self.name])
                    .inc();
            }
            BreakerState::Open { .. } => {
                // Already open, nothing to do
            }
        }
    }

    /// Get current state (for monitoring)
    pub fn is_open(&self) -> bool {
        matches!(*self.state.lock(), BreakerState::Open { .. })
    }

    /// Get current state (for monitoring)
    pub fn is_half_open(&self) -> bool {
        matches!(*self.state.lock(), BreakerState::HalfOpen)
    }

    /// Get current state (for monitoring)
    pub fn is_closed(&self) -> bool {
        matches!(*self.state.lock(), BreakerState::Closed)
    }

    /// Get consecutive failure count
    pub fn consecutive_failures(&self) -> u32 {
        *self.consecutive_failures.lock()
    }

    /// Manually reset the circuit breaker
    pub fn reset(&self) {
        *self.state.lock() = BreakerState::Closed;
        *self.consecutive_failures.lock() = 0;
        *self.consecutive_successes.lock() = 0;
        tracing::info!("Circuit breaker '{}': Manually reset", self.name);
    }
}

/// Circuit breaker error
#[derive(Debug)]
pub enum CircuitBreakerError<E> {
    /// Circuit is open - failing fast
    Open,

    /// The actual call failed
    CallFailed(E),
}

impl<E: std::fmt::Display> std::fmt::Display for CircuitBreakerError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CircuitBreakerError::Open => write!(f, "Circuit breaker is open"),
            CircuitBreakerError::CallFailed(e) => write!(f, "Call failed: {}", e),
        }
    }
}

impl<E: std::error::Error> std::error::Error for CircuitBreakerError<E> {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_circuit_breaker_opens_after_threshold() {
        let breaker = CircuitBreaker::new(
            "test",
            CircuitBreakerConfig {
                failure_threshold: 3,
                timeout: Duration::from_secs(1),
                success_threshold: 2,
            },
        );

        assert!(breaker.is_closed());

        // First 2 failures - should stay closed
        for _ in 0..2 {
            let result = breaker.call(|| Err::<(), &str>("error"));
            assert!(matches!(result, Err(CircuitBreakerError::CallFailed(_))));
            assert!(breaker.is_closed());
        }

        // 3rd failure - should open
        let result = breaker.call(|| Err::<(), &str>("error"));
        assert!(matches!(result, Err(CircuitBreakerError::CallFailed(_))));
        assert!(breaker.is_open());

        // Next call should fail fast
        let result = breaker.call(|| Ok::<(), &str>(()));
        assert!(matches!(result, Err(CircuitBreakerError::Open)));
    }

    #[test]
    fn test_circuit_breaker_half_open_recovery() {
        let breaker = CircuitBreaker::new(
            "test",
            CircuitBreakerConfig {
                failure_threshold: 2,
                timeout: Duration::from_millis(100),
                success_threshold: 2,
            },
        );

        // Open the circuit
        for _ in 0..2 {
            let _ = breaker.call(|| Err::<(), &str>("error"));
        }
        assert!(breaker.is_open());

        // Wait for timeout
        std::thread::sleep(Duration::from_millis(150));

        // Next call should transition to half-open
        let result = breaker.call(|| Ok::<(), &str>(()));
        assert!(result.is_ok());
        assert!(breaker.is_half_open());

        // One more success should close the circuit
        let result = breaker.call(|| Ok::<(), &str>(()));
        assert!(result.is_ok());
        assert!(breaker.is_closed());
    }

    #[test]
    fn test_circuit_breaker_reopen_on_half_open_failure() {
        let breaker = CircuitBreaker::new(
            "test",
            CircuitBreakerConfig {
                failure_threshold: 2,
                timeout: Duration::from_millis(100),
                success_threshold: 2,
            },
        );

        // Open the circuit
        for _ in 0..2 {
            let _ = breaker.call(|| Err::<(), &str>("error"));
        }
        assert!(breaker.is_open());

        // Wait for timeout
        std::thread::sleep(Duration::from_millis(150));

        // Transition to half-open
        let result = breaker.call(|| Ok::<(), &str>(()));
        assert!(result.is_ok());
        assert!(breaker.is_half_open());

        // Failure in half-open → re-open
        let result = breaker.call(|| Err::<(), &str>("error"));
        assert!(matches!(result, Err(CircuitBreakerError::CallFailed(_))));
        assert!(breaker.is_open());
    }
}
