//! Raft configuration for bitemporal consensus.

use std::time::Duration;

/// Raft configuration parameters.
///
/// These parameters control the behavior of the Raft consensus algorithm.
#[derive(Debug, Clone)]
pub struct RaftConfig {
    /// Unique node identifier (must be > 0)
    pub node_id: u64,

    /// Number of ticks between elections
    ///
    /// Higher values reduce election frequency but increase latency
    /// when leader fails. Typical values: 10-50.
    pub election_tick: usize,

    /// Number of ticks between heartbeats
    ///
    /// Higher values reduce network traffic but increase detection
    /// time for leader failures. Typical values: 1-5.
    pub heartbeat_tick: usize,

    /// Maximum number of uncommitted entries in the log
    ///
    /// Limits memory usage. Typical values: 1000-10000.
    pub max_inflight_msgs: usize,

    /// Path to RocksDB storage
    pub storage_path: String,

    /// Tick interval duration
    ///
    /// How often the Raft tick function is called.
    /// Typical value: 100ms.
    pub tick_interval: Duration,
}

impl RaftConfig {
    /// Create a new Raft configuration with default values.
    pub fn new(node_id: u64, storage_path: String) -> Self {
        Self {
            node_id,
            election_tick: 10,
            heartbeat_tick: 3,
            max_inflight_msgs: 256,
            storage_path,
            tick_interval: Duration::from_millis(100),
        }
    }

    /// Set the election tick count.
    pub fn with_election_tick(mut self, tick: usize) -> Self {
        self.election_tick = tick;
        self
    }

    /// Set the heartbeat tick count.
    pub fn with_heartbeat_tick(mut self, tick: usize) -> Self {
        self.heartbeat_tick = tick;
        self
    }

    /// Set the maximum inflight messages.
    pub fn with_max_inflight_msgs(mut self, max: usize) -> Self {
        self.max_inflight_msgs = max;
        self
    }

    /// Set the tick interval.
    pub fn with_tick_interval(mut self, interval: Duration) -> Self {
        self.tick_interval = interval;
        self
    }

    /// Validate the configuration.
    ///
    /// Returns an error if the configuration is invalid.
    pub fn validate(&self) -> Result<(), String> {
        if self.node_id == 0 {
            return Err("node_id must be > 0".to_string());
        }

        if self.heartbeat_tick == 0 {
            return Err("heartbeat_tick must be > 0".to_string());
        }

        if self.election_tick <= self.heartbeat_tick {
            return Err("election_tick must be > heartbeat_tick".to_string());
        }

        if self.max_inflight_msgs == 0 {
            return Err("max_inflight_msgs must be > 0".to_string());
        }

        if self.storage_path.is_empty() {
            return Err("storage_path cannot be empty".to_string());
        }

        Ok(())
    }
}

impl Default for RaftConfig {
    fn default() -> Self {
        Self::new(1, ":memory:".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = RaftConfig::default();
        assert_eq!(config.node_id, 1);
        assert_eq!(config.election_tick, 10);
        assert_eq!(config.heartbeat_tick, 3);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_new_config() {
        let config = RaftConfig::new(42, "/tmp/raft".to_string());
        assert_eq!(config.node_id, 42);
        assert_eq!(config.storage_path, "/tmp/raft");
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_builder_pattern() {
        let config = RaftConfig::new(1, "/tmp/raft".to_string())
            .with_election_tick(20)
            .with_heartbeat_tick(5)
            .with_max_inflight_msgs(512)
            .with_tick_interval(Duration::from_millis(50));

        assert_eq!(config.election_tick, 20);
        assert_eq!(config.heartbeat_tick, 5);
        assert_eq!(config.max_inflight_msgs, 512);
        assert_eq!(config.tick_interval, Duration::from_millis(50));
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_zero_node_id() {
        let config = RaftConfig {
            node_id: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_election_tick_too_small() {
        let config = RaftConfig::new(1, "/tmp/raft".to_string())
            .with_election_tick(3)
            .with_heartbeat_tick(3);
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_heartbeat_tick_zero() {
        let config = RaftConfig::new(1, "/tmp/raft".to_string()).with_heartbeat_tick(0);
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_empty_storage_path() {
        let config = RaftConfig {
            storage_path: String::new(),
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_zero_max_inflight() {
        let config = RaftConfig {
            max_inflight_msgs: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }
}
