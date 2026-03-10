//! Execution Mode Configuration
//!
//! Defines how workflows are executed: Batch, Streaming, or MicroBatch.
//! Designed for horizontal scalability, fault tolerance, and backward compatibility.

use chrono::Duration;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Execution mode for workflows
///
/// Determines whether a workflow runs as traditional batch, real-time streaming,
/// or micro-batch (hybrid mode).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ExecutionMode {
    /// Traditional batch execution (default for backward compatibility)
    ///
    /// Processes entire datasets at once with fixed schedule.
    /// Best for: End-of-day reports, large backfills, complex multi-table joins
    Batch,

    /// Real-time streaming execution
    ///
    /// Processes events as they arrive with sub-second latency.
    /// Best for: Real-time dashboards, fraud detection, live data quality
    Streaming {
        #[serde(flatten)]
        config: StreamingConfig,
    },

    /// Micro-batch execution (hybrid mode)
    ///
    /// Accumulates events into small batches for processing.
    /// Best for: Near real-time with simpler operations, gradual migration from batch
    MicroBatch {
        #[serde(flatten)]
        config: MicroBatchConfig,
    },
}

impl Default for ExecutionMode {
    /// Default to Batch mode for backward compatibility
    fn default() -> Self {
        ExecutionMode::Batch
    }
}

/// Configuration for streaming execution
///
/// Designed for horizontal scalability with multiple workers consuming
/// from partitioned Kafka topics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StreamingConfig {
    /// Kafka topic to consume from
    ///
    /// Should be partitioned for parallelism (recommended: 10+ partitions)
    pub source_topic: String,

    /// Consumer group ID for coordinated consumption
    ///
    /// Multiple workers with same group_id automatically share partitions
    pub consumer_group: String,

    /// Checkpoint interval in milliseconds
    ///
    /// How often to persist Kafka offsets for fault tolerance.
    /// Smaller values = less data loss on failure, higher overhead.
    /// Recommended: 30000-60000 (30-60 seconds)
    #[serde(default = "default_checkpoint_interval_ms")]
    pub checkpoint_interval_ms: u64,

    /// Watermark strategy for event-time processing
    ///
    /// Controls how late events are handled in windowed operations.
    #[serde(default)]
    pub watermark_strategy: WatermarkStrategy,

    /// Maximum parallelism (number of workers)
    ///
    /// Each worker processes a subset of Kafka partitions.
    /// Should be <= number of Kafka topic partitions for efficiency.
    /// Set to None for unlimited (auto-scale based on partitions).
    #[serde(default)]
    pub max_parallel_workers: Option<usize>,

    /// State backend configuration
    ///
    /// Where to store stateful operator state (aggregations, joins).
    #[serde(default)]
    pub state_backend: StateBackendConfig,

    /// Auto-scaling configuration
    ///
    /// Automatically adjust worker count based on load.
    #[serde(default)]
    pub auto_scaling: Option<AutoScalingConfig>,

    /// Additional Kafka consumer properties
    ///
    /// Advanced configuration (e.g., fetch.min.bytes, session.timeout.ms)
    #[serde(default)]
    pub kafka_properties: HashMap<String, String>,
}

/// Configuration for micro-batch execution
///
/// Simpler than full streaming but provides lower latency than traditional batch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MicroBatchConfig {
    /// Kafka topic to consume from
    pub source_topic: String,

    /// Consumer group ID
    pub consumer_group: String,

    /// Maximum number of records per micro-batch
    ///
    /// Larger batches = better throughput, higher latency.
    /// Recommended: 1000-10000
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,

    /// Maximum time to wait before flushing batch (milliseconds)
    ///
    /// Ensures low latency even with low event rates.
    /// Recommended: 5000-30000 (5-30 seconds)
    #[serde(default = "default_batch_interval_ms")]
    pub batch_interval_ms: u64,

    /// Maximum parallel micro-batches in flight
    ///
    /// Limits memory usage and backpressure.
    /// Recommended: 4-16
    #[serde(default = "default_max_parallel_batches")]
    pub max_parallel_batches: usize,

    /// Checkpoint interval in milliseconds
    #[serde(default = "default_checkpoint_interval_ms")]
    pub checkpoint_interval_ms: u64,
}

/// Watermark strategy for event-time processing
///
/// Determines how to generate watermarks that track progress in event time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WatermarkStrategy {
    /// Periodic watermarks based on processing time
    ///
    /// Simplest strategy, but doesn't handle out-of-order events well.
    ProcessingTime {
        /// Interval between watermark updates (milliseconds)
        #[serde(default = "default_watermark_interval_ms")]
        interval_ms: u64,
    },

    /// Bounded out-of-orderness watermarks
    ///
    /// Allows events to be late up to max_out_of_orderness before considered late.
    /// Best for most use cases with reasonably ordered streams.
    BoundedOutOfOrderness {
        /// Maximum allowed lateness (milliseconds)
        ///
        /// Events older than (current_watermark - max_out_of_orderness) are late.
        /// Recommended: 10000-60000 (10-60 seconds)
        #[serde(default = "default_max_out_of_orderness_ms")]
        max_out_of_orderness_ms: u64,
    },

    /// Custom watermark generation based on event field
    ///
    /// Extract event time from specific field in records.
    EventTime {
        /// Field name containing event timestamp
        ///
        /// Should be ISO 8601 string or Unix timestamp (ms)
        timestamp_field: String,

        /// Maximum allowed lateness (milliseconds)
        #[serde(default = "default_max_out_of_orderness_ms")]
        max_out_of_orderness_ms: u64,
    },
}

impl Default for WatermarkStrategy {
    fn default() -> Self {
        WatermarkStrategy::BoundedOutOfOrderness {
            max_out_of_orderness_ms: default_max_out_of_orderness_ms(),
        }
    }
}

/// State backend configuration
///
/// Where to store operator state for fault tolerance and recovery.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StateBackendConfig {
    /// In-memory state (fastest, but lost on crash)
    ///
    /// Only suitable for development or stateless operations.
    Memory,

    /// RocksDB-based state (persistent, fault-tolerant)
    ///
    /// Recommended for production. Automatically checkpointed.
    RocksDB {
        /// Base path for RocksDB storage
        ///
        /// Each worker creates a subdirectory here.
        #[serde(default = "default_rocksdb_path")]
        path: String,

        /// Enable incremental checkpoints
        ///
        /// Only checkpoint changed state (faster, more efficient).
        #[serde(default = "default_incremental_checkpoints")]
        incremental_checkpoints: bool,
    },
}

impl Default for StateBackendConfig {
    fn default() -> Self {
        StateBackendConfig::RocksDB {
            path: default_rocksdb_path(),
            incremental_checkpoints: true,
        }
    }
}

/// Auto-scaling configuration for streaming workflows
///
/// Dynamically adjust worker count based on load metrics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AutoScalingConfig {
    /// Minimum number of workers (for low traffic)
    #[serde(default = "default_min_workers")]
    pub min_workers: usize,

    /// Maximum number of workers (cost ceiling)
    #[serde(default = "default_max_workers")]
    pub max_workers: usize,

    /// Target Kafka consumer lag (messages)
    ///
    /// Scale up if lag exceeds this threshold.
    /// Recommended: 10000-100000
    #[serde(default = "default_target_lag")]
    pub target_lag: u64,

    /// Target processing latency (milliseconds)
    ///
    /// Scale up if P95 latency exceeds this threshold.
    /// Recommended: 1000-5000 (1-5 seconds)
    #[serde(default = "default_target_latency_ms")]
    pub target_latency_ms: u64,

    /// Scale-up cooldown period (seconds)
    ///
    /// Wait this long before scaling up again (avoid thrashing).
    /// Recommended: 60-300 (1-5 minutes)
    #[serde(default = "default_scaleup_cooldown_secs")]
    pub scaleup_cooldown_secs: u64,

    /// Scale-down cooldown period (seconds)
    ///
    /// Wait this long before scaling down (avoid thrashing).
    /// Recommended: 300-900 (5-15 minutes)
    #[serde(default = "default_scaledown_cooldown_secs")]
    pub scaledown_cooldown_secs: u64,
}

// === Default Values (Tuned for Production) ===

fn default_checkpoint_interval_ms() -> u64 {
    60_000 // 1 minute
}

fn default_batch_size() -> usize {
    5_000 // 5K records per micro-batch
}

fn default_batch_interval_ms() -> u64 {
    10_000 // 10 seconds
}

fn default_max_parallel_batches() -> usize {
    8 // 8 concurrent micro-batches
}

fn default_watermark_interval_ms() -> u64 {
    1_000 // 1 second
}

fn default_max_out_of_orderness_ms() -> u64 {
    30_000 // 30 seconds
}

fn default_rocksdb_path() -> String {
    "/data/streaming_state".to_string()
}

fn default_incremental_checkpoints() -> bool {
    true
}

fn default_min_workers() -> usize {
    2 // At least 2 for HA
}

fn default_max_workers() -> usize {
    20 // Cap at 20 workers
}

fn default_target_lag() -> u64 {
    50_000 // 50K messages lag target
}

fn default_target_latency_ms() -> u64 {
    2_000 // 2 second P95 latency target
}

fn default_scaleup_cooldown_secs() -> u64 {
    120 // 2 minutes
}

fn default_scaledown_cooldown_secs() -> u64 {
    600 // 10 minutes
}

// === Validation Methods ===

impl ExecutionMode {
    /// Validate execution mode configuration
    pub fn validate(&self) -> anyhow::Result<()> {
        match self {
            ExecutionMode::Batch => Ok(()),

            ExecutionMode::Streaming { config } => {
                if config.source_topic.is_empty() {
                    anyhow::bail!("Streaming source_topic cannot be empty");
                }
                if config.consumer_group.is_empty() {
                    anyhow::bail!("Streaming consumer_group cannot be empty");
                }
                if config.checkpoint_interval_ms == 0 {
                    anyhow::bail!("Checkpoint interval must be > 0");
                }
                Ok(())
            }

            ExecutionMode::MicroBatch { config } => {
                if config.source_topic.is_empty() {
                    anyhow::bail!("MicroBatch source_topic cannot be empty");
                }
                if config.consumer_group.is_empty() {
                    anyhow::bail!("MicroBatch consumer_group cannot be empty");
                }
                if config.batch_size == 0 {
                    anyhow::bail!("Batch size must be > 0");
                }
                if config.batch_interval_ms == 0 {
                    anyhow::bail!("Batch interval must be > 0");
                }
                Ok(())
            }
        }
    }

    /// Returns true if this mode requires Kafka
    pub fn requires_kafka(&self) -> bool {
        matches!(
            self,
            ExecutionMode::Streaming { .. } | ExecutionMode::MicroBatch { .. }
        )
    }

    /// Returns true if this mode is stateful (requires checkpointing)
    pub fn is_stateful(&self) -> bool {
        matches!(
            self,
            ExecutionMode::Streaming { .. } | ExecutionMode::MicroBatch { .. }
        )
    }

    /// Get consumer group ID (if applicable)
    pub fn consumer_group(&self) -> Option<&str> {
        match self {
            ExecutionMode::Streaming { config } => Some(&config.consumer_group),
            ExecutionMode::MicroBatch { config } => Some(&config.consumer_group),
            ExecutionMode::Batch => None,
        }
    }

    /// Get source topic (if applicable)
    pub fn source_topic(&self) -> Option<&str> {
        match self {
            ExecutionMode::Streaming { config } => Some(&config.source_topic),
            ExecutionMode::MicroBatch { config } => Some(&config.source_topic),
            ExecutionMode::Batch => None,
        }
    }

    /// Estimate resource requirements
    pub fn estimate_resources(&self) -> ResourceEstimate {
        match self {
            ExecutionMode::Batch => ResourceEstimate {
                cpu_cores: 4,
                memory_mb: 8192,
                storage_mb: 1024,
                network_mbps: 100,
            },

            ExecutionMode::Streaming { config } => {
                let workers = config.max_parallel_workers.unwrap_or(10);
                ResourceEstimate {
                    cpu_cores: workers * 2,     // 2 cores per worker
                    memory_mb: workers * 4096,  // 4GB per worker
                    storage_mb: 10240,          // 10GB for RocksDB state
                    network_mbps: workers * 50, // 50 Mbps per worker
                }
            }

            ExecutionMode::MicroBatch { config } => ResourceEstimate {
                cpu_cores: config.max_parallel_batches * 2,
                memory_mb: config.max_parallel_batches * 2048,
                storage_mb: 2048,
                network_mbps: 200,
            },
        }
    }
}

/// Estimated resource requirements for an execution mode
#[derive(Debug, Clone, PartialEq)]
pub struct ResourceEstimate {
    pub cpu_cores: usize,
    pub memory_mb: usize,
    pub storage_mb: usize,
    pub network_mbps: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_execution_mode() {
        let mode = ExecutionMode::default();
        assert_eq!(mode, ExecutionMode::Batch);
    }

    #[test]
    fn test_batch_mode_validation() {
        let mode = ExecutionMode::Batch;
        assert!(mode.validate().is_ok());
        assert!(!mode.requires_kafka());
        assert!(!mode.is_stateful());
    }

    #[test]
    fn test_streaming_mode_validation() {
        let mode = ExecutionMode::Streaming {
            config: StreamingConfig {
                source_topic: "events".to_string(),
                consumer_group: "processors".to_string(),
                checkpoint_interval_ms: 60_000,
                watermark_strategy: WatermarkStrategy::default(),
                max_parallel_workers: Some(10),
                state_backend: StateBackendConfig::default(),
                auto_scaling: None,
                kafka_properties: HashMap::new(),
            },
        };

        assert!(mode.validate().is_ok());
        assert!(mode.requires_kafka());
        assert!(mode.is_stateful());
        assert_eq!(mode.consumer_group(), Some("processors"));
        assert_eq!(mode.source_topic(), Some("events"));
    }

    #[test]
    fn test_streaming_mode_invalid() {
        let mode = ExecutionMode::Streaming {
            config: StreamingConfig {
                source_topic: "".to_string(), // Invalid: empty
                consumer_group: "processors".to_string(),
                checkpoint_interval_ms: 60_000,
                watermark_strategy: WatermarkStrategy::default(),
                max_parallel_workers: Some(10),
                state_backend: StateBackendConfig::default(),
                auto_scaling: None,
                kafka_properties: HashMap::new(),
            },
        };

        assert!(mode.validate().is_err());
    }

    #[test]
    fn test_micro_batch_mode() {
        let mode = ExecutionMode::MicroBatch {
            config: MicroBatchConfig {
                source_topic: "events".to_string(),
                consumer_group: "processors".to_string(),
                batch_size: 5_000,
                batch_interval_ms: 10_000,
                max_parallel_batches: 8,
                checkpoint_interval_ms: 60_000,
            },
        };

        assert!(mode.validate().is_ok());
        assert!(mode.requires_kafka());
        assert!(mode.is_stateful());
    }

    #[test]
    fn test_resource_estimation_batch() {
        let mode = ExecutionMode::Batch;
        let estimate = mode.estimate_resources();

        assert_eq!(estimate.cpu_cores, 4);
        assert_eq!(estimate.memory_mb, 8192);
    }

    #[test]
    fn test_resource_estimation_streaming() {
        let mode = ExecutionMode::Streaming {
            config: StreamingConfig {
                source_topic: "events".to_string(),
                consumer_group: "processors".to_string(),
                checkpoint_interval_ms: 60_000,
                watermark_strategy: WatermarkStrategy::default(),
                max_parallel_workers: Some(5), // 5 workers
                state_backend: StateBackendConfig::default(),
                auto_scaling: None,
                kafka_properties: HashMap::new(),
            },
        };

        let estimate = mode.estimate_resources();
        assert_eq!(estimate.cpu_cores, 10); // 5 workers * 2 cores
        assert_eq!(estimate.memory_mb, 20480); // 5 workers * 4GB
    }

    #[test]
    fn test_serialization_backward_compatible() {
        // Workflow without execution_mode field should default to Batch
        let json = r#"{"type": "batch"}"#;
        let mode: ExecutionMode = serde_json::from_str(json).unwrap();
        assert_eq!(mode, ExecutionMode::Batch);
    }

    #[test]
    fn test_serialization_streaming() {
        let mode = ExecutionMode::Streaming {
            config: StreamingConfig {
                source_topic: "events".to_string(),
                consumer_group: "processors".to_string(),
                checkpoint_interval_ms: 60_000,
                watermark_strategy: WatermarkStrategy::BoundedOutOfOrderness {
                    max_out_of_orderness_ms: 30_000,
                },
                max_parallel_workers: Some(10),
                state_backend: StateBackendConfig::RocksDB {
                    path: "/data/state".to_string(),
                    incremental_checkpoints: true,
                },
                auto_scaling: None,
                kafka_properties: HashMap::new(),
            },
        };

        let json = serde_json::to_string(&mode).unwrap();
        let deserialized: ExecutionMode = serde_json::from_str(&json).unwrap();
        assert_eq!(mode, deserialized);
    }

    #[test]
    fn test_auto_scaling_config() {
        let config = AutoScalingConfig {
            min_workers: 2,
            max_workers: 20,
            target_lag: 50_000,
            target_latency_ms: 2_000,
            scaleup_cooldown_secs: 120,
            scaledown_cooldown_secs: 600,
        };

        assert_eq!(config.min_workers, 2);
        assert_eq!(config.max_workers, 20);
    }

    #[test]
    fn test_watermark_strategies() {
        let processing_time = WatermarkStrategy::ProcessingTime { interval_ms: 1_000 };
        let bounded = WatermarkStrategy::BoundedOutOfOrderness {
            max_out_of_orderness_ms: 30_000,
        };
        let event_time = WatermarkStrategy::EventTime {
            timestamp_field: "event_time".to_string(),
            max_out_of_orderness_ms: 60_000,
        };

        assert!(serde_json::to_string(&processing_time).is_ok());
        assert!(serde_json::to_string(&bounded).is_ok());
        assert!(serde_json::to_string(&event_time).is_ok());
    }

    #[test]
    fn test_state_backend_configs() {
        let memory = StateBackendConfig::Memory;
        let rocksdb = StateBackendConfig::RocksDB {
            path: "/data/state".to_string(),
            incremental_checkpoints: true,
        };

        assert!(serde_json::to_string(&memory).is_ok());
        assert!(serde_json::to_string(&rocksdb).is_ok());
    }
}
