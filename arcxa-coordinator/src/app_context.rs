//! Application Context
//!
//! Central infrastructure for all components (HTTP, background, workers).
//!
//! ## Purpose
//!
//! `AppContext` provides a unified way to access application-wide infrastructure:
//! - Observability (metrics, tracing)
//! - Configuration
//! - Shared resources
//!
//! Unlike `ApiState` (which is HTTP-specific), `AppContext` is designed to be
//! passed to ALL components regardless of their layer (HTTP, background, workers).
//!
//! ## Architecture
//!
//! ```text
//! main.rs:
//!   1. Initialize AppContext (metrics, tracing, config)
//!   2. Pass context to QueryExecutor, WorkflowEngine, etc.
//!   3. Create ApiState (HTTP-specific state)
//!   4. ApiState contains reference to metrics for middleware
//! ```
//!
//! ## Usage
//!
//! ```ignore
//! use graphica_coordinator::app_context::AppContext;
//!
//! // In main.rs
//! let context = AppContext::new("production".to_string())?;
//!
//! // Pass to background components
//! let executor = QueryExecutor::new(router, pool, context.clone());
//!
//! // Use in component
//! if let Some(metrics) = context.metrics() {
//!     metrics.shard.record_request(0, "scatter", 0.5);
//! }
//! ```

use anyhow::Result;
use std::sync::Arc;

use crate::observability::MetricsRegistry;

/// Application-wide context available to all components
///
/// This is the single source of truth for infrastructure concerns.
/// All components (HTTP handlers, background workers, coordinators) receive this.
///
/// ## Design Principles
///
/// - **Explicit dependencies**: No global state, context is passed explicitly
/// - **Graceful degradation**: All infrastructure is `Option<>`, system works without it
/// - **Cloneable**: Uses `Arc` internally for cheap clones
/// - **Testable**: `minimal()` constructor for tests without infrastructure
#[derive(Clone)]
pub struct AppContext {
    /// Metrics registry (optional - graceful degradation)
    pub metrics: Option<Arc<MetricsRegistry>>,

    /// Application name and version
    pub app_name: String,
    pub app_version: String,

    /// Environment (dev, staging, prod)
    pub environment: String,
    // Future extensions:
    // pub tracer: Option<Arc<dyn Tracer>>,  // OpenTelemetry distributed tracing
    // pub feature_flags: Option<Arc<FeatureFlags>>,
    // pub config: Option<Arc<Config>>,
}

impl AppContext {
    /// Create new application context with full observability
    ///
    /// Initializes:
    /// - Prometheus metrics registry
    /// - Application metadata (name, version, environment)
    ///
    /// # Arguments
    ///
    /// * `environment` - Environment name (dev, staging, production)
    ///
    /// # Errors
    ///
    /// Returns error if metrics initialization fails.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use graphica_coordinator::app_context::AppContext;
    ///
    /// # fn main() -> anyhow::Result<()> {
    /// let context = AppContext::new("production".to_string())?;
    /// assert!(context.metrics().is_some());
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(environment: String) -> Result<Self> {
        let metrics = Some(Arc::new(crate::observability::initialize()?));

        tracing::info!(
            "Application context initialized (environment: {}, version: {})",
            environment,
            env!("CARGO_PKG_VERSION")
        );

        Ok(Self {
            metrics,
            app_name: "arcxa-coordinator".to_string(),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            environment,
        })
    }

    /// Create minimal context without observability
    ///
    /// Used for:
    /// - Unit tests
    /// - Development without metrics overhead
    /// - Embedded scenarios
    ///
    /// # Examples
    ///
    /// ```
    /// use graphica_coordinator::app_context::AppContext;
    ///
    /// let context = AppContext::minimal();
    /// assert!(context.metrics().is_none());
    /// assert_eq!(context.environment, "test");
    /// ```
    pub fn minimal() -> Self {
        Self {
            metrics: None,
            app_name: "arcxa-coordinator".to_string(),
            app_version: "test".to_string(),
            environment: "test".to_string(),
        }
    }

    /// Create context with custom metrics registry
    ///
    /// Used for:
    /// - Testing with mock metrics
    /// - Custom observability pipelines
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use graphica_coordinator::app_context::AppContext;
    /// use graphica_coordinator::observability::MetricsRegistry;
    /// use std::sync::Arc;
    ///
    /// # fn main() -> anyhow::Result<()> {
    /// let custom_metrics = Arc::new(MetricsRegistry::new()?);
    /// let context = AppContext::with_metrics(
    ///     "test".to_string(),
    ///     Some(custom_metrics)
    /// );
    /// # Ok(())
    /// # }
    /// ```
    pub fn with_metrics(environment: String, metrics: Option<Arc<MetricsRegistry>>) -> Self {
        Self {
            metrics,
            app_name: "arcxa-coordinator".to_string(),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            environment,
        }
    }

    /// Get metrics registry with graceful fallback
    ///
    /// Returns `None` if metrics are not initialized (e.g., in test mode).
    /// Components should handle `None` gracefully.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use graphica_coordinator::app_context::AppContext;
    /// # fn example(context: &AppContext) {
    /// if let Some(metrics) = context.metrics() {
    ///     metrics.shard.record_request(0, "query", 0.5);
    /// }
    /// // System continues to work without metrics
    /// # }
    /// ```
    pub fn metrics(&self) -> Option<&MetricsRegistry> {
        self.metrics.as_ref().map(|m| m.as_ref())
    }

    /// Check if metrics are enabled
    ///
    /// Useful for conditional expensive metric collection.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use graphica_coordinator::app_context::AppContext;
    /// # fn example(context: &AppContext) {
    /// if context.has_metrics() {
    ///     // Only compute expensive metrics if collection is enabled
    ///     let expensive_data = compute_expensive_stats();
    ///     if let Some(metrics) = context.metrics() {
    ///         // record metrics...
    ///     }
    /// }
    /// # }
    /// # fn compute_expensive_stats() -> u64 { 42 }
    /// ```
    pub fn has_metrics(&self) -> bool {
        self.metrics.is_some()
    }

    /// Get application version string
    pub fn version(&self) -> &str {
        &self.app_version
    }

    /// Get environment name
    pub fn env(&self) -> &str {
        &self.environment
    }

    /// Check if running in production environment
    pub fn is_production(&self) -> bool {
        self.environment == "production" || self.environment == "prod"
    }

    /// Check if running in test/development environment
    pub fn is_development(&self) -> bool {
        self.environment == "test" || self.environment == "dev" || self.environment == "development"
    }
}

impl Default for AppContext {
    /// Create minimal context by default (for tests)
    fn default() -> Self {
        Self::minimal()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_minimal_context_has_no_metrics() {
        let ctx = AppContext::minimal();
        assert!(ctx.metrics().is_none());
        assert!(!ctx.has_metrics());
        assert_eq!(ctx.environment, "test");
    }

    #[test]
    fn test_minimal_context_is_default() {
        let ctx1 = AppContext::minimal();
        let ctx2 = AppContext::default();

        assert_eq!(ctx1.environment, ctx2.environment);
        assert_eq!(ctx1.app_name, ctx2.app_name);
    }

    #[test]
    fn test_context_clone_is_cheap() {
        let ctx = AppContext::minimal();
        let cloned = ctx.clone();

        assert_eq!(ctx.app_name, cloned.app_name);
        assert_eq!(ctx.environment, cloned.environment);
    }

    #[test]
    fn test_environment_checks() {
        let prod = AppContext::with_metrics("production".to_string(), None);
        assert!(prod.is_production());
        assert!(!prod.is_development());

        let dev = AppContext::with_metrics("dev".to_string(), None);
        assert!(!dev.is_production());
        assert!(dev.is_development());

        let test = AppContext::minimal();
        assert!(!test.is_production());
        assert!(test.is_development());
    }

    #[test]
    fn test_version_accessor() {
        let ctx = AppContext::minimal();
        assert!(!ctx.version().is_empty());
    }

    #[test]
    fn test_with_custom_metrics() {
        // Can create context with custom (None) metrics
        let ctx = AppContext::with_metrics("test".to_string(), None);
        assert!(ctx.metrics().is_none());
    }
}
