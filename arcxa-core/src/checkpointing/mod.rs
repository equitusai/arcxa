//! # Checkpointing Module
//!
//! Coordinates Kafka offsets + deduplication state for exactly-once semantics.
//! Enables fast recovery after crashes without re-processing or losing dedup state.

mod dedup;
mod manager;
mod storage;

pub use dedup::CheckpointableDedupState;
pub use manager::CheckpointManager;
pub use storage::CheckpointStorage;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::SystemTime;

/// Complete system checkpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Kafka partition offsets at checkpoint time
    pub kafka_offsets: HashMap<i32, i64>,

    /// Deduplication state (record_id → timestamp)
    pub dedup_state: HashMap<String, i64>,

    /// Checkpoint creation time
    pub timestamp: SystemTime,

    /// Worker count (for validation on restore)
    pub worker_count: usize,

    /// Version for forward compatibility
    pub version: u32,
}

impl Checkpoint {
    pub fn new(worker_count: usize) -> Self {
        Self {
            kafka_offsets: HashMap::new(),
            dedup_state: HashMap::new(),
            timestamp: SystemTime::now(),
            worker_count,
            version: 1,
        }
    }

    pub fn with_offsets(mut self, offsets: HashMap<i32, i64>) -> Self {
        self.kafka_offsets = offsets;
        self
    }

    pub fn with_dedup_state(mut self, state: HashMap<String, i64>) -> Self {
        self.dedup_state = state;
        self
    }

    /// Size estimate in bytes
    pub fn estimated_size(&self) -> usize {
        // Rough estimate: offset map + dedup map + metadata
        (self.kafka_offsets.len() * 16) +
        (self.dedup_state.len() * 100) + // ~100 bytes per entry (ID + timestamp + overhead)
        100 // metadata
    }
}

/// Trait for components that can be checkpointed
pub trait Checkpointable: Send + Sync {
    /// Create a snapshot of current state
    fn snapshot(&self) -> Result<HashMap<String, i64>>;

    /// Restore from a snapshot
    fn restore(&mut self, state: HashMap<String, i64>) -> Result<()>;
}
