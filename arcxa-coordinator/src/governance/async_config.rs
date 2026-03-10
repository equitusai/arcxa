use std::time::Duration;

/// Configuration for async governance brain with batching
#[derive(Debug, Clone)]
pub struct AsyncGovernanceConfig {
    /// Maximum batch size before forcing flush
    pub batch_size: usize,

    /// Maximum time to wait before flushing batch
    pub batch_timeout: Duration,

    /// Number of background processing tasks
    pub num_processors: usize,

    /// Channel capacity for incoming events
    pub channel_capacity: usize,

    /// Maximum retries for failed batches
    pub max_retries: usize,

    /// Enable metrics collection
    pub enable_metrics: bool,
}

impl Default for AsyncGovernanceConfig {
    fn default() -> Self {
        Self {
            batch_size: 500,
            batch_timeout: Duration::from_millis(100),
            num_processors: 4,
            channel_capacity: 10_000,
            max_retries: 3,
            enable_metrics: true,
        }
    }
}

impl AsyncGovernanceConfig {
    /// Create config optimized for low latency
    pub fn low_latency() -> Self {
        Self {
            batch_size: 100,
            batch_timeout: Duration::from_millis(10),
            num_processors: 8,
            ..Default::default()
        }
    }

    /// Create config optimized for high throughput
    pub fn high_throughput() -> Self {
        Self {
            batch_size: 1000,
            batch_timeout: Duration::from_millis(200),
            num_processors: 4,
            channel_capacity: 50_000,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let config = AsyncGovernanceConfig::default();
        assert_eq!(config.batch_size, 500);
        assert_eq!(config.num_processors, 4);
    }

    #[test]
    fn test_config_presets() {
        let low_latency = AsyncGovernanceConfig::low_latency();
        assert_eq!(low_latency.batch_size, 100);

        let high_throughput = AsyncGovernanceConfig::high_throughput();
        assert_eq!(high_throughput.batch_size, 1000);
    }
}
