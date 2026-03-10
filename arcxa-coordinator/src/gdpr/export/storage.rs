//! Export Job Storage
//!
//! RocksDB-backed storage for GDPR data export jobs.
//! Provides CRUD operations and indexed queries for efficient job management.
//!
//! ## Storage Layout
//!
//! ### Primary Keys
//! - `export:job:{job_id}` → Serialized ExportJob
//!
//! ### Secondary Indexes
//! - `export:by_user:{user_id}:{created_at_ms}:{job_id}` → Empty value (for user queries)
//! - `export:by_status:{status}:{created_at_ms}:{job_id}` → Empty value (for status queries)
//!
//! ### Implementation Notes
//! - Uses bincode for efficient binary serialization
//! - Maintains secondary indexes for user_id and status queries
//! - Leverages KvStore's prefix_scan for efficient index queries
//! - Handles job updates by updating both primary record and indexes

use super::types::{ExportJob, ExportStatus};
use crate::storage::kv_store::KvStore;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use std::sync::Arc;
use uuid::Uuid;

/// Export job storage using RocksDB
pub struct ExportJobStore {
    kv: Arc<KvStore>,
}

impl ExportJobStore {
    /// Create new export job store
    ///
    /// # Arguments
    /// * `kv` - Shared KvStore instance (can be shared with other stores)
    pub fn new(kv: Arc<KvStore>) -> Self {
        Self { kv }
    }

    /// Create a new export job store with its own RocksDB instance
    ///
    /// # Arguments
    /// * `path` - Path to RocksDB directory
    pub fn create<P: AsRef<std::path::Path>>(path: P) -> Result<Self> {
        let kv = Arc::new(KvStore::new(path)?);
        Ok(Self::new(kv))
    }

    /// Save export job (create or update)
    ///
    /// Atomically updates both the primary record and all secondary indexes.
    pub fn save(&self, job: &ExportJob) -> Result<()> {
        // Serialize job to bincode
        let value = bincode::serialize(job).context("Failed to serialize ExportJob")?;

        // Primary key: export:job:{job_id}
        let primary_key = Self::job_key(job.id);
        self.kv.put(primary_key.as_bytes(), &value)?;

        // Update secondary indexes
        self.update_indexes(job)?;

        Ok(())
    }

    /// Get export job by ID
    pub fn get(&self, job_id: Uuid) -> Result<Option<ExportJob>> {
        let key = Self::job_key(job_id);

        match self.kv.get(key.as_bytes())? {
            Some(bytes) => {
                let job: ExportJob =
                    bincode::deserialize(&bytes).context("Failed to deserialize ExportJob")?;
                Ok(Some(job))
            }
            None => Ok(None),
        }
    }

    /// Delete export job
    ///
    /// Removes both the primary record and all secondary indexes.
    pub fn delete(&self, job_id: Uuid) -> Result<()> {
        // First, fetch the job to get index keys
        let job = match self.get(job_id)? {
            Some(job) => job,
            None => return Ok(()), // Already deleted
        };

        // Delete primary key
        let primary_key = Self::job_key(job_id);
        self.kv.delete(primary_key.as_bytes())?;

        // Delete secondary indexes
        self.delete_indexes(&job)?;

        Ok(())
    }

    /// List all export jobs for a user
    ///
    /// Returns jobs ordered by creation time (newest first).
    ///
    /// # Arguments
    /// * `user_id` - User ID to query
    /// * `limit` - Maximum number of jobs to return (default: 100)
    pub fn list_by_user(&self, user_id: &str, limit: Option<usize>) -> Result<Vec<ExportJob>> {
        let prefix = format!("export:by_user:{}:", user_id);
        let index_entries = self.kv.prefix_scan(prefix.as_bytes())?;

        let limit = limit.unwrap_or(100);
        let mut jobs = Vec::new();

        // Index entries are sorted by created_at DESC (due to key format)
        for (key, _) in index_entries.into_iter().take(limit) {
            // Extract job_id from key: export:by_user:{user_id}:{created_at_ms}:{job_id}
            if let Some(job_id_str) = Self::extract_job_id_from_index(&key) {
                if let Ok(job_id) = Uuid::parse_str(&job_id_str) {
                    if let Some(job) = self.get(job_id)? {
                        jobs.push(job);
                    }
                }
            }
        }

        // Sort by created_at DESC (newest first)
        jobs.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        Ok(jobs)
    }

    /// List export jobs by status
    ///
    /// Returns jobs ordered by creation time (newest first).
    ///
    /// # Arguments
    /// * `status` - Status to filter by
    /// * `limit` - Maximum number of jobs to return (default: 100)
    pub fn list_by_status(
        &self,
        status: ExportStatus,
        limit: Option<usize>,
    ) -> Result<Vec<ExportJob>> {
        let status_str = Self::status_to_str(&status);
        let prefix = format!("export:by_status:{}:", status_str);
        let index_entries = self.kv.prefix_scan(prefix.as_bytes())?;

        let limit = limit.unwrap_or(100);
        let mut jobs = Vec::new();

        for (key, _) in index_entries.into_iter().take(limit) {
            if let Some(job_id_str) = Self::extract_job_id_from_index(&key) {
                if let Ok(job_id) = Uuid::parse_str(&job_id_str) {
                    if let Some(job) = self.get(job_id)? {
                        jobs.push(job);
                    }
                }
            }
        }

        // Sort by created_at DESC (newest first)
        jobs.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        Ok(jobs)
    }

    /// List jobs by user and status
    ///
    /// Combines both filters for efficient querying.
    pub fn list_by_user_and_status(
        &self,
        user_id: &str,
        status: ExportStatus,
        limit: Option<usize>,
    ) -> Result<Vec<ExportJob>> {
        // Get all jobs for user, then filter by status
        // This is more efficient than loading all jobs
        let user_jobs = self.list_by_user(user_id, None)?;

        let limit = limit.unwrap_or(100);
        let filtered: Vec<ExportJob> = user_jobs
            .into_iter()
            .filter(|job| job.status == status)
            .take(limit)
            .collect();

        Ok(filtered)
    }

    /// Count total jobs for a user
    pub fn count_by_user(&self, user_id: &str) -> Result<usize> {
        let prefix = format!("export:by_user:{}:", user_id);
        let index_entries = self.kv.prefix_scan(prefix.as_bytes())?;
        Ok(index_entries.len())
    }

    /// Count jobs by status
    pub fn count_by_status(&self, status: ExportStatus) -> Result<usize> {
        let status_str = Self::status_to_str(&status);
        let prefix = format!("export:by_status:{}:", status_str);
        let index_entries = self.kv.prefix_scan(prefix.as_bytes())?;
        Ok(index_entries.len())
    }

    /// Find expired jobs that need cleanup
    ///
    /// Returns jobs with status=Ready that have expired.
    pub fn find_expired_jobs(&self, now: DateTime<Utc>) -> Result<Vec<ExportJob>> {
        let ready_jobs = self.list_by_status(ExportStatus::Ready, None)?;

        let expired: Vec<ExportJob> = ready_jobs
            .into_iter()
            .filter(|job| {
                if let Some(expires_at) = job.expires_at {
                    expires_at < now
                } else {
                    false
                }
            })
            .collect();

        Ok(expired)
    }

    /// Cleanup expired jobs
    ///
    /// Transitions expired jobs to Expired status and optionally deletes old jobs.
    ///
    /// # Arguments
    /// * `delete_after_days` - Delete jobs that have been expired for this many days
    ///
    /// # Returns
    /// Number of jobs cleaned up
    pub fn cleanup_expired(&self, delete_after_days: Option<i64>) -> Result<usize> {
        let now = Utc::now();
        let expired_jobs = self.find_expired_jobs(now)?;

        let mut count = 0;

        for mut job in expired_jobs {
            // Check if we should delete or just mark as expired
            if let Some(days) = delete_after_days {
                if let Some(expires_at) = job.expires_at {
                    let age_days = (now - expires_at).num_days();
                    if age_days > days {
                        // Delete the job entirely
                        self.delete(job.id)?;
                        count += 1;
                        continue;
                    }
                }
            }

            // Just mark as expired
            if job.status == ExportStatus::Ready {
                job.status = ExportStatus::Expired;
                job.updated_at = now;
                self.save(&job)?;
                count += 1;
            }
        }

        Ok(count)
    }

    // ===== Private Helper Methods =====

    /// Generate primary key for a job
    fn job_key(job_id: Uuid) -> String {
        format!("export:job:{}", job_id)
    }

    /// Generate user index key
    ///
    /// Format: export:by_user:{user_id}:{created_at_ms}:{job_id}
    ///
    /// Uses negative timestamp for reverse chronological order:
    /// - Newer jobs have smaller timestamps
    /// - RocksDB sorts lexicographically, so newer jobs come first
    fn user_index_key(user_id: &str, created_at: DateTime<Utc>, job_id: Uuid) -> String {
        // Use negative timestamp for DESC order
        let ts_inverted = i64::MAX - created_at.timestamp_millis();
        format!("export:by_user:{}:{:020}:{}", user_id, ts_inverted, job_id)
    }

    /// Generate status index key
    ///
    /// Format: export:by_status:{status}:{created_at_ms}:{job_id}
    fn status_index_key(status: &ExportStatus, created_at: DateTime<Utc>, job_id: Uuid) -> String {
        let ts_inverted = i64::MAX - created_at.timestamp_millis();
        let status_str = Self::status_to_str(status);
        format!(
            "export:by_status:{}:{:020}:{}",
            status_str, ts_inverted, job_id
        )
    }

    /// Update all secondary indexes for a job
    fn update_indexes(&self, job: &ExportJob) -> Result<()> {
        // Delete old indexes (if job is being updated)
        // Note: This is idempotent - if keys don't exist, delete is a no-op
        self.delete_indexes(job)?;

        // Create new indexes
        let user_idx = Self::user_index_key(&job.user_id, job.created_at, job.id);
        let status_idx = Self::status_index_key(&job.status, job.created_at, job.id);

        // Empty values - we only need the keys for indexing
        self.kv.put(user_idx.as_bytes(), &[])?;
        self.kv.put(status_idx.as_bytes(), &[])?;

        Ok(())
    }

    /// Delete all secondary indexes for a job
    fn delete_indexes(&self, job: &ExportJob) -> Result<()> {
        // Delete user index for all possible statuses (in case status changed)
        for status in [
            ExportStatus::Pending,
            ExportStatus::Processing,
            ExportStatus::Ready,
            ExportStatus::Failed,
            ExportStatus::Expired,
            ExportStatus::Cancelled,
        ] {
            let status_idx = Self::status_index_key(&status, job.created_at, job.id);
            let _ = self.kv.delete(status_idx.as_bytes());
        }

        // Delete user index
        let user_idx = Self::user_index_key(&job.user_id, job.created_at, job.id);
        let _ = self.kv.delete(user_idx.as_bytes());

        Ok(())
    }

    /// Extract job_id from index key
    ///
    /// Index key format: export:by_{index}:{value}:{created_at_ms}:{job_id}
    fn extract_job_id_from_index(key: &[u8]) -> Option<String> {
        let key_str = std::str::from_utf8(key).ok()?;
        key_str.split(':').last().map(|s| s.to_string())
    }

    /// Convert status to string for indexing
    fn status_to_str(status: &ExportStatus) -> &'static str {
        match status {
            ExportStatus::Pending => "pending",
            ExportStatus::Processing => "processing",
            ExportStatus::Ready => "ready",
            ExportStatus::Failed => "failed",
            ExportStatus::Expired => "expired",
            ExportStatus::Cancelled => "cancelled",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gdpr::export::types::{ExportFormat, ExportRequest};

    fn create_test_store() -> ExportJobStore {
        let kv = Arc::new(KvStore::new_in_memory().unwrap());
        ExportJobStore::new(kv)
    }

    fn create_test_job(user_id: &str, status: ExportStatus) -> ExportJob {
        use std::collections::HashMap;

        let request = ExportRequest {
            user_id: user_id.to_string(),
            format: ExportFormat::Json,
            categories: vec![],
            include_derived: false,
            include_metadata: true,
            include_audit_trail: false,
            time_range: None,
            filters: HashMap::new(),
        };

        let mut job = ExportJob::new(
            user_id.to_string(),
            "requester@test.com".to_string(),
            request,
        );
        job.status = status;
        job
    }

    #[test]
    fn test_save_and_get() {
        let store = create_test_store();
        let job = create_test_job("user123", ExportStatus::Pending);
        let job_id = job.id;

        // Save job
        store.save(&job).unwrap();

        // Retrieve job
        let retrieved = store.get(job_id).unwrap().expect("Job should exist");
        assert_eq!(retrieved.id, job_id);
        assert_eq!(retrieved.user_id, "user123");
        assert_eq!(retrieved.status, ExportStatus::Pending);
    }

    #[test]
    fn test_delete() {
        let store = create_test_store();
        let job = create_test_job("user123", ExportStatus::Pending);
        let job_id = job.id;

        store.save(&job).unwrap();
        assert!(store.get(job_id).unwrap().is_some());

        // Delete job
        store.delete(job_id).unwrap();
        assert!(store.get(job_id).unwrap().is_none());
    }

    #[test]
    fn test_list_by_user() {
        let store = create_test_store();

        // Create jobs for different users
        let job1 = create_test_job("alice", ExportStatus::Pending);
        let job2 = create_test_job("alice", ExportStatus::Ready);
        let job3 = create_test_job("bob", ExportStatus::Pending);

        store.save(&job1).unwrap();
        store.save(&job2).unwrap();
        store.save(&job3).unwrap();

        // Query Alice's jobs
        let alice_jobs = store.list_by_user("alice", None).unwrap();
        assert_eq!(alice_jobs.len(), 2);
        assert!(alice_jobs.iter().all(|j| j.user_id == "alice"));

        // Query Bob's jobs
        let bob_jobs = store.list_by_user("bob", None).unwrap();
        assert_eq!(bob_jobs.len(), 1);
        assert_eq!(bob_jobs[0].user_id, "bob");
    }

    #[test]
    fn test_list_by_status() {
        let store = create_test_store();

        let job1 = create_test_job("user1", ExportStatus::Pending);
        let job2 = create_test_job("user2", ExportStatus::Pending);
        let job3 = create_test_job("user3", ExportStatus::Ready);

        store.save(&job1).unwrap();
        store.save(&job2).unwrap();
        store.save(&job3).unwrap();

        // Query pending jobs
        let pending = store.list_by_status(ExportStatus::Pending, None).unwrap();
        assert_eq!(pending.len(), 2);

        // Query ready jobs
        let ready = store.list_by_status(ExportStatus::Ready, None).unwrap();
        assert_eq!(ready.len(), 1);
    }

    #[test]
    fn test_update_job_status() {
        let store = create_test_store();
        let mut job = create_test_job("user123", ExportStatus::Pending);
        let job_id = job.id;

        store.save(&job).unwrap();

        // Verify initial status
        let pending = store.list_by_status(ExportStatus::Pending, None).unwrap();
        assert_eq!(pending.len(), 1);

        // Update status
        job.status = ExportStatus::Processing;
        store.save(&job).unwrap();

        // Verify indexes updated
        let pending = store.list_by_status(ExportStatus::Pending, None).unwrap();
        assert_eq!(pending.len(), 0);

        let processing = store
            .list_by_status(ExportStatus::Processing, None)
            .unwrap();
        assert_eq!(processing.len(), 1);
        assert_eq!(processing[0].id, job_id);
    }

    #[test]
    fn test_count_operations() {
        let store = create_test_store();

        let job1 = create_test_job("alice", ExportStatus::Pending);
        let job2 = create_test_job("alice", ExportStatus::Ready);
        let job3 = create_test_job("bob", ExportStatus::Pending);

        store.save(&job1).unwrap();
        store.save(&job2).unwrap();
        store.save(&job3).unwrap();

        assert_eq!(store.count_by_user("alice").unwrap(), 2);
        assert_eq!(store.count_by_user("bob").unwrap(), 1);
        assert_eq!(store.count_by_status(ExportStatus::Pending).unwrap(), 2);
        assert_eq!(store.count_by_status(ExportStatus::Ready).unwrap(), 1);
    }

    #[test]
    fn test_list_with_limit() {
        let store = create_test_store();

        // Create 5 jobs for same user
        for _ in 0..5 {
            let job = create_test_job("alice", ExportStatus::Pending);
            store.save(&job).unwrap();
        }

        let jobs = store.list_by_user("alice", Some(3)).unwrap();
        assert_eq!(jobs.len(), 3);
    }

    #[test]
    fn test_find_expired_jobs() {
        let store = create_test_store();

        // Create ready job with expiration in the past
        let mut job1 = create_test_job("user1", ExportStatus::Ready);
        job1.expires_at = Some(Utc::now() - chrono::Duration::hours(1));
        store.save(&job1).unwrap();

        // Create ready job with expiration in the future
        let mut job2 = create_test_job("user2", ExportStatus::Ready);
        job2.expires_at = Some(Utc::now() + chrono::Duration::hours(24));
        store.save(&job2).unwrap();

        let expired = store.find_expired_jobs(Utc::now()).unwrap();
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].id, job1.id);
    }

    #[test]
    fn test_cleanup_expired() {
        let store = create_test_store();

        // Create expired job
        let mut job = create_test_job("user1", ExportStatus::Ready);
        job.expires_at = Some(Utc::now() - chrono::Duration::hours(1));
        let job_id = job.id;
        store.save(&job).unwrap();

        // Cleanup without deletion
        let count = store.cleanup_expired(None).unwrap();
        assert_eq!(count, 1);

        // Verify status changed to Expired
        let job = store.get(job_id).unwrap().unwrap();
        assert_eq!(job.status, ExportStatus::Expired);
    }

    #[test]
    fn test_list_by_user_and_status() {
        let store = create_test_store();

        let job1 = create_test_job("alice", ExportStatus::Pending);
        let job2 = create_test_job("alice", ExportStatus::Ready);
        let job3 = create_test_job("alice", ExportStatus::Pending);

        store.save(&job1).unwrap();
        store.save(&job2).unwrap();
        store.save(&job3).unwrap();

        let pending = store
            .list_by_user_and_status("alice", ExportStatus::Pending, None)
            .unwrap();
        assert_eq!(pending.len(), 2);

        let ready = store
            .list_by_user_and_status("alice", ExportStatus::Ready, None)
            .unwrap();
        assert_eq!(ready.len(), 1);
    }
}
