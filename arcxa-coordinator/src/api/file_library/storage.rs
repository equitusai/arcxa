//! File Library Storage Layer
//!
//! In-memory storage for MVP. Can be upgraded to persistent storage (PostgreSQL/RocksDB) later.

use super::storage_trait::FileLibraryStore;
use super::types::*;
use anyhow::{Context, Result};
use chrono::Utc;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// File Library storage
#[derive(Clone)]
pub struct FileLibraryStorage {
    files: Arc<RwLock<HashMap<String, DataFile>>>,
    folders: Arc<RwLock<HashMap<String, Folder>>>,
    jobs: Arc<RwLock<HashMap<String, ImportJob>>>,
    tag_index: Arc<RwLock<HashMap<String, Vec<String>>>>, // tag -> [file_ids]
    folder_index: Arc<RwLock<HashMap<String, Vec<String>>>>, // folder_id -> [file_ids]
}

impl FileLibraryStorage {
    pub fn new() -> Self {
        let storage = Self {
            files: Arc::new(RwLock::new(HashMap::new())),
            folders: Arc::new(RwLock::new(HashMap::new())),
            jobs: Arc::new(RwLock::new(HashMap::new())),
            tag_index: Arc::new(RwLock::new(HashMap::new())),
            folder_index: Arc::new(RwLock::new(HashMap::new())),
        };

        // Initialize with root folder
        let root_folder = Folder {
            id: "root".to_string(),
            name: "Root".to_string(),
            parent_id: None,
            description: Some("Root folder".to_string()),
            path: "/".to_string(),
            file_count: 0,
            subfolder_count: 0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            children: Some(Vec::new()),
        };

        storage
            .folders
            .write()
            .unwrap()
            .insert("root".to_string(), root_folder);
        storage
    }

    // ========================================================================
    // File Operations
    // ========================================================================

    pub fn create_file(&self, file: DataFile) -> Result<()> {
        let file_id = file.id.clone();

        // Add to main storage
        self.files
            .write()
            .unwrap()
            .insert(file_id.clone(), file.clone());

        // Update indexes
        self.update_indexes_for_file(&file)?;

        Ok(())
    }

    pub fn get_file(&self, file_id: &str) -> Result<Option<DataFile>> {
        Ok(self.files.read().unwrap().get(file_id).cloned())
    }

    pub fn update_file(&self, file_id: &str, updates: UpdateFileRequest) -> Result<DataFile> {
        let mut files = self.files.write().unwrap();
        let file = files.get_mut(file_id).context("File not found")?;

        // Apply updates
        if let Some(name) = updates.name {
            file.name = name;
        }
        if let Some(description) = updates.description {
            file.description = Some(description);
        }
        if let Some(tags) = updates.tags {
            // Remove old tag indexes
            self.remove_from_tag_index(file_id, &file.tags)?;
            file.tags = tags.clone();
            // Add new tag indexes
            self.add_to_tag_index(file_id, &tags)?;
        }
        if let Some(folder_id) = updates.folder_id {
            // Update folder index
            if let Some(old_folder) = &file.folder_id {
                self.remove_from_folder_index(old_folder, file_id)?;
            }
            file.folder_id = Some(folder_id.clone());
            self.add_to_folder_index(&folder_id, file_id)?;
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

        file.updated_at = Utc::now();

        Ok(file.clone())
    }

    pub fn delete_file(&self, file_id: &str) -> Result<()> {
        let mut files = self.files.write().unwrap();
        if let Some(file) = files.remove(file_id) {
            // Remove from indexes
            self.remove_from_tag_index(file_id, &file.tags)?;
            if let Some(folder_id) = &file.folder_id {
                self.remove_from_folder_index(folder_id, file_id)?;
            }
        }
        Ok(())
    }

    pub fn update_last_accessed(&self, file_id: &str) -> Result<()> {
        let mut files = self.files.write().unwrap();
        if let Some(file) = files.get_mut(file_id) {
            file.last_accessed = Some(Utc::now());
        }
        Ok(())
    }

    pub fn list_files(&self, request: &ListFilesRequest) -> Result<Vec<DataFile>> {
        let files = self.files.read().unwrap();
        let mut results: Vec<DataFile> = files.values().cloned().collect();

        // Apply filters
        if let Some(folder_id) = &request.folder_id {
            results.retain(|f| f.folder_id.as_ref() == Some(folder_id));
        }

        if let Some(tags) = &request.tags {
            results.retain(|f| tags.iter().all(|tag| f.tags.contains(tag)));
        }

        if let Some(search) = &request.search {
            let search_lower = search.to_lowercase();
            results.retain(|f| {
                f.name.to_lowercase().contains(&search_lower)
                    || f.description
                        .as_ref()
                        .map_or(false, |d| d.to_lowercase().contains(&search_lower))
            });
        }

        if let Some(status) = &request.status {
            results.retain(|f| &f.status == status);
        }

        if let Some(owner) = &request.owner {
            results.retain(|f| &f.owner.user_id == owner || &f.owner.email == owner);
        }

        // Sort results
        let sort_field = request.sort.as_ref().unwrap_or(&SortField::Modified);
        let sort_order = request.order.as_ref().unwrap_or(&SortOrder::Desc);

        results.sort_by(|a, b| {
            let cmp = match sort_field {
                SortField::Name => a.name.cmp(&b.name),
                SortField::Modified => a.updated_at.cmp(&b.updated_at),
                SortField::Size => a.size_bytes.cmp(&b.size_bytes),
                SortField::Created => a.created_at.cmp(&b.created_at),
            };

            match sort_order {
                SortOrder::Asc => cmp,
                SortOrder::Desc => cmp.reverse(),
            }
        });

        Ok(results)
    }

    pub fn search_files(&self, request: &SearchRequest) -> Result<Vec<DataFile>> {
        let files = self.files.read().unwrap();
        let query_lower = request.query.to_lowercase();
        let mut results: Vec<DataFile> = files
            .values()
            .filter(|f| {
                f.name.to_lowercase().contains(&query_lower)
                    || f.description
                        .as_ref()
                        .map_or(false, |d| d.to_lowercase().contains(&query_lower))
                    || f.tags
                        .iter()
                        .any(|t| t.to_lowercase().contains(&query_lower))
            })
            .cloned()
            .collect();

        // Apply filters
        if let Some(filters) = &request.filters {
            if let Some(folder_ids) = &filters.folder_ids {
                results.retain(|f| {
                    f.folder_id
                        .as_ref()
                        .map_or(false, |fid| folder_ids.contains(fid))
                });
            }

            if let Some(tags) = &filters.tags {
                results.retain(|f| tags.iter().all(|tag| f.tags.contains(tag)));
            }

            if let Some(statuses) = &filters.status {
                results.retain(|f| statuses.contains(&f.status));
            }

            if let Some(has_pii) = filters.has_pii {
                results.retain(|f| {
                    f.schema.as_ref().map_or(false, |s| {
                        s.fields.iter().any(|field| field.is_pii.unwrap_or(false)) == has_pii
                    })
                });
            }

            if let Some(min_rows) = filters.min_rows {
                results.retain(|f| {
                    f.schema
                        .as_ref()
                        .map_or(false, |s| s.total_rows >= min_rows)
                });
            }

            if let Some(max_rows) = filters.max_rows {
                results.retain(|f| {
                    f.schema
                        .as_ref()
                        .map_or(false, |s| s.total_rows <= max_rows)
                });
            }
        }

        Ok(results)
    }

    // ========================================================================
    // Folder Operations
    // ========================================================================

    pub fn create_folder(&self, folder: Folder) -> Result<Folder> {
        let folder_id = folder.id.clone();
        self.folders
            .write()
            .unwrap()
            .insert(folder_id, folder.clone());
        Ok(folder)
    }

    pub fn get_folder(&self, folder_id: &str) -> Result<Option<Folder>> {
        Ok(self.folders.read().unwrap().get(folder_id).cloned())
    }

    pub fn list_folders(&self) -> Result<Vec<Folder>> {
        Ok(self.folders.read().unwrap().values().cloned().collect())
    }

    pub fn update_folder(&self, folder_id: &str, updates: UpdateFolderRequest) -> Result<Folder> {
        let mut folders = self.folders.write().unwrap();
        let folder = folders.get_mut(folder_id).context("Folder not found")?;

        if let Some(name) = updates.name {
            folder.name = name;
        }
        if let Some(parent_id) = updates.parent_id {
            folder.parent_id = Some(parent_id);
        }
        if let Some(description) = updates.description {
            folder.description = Some(description);
        }

        folder.updated_at = Utc::now();

        Ok(folder.clone())
    }

    pub fn delete_folder(&self, folder_id: &str, force: bool) -> Result<()> {
        // Check if folder has files
        let file_count = self
            .folder_index
            .read()
            .unwrap()
            .get(folder_id)
            .map(|files| files.len())
            .unwrap_or(0);

        if file_count > 0 && !force {
            anyhow::bail!(
                "Folder contains {} files. Use force=true to delete.",
                file_count
            );
        }

        self.folders.write().unwrap().remove(folder_id);
        self.folder_index.write().unwrap().remove(folder_id);

        Ok(())
    }

    // ========================================================================
    // Job Operations
    // ========================================================================

    pub fn create_job(&self, job: ImportJob) -> Result<()> {
        self.jobs.write().unwrap().insert(job.job_id.clone(), job);
        Ok(())
    }

    pub fn get_job(&self, job_id: &str) -> Result<Option<ImportJob>> {
        Ok(self.jobs.read().unwrap().get(job_id).cloned())
    }

    pub fn update_job(&self, job: ImportJob) -> Result<()> {
        self.jobs.write().unwrap().insert(job.job_id.clone(), job);
        Ok(())
    }

    pub fn update_job_progress(
        &self,
        job_id: &str,
        processed_files: usize,
        progress_percent: f32,
    ) -> Result<()> {
        let mut jobs = self.jobs.write().unwrap();
        if let Some(job) = jobs.get_mut(job_id) {
            job.processed_files = processed_files;
            job.progress_percent = progress_percent;
        }
        Ok(())
    }

    pub fn complete_job(
        &self,
        job_id: &str,
        status: JobStatus,
        successful_files: usize,
        failed_files: usize,
        results: Vec<ImportResult>,
        duration_ms: u64,
    ) -> Result<()> {
        let mut jobs = self.jobs.write().unwrap();
        if let Some(job) = jobs.get_mut(job_id) {
            job.status = status;
            job.successful_files = successful_files;
            job.failed_files = failed_files;
            job.processed_files = successful_files + failed_files;
            job.progress_percent = 100.0;
            job.results = results;
            job.completed_at = Some(Utc::now());
            job.duration_ms = Some(duration_ms);
        }
        Ok(())
    }

    // ========================================================================
    // Tag Operations
    // ========================================================================

    pub fn list_tags(&self) -> Result<Vec<TagInfo>> {
        let tag_index = self.tag_index.read().unwrap();
        let mut tags: Vec<TagInfo> = tag_index
            .iter()
            .map(|(name, file_ids)| TagInfo {
                name: name.clone(),
                count: file_ids.len(),
                color: None,
            })
            .collect();

        tags.sort_by(|a, b| b.count.cmp(&a.count));
        Ok(tags)
    }

    // ========================================================================
    // Statistics
    // ========================================================================

    pub fn get_statistics(&self) -> Result<LibraryStatsResponse> {
        let files = self.files.read().unwrap();

        let total_files = files.len();
        let total_size_bytes: u64 = files.values().map(|f| f.size_bytes).sum();
        let total_rows: u64 = files
            .values()
            .filter_map(|f| f.schema.as_ref().map(|s| s.total_rows))
            .sum();

        let mut files_by_status = HashMap::new();
        for file in files.values() {
            *files_by_status
                .entry(format!("{:?}", file.status))
                .or_insert(0) += 1;
        }

        let mut files_by_folder = HashMap::new();
        for file in files.values() {
            let folder_id = file.folder_id.as_deref().unwrap_or("root");
            *files_by_folder.entry(folder_id.to_string()).or_insert(0) += 1;
        }

        let files_with_pii = files
            .values()
            .filter(|f| {
                f.schema.as_ref().map_or(false, |s| {
                    s.fields.iter().any(|field| field.is_pii.unwrap_or(false))
                })
            })
            .count();

        let top_tags = self.list_tags()?;

        // Get recent uploads
        let mut recent_files: Vec<DataFile> = files.values().cloned().collect();
        recent_files.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        let recent_uploads = recent_files.into_iter().take(10).collect();

        // Get most used files (sorted by last_accessed)
        let mut accessed_files: Vec<DataFile> = files
            .values()
            .filter(|f| f.last_accessed.is_some())
            .cloned()
            .collect();
        accessed_files.sort_by(|a, b| b.last_accessed.unwrap().cmp(&a.last_accessed.unwrap()));
        let most_used = accessed_files.into_iter().take(10).collect();

        Ok(LibraryStatsResponse {
            total_files,
            total_size_bytes,
            total_rows,
            files_by_status,
            files_by_folder,
            files_with_pii,
            top_tags: top_tags.into_iter().take(10).collect(),
            recent_uploads,
            most_used,
        })
    }

    // ========================================================================
    // Private Helper Methods
    // ========================================================================

    fn update_indexes_for_file(&self, file: &DataFile) -> Result<()> {
        // Update tag index
        for tag in &file.tags {
            self.add_to_tag_index(&file.id, &[tag.clone()])?;
        }

        // Update folder index
        if let Some(folder_id) = &file.folder_id {
            self.add_to_folder_index(folder_id, &file.id)?;
        }

        Ok(())
    }

    fn add_to_tag_index(&self, file_id: &str, tags: &[String]) -> Result<()> {
        let mut tag_index = self.tag_index.write().unwrap();
        for tag in tags {
            tag_index
                .entry(tag.clone())
                .or_insert_with(Vec::new)
                .push(file_id.to_string());
        }
        Ok(())
    }

    fn remove_from_tag_index(&self, file_id: &str, tags: &[String]) -> Result<()> {
        let mut tag_index = self.tag_index.write().unwrap();
        for tag in tags {
            if let Some(file_ids) = tag_index.get_mut(tag) {
                file_ids.retain(|id| id != file_id);
                if file_ids.is_empty() {
                    tag_index.remove(tag);
                }
            }
        }
        Ok(())
    }

    fn add_to_folder_index(&self, folder_id: &str, file_id: &str) -> Result<()> {
        self.folder_index
            .write()
            .unwrap()
            .entry(folder_id.to_string())
            .or_insert_with(Vec::new)
            .push(file_id.to_string());
        Ok(())
    }

    fn remove_from_folder_index(&self, folder_id: &str, file_id: &str) -> Result<()> {
        if let Some(file_ids) = self.folder_index.write().unwrap().get_mut(folder_id) {
            file_ids.retain(|id| id != file_id);
        }
        Ok(())
    }
}

impl Default for FileLibraryStorage {
    fn default() -> Self {
        Self::new()
    }
}

// Implement the FileLibraryStore trait
impl FileLibraryStore for FileLibraryStorage {
    fn create_file(&self, file: DataFile) -> Result<()> {
        self.create_file(file)
    }

    fn get_file(&self, file_id: &str) -> Result<Option<DataFile>> {
        self.get_file(file_id)
    }

    fn update_file(&self, file_id: &str, updates: UpdateFileRequest) -> Result<DataFile> {
        self.update_file(file_id, updates)
    }

    fn delete_file(&self, file_id: &str) -> Result<()> {
        self.delete_file(file_id)
    }

    fn update_last_accessed(&self, file_id: &str) -> Result<()> {
        self.update_last_accessed(file_id)
    }

    fn list_files(&self, request: &ListFilesRequest) -> Result<Vec<DataFile>> {
        self.list_files(request)
    }

    fn search_files(&self, request: &SearchRequest) -> Result<Vec<DataFile>> {
        self.search_files(request)
    }

    fn create_folder(&self, folder: Folder) -> Result<Folder> {
        self.create_folder(folder)
    }

    fn get_folder(&self, folder_id: &str) -> Result<Option<Folder>> {
        self.get_folder(folder_id)
    }

    fn list_folders(&self) -> Result<Vec<Folder>> {
        self.list_folders()
    }

    fn update_folder(&self, folder_id: &str, updates: UpdateFolderRequest) -> Result<Folder> {
        self.update_folder(folder_id, updates)
    }

    fn delete_folder(&self, folder_id: &str, force: bool) -> Result<()> {
        self.delete_folder(folder_id, force)
    }

    fn create_job(&self, job: ImportJob) -> Result<()> {
        self.create_job(job)
    }

    fn get_job(&self, job_id: &str) -> Result<Option<ImportJob>> {
        self.get_job(job_id)
    }

    fn update_job(&self, job: ImportJob) -> Result<()> {
        self.update_job(job)
    }

    fn update_job_progress(
        &self,
        job_id: &str,
        processed_files: usize,
        progress_percent: f32,
    ) -> Result<()> {
        self.update_job_progress(job_id, processed_files, progress_percent)
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
        self.complete_job(
            job_id,
            status,
            successful_files,
            failed_files,
            results,
            duration_ms,
        )
    }

    fn list_tags(&self) -> Result<Vec<TagInfo>> {
        self.list_tags()
    }

    fn get_statistics(&self) -> Result<LibraryStatsResponse> {
        self.get_statistics()
    }
}
