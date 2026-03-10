//! Model health monitoring
//!
//! Tracks health status, response times, and error rates for registered models.

use anyhow::{Context, Result};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Health status for a model
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    /// Model is healthy and responding normally
    Healthy,
    /// Model is degraded but still functional
    Degraded,
    /// Model is unhealthy and should not be used
    Unhealthy,
    /// Health status unknown (newly registered)
    Unknown,
}

/// Health metrics for a model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthMetrics {
    /// Current health status
    pub status: HealthStatus,
    /// Total number of requests
    pub total_requests: u64,
    /// Number of successful requests
    pub successful_requests: u64,
    /// Number of failed requests
    pub failed_requests: u64,
    /// Current error rate (0.0 to 1.0)
    pub error_rate: f64,
    /// Average response time in milliseconds
    pub avg_response_time_ms: f64,
    /// P95 response time in milliseconds
    pub p95_response_time_ms: f64,
    /// P99 response time in milliseconds
    pub p99_response_time_ms: f64,
    /// Last successful request timestamp (not serialized)
    #[serde(skip)]
    pub last_success: Option<Instant>,
    /// Last failed request timestamp (not serialized)
    #[serde(skip)]
    pub last_failure: Option<Instant>,
    /// Consecutive failures
    pub consecutive_failures: u32,
}

impl Default for HealthMetrics {
    fn default() -> Self {
        Self {
            status: HealthStatus::Unknown,
            total_requests: 0,
            successful_requests: 0,
            failed_requests: 0,
            error_rate: 0.0,
            avg_response_time_ms: 0.0,
            p95_response_time_ms: 0.0,
            p99_response_time_ms: 0.0,
            last_success: None,
            last_failure: None,
            consecutive_failures: 0,
        }
    }
}

/// Configuration for health monitoring
#[derive(Debug, Clone)]
pub struct HealthConfig {
    /// Window size for calculating metrics (number of requests)
    pub window_size: usize,
    /// Error rate threshold for degraded status (0.0 to 1.0)
    pub degraded_threshold: f64,
    /// Error rate threshold for unhealthy status (0.0 to 1.0)
    pub unhealthy_threshold: f64,
    /// Consecutive failures before marking unhealthy
    pub failure_threshold: u32,
    /// Response time threshold for degraded status (milliseconds)
    pub slow_response_threshold_ms: f64,
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            window_size: 100,
            degraded_threshold: 0.1,  // 10% error rate
            unhealthy_threshold: 0.5, // 50% error rate
            failure_threshold: 5,
            slow_response_threshold_ms: 5000.0, // 5 seconds
        }
    }
}

/// Health monitor for tracking model health
pub struct HealthMonitor {
    config: HealthConfig,
    metrics: Arc<RwLock<HashMap<String, ModelHealth>>>,
}

/// Internal health tracking for a model
struct ModelHealth {
    metrics: HealthMetrics,
    response_times: Vec<f64>,
    recent_results: Vec<bool>, // true = success, false = failure
}

impl ModelHealth {
    fn new(window_size: usize) -> Self {
        Self {
            metrics: HealthMetrics::default(),
            response_times: Vec::with_capacity(window_size),
            recent_results: Vec::with_capacity(window_size),
        }
    }

    fn record_success(&mut self, response_time_ms: f64, window_size: usize) {
        self.metrics.total_requests += 1;
        self.metrics.successful_requests += 1;
        self.metrics.last_success = Some(Instant::now());
        self.metrics.consecutive_failures = 0;

        // Add to sliding window
        if self.response_times.len() >= window_size {
            self.response_times.remove(0);
        }
        self.response_times.push(response_time_ms);

        if self.recent_results.len() >= window_size {
            self.recent_results.remove(0);
        }
        self.recent_results.push(true);

        self.update_metrics();
    }

    fn record_failure(&mut self, response_time_ms: Option<f64>, window_size: usize) {
        self.metrics.total_requests += 1;
        self.metrics.failed_requests += 1;
        self.metrics.last_failure = Some(Instant::now());
        self.metrics.consecutive_failures += 1;

        // Record response time if available
        if let Some(time) = response_time_ms {
            if self.response_times.len() >= window_size {
                self.response_times.remove(0);
            }
            self.response_times.push(time);
        }

        if self.recent_results.len() >= window_size {
            self.recent_results.remove(0);
        }
        self.recent_results.push(false);

        self.update_metrics();
    }

    fn update_metrics(&mut self) {
        // Calculate error rate
        let total = self.recent_results.len() as f64;
        if total > 0.0 {
            let failures = self.recent_results.iter().filter(|&&r| !r).count() as f64;
            self.metrics.error_rate = failures / total;
        }

        // Calculate response time metrics
        if !self.response_times.is_empty() {
            let mut sorted = self.response_times.clone();
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

            // Average
            let sum: f64 = sorted.iter().sum();
            self.metrics.avg_response_time_ms = sum / sorted.len() as f64;

            // P95
            let p95_index = ((sorted.len() as f64) * 0.95) as usize;
            self.metrics.p95_response_time_ms = sorted.get(p95_index).copied().unwrap_or(0.0);

            // P99
            let p99_index = ((sorted.len() as f64) * 0.99) as usize;
            self.metrics.p99_response_time_ms = sorted.get(p99_index).copied().unwrap_or(0.0);
        }
    }
}

impl HealthMonitor {
    /// Create new health monitor
    pub fn new(config: HealthConfig) -> Self {
        Self {
            config,
            metrics: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create with default configuration
    pub fn with_default_config() -> Self {
        Self::new(HealthConfig::default())
    }

    /// Record successful request
    pub fn record_success(&self, model_id: &str, response_time: Duration) {
        let response_time_ms = response_time.as_secs_f64() * 1000.0;

        let mut metrics = self.metrics.write();
        let health = metrics
            .entry(model_id.to_string())
            .or_insert_with(|| ModelHealth::new(self.config.window_size));

        health.record_success(response_time_ms, self.config.window_size);
        self.update_health_status(health);
    }

    /// Record failed request
    pub fn record_failure(&self, model_id: &str, response_time: Option<Duration>) {
        let response_time_ms = response_time.map(|d| d.as_secs_f64() * 1000.0);

        let mut metrics = self.metrics.write();
        let health = metrics
            .entry(model_id.to_string())
            .or_insert_with(|| ModelHealth::new(self.config.window_size));

        health.record_failure(response_time_ms, self.config.window_size);
        self.update_health_status(health);
    }

    /// Get health metrics for a model
    pub fn get_health(&self, model_id: &str) -> Option<HealthMetrics> {
        let metrics = self.metrics.read();
        metrics.get(model_id).map(|h| h.metrics.clone())
    }

    /// Get health status for a model
    pub fn get_status(&self, model_id: &str) -> HealthStatus {
        let metrics = self.metrics.read();
        metrics
            .get(model_id)
            .map(|h| h.metrics.status.clone())
            .unwrap_or(HealthStatus::Unknown)
    }

    /// List all model health metrics
    pub fn list_all(&self) -> HashMap<String, HealthMetrics> {
        let metrics = self.metrics.read();
        metrics
            .iter()
            .map(|(id, health)| (id.clone(), health.metrics.clone()))
            .collect()
    }

    /// Check if model is healthy
    pub fn is_healthy(&self, model_id: &str) -> bool {
        matches!(
            self.get_status(model_id),
            HealthStatus::Healthy | HealthStatus::Unknown
        )
    }

    /// Update health status based on metrics
    fn update_health_status(&self, health: &mut ModelHealth) {
        let metrics = &mut health.metrics;

        // Check consecutive failures first
        if metrics.consecutive_failures >= self.config.failure_threshold {
            metrics.status = HealthStatus::Unhealthy;
            return;
        }

        // Check error rate
        if metrics.error_rate >= self.config.unhealthy_threshold {
            metrics.status = HealthStatus::Unhealthy;
        } else if metrics.error_rate >= self.config.degraded_threshold {
            metrics.status = HealthStatus::Degraded;
        } else if metrics.avg_response_time_ms >= self.config.slow_response_threshold_ms {
            // Slow but not failing
            metrics.status = HealthStatus::Degraded;
        } else if metrics.total_requests > 0 {
            // Has requests and passes all checks
            metrics.status = HealthStatus::Healthy;
        } else {
            metrics.status = HealthStatus::Unknown;
        }
    }

    /// Reset health metrics for a model
    pub fn reset(&self, model_id: &str) {
        let mut metrics = self.metrics.write();
        metrics.remove(model_id);
    }

    /// Reset all health metrics
    pub fn reset_all(&self) {
        let mut metrics = self.metrics.write();
        metrics.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_monitor_success() {
        let monitor = HealthMonitor::with_default_config();

        // Record successful requests
        for _ in 0..10 {
            monitor.record_success("test_model", Duration::from_millis(100));
        }

        let health = monitor.get_health("test_model").unwrap();
        assert_eq!(health.status, HealthStatus::Healthy);
        assert_eq!(health.total_requests, 10);
        assert_eq!(health.successful_requests, 10);
        assert_eq!(health.error_rate, 0.0);
    }

    #[test]
    fn test_health_monitor_degraded() {
        let monitor = HealthMonitor::with_default_config();

        // Record mixed results - 15% failure rate (above 10% degraded threshold)
        // Using window size of 100 (default)
        for _ in 0..85 {
            monitor.record_success("test_model", Duration::from_millis(100));
        }
        for _ in 0..15 {
            monitor.record_failure("test_model", Some(Duration::from_millis(100)));
        }

        let health = monitor.get_health("test_model").unwrap();
        // Within the sliding window of last 100 requests, we have 15 failures
        assert!(matches!(
            health.status,
            HealthStatus::Degraded | HealthStatus::Unhealthy
        ));
        assert_eq!(health.total_requests, 100);
        assert!(health.error_rate >= 0.1);
    }

    #[test]
    fn test_health_monitor_unhealthy() {
        let monitor = HealthMonitor::with_default_config();

        // Record mostly failures - 60% failure rate
        for _ in 0..40 {
            monitor.record_success("test_model", Duration::from_millis(100));
        }
        for _ in 0..60 {
            monitor.record_failure("test_model", Some(Duration::from_millis(100)));
        }

        let health = monitor.get_health("test_model").unwrap();
        assert_eq!(health.status, HealthStatus::Unhealthy);
        assert!(health.error_rate >= 0.5);
    }

    #[test]
    fn test_consecutive_failures() {
        let config = HealthConfig {
            failure_threshold: 3,
            ..Default::default()
        };
        let monitor = HealthMonitor::new(config);

        // Record 3 consecutive failures
        for _ in 0..3 {
            monitor.record_failure("test_model", None);
        }

        let health = monitor.get_health("test_model").unwrap();
        assert_eq!(health.status, HealthStatus::Unhealthy);
        assert_eq!(health.consecutive_failures, 3);
    }

    #[test]
    fn test_response_time_tracking() {
        let monitor = HealthMonitor::with_default_config();

        // Record various response times
        monitor.record_success("test_model", Duration::from_millis(100));
        monitor.record_success("test_model", Duration::from_millis(200));
        monitor.record_success("test_model", Duration::from_millis(300));
        monitor.record_success("test_model", Duration::from_millis(400));
        monitor.record_success("test_model", Duration::from_millis(500));

        let health = monitor.get_health("test_model").unwrap();
        assert_eq!(health.avg_response_time_ms, 300.0);
        assert!(health.p95_response_time_ms >= 400.0);
        assert!(health.p99_response_time_ms >= 400.0);
    }

    #[test]
    fn test_slow_response_degraded() {
        let config = HealthConfig {
            slow_response_threshold_ms: 1000.0,
            ..Default::default()
        };
        let monitor = HealthMonitor::new(config);

        // Record slow but successful requests
        for _ in 0..10 {
            monitor.record_success("test_model", Duration::from_millis(1500));
        }

        let health = monitor.get_health("test_model").unwrap();
        assert_eq!(health.status, HealthStatus::Degraded);
        assert!(health.avg_response_time_ms >= 1000.0);
    }
}
