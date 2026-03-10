//! Batch Job Storage
//!
//! RocksDB-backed persistent storage for batch jobs with indexing support.
//!
//! ## Storage Layout
//!
//! ```text
//! Column Families:
//! - batch_jobs          → job_id -> BatchJob (JSON)
//! - batch_job_by_user   → user_id:timestamp -> job_id (index)
//! - batch_job_by_status → status:timestamp -> job_id (index)
//! ```
//!
//! ## Concurrency
//!
//! Thread-safe using RwLock for in-memory cache + RocksDB persistence.

use anyhow::{Context, Result};
use rocksdb::{ColumnFamilyDescriptor, Options, DB};
use serde_json;
use std::path::Path;
use std::sync::{Arc, RwLock};
use tracing::{debug, info, warn};

use crate::workflows::domain::{BatchJob, BatchJobStatus};

/// Column family names
const CF_BATCH_JOBS: &str = "batch_jobs";
const CF_INDEX_BY_USER: &str = "batch_job_by_user";
const CF_INDEX_BY_STATUS: &str = "batch_job_by_status";

/// Batch job storage with RocksDB backend
pub struct BatchJobStore {
    /// RocksDB instance
    db: Arc<DB>,

    /// In-memory cache for fast reads
    cache: Arc<RwLock<std::collections::HashMap<String, BatchJob>>>,
}

impl BatchJobStore {
    /// Open or create batch job store
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let mut db_opts = Options::default();
        db_opts.create_if_missing(true);
        db_opts.create_missing_column_families(true);

        // Define column families
        let cf_batch_jobs = ColumnFamilyDescriptor::new(CF_BATCH_JOBS, Options::default());
        let cf_index_by_user = ColumnFamilyDescriptor::new(CF_INDEX_BY_USER, Options::default());
        let cf_index_by_status =
            ColumnFamilyDescriptor::new(CF_INDEX_BY_STATUS, Options::default());

        let cfs = vec![cf_batch_jobs, cf_index_by_user, cf_index_by_status];

        let db = DB::open_cf_descriptors(&db_opts, path, cfs)
            .context("Failed to open RocksDB for batch jobs")?;

        info!("Batch job store opened successfully");

        Ok(Self {
            db: Arc::new(db),
            cache: Arc::new(RwLock::new(std::collections::HashMap::new())),
        })
    }

    /// Create a new batch job
    pub fn create(&self, batch_job: BatchJob) -> Result<()> {
        let job_id = batch_job.job_id.clone();
        let created_by = batch_job.created_by.clone();
        let status = batch_job.status;
        let created_at_ts = batch_job.created_at.timestamp_millis();

        debug!("Creating batch job: {}", job_id);

        // Serialize batch job
        let json = serde_json::to_vec(&batch_job).context("Failed to serialize batch job")?;

        let cf_batch_jobs = self
            .db
            .cf_handle(CF_BATCH_JOBS)
            .context("Missing column family: batch_jobs")?;
        let cf_index_by_user = self
            .db
            .cf_handle(CF_INDEX_BY_USER)
            .context("Missing column family: batch_job_by_user")?;
        let cf_index_by_status = self
            .db
            .cf_handle(CF_INDEX_BY_STATUS)
            .context("Missing column family: batch_job_by_status")?;

        // Atomic batch write
        let mut batch = rocksdb::WriteBatch::default();

        // 1. Store batch job
        batch.put_cf(&cf_batch_jobs, job_id.as_bytes(), &json);

        // 2. Index by user (for "show my jobs")
        // Include job_id in key to ensure uniqueness
        let user_index_key = format!("{}:{:020}:{}", created_by, created_at_ts, job_id);
        batch.put_cf(
            &cf_index_by_user,
            user_index_key.as_bytes(),
            job_id.as_bytes(),
        );

        // 3. Index by status (for "show running jobs")
        // Include job_id in key to ensure uniqueness
        let status_index_key = format!("{:?}:{:020}:{}", status, created_at_ts, job_id);
        batch.put_cf(
            &cf_index_by_status,
            status_index_key.as_bytes(),
            job_id.as_bytes(),
        );

        // Write atomically
        self.db
            .write(batch)
            .context("Failed to write batch job to RocksDB")?;

        // Update cache
        {
            let mut cache = self.cache.write().unwrap();
            cache.insert(job_id.clone(), batch_job);
        }

        info!("Batch job created: {}", job_id);

        Ok(())
    }

    /// Get a batch job by ID
    pub fn get(&self, job_id: &str) -> Result<Option<BatchJob>> {
        // Check cache first
        {
            let cache = self.cache.read().unwrap();
            if let Some(batch_job) = cache.get(job_id) {
                return Ok(Some(batch_job.clone()));
            }
        }

        // Not in cache, read from RocksDB
        let cf = self
            .db
            .cf_handle(CF_BATCH_JOBS)
            .context("Missing column family: batch_jobs")?;

        if let Some(json_bytes) = self
            .db
            .get_cf(&cf, job_id.as_bytes())
            .context("Failed to read from RocksDB")?
        {
            let batch_job: BatchJob =
                serde_json::from_slice(&json_bytes).context("Failed to deserialize batch job")?;

            // Update cache
            {
                let mut cache = self.cache.write().unwrap();
                cache.insert(job_id.to_string(), batch_job.clone());
            }

            Ok(Some(batch_job))
        } else {
            Ok(None)
        }
    }

    /// Update a batch job (overwrites existing)
    pub fn update(&self, batch_job: BatchJob) -> Result<()> {
        let job_id = batch_job.job_id.clone();

        debug!("Updating batch job: {}", job_id);

        // Get old batch job to update indexes if status changed
        let old_batch_job = self.get(&job_id)?;

        // Serialize new batch job
        let json = serde_json::to_vec(&batch_job).context("Failed to serialize batch job")?;

        let cf_batch_jobs = self
            .db
            .cf_handle(CF_BATCH_JOBS)
            .context("Missing column family: batch_jobs")?;

        let mut batch = rocksdb::WriteBatch::default();

        // Update batch job
        batch.put_cf(&cf_batch_jobs, job_id.as_bytes(), &json);

        // If status changed, update status index
        if let Some(old_job) = old_batch_job {
            if old_job.status != batch_job.status {
                let cf_index_by_status = self
                    .db
                    .cf_handle(CF_INDEX_BY_STATUS)
                    .context("Missing column family: batch_job_by_status")?;

                // Delete old status index entry
                let old_status_key = format!(
                    "{:?}:{:020}:{}",
                    old_job.status,
                    old_job.created_at.timestamp_millis(),
                    job_id
                );
                batch.delete_cf(&cf_index_by_status, old_status_key.as_bytes());

                // Add new status index entry
                let new_status_key = format!(
                    "{:?}:{:020}:{}",
                    batch_job.status,
                    batch_job.created_at.timestamp_millis(),
                    job_id
                );
                batch.put_cf(
                    &cf_index_by_status,
                    new_status_key.as_bytes(),
                    job_id.as_bytes(),
                );
            }
        }

        // Write atomically
        self.db
            .write(batch)
            .context("Failed to update batch job in RocksDB")?;

        // Update cache
        {
            let mut cache = self.cache.write().unwrap();
            cache.insert(job_id.clone(), batch_job);
        }

        debug!("Batch job updated: {}", job_id);

        Ok(())
    }

    /// Delete a batch job
    pub fn delete(&self, job_id: &str) -> Result<bool> {
        debug!("Deleting batch job: {}", job_id);

        // Get batch job to delete index entries
        let batch_job = match self.get(job_id)? {
            Some(job) => job,
            None => return Ok(false),
        };

        let cf_batch_jobs = self
            .db
            .cf_handle(CF_BATCH_JOBS)
            .context("Missing column family: batch_jobs")?;
        let cf_index_by_user = self
            .db
            .cf_handle(CF_INDEX_BY_USER)
            .context("Missing column family: batch_job_by_user")?;
        let cf_index_by_status = self
            .db
            .cf_handle(CF_INDEX_BY_STATUS)
            .context("Missing column family: batch_job_by_status")?;

        let mut batch = rocksdb::WriteBatch::default();

        // Delete batch job
        batch.delete_cf(&cf_batch_jobs, job_id.as_bytes());

        // Delete user index entry
        let user_index_key = format!(
            "{}:{:020}:{}",
            batch_job.created_by,
            batch_job.created_at.timestamp_millis(),
            job_id
        );
        batch.delete_cf(&cf_index_by_user, user_index_key.as_bytes());

        // Delete status index entry
        let status_index_key = format!(
            "{:?}:{:020}:{}",
            batch_job.status,
            batch_job.created_at.timestamp_millis(),
            job_id
        );
        batch.delete_cf(&cf_index_by_status, status_index_key.as_bytes());

        // Write atomically
        self.db
            .write(batch)
            .context("Failed to delete batch job from RocksDB")?;

        // Remove from cache
        {
            let mut cache = self.cache.write().unwrap();
            cache.remove(job_id);
        }

        info!("Batch job deleted: {}", job_id);

        Ok(true)
    }

    /// List batch jobs by user (paginated)
    pub fn list_by_user(
        &self,
        user_id: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<BatchJob>> {
        let cf_index_by_user = self
            .db
            .cf_handle(CF_INDEX_BY_USER)
            .context("Missing column family: batch_job_by_user")?;

        let prefix = format!("{}:", user_id);
        let mut job_ids = Vec::new();

        // Scan index in reverse order (newest first)
        let iter = self.db.iterator_cf(
            &cf_index_by_user,
            rocksdb::IteratorMode::From(prefix.as_bytes(), rocksdb::Direction::Forward),
        );

        for item in iter {
            let (key, value) = item.context("Iterator error")?;

            // Check if key starts with prefix
            let key_str = String::from_utf8_lossy(&key);
            if !key_str.starts_with(&prefix) {
                break;
            }

            let job_id = String::from_utf8_lossy(&value).to_string();
            job_ids.push(job_id);
        }

        // Apply pagination
        let paginated_ids: Vec<_> = job_ids.into_iter().skip(offset).take(limit).collect();

        // Fetch batch jobs
        let mut batch_jobs = Vec::new();
        for job_id in paginated_ids {
            if let Some(batch_job) = self.get(&job_id)? {
                batch_jobs.push(batch_job);
            }
        }

        // Sort by created_at desc (newest first)
        batch_jobs.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        Ok(batch_jobs)
    }

    /// List batch jobs by status
    pub fn list_by_status(&self, status: BatchJobStatus, limit: usize) -> Result<Vec<BatchJob>> {
        let cf_index_by_status = self
            .db
            .cf_handle(CF_INDEX_BY_STATUS)
            .context("Missing column family: batch_job_by_status")?;

        let prefix = format!("{:?}:", status);
        let mut job_ids = Vec::new();

        // Scan index
        let iter = self.db.iterator_cf(
            &cf_index_by_status,
            rocksdb::IteratorMode::From(prefix.as_bytes(), rocksdb::Direction::Forward),
        );

        for item in iter {
            let (key, value) = item.context("Iterator error")?;

            // Check if key starts with prefix
            let key_str = String::from_utf8_lossy(&key);
            if !key_str.starts_with(&prefix) {
                break;
            }

            let job_id = String::from_utf8_lossy(&value).to_string();
            job_ids.push(job_id);

            if job_ids.len() >= limit {
                break;
            }
        }

        // Fetch batch jobs
        let mut batch_jobs = Vec::new();
        for job_id in job_ids {
            if let Some(batch_job) = self.get(&job_id)? {
                batch_jobs.push(batch_job);
            }
        }

        Ok(batch_jobs)
    }

    /// List all batch jobs (use with caution - can be large)
    pub fn list_all(&self, limit: usize) -> Result<Vec<BatchJob>> {
        let cf_batch_jobs = self
            .db
            .cf_handle(CF_BATCH_JOBS)
            .context("Missing column family: batch_jobs")?;

        let mut batch_jobs = Vec::new();

        let iter = self
            .db
            .iterator_cf(&cf_batch_jobs, rocksdb::IteratorMode::Start);

        for item in iter {
            let (_, value) = item.context("Iterator error")?;

            let batch_job: BatchJob =
                serde_json::from_slice(&value).context("Failed to deserialize batch job")?;

            batch_jobs.push(batch_job);

            if batch_jobs.len() >= limit {
                break;
            }
        }

        // Sort by created_at desc
        batch_jobs.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        Ok(batch_jobs)
    }

    /// Count batch jobs by status
    pub fn count_by_status(&self, status: BatchJobStatus) -> Result<usize> {
        let cf_index_by_status = self
            .db
            .cf_handle(CF_INDEX_BY_STATUS)
            .context("Missing column family: batch_job_by_status")?;

        let prefix = format!("{:?}:", status);
        let mut count = 0;

        let iter = self.db.iterator_cf(
            &cf_index_by_status,
            rocksdb::IteratorMode::From(prefix.as_bytes(), rocksdb::Direction::Forward),
        );

        for item in iter {
            let (key, _) = item.context("Iterator error")?;

            let key_str = String::from_utf8_lossy(&key);
            if !key_str.starts_with(&prefix) {
                break;
            }

            count += 1;
        }

        Ok(count)
    }

    /// Clear all batch jobs (use with caution!)
    pub fn clear_all(&self) -> Result<()> {
        warn!("Clearing all batch jobs from storage");

        let cf_batch_jobs = self
            .db
            .cf_handle(CF_BATCH_JOBS)
            .context("Missing column family: batch_jobs")?;
        let cf_index_by_user = self
            .db
            .cf_handle(CF_INDEX_BY_USER)
            .context("Missing column family: batch_job_by_user")?;
        let cf_index_by_status = self
            .db
            .cf_handle(CF_INDEX_BY_STATUS)
            .context("Missing column family: batch_job_by_status")?;

        // Clear all column families
        for cf in [&cf_batch_jobs, &cf_index_by_user, &cf_index_by_status] {
            let iter = self.db.iterator_cf(cf, rocksdb::IteratorMode::Start);

            let mut batch = rocksdb::WriteBatch::default();
            for item in iter {
                let (key, _) = item.context("Iterator error")?;
                batch.delete_cf(cf, &key);
            }

            self.db
                .write(batch)
                .context("Failed to clear column family")?;
        }

        // Clear cache
        {
            let mut cache = self.cache.write().unwrap();
            cache.clear();
        }

        info!("All batch jobs cleared");

        Ok(())
    }

    /// Get storage statistics
    pub fn get_stats(&self) -> Result<BatchJobStoreStats> {
        let total_jobs = self.list_all(usize::MAX)?.len();

        let mut stats = BatchJobStoreStats {
            total_jobs,
            by_status: std::collections::HashMap::new(),
            cache_size: 0,
        };

        // Count by status
        for status in &[
            BatchJobStatus::Pending,
            BatchJobStatus::Validating,
            BatchJobStatus::Running,
            BatchJobStatus::Paused,
            BatchJobStatus::Completed,
            BatchJobStatus::PartiallyCompleted,
            BatchJobStatus::Failed,
            BatchJobStatus::Cancelled,
        ] {
            let count = self.count_by_status(*status)?;
            stats.by_status.insert(*status, count);
        }

        // Cache size
        {
            let cache = self.cache.read().unwrap();
            stats.cache_size = cache.len();
        }

        Ok(stats)
    }
}

/// Storage statistics
#[derive(Debug, Clone)]
pub struct BatchJobStoreStats {
    pub total_jobs: usize,
    pub by_status: std::collections::HashMap<BatchJobStatus, usize>,
    pub cache_size: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflows::domain::{BatchJobConfig, WorkflowExecutionRef};
    use tempfile::TempDir;

    fn create_test_store() -> (BatchJobStore, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let store = BatchJobStore::open(temp_dir.path()).unwrap();
        (store, temp_dir)
    }

    fn create_test_batch_job(user_id: &str) -> BatchJob {
        let config = BatchJobConfig::default();
        let mut batch_job = BatchJob::new(
            "Test Batch".to_string(),
            "workflow_123".to_string(),
            config,
            user_id.to_string(),
        );

        #[allow(deprecated)]
        let exec = WorkflowExecutionRef::from_file(
            "file_1".to_string(),
            "data.csv".to_string(),
            "data".to_string(),
        );
        batch_job.add_execution(exec);

        batch_job
    }

    #[test]
    fn test_create_and_get() {
        let (store, _temp) = create_test_store();

        let batch_job = create_test_batch_job("user_1");
        let job_id = batch_job.job_id.clone();

        // Create
        store.create(batch_job.clone()).unwrap();

        // Get
        let retrieved = store.get(&job_id).unwrap();
        assert!(retrieved.is_some());

        let retrieved_job = retrieved.unwrap();
        assert_eq!(retrieved_job.job_id, job_id);
        assert_eq!(retrieved_job.name, "Test Batch");
        assert_eq!(retrieved_job.created_by, "user_1");
    }

    #[test]
    fn test_update() {
        let (store, _temp) = create_test_store();

        let mut batch_job = create_test_batch_job("user_1");
        let job_id = batch_job.job_id.clone();

        // Create
        store.create(batch_job.clone()).unwrap();

        // Update status
        batch_job.update_status(BatchJobStatus::Running);
        store.update(batch_job.clone()).unwrap();

        // Verify update
        let retrieved = store.get(&job_id).unwrap().unwrap();
        assert_eq!(retrieved.status, BatchJobStatus::Running);
    }

    #[test]
    fn test_delete() {
        let (store, _temp) = create_test_store();

        let batch_job = create_test_batch_job("user_1");
        let job_id = batch_job.job_id.clone();

        // Create
        store.create(batch_job).unwrap();

        // Delete
        let deleted = store.delete(&job_id).unwrap();
        assert!(deleted);

        // Verify deleted
        let retrieved = store.get(&job_id).unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_list_by_user() {
        let (store, _temp) = create_test_store();

        // Create batch jobs for different users
        let batch1 = create_test_batch_job("user_1");
        let batch2 = create_test_batch_job("user_1");
        let batch3 = create_test_batch_job("user_2");

        store.create(batch1).unwrap();
        store.create(batch2).unwrap();
        store.create(batch3).unwrap();

        // List user_1's jobs
        let user1_jobs = store.list_by_user("user_1", 10, 0).unwrap();
        assert_eq!(user1_jobs.len(), 2);

        // List user_2's jobs
        let user2_jobs = store.list_by_user("user_2", 10, 0).unwrap();
        assert_eq!(user2_jobs.len(), 1);
    }

    #[test]
    fn test_list_by_status() {
        let (store, _temp) = create_test_store();

        let mut batch1 = create_test_batch_job("user_1");
        batch1.update_status(BatchJobStatus::Running);

        let mut batch2 = create_test_batch_job("user_1");
        batch2.update_status(BatchJobStatus::Running);

        let batch3 = create_test_batch_job("user_1");
        // batch3 stays Pending

        store.create(batch1).unwrap();
        store.create(batch2).unwrap();
        store.create(batch3).unwrap();

        // List running jobs
        let running_jobs = store.list_by_status(BatchJobStatus::Running, 10).unwrap();
        assert_eq!(running_jobs.len(), 2);

        // List pending jobs
        let pending_jobs = store.list_by_status(BatchJobStatus::Pending, 10).unwrap();
        assert_eq!(pending_jobs.len(), 1);
    }

    #[test]
    fn test_count_by_status() {
        let (store, _temp) = create_test_store();

        let mut batch1 = create_test_batch_job("user_1");
        batch1.update_status(BatchJobStatus::Completed);

        let mut batch2 = create_test_batch_job("user_1");
        batch2.update_status(BatchJobStatus::Completed);

        store.create(batch1).unwrap();
        store.create(batch2).unwrap();

        let count = store.count_by_status(BatchJobStatus::Completed).unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn test_status_index_update() {
        let (store, _temp) = create_test_store();

        let mut batch_job = create_test_batch_job("user_1");
        store.create(batch_job.clone()).unwrap();

        // Initially pending
        assert_eq!(store.count_by_status(BatchJobStatus::Pending).unwrap(), 1);
        assert_eq!(store.count_by_status(BatchJobStatus::Running).unwrap(), 0);

        // Update to running
        batch_job.update_status(BatchJobStatus::Running);
        store.update(batch_job.clone()).unwrap();

        // Verify index updated
        assert_eq!(store.count_by_status(BatchJobStatus::Pending).unwrap(), 0);
        assert_eq!(store.count_by_status(BatchJobStatus::Running).unwrap(), 1);
    }

    #[test]
    fn test_pagination() {
        let (store, _temp) = create_test_store();

        // Create 5 batch jobs
        for i in 0..5 {
            let mut batch = create_test_batch_job("user_1");
            batch.name = format!("Batch {}", i);
            store.create(batch).unwrap();
        }

        // Get first 2
        let page1 = store.list_by_user("user_1", 2, 0).unwrap();
        assert_eq!(page1.len(), 2);

        // Get next 2
        let page2 = store.list_by_user("user_1", 2, 2).unwrap();
        assert_eq!(page2.len(), 2);

        // Get last 1
        let page3 = store.list_by_user("user_1", 2, 4).unwrap();
        assert_eq!(page3.len(), 1);
    }
}
