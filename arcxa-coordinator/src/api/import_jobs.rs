//! Background Import Job Management
//!
//! Handles async dataset imports with status tracking and progress reporting.

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use std::sync::Arc;

use crate::api::dto::datasets::*;

/// Import job manager with concurrent status tracking
#[derive(Clone)]
pub struct ImportJobManager {
    /// Active and completed job statuses (thread-safe)
    jobs: Arc<DashMap<String, ImportJobStatus>>,
}

impl ImportJobManager {
    pub fn new() -> Self {
        Self {
            jobs: Arc::new(DashMap::new()),
        }
    }

    /// Register a new import job
    pub fn register_job(&self, import_id: String, request: ImportJobRequest) {
        let status = ImportJobStatus {
            import_id: import_id.clone(),
            status: ImportStatus::Pending,
            progress: 0,
            dataset_id: None,
            dataset_name: request.name.clone(),
            started_at: Utc::now(),
            completed_at: None,
            records_processed: 0,
            records_failed: 0,
            errors: Vec::new(),
            profile: None,
        };

        self.jobs.insert(import_id, status);
    }

    /// Update job progress
    pub fn update_progress(&self, import_id: &str, progress: u8, records_processed: u64) {
        if let Some(mut job) = self.jobs.get_mut(import_id) {
            job.progress = progress;
            job.records_processed = records_processed;
            job.status = ImportStatus::Processing;
        }
    }

    /// Mark job as completed
    pub fn complete_job(
        &self,
        import_id: &str,
        dataset_id: String,
        record_count: u64,
        profile: Option<ImportProfile>,
    ) {
        if let Some(mut job) = self.jobs.get_mut(import_id) {
            job.status = ImportStatus::Imported;
            job.progress = 100;
            job.dataset_id = Some(dataset_id);
            job.records_processed = record_count;
            job.completed_at = Some(Utc::now());
            job.profile = profile;
        }
    }

    /// Mark job as failed
    pub fn fail_job(&self, import_id: &str, error: ImportError) {
        if let Some(mut job) = self.jobs.get_mut(import_id) {
            job.status = ImportStatus::Failed;
            job.completed_at = Some(Utc::now());
            job.errors.push(error);
        }
    }

    /// Get job status
    pub fn get_status(&self, import_id: &str) -> Option<ImportJobStatus> {
        self.jobs.get(import_id).map(|entry| entry.value().clone())
    }

    /// List all jobs (with optional status filter)
    pub fn list_jobs(
        &self,
        status_filter: Option<ImportStatus>,
        limit: usize,
    ) -> Vec<ImportSummary> {
        let mut jobs: Vec<_> = self
            .jobs
            .iter()
            .filter_map(|entry| {
                let job = entry.value();

                // Apply status filter
                if let Some(ref filter) = status_filter {
                    if job.status != *filter {
                        return None;
                    }
                }

                Some(ImportSummary {
                    import_id: job.import_id.clone(),
                    dataset_id: job.dataset_id.clone(),
                    dataset_name: job.dataset_name.clone(),
                    status: job.status.clone(),
                    record_count: job.records_processed,
                    imported_by: "user_admin".to_string(), // TODO: Track from job
                    imported_at: job.started_at.to_rfc3339(),
                })
            })
            .collect();

        // Sort by start time (most recent first)
        jobs.sort_by(|a, b| b.imported_at.cmp(&a.imported_at));
        jobs.truncate(limit);
        jobs
    }

    /// Clean up old completed jobs (keep last 1000)
    pub fn cleanup_old_jobs(&self) {
        if self.jobs.len() > 1000 {
            let mut completed: Vec<_> = self
                .jobs
                .iter()
                .filter(|e| {
                    matches!(
                        e.value().status,
                        ImportStatus::Imported | ImportStatus::Failed
                    )
                })
                .map(|e| (e.key().clone(), e.value().started_at))
                .collect();

            completed.sort_by(|a, b| a.1.cmp(&b.1));

            // Remove oldest 20%
            let remove_count = completed.len() / 5;
            for (id, _) in completed.iter().take(remove_count) {
                self.jobs.remove(id);
            }
        }
    }
}

impl Default for ImportJobManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Import job status (stored in memory)
#[derive(Debug, Clone)]
pub struct ImportJobStatus {
    pub import_id: String,
    pub status: ImportStatus,
    pub progress: u8,
    pub dataset_id: Option<String>,
    pub dataset_name: Option<String>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub records_processed: u64,
    pub records_failed: u64,
    pub errors: Vec<ImportError>,
    pub profile: Option<ImportProfile>,
}

/// Quality profile results
#[derive(Debug, Clone, serde::Serialize)]
pub struct ImportProfile {
    pub quality_score: u8,
    pub completeness: u8,
    pub validity: u8,
    pub uniqueness: u8,
    pub column_profiles: Vec<ColumnProfileSummary>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ColumnProfileSummary {
    pub name: String,
    pub null_count: u64,
    pub distinct_count: u64,
    pub top_values: Vec<(String, u64)>,
}

/// Import job request
#[derive(Debug, Clone)]
pub struct ImportJobRequest {
    pub name: Option<String>,
    pub source_id: String,
    pub table: String,
    pub schema: Option<String>,
    pub where_clause: Option<String>,
    pub columns: Vec<String>,
    pub limit: Option<usize>,
    pub profile: bool,
    pub tags: Vec<String>,
    pub description: Option<String>,
}

impl From<&DatasourceImportRequest> for ImportJobRequest {
    fn from(req: &DatasourceImportRequest) -> Self {
        Self {
            name: req.name.clone(),
            source_id: req.source_id.clone(),
            table: req.table.clone(),
            schema: req.schema.clone(),
            where_clause: req.where_clause.clone(),
            columns: req.columns.clone(),
            limit: req.limit,
            profile: req.profile,
            tags: req.tags.clone(),
            description: req.description.clone(),
        }
    }
}

/// Convert job status to API response
impl From<&ImportJobStatus> for ImportStatusResponse {
    fn from(job: &ImportJobStatus) -> Self {
        Self {
            import_id: job.import_id.clone(),
            status: job.status.clone(),
            progress: job.progress,
            dataset_id: job.dataset_id.clone(),
            started_at: job.started_at.to_rfc3339(),
            completed_at: job.completed_at.map(|dt| dt.to_rfc3339()),
            records_processed: job.records_processed,
            records_failed: job.records_failed,
            errors: job.errors.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_job_lifecycle() {
        let manager = ImportJobManager::new();
        let import_id = "test_import_123".to_string();

        let request = ImportJobRequest {
            name: Some("Test Import".to_string()),
            source_id: "source_1".to_string(),
            table: "customers".to_string(),
            schema: None,
            where_clause: None,
            columns: vec![],
            limit: None,
            profile: false,
            tags: vec![],
            description: None,
        };

        // Register job
        manager.register_job(import_id.clone(), request);
        let status = manager.get_status(&import_id).unwrap();
        assert_eq!(status.status, ImportStatus::Pending);
        assert_eq!(status.progress, 0);
        assert_eq!(status.records_processed, 0);

        // Update progress
        manager.update_progress(&import_id, 50, 5000);
        let status = manager.get_status(&import_id).unwrap();
        assert_eq!(status.progress, 50);
        assert_eq!(status.records_processed, 5000);
        assert_eq!(status.status, ImportStatus::Processing);

        // Complete job
        manager.complete_job(&import_id, "dataset_123".to_string(), 10000, None);
        let status = manager.get_status(&import_id).unwrap();
        assert_eq!(status.status, ImportStatus::Imported);
        assert_eq!(status.progress, 100);
        assert_eq!(status.dataset_id, Some("dataset_123".to_string()));
        assert_eq!(status.records_processed, 10000);
        assert!(status.completed_at.is_some());
    }

    #[test]
    fn test_job_failure() {
        let manager = ImportJobManager::new();
        let import_id = "test_import_fail".to_string();

        let request = ImportJobRequest {
            name: Some("Failing Import".to_string()),
            source_id: "source_1".to_string(),
            table: "invalid_table".to_string(),
            schema: None,
            where_clause: None,
            columns: vec![],
            limit: None,
            profile: false,
            tags: vec![],
            description: None,
        };

        manager.register_job(import_id.clone(), request);

        // Fail job
        let error = ImportError {
            row: Some(100),
            column: Some("email".to_string()),
            message: "Invalid email format".to_string(),
            code: "VALIDATION_ERROR".to_string(),
        };

        manager.fail_job(&import_id, error);

        let status = manager.get_status(&import_id).unwrap();
        assert_eq!(status.status, ImportStatus::Failed);
        assert!(status.completed_at.is_some());
        assert_eq!(status.errors.len(), 1);
        assert_eq!(status.errors[0].code, "VALIDATION_ERROR");
        assert_eq!(status.errors[0].message, "Invalid email format");
    }

    #[test]
    fn test_progress_updates() {
        let manager = ImportJobManager::new();
        let import_id = "test_progress".to_string();

        let request = ImportJobRequest {
            name: Some("Progress Test".to_string()),
            source_id: "source_1".to_string(),
            table: "data".to_string(),
            schema: None,
            where_clause: None,
            columns: vec![],
            limit: None,
            profile: false,
            tags: vec![],
            description: None,
        };

        manager.register_job(import_id.clone(), request);

        // Simulate progress updates at various stages
        let checkpoints = vec![(10, 0), (20, 0), (50, 5000), (80, 8000), (90, 9000)];

        for (progress, records) in checkpoints {
            manager.update_progress(&import_id, progress, records);
            let status = manager.get_status(&import_id).unwrap();
            assert_eq!(status.progress, progress);
            assert_eq!(status.records_processed, records);
            assert_eq!(status.status, ImportStatus::Processing);
        }
    }

    #[test]
    fn test_job_with_profile() {
        let manager = ImportJobManager::new();
        let import_id = "test_profile".to_string();

        let request = ImportJobRequest {
            name: Some("Profiled Import".to_string()),
            source_id: "source_1".to_string(),
            table: "customers".to_string(),
            schema: None,
            where_clause: None,
            columns: vec![],
            limit: None,
            profile: true,
            tags: vec![],
            description: None,
        };

        manager.register_job(import_id.clone(), request);

        // Create profile data
        let profile = ImportProfile {
            quality_score: 85,
            completeness: 90,
            validity: 95,
            uniqueness: 70,
            column_profiles: vec![ColumnProfileSummary {
                name: "email".to_string(),
                null_count: 5,
                distinct_count: 995,
                top_values: vec![
                    ("john@example.com".to_string(), 2),
                    ("jane@example.com".to_string(), 1),
                ],
            }],
        };

        manager.complete_job(&import_id, "ds_123".to_string(), 1000, Some(profile));

        let status = manager.get_status(&import_id).unwrap();
        assert!(status.profile.is_some());

        let profile = status.profile.unwrap();
        assert_eq!(profile.quality_score, 85);
        assert_eq!(profile.completeness, 90);
        assert_eq!(profile.validity, 95);
        assert_eq!(profile.uniqueness, 70);
        assert_eq!(profile.column_profiles.len(), 1);
        assert_eq!(profile.column_profiles[0].name, "email");
    }

    #[test]
    fn test_list_jobs_filter() {
        let manager = ImportJobManager::new();

        for i in 0..5 {
            let request = ImportJobRequest {
                name: Some(format!("Import {}", i)),
                source_id: "source_1".to_string(),
                table: "table".to_string(),
                schema: None,
                where_clause: None,
                columns: vec![],
                limit: None,
                profile: false,
                tags: vec![],
                description: None,
            };
            manager.register_job(format!("import_{}", i), request);
        }

        // Complete some jobs
        manager.complete_job("import_0", "ds_0".to_string(), 100, None);
        manager.complete_job("import_1", "ds_1".to_string(), 200, None);

        // Fail one job
        manager.fail_job(
            "import_2",
            ImportError {
                row: None,
                column: None,
                message: "Test error".to_string(),
                code: "TEST_ERROR".to_string(),
            },
        );

        // List all jobs
        let all_jobs = manager.list_jobs(None, 100);
        assert_eq!(all_jobs.len(), 5);

        // List only completed
        let completed = manager.list_jobs(Some(ImportStatus::Imported), 100);
        assert_eq!(completed.len(), 2);

        // List only pending
        let pending = manager.list_jobs(Some(ImportStatus::Pending), 100);
        assert_eq!(pending.len(), 2);

        // List only failed
        let failed = manager.list_jobs(Some(ImportStatus::Failed), 100);
        assert_eq!(failed.len(), 1);
    }

    #[test]
    fn test_list_jobs_pagination() {
        let manager = ImportJobManager::new();

        // Create 10 jobs
        for i in 0..10 {
            let request = ImportJobRequest {
                name: Some(format!("Import {}", i)),
                source_id: "source_1".to_string(),
                table: "table".to_string(),
                schema: None,
                where_clause: None,
                columns: vec![],
                limit: None,
                profile: false,
                tags: vec![],
                description: None,
            };
            manager.register_job(format!("import_{:02}", i), request);
        }

        // Test pagination
        let page1 = manager.list_jobs(None, 5);
        assert_eq!(page1.len(), 5);

        let page2 = manager.list_jobs(None, 3);
        assert_eq!(page2.len(), 3);

        let all = manager.list_jobs(None, 100);
        assert_eq!(all.len(), 10);
    }

    #[test]
    fn test_nonexistent_job() {
        let manager = ImportJobManager::new();
        let status = manager.get_status("nonexistent_id");
        assert!(status.is_none());
    }

    #[test]
    fn test_update_nonexistent_job() {
        let manager = ImportJobManager::new();

        // Should not panic, just silently no-op
        manager.update_progress("nonexistent_id", 50, 1000);

        let status = manager.get_status("nonexistent_id");
        assert!(status.is_none());
    }

    #[test]
    fn test_complete_nonexistent_job() {
        let manager = ImportJobManager::new();

        // Should not panic, just silently no-op
        manager.complete_job("nonexistent_id", "ds_123".to_string(), 1000, None);

        let status = manager.get_status("nonexistent_id");
        assert!(status.is_none());
    }

    #[test]
    fn test_concurrent_access() {
        use std::sync::Arc;
        use std::thread;

        let manager = Arc::new(ImportJobManager::new());
        let mut handles = vec![];

        // Spawn multiple threads registering jobs
        for i in 0..10 {
            let manager_clone = manager.clone();
            let handle = thread::spawn(move || {
                let request = ImportJobRequest {
                    name: Some(format!("Concurrent Import {}", i)),
                    source_id: "source_1".to_string(),
                    table: "table".to_string(),
                    schema: None,
                    where_clause: None,
                    columns: vec![],
                    limit: None,
                    profile: false,
                    tags: vec![],
                    description: None,
                };
                manager_clone.register_job(format!("import_{}", i), request);

                // Update progress
                manager_clone.update_progress(&format!("import_{}", i), 50, 500);

                // Complete job
                manager_clone.complete_job(
                    &format!("import_{}", i),
                    format!("ds_{}", i),
                    1000,
                    None,
                );
            });
            handles.push(handle);
        }

        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }

        // Verify all jobs completed
        let all_jobs = manager.list_jobs(None, 100);
        assert_eq!(all_jobs.len(), 10);

        let completed = manager.list_jobs(Some(ImportStatus::Imported), 100);
        assert_eq!(completed.len(), 10);
    }

    #[test]
    fn test_cleanup_old_jobs() {
        let manager = ImportJobManager::new();

        // Create many jobs to trigger cleanup
        for i in 0..1100 {
            let request = ImportJobRequest {
                name: Some(format!("Import {}", i)),
                source_id: "source_1".to_string(),
                table: "table".to_string(),
                schema: None,
                where_clause: None,
                columns: vec![],
                limit: None,
                profile: false,
                tags: vec![],
                description: None,
            };
            manager.register_job(format!("import_{:04}", i), request);

            // Complete most jobs
            if i < 1000 {
                manager.complete_job(&format!("import_{:04}", i), format!("ds_{}", i), 100, None);
            }
        }

        // Trigger cleanup
        manager.cleanup_old_jobs();

        // Should have removed some old jobs
        let all_jobs = manager.list_jobs(None, 10000);
        assert!(all_jobs.len() <= 1000, "Should have cleaned up old jobs");
    }

    #[test]
    fn test_job_request_from_datasource_import() {
        use crate::api::dto::datasets::DatasourceImportRequest;

        let datasource_req = DatasourceImportRequest {
            source_id: "pg_source".to_string(),
            table: "users".to_string(),
            schema: Some("public".to_string()),
            name: Some("User Import".to_string()),
            where_clause: Some("active = true".to_string()),
            columns: vec!["id".to_string(), "email".to_string()],
            limit: Some(1000),
            profile: true,
            async_mode: false,
            tags: vec!["prod".to_string(), "users".to_string()],
            description: Some("Production user data".to_string()),
            incremental: None,
        };

        let job_req = ImportJobRequest::from(&datasource_req);

        assert_eq!(job_req.source_id, "pg_source");
        assert_eq!(job_req.table, "users");
        assert_eq!(job_req.schema, Some("public".to_string()));
        assert_eq!(job_req.name, Some("User Import".to_string()));
        assert_eq!(job_req.where_clause, Some("active = true".to_string()));
        assert_eq!(job_req.columns.len(), 2);
        assert_eq!(job_req.limit, Some(1000));
        assert_eq!(job_req.profile, true);
        assert_eq!(job_req.tags.len(), 2);
    }

    #[test]
    fn test_import_status_response_conversion() {
        let job_status = ImportJobStatus {
            import_id: "import_123".to_string(),
            status: ImportStatus::Processing,
            progress: 75,
            dataset_id: None,
            dataset_name: Some("Test Dataset".to_string()),
            started_at: Utc::now(),
            completed_at: None,
            records_processed: 7500,
            records_failed: 10,
            errors: vec![],
            profile: None,
        };

        let response = crate::api::dto::datasets::ImportStatusResponse::from(&job_status);

        assert_eq!(response.import_id, "import_123");
        assert_eq!(response.progress, 75);
        assert_eq!(response.records_processed, 7500);
        assert_eq!(response.records_failed, 10);
        assert!(response.completed_at.is_none());
    }
}
