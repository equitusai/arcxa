#![cfg(feature = "workflow-storage")]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::RwLock;
use rocksdb::DB;

use crate::orchestration::workflow::error::{Result, WorkflowError};
use crate::orchestration::workflow::row_storage::{
    estimate_memory_size, RowStorage, RowStorageHandle,
};

use super::parquet as parquet_backend;
use super::rocksdb as rocksdb_backend;
use super::{store_inline_rows, StorageTieringPlan, StorageTieringPolicy};

#[allow(dead_code)]
enum StorageEntry {
    RocksDB(Arc<RowStorageHandle>),
    Parquet(PathBuf),
}

#[derive(Debug, Clone)]
pub struct SpillQuotaConfig {
    pub max_total_spill_bytes: usize,
    pub max_spill_bytes_per_execution: usize,
}

impl Default for SpillQuotaConfig {
    fn default() -> Self {
        Self {
            max_total_spill_bytes: 20 * 1024 * 1024 * 1024,
            max_spill_bytes_per_execution: 4 * 1024 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpillQuotaUsage {
    pub total_reserved_bytes: usize,
    pub reserved_bytes_by_execution: HashMap<String, usize>,
}

#[derive(Debug, Clone)]
pub struct StoragePlacementOutcome {
    pub reserved_spill_bytes: usize,
    pub execution_reserved_spill_bytes: usize,
    pub total_reserved_spill_bytes: usize,
    pub storage_location: Option<String>,
    pub storage: RowStorage,
}

struct ManagedStorageEntry {
    storage: StorageEntry,
    execution_id: String,
    reserved_bytes: usize,
}

/// Storage manager for lifecycle management.
pub struct StorageManager {
    /// RocksDB instance (shared across workflows).
    rocks_db: Arc<DB>,

    /// Temporary directory for Parquet files and temp RocksDBs.
    pub temp_dir: PathBuf,

    /// Spill quota configuration.
    quota_config: SpillQuotaConfig,

    /// Reserved spill bytes by execution and in total.
    spill_quota_usage: Arc<RwLock<SpillQuotaUsage>>,

    /// Active storage handles for cleanup.
    active_storage: Arc<RwLock<HashMap<String, ManagedStorageEntry>>>,
}

impl StorageManager {
    pub fn new(rocks_path: &Path, temp_dir: &Path) -> Result<Self> {
        Self::with_quota_config(rocks_path, temp_dir, SpillQuotaConfig::default())
    }

    pub fn with_quota_config(
        rocks_path: &Path,
        temp_dir: &Path,
        quota_config: SpillQuotaConfig,
    ) -> Result<Self> {
        let rocks_db = Arc::new(
            DB::open_default(rocks_path).map_err(|e| WorkflowError::Storage(e.to_string()))?,
        );

        Ok(Self {
            rocks_db,
            temp_dir: temp_dir.to_path_buf(),
            quota_config,
            spill_quota_usage: Arc::new(RwLock::new(SpillQuotaUsage::default())),
            active_storage: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    pub fn quota_config(&self) -> &SpillQuotaConfig {
        &self.quota_config
    }

    pub fn spill_quota_usage(&self) -> SpillQuotaUsage {
        self.spill_quota_usage.read().clone()
    }

    /// Get RocksDB handle for external use (e.g., streaming deduplicator).
    pub fn rocks_db(&self) -> Arc<DB> {
        self.rocks_db.clone()
    }

    fn reserve_spill_quota(&self, execution_id: &str, bytes: usize) -> Result<SpillQuotaUsage> {
        let mut usage = self.spill_quota_usage.write();
        let current_execution_bytes = usage
            .reserved_bytes_by_execution
            .get(execution_id)
            .copied()
            .unwrap_or(0);

        let new_total = usage.total_reserved_bytes.saturating_add(bytes);
        if new_total > self.quota_config.max_total_spill_bytes {
            return Err(WorkflowError::ResourceLimit(format!(
                "Spill quota exceeded: reserving {} bytes for execution '{}' would exceed total spill quota {} bytes (current total {})",
                bytes,
                execution_id,
                self.quota_config.max_total_spill_bytes,
                usage.total_reserved_bytes
            )));
        }

        let new_execution_total = current_execution_bytes.saturating_add(bytes);
        if new_execution_total > self.quota_config.max_spill_bytes_per_execution {
            return Err(WorkflowError::ResourceLimit(format!(
                "Spill quota exceeded: reserving {} bytes for execution '{}' would exceed per-execution spill quota {} bytes (current execution total {})",
                bytes,
                execution_id,
                self.quota_config.max_spill_bytes_per_execution,
                current_execution_bytes
            )));
        }

        usage.total_reserved_bytes = new_total;
        usage
            .reserved_bytes_by_execution
            .insert(execution_id.to_string(), new_execution_total);
        Ok(usage.clone())
    }

    fn reconcile_spill_quota(
        &self,
        execution_id: &str,
        previous_bytes: usize,
        actual_bytes: usize,
    ) -> Result<SpillQuotaUsage> {
        if actual_bytes == previous_bytes {
            return Ok(self.spill_quota_usage());
        }

        if actual_bytes > previous_bytes {
            let additional_bytes = actual_bytes - previous_bytes;
            return self.reserve_spill_quota(execution_id, additional_bytes);
        }

        self.release_spill_quota(execution_id, previous_bytes - actual_bytes);
        Ok(self.spill_quota_usage())
    }

    fn release_spill_quota(&self, execution_id: &str, bytes: usize) {
        let mut usage = self.spill_quota_usage.write();
        usage.total_reserved_bytes = usage.total_reserved_bytes.saturating_sub(bytes);

        if let Some(current_execution_bytes) =
            usage.reserved_bytes_by_execution.get_mut(execution_id)
        {
            *current_execution_bytes = current_execution_bytes.saturating_sub(bytes);
            if *current_execution_bytes == 0 {
                usage.reserved_bytes_by_execution.remove(execution_id);
            }
        }
    }

    fn build_placement_outcome(
        &self,
        execution_id: &str,
        reserved_spill_bytes: usize,
        storage_location: Option<String>,
        storage: RowStorage,
    ) -> StoragePlacementOutcome {
        let usage = self.spill_quota_usage();
        StoragePlacementOutcome {
            reserved_spill_bytes,
            execution_reserved_spill_bytes: usage
                .reserved_bytes_by_execution
                .get(execution_id)
                .copied()
                .unwrap_or(0),
            total_reserved_spill_bytes: usage.total_reserved_bytes,
            storage_location,
            storage,
        }
    }

    pub fn create_rocks_storage(
        &self,
        execution_id: &str,
        step_id: &str,
        rows: Vec<serde_json::Value>,
    ) -> Result<RowStorage> {
        self.create_rocks_storage_with_details(execution_id, step_id, rows)
            .map(|outcome| outcome.storage)
    }

    pub fn create_rocks_storage_with_details(
        &self,
        execution_id: &str,
        step_id: &str,
        rows: Vec<serde_json::Value>,
    ) -> Result<StoragePlacementOutcome> {
        let reserved_bytes = estimate_memory_size(&rows);
        self.reserve_spill_quota(execution_id, reserved_bytes)?;

        let rocks_storage = rocksdb_backend::create_rocks_storage(
            self.rocks_db.clone(),
            execution_id,
            step_id,
            rows,
        )
        .map_err(|err| {
            self.release_spill_quota(execution_id, reserved_bytes);
            err
        })?;

        self.active_storage.write().insert(
            rocks_storage.prefix.clone(),
            ManagedStorageEntry {
                storage: StorageEntry::RocksDB(rocks_storage.handle.clone()),
                execution_id: execution_id.to_string(),
                reserved_bytes,
            },
        );

        let storage_location = Some(rocks_storage.prefix.clone());
        let storage = RowStorage::RocksDB {
            handle: rocks_storage.handle,
            prefix: rocks_storage.prefix,
            row_count: rocks_storage.row_count,
        };

        Ok(self.build_placement_outcome(execution_id, reserved_bytes, storage_location, storage))
    }

    fn create_parquet_storage_with_details(
        &self,
        execution_id: &str,
        step_id: &str,
        rows: Vec<serde_json::Value>,
    ) -> Result<StoragePlacementOutcome> {
        let estimated_reserved_bytes = estimate_memory_size(&rows);
        self.reserve_spill_quota(execution_id, estimated_reserved_bytes)?;

        let parquet_storage =
            parquet_backend::create_parquet_storage(&self.temp_dir, execution_id, step_id, &rows)
                .map_err(|err| {
                self.release_spill_quota(execution_id, estimated_reserved_bytes);
                err
            })?;

        let reserved_bytes = parquet_storage.file_size_bytes;
        if let Err(err) =
            self.reconcile_spill_quota(execution_id, estimated_reserved_bytes, reserved_bytes)
        {
            let _ = std::fs::remove_file(&parquet_storage.path);
            self.release_spill_quota(execution_id, estimated_reserved_bytes);
            return Err(err);
        }

        let storage_location = parquet_storage.path.display().to_string();
        let storage_key = format!(
            "{}/parquet/{}",
            execution_id,
            parquet_storage
                .path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("spill.parquet")
        );
        self.active_storage.write().insert(
            storage_key,
            ManagedStorageEntry {
                storage: StorageEntry::Parquet(parquet_storage.path.clone()),
                execution_id: execution_id.to_string(),
                reserved_bytes,
            },
        );

        let storage = RowStorage::Parquet {
            path: parquet_storage.path,
            schema: parquet_storage.schema,
            row_count: parquet_storage.row_count,
            index: parquet_storage.index,
        };

        Ok(self.build_placement_outcome(
            execution_id,
            reserved_bytes,
            Some(storage_location),
            storage,
        ))
    }

    pub fn store_rows(
        &self,
        execution_id: &str,
        step_id: &str,
        rows: Vec<serde_json::Value>,
    ) -> Result<RowStorage> {
        self.store_rows_with_details(execution_id, step_id, rows)
            .map(|outcome| outcome.storage)
    }

    /// Store rows with automatic tiering.
    pub fn store_rows_with_details(
        &self,
        execution_id: &str,
        step_id: &str,
        rows: Vec<serde_json::Value>,
    ) -> Result<StoragePlacementOutcome> {
        let row_count = rows.len();
        let estimated_size = estimate_memory_size(&rows);
        let plan = StorageTieringPolicy::default().plan(row_count, estimated_size);

        match plan {
            StorageTieringPlan::InMemory | StorageTieringPlan::Shared => {
                let storage = store_inline_rows(rows, plan, row_count, estimated_size);
                Ok(StoragePlacementOutcome {
                    reserved_spill_bytes: 0,
                    execution_reserved_spill_bytes: self
                        .spill_quota_usage()
                        .reserved_bytes_by_execution
                        .get(execution_id)
                        .copied()
                        .unwrap_or(0),
                    total_reserved_spill_bytes: self.spill_quota_usage().total_reserved_bytes,
                    storage_location: None,
                    storage,
                })
            }
            StorageTieringPlan::RocksDb => {
                self.create_rocks_storage_with_details(execution_id, step_id, rows)
            }
            StorageTieringPlan::Parquet => {
                match self.create_parquet_storage_with_details(execution_id, step_id, rows.clone())
                {
                    Ok(outcome) => Ok(outcome),
                    Err(parquet_error) => {
                        tracing::warn!(
                            error = %parquet_error,
                            execution_id,
                            step_id,
                            row_count,
                            estimated_size,
                            "Parquet spill failed; falling back to RocksDB"
                        );
                        self.create_rocks_storage_with_details(execution_id, step_id, rows)
                    }
                }
            }
        }
    }

    /// Cleanup storage for an execution.
    pub fn cleanup_execution(&self, execution_id: &str) -> Result<()> {
        let mut storage = self.active_storage.write();
        let prefix = format!("{}/", execution_id);

        let keys_to_remove: Vec<String> = storage
            .keys()
            .filter(|key| key.starts_with(&prefix))
            .cloned()
            .collect();

        for key in keys_to_remove {
            if let Some(entry) = storage.remove(&key) {
                self.release_spill_quota(&entry.execution_id, entry.reserved_bytes);
                match entry.storage {
                    StorageEntry::RocksDB(handle) => {
                        let _ = &handle;
                        rocksdb_backend::delete_rocks_prefix(&self.rocks_db, &key)?;
                    }
                    StorageEntry::Parquet(path) => {
                        if path.exists() {
                            std::fs::remove_file(path)
                                .map_err(|e| WorkflowError::Storage(e.to_string()))?;
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use crate::orchestration::workflow::row_storage::{RowAccessor, StorageType};

    use super::{SpillQuotaConfig, StorageManager};

    #[test]
    fn enforces_total_spill_quota() {
        let rocks_dir = tempdir().unwrap();
        let temp_dir = tempdir().unwrap();
        let manager = StorageManager::with_quota_config(
            rocks_dir.path(),
            temp_dir.path(),
            SpillQuotaConfig {
                max_total_spill_bytes: 64,
                max_spill_bytes_per_execution: 64,
            },
        )
        .unwrap();

        let rows = vec![serde_json::json!({
            "value": "this row uses more than sixty four bytes when serialized"
        })];
        let error = manager
            .create_rocks_storage("exec_quota", "step_1", rows)
            .expect_err("quota to be enforced");

        assert!(error.to_string().contains("Spill quota exceeded"));
        assert_eq!(manager.spill_quota_usage().total_reserved_bytes, 0);
    }

    #[test]
    fn cleanup_execution_releases_reserved_quota() {
        let rocks_dir = tempdir().unwrap();
        let temp_dir = tempdir().unwrap();
        let manager = StorageManager::with_quota_config(
            rocks_dir.path(),
            temp_dir.path(),
            SpillQuotaConfig {
                max_total_spill_bytes: 512,
                max_spill_bytes_per_execution: 512,
            },
        )
        .unwrap();

        manager
            .create_rocks_storage(
                "exec_cleanup",
                "step_1",
                vec![serde_json::json!({"value": "first spill reservation"})],
            )
            .unwrap();

        let usage_after_store = manager.spill_quota_usage();
        assert!(usage_after_store.total_reserved_bytes > 0);
        assert!(usage_after_store
            .reserved_bytes_by_execution
            .contains_key("exec_cleanup"));

        manager.cleanup_execution("exec_cleanup").unwrap();

        let usage_after_cleanup = manager.spill_quota_usage();
        assert_eq!(usage_after_cleanup.total_reserved_bytes, 0);
        assert!(usage_after_cleanup.reserved_bytes_by_execution.is_empty());
    }

    #[test]
    fn creates_parquet_storage_and_reconciles_quota_usage() {
        let rocks_dir = tempdir().unwrap();
        let temp_dir = tempdir().unwrap();
        let manager = StorageManager::with_quota_config(
            rocks_dir.path(),
            temp_dir.path(),
            SpillQuotaConfig {
                max_total_spill_bytes: 1024 * 1024,
                max_spill_bytes_per_execution: 1024 * 1024,
            },
        )
        .unwrap();

        let rows = vec![
            serde_json::json!({"id": 1, "name": "alpha", "active": true}),
            serde_json::json!({"id": 2, "name": "beta", "active": false}),
            serde_json::json!({"id": 3, "name": "gamma", "active": true}),
        ];
        let outcome = manager
            .create_parquet_storage_with_details("exec_parquet", "step_1", rows.clone())
            .expect("parquet storage to be created");

        assert_eq!(outcome.storage.storage_type(), StorageType::Parquet);
        assert!(outcome.reserved_spill_bytes > 0);
        assert!(outcome.storage_location.is_some());

        let materialized = RowAccessor::new(outcome.storage.clone()).to_vec().unwrap();
        assert_eq!(materialized, rows);

        let usage_after_store = manager.spill_quota_usage();
        assert_eq!(
            usage_after_store.total_reserved_bytes,
            outcome.total_reserved_spill_bytes
        );
        assert_eq!(
            usage_after_store
                .reserved_bytes_by_execution
                .get("exec_parquet")
                .copied()
                .unwrap_or(0),
            outcome.execution_reserved_spill_bytes
        );

        manager.cleanup_execution("exec_parquet").unwrap();
        let usage_after_cleanup = manager.spill_quota_usage();
        assert_eq!(usage_after_cleanup.total_reserved_bytes, 0);
    }
}
