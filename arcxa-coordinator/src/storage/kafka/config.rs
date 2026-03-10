//! Configuration for durable Kafka producer

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Main Kafka configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KafkaConfig {
    /// Durability settings
    pub durability: DurabilityConfig,

    /// Kafka producer settings
    pub producer: ProducerConfig,

    /// Topic configuration
    pub topic: String,

    /// Acknowledgment tracking settings
    pub ack_tracking: AckTrackingConfig,
}

impl Default for KafkaConfig {
    fn default() -> Self {
        Self {
            durability: DurabilityConfig::default(),
            producer: ProducerConfig::default(),
            topic: "graphica.lineage.events".to_string(),
            ack_tracking: AckTrackingConfig::default(),
        }
    }
}

/// Durability configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DurabilityConfig {
    /// Enable WAL-backed durability
    pub enabled: bool,

    /// Timeout for Kafka sends
    pub send_timeout: Duration,

    /// Maximum retries for failed sends
    pub max_retries: u32,

    /// Retry backoff (exponential)
    pub retry_backoff: Duration,
}

impl Default for DurabilityConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            send_timeout: Duration::from_secs(5),
            max_retries: 3,
            retry_backoff: Duration::from_secs(1),
        }
    }
}

/// Kafka producer configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProducerConfig {
    /// Compression codec
    pub compression: String,

    /// Batch size (bytes)
    pub batch_size: usize,

    /// Linger time (ms) - how long to wait for batching
    pub linger_ms: u64,

    /// Request timeout
    pub request_timeout: Duration,

    /// Max in-flight requests per connection
    pub max_in_flight: usize,

    /// Enable idempotence
    pub enable_idempotence: bool,

    /// Acks required (0, 1, all)
    pub acks: String,
}

impl Default for ProducerConfig {
    fn default() -> Self {
        Self {
            compression: "snappy".to_string(),
            batch_size: 16384, // 16KB
            linger_ms: 10,
            request_timeout: Duration::from_secs(30),
            max_in_flight: 5,
            enable_idempotence: true,
            acks: "all".to_string(), // Wait for all replicas
        }
    }
}

/// Acknowledgment tracking configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AckTrackingConfig {
    /// Cleanup interval for acknowledged entries
    pub cleanup_interval: Duration,

    /// Retention for acknowledged entries (for deduplication)
    pub ack_retention: Duration,

    /// Maximum pending acknowledgments before backpressure
    pub max_pending: usize,
}

impl Default for AckTrackingConfig {
    fn default() -> Self {
        Self {
            cleanup_interval: Duration::from_secs(60),
            ack_retention: Duration::from_secs(3600), // 1 hour
            max_pending: 100_000,
        }
    }
}

impl KafkaConfig {
    /// Production configuration with high durability
    pub fn production() -> Self {
        Self {
            durability: DurabilityConfig {
                enabled: true,
                send_timeout: Duration::from_secs(10),
                max_retries: 5,
                retry_backoff: Duration::from_secs(2),
            },
            producer: ProducerConfig {
                compression: "lz4".to_string(),
                batch_size: 65536, // 64KB
                linger_ms: 5,
                request_timeout: Duration::from_secs(30),
                max_in_flight: 5,
                enable_idempotence: true,
                acks: "all".to_string(),
            },
            topic: "graphica.lineage.events".to_string(),
            ack_tracking: AckTrackingConfig {
                cleanup_interval: Duration::from_secs(30),
                ack_retention: Duration::from_secs(7200), // 2 hours
                max_pending: 500_000,
            },
        }
    }

    /// High-throughput configuration (lower durability)
    pub fn high_throughput() -> Self {
        Self {
            durability: DurabilityConfig {
                enabled: true,
                send_timeout: Duration::from_secs(3),
                max_retries: 2,
                retry_backoff: Duration::from_millis(500),
            },
            producer: ProducerConfig {
                compression: "snappy".to_string(),
                batch_size: 131072, // 128KB
                linger_ms: 20,
                request_timeout: Duration::from_secs(15),
                max_in_flight: 10,
                enable_idempotence: true,
                acks: "1".to_string(), // Leader only
            },
            topic: "graphica.lineage.events".to_string(),
            ack_tracking: AckTrackingConfig {
                cleanup_interval: Duration::from_secs(120),
                ack_retention: Duration::from_secs(1800), // 30 minutes
                max_pending: 1_000_000,
            },
        }
    }

    /// Builder pattern
    pub fn with_topic(mut self, topic: String) -> Self {
        self.topic = topic;
        self
    }

    pub fn with_durability(mut self, enabled: bool) -> Self {
        self.durability.enabled = enabled;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = KafkaConfig::default();
        assert!(config.durability.enabled);
        assert_eq!(config.topic, "graphica.lineage.events");
        assert_eq!(config.producer.acks, "all");
    }

    #[test]
    fn test_production_config() {
        let config = KafkaConfig::production();
        assert_eq!(config.producer.acks, "all");
        assert_eq!(config.durability.max_retries, 5);
    }

    #[test]
    fn test_high_throughput_config() {
        let config = KafkaConfig::high_throughput();
        assert_eq!(config.producer.acks, "1");
        assert_eq!(config.producer.batch_size, 131072);
    }

    #[test]
    fn test_builder_pattern() {
        let config = KafkaConfig::default()
            .with_topic("custom.topic".to_string())
            .with_durability(false);

        assert_eq!(config.topic, "custom.topic");
        assert!(!config.durability.enabled);
    }
}
