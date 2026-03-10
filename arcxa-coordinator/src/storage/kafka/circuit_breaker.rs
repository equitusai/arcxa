//! Circuit breaker for Kafka failure detection and graceful degradation
//!
//! Implements the Circuit Breaker pattern to prevent cascading failures
//! when Kafka is unavailable or experiencing issues.
//!
//! # State Machine
//!
//! ```text
//!            CLOSED (Normal)
//!                 │
//!                 │ 5 consecutive failures
//!                 ▼
//!            OPEN (Failed)
//!                 │
//!                 │ 30s timeout
//!                 ▼
//!          HALF-OPEN (Testing)
//!                 │
//!        ┌────────┴────────┐
//!        │                 │
//!   3 successes      1 failure
//!        │                 │
//!        ▼                 ▼
//!     CLOSED            OPEN
//! ```

use anyhow::Result;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Circuit breaker state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// Circuit is closed, requests flow normally
    Closed,
    /// Circuit is open, requests are rejected (fail-fast)
    Open,
    /// Circuit is half-open, testing if service recovered
    HalfOpen,
}

/// Circuit breaker configuration
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Number of consecutive failures before opening circuit
    pub failure_threshold: u32,

    /// Number of consecutive successes to close circuit (from half-open)
    pub success_threshold: u32,

    /// Timeout before transitioning from OPEN to HALF-OPEN
    pub timeout: Duration,

    /// Maximum concurrent test requests in HALF-OPEN state
    pub half_open_max_requests: u32,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            success_threshold: 3,
            timeout: Duration::from_secs(30),
            half_open_max_requests: 3,
        }
    }
}

impl CircuitBreakerConfig {
    /// Production configuration
    pub fn production() -> Self {
        Self {
            failure_threshold: 5,
            success_threshold: 3,
            timeout: Duration::from_secs(30),
            half_open_max_requests: 3,
        }
    }

    /// Aggressive configuration (faster recovery)
    pub fn aggressive() -> Self {
        Self {
            failure_threshold: 3,
            success_threshold: 2,
            timeout: Duration::from_secs(10),
            half_open_max_requests: 5,
        }
    }

    /// Conservative configuration (slower to open/close)
    pub fn conservative() -> Self {
        Self {
            failure_threshold: 10,
            success_threshold: 5,
            timeout: Duration::from_secs(60),
            half_open_max_requests: 2,
        }
    }
}

/// Internal circuit breaker state
#[derive(Debug)]
struct CircuitBreakerState {
    /// Current state
    state: CircuitState,

    /// Consecutive failure count
    consecutive_failures: u32,

    /// Consecutive success count (in HALF-OPEN)
    consecutive_successes: u32,

    /// When circuit was last opened
    opened_at: Option<Instant>,

    /// Number of requests in flight during HALF-OPEN
    half_open_requests: u32,
}

/// Circuit breaker for protecting against cascading failures
pub struct CircuitBreaker {
    /// Configuration
    config: CircuitBreakerConfig,

    /// Internal state (protected by RwLock)
    state: Arc<RwLock<CircuitBreakerState>>,

    /// Metrics: total failures
    total_failures: Arc<AtomicU64>,

    /// Metrics: total successes
    total_successes: Arc<AtomicU64>,

    /// Metrics: state transitions
    state_transitions: Arc<AtomicU32>,
}

impl CircuitBreaker {
    /// Create new circuit breaker
    pub fn new(config: CircuitBreakerConfig) -> Self {
        info!(
            "Creating circuit breaker: failure_threshold={}, success_threshold={}, timeout={:?}",
            config.failure_threshold, config.success_threshold, config.timeout
        );

        Self {
            config,
            state: Arc::new(RwLock::new(CircuitBreakerState {
                state: CircuitState::Closed,
                consecutive_failures: 0,
                consecutive_successes: 0,
                opened_at: None,
                half_open_requests: 0,
            })),
            total_failures: Arc::new(AtomicU64::new(0)),
            total_successes: Arc::new(AtomicU64::new(0)),
            state_transitions: Arc::new(AtomicU32::new(0)),
        }
    }

    /// Execute function with circuit breaker protection
    pub async fn call<F, Fut, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        // Check if we should allow the request
        if !self.should_allow_request().await {
            return Err(anyhow::anyhow!(
                "Circuit breaker is OPEN - request rejected"
            ));
        }

        // Execute the function
        match f().await {
            Ok(result) => {
                self.record_success().await;
                Ok(result)
            }
            Err(e) => {
                self.record_failure().await;
                Err(e)
            }
        }
    }

    /// Check if request should be allowed
    async fn should_allow_request(&self) -> bool {
        let mut state = self.state.write().await;

        // Check for timeout-based state transition (OPEN → HALF-OPEN)
        if state.state == CircuitState::Open {
            if let Some(opened_at) = state.opened_at {
                if opened_at.elapsed() >= self.config.timeout {
                    info!(
                        "Circuit breaker timeout expired ({:?}), transitioning OPEN → HALF-OPEN",
                        self.config.timeout
                    );
                    state.state = CircuitState::HalfOpen;
                    state.consecutive_successes = 0;
                    state.half_open_requests = 0;
                    self.state_transitions.fetch_add(1, Ordering::Relaxed);
                }
            }
        }

        match state.state {
            CircuitState::Closed => true,
            CircuitState::Open => false,
            CircuitState::HalfOpen => {
                // Limit concurrent requests in HALF-OPEN state
                if state.half_open_requests < self.config.half_open_max_requests {
                    state.half_open_requests += 1;
                    true
                } else {
                    debug!("Circuit breaker HALF-OPEN: max concurrent requests reached");
                    false
                }
            }
        }
    }

    /// Record successful operation
    pub async fn record_success(&self) {
        let mut state = self.state.write().await;
        self.total_successes.fetch_add(1, Ordering::Relaxed);

        match state.state {
            CircuitState::Closed => {
                // Reset failure count on success
                if state.consecutive_failures > 0 {
                    debug!(
                        "Resetting failure count from {} to 0",
                        state.consecutive_failures
                    );
                    state.consecutive_failures = 0;
                }
            }
            CircuitState::HalfOpen => {
                state.consecutive_successes += 1;
                state.half_open_requests = state.half_open_requests.saturating_sub(1);

                debug!(
                    "HALF-OPEN success: {}/{}",
                    state.consecutive_successes, self.config.success_threshold
                );

                // Transition to CLOSED if enough successes
                if state.consecutive_successes >= self.config.success_threshold {
                    info!(
                        "Circuit breaker recovered: {} consecutive successes, transitioning HALF-OPEN → CLOSED",
                        state.consecutive_successes
                    );
                    state.state = CircuitState::Closed;
                    state.consecutive_failures = 0;
                    state.consecutive_successes = 0;
                    state.opened_at = None;
                    self.state_transitions.fetch_add(1, Ordering::Relaxed);
                }
            }
            CircuitState::Open => {
                // Should not receive success in OPEN state
                warn!("Received success in OPEN state - unexpected");
            }
        }
    }

    /// Record failed operation
    pub async fn record_failure(&self) {
        let mut state = self.state.write().await;
        self.total_failures.fetch_add(1, Ordering::Relaxed);

        match state.state {
            CircuitState::Closed => {
                state.consecutive_failures += 1;
                state.consecutive_successes = 0;

                debug!(
                    "Failure recorded: {}/{}",
                    state.consecutive_failures, self.config.failure_threshold
                );

                // Transition to OPEN if threshold exceeded
                if state.consecutive_failures >= self.config.failure_threshold {
                    warn!(
                        "Circuit breaker opening: {} consecutive failures (threshold: {})",
                        state.consecutive_failures, self.config.failure_threshold
                    );
                    state.state = CircuitState::Open;
                    state.opened_at = Some(Instant::now());
                    self.state_transitions.fetch_add(1, Ordering::Relaxed);
                }
            }
            CircuitState::HalfOpen => {
                state.half_open_requests = state.half_open_requests.saturating_sub(1);

                warn!("Failure in HALF-OPEN state, transitioning back to OPEN");

                // Any failure in HALF-OPEN → back to OPEN
                state.state = CircuitState::Open;
                state.consecutive_failures = self.config.failure_threshold; // Already at threshold
                state.consecutive_successes = 0;
                state.opened_at = Some(Instant::now());
                self.state_transitions.fetch_add(1, Ordering::Relaxed);
            }
            CircuitState::Open => {
                // Already open, just increment failure count
                state.consecutive_failures += 1;
            }
        }
    }

    /// Check if circuit is currently open
    pub async fn is_open(&self) -> bool {
        let state = self.state.read().await;
        state.state == CircuitState::Open
    }

    /// Get current state
    pub async fn state(&self) -> CircuitState {
        let state = self.state.read().await;
        state.state
    }

    /// Get metrics snapshot
    pub async fn metrics(&self) -> CircuitBreakerMetrics {
        let state = self.state.read().await;

        CircuitBreakerMetrics {
            state: state.state,
            consecutive_failures: state.consecutive_failures,
            consecutive_successes: state.consecutive_successes,
            total_failures: self.total_failures.load(Ordering::Relaxed),
            total_successes: self.total_successes.load(Ordering::Relaxed),
            state_transitions: self.state_transitions.load(Ordering::Relaxed),
            time_in_open_state: state.opened_at.map(|t| t.elapsed()),
        }
    }

    /// Force circuit to specific state (for testing)
    #[cfg(test)]
    pub async fn force_state(&self, new_state: CircuitState) {
        let mut state = self.state.write().await;
        state.state = new_state;
        if new_state == CircuitState::Open {
            state.opened_at = Some(Instant::now());
        }
    }
}

/// Circuit breaker metrics snapshot
#[derive(Debug, Clone)]
pub struct CircuitBreakerMetrics {
    pub state: CircuitState,
    pub consecutive_failures: u32,
    pub consecutive_successes: u32,
    pub total_failures: u64,
    pub total_successes: u64,
    pub state_transitions: u32,
    pub time_in_open_state: Option<Duration>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_circuit_starts_closed() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig::default());
        assert_eq!(cb.state().await, CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_circuit_opens_on_threshold() {
        let config = CircuitBreakerConfig {
            failure_threshold: 5,
            ..Default::default()
        };
        let cb = CircuitBreaker::new(config);

        // Record 4 failures - should stay closed
        for _ in 0..4 {
            cb.record_failure().await;
        }
        assert_eq!(cb.state().await, CircuitState::Closed);

        // 5th failure should open circuit
        cb.record_failure().await;
        assert_eq!(cb.state().await, CircuitState::Open);
    }

    #[tokio::test]
    async fn test_circuit_stays_closed_below_threshold() {
        let config = CircuitBreakerConfig {
            failure_threshold: 5,
            ..Default::default()
        };
        let cb = CircuitBreaker::new(config);

        // Record 4 failures
        for _ in 0..4 {
            cb.record_failure().await;
        }

        // Should still be closed
        assert_eq!(cb.state().await, CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_circuit_resets_on_success() {
        let config = CircuitBreakerConfig {
            failure_threshold: 5,
            ..Default::default()
        };
        let cb = CircuitBreaker::new(config);

        // Record 3 failures
        for _ in 0..3 {
            cb.record_failure().await;
        }

        // Record success - should reset
        cb.record_success().await;

        let metrics = cb.metrics().await;
        assert_eq!(metrics.consecutive_failures, 0);
    }

    #[tokio::test]
    async fn test_circuit_half_open_after_timeout() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            timeout: Duration::from_millis(100),
            ..Default::default()
        };
        let cb = CircuitBreaker::new(config);

        // Open the circuit
        cb.record_failure().await;
        cb.record_failure().await;
        assert_eq!(cb.state().await, CircuitState::Open);

        // Wait for timeout
        tokio::time::sleep(Duration::from_millis(150)).await;

        // Check state - should trigger transition to HALF-OPEN
        let allowed = cb.should_allow_request().await;
        assert!(allowed);
        assert_eq!(cb.state().await, CircuitState::HalfOpen);
    }

    #[tokio::test]
    async fn test_half_open_closes_on_successes() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            success_threshold: 3,
            timeout: Duration::from_millis(100),
            ..Default::default()
        };
        let cb = CircuitBreaker::new(config);

        // Open the circuit
        cb.force_state(CircuitState::Open).await;
        tokio::time::sleep(Duration::from_millis(150)).await;

        // Transition to HALF-OPEN
        cb.should_allow_request().await;
        assert_eq!(cb.state().await, CircuitState::HalfOpen);

        // Record 3 successes - should close
        for _ in 0..3 {
            cb.record_success().await;
        }

        assert_eq!(cb.state().await, CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_half_open_opens_on_failure() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            timeout: Duration::from_millis(100),
            ..Default::default()
        };
        let cb = CircuitBreaker::new(config);

        // Open the circuit
        cb.force_state(CircuitState::Open).await;
        tokio::time::sleep(Duration::from_millis(150)).await;

        // Transition to HALF-OPEN
        cb.should_allow_request().await;
        assert_eq!(cb.state().await, CircuitState::HalfOpen);

        // Single failure should reopen
        cb.record_failure().await;
        assert_eq!(cb.state().await, CircuitState::Open);
    }

    #[tokio::test]
    async fn test_circuit_limits_half_open_requests() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            timeout: Duration::from_millis(100),
            half_open_max_requests: 3,
            ..Default::default()
        };
        let cb = CircuitBreaker::new(config);

        // Open and transition to HALF-OPEN
        cb.force_state(CircuitState::Open).await;
        tokio::time::sleep(Duration::from_millis(150)).await;

        // First call transitions to HALF-OPEN and counts as request 1
        assert!(cb.should_allow_request().await);
        assert_eq!(cb.state().await, CircuitState::HalfOpen);

        // Requests 2 and 3 should be allowed
        assert!(cb.should_allow_request().await);
        assert!(cb.should_allow_request().await);

        // 4th request should be rejected (already at max of 3)
        assert!(!cb.should_allow_request().await);
    }

    #[tokio::test]
    async fn test_call_wrapper_success() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig::default());

        let result = cb.call(|| async { Ok::<i32, anyhow::Error>(42) }).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);

        let metrics = cb.metrics().await;
        assert_eq!(metrics.total_successes, 1);
    }

    #[tokio::test]
    async fn test_call_wrapper_failure() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig::default());

        let result = cb
            .call(|| async { Err::<i32, anyhow::Error>(anyhow::anyhow!("test error")) })
            .await;

        assert!(result.is_err());

        let metrics = cb.metrics().await;
        assert_eq!(metrics.total_failures, 1);
    }

    #[tokio::test]
    async fn test_call_rejected_when_open() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            ..Default::default()
        };
        let cb = CircuitBreaker::new(config);

        // Open the circuit
        cb.force_state(CircuitState::Open).await;

        // Call should be rejected
        let result = cb.call(|| async { Ok::<i32, anyhow::Error>(42) }).await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Circuit breaker is OPEN"));
    }

    #[tokio::test]
    async fn test_metrics_track_state_changes() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            success_threshold: 2,
            timeout: Duration::from_millis(100),
            ..Default::default()
        };
        let cb = CircuitBreaker::new(config);

        // CLOSED → OPEN
        cb.record_failure().await;
        cb.record_failure().await;

        let metrics1 = cb.metrics().await;
        assert_eq!(metrics1.state_transitions, 1);

        // Wait for timeout and transition to HALF-OPEN
        tokio::time::sleep(Duration::from_millis(150)).await;
        cb.should_allow_request().await;

        let metrics2 = cb.metrics().await;
        assert_eq!(metrics2.state_transitions, 2);

        // HALF-OPEN → CLOSED
        cb.record_success().await;
        cb.record_success().await;

        let metrics3 = cb.metrics().await;
        assert_eq!(metrics3.state_transitions, 3);
    }
}
