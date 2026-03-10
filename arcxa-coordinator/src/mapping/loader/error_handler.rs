//! Error Handler with Retry Logic
//!
//! Provides intelligent error handling and retry strategies for ETL operations.
//!
//! ## Features
//!
//! - **Automatic Retry**: Exponential backoff for transient errors
//! - **Circuit Breaker**: Stop retrying after repeated failures
//! - **Error Classification**: Categorize errors as transient or fatal
//! - **Dead Letter Queue**: Route failed rows for later analysis
//! - **Recovery Actions**: Custom recovery strategies per error type
//!
//! ## Usage
//!
//! ```rust,ignore
//! use graphica_coordinator::mapping::loader::error_handler::{ErrorHandler, ErrorHandlerConfig};
//!
//! let config = ErrorHandlerConfig {
//!     max_retries: 3,
//!     circuit_breaker_threshold: 10,
//!     ..Default::default()
//! };
//!
//! let mut handler = ErrorHandler::new(config);
//!
//! // Execute with automatic retry
//! let result = handler.execute_with_retry(|| {
//!     // Your operation that might fail
//!     process_row(&row)
//! }).await?;
//! ```

use super::checkpoint::{CheckpointManager, ErrorCategory};
use anyhow::{anyhow, Context, Result};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

/// Error handler configuration
#[derive(Debug, Clone)]
pub struct ErrorHandlerConfig {
    /// Maximum retry attempts for transient errors
    pub max_retries: usize,

    /// Initial retry delay
    pub initial_retry_delay: Duration,

    /// Backoff multiplier for retries
    pub retry_backoff_multiplier: f64,

    /// Maximum retry delay
    pub max_retry_delay: Duration,

    /// Circuit breaker failure threshold
    pub circuit_breaker_threshold: usize,

    /// Circuit breaker reset timeout
    pub circuit_breaker_reset_timeout: Duration,

    /// Whether to enable circuit breaker
    pub enable_circuit_breaker: bool,
}

impl Default for ErrorHandlerConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_retry_delay: Duration::from_millis(100),
            retry_backoff_multiplier: 2.0,
            max_retry_delay: Duration::from_secs(30),
            circuit_breaker_threshold: 10,
            circuit_breaker_reset_timeout: Duration::from_secs(60),
            enable_circuit_breaker: true,
        }
    }
}

/// Circuit breaker state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitBreakerState {
    /// Circuit is closed (normal operation)
    Closed,

    /// Circuit is open (failing fast)
    Open,

    /// Circuit is half-open (testing recovery)
    HalfOpen,
}

/// Circuit breaker
#[derive(Debug)]
pub struct CircuitBreaker {
    /// Current state
    state: CircuitBreakerState,

    /// Consecutive failure count
    failure_count: Arc<AtomicUsize>,

    /// Configuration
    config: ErrorHandlerConfig,

    /// Last failure time
    last_failure_time: Option<std::time::Instant>,
}

impl CircuitBreaker {
    /// Create new circuit breaker
    pub fn new(config: ErrorHandlerConfig) -> Self {
        Self {
            state: CircuitBreakerState::Closed,
            failure_count: Arc::new(AtomicUsize::new(0)),
            config,
            last_failure_time: None,
        }
    }

    /// Record success
    pub fn record_success(&mut self) {
        self.failure_count.store(0, Ordering::SeqCst);
        self.state = CircuitBreakerState::Closed;
    }

    /// Record failure
    pub fn record_failure(&mut self) {
        let count = self.failure_count.fetch_add(1, Ordering::SeqCst) + 1;
        self.last_failure_time = Some(std::time::Instant::now());

        if count >= self.config.circuit_breaker_threshold {
            self.state = CircuitBreakerState::Open;
        }
    }

    /// Check if circuit should allow request
    pub fn allow_request(&mut self) -> Result<()> {
        if !self.config.enable_circuit_breaker {
            return Ok(());
        }

        match self.state {
            CircuitBreakerState::Closed => Ok(()),
            CircuitBreakerState::HalfOpen => Ok(()),
            CircuitBreakerState::Open => {
                // Check if timeout has elapsed
                if let Some(last_failure) = self.last_failure_time {
                    if last_failure.elapsed() >= self.config.circuit_breaker_reset_timeout {
                        self.state = CircuitBreakerState::HalfOpen;
                        Ok(())
                    } else {
                        Err(anyhow!(
                            "Circuit breaker is OPEN - too many failures ({})",
                            self.failure_count.load(Ordering::SeqCst)
                        ))
                    }
                } else {
                    Err(anyhow!("Circuit breaker is OPEN"))
                }
            }
        }
    }

    /// Get current state
    pub fn state(&self) -> CircuitBreakerState {
        self.state
    }

    /// Get failure count
    pub fn failure_count(&self) -> usize {
        self.failure_count.load(Ordering::SeqCst)
    }
}

/// Error handler with retry logic
pub struct ErrorHandler {
    /// Configuration
    config: ErrorHandlerConfig,

    /// Circuit breaker
    circuit_breaker: CircuitBreaker,
}

impl ErrorHandler {
    /// Create new error handler
    pub fn new(config: ErrorHandlerConfig) -> Self {
        let circuit_breaker = CircuitBreaker::new(config.clone());
        Self {
            config,
            circuit_breaker,
        }
    }

    /// Execute operation with automatic retry
    pub async fn execute_with_retry<F, T>(&mut self, mut operation: F) -> Result<T>
    where
        F: FnMut() -> Result<T>,
    {
        // Check circuit breaker
        self.circuit_breaker.allow_request()?;

        let mut retry_count = 0;
        let mut last_error: Option<anyhow::Error> = None;

        loop {
            match operation() {
                Ok(result) => {
                    self.circuit_breaker.record_success();
                    return Ok(result);
                }
                Err(error) => {
                    let category = ErrorCategory::from_error_message(&error.to_string());

                    // Check if error is transient and we should retry
                    if category.is_transient() && retry_count < self.config.max_retries {
                        let delay = self.calculate_retry_delay(retry_count);

                        tracing::warn!(
                            "Transient error ({}), retry {}/{} after {:?}: {}",
                            category,
                            retry_count + 1,
                            self.config.max_retries,
                            delay,
                            error
                        );

                        sleep(delay).await;
                        retry_count += 1;
                        last_error = Some(error);
                        continue;
                    } else {
                        // Fatal error or max retries exceeded
                        self.circuit_breaker.record_failure();

                        if retry_count > 0 {
                            return Err(anyhow!(
                                "Operation failed after {} retries: {}",
                                retry_count,
                                error
                            ));
                        } else {
                            return Err(error);
                        }
                    }
                }
            }
        }
    }

    /// Execute operation with retry (sync version)
    pub fn execute_with_retry_sync<F, T>(&mut self, mut operation: F) -> Result<T>
    where
        F: FnMut() -> Result<T>,
    {
        // Check circuit breaker
        self.circuit_breaker.allow_request()?;

        let mut retry_count = 0;

        loop {
            match operation() {
                Ok(result) => {
                    self.circuit_breaker.record_success();
                    return Ok(result);
                }
                Err(error) => {
                    let category = ErrorCategory::from_error_message(&error.to_string());

                    // Check if error is transient and we should retry
                    if category.is_transient() && retry_count < self.config.max_retries {
                        let delay = self.calculate_retry_delay(retry_count);

                        tracing::warn!(
                            "Transient error ({}), retry {}/{} after {:?}: {}",
                            category,
                            retry_count + 1,
                            self.config.max_retries,
                            delay,
                            error
                        );

                        std::thread::sleep(delay);
                        retry_count += 1;
                        continue;
                    } else {
                        // Fatal error or max retries exceeded
                        self.circuit_breaker.record_failure();

                        if retry_count > 0 {
                            return Err(anyhow!(
                                "Operation failed after {} retries: {}",
                                retry_count,
                                error
                            ));
                        } else {
                            return Err(error);
                        }
                    }
                }
            }
        }
    }

    /// Calculate retry delay with exponential backoff
    fn calculate_retry_delay(&self, retry_count: usize) -> Duration {
        let multiplier = self
            .config
            .retry_backoff_multiplier
            .powi(retry_count as i32);
        let delay_ms = (self.config.initial_retry_delay.as_millis() as f64 * multiplier) as u64;
        let delay = Duration::from_millis(delay_ms);

        // Cap at max retry delay
        if delay > self.config.max_retry_delay {
            self.config.max_retry_delay
        } else {
            delay
        }
    }

    /// Get circuit breaker state
    pub fn circuit_breaker_state(&self) -> CircuitBreakerState {
        self.circuit_breaker.state()
    }

    /// Get circuit breaker failure count
    pub fn circuit_breaker_failures(&self) -> usize {
        self.circuit_breaker.failure_count()
    }

    /// Reset circuit breaker
    pub fn reset_circuit_breaker(&mut self) {
        self.circuit_breaker.record_success();
    }
}

/// Batch error handler with checkpoint integration
pub struct BatchErrorHandler {
    /// Error handler
    error_handler: ErrorHandler,

    /// Checkpoint manager
    checkpoint_manager: CheckpointManager,
}

impl BatchErrorHandler {
    /// Create new batch error handler
    pub fn new(error_handler: ErrorHandler, checkpoint_manager: CheckpointManager) -> Self {
        Self {
            error_handler,
            checkpoint_manager,
        }
    }

    /// Process batch with error handling and checkpointing
    pub async fn process_batch<F, T>(
        &mut self,
        batch_start: u64,
        batch_size: u64,
        mut process_fn: F,
    ) -> Result<Vec<T>>
    where
        F: FnMut(u64) -> Result<T>,
    {
        self.checkpoint_manager.start_batch(batch_start, batch_size);
        let mut results = Vec::new();
        let mut errors = 0;

        for row_num in batch_start..(batch_start + batch_size) {
            match self
                .error_handler
                .execute_with_retry_sync(|| process_fn(row_num))
            {
                Ok(result) => {
                    self.checkpoint_manager.record_success(row_num)?;
                    results.push(result);
                }
                Err(error) => {
                    self.checkpoint_manager.record_error(row_num, error)?;
                    errors += 1;
                }
            }

            // Checkpoint periodically
            if self.checkpoint_manager.should_checkpoint() {
                self.checkpoint_manager.checkpoint()?;
            }
        }

        if errors > 0 {
            self.checkpoint_manager.fail_batch()?;
        } else {
            self.checkpoint_manager.complete_batch(batch_size)?;
        }

        Ok(results)
    }

    /// Get checkpoint manager
    pub fn checkpoint_manager(&self) -> &CheckpointManager {
        &self.checkpoint_manager
    }

    /// Get mutable checkpoint manager
    pub fn checkpoint_manager_mut(&mut self) -> &mut CheckpointManager {
        &mut self.checkpoint_manager
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[test]
    fn test_circuit_breaker_closed() {
        let config = ErrorHandlerConfig::default();
        let mut cb = CircuitBreaker::new(config);

        assert_eq!(cb.state(), CircuitBreakerState::Closed);
        assert!(cb.allow_request().is_ok());
    }

    #[test]
    fn test_circuit_breaker_opens_after_failures() {
        let mut config = ErrorHandlerConfig::default();
        config.circuit_breaker_threshold = 3;

        let mut cb = CircuitBreaker::new(config);

        // Record failures
        cb.record_failure();
        assert_eq!(cb.state(), CircuitBreakerState::Closed);

        cb.record_failure();
        assert_eq!(cb.state(), CircuitBreakerState::Closed);

        cb.record_failure();
        assert_eq!(cb.state(), CircuitBreakerState::Open);

        // Should block requests
        assert!(cb.allow_request().is_err());
    }

    #[test]
    fn test_circuit_breaker_success_resets() {
        let mut config = ErrorHandlerConfig::default();
        config.circuit_breaker_threshold = 3;

        let mut cb = CircuitBreaker::new(config);

        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.failure_count(), 2);

        cb.record_success();
        assert_eq!(cb.failure_count(), 0);
        assert_eq!(cb.state(), CircuitBreakerState::Closed);
    }

    #[tokio::test]
    async fn test_retry_transient_error() {
        let config = ErrorHandlerConfig {
            max_retries: 3,
            initial_retry_delay: Duration::from_millis(1),
            ..Default::default()
        };

        let mut handler = ErrorHandler::new(config);
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_clone = attempts.clone();

        let result = handler
            .execute_with_retry(|| {
                let count = attempts_clone.fetch_add(1, Ordering::SeqCst);
                if count < 2 {
                    // Fail with transient error first 2 times
                    Err(anyhow!("Connection timeout"))
                } else {
                    Ok(42)
                }
            })
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
        assert_eq!(attempts.load(Ordering::SeqCst), 3); // 2 failures + 1 success
    }

    #[tokio::test]
    async fn test_no_retry_fatal_error() {
        let config = ErrorHandlerConfig::default();
        let mut handler = ErrorHandler::new(config);
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_clone = attempts.clone();

        let result: Result<i32> = handler
            .execute_with_retry(|| {
                attempts_clone.fetch_add(1, Ordering::SeqCst);
                // Fatal error (not transient)
                Err(anyhow!("Constraint violation"))
            })
            .await;

        assert!(result.is_err());
        assert_eq!(attempts.load(Ordering::SeqCst), 1); // Only 1 attempt
    }

    #[tokio::test]
    async fn test_max_retries_exceeded() {
        let config = ErrorHandlerConfig {
            max_retries: 2,
            initial_retry_delay: Duration::from_millis(1),
            ..Default::default()
        };

        let mut handler = ErrorHandler::new(config);
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_clone = attempts.clone();

        let result: Result<i32> = handler
            .execute_with_retry(|| {
                attempts_clone.fetch_add(1, Ordering::SeqCst);
                // Always fail with transient error
                Err(anyhow!("Timeout"))
            })
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("after 2 retries"));
        assert_eq!(attempts.load(Ordering::SeqCst), 3); // 1 initial + 2 retries
    }

    #[test]
    fn test_retry_delay_calculation() {
        let config = ErrorHandlerConfig {
            initial_retry_delay: Duration::from_millis(100),
            retry_backoff_multiplier: 2.0,
            max_retry_delay: Duration::from_secs(10),
            ..Default::default()
        };

        let handler = ErrorHandler::new(config);

        assert_eq!(handler.calculate_retry_delay(0).as_millis(), 100); // 100 * 2^0
        assert_eq!(handler.calculate_retry_delay(1).as_millis(), 200); // 100 * 2^1
        assert_eq!(handler.calculate_retry_delay(2).as_millis(), 400); // 100 * 2^2
        assert_eq!(handler.calculate_retry_delay(3).as_millis(), 800); // 100 * 2^3
    }

    #[test]
    fn test_retry_delay_max_cap() {
        let config = ErrorHandlerConfig {
            initial_retry_delay: Duration::from_millis(100),
            retry_backoff_multiplier: 2.0,
            max_retry_delay: Duration::from_millis(500),
            ..Default::default()
        };

        let handler = ErrorHandler::new(config);

        // Should cap at 500ms
        assert_eq!(handler.calculate_retry_delay(10).as_millis(), 500);
    }

    #[test]
    fn test_sync_retry() {
        let config = ErrorHandlerConfig {
            max_retries: 3,
            initial_retry_delay: Duration::from_millis(1),
            ..Default::default()
        };

        let mut handler = ErrorHandler::new(config);
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_clone = attempts.clone();

        let result = handler.execute_with_retry_sync(|| {
            let count = attempts_clone.fetch_add(1, Ordering::SeqCst);
            if count < 2 {
                Err(anyhow!("Connection failed"))
            } else {
                Ok(42)
            }
        });

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }
}
