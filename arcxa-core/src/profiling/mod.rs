//! # Profiling Module
//!
//! High-performance data profiling using SIMD and probabilistic data structures.

use hyperloglog::HyperLogLog;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Dataset profile statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetProfile {
    pub dataset: String,
    pub total_records: u64,
    pub column_profiles: HashMap<String, ColumnProfile>,
    pub computed_at: chrono::DateTime<chrono::Utc>,
}

/// Column-level profile
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnProfile {
    pub column_name: String,
    pub data_type: DataType,
    pub null_count: u64,
    pub distinct_count: u64,
    pub min_value: Option<String>,
    pub max_value: Option<String>,
    pub avg_length: Option<f64>,
    pub distribution: ValueDistribution,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DataType {
    String,
    Integer,
    Float,
    Boolean,
    DateTime,
    Json,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValueDistribution {
    pub top_values: Vec<(String, u64)>,
    pub histogram: Vec<HistogramBucket>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistogramBucket {
    pub lower: f64,
    pub upper: f64,
    pub count: u64,
}

/// Incremental profiler using sketches
pub struct IncrementalProfiler {
    hll: HyperLogLog,
    value_counts: HashMap<String, u64>,
    null_count: u64,
    total_count: u64,
}

impl IncrementalProfiler {
    pub fn new() -> Self {
        Self {
            hll: HyperLogLog::new(0.01), // 1% error rate
            value_counts: HashMap::new(),
            null_count: 0,
            total_count: 0,
        }
    }

    pub fn observe(&mut self, value: &serde_json::Value) {
        self.total_count += 1;

        if value.is_null() {
            self.null_count += 1;
            return;
        }

        let str_val = value.to_string();
        self.hll.insert(&str_val);

        *self.value_counts.entry(str_val).or_insert(0) += 1;
    }

    pub fn finalize(&self) -> (u64, u64, Vec<(String, u64)>) {
        // For small datasets (< 10k values), use exact count from HashMap
        // For larger datasets, use HyperLogLog estimate to save memory
        let distinct_count = if self.total_count < 10_000 {
            self.value_counts.len() as u64
        } else {
            self.hll.len() as u64
        };

        let mut top_values: Vec<_> = self
            .value_counts
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        top_values.sort_by(|a, b| b.1.cmp(&a.1));
        top_values.truncate(10);

        (self.null_count, distinct_count, top_values)
    }
}

impl Default for IncrementalProfiler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_incremental_profiler() {
        let mut profiler = IncrementalProfiler::new();

        profiler.observe(&serde_json::json!("apple"));
        profiler.observe(&serde_json::json!("banana"));
        profiler.observe(&serde_json::json!("apple"));
        profiler.observe(&serde_json::Value::Null);

        let (null_count, distinct_count, top_values) = profiler.finalize();

        assert_eq!(null_count, 1);
        assert_eq!(distinct_count, 2);
        assert_eq!(top_values.len(), 2);
    }
}
