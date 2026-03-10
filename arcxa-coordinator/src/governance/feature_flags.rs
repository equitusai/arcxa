//! Feature flags for governance brain migration
//!
//! Supports both compile-time (Cargo features) and runtime configuration
//! to enable gradual rollout of async governance brain.

use once_cell::sync::Lazy;
use std::env;

/// Governance brain operating mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GovernanceMode {
    /// Use synchronous implementation (legacy)
    Sync,
    /// Use asynchronous implementation (new)
    Async,
    /// Automatically choose based on workload characteristics
    Auto,
}

impl GovernanceMode {
    /// Get the configured governance mode from environment
    pub fn from_env() -> Self {
        match env::var("GRAPHICA_GOVERNANCE_MODE") {
            Ok(val) if val.eq_ignore_ascii_case("sync") => GovernanceMode::Sync,
            Ok(val) if val.eq_ignore_ascii_case("async") => GovernanceMode::Async,
            Ok(val) if val.eq_ignore_ascii_case("auto") => GovernanceMode::Auto,
            _ => Self::default(),
        }
    }

    /// Check if async mode is enabled
    pub fn is_async(&self) -> bool {
        match self {
            GovernanceMode::Async => true,
            GovernanceMode::Auto => Self::should_use_async_auto(),
            GovernanceMode::Sync => false,
        }
    }

    /// Auto-detection logic for choosing sync vs async
    fn should_use_async_auto() -> bool {
        // Use async if:
        // 1. Multiple CPU cores available (parallelism benefit)
        // 2. Not in test mode (unless explicitly testing async)
        // 3. Async feature is compiled in

        #[cfg(feature = "async-governance")]
        {
            let num_cores = num_cpus::get();
            let in_test = cfg!(test) && !env::var("TEST_ASYNC_GOVERNANCE").is_ok();

            num_cores > 2 && !in_test
        }

        #[cfg(not(feature = "async-governance"))]
        {
            false // Can't use async if not compiled in
        }
    }
}

impl Default for GovernanceMode {
    fn default() -> Self {
        // Default based on compiled features
        #[cfg(all(feature = "async-governance", not(feature = "sync-governance")))]
        {
            GovernanceMode::Async
        }

        #[cfg(all(feature = "sync-governance", not(feature = "async-governance")))]
        {
            GovernanceMode::Sync
        }

        #[cfg(feature = "all-governance")]
        {
            GovernanceMode::Auto
        }

        #[cfg(not(any(feature = "async-governance", feature = "sync-governance")))]
        {
            // If no features specified, use default feature (async)
            GovernanceMode::Async
        }
    }
}

/// Global governance mode configuration
pub static GOVERNANCE_MODE: Lazy<GovernanceMode> = Lazy::new(|| GovernanceMode::from_env());

/// Migration configuration for gradual rollout
#[derive(Debug, Clone)]
pub struct MigrationConfig {
    /// Percentage of requests to route to async (0-100)
    pub async_percentage: u8,
    /// Enable A/B testing metrics
    pub enable_ab_metrics: bool,
    /// Fallback to sync on async errors
    pub fallback_on_error: bool,
    /// Maximum retry attempts before fallback
    pub max_retries: u32,
}

impl MigrationConfig {
    /// Load migration config from environment
    pub fn from_env() -> Self {
        Self {
            async_percentage: env::var("GRAPHICA_ASYNC_PERCENTAGE")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(100),
            enable_ab_metrics: env::var("GRAPHICA_AB_METRICS")
                .ok()
                .map(|v| v.eq_ignore_ascii_case("true"))
                .unwrap_or(false),
            fallback_on_error: env::var("GRAPHICA_FALLBACK_ON_ERROR")
                .ok()
                .map(|v| v.eq_ignore_ascii_case("true"))
                .unwrap_or(true),
            max_retries: env::var("GRAPHICA_MAX_RETRIES")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(3),
        }
    }

    /// Check if request should use async based on percentage
    pub fn should_use_async(&self) -> bool {
        // For gradual rollout, always use async if percentage >= 100
        if self.async_percentage >= 100 {
            return true;
        }
        if self.async_percentage == 0 {
            return false;
        }

        // Use random selection for percentage-based rollout
        // This provides proper distribution for gradual feature rollouts
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let random_value: u8 = rng.gen_range(0..100);
        random_value < self.async_percentage
    }
}

impl Default for MigrationConfig {
    fn default() -> Self {
        Self {
            async_percentage: 100, // Full async by default
            enable_ab_metrics: false,
            fallback_on_error: true,
            max_retries: 3,
        }
    }
}

/// Factory for creating governance brain based on feature flags
pub struct GovernanceBrainFactory;

impl GovernanceBrainFactory {
    /// Create appropriate governance brain based on mode
    #[cfg(feature = "all-governance")]
    pub async fn create() -> Box<dyn crate::governance::GovernanceBrainTrait> {
        match *GOVERNANCE_MODE {
            GovernanceMode::Sync => Box::new(crate::governance::GovernanceBrain::new()),
            GovernanceMode::Async | GovernanceMode::Auto => {
                Box::new(crate::governance::SharedGovernanceBrain::new().await)
            }
        }
    }

    /// Create with explicit mode override
    #[cfg(feature = "all-governance")]
    pub async fn create_with_mode(
        mode: GovernanceMode,
    ) -> Box<dyn crate::governance::GovernanceBrainTrait> {
        match mode {
            GovernanceMode::Sync => Box::new(crate::governance::GovernanceBrain::new()),
            GovernanceMode::Async | GovernanceMode::Auto => {
                Box::new(crate::governance::SharedGovernanceBrain::new().await)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_governance_mode_from_env() {
        env::set_var("GRAPHICA_GOVERNANCE_MODE", "async");
        assert_eq!(GovernanceMode::from_env(), GovernanceMode::Async);

        env::set_var("GRAPHICA_GOVERNANCE_MODE", "sync");
        assert_eq!(GovernanceMode::from_env(), GovernanceMode::Sync);

        env::set_var("GRAPHICA_GOVERNANCE_MODE", "auto");
        assert_eq!(GovernanceMode::from_env(), GovernanceMode::Auto);

        env::remove_var("GRAPHICA_GOVERNANCE_MODE");
        // Should use default
    }

    #[test]
    fn test_migration_config() {
        env::set_var("GRAPHICA_ASYNC_PERCENTAGE", "50");
        env::set_var("GRAPHICA_AB_METRICS", "true");

        let config = MigrationConfig::from_env();
        assert_eq!(config.async_percentage, 50);
        assert!(config.enable_ab_metrics);

        env::remove_var("GRAPHICA_ASYNC_PERCENTAGE");
        env::remove_var("GRAPHICA_AB_METRICS");
    }

    #[test]
    fn test_should_use_async_percentage() {
        let config = MigrationConfig {
            async_percentage: 100,
            ..Default::default()
        };
        assert!(config.should_use_async());

        let config = MigrationConfig {
            async_percentage: 0,
            ..Default::default()
        };
        assert!(!config.should_use_async());

        // Test 50% - should be roughly half true/false over many runs
        let config = MigrationConfig {
            async_percentage: 50,
            ..Default::default()
        };

        let mut async_count = 0;
        for _ in 0..1000 {
            if config.should_use_async() {
                async_count += 1;
            }
        }

        // Should be roughly 500, allow 40-60% range
        assert!(async_count > 400 && async_count < 600);
    }
}
