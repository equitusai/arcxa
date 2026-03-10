//! Health Monitoring with Circuit Breakers
//!
//! This module implements health monitoring for shards with circuit breaker
//! pattern to prevent cascading failures.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::time::interval;
use tracing::{debug, error, info, warn};

use crate::governance::distributed::{ShardId, ShardStatus};

/// Health state of a shard
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShardHealth {
    /// Shard is healthy and responding normally
    Healthy,
    /// Shard is experiencing degraded performance
    Degraded,
    /// Shard is unhealthy but may recover
    Unhealthy,
    /// Shard is completely down
    Down,
}

impl ShardHealth {
    /// Check if shard is available for routing
    pub fn is_available(&self) -> bool {
        matches!(self, ShardHealth::Healthy | ShardHealth::Degraded)
    }

    /// Check if shard should be probed
    pub fn should_probe(&self) -> bool {
        !matches!(self, ShardHealth::Down)
    }
}

/// Circuit breaker state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// Circuit is closed, requests are allowed
    Closed,
    /// Circuit is open, requests are blocked
    Open,
    /// Circuit is half-open, limited requests for testing
    HalfOpen,
}

/// Circuit breaker for a shard
#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    /// Current state
    state: CircuitState,
    /// Failure count
    failure_count: u32,
    /// Success count (in half-open state)
    success_count: u32,
    /// Last failure time
    last_failure: Option<Instant>,
    /// Time when circuit was opened
    opened_at: Option<Instant>,
    /// Configuration
    config: CircuitBreakerConfig,
}

/// Circuit breaker configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    /// Failure threshold to open circuit
    pub failure_threshold: u32,
    /// Success threshold to close circuit from half-open
    pub success_threshold: u32,
    /// Timeout before transitioning from open to half-open
    pub timeout: Duration,
    /// Reset timeout for failure count
    pub reset_timeout: Duration,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        CircuitBreakerConfig {
            failure_threshold: 5,
            success_threshold: 3,
            timeout: Duration::from_secs(30),
            reset_timeout: Duration::from_secs(60),
        }
    }
}

impl CircuitBreaker {
    /// Create a new circuit breaker
    pub fn new(config: CircuitBreakerConfig) -> Self {
        CircuitBreaker {
            state: CircuitState::Closed,
            failure_count: 0,
            success_count: 0,
            last_failure: None,
            opened_at: None,
            config,
        }
    }

    /// Record a success
    pub fn record_success(&mut self) {
        match self.state {
            CircuitState::HalfOpen => {
                self.success_count += 1;
                if self.success_count >= self.config.success_threshold {
                    self.close();
                }
            }
            CircuitState::Closed => {
                // Reset failure count on success in closed state
                if self.failure_count > 0 {
                    if let Some(last_failure) = self.last_failure {
                        if last_failure.elapsed() > self.config.reset_timeout {
                            self.failure_count = 0;
                        }
                    }
                }
            }
            CircuitState::Open => {
                // Shouldn't happen, but handle gracefully
            }
        }
    }

    /// Record a failure
    pub fn record_failure(&mut self) {
        self.last_failure = Some(Instant::now());

        match self.state {
            CircuitState::Closed => {
                self.failure_count += 1;
                if self.failure_count >= self.config.failure_threshold {
                    self.open();
                }
            }
            CircuitState::HalfOpen => {
                // Any failure in half-open state reopens the circuit
                self.open();
            }
            CircuitState::Open => {
                // Already open, no action needed
            }
        }
    }

    /// Open the circuit
    fn open(&mut self) {
        self.state = CircuitState::Open;
        self.opened_at = Some(Instant::now());
        self.failure_count = 0;
        self.success_count = 0;
        debug!("Circuit breaker opened");
    }

    /// Close the circuit
    fn close(&mut self) {
        self.state = CircuitState::Closed;
        self.failure_count = 0;
        self.success_count = 0;
        self.opened_at = None;
        debug!("Circuit breaker closed");
    }

    /// Check if request should be allowed
    pub fn should_allow(&mut self) -> bool {
        self.update_state();

        match self.state {
            CircuitState::Closed => true,
            CircuitState::Open => false,
            CircuitState::HalfOpen => {
                // Allow limited requests in half-open state
                true
            }
        }
    }

    /// Update circuit state based on timeouts
    fn update_state(&mut self) {
        if self.state == CircuitState::Open {
            if let Some(opened_at) = self.opened_at {
                if opened_at.elapsed() >= self.config.timeout {
                    self.state = CircuitState::HalfOpen;
                    self.success_count = 0;
                    debug!("Circuit breaker transitioned to half-open");
                }
            }
        }
    }

    /// Get current state
    pub fn state(&self) -> CircuitState {
        self.state
    }
}

/// Health monitor for all shards
pub struct HealthMonitor {
    /// Shard health states
    health_states: Arc<RwLock<HashMap<ShardId, ShardHealthState>>>,
    /// Configuration
    config: HealthMonitorConfig,
}

/// Health state for a single shard
#[derive(Debug, Clone)]
pub struct ShardHealthState {
    /// Current health
    pub health: ShardHealth,
    /// Circuit breaker
    pub circuit_breaker: CircuitBreaker,
    /// Last heartbeat time
    pub last_heartbeat: Option<Instant>,
    /// Last successful probe
    pub last_probe_success: Option<Instant>,
    /// Consecutive probe failures
    pub probe_failures: u32,
    /// Response time percentiles (for SLO tracking)
    pub response_times: ResponseTimeTracker,
}

/// Response time tracking
#[derive(Debug, Clone)]
pub struct ResponseTimeTracker {
    /// Recent response times (circular buffer)
    times: Vec<Duration>,
    /// Current position in buffer
    position: usize,
}

impl ResponseTimeTracker {
    fn new(capacity: usize) -> Self {
        ResponseTimeTracker {
            times: Vec::with_capacity(capacity),
            position: 0,
        }
    }

    fn record(&mut self, duration: Duration) {
        if self.times.len() < self.times.capacity() {
            self.times.push(duration);
        } else {
            self.times[self.position] = duration;
            self.position = (self.position + 1) % self.times.len();
        }
    }

    fn percentile(&self, p: f64) -> Option<Duration> {
        if self.times.is_empty() {
            return None;
        }

        let mut sorted = self.times.clone();
        sorted.sort();
        let index = ((p / 100.0) * sorted.len() as f64) as usize;
        sorted.get(index.min(sorted.len() - 1)).copied()
    }
}

/// Health monitor configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthMonitorConfig {
    /// Heartbeat timeout
    pub heartbeat_timeout: Duration,
    /// Probe interval
    pub probe_interval: Duration,
    /// Probe timeout
    pub probe_timeout: Duration,
    /// Degraded threshold (probe failures)
    pub degraded_threshold: u32,
    /// Unhealthy threshold (probe failures)
    pub unhealthy_threshold: u32,
    /// Down threshold (probe failures)
    pub down_threshold: u32,
    /// Response time window size
    pub response_time_window: usize,
    /// Circuit breaker config
    pub circuit_breaker: CircuitBreakerConfig,
}

impl Default for HealthMonitorConfig {
    fn default() -> Self {
        HealthMonitorConfig {
            heartbeat_timeout: Duration::from_secs(30),
            probe_interval: Duration::from_secs(10),
            probe_timeout: Duration::from_secs(5),
            degraded_threshold: 2,
            unhealthy_threshold: 5,
            down_threshold: 10,
            response_time_window: 100,
            circuit_breaker: CircuitBreakerConfig::default(),
        }
    }
}

impl HealthMonitor {
    /// Create a new health monitor
    pub fn new(config: HealthMonitorConfig) -> Self {
        HealthMonitor {
            health_states: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    /// Register a shard for monitoring
    pub fn register_shard(&self, shard_id: ShardId) {
        let mut states = self.health_states.write().unwrap();
        states.insert(
            shard_id,
            ShardHealthState {
                health: ShardHealth::Healthy,
                circuit_breaker: CircuitBreaker::new(self.config.circuit_breaker.clone()),
                last_heartbeat: Some(Instant::now()),
                last_probe_success: None,
                probe_failures: 0,
                response_times: ResponseTimeTracker::new(self.config.response_time_window),
            },
        );
        info!("Registered shard {} for health monitoring", shard_id);
    }

    /// Unregister a shard
    pub fn unregister_shard(&self, shard_id: ShardId) {
        let mut states = self.health_states.write().unwrap();
        states.remove(&shard_id);
        info!("Unregistered shard {} from health monitoring", shard_id);
    }

    /// Update heartbeat for a shard
    pub fn update_heartbeat(&self, shard_id: ShardId) {
        let mut states = self.health_states.write().unwrap();
        if let Some(state) = states.get_mut(&shard_id) {
            state.last_heartbeat = Some(Instant::now());
            debug!("Updated heartbeat for shard {}", shard_id);
        }
    }

    /// Record successful probe
    pub fn record_probe_success(&self, shard_id: ShardId, response_time: Duration) {
        let mut states = self.health_states.write().unwrap();
        if let Some(state) = states.get_mut(&shard_id) {
            state.last_probe_success = Some(Instant::now());
            state.probe_failures = 0;
            state.response_times.record(response_time);
            state.circuit_breaker.record_success();

            // Update health based on probe success
            if state.health != ShardHealth::Healthy {
                state.health = ShardHealth::Healthy;
                info!("Shard {} recovered to healthy state", shard_id);
            }
        }
    }

    /// Record failed probe
    pub fn record_probe_failure(&self, shard_id: ShardId) {
        let mut states = self.health_states.write().unwrap();
        if let Some(state) = states.get_mut(&shard_id) {
            state.probe_failures += 1;
            state.circuit_breaker.record_failure();

            // Update health based on failure count
            let new_health = if state.probe_failures >= self.config.down_threshold {
                ShardHealth::Down
            } else if state.probe_failures >= self.config.unhealthy_threshold {
                ShardHealth::Unhealthy
            } else if state.probe_failures >= self.config.degraded_threshold {
                ShardHealth::Degraded
            } else {
                state.health // No change
            };

            if new_health != state.health {
                warn!(
                    "Shard {} health changed from {:?} to {:?}",
                    shard_id, state.health, new_health
                );
                state.health = new_health;
            }
        }
    }

    /// Get health of a shard
    pub fn get_health(&self, shard_id: ShardId) -> Option<ShardHealth> {
        let states = self.health_states.read().unwrap();
        states.get(&shard_id).map(|s| s.health)
    }

    /// Check if a shard should be used for routing
    pub fn should_route_to(&self, shard_id: ShardId) -> bool {
        let mut states = self.health_states.write().unwrap();
        if let Some(state) = states.get_mut(&shard_id) {
            state.health.is_available() && state.circuit_breaker.should_allow()
        } else {
            false
        }
    }

    /// Get all healthy shards
    pub fn get_healthy_shards(&self) -> Vec<ShardId> {
        let states = self.health_states.read().unwrap();
        states
            .iter()
            .filter(|(_, state)| state.health == ShardHealth::Healthy)
            .map(|(id, _)| *id)
            .collect()
    }

    /// Get health statistics
    pub fn get_stats(&self) -> HealthStats {
        let states = self.health_states.read().unwrap();

        let mut stats = HealthStats {
            total_shards: states.len(),
            healthy: 0,
            degraded: 0,
            unhealthy: 0,
            down: 0,
            circuits_open: 0,
            circuits_half_open: 0,
        };

        for state in states.values() {
            match state.health {
                ShardHealth::Healthy => stats.healthy += 1,
                ShardHealth::Degraded => stats.degraded += 1,
                ShardHealth::Unhealthy => stats.unhealthy += 1,
                ShardHealth::Down => stats.down += 1,
            }

            match state.circuit_breaker.state() {
                CircuitState::Open => stats.circuits_open += 1,
                CircuitState::HalfOpen => stats.circuits_half_open += 1,
                _ => {}
            }
        }

        stats
    }

    /// Run periodic health checks
    pub async fn run_health_checks(&self) -> Result<()> {
        let mut interval_timer = interval(self.config.probe_interval);

        loop {
            interval_timer.tick().await;

            // Check heartbeat timeouts
            self.check_heartbeat_timeouts();

            // TODO: Implement actual probing
            // For now, just log
            debug!("Running health checks");
        }
    }

    /// Check for heartbeat timeouts
    fn check_heartbeat_timeouts(&self) {
        let mut states = self.health_states.write().unwrap();

        for (shard_id, state) in states.iter_mut() {
            if let Some(last_heartbeat) = state.last_heartbeat {
                if last_heartbeat.elapsed() > self.config.heartbeat_timeout {
                    if state.health != ShardHealth::Down {
                        warn!("Shard {} heartbeat timeout, marking as down", shard_id);
                        state.health = ShardHealth::Down;
                    }
                }
            }
        }
    }
}

/// Health statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStats {
    pub total_shards: usize,
    pub healthy: usize,
    pub degraded: usize,
    pub unhealthy: usize,
    pub down: usize,
    pub circuits_open: usize,
    pub circuits_half_open: usize,
}

impl HealthStats {
    /// Calculate health percentage
    pub fn health_percentage(&self) -> f64 {
        if self.total_shards == 0 {
            return 0.0;
        }
        (self.healthy as f64 / self.total_shards as f64) * 100.0
    }

    /// Check if cluster is healthy (majority of shards healthy)
    pub fn is_cluster_healthy(&self) -> bool {
        self.healthy > self.total_shards / 2
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_circuit_breaker_lifecycle() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            success_threshold: 2,
            timeout: Duration::from_millis(100),
            reset_timeout: Duration::from_secs(60),
        };

        let mut cb = CircuitBreaker::new(config);

        // Initially closed
        assert_eq!(cb.state(), CircuitState::Closed);
        assert!(cb.should_allow());

        // Record failures to open circuit
        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Closed);

        cb.record_failure(); // Third failure opens circuit
        assert_eq!(cb.state(), CircuitState::Open);
        assert!(!cb.should_allow());

        // Wait for timeout to transition to half-open
        std::thread::sleep(Duration::from_millis(150));
        assert!(cb.should_allow()); // Updates state internally
        assert_eq!(cb.state(), CircuitState::HalfOpen);

        // Success in half-open moves towards closing
        cb.record_success();
        cb.record_success(); // Second success closes circuit
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[test]
    fn test_health_monitor_registration() {
        let monitor = HealthMonitor::new(HealthMonitorConfig::default());

        monitor.register_shard(ShardId(1));
        assert_eq!(monitor.get_health(ShardId(1)), Some(ShardHealth::Healthy));

        monitor.unregister_shard(ShardId(1));
        assert_eq!(monitor.get_health(ShardId(1)), None);
    }

    #[test]
    fn test_health_degradation() {
        let config = HealthMonitorConfig {
            degraded_threshold: 2,
            unhealthy_threshold: 4,
            down_threshold: 6,
            ..Default::default()
        };

        let monitor = HealthMonitor::new(config);
        monitor.register_shard(ShardId(1));

        // Initial state is healthy
        assert_eq!(monitor.get_health(ShardId(1)), Some(ShardHealth::Healthy));

        // First failure
        monitor.record_probe_failure(ShardId(1));
        assert_eq!(monitor.get_health(ShardId(1)), Some(ShardHealth::Healthy));

        // Second failure -> degraded
        monitor.record_probe_failure(ShardId(1));
        assert_eq!(monitor.get_health(ShardId(1)), Some(ShardHealth::Degraded));

        // More failures -> unhealthy
        monitor.record_probe_failure(ShardId(1));
        monitor.record_probe_failure(ShardId(1));
        assert_eq!(monitor.get_health(ShardId(1)), Some(ShardHealth::Unhealthy));

        // Even more failures -> down
        monitor.record_probe_failure(ShardId(1));
        monitor.record_probe_failure(ShardId(1));
        assert_eq!(monitor.get_health(ShardId(1)), Some(ShardHealth::Down));

        // Recovery
        monitor.record_probe_success(ShardId(1), Duration::from_millis(10));
        assert_eq!(monitor.get_health(ShardId(1)), Some(ShardHealth::Healthy));
    }

    #[test]
    fn test_health_stats() {
        let monitor = HealthMonitor::new(HealthMonitorConfig::default());

        monitor.register_shard(ShardId(1));
        monitor.register_shard(ShardId(2));
        monitor.register_shard(ShardId(3));

        // Make shard 2 degraded
        monitor.record_probe_failure(ShardId(2));
        monitor.record_probe_failure(ShardId(2));

        let stats = monitor.get_stats();
        assert_eq!(stats.total_shards, 3);
        assert_eq!(stats.healthy, 2);
        assert_eq!(stats.degraded, 1);
        assert!(stats.is_cluster_healthy());
    }
}
