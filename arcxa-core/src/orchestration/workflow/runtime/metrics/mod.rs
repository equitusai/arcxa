//! Runtime execution metrics shared by future batch and stream operators.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RuntimeStepMetrics {
    pub input_rows: usize,
    pub output_rows: usize,
    pub materialization_count: usize,
    pub spill_bytes: usize,
    pub pushdown_applied: bool,
}
