use std::sync::Arc;

use parking_lot::RwLock;

use crate::orchestration::workflow::row_storage::RowStorage;

use super::StorageTieringPlan;

pub(crate) fn store_inline_rows(
    rows: Vec<serde_json::Value>,
    plan: StorageTieringPlan,
    row_count: usize,
    estimated_size: usize,
) -> RowStorage {
    match plan {
        StorageTieringPlan::InMemory => {
            tracing::debug!("Using InMemory storage for {} rows", row_count);
            RowStorage::InMemory {
                rows: Arc::new(rows),
            }
        }
        StorageTieringPlan::Shared => {
            tracing::debug!(
                "Using Shared storage for {} rows ({} bytes)",
                row_count,
                estimated_size
            );
            RowStorage::Shared {
                rows: Arc::new(RwLock::new(rows)),
                version: 0,
            }
        }
        StorageTieringPlan::RocksDb | StorageTieringPlan::Parquet => {
            tracing::warn!(
                "Dataset planned for {:?} storage ({} rows, {} bytes) is using Shared storage because StorageManager is not available.",
                plan,
                row_count,
                estimated_size
            );
            RowStorage::Shared {
                rows: Arc::new(RwLock::new(rows)),
                version: 0,
            }
        }
    }
}
