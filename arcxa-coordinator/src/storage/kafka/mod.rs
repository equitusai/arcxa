//! Kafka integration with durability guarantees
//!
//! This module provides a WAL-backed Kafka producer that ensures
//! exactly-once delivery semantics for lineage events.
//!
//! # Architecture
//!
//! The durable Kafka sink provides:
//! - **Write-Ahead Logging**: All events written to WAL before Kafka send
//! - **Acknowledgment Tracking**: In-memory tracking of Kafka confirmations
//! - **Circuit Breaker**: Graceful degradation during Kafka failures
//! - **Automatic Replay**: Recovery of unacknowledged events on startup
//! - **Distributed Coordination**: Raft-based leader election for HA deployments
//!
//! # Usage
//!
//! ```rust,ignore
//! use graphica_coordinator::storage::kafka::{DurableKafkaLineageSink, KafkaConfig};
//! use graphica_coordinator::storage::wal::{WalCoordinator, WalConfig, FileWal};
//! use graphica_core::core::LineageSink;
//! use std::sync::Arc;
//!
//! # async fn example() -> anyhow::Result<()> {
//! # let file_wal = Arc::new(FileWal::new("/tmp/wal".into(), WalConfig::default())?);
//! let wal = Arc::new(WalCoordinator::new(file_wal, WalConfig::default()).await?);
//! let config = KafkaConfig::default();
//!
//! let sink = DurableKafkaLineageSink::new(
//!     "localhost:9092",
//!     wal,
//!     config,
//! ).await?;
//!
//! // Write lineage event - guaranteed durable
//! # let event = graphica_core::core::LineageEvent::default();
//! sink.write(event)?;
//! # Ok(())
//! # }
//! ```

mod acknowledgment_tracker;
mod circuit_breaker;
mod config;
mod distributed_replay;
mod durable_sink;
mod feature_flags;
mod kafka_tracing;
mod metrics;
mod replay_manager;

pub use acknowledgment_tracker::AcknowledgmentTracker;
pub use circuit_breaker::{CircuitBreaker, CircuitBreakerConfig, CircuitState};
pub use config::{DurabilityConfig, KafkaConfig};
pub use distributed_replay::{DistributedReplayCoordinator, RaftConfig, RaftState, ReplayLogEntry};
pub use durable_sink::DurableKafkaLineageSink;
pub use feature_flags::{FeatureFlagConfig, FeatureFlagManager, FeatureFlags, RolloutStats};
pub use kafka_tracing::KafkaTracing;
pub use metrics::{KafkaMetrics, KafkaSendResult};
pub use replay_manager::{RecoveryReport, ReplayConfig, ReplayManager};

// Re-export for backward compatibility (DEPRECATED)
#[deprecated(
    since = "0.3.0",
    note = "Use DurableKafkaLineageSink instead - the old KafkaLineageSink has fire-and-forget behavior that can lose data"
)]
pub use super::kafka_legacy::KafkaLineageSink;
