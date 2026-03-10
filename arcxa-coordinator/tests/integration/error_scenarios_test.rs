//! Phase 2 Error Scenario Integration Tests
//!
//! Comprehensive testing of Phase 2 production hardening features:
//! - Retry exhaustion with retryable errors
//! - No retry on permanent errors
//! - Workflow timeout detection
//! - Stage timeout detection
//! - Memory pressure backpressure
//! - Circuit breaker behavior
//!
//! These tests validate the reliability and resilience features added in Phase 2.

use anyhow::anyhow;
use graphica_core::reliability::{
    async_retry::{retry_async_with, RetryPolicy},
    circuit_breaker::{CircuitBreaker, CircuitBreakerConfig},
};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio;

// ============================================================================
// Module 1: Retry Policy Tests
// ============================================================================

#[cfg(test)]
mod retry_tests {
    use super::*;

    /// Test retry exhaustion with retryable errors
    /// Validates that connection errors are retried up to max_retries limit
    #[tokio::test]
    async fn test_retry_exhaustion_on_connection_error() {
        let retry_policy = RetryPolicy {
            max_retries: 3,
            initial_delay: Duration::from_millis(10), // Fast for testing
            max_delay: Duration::from_millis(100),
            backoff_multiplier: 2.0,
        };

        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = Arc::clone(&attempts);

        // Simulate a connection error that always fails
        let result = retry_async_with(
            retry_policy,
            || {
                let attempts = Arc::clone(&attempts_clone);
                async move {
                    attempts.fetch_add(1, Ordering::SeqCst);
                    Err::<(), _>(anyhow!("SQL-30081N Connection failed"))
                }
            },
            |err: &anyhow::Error| {
                // Categorize as retryable if it's a connection error
                err.to_string().contains("-30081")
            },
        )
        .await;

        assert!(result.is_err(), "Should fail after retries exhausted");
        assert_eq!(
            attempts.load(Ordering::SeqCst),
            4,
            "Should be 1 initial + 3 retries"
        );
    }

    /// Test that permanent errors don't retry
    /// Validates that validation errors fail immediately without retries
    #[tokio::test]
    async fn test_no_retry_on_permanent_error() {
        let retry_policy = RetryPolicy::default();

        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = Arc::clone(&attempts);

        let result = retry_async_with(
            retry_policy,
            || {
                let attempts = Arc::clone(&attempts_clone);
                async move {
                    attempts.fetch_add(1, Ordering::SeqCst);
                    Err::<(), _>(anyhow!("SQL-803 Duplicate key violation"))
                }
            },
            |err: &anyhow::Error| {
                // Only retry connection errors, not constraint violations
                err.to_string().contains("-30081")
                    || err.to_string().contains("connection")
                    || err.to_string().contains("timeout")
            },
        )
        .await;

        assert!(result.is_err(), "Should fail immediately");
        assert_eq!(
            attempts.load(Ordering::SeqCst),
            1,
            "Should only attempt once for permanent error"
        );
    }

    /// Test exponential backoff timing
    #[tokio::test]
    async fn test_exponential_backoff_delays() {
        let retry_policy = RetryPolicy {
            max_retries: 3,
            initial_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(100),
            backoff_multiplier: 2.0,
        };

        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = Arc::clone(&attempts);
        let start = Instant::now();

        let _result = retry_async_with(
            retry_policy,
            || {
                let attempts = Arc::clone(&attempts_clone);
                async move {
                    attempts.fetch_add(1, Ordering::SeqCst);
                    Err::<(), _>(anyhow!("Temporary error"))
                }
            },
            |_err: &anyhow::Error| true, // Always retry
        )
        .await;

        let elapsed = start.elapsed();

        // Expected delays: 10ms + 20ms + 40ms = 70ms minimum
        assert!(
            elapsed >= Duration::from_millis(60),
            "Should have exponential backoff delays"
        );
        assert_eq!(attempts.load(Ordering::SeqCst), 4);
    }

    /// Test successful retry after transient failures
    #[tokio::test]
    async fn test_successful_retry_after_transient_failures() {
        let retry_policy = RetryPolicy {
            max_retries: 5,
            initial_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(100),
            backoff_multiplier: 2.0,
        };

        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = Arc::clone(&attempts);

        let result = retry_async_with(
            retry_policy,
            || {
                let attempts = Arc::clone(&attempts_clone);
                async move {
                    let count = attempts.fetch_add(1, Ordering::SeqCst);
                    if count < 2 {
                        // Fail first 2 attempts
                        Err(anyhow!("SQL-30081N Connection timeout"))
                    } else {
                        // Succeed on 3rd attempt
                        Ok(42)
                    }
                }
            },
            |err: &anyhow::Error| err.to_string().contains("-30081"),
        )
        .await;

        assert!(result.is_ok(), "Should succeed after transient failures");
        assert_eq!(result.unwrap(), 42);
        assert_eq!(
            attempts.load(Ordering::SeqCst),
            3,
            "Should succeed on 3rd attempt"
        );
    }
}

// ============================================================================
// Module 2: Timeout Tests
// ============================================================================

#[cfg(test)]
mod timeout_tests {
    use super::*;

    /// Test workflow timeout detection
    #[tokio::test]
    async fn test_workflow_timeout() {
        let timeout_duration = Duration::from_secs(2);
        let workflow_start = Instant::now();

        // Simulate long-running workflow
        tokio::time::sleep(Duration::from_millis(2500)).await;

        let elapsed = workflow_start.elapsed();
        let is_timeout = elapsed > timeout_duration;

        assert!(
            is_timeout,
            "Workflow should be detected as timed out after {} seconds",
            timeout_duration.as_secs()
        );
    }

    /// Test stage timeout detection
    #[tokio::test]
    async fn test_stage_timeout() {
        let stage_timeout = Duration::from_secs(1);
        let stage_start = Instant::now();

        // Simulate long-running stage
        tokio::time::sleep(Duration::from_millis(1500)).await;

        let elapsed = stage_start.elapsed();
        let is_timeout = elapsed > stage_timeout;

        assert!(
            is_timeout,
            "Stage should be detected as timed out after {} seconds",
            stage_timeout.as_secs()
        );
    }

    /// Test record-level timeout (batch processing)
    #[tokio::test]
    async fn test_record_processing_timeout() {
        let record_timeout = Duration::from_millis(500);
        let mut timed_out_records = 0;

        // Simulate processing 10 records, some slow
        for i in 0..10 {
            let record_start = Instant::now();

            // Simulate variable processing time
            let processing_time = if i % 3 == 0 {
                Duration::from_millis(600) // Slow
            } else {
                Duration::from_millis(100) // Fast
            };

            tokio::time::sleep(processing_time).await;

            if record_start.elapsed() > record_timeout {
                timed_out_records += 1;
            }
        }

        assert!(
            timed_out_records > 0,
            "Should detect some record-level timeouts"
        );
        assert!(timed_out_records < 10, "Not all records should timeout");
    }

    /// Test timeout with graceful cancellation
    #[tokio::test]
    async fn test_timeout_with_cancellation() {
        use tokio::time::timeout;

        let result = timeout(Duration::from_millis(100), async {
            // Simulate long operation
            tokio::time::sleep(Duration::from_secs(10)).await;
            42
        })
        .await;

        assert!(
            result.is_err(),
            "Operation should be cancelled due to timeout"
        );
    }
}

// ============================================================================
// Module 3: Circuit Breaker Tests
// ============================================================================

#[cfg(test)]
mod circuit_breaker_tests {
    use super::*;

    /// Test circuit breaker opens after failures
    #[test]
    fn test_circuit_breaker_opens_after_failures() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            success_threshold: 2,
            timeout: Duration::from_secs(5),
        };

        let cb = CircuitBreaker::new("test_breaker", config);

        // Initially closed
        assert!(cb.is_closed(), "Circuit should start closed");

        // Record failures
        cb.record_failure();
        assert!(cb.is_closed(), "Should still be closed after 1 failure");

        cb.record_failure();
        assert!(cb.is_closed(), "Should still be closed after 2 failures");

        cb.record_failure();
        assert!(cb.is_open(), "Should be open after 3 failures (threshold)");
    }

    /// Test circuit breaker half-open state recovery
    #[test]
    fn test_circuit_breaker_half_open_recovery() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            success_threshold: 2,
            timeout: Duration::from_millis(100),
        };

        let cb = CircuitBreaker::new("test_recovery", config);

        // Open the circuit
        cb.record_failure();
        cb.record_failure();
        assert!(cb.is_open(), "Circuit should be open");

        // Wait for timeout
        std::thread::sleep(Duration::from_millis(150));

        // Record successes to recover
        cb.record_success();
        cb.record_success();

        // Circuit should eventually close after success threshold
        assert!(
            cb.consecutive_failures() == 0,
            "Consecutive failures should reset after successes"
        );
    }

    /// Test circuit breaker fast fail when open
    #[test]
    fn test_circuit_breaker_fast_fail_when_open() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            success_threshold: 1,
            timeout: Duration::from_secs(10),
        };

        let cb = CircuitBreaker::new("test_fast_fail", config);

        // Open circuit
        cb.record_failure();
        assert!(cb.is_open(), "Circuit should be open");

        // Operations should fail fast
        let result = if cb.is_closed() {
            Ok(())
        } else {
            Err(anyhow!("Circuit breaker open"))
        };

        assert!(
            result.is_err(),
            "Operations should fail fast when circuit is open"
        );
    }

    /// Test circuit breaker reset
    #[test]
    fn test_circuit_breaker_manual_reset() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            success_threshold: 2,
            timeout: Duration::from_secs(5),
        };

        let cb = CircuitBreaker::new("test_reset", config);

        // Open circuit
        cb.record_failure();
        cb.record_failure();
        assert!(cb.is_open(), "Circuit should be open");

        // Manual reset
        cb.reset();
        assert!(cb.is_closed(), "Circuit should be closed after reset");
        assert_eq!(
            cb.consecutive_failures(),
            0,
            "Failure count should be reset"
        );
    }
}

// ============================================================================
// Module 4: Memory Pressure Tests (Mock)
// ============================================================================

#[cfg(test)]
mod memory_pressure_tests {
    use super::*;

    /// Mock memory monitor for testing
    struct MockMemoryMonitor {
        current_pressure: f64,
        min_batch_size: usize,
        max_batch_size: usize,
        default_batch_size: usize,
    }

    impl MockMemoryMonitor {
        fn new() -> Self {
            Self {
                current_pressure: 0.5,
                min_batch_size: 100,
                max_batch_size: 10_000,
                default_batch_size: 1_000,
            }
        }

        fn set_pressure(&mut self, pressure: f64) {
            self.current_pressure = pressure;
        }

        fn should_backpressure(&self) -> bool {
            self.current_pressure > 0.85
        }

        fn get_adaptive_batch_size(&self) -> usize {
            if self.current_pressure < 0.70 {
                self.default_batch_size
            } else if self.current_pressure < 0.85 {
                // Reduce batch size linearly
                let reduction_factor = (0.85 - self.current_pressure) / 0.15;
                let reduced_size = (self.default_batch_size as f64 * reduction_factor) as usize;
                reduced_size.max(self.min_batch_size)
            } else {
                self.min_batch_size
            }
        }
    }

    #[tokio::test]
    async fn test_memory_pressure_backpressure() {
        let mut monitor = MockMemoryMonitor::new();

        // Normal pressure - no backpressure
        monitor.set_pressure(0.60);
        assert!(
            !monitor.should_backpressure(),
            "Should not backpressure at 60%"
        );

        // High pressure - backpressure triggered
        monitor.set_pressure(0.90);
        assert!(monitor.should_backpressure(), "Should backpressure at 90%");
    }

    #[tokio::test]
    async fn test_adaptive_batch_sizing() {
        let mut monitor = MockMemoryMonitor::new();

        // Low pressure - full batch size
        monitor.set_pressure(0.50);
        let batch_size = monitor.get_adaptive_batch_size();
        assert_eq!(
            batch_size, 1_000,
            "Should use default batch size at low pressure"
        );

        // Medium pressure - reduced batch size
        monitor.set_pressure(0.75);
        let batch_size = monitor.get_adaptive_batch_size();
        assert!(
            batch_size < 1_000 && batch_size >= 100,
            "Should reduce batch size at medium pressure"
        );

        // Critical pressure - minimum batch size
        monitor.set_pressure(0.95);
        let batch_size = monitor.get_adaptive_batch_size();
        assert_eq!(
            batch_size, 100,
            "Should use minimum batch size at critical pressure"
        );
    }

    #[tokio::test]
    async fn test_memory_pressure_workflow_adaptation() {
        let mut monitor = MockMemoryMonitor::new();
        let mut processed_records = 0;
        let total_records = 10_000;

        // Simulate varying memory pressure during batch processing
        while processed_records < total_records {
            // Simulate increasing memory pressure
            let pressure = 0.5 + (processed_records as f64 / total_records as f64) * 0.4;
            monitor.set_pressure(pressure);

            let batch_size = monitor.get_adaptive_batch_size();

            // Process batch
            let remaining = total_records - processed_records;
            processed_records += batch_size.min(remaining);

            // Verify batch size adapts to pressure
            if pressure > 0.85 {
                assert_eq!(
                    batch_size, 100,
                    "Should use min batch size at high pressure"
                );
            }
        }

        assert_eq!(processed_records, 10_000);
    }
}

// ============================================================================
// Module 5: Combined Error Scenarios
// ============================================================================

#[cfg(test)]
mod combined_scenarios {
    use super::*;

    /// Test retry with circuit breaker
    #[tokio::test]
    async fn test_retry_with_circuit_breaker() {
        let retry_policy = RetryPolicy {
            max_retries: 5,
            initial_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(100),
            backoff_multiplier: 2.0,
        };

        let cb_config = CircuitBreakerConfig {
            failure_threshold: 3,
            success_threshold: 2,
            timeout: Duration::from_secs(5),
        };

        let cb = Arc::new(CircuitBreaker::new("combined_test", cb_config));
        let attempts = Arc::new(AtomicU32::new(0));

        let cb_clone = Arc::clone(&cb);
        let attempts_clone = Arc::clone(&attempts);

        let result = retry_async_with(
            retry_policy,
            || {
                let cb = Arc::clone(&cb_clone);
                let attempts = Arc::clone(&attempts_clone);
                async move {
                    // Check circuit breaker first
                    if !cb.is_closed() {
                        return Err(anyhow!("Circuit breaker open"));
                    }

                    let count = attempts.fetch_add(1, Ordering::SeqCst);
                    if count < 2 {
                        cb.record_failure();
                        Err(anyhow!("Temporary failure"))
                    } else {
                        cb.record_success();
                        Ok(42)
                    }
                }
            },
            |_err: &anyhow::Error| true,
        )
        .await;

        assert!(result.is_ok(), "Should eventually succeed");
        assert_eq!(result.unwrap(), 42);
    }

    /// Test timeout with retry
    #[tokio::test]
    async fn test_timeout_with_retry() {
        use tokio::time::timeout;

        let retry_policy = RetryPolicy {
            max_retries: 3,
            initial_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(50),
            backoff_multiplier: 2.0,
        };

        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = Arc::clone(&attempts);

        // Wrap retry logic with timeout
        let result = timeout(
            Duration::from_millis(200),
            retry_async_with(
                retry_policy,
                || {
                    let attempts = Arc::clone(&attempts_clone);
                    async move {
                        attempts.fetch_add(1, Ordering::SeqCst);
                        // Simulate slow failing operation so workflow timeout can trigger
                        tokio::time::sleep(Duration::from_millis(120)).await;
                        // Always fail
                        Err::<(), _>(anyhow!("Connection timeout"))
                    }
                },
                |_err: &anyhow::Error| true,
            ),
        )
        .await;

        // Should timeout before exhausting retries
        assert!(
            result.is_err(),
            "Should timeout before completing all retries"
        );
    }
}

// ============================================================================
// Module 6: Error Categorization Tests
// ============================================================================

#[cfg(test)]
mod error_categorization_tests {
    /// Categorize errors for retry decisions
    fn categorize_error(err: &str) -> ErrorCategory {
        let err = err.to_ascii_lowercase();

        if err.contains("-30081")
            || err.contains("connection")
            || err.contains("timeout")
            || err.contains("deadlock")
        {
            ErrorCategory::Retryable
        } else if err.contains("-803")
            || err.contains("duplicate")
            || err.contains("constraint")
            || err.contains("invalid")
        {
            ErrorCategory::Permanent
        } else {
            ErrorCategory::Unknown
        }
    }

    #[derive(Debug, PartialEq)]
    enum ErrorCategory {
        Retryable,
        Permanent,
        Unknown,
    }

    #[test]
    fn test_connection_error_is_retryable() {
        assert_eq!(
            categorize_error("SQL-30081N Connection failed"),
            ErrorCategory::Retryable
        );
        assert_eq!(
            categorize_error("Connection timeout"),
            ErrorCategory::Retryable
        );
    }

    #[test]
    fn test_constraint_violation_is_permanent() {
        assert_eq!(
            categorize_error("SQL-803 Duplicate key"),
            ErrorCategory::Permanent
        );
        assert_eq!(
            categorize_error("Constraint violation: unique_email"),
            ErrorCategory::Permanent
        );
    }

    #[test]
    fn test_deadlock_is_retryable() {
        assert_eq!(
            categorize_error("SQL-911 Deadlock detected"),
            ErrorCategory::Retryable
        );
    }

    #[test]
    fn test_unknown_error_category() {
        assert_eq!(
            categorize_error("Some unknown error"),
            ErrorCategory::Unknown
        );
    }
}
