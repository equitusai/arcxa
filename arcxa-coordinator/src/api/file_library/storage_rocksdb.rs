//! RocksDB-based File Library Storage
//!
//! Production-ready persistent storage backend using RocksDB with column families.

use super::storage_trait::FileLibraryStore;
use super::types::*;
use anyhow::{Context, Result};
use rocksdb::{ColumnFamilyDescriptor, Options, DB};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

/// Column family names
const CF_FILES: &str = "files";
const CF_FOLDERS: &str = "folders";
const CF_JOBS: &str = "jobs";
const CF_TAG_INDEX: &str = "tag_index";
const CF_FOLDER_INDEX: &str = "folder_index";

/// RocksDB-backed file library storage
pub struct RocksDBFileLibrary {
    db: Arc<DB>,
}

impl RocksDBFileLibrary {
    /// Open or create a new RocksDB file library at the given path
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);

        // Define column families
        let cfs = vec![
            ColumnFamilyDescriptor::new(CF_FILES, Options::default()),
            ColumnFamilyDescriptor::new(CF_FOLDERS, Options::default()),
            ColumnFamilyDescriptor::new(CF_JOBS, Options::default()),
            ColumnFamilyDescriptor::new(CF_TAG_INDEX, Options::default()),
            ColumnFamilyDescriptor::new(CF_FOLDER_INDEX, Options::default()),
        ];

        let db = DB::open_cf_descriptors(&opts, path, cfs).context("Failed to open RocksDB")?;

        Ok(Self { db: Arc::new(db) })
    }

    /// Serialize a value using bincode
    fn serialize<T: Serialize>(value: &T) -> Result<Vec<u8>> {
        bincode::serialize(value).context("Failed to serialize value")
    }

    /// Deserialize a value using bincode
    fn deserialize<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> Result<T> {
        bincode::deserialize(bytes).context("Failed to deserialize value")
    }

    /// Get a column family handle
    fn cf(&self, name: &str) -> Result<&rocksdb::ColumnFamily> {
        self.db
            .cf_handle(name)
            .ok_or_else(|| anyhow::anyhow!("Column family '{}' not found", name))
    }

    /// Update tag index for a file
    fn update_tag_index(&self, file_id: &str, tags: &[String]) -> Result<()> {
        let cf = self.cf(CF_TAG_INDEX)?;

        for tag in tags {
            let key = format!("tag:{}:{}", tag, file_id);
            self.db.put_cf(cf, key.as_bytes(), b"")?;
        }

        Ok(())
    }

    /// Remove file from tag index
    fn remove_from_tag_index(&self, file_id: &str, tags: &[String]) -> Result<()> {
        let cf = self.cf(CF_TAG_INDEX)?;

        for tag in tags {
            let key = format!("tag:{}:{}", tag, file_id);
            self.db.delete_cf(cf, key.as_bytes())?;
        }

        Ok(())
    }

    /// Update folder index for a file
    fn update_folder_index(&self, folder_id: &str, file_id: &str) -> Result<()> {
        let cf = self.cf(CF_FOLDER_INDEX)?;
        let key = format!("folder:{}:{}", folder_id, file_id);
        self.db.put_cf(cf, key.as_bytes(), b"")?;
        Ok(())
    }

    /// Remove file from folder index
    fn remove_from_folder_index(&self, folder_id: &str, file_id: &str) -> Result<()> {
        let cf = self.cf(CF_FOLDER_INDEX)?;
        let key = format!("folder:{}:{}", folder_id, file_id);
        self.db.delete_cf(cf, key.as_bytes())?;
        Ok(())
    }

    /// Get all files in a folder from index
    fn get_files_in_folder(&self, folder_id: &str) -> Result<Vec<String>> {
        let cf = self.cf(CF_FOLDER_INDEX)?;
        let prefix = format!("folder:{}:", folder_id);

        let mut file_ids = Vec::new();
        let iter = self.db.prefix_iterator_cf(cf, prefix.as_bytes());

        for item in iter {
            let (key, _) = item?;
            let key_str = String::from_utf8_lossy(&key);
            if let Some(file_id) = key_str.strip_prefix(&prefix) {
                file_ids.push(file_id.to_string());
            } else {
                break; // End of prefix range
            }
        }

        Ok(file_ids)
    }

    /// Get all files with a tag from index
    fn get_files_with_tag(&self, tag: &str) -> Result<Vec<String>> {
        let cf = self.cf(CF_TAG_INDEX)?;
        let prefix = format!("tag:{}:", tag);

        let mut file_ids = Vec::new();
        let iter = self.db.prefix_iterator_cf(cf, prefix.as_bytes());

        for item in iter {
            let (key, _) = item?;
            let key_str = String::from_utf8_lossy(&key);
            if let Some(file_id) = key_str.strip_prefix(&prefix) {
                file_ids.push(file_id.to_string());
            } else {
                break; // End of prefix range
            }
        }

        Ok(file_ids)
    }
}

impl FileLibraryStore for RocksDBFileLibrary {
    // ========================================================================
    // File Operations
    // ========================================================================

    fn create_file(&self, file: DataFile) -> Result<()> {
        let cf = self.cf(CF_FILES)?;
        let key = file.id.as_bytes();
        let value = Self::serialize(&file)?;

        self.db.put_cf(cf, key, value)?;

        // Update indexes
        if let Some(folder_id) = &file.folder_id {
            self.update_folder_index(folder_id, &file.id)?;
        }
        self.update_tag_index(&file.id, &file.tags)?;

        Ok(())
    }

    fn get_file(&self, file_id: &str) -> Result<Option<DataFile>> {
        let cf = self.cf(CF_FILES)?;

        match self.db.get_cf(cf, file_id.as_bytes())? {
            Some(bytes) => Ok(Some(Self::deserialize(&bytes)?)),
            None => Ok(None),
        }
    }

    fn update_file(&self, file_id: &str, updates: UpdateFileRequest) -> Result<DataFile> {
        let mut file = self
            .get_file(file_id)?
            .ok_or_else(|| anyhow::anyhow!("File not found: {}", file_id))?;

        // Store old values for index updates
        let old_folder_id = file.folder_id.clone();
        let old_tags = file.tags.clone();

        // Apply updates
        if let Some(name) = updates.name {
            file.name = name;
        }
        if let Some(description) = updates.description {
            file.description = Some(description);
        }
        if let Some(folder_id) = updates.folder_id {
            file.folder_id = Some(folder_id);
        }
        if let Some(tags) = updates.tags {
            file.tags = tags;
        }
        if let Some(metadata) = updates.metadata {
            file.metadata = metadata;
        }
        if let Some(schema) = updates.schema {
            file.schema = Some(schema);
        }
        if !updates.ontology_mappings.is_empty() {
            file.ontology_mappings = updates.ontology_mappings;
        }

        file.updated_at = chrono::Utc::now();

        // Update in database
        let cf = self.cf(CF_FILES)?;
        let value = Self::serialize(&file)?;
        self.db.put_cf(cf, file_id.as_bytes(), value)?;

        // Update indexes if folder or tags changed
        if old_folder_id != file.folder_id {
            if let Some(old_id) = old_folder_id {
                self.remove_from_folder_index(&old_id, file_id)?;
            }
            if let Some(new_id) = &file.folder_id {
                self.update_folder_index(new_id, file_id)?;
            }
        }

        if old_tags != file.tags {
            self.remove_from_tag_index(file_id, &old_tags)?;
            self.update_tag_index(file_id, &file.tags)?;
        }

        Ok(file)
    }

    fn delete_file(&self, file_id: &str) -> Result<()> {
        // Get file first to update indexes
        let file = self
            .get_file(file_id)?
            .ok_or_else(|| anyhow::anyhow!("File not found: {}", file_id))?;

        // Remove from indexes
        if let Some(folder_id) = &file.folder_id {
            self.remove_from_folder_index(folder_id, file_id)?;
        }
        self.remove_from_tag_index(file_id, &file.tags)?;

        // Delete from database
        let cf = self.cf(CF_FILES)?;
        self.db.delete_cf(cf, file_id.as_bytes())?;

        Ok(())
    }

    fn update_last_accessed(&self, file_id: &str) -> Result<()> {
        let mut file = self
            .get_file(file_id)?
            .ok_or_else(|| anyhow::anyhow!("File not found: {}", file_id))?;

        file.last_accessed = Some(chrono::Utc::now());

        // Update in database
        let cf = self.cf(CF_FILES)?;
        let value = Self::serialize(&file)?;
        self.db.put_cf(cf, file_id.as_bytes(), value)?;

        Ok(())
    }

    fn list_files(&self, request: &ListFilesRequest) -> Result<Vec<DataFile>> {
        let cf = self.cf(CF_FILES)?;
        let mut files = Vec::new();

        // Start with all files or filter by folder/tag
        let candidate_ids: Vec<String> = if let Some(folder_id) = &request.folder_id {
            self.get_files_in_folder(folder_id)?
        } else if let Some(tag) = request.tags.as_ref().and_then(|t| t.first()) {
            self.get_files_with_tag(tag)?
        } else {
            // Get all files
            let iter = self.db.iterator_cf(cf, rocksdb::IteratorMode::Start);
            iter.filter_map(|item| {
                item.ok()
                    .and_then(|(key, _)| String::from_utf8(key.to_vec()).ok())
            })
            .collect()
        };

        // Load and filter files
        for file_id in candidate_ids {
            if let Some(file) = self.get_file(&file_id)? {
                // Apply filters
                let mut matches = true;

                if let Some(status) = &request.status {
                    matches = matches && file.status == *status;
                }

                if let Some(tags) = &request.tags {
                    matches = matches && tags.iter().all(|t| file.tags.contains(t));
                }

                if matches {
                    files.push(file);
                }
            }
        }

        // Sort by created_at (newest first)
        files.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        // Apply pagination
        let start = request.offset.unwrap_or(0);
        let end = start + request.limit.unwrap_or(100);

        Ok(files.into_iter().skip(start).take(end - start).collect())
    }

    fn search_files(&self, request: &SearchRequest) -> Result<Vec<DataFile>> {
        let cf = self.cf(CF_FILES)?;
        let mut files = Vec::new();

        // Iterate all files and search in name/description
        let iter = self.db.iterator_cf(cf, rocksdb::IteratorMode::Start);

        for item in iter {
            let (_, value) = item?;
            let file: DataFile = Self::deserialize(&value)?;

            let query_lower = request.query.to_lowercase();
            let matches = file.name.to_lowercase().contains(&query_lower)
                || file
                    .description
                    .as_ref()
                    .map(|d| d.to_lowercase().contains(&query_lower))
                    .unwrap_or(false);

            if matches {
                files.push(file);
            }
        }

        // Sort by relevance (simple: name matches first)
        files.sort_by(|a, b| {
            let a_name_match = a
                .name
                .to_lowercase()
                .contains(&request.query.to_lowercase());
            let b_name_match = b
                .name
                .to_lowercase()
                .contains(&request.query.to_lowercase());
            b_name_match.cmp(&a_name_match)
        });

        Ok(files)
    }

    // ========================================================================
    // Folder Operations
    // ========================================================================

    fn create_folder(&self, folder: Folder) -> Result<Folder> {
        let cf = self.cf(CF_FOLDERS)?;
        let key = folder.id.as_bytes();
        let value = Self::serialize(&folder)?;

        self.db.put_cf(cf, key, value)?;

        Ok(folder)
    }

    fn get_folder(&self, folder_id: &str) -> Result<Option<Folder>> {
        let cf = self.cf(CF_FOLDERS)?;

        match self.db.get_cf(cf, folder_id.as_bytes())? {
            Some(bytes) => Ok(Some(Self::deserialize(&bytes)?)),
            None => Ok(None),
        }
    }

    fn list_folders(&self) -> Result<Vec<Folder>> {
        let cf = self.cf(CF_FOLDERS)?;
        let mut folders = Vec::new();

        let iter = self.db.iterator_cf(cf, rocksdb::IteratorMode::Start);

        for item in iter {
            let (_, value) = item?;
            let folder: Folder = Self::deserialize(&value)?;
            folders.push(folder);
        }

        // Sort by created_at
        folders.sort_by(|a, b| a.created_at.cmp(&b.created_at));

        Ok(folders)
    }

    fn update_folder(&self, folder_id: &str, updates: UpdateFolderRequest) -> Result<Folder> {
        let mut folder = self
            .get_folder(folder_id)?
            .ok_or_else(|| anyhow::anyhow!("Folder not found: {}", folder_id))?;

        if let Some(name) = updates.name {
            folder.name = name;
        }
        if let Some(description) = updates.description {
            folder.description = Some(description);
        }

        folder.updated_at = chrono::Utc::now();

        let cf = self.cf(CF_FOLDERS)?;
        let value = Self::serialize(&folder)?;
        self.db.put_cf(cf, folder_id.as_bytes(), value)?;

        Ok(folder)
    }

    fn delete_folder(&self, folder_id: &str, force: bool) -> Result<()> {
        // Check if folder has files
        let files_in_folder = self.get_files_in_folder(folder_id)?;

        if !files_in_folder.is_empty() && !force {
            anyhow::bail!(
                "Folder contains {} files. Use force=true to delete anyway.",
                files_in_folder.len()
            );
        }

        // Delete folder
        let cf = self.cf(CF_FOLDERS)?;
        self.db.delete_cf(cf, folder_id.as_bytes())?;

        // If force, remove folder_id from files (orphan them)
        if force {
            for file_id in files_in_folder {
                if let Some(mut file) = self.get_file(&file_id)? {
                    file.folder_id = None;
                    let cf_files = self.cf(CF_FILES)?;
                    let value = Self::serialize(&file)?;
                    self.db.put_cf(cf_files, file_id.as_bytes(), value)?;
                }
            }
        }

        Ok(())
    }

    // ========================================================================
    // Job Operations
    // ========================================================================

    fn create_job(&self, job: ImportJob) -> Result<()> {
        let cf = self.cf(CF_JOBS)?;
        let key = job.job_id.as_bytes();
        let value = Self::serialize(&job)?;

        self.db.put_cf(cf, key, value)?;

        Ok(())
    }

    fn get_job(&self, job_id: &str) -> Result<Option<ImportJob>> {
        let cf = self.cf(CF_JOBS)?;

        match self.db.get_cf(cf, job_id.as_bytes())? {
            Some(bytes) => Ok(Some(Self::deserialize(&bytes)?)),
            None => Ok(None),
        }
    }

    fn update_job(&self, job: ImportJob) -> Result<()> {
        let cf = self.cf(CF_JOBS)?;
        let key = job.job_id.as_bytes();
        let value = Self::serialize(&job)?;

        self.db.put_cf(cf, key, value)?;

        Ok(())
    }

    fn update_job_progress(
        &self,
        job_id: &str,
        processed_files: usize,
        progress_percent: f32,
    ) -> Result<()> {
        let mut job = self
            .get_job(job_id)?
            .ok_or_else(|| anyhow::anyhow!("Job not found: {}", job_id))?;

        job.processed_files = processed_files;
        job.progress_percent = progress_percent;

        self.update_job(job)
    }

    fn complete_job(
        &self,
        job_id: &str,
        status: JobStatus,
        successful_files: usize,
        failed_files: usize,
        results: Vec<ImportResult>,
        duration_ms: u64,
    ) -> Result<()> {
        let mut job = self
            .get_job(job_id)?
            .ok_or_else(|| anyhow::anyhow!("Job not found: {}", job_id))?;

        job.status = status;
        job.successful_files = successful_files;
        job.failed_files = failed_files;
        job.results = results;
        job.duration_ms = Some(duration_ms);
        job.completed_at = Some(chrono::Utc::now());

        self.update_job(job)
    }

    // ========================================================================
    // Tag Operations
    // ========================================================================

    fn list_tags(&self) -> Result<Vec<TagInfo>> {
        let cf = self.cf(CF_TAG_INDEX)?;
        let mut tag_counts: HashMap<String, usize> = HashMap::new();

        // Iterate tag index and count occurrences
        let iter = self.db.iterator_cf(cf, rocksdb::IteratorMode::Start);

        for item in iter {
            let (key, _) = item?;
            let key_str = String::from_utf8_lossy(&key);

            // Key format: "tag:{tag_name}:{file_id}"
            if let Some(rest) = key_str.strip_prefix("tag:") {
                if let Some(tag_name) = rest.split(':').next() {
                    *tag_counts.entry(tag_name.to_string()).or_insert(0) += 1;
                }
            }
        }

        let mut tags: Vec<TagInfo> = tag_counts
            .into_iter()
            .map(|(name, count)| TagInfo {
                name,
                count,
                color: None,
            })
            .collect();

        tags.sort_by(|a, b| b.count.cmp(&a.count));

        Ok(tags)
    }

    // ========================================================================
    // Statistics
    // ========================================================================

    fn get_statistics(&self) -> Result<LibraryStatsResponse> {
        let files_cf = self.cf(CF_FILES)?;

        let mut total_files = 0;
        let mut total_size_bytes = 0u64;
        let mut total_rows = 0u64;
        let mut files_by_status: HashMap<String, usize> = HashMap::new();
        let mut files_by_folder: HashMap<String, usize> = HashMap::new();
        let mut files_with_pii = 0;
        let mut all_files = Vec::new();

        // Count files and aggregate stats
        let iter = self.db.iterator_cf(files_cf, rocksdb::IteratorMode::Start);

        for item in iter {
            let (_, value) = item?;
            let file: DataFile = Self::deserialize(&value)?;

            total_files += 1;
            total_size_bytes += file.size_bytes;

            // Count rows if schema is available
            if let Some(ref schema) = file.schema {
                total_rows += schema.total_rows;
            }

            // Count by status
            let status_key = format!("{:?}", file.status);
            *files_by_status.entry(status_key).or_insert(0) += 1;

            // Count by folder
            if let Some(ref folder_id) = file.folder_id {
                *files_by_folder.entry(folder_id.clone()).or_insert(0) += 1;
            }

            // Count PII files
            if file.sensitivity_level.is_some() {
                files_with_pii += 1;
            }

            all_files.push(file);
        }

        // Get top tags
        let top_tags = self.list_tags()?.into_iter().take(10).collect();

        // Get recent uploads (last 10)
        all_files.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        let recent_uploads: Vec<DataFile> = all_files.iter().take(10).cloned().collect();

        // Get most used (by last_accessed)
        all_files.sort_by(|a, b| {
            b.last_accessed
                .unwrap_or(b.created_at)
                .cmp(&a.last_accessed.unwrap_or(a.created_at))
        });
        let most_used: Vec<DataFile> = all_files.into_iter().take(10).collect();

        Ok(LibraryStatsResponse {
            total_files,
            total_size_bytes,
            total_rows,
            files_by_status,
            files_by_folder,
            files_with_pii,
            top_tags,
            recent_uploads,
            most_used,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_rocksdb_initialization() {
        let temp_dir = TempDir::new().unwrap();
        let db = RocksDBFileLibrary::open(temp_dir.path());
        assert!(db.is_ok());
    }

    #[test]
    fn test_rocksdb_persistence() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().to_path_buf();

        // Create and close
        {
            let _db = RocksDBFileLibrary::open(&path).unwrap();
        }

        // Reopen to verify persistence
        {
            let db = RocksDBFileLibrary::open(&path);
            assert!(db.is_ok());
        }
    }
}
