//! Deterministic spill policy and storage selection types.

mod policy;
mod tiering;

pub use policy::{SpillBackend, SpillDecision, SpillPolicy, SpillThresholds};
pub use tiering::{StorageTieringPlan, StorageTieringPolicy, StorageTieringThresholds};
