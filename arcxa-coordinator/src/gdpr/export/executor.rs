//! Async Export Job Executor
//!
//! Orchestrates the complete export workflow:
//! 1. Discovery: Scan data stores to find user's personal data
//! 2. Collection: Retrieve actual data from storage (future)
//! 3. Conversion: Convert to requested format
//! 4. Storage: Save export file
//! 5. Finalization: Generate download URL with expiry
//!
//! ## Usage
//!
//! ```ignore
//! let executor = ExportExecutor::new(
//!     job_store,
//!     discovery_service,
//!     converter,
//!     "/exports".to_string(),
//! );
//!
//! // Execute export job asynchronously
//! executor.execute_job(job_id).await?;
//! ```
//!
//! ## Error Handling
//!
//! If any phase fails, the job is marked as Failed with error details.
//! Jobs can be retried by calling execute_job again.

use super::converters::FormatConverter;
use super::discovery::{DataDiscoveryService, DiscoveryResult};
use super::storage::ExportJobStore;
use super::types::{
    ExportError, ExportErrorCode, ExportJob, ExportPhase, ExportResult, ExportStatus,
};
use anyhow::{Context, Result};
use chrono::Utc;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs;
use uuid::Uuid;

/// Export job executor
pub struct ExportExecutor {
    /// Job storage
    pub job_store: Arc<ExportJobStore>,

    /// Data discovery service
    discovery: Arc<DataDiscoveryService>,

    /// Format converter
    converter: Arc<FormatConverter>,

    /// Export files base directory
    export_dir: PathBuf,

    /// Default expiry hours for downloads
    default_expiry_hours: i64,
}

impl ExportExecutor {
    /// Create new export executor
    ///
    /// # Arguments
    /// * `job_store` - Storage for export jobs
    /// * `discovery` - Data discovery service
    /// * `converter` - Format converter
    /// * `export_dir` - Directory to store export files
    pub fn new(
        job_store: Arc<ExportJobStore>,
        discovery: Arc<DataDiscoveryService>,
        converter: Arc<FormatConverter>,
        export_dir: impl Into<PathBuf>,
    ) -> Self {
        Self {
            job_store,
            discovery,
            converter,
            export_dir: export_dir.into(),
            default_expiry_hours: 48, // 48 hour default
        }
    }

    /// Set custom expiry duration
    pub fn with_expiry_hours(mut self, hours: i64) -> Self {
        self.default_expiry_hours = hours;
        self
    }

    /// Execute an export job
    ///
    /// This runs the complete export workflow asynchronously.
    /// Job progress is persisted to storage at each phase.
    pub async fn execute_job(&self, job_id: Uuid) -> Result<()> {
        // Load job
        let mut job = self
            .job_store
            .get(job_id)?
            .ok_or_else(|| anyhow::anyhow!("Export job not found: {}", job_id))?;

        // Check if job is already completed
        if matches!(
            job.status,
            ExportStatus::Ready | ExportStatus::Failed | ExportStatus::Cancelled
        ) {
            return Ok(());
        }

        // Mark as processing
        job.status = ExportStatus::Processing;
        job.updated_at = Utc::now();
        self.job_store.save(&job)?;

        // Execute workflow
        match self.execute_workflow(&mut job).await {
            Ok(()) => {
                tracing::info!("Export job {} completed successfully", job_id);
                Ok(())
            }
            Err(e) => {
                tracing::error!("Export job {} failed: {}", job_id, e);

                // Mark job as failed
                job.status = ExportStatus::Failed;
                job.error = Some(ExportError {
                    code: ExportErrorCode::InternalError,
                    message: e.to_string(),
                    details: None,
                    occurred_at: Utc::now(),
                });
                job.updated_at = Utc::now();
                self.job_store.save(&job)?;

                Err(e)
            }
        }
    }

    /// Execute the complete export workflow
    async fn execute_workflow(&self, job: &mut ExportJob) -> Result<()> {
        // Phase 1: Discovery
        job.update_progress(
            ExportPhase::Discovery,
            10,
            Some("Discovering data...".to_string()),
        );
        self.job_store.save(job)?;

        let discovery_result = self.discovery.discover_user_data(&job.request).await?;

        tracing::info!(
            "Discovered {} items for user {}",
            discovery_result.total_items,
            job.user_id
        );

        // Phase 2: Collection (placeholder - actual data collection not implemented yet)
        job.update_progress(
            ExportPhase::Collection,
            30,
            Some(format!(
                "Collecting {} items...",
                discovery_result.total_items
            )),
        );
        self.job_store.save(job)?;

        // For now, we skip actual data retrieval and just use the discovery metadata

        // Phase 3: Conversion
        job.update_progress(
            ExportPhase::Conversion,
            60,
            Some("Converting to export format...".to_string()),
        );
        self.job_store.save(job)?;

        let export_data = self
            .converter
            .convert(&discovery_result, &job.request)
            .context("Failed to convert data to export format")?;

        tracing::info!(
            "Converted data to {:?} format, size: {} bytes",
            job.request.format,
            export_data.len()
        );

        // Phase 4: Storage
        job.update_progress(
            ExportPhase::Storage,
            80,
            Some("Saving export file...".to_string()),
        );
        self.job_store.save(job)?;

        let file_path = self
            .save_export_file(job.id, &job.request.format, &export_data)
            .await?;

        tracing::info!("Saved export file to: {}", file_path.display());

        // Phase 5: Finalization
        job.update_progress(
            ExportPhase::Finalization,
            95,
            Some("Finalizing export...".to_string()),
        );
        self.job_store.save(job)?;

        let download_url = self.generate_download_url(job.id, &job.request.format);

        // Calculate checksum
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(&export_data);
        let checksum = format!("{:x}", hasher.finalize());

        // Mark job as complete
        let result = ExportResult {
            file_path: file_path.to_string_lossy().to_string(),
            file_size_bytes: export_data.len(),
            download_url: download_url.clone(),
            checksum,
            metadata: super::types::ExportMetadata {
                format: job.request.format,
                record_count: discovery_result.total_items,
                categories: discovery_result.items_by_category.keys().cloned().collect(),
                sources: vec![], // TODO: populate from discovery
                time_range: discovery_result
                    .time_range
                    .map(|(start, end)| super::types::TimeRange { start, end }),
                generated_at: Utc::now(),
            },
        };

        job.complete(result, self.default_expiry_hours);
        self.job_store.save(job)?;

        tracing::info!(
            "Export job {} finalized, download URL: {}",
            job.id,
            download_url
        );

        Ok(())
    }

    /// Save export file to disk
    async fn save_export_file(
        &self,
        job_id: Uuid,
        format: &super::types::ExportFormat,
        data: &[u8],
    ) -> Result<PathBuf> {
        // Ensure export directory exists
        fs::create_dir_all(&self.export_dir)
            .await
            .context("Failed to create export directory")?;

        // Generate file path
        let filename = format!("{}.{}", job_id, format.extension());
        let file_path = self.export_dir.join(filename);

        // Write file
        fs::write(&file_path, data)
            .await
            .context("Failed to write export file")?;

        Ok(file_path)
    }

    /// Generate download URL for export file
    fn generate_download_url(&self, job_id: Uuid, format: &super::types::ExportFormat) -> String {
        // TODO: In production, this should generate signed URLs with expiry
        // For now, just return a simple path-based URL
        format!("/api/v1/gdpr/exports/{}/download", job_id)
    }

    /// Cleanup expired export files
    ///
    /// Removes files for expired jobs from disk.
    pub async fn cleanup_expired_files(&self) -> Result<usize> {
        let now = Utc::now();
        let expired_jobs = self.job_store.find_expired_jobs(now)?;

        let mut count = 0;

        for job in expired_jobs {
            if let Some(ref result) = job.result {
                let file_path = PathBuf::from(&result.file_path);

                if file_path.exists() {
                    match fs::remove_file(&file_path).await {
                        Ok(()) => {
                            tracing::info!("Deleted expired export file: {}", file_path.display());
                            count += 1;
                        }
                        Err(e) => {
                            tracing::warn!(
                                "Failed to delete expired export file {}: {}",
                                file_path.display(),
                                e
                            );
                        }
                    }
                }
            }
        }

        // Also update job statuses
        let _ = self.job_store.cleanup_expired(Some(7)); // Delete jobs expired for 7+ days

        Ok(count)
    }

    /// Get job by ID
    pub fn get_job(&self, job_id: Uuid) -> Result<Option<ExportJob>> {
        self.job_store.get(job_id)
    }

    /// List jobs for a user
    pub fn list_user_jobs(&self, user_id: &str, limit: Option<usize>) -> Result<Vec<ExportJob>> {
        self.job_store.list_by_user(user_id, limit)
    }

    /// Cancel a job
    pub fn cancel_job(&self, job_id: Uuid) -> Result<()> {
        let mut job = self
            .job_store
            .get(job_id)?
            .ok_or_else(|| anyhow::anyhow!("Export job not found: {}", job_id))?;

        // Can only cancel pending or processing jobs
        if !matches!(job.status, ExportStatus::Pending | ExportStatus::Processing) {
            anyhow::bail!("Cannot cancel job in status: {:?}", job.status);
        }

        job.status = ExportStatus::Cancelled;
        job.updated_at = Utc::now();
        self.job_store.save(&job)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gdpr::export::types::{ExportFormat, ExportRequest};
    use crate::storage::column_lineage_store::ColumnLineageStore;
    use crate::storage::kv_store::KvStore;
    use crate::storage::row_lineage_store::RowLineageStore;
    use graphica_core::core::lineage::{DataRef, LineageEvent};
    use std::collections::HashMap;
    use tempfile::TempDir;

    /// Mock lineage sink for testing
    struct MockLineageSink {
        events: Vec<LineageEvent>,
    }

    impl graphica_core::core::lineage::LineageSink for MockLineageSink {
        fn write(&self, _event: LineageEvent) -> Result<()> {
            Ok(())
        }

        fn get_record_lineage(&self, record_id: &str) -> Result<Vec<LineageEvent>> {
            Ok(self
                .events
                .iter()
                .filter(|e| e.record_id == record_id || e.tenant_id == record_id)
                .cloned()
                .collect())
        }

        fn get_model_impact(&self, _model_id: &str, _version: &str) -> Result<Vec<LineageEvent>> {
            Ok(vec![])
        }

        fn query_by_time_range(
            &self,
            start: chrono::DateTime<Utc>,
            end: chrono::DateTime<Utc>,
        ) -> Result<Vec<LineageEvent>> {
            Ok(self
                .events
                .iter()
                .filter(|e| e.ts >= start && e.ts <= end)
                .cloned()
                .collect())
        }

        fn get_run_lineage(&self, _run_id: &str) -> Result<Vec<LineageEvent>> {
            Ok(vec![])
        }

        fn get_lineage_as_of(
            &self,
            _record_id: &str,
            _as_of: chrono::DateTime<Utc>,
        ) -> Result<Vec<LineageEvent>> {
            Ok(vec![])
        }
    }

    fn create_test_event(user_id: &str) -> LineageEvent {
        LineageEvent {
            id: Uuid::new_v4(),
            dataset: "test.dataset".to_string(),
            record_id: format!("record_{}", user_id),
            source_refs: vec![],
            transforms: vec![],
            model_refs: vec![],
            output_ref: DataRef {
                system: "test".to_string(),
                path: "/test/path".to_string(),
                version: None,
                extracted_at: Utc::now(),
                cdc_position: None,
            },
            ts: Utc::now(),
            run_id: Uuid::new_v4().to_string(),
            tenant_id: user_id.to_string(),
            correlation_id: None,
            metadata: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn test_execute_job_workflow() {
        let temp_dir = TempDir::new().unwrap();
        let export_dir = temp_dir.path().join("exports");
        let db_dir = temp_dir.path().join("db");
        let row_lineage_dir = temp_dir.path().join("row_lineage");
        let col_lineage_dir = temp_dir.path().join("col_lineage");

        // Setup
        let kv = Arc::new(KvStore::new(&db_dir).unwrap());
        let job_store = Arc::new(ExportJobStore::new(kv));

        let mock_storage = Arc::new(MockLineageSink {
            events: vec![create_test_event("alice")],
        }) as Arc<dyn graphica_core::core::lineage::LineageSink>;

        let row_lineage = Arc::new(RowLineageStore::new(&row_lineage_dir).unwrap());
        let col_lineage = Arc::new(ColumnLineageStore::new(&col_lineage_dir).unwrap());

        let discovery = Arc::new(DataDiscoveryService::new(
            mock_storage,
            row_lineage,
            col_lineage,
            None, // No governance brain for tests
            None, // No file library for tests
        ));
        let converter = Arc::new(FormatConverter::new());

        let executor =
            ExportExecutor::new(job_store.clone(), discovery, converter, export_dir.clone());

        // Create a test job
        let request = ExportRequest {
            user_id: "alice".to_string(),
            format: ExportFormat::Json,
            categories: vec![],
            include_derived: false,
            include_metadata: true,
            include_audit_trail: false,
            time_range: None,
            filters: HashMap::new(),
        };

        let mut job = ExportJob::new("alice".to_string(), "admin@test.com".to_string(), request);
        let job_id = job.id;
        job_store.save(&job).unwrap();

        // Execute job
        executor.execute_job(job_id).await.unwrap();

        // Verify job completed
        let completed_job = job_store.get(job_id).unwrap().unwrap();
        assert_eq!(completed_job.status, ExportStatus::Ready);
        assert!(completed_job.result.is_some());

        let result = completed_job.result.unwrap();
        assert!(result.metadata.record_count > 0);
        assert!(result.file_size_bytes > 0);

        // Verify file was created
        let file_path = PathBuf::from(&result.file_path);
        assert!(file_path.exists());
    }

    #[tokio::test]
    async fn test_cancel_job() {
        let temp_dir = TempDir::new().unwrap();
        let export_dir = temp_dir.path().join("exports");
        let db_dir = temp_dir.path().join("db");
        let row_lineage_dir = temp_dir.path().join("row_lineage");
        let col_lineage_dir = temp_dir.path().join("col_lineage");

        let kv = Arc::new(KvStore::new(&db_dir).unwrap());
        let job_store = Arc::new(ExportJobStore::new(kv));

        let mock_storage = Arc::new(MockLineageSink { events: vec![] })
            as Arc<dyn graphica_core::core::lineage::LineageSink>;
        let row_lineage = Arc::new(RowLineageStore::new(&row_lineage_dir).unwrap());
        let col_lineage = Arc::new(ColumnLineageStore::new(&col_lineage_dir).unwrap());

        let discovery = Arc::new(DataDiscoveryService::new(
            mock_storage,
            row_lineage,
            col_lineage,
            None,
            None,
        ));
        let converter = Arc::new(FormatConverter::new());

        let executor = ExportExecutor::new(job_store.clone(), discovery, converter, export_dir);

        // Create a pending job
        let request = ExportRequest {
            user_id: "bob".to_string(),
            format: ExportFormat::Json,
            categories: vec![],
            include_derived: false,
            include_metadata: true,
            include_audit_trail: false,
            time_range: None,
            filters: HashMap::new(),
        };

        let job = ExportJob::new("bob".to_string(), "admin@test.com".to_string(), request);
        let job_id = job.id;
        job_store.save(&job).unwrap();

        // Cancel job
        executor.cancel_job(job_id).unwrap();

        // Verify cancelled
        let cancelled_job = job_store.get(job_id).unwrap().unwrap();
        assert_eq!(cancelled_job.status, ExportStatus::Cancelled);
    }
}
