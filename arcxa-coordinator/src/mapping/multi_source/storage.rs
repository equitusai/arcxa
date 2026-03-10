//! Storage layer for unified mapping sessions
//!
//! This module provides RocksDB-backed persistence for unified mapping sessions
//! with support for CRUD operations, querying, and statistics.

use super::types::{
    UnifiedLoadJob, UnifiedLoadJobStatus, UnifiedMappingSession, UnifiedSessionStatus,
};
use anyhow::{Context, Result};
use rocksdb::{ColumnFamilyDescriptor, Options, DB};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use tracing::{debug, info};

/// Column family names for unified mapping storage
const CF_SESSIONS: &str = "unified_sessions";
const CF_SESSION_INDEX: &str = "unified_session_index";
const CF_STATUS_INDEX: &str = "unified_status_index";
const CF_LOAD_JOBS: &str = "unified_load_jobs";
const CF_LOAD_JOB_INDEX: &str = "unified_load_job_index";
const CF_LOAD_JOB_SESSION_INDEX: &str = "unified_load_job_session_index";

/// Storage for unified mapping sessions
pub struct UnifiedMappingStorage {
    db: Arc<DB>,
}

/// Storage statistics
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StorageStatistics {
    /// Total number of unified sessions
    pub total_sessions: usize,

    /// Sessions by status
    pub by_status: std::collections::HashMap<String, usize>,

    /// Total source sessions referenced
    pub total_source_sessions: usize,

    /// Total field mappings across all sessions
    pub total_field_mappings: usize,
}

impl UnifiedMappingStorage {
    /// Create a new storage instance
    ///
    /// Opens or creates a RocksDB database at the specified path with
    /// column families for sessions and indexes.
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);

        // Define column families
        let cfs = vec![
            ColumnFamilyDescriptor::new(CF_SESSIONS, Options::default()),
            ColumnFamilyDescriptor::new(CF_SESSION_INDEX, Options::default()),
            ColumnFamilyDescriptor::new(CF_STATUS_INDEX, Options::default()),
            ColumnFamilyDescriptor::new(CF_LOAD_JOBS, Options::default()),
            ColumnFamilyDescriptor::new(CF_LOAD_JOB_INDEX, Options::default()),
            ColumnFamilyDescriptor::new(CF_LOAD_JOB_SESSION_INDEX, Options::default()),
        ];

        let db = DB::open_cf_descriptors(&opts, path, cfs)
            .context("Failed to open RocksDB for unified mapping storage")?;

        info!("Unified mapping storage initialized");

        Ok(Self { db: Arc::new(db) })
    }

    /// Store a unified mapping session
    ///
    /// Persists the session and updates all indexes.
    pub fn store_session(&self, session: &UnifiedMappingSession) -> Result<()> {
        let cf_sessions = self
            .db
            .cf_handle(CF_SESSIONS)
            .context("Sessions CF not found")?;
        let cf_session_index = self
            .db
            .cf_handle(CF_SESSION_INDEX)
            .context("Session index CF not found")?;
        let cf_status_index = self
            .db
            .cf_handle(CF_STATUS_INDEX)
            .context("Status index CF not found")?;

        // Serialize session
        let session_json =
            serde_json::to_vec(session).context("Failed to serialize unified session")?;

        // Store session
        self.db
            .put_cf(cf_sessions, session.id.as_bytes(), session_json)
            .context("Failed to store unified session")?;

        // Update session index (for listing all sessions)
        self.db
            .put_cf(
                cf_session_index,
                session.id.as_bytes(),
                session.created_at.to_le_bytes(),
            )
            .context("Failed to update session index")?;

        // Update status index (for querying by status)
        let status_key = format!("{}:{}", format!("{:?}", session.status), session.id);
        self.db
            .put_cf(cf_status_index, status_key.as_bytes(), b"1")
            .context("Failed to update status index")?;

        debug!("Stored unified session: {}", session.id);

        Ok(())
    }

    /// Retrieve a unified mapping session by ID
    pub fn get_session(&self, session_id: &str) -> Result<Option<UnifiedMappingSession>> {
        let cf_sessions = self
            .db
            .cf_handle(CF_SESSIONS)
            .context("Sessions CF not found")?;

        match self.db.get_cf(cf_sessions, session_id.as_bytes())? {
            Some(data) => {
                let session: UnifiedMappingSession = serde_json::from_slice(&data)
                    .context("Failed to deserialize unified session")?;
                Ok(Some(session))
            }
            None => Ok(None),
        }
    }

    /// Update session status
    ///
    /// Updates the session status and updates the status index.
    pub fn update_session_status(
        &self,
        session_id: &str,
        new_status: UnifiedSessionStatus,
    ) -> Result<()> {
        let mut session = self
            .get_session(session_id)?
            .ok_or_else(|| anyhow::anyhow!("Session not found: {}", session_id))?;

        // Remove old status index entry
        let old_status_key = format!("{}:{}", format!("{:?}", session.status), session.id);
        let cf_status_index = self
            .db
            .cf_handle(CF_STATUS_INDEX)
            .context("Status index CF not found")?;
        self.db
            .delete_cf(cf_status_index, old_status_key.as_bytes())
            .context("Failed to delete old status index entry")?;

        // Update session
        session.status = new_status;
        session.updated_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        // Store updated session
        self.store_session(&session)?;

        debug!(
            "Updated session {} status to {:?}",
            session_id, session.status
        );

        Ok(())
    }

    /// List all unified sessions
    ///
    /// Returns sessions ordered by creation time (newest first).
    pub fn list_sessions(&self, limit: Option<usize>) -> Result<Vec<UnifiedMappingSession>> {
        let cf_session_index = self
            .db
            .cf_handle(CF_SESSION_INDEX)
            .context("Session index CF not found")?;

        let mut sessions = Vec::new();
        let mut session_times: Vec<(String, i64)> = Vec::new();

        // Collect all session IDs with timestamps
        let iter = self
            .db
            .iterator_cf(cf_session_index, rocksdb::IteratorMode::Start);
        for item in iter {
            let (key, value) = item.context("Failed to read from session index")?;
            let session_id =
                String::from_utf8(key.to_vec()).context("Invalid UTF-8 in session ID")?;
            let timestamp = i64::from_le_bytes(
                value
                    .as_ref()
                    .try_into()
                    .context("Invalid timestamp in index")?,
            );
            session_times.push((session_id, timestamp));
        }

        // Sort by timestamp (newest first)
        session_times.sort_by(|a, b| b.1.cmp(&a.1));

        // Apply limit
        if let Some(limit) = limit {
            session_times.truncate(limit);
        }

        // Load sessions
        for (session_id, _) in session_times {
            if let Some(session) = self.get_session(&session_id)? {
                sessions.push(session);
            }
        }

        Ok(sessions)
    }

    /// List sessions by status
    pub fn list_sessions_by_status(
        &self,
        status: &UnifiedSessionStatus,
    ) -> Result<Vec<UnifiedMappingSession>> {
        let cf_status_index = self
            .db
            .cf_handle(CF_STATUS_INDEX)
            .context("Status index CF not found")?;

        let status_prefix = format!("{:?}:", status);
        let mut sessions = Vec::new();

        let iter = self
            .db
            .iterator_cf(cf_status_index, rocksdb::IteratorMode::Start);

        for item in iter {
            let (key, _) = item.context("Failed to read from status index")?;
            let key_str = String::from_utf8(key.to_vec()).context("Invalid UTF-8 in key")?;

            if key_str.starts_with(&status_prefix) {
                let session_id = key_str
                    .strip_prefix(&status_prefix)
                    .context("Failed to extract session ID from key")?;
                if let Some(session) = self.get_session(session_id)? {
                    sessions.push(session);
                }
            }
        }

        Ok(sessions)
    }

    /// Delete a unified session
    ///
    /// Removes the session and all its index entries.
    pub fn delete_session(&self, session_id: &str) -> Result<()> {
        // Load session to get status for index cleanup
        let session = self
            .get_session(session_id)?
            .ok_or_else(|| anyhow::anyhow!("Session not found: {}", session_id))?;

        let cf_sessions = self
            .db
            .cf_handle(CF_SESSIONS)
            .context("Sessions CF not found")?;
        let cf_session_index = self
            .db
            .cf_handle(CF_SESSION_INDEX)
            .context("Session index CF not found")?;
        let cf_status_index = self
            .db
            .cf_handle(CF_STATUS_INDEX)
            .context("Status index CF not found")?;

        // Delete session
        self.db
            .delete_cf(cf_sessions, session_id.as_bytes())
            .context("Failed to delete session")?;

        // Delete from session index
        self.db
            .delete_cf(cf_session_index, session_id.as_bytes())
            .context("Failed to delete from session index")?;

        // Delete from status index
        let status_key = format!("{}:{}", format!("{:?}", session.status), session_id);
        self.db
            .delete_cf(cf_status_index, status_key.as_bytes())
            .context("Failed to delete from status index")?;

        debug!("Deleted unified session: {}", session_id);

        Ok(())
    }

    /// Get storage statistics
    pub fn get_statistics(&self) -> Result<StorageStatistics> {
        let sessions = self.list_sessions(None)?;

        let total_sessions = sessions.len();

        // Count by status
        let mut by_status = std::collections::HashMap::new();
        for session in &sessions {
            let status_str = format!("{:?}", session.status);
            *by_status.entry(status_str).or_insert(0) += 1;
        }

        // Count source sessions
        let mut source_sessions_set = std::collections::HashSet::new();
        for session in &sessions {
            for source_id in &session.source_sessions {
                source_sessions_set.insert(source_id.clone());
            }
        }
        let total_source_sessions = source_sessions_set.len();

        // Count field mappings
        let total_field_mappings: usize = sessions.iter().map(|s| s.field_mappings.len()).sum();

        Ok(StorageStatistics {
            total_sessions,
            by_status,
            total_source_sessions,
            total_field_mappings,
        })
    }

    /// Persist a unified load job and maintain indexes.
    pub fn store_load_job(&self, job: &UnifiedLoadJob) -> Result<()> {
        let cf_jobs = self
            .db
            .cf_handle(CF_LOAD_JOBS)
            .context("Load jobs CF not found")?;
        let cf_job_index = self
            .db
            .cf_handle(CF_LOAD_JOB_INDEX)
            .context("Load job index CF not found")?;
        let cf_session_index = self
            .db
            .cf_handle(CF_LOAD_JOB_SESSION_INDEX)
            .context("Load job session index CF not found")?;

        let json = serde_json::to_vec(job).context("Failed to serialize load job")?;
        self.db
            .put_cf(cf_jobs, job.id.as_bytes(), json)
            .context("Failed to store load job")?;

        self.db
            .put_cf(
                cf_job_index,
                job.id.as_bytes(),
                job.created_at.to_le_bytes().to_vec(),
            )
            .context("Failed to store load job index")?;

        let session_key = format!("{}:{:020}:{}", job.session_id, job.created_at, job.id);
        self.db
            .put_cf(cf_session_index, session_key.as_bytes(), job.id.as_bytes())
            .context("Failed to store load job session index")?;

        Ok(())
    }

    /// Fetch a unified load job by ID.
    pub fn get_load_job(&self, job_id: &str) -> Result<Option<UnifiedLoadJob>> {
        let cf_jobs = self
            .db
            .cf_handle(CF_LOAD_JOBS)
            .context("Load jobs CF not found")?;

        match self.db.get_cf(cf_jobs, job_id.as_bytes())? {
            Some(bytes) => {
                let job: UnifiedLoadJob = serde_json::from_slice(&bytes)
                    .context("Failed to deserialize unified load job")?;
                Ok(Some(job))
            }
            None => Ok(None),
        }
    }

    /// Update load job status and optional error/external ID details.
    pub fn update_load_job_status(
        &self,
        job_id: &str,
        status: UnifiedLoadJobStatus,
        error_message: Option<String>,
        external_run_id: Option<String>,
    ) -> Result<()> {
        let mut job = self
            .get_load_job(job_id)?
            .ok_or_else(|| anyhow::anyhow!("Load job not found: {}", job_id))?;

        job.status = status;
        job.updated_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        if job.started_at.is_none() && matches!(job.status, UnifiedLoadJobStatus::Running) {
            job.started_at = Some(job.updated_at);
        }
        if matches!(
            job.status,
            UnifiedLoadJobStatus::Completed
                | UnifiedLoadJobStatus::Failed
                | UnifiedLoadJobStatus::Cancelled
        ) {
            job.completed_at = Some(job.updated_at);
        }
        if error_message.is_some() {
            job.error_message = error_message;
        }
        if external_run_id.is_some() {
            job.external_run_id = external_run_id;
        }

        self.store_load_job(&job)
    }

    /// Update load job progress metrics.
    pub fn update_load_job_progress(
        &self,
        job_id: &str,
        progress: super::types::UnifiedLoadProgress,
    ) -> Result<()> {
        let mut job = self
            .get_load_job(job_id)?
            .ok_or_else(|| anyhow::anyhow!("Load job not found: {}", job_id))?;
        job.progress = progress;
        job.updated_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        self.store_load_job(&job)
    }

    /// List load jobs for a unified mapping session ordered by most recent.
    pub fn list_load_jobs_for_session(
        &self,
        session_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<UnifiedLoadJob>> {
        let cf_session_index = self
            .db
            .cf_handle(CF_LOAD_JOB_SESSION_INDEX)
            .context("Load job session index CF not found")?;

        let mut keyed_ids: Vec<(i64, String)> = Vec::new();
        let prefix = format!("{}:", session_id);
        for entry in self
            .db
            .iterator_cf(cf_session_index, rocksdb::IteratorMode::Start)
        {
            let (key, value) = entry.context("Failed to read load job session index")?;
            let key_str = String::from_utf8(key.to_vec()).context("Invalid UTF-8 job key")?;
            if key_str.starts_with(&prefix) {
                // Key format: "{session_id}:{created_at:020}:{job_id}"
                // Parse timestamp from right so session IDs can safely include ':'.
                let mut right_split = key_str.rsplitn(2, ':');
                let _job_id_suffix = right_split.next();
                let session_and_time = right_split.next().unwrap_or_default();
                let created_at = session_and_time
                    .rsplit(':')
                    .next()
                    .and_then(|ts| ts.parse::<i64>().ok())
                    .unwrap_or(0);

                keyed_ids.push((
                    created_at,
                    String::from_utf8(value.to_vec()).context("Invalid UTF-8 job id")?,
                ));
            }
        }

        keyed_ids.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
        if let Some(limit) = limit {
            keyed_ids.truncate(limit);
        }

        let mut jobs = Vec::new();
        for (_, id) in keyed_ids {
            if let Some(job) = self.get_load_job(&id)? {
                jobs.push(job);
            }
        }
        Ok(jobs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::multi_source::types::{
        ConflictResolution, SourceFieldRef, TargetColumnRef, TargetDatabaseConfig,
        UnifiedFieldMapping, UnifiedLoadJob, UnifiedLoadJobStatus, UnifiedLoadProgress,
    };
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn create_test_session(id: &str, status: UnifiedSessionStatus) -> UnifiedMappingSession {
        UnifiedMappingSession {
            id: id.to_string(),
            source_sessions: vec!["session_001".to_string(), "session_002".to_string()],
            target_database: TargetDatabaseConfig {
                datasource_id: "test_postgres".to_string(),
                schema: "public".to_string(),
                tables: HashMap::new(),
            },
            field_mappings: vec![],
            conflicts: vec![],
            status,
            created_at: 1697356800,
            created_by: "test_user".to_string(),
            updated_at: 1697356800,
        }
    }

    fn create_test_load_job(
        id: &str,
        session_id: &str,
        created_at: i64,
        status: UnifiedLoadJobStatus,
    ) -> UnifiedLoadJob {
        UnifiedLoadJob {
            id: id.to_string(),
            session_id: session_id.to_string(),
            database_type: "databricks".to_string(),
            status,
            progress: UnifiedLoadProgress::default(),
            started_at: None,
            completed_at: None,
            error_message: None,
            external_run_id: None,
            created_at,
            updated_at: created_at,
        }
    }

    #[test]
    fn test_storage_initialization() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let storage = UnifiedMappingStorage::new(temp_dir.path())?;

        // Verify storage was created
        assert!(temp_dir.path().exists());

        // Verify statistics are empty
        let stats = storage.get_statistics()?;
        assert_eq!(stats.total_sessions, 0);

        Ok(())
    }

    #[test]
    fn test_store_and_retrieve_session() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let storage = UnifiedMappingStorage::new(temp_dir.path())?;

        let session = create_test_session("unified_001", UnifiedSessionStatus::Building);

        // Store session
        storage.store_session(&session)?;

        // Retrieve session
        let retrieved = storage.get_session("unified_001")?;
        assert!(retrieved.is_some());

        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.id, "unified_001");
        assert_eq!(retrieved.source_sessions.len(), 2);
        assert_eq!(retrieved.created_by, "test_user");

        Ok(())
    }

    #[test]
    fn test_retrieve_nonexistent_session() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let storage = UnifiedMappingStorage::new(temp_dir.path())?;

        let retrieved = storage.get_session("nonexistent")?;
        assert!(retrieved.is_none());

        Ok(())
    }

    #[test]
    fn test_update_session_status() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let storage = UnifiedMappingStorage::new(temp_dir.path())?;

        let session = create_test_session("unified_001", UnifiedSessionStatus::Building);
        storage.store_session(&session)?;

        // Update status
        storage.update_session_status("unified_001", UnifiedSessionStatus::ReadyToLoad)?;

        // Verify update
        let retrieved = storage.get_session("unified_001")?.unwrap();
        assert!(matches!(
            retrieved.status,
            UnifiedSessionStatus::ReadyToLoad
        ));
        assert!(retrieved.updated_at > session.updated_at);

        Ok(())
    }

    #[test]
    fn test_list_sessions_ordered_by_time() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let storage = UnifiedMappingStorage::new(temp_dir.path())?;

        // Store sessions with different timestamps
        let mut session1 = create_test_session("unified_001", UnifiedSessionStatus::Building);
        session1.created_at = 1000;
        storage.store_session(&session1)?;

        let mut session2 = create_test_session("unified_002", UnifiedSessionStatus::Building);
        session2.created_at = 2000;
        storage.store_session(&session2)?;

        let mut session3 = create_test_session("unified_003", UnifiedSessionStatus::Building);
        session3.created_at = 1500;
        storage.store_session(&session3)?;

        // List sessions
        let sessions = storage.list_sessions(None)?;
        assert_eq!(sessions.len(), 3);

        // Should be ordered by time (newest first)
        assert_eq!(sessions[0].id, "unified_002"); // 2000
        assert_eq!(sessions[1].id, "unified_003"); // 1500
        assert_eq!(sessions[2].id, "unified_001"); // 1000

        Ok(())
    }

    #[test]
    fn test_list_sessions_with_limit() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let storage = UnifiedMappingStorage::new(temp_dir.path())?;

        // Store 5 sessions
        for i in 1..=5 {
            let session =
                create_test_session(&format!("unified_{:03}", i), UnifiedSessionStatus::Building);
            storage.store_session(&session)?;
        }

        // List with limit
        let sessions = storage.list_sessions(Some(2))?;
        assert_eq!(sessions.len(), 2);

        Ok(())
    }

    #[test]
    fn test_list_sessions_by_status() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let storage = UnifiedMappingStorage::new(temp_dir.path())?;

        // Store sessions with different statuses
        let session1 = create_test_session("unified_001", UnifiedSessionStatus::Building);
        storage.store_session(&session1)?;

        let session2 = create_test_session("unified_002", UnifiedSessionStatus::ReadyToLoad);
        storage.store_session(&session2)?;

        let session3 = create_test_session("unified_003", UnifiedSessionStatus::Building);
        storage.store_session(&session3)?;

        let session4 = create_test_session("unified_004", UnifiedSessionStatus::Completed);
        storage.store_session(&session4)?;

        // List by status
        let building_sessions = storage.list_sessions_by_status(&UnifiedSessionStatus::Building)?;
        assert_eq!(building_sessions.len(), 2);

        let ready_sessions = storage.list_sessions_by_status(&UnifiedSessionStatus::ReadyToLoad)?;
        assert_eq!(ready_sessions.len(), 1);

        let completed_sessions =
            storage.list_sessions_by_status(&UnifiedSessionStatus::Completed)?;
        assert_eq!(completed_sessions.len(), 1);

        Ok(())
    }

    #[test]
    fn test_delete_session() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let storage = UnifiedMappingStorage::new(temp_dir.path())?;

        let session = create_test_session("unified_001", UnifiedSessionStatus::Building);
        storage.store_session(&session)?;

        // Verify stored
        assert!(storage.get_session("unified_001")?.is_some());

        // Delete
        storage.delete_session("unified_001")?;

        // Verify deleted
        assert!(storage.get_session("unified_001")?.is_none());

        Ok(())
    }

    #[test]
    fn test_delete_nonexistent_session() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let storage = UnifiedMappingStorage::new(temp_dir.path())?;

        // Should return error for nonexistent session
        let result = storage.delete_session("nonexistent");
        assert!(result.is_err());

        Ok(())
    }

    #[test]
    fn test_storage_statistics_empty() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let storage = UnifiedMappingStorage::new(temp_dir.path())?;

        let stats = storage.get_statistics()?;
        assert_eq!(stats.total_sessions, 0);
        assert_eq!(stats.total_source_sessions, 0);
        assert_eq!(stats.total_field_mappings, 0);

        Ok(())
    }

    #[test]
    fn test_storage_statistics_with_sessions() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let storage = UnifiedMappingStorage::new(temp_dir.path())?;

        // Create sessions with field mappings
        let mut session1 = create_test_session("unified_001", UnifiedSessionStatus::Building);
        session1.field_mappings = vec![
            UnifiedFieldMapping {
                id: "mapping_001".to_string(),
                source_fields: vec![],
                ontology_term_uri: "http://schema.org/email".to_string(),
                target_column: TargetColumnRef {
                    table_name: "customers".to_string(),
                    column_name: "email".to_string(),
                    data_type: "VARCHAR(255)".to_string(),
                },
                conflict_resolution: ConflictResolution::NoConflict,
                transformation: None,
                confidence: 0.95,
            },
            UnifiedFieldMapping {
                id: "mapping_002".to_string(),
                source_fields: vec![],
                ontology_term_uri: "http://schema.org/name".to_string(),
                target_column: TargetColumnRef {
                    table_name: "customers".to_string(),
                    column_name: "name".to_string(),
                    data_type: "VARCHAR(255)".to_string(),
                },
                conflict_resolution: ConflictResolution::NoConflict,
                transformation: None,
                confidence: 0.9,
            },
        ];
        storage.store_session(&session1)?;

        let mut session2 = create_test_session("unified_002", UnifiedSessionStatus::ReadyToLoad);
        session2.source_sessions = vec!["session_003".to_string()]; // Different source
        session2.field_mappings = vec![UnifiedFieldMapping {
            id: "mapping_003".to_string(),
            source_fields: vec![],
            ontology_term_uri: "http://schema.org/price".to_string(),
            target_column: TargetColumnRef {
                table_name: "products".to_string(),
                column_name: "price".to_string(),
                data_type: "DECIMAL".to_string(),
            },
            conflict_resolution: ConflictResolution::NoConflict,
            transformation: None,
            confidence: 0.85,
        }];
        storage.store_session(&session2)?;

        // Get statistics
        let stats = storage.get_statistics()?;

        assert_eq!(stats.total_sessions, 2);
        assert_eq!(stats.by_status.get("Building"), Some(&1));
        assert_eq!(stats.by_status.get("ReadyToLoad"), Some(&1));
        assert_eq!(stats.total_source_sessions, 3); // session_001, session_002, session_003
        assert_eq!(stats.total_field_mappings, 3);

        Ok(())
    }

    #[test]
    fn test_session_with_field_mappings() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let storage = UnifiedMappingStorage::new(temp_dir.path())?;

        let mut session = create_test_session("unified_001", UnifiedSessionStatus::Building);
        session.field_mappings = vec![UnifiedFieldMapping {
            id: "mapping_001".to_string(),
            source_fields: vec![SourceFieldRef {
                session_id: "session_001".to_string(),
                datasource_id: "csv_001".to_string(),
                table_name: "customers.csv".to_string(),
                field_name: "email".to_string(),
                source_data_type: "TEXT".to_string(),
            }],
            ontology_term_uri: "http://schema.org/email".to_string(),
            target_column: TargetColumnRef {
                table_name: "customers".to_string(),
                column_name: "email_address".to_string(),
                data_type: "VARCHAR(255)".to_string(),
            },
            conflict_resolution: ConflictResolution::NoConflict,
            transformation: None,
            confidence: 0.95,
        }];

        // Store and retrieve
        storage.store_session(&session)?;
        let retrieved = storage.get_session("unified_001")?.unwrap();

        assert_eq!(retrieved.field_mappings.len(), 1);
        assert_eq!(retrieved.field_mappings[0].id, "mapping_001");
        assert_eq!(
            retrieved.field_mappings[0].ontology_term_uri,
            "http://schema.org/email"
        );
        assert_eq!(retrieved.field_mappings[0].source_fields.len(), 1);

        Ok(())
    }

    #[test]
    fn test_status_index_updates_on_status_change() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let storage = UnifiedMappingStorage::new(temp_dir.path())?;

        let session = create_test_session("unified_001", UnifiedSessionStatus::Building);
        storage.store_session(&session)?;

        // Verify in Building list
        let building_sessions = storage.list_sessions_by_status(&UnifiedSessionStatus::Building)?;
        assert_eq!(building_sessions.len(), 1);

        // Update status
        storage.update_session_status("unified_001", UnifiedSessionStatus::Completed)?;

        // Verify no longer in Building list
        let building_sessions = storage.list_sessions_by_status(&UnifiedSessionStatus::Building)?;
        assert_eq!(building_sessions.len(), 0);

        // Verify in Completed list
        let completed_sessions =
            storage.list_sessions_by_status(&UnifiedSessionStatus::Completed)?;
        assert_eq!(completed_sessions.len(), 1);

        Ok(())
    }

    #[test]
    fn test_store_and_retrieve_load_job() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let storage = UnifiedMappingStorage::new(temp_dir.path())?;

        let job = create_test_load_job(
            "loadjob_001",
            "unified_001",
            1_700_000_000,
            UnifiedLoadJobStatus::Queued,
        );
        storage.store_load_job(&job)?;

        let loaded = storage.get_load_job("loadjob_001")?.expect("load job");
        assert_eq!(loaded.id, "loadjob_001");
        assert_eq!(loaded.session_id, "unified_001");
        assert_eq!(loaded.database_type, "databricks");
        assert_eq!(loaded.status, UnifiedLoadJobStatus::Queued);
        Ok(())
    }

    #[test]
    fn test_update_load_job_status_tracks_run_metadata() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let storage = UnifiedMappingStorage::new(temp_dir.path())?;

        let job = create_test_load_job(
            "loadjob_002",
            "unified_001",
            1_700_000_000,
            UnifiedLoadJobStatus::Queued,
        );
        storage.store_load_job(&job)?;

        storage.update_load_job_status("loadjob_002", UnifiedLoadJobStatus::Running, None, None)?;
        storage.update_load_job_status(
            "loadjob_002",
            UnifiedLoadJobStatus::Submitted,
            None,
            Some("ext_run_123".to_string()),
        )?;

        let updated = storage.get_load_job("loadjob_002")?.expect("load job");
        assert_eq!(updated.status, UnifiedLoadJobStatus::Submitted);
        assert!(updated.started_at.is_some());
        assert_eq!(updated.external_run_id.as_deref(), Some("ext_run_123"));
        assert!(updated.completed_at.is_none());
        Ok(())
    }

    #[test]
    fn test_list_load_jobs_for_session_returns_newest_first() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let storage = UnifiedMappingStorage::new(temp_dir.path())?;

        let old_job = create_test_load_job(
            "loadjob_old",
            "unified_001",
            1_700_000_001,
            UnifiedLoadJobStatus::Queued,
        );
        let new_job = create_test_load_job(
            "loadjob_new",
            "unified_001",
            1_700_000_099,
            UnifiedLoadJobStatus::Queued,
        );
        let other_session_job = create_test_load_job(
            "loadjob_other",
            "unified_002",
            1_700_000_200,
            UnifiedLoadJobStatus::Queued,
        );

        storage.store_load_job(&old_job)?;
        storage.store_load_job(&new_job)?;
        storage.store_load_job(&other_session_job)?;

        let jobs = storage.list_load_jobs_for_session("unified_001", None)?;
        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[0].id, "loadjob_new");
        assert_eq!(jobs[1].id, "loadjob_old");
        Ok(())
    }
}
