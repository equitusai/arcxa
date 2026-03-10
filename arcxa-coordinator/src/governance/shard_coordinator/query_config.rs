//! Query Configuration and Failure Handling
//!
//! Configures how the distributed query system handles partial failures
//! when some shards are unavailable or return errors.
//!
//! ## Failure Modes
//!
//! - **FailFast**: Return error immediately if any shard fails
//! - **BestEffort**: Continue with results from successful shards (default)
//! - **Majority**: Require majority of shards to succeed
//! - **Quorum**: Require specific number/percentage of shards to succeed
//!
//! ## Usage
//!
//! ```ignore
//! use graphica_coordinator::governance::shard_coordinator::query_config::{QueryConfig, FailureMode};
//!
//! // Fail-fast mode
//! let config = QueryConfig::new(FailureMode::FailFast);
//!
//! // Best-effort mode (default)
//! let config = QueryConfig::default();
//!
//! // Majority mode
//! let config = QueryConfig::new(FailureMode::Majority);
//!
//! // Custom quorum
//! let config = QueryConfig::new(FailureMode::Quorum { min_shards: 3 });
//! ```

use anyhow::{Context, Result};

/// Query failure handling mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureMode {
    /// Return error immediately if any shard fails
    ///
    /// Use when query must succeed on all shards or fail completely.
    /// Guarantees complete results or error.
    FailFast,

    /// Continue with results from successful shards
    ///
    /// Use when partial results are acceptable.
    /// Returns whatever results are available from successful shards.
    BestEffort,

    /// Require majority of shards to succeed
    ///
    /// Use when you need high confidence but can tolerate some failures.
    /// Returns error if less than 50% of shards succeed.
    Majority,

    /// Require specific number of shards to succeed
    ///
    /// Use when you have specific consistency requirements.
    /// Returns error if fewer than `min_shards` succeed.
    Quorum {
        /// Minimum number of successful shards required
        min_shards: usize,
    },
}

impl Default for FailureMode {
    fn default() -> Self {
        FailureMode::BestEffort
    }
}

/// Query execution configuration
#[derive(Debug, Clone)]
pub struct QueryConfig {
    /// How to handle partial shard failures
    pub failure_mode: FailureMode,

    /// Timeout for individual shard queries (milliseconds)
    pub shard_timeout_ms: u64,

    /// Enable result deduplication (default: true)
    ///
    /// Deduplication removes duplicate triples that may exist
    /// on multiple shards due to replication.
    pub enable_deduplication: bool,

    /// Maximum number of results to return (default: unlimited)
    pub result_limit: Option<usize>,
}

impl Default for QueryConfig {
    fn default() -> Self {
        Self {
            failure_mode: FailureMode::BestEffort,
            shard_timeout_ms: 30_000, // 30 seconds
            enable_deduplication: true,
            result_limit: None,
        }
    }
}

impl QueryConfig {
    /// Create a new query configuration with specified failure mode
    pub fn new(failure_mode: FailureMode) -> Self {
        Self {
            failure_mode,
            ..Default::default()
        }
    }

    /// Set shard timeout in milliseconds
    pub fn with_timeout(mut self, timeout_ms: u64) -> Self {
        self.shard_timeout_ms = timeout_ms;
        self
    }

    /// Enable or disable result deduplication
    pub fn with_deduplication(mut self, enable: bool) -> Self {
        self.enable_deduplication = enable;
        self
    }

    /// Set result limit
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.result_limit = Some(limit);
        self
    }

    /// Check if the query results satisfy the failure mode requirements
    ///
    /// ## Arguments
    /// * `total_shards` - Total number of shards queried
    /// * `successful_shards` - Number of shards that succeeded
    /// * `errors` - Error messages from failed shards
    ///
    /// ## Returns
    /// `Ok(())` if requirements are met, `Err` with details if not
    pub fn validate_results(
        &self,
        total_shards: usize,
        successful_shards: usize,
        errors: &[String],
    ) -> Result<()> {
        match self.failure_mode {
            FailureMode::FailFast => {
                if !errors.is_empty() {
                    anyhow::bail!(
                        "Query failed in FailFast mode: {} of {} shards failed. Errors: {:?}",
                        errors.len(),
                        total_shards,
                        errors
                    );
                }
                Ok(())
            }

            FailureMode::BestEffort => {
                // Always OK in best-effort mode, even if all shards fail
                Ok(())
            }

            FailureMode::Majority => {
                // Strict majority: more than 50% (not equal to)
                if successful_shards * 2 <= total_shards {
                    let required = (total_shards / 2) + 1;
                    anyhow::bail!(
                        "Query failed in Majority mode: only {} of {} shards succeeded (need {} for majority). Errors: {:?}",
                        successful_shards,
                        total_shards,
                        required,
                        errors
                    );
                }
                Ok(())
            }

            FailureMode::Quorum { min_shards } => {
                if successful_shards < min_shards {
                    anyhow::bail!(
                        "Query failed in Quorum mode: only {} of {} shards succeeded (need {}). Errors: {:?}",
                        successful_shards,
                        total_shards,
                        min_shards,
                        errors
                    );
                }
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = QueryConfig::default();
        assert_eq!(config.failure_mode, FailureMode::BestEffort);
        assert_eq!(config.shard_timeout_ms, 30_000);
        assert!(config.enable_deduplication);
        assert_eq!(config.result_limit, None);
    }

    #[test]
    fn test_config_builder() {
        let config = QueryConfig::new(FailureMode::FailFast)
            .with_timeout(10_000)
            .with_deduplication(false)
            .with_limit(100);

        assert_eq!(config.failure_mode, FailureMode::FailFast);
        assert_eq!(config.shard_timeout_ms, 10_000);
        assert!(!config.enable_deduplication);
        assert_eq!(config.result_limit, Some(100));
    }

    #[test]
    fn test_fail_fast_all_succeed() {
        let config = QueryConfig::new(FailureMode::FailFast);
        let result = config.validate_results(4, 4, &[]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_fail_fast_one_fails() {
        let config = QueryConfig::new(FailureMode::FailFast);
        let errors = vec!["Shard 2 timeout".to_string()];
        let result = config.validate_results(4, 3, &errors);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("FailFast mode"));
    }

    #[test]
    fn test_best_effort_all_fail() {
        let config = QueryConfig::new(FailureMode::BestEffort);
        let errors = vec![
            "Shard 1 timeout".to_string(),
            "Shard 2 timeout".to_string(),
            "Shard 3 timeout".to_string(),
            "Shard 4 timeout".to_string(),
        ];
        let result = config.validate_results(4, 0, &errors);
        assert!(result.is_ok()); // Best-effort always succeeds
    }

    #[test]
    fn test_majority_exact_half() {
        let config = QueryConfig::new(FailureMode::Majority);
        let errors = vec!["Shard 1 timeout".to_string(), "Shard 2 timeout".to_string()];
        let result = config.validate_results(4, 2, &errors);
        assert!(result.is_err()); // Need 3 out of 4 (ceiling of 4/2 = 2, but we need >50%)
    }

    #[test]
    fn test_majority_more_than_half() {
        let config = QueryConfig::new(FailureMode::Majority);
        let errors = vec!["Shard 1 timeout".to_string()];
        let result = config.validate_results(4, 3, &errors);
        assert!(result.is_ok()); // 3 out of 4 is majority
    }

    #[test]
    fn test_majority_odd_number() {
        let config = QueryConfig::new(FailureMode::Majority);

        // 3 out of 5 is majority
        let result = config.validate_results(5, 3, &["e1".to_string(), "e2".to_string()]);
        assert!(result.is_ok());

        // 2 out of 5 is not majority
        let result = config.validate_results(
            5,
            2,
            &["e1".to_string(), "e2".to_string(), "e3".to_string()],
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_quorum_sufficient() {
        let config = QueryConfig::new(FailureMode::Quorum { min_shards: 3 });
        let result = config.validate_results(5, 3, &["e1".to_string(), "e2".to_string()]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_quorum_insufficient() {
        let config = QueryConfig::new(FailureMode::Quorum { min_shards: 3 });
        let errors = vec![
            "Shard 1 timeout".to_string(),
            "Shard 2 timeout".to_string(),
            "Shard 3 timeout".to_string(),
        ];
        let result = config.validate_results(5, 2, &errors);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Quorum mode"));
    }

    #[test]
    fn test_quorum_exact() {
        let config = QueryConfig::new(FailureMode::Quorum { min_shards: 4 });
        let result = config.validate_results(5, 4, &["e1".to_string()]);
        assert!(result.is_ok());
    }
}
