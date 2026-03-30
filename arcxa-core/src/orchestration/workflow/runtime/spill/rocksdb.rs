use std::sync::Arc;
use std::time::Instant;

use rocksdb::{IteratorMode, WriteBatch, DB};

use crate::orchestration::workflow::error::{Result, WorkflowError};
use crate::orchestration::workflow::row_storage::RowStorageHandle;

pub(crate) struct RocksDbStorage {
    pub handle: Arc<RowStorageHandle>,
    pub prefix: String,
    pub row_count: usize,
}

pub(crate) fn create_rocks_storage(
    db: Arc<DB>,
    execution_id: &str,
    step_id: &str,
    rows: Vec<serde_json::Value>,
) -> Result<RocksDbStorage> {
    let prefix = format!("{}/{}", execution_id, step_id);
    let row_count = rows.len();

    let mut batch = WriteBatch::default();
    for (index, row) in rows.iter().enumerate() {
        let key = format!("{}/{}", prefix, index);
        let value =
            serde_json::to_vec(row).map_err(|e| WorkflowError::Serialization(e.to_string()))?;
        batch.put(key.as_bytes(), &value);
    }

    db.write(batch)
        .map_err(|e| WorkflowError::Storage(e.to_string()))?;

    let handle = Arc::new(RowStorageHandle {
        db,
        execution_id: execution_id.to_string(),
        step_id: step_id.to_string(),
        created_at: Instant::now(),
    });

    Ok(RocksDbStorage {
        handle,
        prefix,
        row_count,
    })
}

pub(crate) fn delete_rocks_prefix(db: &DB, prefix: &str) -> Result<()> {
    let prefix_bytes = prefix.as_bytes();
    let iter = db.iterator(IteratorMode::From(
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

    db.write(batch)
        .map_err(|e| WorkflowError::Storage(e.to_string()))?;

    Ok(())
}
