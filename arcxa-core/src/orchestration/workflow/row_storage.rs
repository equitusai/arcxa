//! Row storage abstraction for efficient workflow data handling
//!
//! This module provides a tiered storage system that automatically selects
//! the most efficient backend based on dataset size, reducing memory overhead
//! and eliminating expensive clone operations.

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[cfg(feature = "workflow-storage")]
use rocksdb::{IteratorMode, WriteBatch, DB};

use super::error::{Result, WorkflowError};
use super::runtime::spill::{StorageTieringPlan, StorageTieringPolicy};
use arrow2::datatypes::Schema;

/// Row storage abstraction with automatic tiering
#[derive(Debug, Clone)]
pub enum RowStorage {
    /// Small datasets kept in memory (< 10K rows)
    InMemory { rows: Arc<Vec<serde_json::Value>> },

    /// Medium datasets with reference counting (10K - 100K rows)
    Shared {
        rows: Arc<RwLock<Vec<serde_json::Value>>>,
        version: u64,
    },

    /// Large datasets in RocksDB (100K - 1M rows)
    #[cfg(feature = "workflow-storage")]
    RocksDB {
        handle: Arc<RowStorageHandle>,
        prefix: String,
        row_count: usize,
    },

    /// Extra large datasets in Parquet (> 1M rows)
    Parquet {
        path: PathBuf,
        schema: Arc<arrow2::datatypes::Schema>,
        row_count: usize,
        index: Arc<BTreeMap<usize, u64>>,
    },
}

/// Handle for RocksDB-backed row storage
#[cfg(feature = "workflow-storage")]
#[derive(Debug)]
pub struct RowStorageHandle {
    pub db: Arc<DB>,
    pub execution_id: String,
    pub step_id: String,
    pub created_at: Instant,
}

/// Storage type indicator
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum StorageType {
    InMemory,
    Shared,
    #[cfg(feature = "workflow-storage")]
    RocksDB,
    Parquet,
}

impl std::fmt::Display for StorageType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StorageType::InMemory => write!(f, "in_memory"),
            StorageType::Shared => write!(f, "shared"),
            #[cfg(feature = "workflow-storage")]
            StorageType::RocksDB => write!(f, "rocksdb"),
            StorageType::Parquet => write!(f, "parquet"),
        }
    }
}

/// Reference to rows with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RowReference {
    /// Storage backend type
    pub storage_type: StorageType,

    /// Unique identifier for this dataset
    pub dataset_id: String,

    /// Number of rows
    pub row_count: usize,

    /// Estimated memory size in bytes
    pub memory_size: usize,

    /// Optional schema for typed access
    pub schema: Option<DataSchema>,
}

/// Simplified schema for JSON data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataSchema {
    pub fields: Vec<FieldSchema>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldSchema {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
}

impl RowStorage {
    fn inline_storage_from_plan(
        rows: Vec<serde_json::Value>,
        plan: StorageTieringPlan,
        row_count: usize,
        estimated_size: usize,
    ) -> Self {
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

    /// Create storage from rows with automatic tiering
    pub fn from_rows(rows: Vec<serde_json::Value>) -> Result<Self> {
        let row_count = rows.len();
        let estimated_size = estimate_memory_size(&rows);
        let plan = StorageTieringPolicy::default().plan(row_count, estimated_size);

        Ok(Self::inline_storage_from_plan(
            rows,
            plan,
            row_count,
            estimated_size,
        ))
    }

    /// Get the number of rows
    pub fn len(&self) -> usize {
        match self {
            RowStorage::InMemory { rows } => rows.len(),
            RowStorage::Shared { rows, .. } => rows.read().len(),
            #[cfg(feature = "workflow-storage")]
            RowStorage::RocksDB { row_count, .. } => *row_count,
            RowStorage::Parquet { row_count, .. } => *row_count,
        }
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get storage type
    pub fn storage_type(&self) -> StorageType {
        match self {
            RowStorage::InMemory { .. } => StorageType::InMemory,
            RowStorage::Shared { .. } => StorageType::Shared,
            #[cfg(feature = "workflow-storage")]
            RowStorage::RocksDB { .. } => StorageType::RocksDB,
            RowStorage::Parquet { .. } => StorageType::Parquet,
        }
    }

    /// Estimate memory usage
    pub fn memory_usage(&self) -> usize {
        match self {
            RowStorage::InMemory { rows } => estimate_memory_size(rows),
            RowStorage::Shared { rows, .. } => estimate_memory_size(&*rows.read()),
            #[cfg(feature = "workflow-storage")]
            RowStorage::RocksDB { .. } => 0, // Data is on disk
            RowStorage::Parquet { .. } => 0, // Data is on disk
        }
    }
}

/// Iterator for streaming row access
pub struct RowIterator {
    storage: RowStorage,
    current: usize,
    batch_size: usize,
    buffer: Vec<serde_json::Value>,
}

impl RowIterator {
    pub fn new(storage: RowStorage) -> Self {
        Self::with_batch_size(storage, 1000)
    }

    pub fn with_batch_size(storage: RowStorage, batch_size: usize) -> Self {
        Self {
            storage,
            current: 0,
            batch_size,
            buffer: Vec::new(),
        }
    }

    fn fetch_next_batch(&mut self) -> Result<()> {
        match &self.storage {
            #[cfg(feature = "workflow-storage")]
            RowStorage::RocksDB {
                handle,
                prefix,
                row_count,
            } => {
                let end = std::cmp::min(self.current + self.batch_size, *row_count);
                for i in self.current..end {
                    let key = format!("{}/{}", prefix, i);
                    if let Some(value) = handle
                        .db
                        .get(key.as_bytes())
                        .map_err(|e| WorkflowError::Storage(e.to_string()))?
                    {
                        let row: serde_json::Value = serde_json::from_slice(&value)
                            .map_err(|e| WorkflowError::Serialization(e.to_string()))?;
                        self.buffer.push(row);
                    }
                }
                self.current = end;
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

impl Iterator for RowIterator {
    type Item = Result<serde_json::Value>;

    fn next(&mut self) -> Option<Self::Item> {
        match &self.storage {
            RowStorage::InMemory { rows } => {
                if self.current < rows.len() {
                    let row = rows[self.current].clone();
                    self.current += 1;
                    Some(Ok(row))
                } else {
                    None
                }
            }
            RowStorage::Shared { rows, .. } => {
                let rows_guard = rows.read();
                if self.current < rows_guard.len() {
                    let row = rows_guard[self.current].clone();
                    self.current += 1;
                    Some(Ok(row))
                } else {
                    None
                }
            }
            #[cfg(feature = "workflow-storage")]
            RowStorage::RocksDB { row_count, .. } => {
                if self.buffer.is_empty() && self.current < *row_count {
                    if let Err(e) = self.fetch_next_batch() {
                        return Some(Err(e));
                    }
                }
                self.buffer.pop().map(Ok)
            }
            RowStorage::Parquet { .. } => {
                // TODO: Implement Parquet streaming
                None
            }
        }
    }
}

/// Batch iterator for efficient processing
pub struct BatchIterator {
    iter: RowIterator,
    batch_size: usize,
}

impl BatchIterator {
    pub fn new(storage: RowStorage, batch_size: usize) -> Self {
        Self {
            iter: RowIterator::with_batch_size(storage, batch_size),
            batch_size,
        }
    }
}

impl Iterator for BatchIterator {
    type Item = Result<Vec<serde_json::Value>>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut batch = Vec::with_capacity(self.batch_size);

        for _ in 0..self.batch_size {
            match self.iter.next() {
                Some(Ok(row)) => batch.push(row),
                Some(Err(e)) => return Some(Err(e)),
                None => break,
            }
        }

        if batch.is_empty() {
            None
        } else {
            Some(Ok(batch))
        }
    }
}

/// Accessor for backwards-compatible row access
pub struct RowAccessor {
    storage: RowStorage,
}

impl RowAccessor {
    pub fn new(storage: RowStorage) -> Self {
        Self { storage }
    }

    pub fn from_json(value: serde_json::Value) -> Self {
        let rows = value.as_array().cloned().unwrap_or_default();
        Self {
            storage: RowStorage::InMemory {
                rows: Arc::new(rows),
            },
        }
    }

    /// Stream rows (zero-copy for read operations)
    pub fn iter(&self) -> RowIterator {
        RowIterator::new(self.storage.clone())
    }

    /// Stream rows in batches
    pub fn iter_batches(&self, batch_size: usize) -> BatchIterator {
        BatchIterator::new(self.storage.clone(), batch_size)
    }

    /// Get all rows (materializes if needed - for backwards compatibility)
    pub fn to_vec(&self) -> Result<Vec<serde_json::Value>> {
        match &self.storage {
            RowStorage::InMemory { rows } => Ok((**rows).clone()),
            RowStorage::Shared { rows, .. } => Ok(rows.read().clone()),
            #[cfg(feature = "workflow-storage")]
            RowStorage::RocksDB {
                handle,
                prefix,
                row_count,
            } => {
                let mut result = Vec::with_capacity(*row_count);
                for i in 0..*row_count {
                    let key = format!("{}/{}", prefix, i);
                    if let Some(value) = handle
                        .db
                        .get(key.as_bytes())
                        .map_err(|e| WorkflowError::Storage(e.to_string()))?
                    {
                        let row: serde_json::Value = serde_json::from_slice(&value)
                            .map_err(|e| WorkflowError::Serialization(e.to_string()))?;
                        result.push(row);
                    }
                }
                Ok(result)
            }
            RowStorage::Parquet { .. } => {
                // TODO: Implement Parquet reading
                Err(WorkflowError::NotImplemented(
                    "Parquet reading not yet implemented".into(),
                ))
            }
        }
    }

    /// Get a single row by index (efficient random access)
    pub fn get(&self, index: usize) -> Result<Option<serde_json::Value>> {
        match &self.storage {
            RowStorage::InMemory { rows } => Ok(rows.get(index).cloned()),
            RowStorage::Shared { rows, .. } => Ok(rows.read().get(index).cloned()),
            #[cfg(feature = "workflow-storage")]
            RowStorage::RocksDB { handle, prefix, .. } => {
                let key = format!("{}/{}", prefix, index);
                handle
                    .db
                    .get(key.as_bytes())
                    .map_err(|e| WorkflowError::Storage(e.to_string()))?
                    .map(|v| serde_json::from_slice(&v))
                    .transpose()
                    .map_err(|e| WorkflowError::Serialization(e.to_string()))
            }
            RowStorage::Parquet { .. } => Err(WorkflowError::NotImplemented(
                "Parquet random access not yet implemented".into(),
            )),
        }
    }

    /// Get row count without materialization
    pub fn len(&self) -> usize {
        self.storage.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.storage.is_empty()
    }
}

/// Storage manager for lifecycle management
#[cfg(feature = "workflow-storage")]
pub struct StorageManager {
    /// RocksDB instance (shared across workflows)
    rocks_db: Arc<DB>,

    /// Temporary directory for Parquet files
    pub temp_dir: PathBuf,

    /// Active storage handles for cleanup
    active_storage: Arc<RwLock<HashMap<String, StorageEntry>>>,
}

#[cfg(feature = "workflow-storage")]
enum StorageEntry {
    RocksDB(Arc<RowStorageHandle>),
    Parquet(PathBuf),
}

#[cfg(feature = "workflow-storage")]
impl StorageManager {
    pub fn new(rocks_path: &Path, temp_dir: &Path) -> Result<Self> {
        let rocks_db = Arc::new(
            DB::open_default(rocks_path).map_err(|e| WorkflowError::Storage(e.to_string()))?,
        );

        Ok(Self {
            rocks_db,
            temp_dir: temp_dir.to_path_buf(),
            active_storage: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Get RocksDB handle for external use (e.g., streaming deduplicator)
    pub fn rocks_db(&self) -> Arc<DB> {
        self.rocks_db.clone()
    }

    /// Create RocksDB storage for a step
    #[cfg(feature = "workflow-storage")]
    pub fn create_rocks_storage(
        &self,
        execution_id: &str,
        step_id: &str,
        rows: Vec<serde_json::Value>,
    ) -> Result<RowStorage> {
        let prefix = format!("{}/{}", execution_id, step_id);
        let row_count = rows.len();

        // Write rows to RocksDB
        let mut batch = WriteBatch::default();
        for (i, row) in rows.iter().enumerate() {
            let key = format!("{}/{}", prefix, i);
            let value =
                serde_json::to_vec(row).map_err(|e| WorkflowError::Serialization(e.to_string()))?;
            batch.put(key.as_bytes(), &value);
        }

        self.rocks_db
            .write(batch)
            .map_err(|e| WorkflowError::Storage(e.to_string()))?;

        let handle = Arc::new(RowStorageHandle {
            db: self.rocks_db.clone(),
            execution_id: execution_id.to_string(),
            step_id: step_id.to_string(),
            created_at: Instant::now(),
        });

        // Track for cleanup
        self.active_storage
            .write()
            .insert(prefix.clone(), StorageEntry::RocksDB(handle.clone()));

        Ok(RowStorage::RocksDB {
            handle,
            prefix,
            row_count,
        })
    }

    /// Store rows with automatic tiering
    pub fn store_rows(
        &self,
        execution_id: &str,
        step_id: &str,
        rows: Vec<serde_json::Value>,
    ) -> Result<RowStorage> {
        let row_count = rows.len();
        let estimated_size = estimate_memory_size(&rows);
        let plan = StorageTieringPolicy::default().plan(row_count, estimated_size);

        match plan {
            StorageTieringPlan::InMemory | StorageTieringPlan::Shared => Ok(
                RowStorage::inline_storage_from_plan(rows, plan, row_count, estimated_size),
            ),
            StorageTieringPlan::RocksDb => self.create_rocks_storage(execution_id, step_id, rows),
            StorageTieringPlan::Parquet => {
                // TODO: Implement Parquet storage
                tracing::warn!(
                    "Dataset planned for Parquet storage ({} rows, {} bytes) is falling back to RocksDB until Parquet spill is implemented.",
                    row_count,
                    estimated_size
                );
                self.create_rocks_storage(execution_id, step_id, rows)
            }
        }
    }

    /// Cleanup storage for an execution
    pub fn cleanup_execution(&self, execution_id: &str) -> Result<()> {
        let mut storage = self.active_storage.write();
        let prefix = format!("{}/", execution_id);

        // Find all entries for this execution
        let keys_to_remove: Vec<String> = storage
            .keys()
            .filter(|k| k.starts_with(&prefix))
            .cloned()
            .collect();

        for key in keys_to_remove {
            if let Some(entry) = storage.remove(&key) {
                match entry {
                    StorageEntry::RocksDB(_) => {
                        // Delete all keys with this prefix from RocksDB
                        self.delete_rocks_prefix(&key)?;
                    }
                    StorageEntry::Parquet(path) => {
                        // Delete Parquet file
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

    fn delete_rocks_prefix(&self, prefix: &str) -> Result<()> {
        let prefix_bytes = prefix.as_bytes();
        let iter = self.rocks_db.iterator(IteratorMode::From(
            prefix_bytes,
            rocksdb::Direction::Forward,
        ));

        let mut batch = WriteBatch::default();
        for item in iter {
            let (key, _) = item.map_err(|e| WorkflowError::Storage(e.to_string()))?;
            if !key.starts_with(prefix_bytes) {
                break;
            }
            batch.delete(&key);
        }

        self.rocks_db
            .write(batch)
            .map_err(|e| WorkflowError::Storage(e.to_string()))?;

        Ok(())
    }
}

/// Estimate memory size of JSON values
pub fn estimate_memory_size(rows: &[serde_json::Value]) -> usize {
    rows.iter()
        .map(|row| serde_json::to_string(row).map(|s| s.len()).unwrap_or(0))
        .sum()
}

#[cfg(feature = "workflow-storage")]
impl Drop for RowStorageHandle {
    fn drop(&mut self) {
        // Log cleanup intent but don't actually delete
        // (StorageManager handles cleanup)
        tracing::trace!(
            "RowStorageHandle dropped for {}/{}",
            self.execution_id,
            self.step_id
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_storage_tiering() {
        // Small dataset -> InMemory
        let small = vec![json!({"id": 1}); 100];
        let storage = RowStorage::from_rows(small).unwrap();
        assert!(matches!(storage, RowStorage::InMemory { .. }));
        assert_eq!(storage.storage_type(), StorageType::InMemory);

        // Medium dataset -> Shared
        let medium = vec![json!({"id": 1}); 50_000];
        let storage = RowStorage::from_rows(medium).unwrap();
        assert!(matches!(storage, RowStorage::Shared { .. }));
        assert_eq!(storage.storage_type(), StorageType::Shared);
    }

    #[test]
    fn test_row_accessor_iteration() {
        let rows = vec![
            json!({"id": 1, "name": "Alice"}),
            json!({"id": 2, "name": "Bob"}),
            json!({"id": 3, "name": "Charlie"}),
        ];

        let storage = RowStorage::from_rows(rows.clone()).unwrap();
        let accessor = RowAccessor::new(storage);

        // Test iteration
        let collected: Vec<_> = accessor.iter().collect::<Result<Vec<_>>>().unwrap();
        assert_eq!(collected.len(), 3);
        assert_eq!(collected[0]["name"], "Alice");

        // Test materialization
        let materialized = accessor.to_vec().unwrap();
        assert_eq!(materialized, rows);

        // Test random access
        let second = accessor.get(1).unwrap().unwrap();
        assert_eq!(second["name"], "Bob");
    }

    #[test]
    fn test_batch_iterator() {
        let rows: Vec<_> = (0..10).map(|i| json!({"id": i})).collect();
        let storage = RowStorage::from_rows(rows).unwrap();
        let accessor = RowAccessor::new(storage);

        let batches: Vec<_> = accessor
            .iter_batches(3)
            .collect::<Result<Vec<_>>>()
            .unwrap();

        assert_eq!(batches.len(), 4); // 3 + 3 + 3 + 1
        assert_eq!(batches[0].len(), 3);
        assert_eq!(batches[3].len(), 1);
    }
}
