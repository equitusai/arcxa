//! Retry strategy with circuit breaker integration
//!
//! Provides a clean, testable abstraction for retry logic with circuit breakers.
//! Addresses Issue #5 from critical review: complex nested retry logic.

use crate::core::lineage::LineageEvent;
use crate::reliability::{CircuitBreaker, CircuitBreakerError};
use anyhow::Result;
use std::time::Duration;

/// Retry configuration
#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub max_retries: u32,
    pub initial_backoff_ms: u64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_backoff_ms: 100,
        }
    }
}

/// Result of a retry attempt
#[derive(Debug)]
pub enum RetryOutcome {
    /// Operation succeeded
    Success { retries_used: u32 },

    /// Circuit breaker opened, sent to DLQ
    CircuitOpen,

    /// Permanent failure after exhausting retries
    PermanentFailure { error: anyhow::Error, retries: u32 },
}

/// Retry executor with circuit breaker integration
///
/// # Architecture
/// Clean separation of concerns:
/// - Circuit breaker handles fail-fast logic
/// - Retry strategy handles exponential backoff
/// - Caller handles DLQ writes based on outcome
///
/// # Benefits over nested match
/// - Testable in isolation
/// - Clear control flow
/// - Single responsibility
/// - Easy to extend
pub struct RetryExecutor {
    config: RetryConfig,
}

impl RetryExecutor {
    pub fn new(config: RetryConfig) -> Self {
        Self { config }
    }

    pub fn with_defaults() -> Self {
        Self::new(RetryConfig::default())
    }

    /// Execute operation with circuit breaker and retry logic
    ///
    /// # Arguments
    /// - `circuit_breaker`: Circuit breaker to protect operation
    /// - `operation`: Closure that performs the actual work
    ///
    /// # Returns
    /// RetryOutcome indicating what happened and what action caller should take
    ///
    /// # Example
    /// ```ignore
    /// let executor = RetryExecutor::with_defaults();
    /// match executor.execute(&storage_breaker, || storage.write(event.clone())) {
    ///     RetryOutcome::Success { .. } => { /* done */ },
    ///     RetryOutcome::CircuitOpen => { /* write to DLQ */ },
    ///     RetryOutcome::PermanentFailure { error, .. } => { /* write to DLQ with error */ },
    /// }
    /// ```
    pub fn execute<F, T, E>(
        &self,
        circuit_breaker: &CircuitBreaker,
        mut operation: F,
    ) -> RetryOutcome
    where
        F: FnMut() -> Result<T, E>,
        E: std::fmt::Display + Send + Sync + 'static,
    {
        // Initial attempt through circuit breaker
        match circuit_breaker.call(&mut operation) {
            Ok(_) => {
                return RetryOutcome::Success { retries_used: 0 };
            }
            Err(CircuitBreakerError::Open) => {
                // Circuit open - fail fast
                tracing::warn!("Circuit breaker open, failing fast");
                crate::ingestion::metrics::CIRCUIT_BREAKER_OPEN
                    .with_label_values(&[circuit_breaker.name()])
                    .inc();
                return RetryOutcome::CircuitOpen;
            }
            Err(CircuitBreakerError::CallFailed(initial_error)) => {
                // Fall through to retry logic
                tracing::debug!("Initial attempt failed, entering retry loop");

                // Retry with exponential backoff
                return self.retry_with_backoff(circuit_breaker, operation, initial_error);
            }
        }
    }

    /// Internal retry loop with exponential backoff
    fn retry_with_backoff<F, T, E>(
        &self,
        circuit_breaker: &CircuitBreaker,
        mut operation: F,
        mut last_error: E,
    ) -> RetryOutcome
    where
        F: FnMut() -> Result<T, E>,
        E: std::fmt::Display + Send + Sync + 'static,
    {
        for retry_attempt in 1..=self.config.max_retries {
            // Exponential backoff: 100ms, 200ms, 400ms
            let backoff_ms = self.config.initial_backoff_ms * 2u64.pow(retry_attempt - 1);

            tracing::warn!(
                "Retry attempt {}/{}, waiting {}ms (error: {})",
                retry_attempt,
                self.config.max_retries,
                backoff_ms,
                last_error
            );

            std::thread::sleep(Duration::from_millis(backoff_ms));

            crate::ingestion::metrics::STORAGE_RETRIES
                .with_label_values(&["retry"])
                .inc();

            // Retry through circuit breaker
            match circuit_breaker.call(&mut operation) {
                Ok(_) => {
                    tracing::info!("Operation succeeded after {} retries", retry_attempt);
                    crate::ingestion::metrics::STORAGE_RETRIES
                        .with_label_values(&["success"])
                        .inc();

                    return RetryOutcome::Success {
                        retries_used: retry_attempt,
                    };
                }
                Err(CircuitBreakerError::Open) => {
                    // Circuit opened during retry - fail fast
                    tracing::warn!(
                        "Circuit breaker opened during retry attempt {}",
                        retry_attempt
                    );
                    return RetryOutcome::CircuitOpen;
                }
                Err(CircuitBreakerError::CallFailed(e)) => {
                    last_error = e;
                    // Continue to next retry
                }
            }
        }

        // Exhausted all retries
        tracing::error!(
            "Operation permanently failed after {} attempts: {}",
            self.config.max_retries,
            last_error
        );

        RetryOutcome::PermanentFailure {
            error: anyhow::anyhow!("{}", last_error),
            retries: self.config.max_retries,
        }
    }
}

/// Helper to process retry outcome and write to DLQ if needed
///
/// # Arguments
/// - `outcome`: The retry outcome
/// - `event`: Event that failed
/// - `dlq`: Dead letter queue to write to
///
/// # Returns
/// true if operation succeeded, false if sent to DLQ
pub fn handle_retry_outcome<D>(outcome: RetryOutcome, event: LineageEvent, dlq: &D) -> bool
where
    D: DlqWriter,
{
    match outcome {
        RetryOutcome::Success { retries_used } => {
            if retries_used > 0 {
                tracing::debug!("Operation succeeded after {} retries", retries_used);
            }
            true
        }
        RetryOutcome::CircuitOpen => {
            tracing::warn!("Circuit breaker open, sending to DLQ");
            if let Err(e) = dlq.write(event, "circuit_breaker_open", 0) {
                tracing::error!("DLQ write failed during circuit open: {}", e);
            } else {
                crate::ingestion::metrics::DLQ_WRITES
                    .with_label_values(&["circuit_open"])
                    .inc();
            }
            false
        }
        RetryOutcome::PermanentFailure { error, retries } => {
            tracing::error!("Permanent failure, sending to DLQ: {}", error);
            if let Err(e) = dlq.write(event, &error.to_string(), retries) {
                tracing::error!("DLQ write failed: {}", e);
            } else {
                crate::ingestion::metrics::DLQ_WRITES
                    .with_label_values(&["storage_failure"])
                    .inc();
            }
            false
        }
    }
}

/// Trait for DLQ writers (allows testing with mocks)
pub trait DlqWriter {
    fn write(&self, event: LineageEvent, error: &str, retries: u32) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reliability::{CircuitBreaker, CircuitBreakerConfig};
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[test]
    fn test_retry_success_first_attempt() {
        let breaker = CircuitBreaker::new("test", CircuitBreakerConfig::default());
        let executor = RetryExecutor::with_defaults();

        let outcome = executor.execute(&breaker, || Ok::<(), anyhow::Error>(()));

        match outcome {
            RetryOutcome::Success { retries_used } => {
                assert_eq!(retries_used, 0);
            }
            _ => panic!("Expected success"),
        }
    }

    #[test]
    fn test_retry_success_after_failures() {
        let breaker = CircuitBreaker::new("test", CircuitBreakerConfig::default());
        let executor = RetryExecutor::with_defaults();

        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let outcome = executor.execute(&breaker, move || {
            let count = counter_clone.fetch_add(1, Ordering::SeqCst);
            if count < 2 {
                Err(anyhow::anyhow!("Simulated failure"))
            } else {
                Ok(())
            }
        });

        match outcome {
            RetryOutcome::Success { retries_used } => {
                assert_eq!(retries_used, 2, "Should succeed after 2 retries");
            }
            _ => panic!("Expected success after retries"),
        }
    }

    #[test]
    fn test_retry_circuit_open_immediate() {
        let breaker = CircuitBreaker::new(
            "test",
            CircuitBreakerConfig {
                failure_threshold: 2,
                timeout: Duration::from_secs(30),
                success_threshold: 2,
            },
        );
        let executor = RetryExecutor::with_defaults();

        // Fail twice to open circuit
        for _ in 0..2 {
            let _ = executor.execute(&breaker, || {
                Err::<(), anyhow::Error>(anyhow::anyhow!("fail"))
            });
        }

        // Next attempt should immediately return CircuitOpen
        let outcome = executor.execute(&breaker, || Ok::<(), anyhow::Error>(()));

        match outcome {
            RetryOutcome::CircuitOpen => { /* expected */ }
            _ => panic!("Expected circuit open"),
        }
    }

    #[test]
    fn test_retry_permanent_failure() {
        let breaker = CircuitBreaker::new("test", CircuitBreakerConfig::default());
        let executor = RetryExecutor::new(RetryConfig {
            max_retries: 3,
            initial_backoff_ms: 1, // Fast for testing
        });

        let outcome = executor.execute(&breaker, || {
            Err::<(), anyhow::Error>(anyhow::anyhow!("Persistent failure"))
        });

        match outcome {
            RetryOutcome::PermanentFailure { retries, .. } => {
                assert_eq!(retries, 3);
            }
            _ => panic!("Expected permanent failure"),
        }
    }

    #[test]
    fn test_retry_exponential_backoff_timing() {
        let breaker = CircuitBreaker::new("test", CircuitBreakerConfig::default());
        let executor = RetryExecutor::new(RetryConfig {
            max_retries: 3,
            initial_backoff_ms: 10,
        });

        let start = std::time::Instant::now();

        let _outcome = executor.execute(&breaker, || {
            Err::<(), anyhow::Error>(anyhow::anyhow!("fail"))
        });

        let elapsed = start.elapsed();

        // Should have waited: 10ms + 20ms + 40ms = 70ms minimum
        assert!(elapsed.as_millis() >= 70, "Should respect backoff timing");
    }
}
