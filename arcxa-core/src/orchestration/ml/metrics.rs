//! Prometheus metrics for ML model invocation
//!
//! Tracks invocation latency, success/failure rates, cache hits, and circuit breaker events.

use once_cell::sync::Lazy;
use prometheus::{
    opts, register_counter_vec, register_histogram_vec, register_int_counter_vec,
    register_int_gauge_vec, CounterVec, HistogramVec, IntCounterVec, IntGaugeVec, Registry,
};

/// ML invocation metrics
pub struct MlMetrics {
    /// Total number of model invocations
    pub invocations_total: IntCounterVec,
    /// Successful invocations
    pub invocations_success: IntCounterVec,
    /// Failed invocations
    pub invocations_failed: IntCounterVec,
    /// Invocation latency histogram (in seconds)
    pub invocation_duration_seconds: HistogramVec,
    /// Cache hits
    pub cache_hits_total: IntCounterVec,
    /// Cache misses
    pub cache_misses_total: IntCounterVec,
    /// Circuit breaker state (0=closed, 1=half-open, 2=open)
    pub circuit_breaker_state: IntGaugeVec,
    /// Circuit breaker trips
    pub circuit_breaker_trips_total: IntCounterVec,
    /// Retry attempts
    pub retry_attempts_total: IntCounterVec,
    /// Batch prediction requests
    pub batch_requests_total: CounterVec,
    /// Batch prediction items
    pub batch_items_total: CounterVec,
}

impl MlMetrics {
    /// Create new metrics instance
    pub fn new() -> Self {
        Self {
            invocations_total: register_int_counter_vec!(
                opts!(
                    "ml_invocations_total",
                    "Total number of ML model invocations"
                ),
                &["model_id", "protocol"]
            )
            .unwrap(),

            invocations_success: register_int_counter_vec!(
                opts!(
                    "ml_invocations_success_total",
                    "Number of successful ML model invocations"
                ),
                &["model_id", "protocol"]
            )
            .unwrap(),

            invocations_failed: register_int_counter_vec!(
                opts!(
                    "ml_invocations_failed_total",
                    "Number of failed ML model invocations"
                ),
                &["model_id", "protocol", "error_type"]
            )
            .unwrap(),

            invocation_duration_seconds: register_histogram_vec!(
                "ml_invocation_duration_seconds",
                "ML model invocation latency in seconds",
                &["model_id", "protocol"],
                vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]
            )
            .unwrap(),

            cache_hits_total: register_int_counter_vec!(
                opts!("ml_cache_hits_total", "Number of ML cache hits"),
                &["model_id"]
            )
            .unwrap(),

            cache_misses_total: register_int_counter_vec!(
                opts!("ml_cache_misses_total", "Number of ML cache misses"),
                &["model_id"]
            )
            .unwrap(),

            circuit_breaker_state: register_int_gauge_vec!(
                opts!(
                    "ml_circuit_breaker_state",
                    "Circuit breaker state: 0=closed, 1=half-open, 2=open"
                ),
                &["model_id"]
            )
            .unwrap(),

            circuit_breaker_trips_total: register_int_counter_vec!(
                opts!(
                    "ml_circuit_breaker_trips_total",
                    "Number of circuit breaker trips"
                ),
                &["model_id"]
            )
            .unwrap(),

            retry_attempts_total: register_int_counter_vec!(
                opts!(
                    "ml_retry_attempts_total",
                    "Number of retry attempts for failed invocations"
                ),
                &["model_id", "attempt"]
            )
            .unwrap(),

            batch_requests_total: register_counter_vec!(
                opts!(
                    "ml_batch_requests_total",
                    "Number of batch prediction requests"
                ),
                &["model_id"]
            )
            .unwrap(),

            batch_items_total: register_counter_vec!(
                opts!(
                    "ml_batch_items_total",
                    "Total number of items in batch predictions"
                ),
                &["model_id"]
            )
            .unwrap(),
        }
    }

    /// Register metrics with a custom registry
    pub fn register_with(registry: &Registry) -> Result<Self, prometheus::Error> {
        let invocations_total = IntCounterVec::new(
            opts!(
                "ml_invocations_total",
                "Total number of ML model invocations"
            ),
            &["model_id", "protocol"],
        )?;
        registry.register(Box::new(invocations_total.clone()))?;

        let invocations_success = IntCounterVec::new(
            opts!(
                "ml_invocations_success_total",
                "Number of successful ML model invocations"
            ),
            &["model_id", "protocol"],
        )?;
        registry.register(Box::new(invocations_success.clone()))?;

        let invocations_failed = IntCounterVec::new(
            opts!(
                "ml_invocations_failed_total",
                "Number of failed ML model invocations"
            ),
            &["model_id", "protocol", "error_type"],
        )?;
        registry.register(Box::new(invocations_failed.clone()))?;

        let invocation_duration_seconds = HistogramVec::new(
            prometheus::HistogramOpts::new(
                "ml_invocation_duration_seconds",
                "ML model invocation latency in seconds",
            )
            .buckets(vec![
                0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
            ]),
            &["model_id", "protocol"],
        )?;
        registry.register(Box::new(invocation_duration_seconds.clone()))?;

        let cache_hits_total = IntCounterVec::new(
            opts!("ml_cache_hits_total", "Number of ML cache hits"),
            &["model_id"],
        )?;
        registry.register(Box::new(cache_hits_total.clone()))?;

        let cache_misses_total = IntCounterVec::new(
            opts!("ml_cache_misses_total", "Number of ML cache misses"),
            &["model_id"],
        )?;
        registry.register(Box::new(cache_misses_total.clone()))?;

        let circuit_breaker_state = IntGaugeVec::new(
            opts!(
                "ml_circuit_breaker_state",
                "Circuit breaker state: 0=closed, 1=half-open, 2=open"
            ),
            &["model_id"],
        )?;
        registry.register(Box::new(circuit_breaker_state.clone()))?;

        let circuit_breaker_trips_total = IntCounterVec::new(
            opts!(
                "ml_circuit_breaker_trips_total",
                "Number of circuit breaker trips"
            ),
            &["model_id"],
        )?;
        registry.register(Box::new(circuit_breaker_trips_total.clone()))?;

        let retry_attempts_total = IntCounterVec::new(
            opts!(
                "ml_retry_attempts_total",
                "Number of retry attempts for failed invocations"
            ),
            &["model_id", "attempt"],
        )?;
        registry.register(Box::new(retry_attempts_total.clone()))?;

        let batch_requests_total = CounterVec::new(
            opts!(
                "ml_batch_requests_total",
                "Number of batch prediction requests"
            ),
            &["model_id"],
        )?;
        registry.register(Box::new(batch_requests_total.clone()))?;

        let batch_items_total = CounterVec::new(
            opts!(
                "ml_batch_items_total",
                "Total number of items in batch predictions"
            ),
            &["model_id"],
        )?;
        registry.register(Box::new(batch_items_total.clone()))?;

        Ok(Self {
            invocations_total,
            invocations_success,
            invocations_failed,
            invocation_duration_seconds,
            cache_hits_total,
            cache_misses_total,
            circuit_breaker_state,
            circuit_breaker_trips_total,
            retry_attempts_total,
            batch_requests_total,
            batch_items_total,
        })
    }
}

impl Default for MlMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Global metrics instance
pub static ML_METRICS: Lazy<MlMetrics> = Lazy::new(MlMetrics::new);

/// Helper functions for recording metrics
impl MlMetrics {
    /// Record invocation start
    pub fn record_invocation_start(&self, model_id: &str, protocol: &str) {
        self.invocations_total
            .with_label_values(&[model_id, protocol])
            .inc();
    }

    /// Record successful invocation
    pub fn record_invocation_success(&self, model_id: &str, protocol: &str, duration_seconds: f64) {
        self.invocations_success
            .with_label_values(&[model_id, protocol])
            .inc();

        self.invocation_duration_seconds
            .with_label_values(&[model_id, protocol])
            .observe(duration_seconds);
    }

    /// Record failed invocation
    pub fn record_invocation_failure(&self, model_id: &str, protocol: &str, error_type: &str) {
        self.invocations_failed
            .with_label_values(&[model_id, protocol, error_type])
            .inc();
    }

    /// Record cache hit
    pub fn record_cache_hit(&self, model_id: &str) {
        self.cache_hits_total.with_label_values(&[model_id]).inc();
    }

    /// Record cache miss
    pub fn record_cache_miss(&self, model_id: &str) {
        self.cache_misses_total.with_label_values(&[model_id]).inc();
    }

    /// Set circuit breaker state
    pub fn set_circuit_breaker_state(&self, model_id: &str, state: i64) {
        self.circuit_breaker_state
            .with_label_values(&[model_id])
            .set(state);
    }

    /// Record circuit breaker trip
    pub fn record_circuit_breaker_trip(&self, model_id: &str) {
        self.circuit_breaker_trips_total
            .with_label_values(&[model_id])
            .inc();
    }

    /// Record retry attempt
    pub fn record_retry_attempt(&self, model_id: &str, attempt: u32) {
        self.retry_attempts_total
            .with_label_values(&[model_id, &attempt.to_string()])
            .inc();
    }

    /// Record batch request
    pub fn record_batch_request(&self, model_id: &str, item_count: usize) {
        self.batch_requests_total
            .with_label_values(&[model_id])
            .inc();

        self.batch_items_total
            .with_label_values(&[model_id])
            .inc_by(item_count as f64);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_creation() {
        // Use custom registry to avoid conflicts with global registry
        let registry = Registry::new();
        let metrics = MlMetrics::register_with(&registry).unwrap();

        // Verify metrics can be accessed
        metrics.record_invocation_start("test_model", "http");
        metrics.record_invocation_success("test_model", "http", 0.5);
        metrics.record_cache_hit("test_model");
    }

    #[test]
    fn test_custom_registry() {
        let registry = Registry::new();
        let metrics = MlMetrics::register_with(&registry).unwrap();

        metrics.record_invocation_start("test_model", "grpc");
        metrics.record_invocation_failure("test_model", "grpc", "timeout");

        // Verify metrics are registered
        let metric_families = registry.gather();
        assert!(!metric_families.is_empty());
    }

    #[test]
    fn test_batch_metrics() {
        let registry = Registry::new();
        let metrics = MlMetrics::register_with(&registry).unwrap();

        metrics.record_batch_request("batch_model", 100);
        metrics.record_batch_request("batch_model", 50);

        // Metrics should be incremented
        // In real usage, we'd query Prometheus to verify values
    }

    #[test]
    fn test_circuit_breaker_metrics() {
        let registry = Registry::new();
        let metrics = MlMetrics::register_with(&registry).unwrap();

        // Set to closed (0)
        metrics.set_circuit_breaker_state("test_model", 0);

        // Record a trip
        metrics.record_circuit_breaker_trip("test_model");

        // Set to open (2)
        metrics.set_circuit_breaker_state("test_model", 2);
    }

    #[test]
    fn test_retry_metrics() {
        let registry = Registry::new();
        let metrics = MlMetrics::register_with(&registry).unwrap();

        metrics.record_retry_attempt("retry_model", 1);
        metrics.record_retry_attempt("retry_model", 2);
        metrics.record_retry_attempt("retry_model", 3);
    }

    #[test]
    fn test_global_metrics() {
        // Test global metrics instance (uses default global registry)
        // These metrics persist across test runs, so we use unique model IDs
        let test_id = format!(
            "global_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        ML_METRICS.record_invocation_start(&test_id, "lambda");
        ML_METRICS.record_invocation_success(&test_id, "lambda", 1.5);
    }
}
