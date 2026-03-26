use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StorageTieringPlan {
    InMemory,
    Shared,
    RocksDb,
    Parquet,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageTieringThresholds {
    pub in_memory_row_limit: usize,
    pub shared_row_limit: usize,
    pub shared_byte_limit: usize,
    pub rocksdb_row_limit: usize,
}

impl Default for StorageTieringThresholds {
    fn default() -> Self {
        Self {
            in_memory_row_limit: 10_000,
            shared_row_limit: 100_000,
            shared_byte_limit: 500_000_000,
            rocksdb_row_limit: 1_000_000,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StorageTieringPolicy {
    pub thresholds: StorageTieringThresholds,
}

impl StorageTieringPolicy {
    pub fn plan(&self, row_count: usize, estimated_bytes: usize) -> StorageTieringPlan {
        let thresholds = &self.thresholds;

        if row_count < thresholds.in_memory_row_limit {
            return StorageTieringPlan::InMemory;
        }

        if row_count < thresholds.shared_row_limit && estimated_bytes < thresholds.shared_byte_limit
        {
            return StorageTieringPlan::Shared;
        }

        if row_count < thresholds.rocksdb_row_limit {
            return StorageTieringPlan::RocksDb;
        }

        StorageTieringPlan::Parquet
    }
}

#[cfg(test)]
mod tests {
    use super::{StorageTieringPlan, StorageTieringPolicy};

    #[test]
    fn chooses_in_memory_for_small_payloads() {
        let policy = StorageTieringPolicy::default();
        let plan = policy.plan(500, 1024);
        assert_eq!(plan, StorageTieringPlan::InMemory);
    }

    #[test]
    fn chooses_shared_for_mid_sized_payloads() {
        let policy = StorageTieringPolicy::default();
        let plan = policy.plan(50_000, 128 * 1024 * 1024);
        assert_eq!(plan, StorageTieringPlan::Shared);
    }

    #[test]
    fn chooses_rocksdb_for_large_payloads() {
        let policy = StorageTieringPolicy::default();
        let plan = policy.plan(150_000, 600 * 1024 * 1024);
        assert_eq!(plan, StorageTieringPlan::RocksDb);
    }

    #[test]
    fn chooses_parquet_for_extra_large_payloads() {
        let policy = StorageTieringPolicy::default();
        let plan = policy.plan(2_000_000, 3 * 1024 * 1024 * 1024usize);
        assert_eq!(plan, StorageTieringPlan::Parquet);
    }
}
