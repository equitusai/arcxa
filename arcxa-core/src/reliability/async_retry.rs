//! # Async Retry Module
//!
//! Non-blocking retry logic using Tokio for asynchronous operations.
//!
//! Eliminates thread blocking during retry delays, enabling higher concurrency.

use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, warn};

/// Retry policy configuration
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// Maximum number of retry attempts (0 = no retries)
    pub max_retries: u32,
    /// Initial delay between retries
    pub initial_delay: Duration,
    /// Maximum delay between retries (for exponential backoff)
    pub max_delay: Duration,
    /// Backoff multiplier (1.0 = fixed delay, 2.0 = exponential)
    pub backoff_multiplier: f64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(10),
            backoff_multiplier: 2.0,
        }
    }
}

impl RetryPolicy {
    /// No retry policy (fail immediately)
    pub fn no_retry() -> Self {
        Self {
            max_retries: 0,
            ..Default::default()
        }
    }

    /// Fixed delay retry (same delay between all retries)
    pub fn fixed_delay(max_retries: u32, delay: Duration) -> Self {
        Self {
            max_retries,
            initial_delay: delay,
            max_delay: delay,
            backoff_multiplier: 1.0,
        }
    }

    /// Exponential backoff retry
    pub fn exponential_backoff(max_retries: u32) -> Self {
        Self {
            max_retries,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(10),
            backoff_multiplier: 2.0,
        }
    }

    /// Calculate delay for a given retry attempt
    fn delay_for_attempt(&self, attempt: u32) -> Duration {
        if self.backoff_multiplier == 1.0 {
            // Fixed delay
            self.initial_delay
        } else {
            // Exponential backoff
            let delay_ms = self.initial_delay.as_millis() as f64
                * self.backoff_multiplier.powi(attempt as i32);
            let delay_ms = delay_ms.min(self.max_delay.as_millis() as f64);
            Duration::from_millis(delay_ms as u64)
        }
    }
}

/// Retry an async operation with exponential backoff
///
/// # Example
/// ```no_run
/// use graphica_core::reliability::async_retry::{retry_async, RetryPolicy};
///
/// async fn flaky_operation() -> Result<String, String> {
///     // Your operation here
///     Ok("success".to_string())
/// }
///
/// # async fn example() {
/// let policy = RetryPolicy::exponential_backoff(5);
/// let result = retry_async(policy, || async {
///     flaky_operation().await
/// }).await;
/// # }
/// ```
pub async fn retry_async<F, Fut, T, E>(policy: RetryPolicy, mut operation: F) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    let mut attempt = 0;

    loop {
        match operation().await {
            Ok(result) => {
                if attempt > 0 {
                    debug!("Operation succeeded after {} retries", attempt);
                }
                return Ok(result);
            }
            Err(e) => {
                if attempt >= policy.max_retries {
                    warn!("Operation failed after {} attempts: {}", attempt + 1, e);
                    return Err(e);
                }

                let delay = policy.delay_for_attempt(attempt);
                warn!(
                    "Operation failed (attempt {}/{}): {}. Retrying in {:?}",
                    attempt + 1,
                    policy.max_retries + 1,
                    e,
                    delay
                );

                // Non-blocking async sleep
                sleep(delay).await;

                attempt += 1;
            }
        }
    }
}

/// Retry an async operation with custom retry decision logic
///
/// # Example
/// ```no_run
/// use graphica_core::reliability::async_retry::{retry_async_with, RetryPolicy};
///
/// async fn operation() -> Result<String, String> {
///     Ok("success".to_string())
/// }
///
/// # async fn example() {
/// let policy = RetryPolicy::default();
/// let result = retry_async_with(
///     policy,
///     || async { operation().await },
///     |err| {
///         // Only retry on specific errors
///         err.contains("temporary")
///     }
/// ).await;
/// # }
/// ```
pub async fn retry_async_with<F, Fut, T, E, P>(
    policy: RetryPolicy,
    mut operation: F,
    mut should_retry: P,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Display,
    P: FnMut(&E) -> bool,
{
    let mut attempt = 0;

    loop {
        match operation().await {
            Ok(result) => {
                if attempt > 0 {
                    debug!("Operation succeeded after {} retries", attempt);
                }
                return Ok(result);
            }
            Err(e) => {
                // Check if we should retry this error
                if !should_retry(&e) {
                    debug!("Error not retryable: {}", e);
                    return Err(e);
                }

                if attempt >= policy.max_retries {
                    warn!("Operation failed after {} attempts: {}", attempt + 1, e);
                    return Err(e);
                }

                let delay = policy.delay_for_attempt(attempt);
                warn!(
                    "Operation failed (attempt {}/{}): {}. Retrying in {:?}",
                    attempt + 1,
                    policy.max_retries + 1,
                    e,
                    delay
                );

                sleep(delay).await;
                attempt += 1;
            }
        }
    }
}

/// Retry metrics for monitoring
pub struct RetryMetrics {
    pub total_attempts: u64,
    pub successful_first_try: u64,
    pub successful_after_retry: u64,
    pub failed_after_retries: u64,
}

impl RetryMetrics {
    pub fn new() -> Self {
        Self {
            total_attempts: 0,
            successful_first_try: 0,
            successful_after_retry: 0,
            failed_after_retries: 0,
        }
    }

    pub fn success_rate(&self) -> f64 {
        if self.total_attempts == 0 {
            return 0.0;
        }
        let successful = self.successful_first_try + self.successful_after_retry;
        (successful as f64 / self.total_attempts as f64) * 100.0
    }

    pub fn retry_rate(&self) -> f64 {
        if self.total_attempts == 0 {
            return 0.0;
        }
        (self.successful_after_retry as f64 / self.total_attempts as f64) * 100.0
    }
}

/// Async retry with metrics tracking
pub async fn retry_async_with_metrics<F, Fut, T, E>(
    policy: RetryPolicy,
    mut operation: F,
    metrics: &mut RetryMetrics,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    metrics.total_attempts += 1;
    let mut attempt = 0;

    loop {
        match operation().await {
            Ok(result) => {
                if attempt == 0 {
                    metrics.successful_first_try += 1;
                } else {
                    metrics.successful_after_retry += 1;
                }
                return Ok(result);
            }
            Err(e) => {
                if attempt >= policy.max_retries {
                    metrics.failed_after_retries += 1;
                    return Err(e);
                }

                let delay = policy.delay_for_attempt(attempt);
                sleep(delay).await;
                attempt += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_retry_succeeds_immediately() {
        let policy = RetryPolicy::default();
        let result = retry_async(policy, || async { Ok::<_, String>("success") }).await;
        assert_eq!(result.unwrap(), "success");
    }

    #[tokio::test]
    async fn test_retry_succeeds_after_failures() {
        let policy = RetryPolicy::exponential_backoff(3);
        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = Arc::clone(&attempts);

        let result = retry_async(policy, move || {
            let attempts = Arc::clone(&attempts_clone);
            async move {
                let count = attempts.fetch_add(1, Ordering::SeqCst);
                if count < 2 {
                    Err::<String, _>("temporary failure".to_string())
                } else {
                    Ok("success".to_string())
                }
            }
        })
        .await;

        assert_eq!(result.unwrap(), "success");
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_fails_after_max_attempts() {
        let policy = RetryPolicy {
            max_retries: 2,
            initial_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(1),
            backoff_multiplier: 1.0,
        };

        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = Arc::clone(&attempts);

        let result = retry_async(policy, move || {
            let attempts = Arc::clone(&attempts_clone);
            async move {
                attempts.fetch_add(1, Ordering::SeqCst);
                Err::<String, _>("permanent failure".to_string())
            }
        })
        .await;

        assert!(result.is_err());
        assert_eq!(attempts.load(Ordering::SeqCst), 3); // 1 initial + 2 retries
    }

    #[tokio::test]
    async fn test_retry_with_custom_logic() {
        let policy = RetryPolicy::default();

        let result = retry_async_with(
            policy,
            || async { Err::<String, _>("not_retryable".to_string()) },
            |err| err.contains("retryable"),
        )
        .await;

        // Should fail immediately without retries
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_exponential_backoff_delays() {
        let policy = RetryPolicy {
            max_retries: 3,
            initial_delay: Duration::from_millis(10),
            max_delay: Duration::from_secs(1),
            backoff_multiplier: 2.0,
        };

        assert_eq!(policy.delay_for_attempt(0), Duration::from_millis(10));
        assert_eq!(policy.delay_for_attempt(1), Duration::from_millis(20));
        assert_eq!(policy.delay_for_attempt(2), Duration::from_millis(40));
        assert_eq!(policy.delay_for_attempt(3), Duration::from_millis(80));
    }

    #[tokio::test]
    async fn test_retry_metrics() {
        let policy = RetryPolicy::exponential_backoff(3);
        let mut metrics = RetryMetrics::new();

        // Successful on first try
        let _ = retry_async_with_metrics(
            policy.clone(),
            || async { Ok::<_, String>("success") },
            &mut metrics,
        )
        .await;

        assert_eq!(metrics.total_attempts, 1);
        assert_eq!(metrics.successful_first_try, 1);
        assert_eq!(metrics.successful_after_retry, 0);

        // Successful after retries
        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = Arc::clone(&attempts);

        let _ = retry_async_with_metrics(
            policy.clone(),
            move || {
                let attempts = Arc::clone(&attempts_clone);
                async move {
                    let count = attempts.fetch_add(1, Ordering::SeqCst);
                    if count < 2 {
                        Err::<String, _>("fail".to_string())
                    } else {
                        Ok("success".to_string())
                    }
                }
            },
            &mut metrics,
        )
        .await;

        assert_eq!(metrics.total_attempts, 2);
        assert_eq!(metrics.successful_first_try, 1);
        assert_eq!(metrics.successful_after_retry, 1);
        assert_eq!(metrics.success_rate(), 100.0);
    }
}
