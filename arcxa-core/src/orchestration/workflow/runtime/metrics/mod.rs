//! Runtime execution metrics shared by future batch and stream operators.

use serde::{Deserialize, Serialize};

use crate::orchestration::workflow::row_storage::StorageType;
use crate::orchestration::workflow::runtime::spill::StorageTieringPlan;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RuntimeStepMetrics {
    pub input_rows: usize,
    pub output_rows: usize,
    pub materialization_count: usize,
    pub spill_events: usize,
    pub spill_bytes: usize,
    pub memory_high_water_mark: usize,
    pub storage_type: Option<String>,
    pub storage_operation: Option<String>,
    pub planned_tier: Option<String>,
    pub storage_decision_reason: Option<String>,
    pub reserved_spill_bytes: usize,
    pub execution_reserved_spill_bytes: usize,
    pub total_reserved_spill_bytes: usize,
    pub storage_location: Option<String>,
    pub pushdown_applied: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageDecisionReason {
    Planned,
    StorageManagerUnavailable,
    ParquetFallbackToRocksDb,
    MemoryPressureSpill,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageDecisionMetric {
    pub operation: String,
    pub planned_tier: StorageTieringPlan,
    pub actual_storage_type: StorageType,
    pub row_count: usize,
    pub estimated_bytes: usize,
    pub reason: StorageDecisionReason,
    pub reserved_spill_bytes: usize,
    pub execution_reserved_spill_bytes: usize,
    pub total_reserved_spill_bytes: usize,
    pub storage_location: Option<String>,
}
