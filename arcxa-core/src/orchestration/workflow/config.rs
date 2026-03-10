//! Workflow Execution Configuration
//!
//! Provides timeout and execution policy configuration for workflow execution.
//! Part of Phase 2 production hardening - timeout management and retry policies.

use super::error::WorkflowErrorCategory;
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::time::Duration;

/// Execution timeout configuration for workflow execution
///
/// Provides hierarchical timeout control:
/// - Workflow level: entire pipeline execution
/// - Stage level: individual transformer steps
/// - Record level: per-record processing
/// - Connection/Query level: external system operations
///
/// # Examples
///
/// ```
/// use graphica_core::orchestration::workflow::config::ExecutionTimeout;
///
/// // Default configuration (production-friendly)
/// let timeout = ExecutionTimeout::default();
/// assert_eq!(timeout.workflow_timeout_secs, Some(3600)); // 1 hour
///
/// // Strict configuration (low-latency scenarios)
/// let strict = ExecutionTimeout::strict();
/// assert_eq!(strict.workflow_timeout_secs, Some(600)); // 10 minutes
///
/// // Infinite configuration (long-running batch jobs)
/// let infinite = ExecutionTimeout::infinite();
/// assert_eq!(infinite.workflow_timeout_secs, None);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionTimeout {
    /// Total workflow timeout (entire pipeline)
    /// None = no timeout (infinite execution allowed)
    pub workflow_timeout_secs: Option<u64>,

    /// Per-stage timeout (each transformer step)
    /// None = no per-stage timeout
    pub stage_timeout_secs: Option<u64>,

    /// Per-record timeout (individual record processing)
    /// None = no per-record timeout
    pub record_timeout_ms: Option<u64>,

    /// Connection timeout for external systems (DB2, Kafka, HTTP)
    /// Always enforced to prevent indefinite hangs
    pub connection_timeout_secs: u64,

    /// Query/operation timeout for databases
    /// Always enforced to prevent long-running queries
    pub query_timeout_secs: u64,
}

impl Default for ExecutionTimeout {
    /// Default timeout configuration for production workflows
    ///
    /// Balanced timeouts suitable for most data processing workloads:
    /// - 1 hour total workflow timeout
    /// - 10 minutes per stage
    /// - 1 second per record
    /// - 30 seconds for connections
    /// - 5 minutes for queries
    fn default() -> Self {
        Self {
            workflow_timeout_secs: Some(3600), // 1 hour total
            stage_timeout_secs: Some(600),     // 10 minutes per stage
            record_timeout_ms: Some(1000),     // 1 second per record
            connection_timeout_secs: 30,       // 30s connection timeout
            query_timeout_secs: 300,           // 5 minutes for queries
        }
    }
}

impl ExecutionTimeout {
    /// Create infinite timeout configuration
    ///
    /// No workflow, stage, or record timeouts.
    /// Only connection and query timeouts are enforced to prevent indefinite hangs.
    ///
    /// Use for:
    /// - Long-running batch ETL jobs
    /// - Data migrations with unpredictable duration
    /// - Development/testing scenarios
    pub fn infinite() -> Self {
        Self {
            workflow_timeout_secs: None,
            stage_timeout_secs: None,
            record_timeout_ms: None,
            connection_timeout_secs: 30, // Still enforce connection timeout
            query_timeout_secs: 300,     // Still enforce query timeout
        }
    }

    /// Create strict timeout configuration
    ///
    /// Aggressive timeouts for low-latency, real-time scenarios.
    ///
    /// Use for:
    /// - Real-time event processing
    /// - Interactive APIs with SLA requirements
    /// - High-throughput transactional workloads
    pub fn strict() -> Self {
        Self {
            workflow_timeout_secs: Some(600), // 10 minutes
            stage_timeout_secs: Some(120),    // 2 minutes
            record_timeout_ms: Some(500),     // 500ms
            connection_timeout_secs: 10,      // 10s connection
            query_timeout_secs: 60,           // 1 minute for queries
        }
    }

    /// Get workflow timeout as Duration
    ///
    /// Returns None if no workflow timeout is configured (infinite)
    pub fn workflow_duration(&self) -> Option<Duration> {
        self.workflow_timeout_secs.map(Duration::from_secs)
    }

    /// Get stage timeout as Duration
    ///
    /// Returns None if no stage timeout is configured
    pub fn stage_duration(&self) -> Option<Duration> {
        self.stage_timeout_secs.map(Duration::from_secs)
    }

    /// Get record timeout as Duration
    ///
    /// Returns None if no record timeout is configured
    pub fn record_duration(&self) -> Option<Duration> {
        self.record_timeout_ms.map(Duration::from_millis)
    }

    /// Get connection timeout as Duration
    ///
    /// Always returns a duration (never None)
    pub fn connection_duration(&self) -> Duration {
        Duration::from_secs(self.connection_timeout_secs)
    }

    /// Get query timeout as Duration
    ///
    /// Always returns a duration (never None)
    pub fn query_duration(&self) -> Duration {
        Duration::from_secs(self.query_timeout_secs)
    }

    /// Create custom timeout configuration
    ///
    /// Builder-style constructor for custom timeout scenarios
    pub fn custom() -> ExecutionTimeoutBuilder {
        ExecutionTimeoutBuilder::default()
    }
}

/// Builder for custom ExecutionTimeout configurations
#[derive(Debug, Default)]
pub struct ExecutionTimeoutBuilder {
    workflow_timeout_secs: Option<u64>,
    stage_timeout_secs: Option<u64>,
    record_timeout_ms: Option<u64>,
    connection_timeout_secs: Option<u64>,
    query_timeout_secs: Option<u64>,
}

impl ExecutionTimeoutBuilder {
    /// Set workflow timeout in seconds
    pub fn workflow_timeout_secs(mut self, secs: u64) -> Self {
        self.workflow_timeout_secs = Some(secs);
        self
    }

    /// Set stage timeout in seconds
    pub fn stage_timeout_secs(mut self, secs: u64) -> Self {
        self.stage_timeout_secs = Some(secs);
        self
    }

    /// Set record timeout in milliseconds
    pub fn record_timeout_ms(mut self, ms: u64) -> Self {
        self.record_timeout_ms = Some(ms);
        self
    }

    /// Set connection timeout in seconds
    pub fn connection_timeout_secs(mut self, secs: u64) -> Self {
        self.connection_timeout_secs = Some(secs);
        self
    }

    /// Set query timeout in seconds
    pub fn query_timeout_secs(mut self, secs: u64) -> Self {
        self.query_timeout_secs = Some(secs);
        self
    }

    /// Build ExecutionTimeout configuration
    pub fn build(self) -> ExecutionTimeout {
        ExecutionTimeout {
            workflow_timeout_secs: self.workflow_timeout_secs,
            stage_timeout_secs: self.stage_timeout_secs,
            record_timeout_ms: self.record_timeout_ms,
            connection_timeout_secs: self.connection_timeout_secs.unwrap_or(30),
            query_timeout_secs: self.query_timeout_secs.unwrap_or(300),
        }
    }
}

/// Retry policy configuration for handling transient failures
///
/// Implements exponential backoff with optional jitter to prevent
/// thundering herd problems in distributed systems.
///
/// # Examples
///
/// ```rust,no_run
/// use graphica_core::orchestration::workflow::config::RetryPolicy;
/// use graphica_core::orchestration::workflow::error::WorkflowErrorCategory;
///
/// async fn connect_to_db() -> Result<(), String> {
///     // ... connection logic
///     Ok(())
/// }
///
/// let policy = RetryPolicy::default();
/// let result = policy.execute_with_retry(
///     || connect_to_db(),
///     |err| {
///         if err.contains("timeout") {
///             WorkflowErrorCategory::TimeoutError
///         } else {
///             WorkflowErrorCategory::ConnectionError
///         }
///     }
/// ).await;
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicy {
    /// Maximum number of retry attempts (0 = no retries)
    pub max_retries: u32,

    /// Initial backoff delay in milliseconds
    pub initial_backoff_ms: u64,

    /// Maximum backoff delay in milliseconds (caps exponential growth)
    pub max_backoff_ms: u64,

    /// Exponential backoff multiplier (e.g., 2.0 = double each time)
    pub multiplier: f64,

    /// Add random jitter to prevent thundering herd (±15% variation)
    pub jitter: bool,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_backoff_ms: 100,
            max_backoff_ms: 5000,
            multiplier: 2.0,
            jitter: true,
        }
    }
}

impl RetryPolicy {
    /// Conservative retry policy for critical operations
    ///
    /// - 5 retries
    /// - 200ms initial delay
    /// - 10s max delay
    /// - 2x multiplier with jitter
    pub fn conservative() -> Self {
        Self {
            max_retries: 5,
            initial_backoff_ms: 200,
            max_backoff_ms: 10000,
            multiplier: 2.0,
            jitter: true,
        }
    }

    /// Aggressive retry policy for latency-sensitive operations
    ///
    /// - 1 retry
    /// - 50ms initial delay
    /// - 1s max delay
    /// - 1.5x multiplier, no jitter
    pub fn aggressive() -> Self {
        Self {
            max_retries: 1,
            initial_backoff_ms: 50,
            max_backoff_ms: 1000,
            multiplier: 1.5,
            jitter: false,
        }
    }

    /// No-retry policy (fail fast)
    pub fn no_retry() -> Self {
        Self {
            max_retries: 0,
            initial_backoff_ms: 0,
            max_backoff_ms: 0,
            multiplier: 1.0,
            jitter: false,
        }
    }

    /// Execute operation with retry logic and exponential backoff
    ///
    /// Only retries errors categorized as retryable (transient failures).
    /// Fatal and permanent errors fail immediately without retry.
    ///
    /// # Arguments
    ///
    /// * `operation` - Async closure to execute (may be called multiple times)
    /// * `error_category_fn` - Function to categorize errors for retry decision
    ///
    /// # Returns
    ///
    /// - `Ok(T)` if operation succeeds (possibly after retries)
    /// - `Err(E)` if operation fails permanently or retries exhausted
    pub async fn execute_with_retry<F, Fut, T, E>(
        &self,
        mut operation: F,
        error_category_fn: impl Fn(&E) -> WorkflowErrorCategory,
    ) -> Result<T, E>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<T, E>>,
    {
        let mut attempts = 0;

        loop {
            match operation().await {
                Ok(result) => return Ok(result),
                Err(err) => {
                    let category = error_category_fn(&err);

                    // Fail fast for non-retryable errors
                    if !category.is_retryable() || attempts >= self.max_retries {
                        return Err(err);
                    }

                    attempts += 1;
                    let delay = self.calculate_backoff(attempts);

                    // Async sleep (non-blocking in tokio runtime)
                    tokio::time::sleep(Duration::from_millis(delay)).await;
                }
            }
        }
    }

    /// Calculate backoff delay for given attempt number
    ///
    /// Implements exponential backoff: initial_delay * multiplier^(attempt-1)
    /// Capped at max_backoff_ms, with optional ±15% jitter.
    fn calculate_backoff(&self, attempt: u32) -> u64 {
        let base_delay = self.initial_backoff_ms as f64 * self.multiplier.powi(attempt as i32 - 1);
        let mut delay = base_delay.min(self.max_backoff_ms as f64) as u64;

        if self.jitter {
            // Add ±15% random jitter to prevent thundering herd
            use rand::Rng;
            let mut rng = rand::thread_rng();
            let jitter_factor = rng.gen_range(-0.15..=0.15);
            let jitter = (delay as f64 * jitter_factor) as i64;
            delay = ((delay as i64 + jitter).max(0)) as u64;
        }

        delay
    }

    /// Calculate all backoff delays for max_retries
    ///
    /// Useful for testing and logging retry schedule.
    pub fn backoff_schedule(&self) -> Vec<u64> {
        (1..=self.max_retries)
            .map(|attempt| {
                let base_delay =
                    self.initial_backoff_ms as f64 * self.multiplier.powi(attempt as i32 - 1);
                base_delay.min(self.max_backoff_ms as f64) as u64
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_timeout() {
        let timeout = ExecutionTimeout::default();
        assert_eq!(timeout.workflow_timeout_secs, Some(3600));
        assert_eq!(timeout.stage_timeout_secs, Some(600));
        assert_eq!(timeout.record_timeout_ms, Some(1000));
        assert_eq!(timeout.connection_timeout_secs, 30);
        assert_eq!(timeout.query_timeout_secs, 300);
    }

    #[test]
    fn test_infinite_timeout() {
        let timeout = ExecutionTimeout::infinite();
        assert_eq!(timeout.workflow_timeout_secs, None);
        assert_eq!(timeout.stage_timeout_secs, None);
        assert_eq!(timeout.record_timeout_ms, None);
        assert_eq!(timeout.connection_timeout_secs, 30);
        assert_eq!(timeout.query_timeout_secs, 300);
    }

    #[test]
    fn test_strict_timeout() {
        let timeout = ExecutionTimeout::strict();
        assert_eq!(timeout.workflow_timeout_secs, Some(600));
        assert_eq!(timeout.stage_timeout_secs, Some(120));
        assert_eq!(timeout.record_timeout_ms, Some(500));
        assert_eq!(timeout.connection_timeout_secs, 10);
        assert_eq!(timeout.query_timeout_secs, 60);
    }

    #[test]
    fn test_timeout_durations() {
        let timeout = ExecutionTimeout::default();

        assert_eq!(timeout.workflow_duration(), Some(Duration::from_secs(3600)));
        assert_eq!(timeout.stage_duration(), Some(Duration::from_secs(600)));
        assert_eq!(timeout.record_duration(), Some(Duration::from_millis(1000)));
        assert_eq!(timeout.connection_duration(), Duration::from_secs(30));
        assert_eq!(timeout.query_duration(), Duration::from_secs(300));
    }

    #[test]
    fn test_custom_timeout_builder() {
        let timeout = ExecutionTimeout::custom()
            .workflow_timeout_secs(1800)
            .stage_timeout_secs(300)
            .record_timeout_ms(2000)
            .connection_timeout_secs(20)
            .query_timeout_secs(180)
            .build();

        assert_eq!(timeout.workflow_timeout_secs, Some(1800));
        assert_eq!(timeout.stage_timeout_secs, Some(300));
        assert_eq!(timeout.record_timeout_ms, Some(2000));
        assert_eq!(timeout.connection_timeout_secs, 20);
        assert_eq!(timeout.query_timeout_secs, 180);
    }

    #[test]
    fn test_serialization() {
        let timeout = ExecutionTimeout::default();
        let json = serde_json::to_string(&timeout).unwrap();
        let deserialized: ExecutionTimeout = serde_json::from_str(&json).unwrap();

        assert_eq!(
            timeout.workflow_timeout_secs,
            deserialized.workflow_timeout_secs
        );
        assert_eq!(timeout.stage_timeout_secs, deserialized.stage_timeout_secs);
        assert_eq!(timeout.record_timeout_ms, deserialized.record_timeout_ms);
    }

    // RetryPolicy tests
    #[test]
    fn test_default_retry_policy() {
        let policy = RetryPolicy::default();
        assert_eq!(policy.max_retries, 3);
        assert_eq!(policy.initial_backoff_ms, 100);
        assert_eq!(policy.max_backoff_ms, 5000);
        assert_eq!(policy.multiplier, 2.0);
        assert!(policy.jitter);
    }

    #[test]
    fn test_conservative_policy() {
        let policy = RetryPolicy::conservative();
        assert_eq!(policy.max_retries, 5);
        assert_eq!(policy.initial_backoff_ms, 200);
        assert_eq!(policy.max_backoff_ms, 10000);
    }

    #[test]
    fn test_aggressive_policy() {
        let policy = RetryPolicy::aggressive();
        assert_eq!(policy.max_retries, 1);
        assert_eq!(policy.initial_backoff_ms, 50);
        assert_eq!(policy.max_backoff_ms, 1000);
        assert!(!policy.jitter);
    }

    #[test]
    fn test_backoff_schedule() {
        let policy = RetryPolicy {
            max_retries: 3,
            initial_backoff_ms: 100,
            max_backoff_ms: 1000,
            multiplier: 2.0,
            jitter: false,
        };

        let schedule = policy.backoff_schedule();
        assert_eq!(schedule.len(), 3);
        assert_eq!(schedule[0], 100); // 100 * 2^0
        assert_eq!(schedule[1], 200); // 100 * 2^1
        assert_eq!(schedule[2], 400); // 100 * 2^2
    }

    #[test]
    fn test_backoff_capping() {
        let policy = RetryPolicy {
            max_retries: 5,
            initial_backoff_ms: 100,
            max_backoff_ms: 500,
            multiplier: 2.0,
            jitter: false,
        };

        let schedule = policy.backoff_schedule();
        assert_eq!(schedule[0], 100); // 100 * 2^0
        assert_eq!(schedule[1], 200); // 100 * 2^1
        assert_eq!(schedule[2], 400); // 100 * 2^2
        assert_eq!(schedule[3], 500); // capped at max
        assert_eq!(schedule[4], 500); // capped at max
    }

    #[tokio::test]
    async fn test_execute_with_retry_success() {
        let policy = RetryPolicy::default();
        let call_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = call_count.clone();

        let result = policy
            .execute_with_retry(
                move || {
                    let counter = counter.clone();
                    async move {
                        counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        Ok::<i32, String>(42)
                    }
                },
                |_: &String| WorkflowErrorCategory::ConnectionError,
            )
            .await;

        assert_eq!(result, Ok(42));
        assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_execute_with_retry_eventual_success() {
        let policy = RetryPolicy::no_retry();
        let call_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = call_count.clone();

        let result = policy
            .execute_with_retry(
                move || {
                    let counter = counter.clone();
                    async move {
                        let attempt = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
                        if attempt < 2 {
                            Err("Transient error")
                        } else {
                            Ok(42)
                        }
                    }
                },
                |_: &&str| WorkflowErrorCategory::ConnectionError,
            )
            .await;

        // With no_retry policy, first failure returns immediately
        assert!(result.is_err());
        assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_execute_with_retry_non_retryable() {
        let policy = RetryPolicy::default();
        let call_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = call_count.clone();

        let result = policy
            .execute_with_retry(
                move || {
                    let counter = counter.clone();
                    async move {
                        counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        Err::<i32, &str>("Data validation failed")
                    }
                },
                |_: &&str| WorkflowErrorCategory::DataValidationError,
            )
            .await;

        // Should fail immediately without retry
        assert!(result.is_err());
        assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 1);
    }
}
