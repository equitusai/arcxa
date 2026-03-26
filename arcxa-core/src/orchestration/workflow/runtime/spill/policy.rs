use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpillBackend {
    InMemory,
    RocksDb,
    Parquet,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpillDecision {
    pub backend: SpillBackend,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpillThresholds {
    pub in_memory_row_limit: usize,
    pub in_memory_byte_limit: usize,
    pub rocksdb_row_limit: usize,
    pub rocksdb_byte_limit: usize,
}

impl Default for SpillThresholds {
    fn default() -> Self {
        Self {
            in_memory_row_limit: 10_000,
            in_memory_byte_limit: 256 * 1024 * 1024,
            rocksdb_row_limit: 1_000_000,
            rocksdb_byte_limit: 2 * 1024 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SpillPolicy {
    pub thresholds: SpillThresholds,
}

impl SpillPolicy {
    pub fn decide(&self, row_count: usize, estimated_bytes: usize) -> SpillDecision {
        let thresholds = &self.thresholds;

        if row_count <= thresholds.in_memory_row_limit
            && estimated_bytes <= thresholds.in_memory_byte_limit
        {
            return SpillDecision {
                backend: SpillBackend::InMemory,
                reason: format!(
                    "within in-memory limits (rows={}, bytes={})",
                    row_count, estimated_bytes
                ),
            };
        }

        if row_count <= thresholds.rocksdb_row_limit
            && estimated_bytes <= thresholds.rocksdb_byte_limit
        {
            return SpillDecision {
                backend: SpillBackend::RocksDb,
                reason: format!(
                    "exceeds in-memory limits but within RocksDB spill limits (rows={}, bytes={})",
                    row_count, estimated_bytes
                ),
            };
        }

        SpillDecision {
            backend: SpillBackend::Parquet,
            reason: format!(
                "exceeds RocksDB spill limits (rows={}, bytes={})",
                row_count, estimated_bytes
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{SpillBackend, SpillPolicy};

    #[test]
    fn chooses_memory_for_small_payloads() {
        let policy = SpillPolicy::default();
        let decision = policy.decide(500, 1024);
        assert_eq!(decision.backend, SpillBackend::InMemory);
    }

    #[test]
    fn chooses_rocksdb_for_mid_sized_payloads() {
        let policy = SpillPolicy::default();
        let decision = policy.decide(50_000, 512 * 1024 * 1024);
        assert_eq!(decision.backend, SpillBackend::RocksDb);
    }

    #[test]
    fn chooses_parquet_for_large_payloads() {
        let policy = SpillPolicy::default();
        let decision = policy.decide(2_000_000, 3 * 1024 * 1024 * 1024usize);
        assert_eq!(decision.backend, SpillBackend::Parquet);
    }
}
