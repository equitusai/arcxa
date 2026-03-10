//! File Library Storage Trait
//!
//! Abstraction over storage backends (in-memory, RocksDB, etc.)

use super::types::*;
use anyhow::Result;

/// Storage backend trait for file library
///
/// This trait abstracts storage operations to enable multiple backends:
/// - In-memory (for testing, development)
/// - RocksDB (for production persistence)
/// - Future: PostgreSQL, etc.
pub trait FileLibraryStore: Send + Sync {
    // ========================================================================
    // File Operations
    // ========================================================================

    /// Create a new file entry
    fn create_file(&self, file: DataFile) -> Result<()>;

    /// Get a file by ID
    fn get_file(&self, file_id: &str) -> Result<Option<DataFile>>;

    /// Update a file
    fn update_file(&self, file_id: &str, updates: UpdateFileRequest) -> Result<DataFile>;

    /// Delete a file
    fn delete_file(&self, file_id: &str) -> Result<()>;

    /// Update last accessed timestamp for a file
    fn update_last_accessed(&self, file_id: &str) -> Result<()>;

    /// List files with filters and pagination
    fn list_files(&self, request: &ListFilesRequest) -> Result<Vec<DataFile>>;

    /// Search files by query
    fn search_files(&self, request: &SearchRequest) -> Result<Vec<DataFile>>;

    // ========================================================================
    // Folder Operations
    // ========================================================================

    /// Create a new folder
    fn create_folder(&self, folder: Folder) -> Result<Folder>;

    /// Get a folder by ID
    fn get_folder(&self, folder_id: &str) -> Result<Option<Folder>>;

    /// List all folders
    fn list_folders(&self) -> Result<Vec<Folder>>;

    /// Update a folder
    fn update_folder(&self, folder_id: &str, updates: UpdateFolderRequest) -> Result<Folder>;

    /// Delete a folder
    fn delete_folder(&self, folder_id: &str, force: bool) -> Result<()>;

    // ========================================================================
    // Job Operations
    // ========================================================================

    /// Create a new import job
    fn create_job(&self, job: ImportJob) -> Result<()>;

    /// Get a job by ID
    fn get_job(&self, job_id: &str) -> Result<Option<ImportJob>>;

    /// Update a job
    fn update_job(&self, job: ImportJob) -> Result<()>;

    /// Update job progress
    fn update_job_progress(
        &self,
        job_id: &str,
        processed_files: usize,
        progress_percent: f32,
    ) -> Result<()>;

    /// Complete a job
    fn complete_job(
        &self,
        job_id: &str,
        status: JobStatus,
        successful_files: usize,
        failed_files: usize,
        results: Vec<ImportResult>,
        duration_ms: u64,
    ) -> Result<()>;

    // ========================================================================
    // Tag Operations
    // ========================================================================

    /// List all tags with usage counts
    fn list_tags(&self) -> Result<Vec<TagInfo>>;

    // ========================================================================
    // Statistics
    // ========================================================================

    /// Get library statistics
    fn get_statistics(&self) -> Result<LibraryStatsResponse>;
}
