//! Feature flags for Kafka durability components
//!
//! Enables progressive rollout, A/B testing, and gradual migration from
//! legacy fire-and-forget to durable WAL-backed Kafka producer.
//!
//! # Features
//!
//! - **Percentage-based rollout**: Enable for N% of events
//! - **Tenant targeting**: Enable for specific tenants
//! - **Dataset targeting**: Enable for specific datasets
//! - **Emergency kill switch**: Instantly disable for all
//! - **Metrics integration**: Track usage per feature flag
//!
//! # Usage
//!
//! ```rust,no_run
//! use graphica_coordinator::storage::kafka::FeatureFlags;
//!
//! # fn main() -> anyhow::Result<()> {
//! let flags = FeatureFlags::from_env()?;
//!
//! // Check if durable writes are enabled for this tenant
//! if flags.is_durable_writes_enabled("tenant_123") {
//!     // Use DurableKafkaLineageSink
//! } else {
//!     // Use legacy KafkaLineageSink
//! }
//!
//! // Check rollout percentage for specific event
//! if flags.is_durable_writes_enabled_for_event("tenant_123", "dataset_1", "event_id_hash") {
//!     // Included in rollout percentage
//! }
//! # Ok(())
//! # }
//! ```

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use tracing::{debug, info};

/// Feature flag configuration for Kafka durability
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureFlags {
    /// Enable durable Kafka writes (vs legacy fire-and-forget)
    pub durable_writes: FeatureFlagConfig,

    /// Enable circuit breaker
    pub circuit_breaker: FeatureFlagConfig,

    /// Enable startup recovery/replay
    pub startup_recovery: FeatureFlagConfig,

    /// Enable acknowledgment tracking
    pub acknowledgment_tracking: FeatureFlagConfig,

    /// Enable Prometheus metrics
    pub metrics: FeatureFlagConfig,

    /// Enable distributed tracing
    pub tracing: FeatureFlagConfig,

    /// Custom feature flags (extensible)
    #[serde(default)]
    pub custom: HashMap<String, FeatureFlagConfig>,
}

/// Configuration for a single feature flag
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureFlagConfig {
    /// Feature enabled globally
    pub enabled: bool,

    /// Rollout percentage (0-100)
    /// Events are hashed and compared against this percentage
    pub rollout_percentage: u8,

    /// Specific tenants to enable (allowlist)
    #[serde(default)]
    pub enabled_tenants: HashSet<String>,

    /// Specific tenants to disable (denylist, takes precedence)
    #[serde(default)]
    pub disabled_tenants: HashSet<String>,

    /// Specific datasets to enable (allowlist)
    #[serde(default)]
    pub enabled_datasets: HashSet<String>,

    /// Specific datasets to disable (denylist, takes precedence)
    #[serde(default)]
    pub disabled_datasets: HashSet<String>,

    /// Emergency kill switch (overrides everything)
    pub emergency_disable: bool,
}

impl Default for FeatureFlagConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            rollout_percentage: 100,
            enabled_tenants: HashSet::new(),
            disabled_tenants: HashSet::new(),
            enabled_datasets: HashSet::new(),
            disabled_datasets: HashSet::new(),
            emergency_disable: false,
        }
    }
}

impl Default for FeatureFlags {
    fn default() -> Self {
        Self {
            durable_writes: FeatureFlagConfig {
                enabled: true,
                rollout_percentage: 100,
                ..Default::default()
            },
            circuit_breaker: FeatureFlagConfig::default(),
            startup_recovery: FeatureFlagConfig::default(),
            acknowledgment_tracking: FeatureFlagConfig::default(),
            metrics: FeatureFlagConfig::default(),
            tracing: FeatureFlagConfig::default(),
            custom: HashMap::new(),
        }
    }
}

impl FeatureFlags {
    /// Create feature flags from environment variables
    ///
    /// Environment variables:
    /// - KAFKA_FEATURE_DURABLE_WRITES_ENABLED=true/false
    /// - KAFKA_FEATURE_DURABLE_WRITES_ROLLOUT_PCT=0-100
    /// - KAFKA_FEATURE_DURABLE_WRITES_ENABLED_TENANTS=tenant1,tenant2
    /// - KAFKA_FEATURE_DURABLE_WRITES_DISABLED_TENANTS=tenant3,tenant4
    /// - KAFKA_FEATURE_CIRCUIT_BREAKER_ENABLED=true/false
    /// - etc.
    pub fn from_env() -> Result<Self> {
        let mut flags = Self::default();

        // Durable writes
        flags.durable_writes = Self::load_feature_config("DURABLE_WRITES")?;

        // Circuit breaker
        flags.circuit_breaker = Self::load_feature_config("CIRCUIT_BREAKER")?;

        // Startup recovery
        flags.startup_recovery = Self::load_feature_config("STARTUP_RECOVERY")?;

        // Acknowledgment tracking
        flags.acknowledgment_tracking = Self::load_feature_config("ACKNOWLEDGMENT_TRACKING")?;

        // Metrics
        flags.metrics = Self::load_feature_config("METRICS")?;

        // Tracing
        flags.tracing = Self::load_feature_config("TRACING")?;

        info!("Kafka feature flags loaded from environment");
        debug!("Feature flags: {:?}", flags);

        Ok(flags)
    }

    /// Load feature config from environment variables
    fn load_feature_config(feature_name: &str) -> Result<FeatureFlagConfig> {
        let mut config = FeatureFlagConfig::default();

        let prefix = format!("KAFKA_FEATURE_{}", feature_name);

        // Enabled flag
        if let Ok(val) = std::env::var(format!("{}_ENABLED", prefix)) {
            config.enabled = val.parse().unwrap_or(true);
        }

        // Rollout percentage
        if let Ok(val) = std::env::var(format!("{}_ROLLOUT_PCT", prefix)) {
            config.rollout_percentage = val.parse().unwrap_or(100).min(100);
        }

        // Enabled tenants
        if let Ok(val) = std::env::var(format!("{}_ENABLED_TENANTS", prefix)) {
            config.enabled_tenants = val.split(',').map(|s| s.trim().to_string()).collect();
        }

        // Disabled tenants
        if let Ok(val) = std::env::var(format!("{}_DISABLED_TENANTS", prefix)) {
            config.disabled_tenants = val.split(',').map(|s| s.trim().to_string()).collect();
        }

        // Enabled datasets
        if let Ok(val) = std::env::var(format!("{}_ENABLED_DATASETS", prefix)) {
            config.enabled_datasets = val.split(',').map(|s| s.trim().to_string()).collect();
        }

        // Disabled datasets
        if let Ok(val) = std::env::var(format!("{}_DISABLED_DATASETS", prefix)) {
            config.disabled_datasets = val.split(',').map(|s| s.trim().to_string()).collect();
        }

        // Emergency kill switch
        if let Ok(val) = std::env::var(format!("{}_EMERGENCY_DISABLE", prefix)) {
            config.emergency_disable = val.parse().unwrap_or(false);
        }

        Ok(config)
    }

    /// Check if durable writes are enabled for a given context
    pub fn is_durable_writes_enabled(&self, tenant_id: &str) -> bool {
        self.evaluate_flag(&self.durable_writes, tenant_id, None, None)
    }

    /// Check if durable writes are enabled for a specific event
    pub fn is_durable_writes_enabled_for_event(
        &self,
        tenant_id: &str,
        dataset: &str,
        event_id: &str,
    ) -> bool {
        self.evaluate_flag(
            &self.durable_writes,
            tenant_id,
            Some(dataset),
            Some(event_id),
        )
    }

    /// Check if circuit breaker is enabled
    pub fn is_circuit_breaker_enabled(&self, tenant_id: &str) -> bool {
        self.evaluate_flag(&self.circuit_breaker, tenant_id, None, None)
    }

    /// Check if startup recovery is enabled
    pub fn is_startup_recovery_enabled(&self) -> bool {
        !self.startup_recovery.emergency_disable && self.startup_recovery.enabled
    }

    /// Check if acknowledgment tracking is enabled
    pub fn is_acknowledgment_tracking_enabled(&self, tenant_id: &str) -> bool {
        self.evaluate_flag(&self.acknowledgment_tracking, tenant_id, None, None)
    }

    /// Check if metrics are enabled
    pub fn is_metrics_enabled(&self) -> bool {
        !self.metrics.emergency_disable && self.metrics.enabled
    }

    /// Check if tracing is enabled
    pub fn is_tracing_enabled(&self) -> bool {
        !self.tracing.emergency_disable && self.tracing.enabled
    }

    /// Evaluate a feature flag for a specific context
    fn evaluate_flag(
        &self,
        config: &FeatureFlagConfig,
        tenant_id: &str,
        dataset: Option<&str>,
        event_id: Option<&str>,
    ) -> bool {
        // Emergency kill switch takes precedence
        if config.emergency_disable {
            return false;
        }

        // Check if globally disabled
        if !config.enabled {
            return false;
        }

        // Check tenant denylist (takes precedence over allowlist)
        if config.disabled_tenants.contains(tenant_id) {
            return false;
        }

        // Check dataset denylist
        if let Some(ds) = dataset {
            if config.disabled_datasets.contains(ds) {
                return false;
            }
        }

        // Check tenant allowlist (if non-empty, must be in list)
        if !config.enabled_tenants.is_empty() && !config.enabled_tenants.contains(tenant_id) {
            return false;
        }

        // Check dataset allowlist (if non-empty, must be in list)
        if let Some(ds) = dataset {
            if !config.enabled_datasets.is_empty() && !config.enabled_datasets.contains(ds) {
                return false;
            }
        }

        // Check rollout percentage (if event_id provided)
        if let Some(eid) = event_id {
            let hash = Self::hash_string(eid);
            let bucket = (hash % 100) as u8;
            if bucket >= config.rollout_percentage {
                return false;
            }
        } else if config.rollout_percentage < 100 {
            // If no event_id but rollout < 100%, use tenant_id for bucketing
            let hash = Self::hash_string(tenant_id);
            let bucket = (hash % 100) as u8;
            if bucket >= config.rollout_percentage {
                return false;
            }
        }

        true
    }

    /// Hash a string to a consistent u64 (for percentage bucketing)
    fn hash_string(s: &str) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        let mut hasher = DefaultHasher::new();
        s.hash(&mut hasher);
        hasher.finish()
    }

    /// Get rollout statistics
    pub fn get_rollout_stats(&self) -> RolloutStats {
        RolloutStats {
            durable_writes_pct: self.durable_writes.rollout_percentage,
            circuit_breaker_pct: self.circuit_breaker.rollout_percentage,
            startup_recovery_enabled: self.is_startup_recovery_enabled(),
            metrics_enabled: self.is_metrics_enabled(),
            tracing_enabled: self.is_tracing_enabled(),
        }
    }
}

/// Rollout statistics for monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RolloutStats {
    pub durable_writes_pct: u8,
    pub circuit_breaker_pct: u8,
    pub startup_recovery_enabled: bool,
    pub metrics_enabled: bool,
    pub tracing_enabled: bool,
}

/// Thread-safe feature flag manager
#[derive(Clone)]
pub struct FeatureFlagManager {
    flags: Arc<parking_lot::RwLock<FeatureFlags>>,
}

impl FeatureFlagManager {
    /// Create new manager with default flags
    pub fn new() -> Self {
        Self {
            flags: Arc::new(parking_lot::RwLock::new(FeatureFlags::default())),
        }
    }

    /// Create manager from environment
    pub fn from_env() -> Result<Self> {
        let flags = FeatureFlags::from_env()?;
        Ok(Self {
            flags: Arc::new(parking_lot::RwLock::new(flags)),
        })
    }

    /// Update flags (for runtime updates)
    pub fn update_flags(&self, flags: FeatureFlags) {
        *self.flags.write() = flags;
        info!("Feature flags updated");
    }

    /// Get current flags (read-only)
    pub fn get_flags(&self) -> FeatureFlags {
        self.flags.read().clone()
    }

    /// Check if durable writes are enabled
    pub fn is_durable_writes_enabled(&self, tenant_id: &str) -> bool {
        self.flags.read().is_durable_writes_enabled(tenant_id)
    }

    /// Check if durable writes are enabled for event
    pub fn is_durable_writes_enabled_for_event(
        &self,
        tenant_id: &str,
        dataset: &str,
        event_id: &str,
    ) -> bool {
        self.flags
            .read()
            .is_durable_writes_enabled_for_event(tenant_id, dataset, event_id)
    }

    /// Emergency disable all features
    pub fn emergency_disable_all(&self) {
        let mut flags = self.flags.write();
        flags.durable_writes.emergency_disable = true;
        flags.circuit_breaker.emergency_disable = true;
        flags.startup_recovery.emergency_disable = true;
        info!("EMERGENCY: All Kafka durability features disabled");
    }

    /// Emergency enable (clear emergency disable flags)
    pub fn emergency_enable_all(&self) {
        let mut flags = self.flags.write();
        flags.durable_writes.emergency_disable = false;
        flags.circuit_breaker.emergency_disable = false;
        flags.startup_recovery.emergency_disable = false;
        info!("Emergency disable flags cleared");
    }
}

impl Default for FeatureFlagManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_flags_all_enabled() {
        let flags = FeatureFlags::default();
        assert!(flags.is_durable_writes_enabled("test_tenant"));
        assert!(flags.is_circuit_breaker_enabled("test_tenant"));
        assert!(flags.is_startup_recovery_enabled());
    }

    #[test]
    fn test_tenant_allowlist() {
        let mut flags = FeatureFlags::default();
        flags
            .durable_writes
            .enabled_tenants
            .insert("tenant_a".to_string());
        flags
            .durable_writes
            .enabled_tenants
            .insert("tenant_b".to_string());

        assert!(flags.is_durable_writes_enabled("tenant_a"));
        assert!(flags.is_durable_writes_enabled("tenant_b"));
        assert!(!flags.is_durable_writes_enabled("tenant_c"));
    }

    #[test]
    fn test_tenant_denylist() {
        let mut flags = FeatureFlags::default();
        flags
            .durable_writes
            .disabled_tenants
            .insert("banned_tenant".to_string());

        assert!(flags.is_durable_writes_enabled("normal_tenant"));
        assert!(!flags.is_durable_writes_enabled("banned_tenant"));
    }

    #[test]
    fn test_denylist_overrides_allowlist() {
        let mut flags = FeatureFlags::default();
        flags
            .durable_writes
            .enabled_tenants
            .insert("tenant_a".to_string());
        flags
            .durable_writes
            .disabled_tenants
            .insert("tenant_a".to_string());

        // Denylist takes precedence
        assert!(!flags.is_durable_writes_enabled("tenant_a"));
    }

    #[test]
    fn test_rollout_percentage() {
        let mut flags = FeatureFlags::default();
        flags.durable_writes.rollout_percentage = 50;

        // Test with many event IDs to verify ~50% enabled
        let mut enabled_count = 0;
        for i in 0..1000 {
            let event_id = format!("event_{}", i);
            if flags.is_durable_writes_enabled_for_event("tenant", "dataset", &event_id) {
                enabled_count += 1;
            }
        }

        // Should be around 500 (±10% tolerance)
        assert!(enabled_count >= 450 && enabled_count <= 550);
    }

    #[test]
    fn test_emergency_disable() {
        let mut flags = FeatureFlags::default();
        flags.durable_writes.emergency_disable = true;

        assert!(!flags.is_durable_writes_enabled("test_tenant"));
        assert!(!flags.is_durable_writes_enabled_for_event("tenant", "dataset", "event_123"));
    }

    #[test]
    fn test_globally_disabled() {
        let mut flags = FeatureFlags::default();
        flags.durable_writes.enabled = false;

        assert!(!flags.is_durable_writes_enabled("test_tenant"));
    }

    #[test]
    fn test_dataset_targeting() {
        let mut flags = FeatureFlags::default();
        flags
            .durable_writes
            .enabled_datasets
            .insert("important_data".to_string());

        assert!(flags.is_durable_writes_enabled_for_event("tenant", "important_data", "event_1"));
        assert!(!flags.is_durable_writes_enabled_for_event("tenant", "other_data", "event_2"));
    }

    #[test]
    fn test_feature_flag_manager() {
        let manager = FeatureFlagManager::new();

        assert!(manager.is_durable_writes_enabled("tenant"));

        // Update flags
        let mut flags = FeatureFlags::default();
        flags.durable_writes.enabled = false;
        manager.update_flags(flags);

        assert!(!manager.is_durable_writes_enabled("tenant"));
    }

    #[test]
    fn test_emergency_disable_all() {
        let manager = FeatureFlagManager::new();

        assert!(manager.is_durable_writes_enabled("tenant"));

        manager.emergency_disable_all();

        assert!(!manager.is_durable_writes_enabled("tenant"));

        manager.emergency_enable_all();

        assert!(manager.is_durable_writes_enabled("tenant"));
    }

    #[test]
    fn test_rollout_stats() {
        let mut flags = FeatureFlags::default();
        flags.durable_writes.rollout_percentage = 75;
        flags.circuit_breaker.rollout_percentage = 50;

        let stats = flags.get_rollout_stats();

        assert_eq!(stats.durable_writes_pct, 75);
        assert_eq!(stats.circuit_breaker_pct, 50);
        assert!(stats.startup_recovery_enabled);
        assert!(stats.metrics_enabled);
        assert!(stats.tracing_enabled);
    }
}
