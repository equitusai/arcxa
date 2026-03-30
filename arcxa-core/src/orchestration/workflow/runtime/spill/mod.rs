//! Deterministic spill policy and storage selection types.

mod manager;
mod memory;
pub(crate) mod parquet;
mod policy;
#[cfg(feature = "workflow-storage")]
mod rocksdb;
mod tiering;

#[cfg(feature = "workflow-storage")]
pub use manager::{SpillQuotaConfig, SpillQuotaUsage, StorageManager, StoragePlacementOutcome};
pub(crate) use memory::store_inline_rows;
pub use policy::{SpillBackend, SpillDecision, SpillPolicy, SpillThresholds};
pub use tiering::{StorageTieringPlan, StorageTieringPolicy, StorageTieringThresholds};
