//! Migration Tool: In-Memory → RocksDB Storage
//!
//! Utility for migrating file library data from in-memory storage to persistent RocksDB backend.
//! Also includes utilities for recovering orphaned files from the filesystem.

use super::storage::FileLibraryStorage;
use super::storage_rocksdb::RocksDBFileLibrary;
use super::storage_trait::FileLibraryStore;
use super::types::*;
use anyhow::{Context, Result};
use chrono::Utc;
use std::path::Path;

/// Migration statistics
#[derive(Debug, Clone)]
pub struct MigrationStats {
    pub files_migrated: usize,
    pub folders_migrated: usize,
    pub jobs_migrated: usize,
    pub errors: Vec<String>,
    pub duration_ms: u64,
}

/// Migrate all data from in-memory storage to RocksDB
pub fn migrate_to_rocksdb<P: AsRef<Path>>(
    source: &FileLibraryStorage,
    target_path: P,
) -> Result<MigrationStats> {
    let start = std::time::Instant::now();
    let mut stats = MigrationStats {
        files_migrated: 0,
        folders_migrated: 0,
        jobs_migrated: 0,
        errors: Vec::new(),
        duration_ms: 0,
    };

    // Open or create RocksDB storage
    let target = RocksDBFileLibrary::open(target_path).context("Failed to open RocksDB storage")?;

    // Migrate folders first (files may reference them)
    let folders = source
        .list_folders()
        .context("Failed to list folders from source")?;

    for folder in folders {
        match target.create_folder(folder.clone()) {
            Ok(_) => stats.folders_migrated += 1,
            Err(e) => stats
                .errors
                .push(format!("Failed to migrate folder {}: {}", folder.id, e)),
        }
    }

    // Migrate files
    let list_request = ListFilesRequest {
        folder_id: None,
        tags: None,
        search: None,
        status: None,
        owner: None,
        sort: None,
        order: None,
        limit: None,
        offset: None,
    };

    let files = source
        .list_files(&list_request)
        .context("Failed to list files from source")?;

    for file in files {
        match target.create_file(file.clone()) {
            Ok(_) => stats.files_migrated += 1,
            Err(e) => stats
                .errors
                .push(format!("Failed to migrate file {}: {}", file.id, e)),
        }
    }

    // Migrate jobs (if we can list them all)
    // Note: Current trait doesn't have list_jobs, so we skip this for now
    // In a production system, you'd want to add this method

    stats.duration_ms = start.elapsed().as_millis() as u64;

    Ok(stats)
}

/// Verify migration by comparing counts
pub fn verify_migration<P: AsRef<Path>>(
    source: &FileLibraryStorage,
    target_path: P,
) -> Result<bool> {
    let target = RocksDBFileLibrary::open(target_path)?;

    // Compare folder counts
    let source_folders = source.list_folders()?.len();
    let target_folders = target.list_folders()?.len();

    if source_folders != target_folders {
        return Ok(false);
    }

    // Compare file counts
    let list_request = ListFilesRequest {
        folder_id: None,
        tags: None,
        search: None,
        status: None,
        owner: None,
        sort: None,
        order: None,
        limit: None,
        offset: None,
    };

    let source_files = source.list_files(&list_request)?.len();
    let target_files = target.list_files(&list_request)?.len();

    if source_files != target_files {
        return Ok(false);
    }

    // Compare tag counts
    let source_tags = source.list_tags()?.len();
    let target_tags = target.list_tags()?.len();

    if source_tags != target_tags {
        return Ok(false);
    }

    Ok(true)
}

/// Export in-memory storage to RocksDB, with verification
pub fn export_with_verification<P: AsRef<Path>>(
    source: &FileLibraryStorage,
    target_path: P,
) -> Result<MigrationStats> {
    let target_path_ref = target_path.as_ref();

    // Perform migration
    let stats = migrate_to_rocksdb(source, target_path_ref)?;

    // Verify
    let verified =
        verify_migration(source, target_path_ref).context("Migration verification failed")?;

    if !verified {
        anyhow::bail!("Migration verification failed: counts don't match");
    }

    Ok(stats)
}

/// Recovery statistics for orphaned files
#[derive(Debug, Clone)]
pub struct RecoveryStats {
    pub files_found: usize,
    pub files_recovered: usize,
    pub files_already_tracked: usize,
    pub errors: Vec<String>,
    pub duration_ms: u64,
}

/// Recover orphaned files from filesystem
///
/// Scans the file library storage directory and imports any files that exist on disk
/// but don't have metadata in RocksDB. This is useful when:
/// - Files were uploaded when the server was using in-memory storage
/// - RocksDB data was lost but files on disk remain
/// - After migrating from a different storage backend
pub async fn recover_orphaned_files<P1: AsRef<Path>, P2: AsRef<Path>>(
    storage_dir: P1,
    db_path: P2,
) -> Result<RecoveryStats> {
    use tokio::fs;

    let start = std::time::Instant::now();
    let mut stats = RecoveryStats {
        files_found: 0,
        files_recovered: 0,
        files_already_tracked: 0,
        errors: Vec::new(),
        duration_ms: 0,
    };

    // Open RocksDB storage
    let storage =
        RocksDBFileLibrary::open(db_path.as_ref()).context("Failed to open RocksDB storage")?;

    tracing::info!(
        "🔍 Scanning {} for orphaned files...",
        storage_dir.as_ref().display()
    );

    // Read directory
    let mut entries = fs::read_dir(storage_dir.as_ref())
        .await
        .context("Failed to read storage directory")?;

    while let Some(entry) = entries
        .next_entry()
        .await
        .context("Failed to read directory entry")?
    {
        let path = entry.path();

        // Skip directories and hidden files
        if !path.is_file()
            || path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.starts_with('.'))
                .unwrap_or(false)
        {
            continue;
        }

        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name.to_string(),
            None => {
                stats.errors.push(format!("Invalid filename: {:?}", path));
                continue;
            }
        };

        // File ID is the filename (format: file_{uuid})
        let file_id = file_name.clone();
        stats.files_found += 1;

        // Check if file already exists in database
        match storage.get_file(&file_id) {
            Ok(Some(_)) => {
                tracing::debug!("File {} already tracked in database", file_id);
                stats.files_already_tracked += 1;
                continue;
            }
            Ok(None) => {
                // File needs to be recovered
                tracing::info!("📦 Recovering orphaned file: {}", file_id);
            }
            Err(e) => {
                stats
                    .errors
                    .push(format!("Error checking file {}: {}", file_id, e));
                continue;
            }
        }

        // Get file metadata from filesystem
        let metadata = match fs::metadata(&path).await {
            Ok(m) => m,
            Err(e) => {
                stats
                    .errors
                    .push(format!("Failed to get metadata for {}: {}", file_id, e));
                continue;
            }
        };

        let size_bytes = metadata.len();
        let created_at = metadata
            .created()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| {
                use chrono::TimeZone;
                Utc.timestamp_opt(d.as_secs() as i64, 0).unwrap()
            })
            .unwrap_or_else(Utc::now);

        let file_path_str = path.to_string_lossy().to_string();

        // Create minimal DataFile record
        let data_file = DataFile {
            id: file_id.clone(),
            name: format!("{} (recovered)", file_name),
            file_path: file_path_str,
            size_bytes,
            encoding: "UTF-8".to_string(),
            delimiter: ",".to_string(),
            has_header: true,
            folder_id: None,
            tags: vec!["recovered".to_string()],
            description: Some("Recovered from filesystem - metadata was lost".to_string()),
            owner: FileOwner {
                user_id: "system".to_string(),
                email: "system@graphica.io".to_string(),
                name: "System Recovery".to_string(),
            },
            schema: None,
            status: FileStatus::Pending,
            validation_errors: Vec::new(),
            validation_warnings: vec!["File recovered from filesystem - please rescan".to_string()],
            created_at,
            updated_at: Utc::now(),
            last_accessed: None,
            metadata: std::collections::HashMap::new(),
            ontology_mappings: Vec::new(),
            sensitivity_level: None,
            retention_policy: None,
            access_control: None,
            version: Some(1),
            previous_versions: Vec::new(),
        };

        // Import to database
        match storage.create_file(data_file) {
            Ok(_) => {
                tracing::info!("✅ Recovered file: {} ({} bytes)", file_id, size_bytes);
                stats.files_recovered += 1;
            }
            Err(e) => {
                stats
                    .errors
                    .push(format!("Failed to recover file {}: {}", file_id, e));
            }
        }
    }

    stats.duration_ms = start.elapsed().as_millis() as u64;

    tracing::info!(
        "📊 Recovery complete: {} files found, {} recovered, {} already tracked, {} errors",
        stats.files_found,
        stats.files_recovered,
        stats.files_already_tracked,
        stats.errors.len()
    );

    Ok(stats)
}

/// Clean up temporary files left over from incomplete uploads
///
/// Scans the file library storage directory and removes any `.tmp` files
/// that were created during upload but never finalized (e.g., due to crashes).
pub async fn cleanup_temp_files<P: AsRef<Path>>(storage_dir: P) -> Result<usize> {
    use tokio::fs;

    let mut cleaned_count = 0;

    tracing::info!(
        "🧹 Cleaning up temporary files in {}...",
        storage_dir.as_ref().display()
    );

    // Read directory
    let mut entries = fs::read_dir(storage_dir.as_ref())
        .await
        .context("Failed to read storage directory")?;

    while let Some(entry) = entries
        .next_entry()
        .await
        .context("Failed to read directory entry")?
    {
        let path = entry.path();

        // Check if file has .tmp extension
        if path.is_file()
            && path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e == "tmp")
                .unwrap_or(false)
        {
            match fs::remove_file(&path).await {
                Ok(_) => {
                    tracing::info!("🗑️  Removed incomplete upload: {:?}", path.file_name());
                    cleaned_count += 1;
                }
                Err(e) => {
                    tracing::warn!("Failed to remove temp file {:?}: {}", path, e);
                }
            }
        }
    }

    if cleaned_count > 0 {
        tracing::info!("✅ Cleaned up {} temporary files", cleaned_count);
    } else {
        tracing::debug!("No temporary files to clean up");
    }

    Ok(cleaned_count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use tempfile::TempDir;

    #[test]
    fn test_migration_empty_storage() {
        let source = FileLibraryStorage::new();
        let temp_dir = TempDir::new().unwrap();

        let stats = migrate_to_rocksdb(&source, temp_dir.path()).unwrap();

        assert_eq!(stats.files_migrated, 0);
        // FileLibraryStorage creates a default root folder
        assert_eq!(stats.folders_migrated, 1);
        assert!(stats.errors.is_empty());
    }

    #[test]
    fn test_migration_with_folders() {
        let source = FileLibraryStorage::new();
        let temp_dir = TempDir::new().unwrap();

        // Create test folder
        let folder = Folder {
            id: "folder_1".to_string(),
            name: "Test Folder".to_string(),
            description: Some("Test".to_string()),
            parent_id: None,
            path: "/Test Folder".to_string(),
            file_count: 0,
            subfolder_count: 0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            children: None,
        };

        source.create_folder(folder.clone()).unwrap();

        let stats = migrate_to_rocksdb(&source, temp_dir.path()).unwrap();

        // 1 created folder + 1 default root folder = 2
        assert_eq!(stats.folders_migrated, 2);
        assert!(stats.errors.is_empty());

        // Verify
        let verified = verify_migration(&source, temp_dir.path()).unwrap();
        assert!(verified);
    }

    #[test]
    fn test_export_with_verification() {
        let source = FileLibraryStorage::new();
        let temp_dir = TempDir::new().unwrap();

        // Create test data
        let folder = Folder {
            id: "folder_1".to_string(),
            name: "Test Folder".to_string(),
            description: None,
            parent_id: None,
            path: "/Test Folder".to_string(),
            file_count: 0,
            subfolder_count: 0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            children: None,
        };

        source.create_folder(folder).unwrap();

        let stats = export_with_verification(&source, temp_dir.path()).unwrap();

        // 1 created folder + 1 default root folder = 2
        assert_eq!(stats.folders_migrated, 2);
        assert!(stats.errors.is_empty());
    }
}
