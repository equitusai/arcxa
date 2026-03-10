//! Integration Tests for File Library Storage
//!
//! Comprehensive test suite covering:
//! - CRUD operations for files, folders, jobs
//! - Secondary indexes (tags, folders)
//! - Search functionality
//! - Statistics aggregation
//! - Migration between storage backends
//! - Persistence across restarts

#[cfg(test)]
mod tests {
    use crate::api::file_library::{
        migration::*, storage::FileLibraryStorage, storage_rocksdb::RocksDBFileLibrary,
        storage_trait::FileLibraryStore, types::*,
    };
    use chrono::Utc;
    use std::collections::HashMap;
    use tempfile::TempDir;

    // Helper function to create a test file
    fn create_test_file(
        id: &str,
        name: &str,
        tags: Vec<String>,
        folder_id: Option<String>,
    ) -> DataFile {
        DataFile {
            id: id.to_string(),
            name: name.to_string(),
            file_path: format!("/tmp/{}", name),
            folder_id,
            description: Some(format!("Test file {}", id)),
            owner: FileOwner {
                user_id: "user_1".to_string(),
                email: "testuser@example.com".to_string(),
                name: "Test User".to_string(),
            },
            size_bytes: 1024,
            encoding: "UTF-8".to_string(),
            delimiter: ",".to_string(),
            has_header: true,
            schema: None,
            ontology_mappings: vec![],
            status: FileStatus::Validated,
            validation_errors: vec![],
            validation_warnings: vec![],
            tags,
            metadata: HashMap::new(),
            sensitivity_level: None,
            retention_policy: None,
            access_control: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_accessed: None,
            version: Some(1),
            previous_versions: vec![],
        }
    }

    // Helper function to create a test folder
    fn create_test_folder(id: &str, name: &str) -> Folder {
        Folder {
            id: id.to_string(),
            name: name.to_string(),
            description: Some(format!("Test folder {}", id)),
            parent_id: None,
            path: format!("/{}", name),
            file_count: 0,
            subfolder_count: 0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            children: None,
        }
    }

    #[test]
    fn test_rocksdb_file_crud() {
        let temp_dir = TempDir::new().unwrap();
        let storage = RocksDBFileLibrary::open(temp_dir.path()).unwrap();

        // Create
        let file = create_test_file("file_1", "test.csv", vec!["test".to_string()], None);
        storage.create_file(file.clone()).unwrap();

        // Read
        let retrieved = storage.get_file("file_1").unwrap().unwrap();
        assert_eq!(retrieved.name, "test.csv");
        assert_eq!(retrieved.tags, vec!["test".to_string()]);

        // Update
        let update = UpdateFileRequest {
            name: Some("updated.csv".to_string()),
            description: Some("Updated description".to_string()),
            folder_id: None,
            tags: Some(vec!["updated".to_string()]),
            metadata: None,
            schema: None,
            ontology_mappings: vec![],
        };
        let updated = storage.update_file("file_1", update).unwrap();
        assert_eq!(updated.name, "updated.csv");
        assert_eq!(updated.tags, vec!["updated".to_string()]);

        // Delete
        storage.delete_file("file_1").unwrap();
        assert!(storage.get_file("file_1").unwrap().is_none());
    }

    #[test]
    fn test_rocksdb_folder_operations() {
        let temp_dir = TempDir::new().unwrap();
        let storage = RocksDBFileLibrary::open(temp_dir.path()).unwrap();

        // Create folders
        let folder1 = create_test_folder("folder_1", "Folder 1");
        let folder2 = create_test_folder("folder_2", "Folder 2");

        storage.create_folder(folder1.clone()).unwrap();
        storage.create_folder(folder2.clone()).unwrap();

        // List folders
        let folders = storage.list_folders().unwrap();
        assert_eq!(folders.len(), 2);

        // Update folder
        let update = UpdateFolderRequest {
            name: Some("Updated Folder".to_string()),
            description: Some("New description".to_string()),
            parent_id: None,
        };
        let updated = storage.update_folder("folder_1", update).unwrap();
        assert_eq!(updated.name, "Updated Folder");

        // Delete folder
        storage.delete_folder("folder_1", false).unwrap();
        let folders = storage.list_folders().unwrap();
        assert_eq!(folders.len(), 1);
    }

    #[test]
    fn test_rocksdb_tag_indexing() {
        let temp_dir = TempDir::new().unwrap();
        let storage = RocksDBFileLibrary::open(temp_dir.path()).unwrap();

        // Create files with different tags
        let file1 = create_test_file(
            "file_1",
            "file1.csv",
            vec!["csv".to_string(), "important".to_string()],
            None,
        );
        let file2 = create_test_file("file_2", "file2.csv", vec!["csv".to_string()], None);
        let file3 = create_test_file("file_3", "file3.json", vec!["json".to_string()], None);

        storage.create_file(file1).unwrap();
        storage.create_file(file2).unwrap();
        storage.create_file(file3).unwrap();

        // Query by tag
        let request = ListFilesRequest {
            folder_id: None,
            tags: Some(vec!["csv".to_string()]),
            search: None,
            status: None,
            owner: None,
            sort: None,
            order: None,
            limit: None,
            offset: None,
        };

        let files = storage.list_files(&request).unwrap();
        assert_eq!(files.len(), 2);

        // List all tags
        let tags = storage.list_tags().unwrap();
        assert_eq!(tags.len(), 3); // csv, important, json

        // Verify tag counts
        let csv_tag = tags.iter().find(|t| t.name == "csv").unwrap();
        assert_eq!(csv_tag.count, 2);
    }

    #[test]
    fn test_rocksdb_folder_indexing() {
        let temp_dir = TempDir::new().unwrap();
        let storage = RocksDBFileLibrary::open(temp_dir.path()).unwrap();

        // Create folder
        let folder = create_test_folder("folder_1", "Test Folder");
        storage.create_folder(folder).unwrap();

        // Create files in folder
        let file1 = create_test_file("file_1", "file1.csv", vec![], Some("folder_1".to_string()));
        let file2 = create_test_file("file_2", "file2.csv", vec![], Some("folder_1".to_string()));
        let file3 = create_test_file("file_3", "file3.csv", vec![], None);

        storage.create_file(file1).unwrap();
        storage.create_file(file2).unwrap();
        storage.create_file(file3).unwrap();

        // Query files in folder
        let request = ListFilesRequest {
            folder_id: Some("folder_1".to_string()),
            tags: None,
            search: None,
            status: None,
            owner: None,
            sort: None,
            order: None,
            limit: None,
            offset: None,
        };

        let files = storage.list_files(&request).unwrap();
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn test_rocksdb_search() {
        let temp_dir = TempDir::new().unwrap();
        let storage = RocksDBFileLibrary::open(temp_dir.path()).unwrap();

        // Create files with different names
        let file1 = create_test_file("file_1", "customers.csv", vec![], None);
        let file2 = create_test_file("file_2", "products.csv", vec![], None);
        let file3 = create_test_file("file_3", "customer_orders.csv", vec![], None);

        storage.create_file(file1).unwrap();
        storage.create_file(file2).unwrap();
        storage.create_file(file3).unwrap();

        // Search for "customer"
        let request = SearchRequest {
            query: "customer".to_string(),
            filters: None,
            sort: None,
            limit: None,
            offset: None,
        };

        let results = storage.search_files(&request).unwrap();
        assert_eq!(results.len(), 2); // customers.csv and customer_orders.csv
    }

    #[test]
    fn test_rocksdb_statistics() {
        let temp_dir = TempDir::new().unwrap();
        let storage = RocksDBFileLibrary::open(temp_dir.path()).unwrap();

        // Create test data
        let folder = create_test_folder("folder_1", "Test Folder");
        storage.create_folder(folder).unwrap();

        let file1 = create_test_file(
            "file_1",
            "file1.csv",
            vec!["csv".to_string()],
            Some("folder_1".to_string()),
        );
        let file2 = create_test_file(
            "file_2",
            "file2.csv",
            vec!["csv".to_string(), "important".to_string()],
            None,
        );

        storage.create_file(file1).unwrap();
        storage.create_file(file2).unwrap();

        // Get statistics
        let stats = storage.get_statistics().unwrap();

        assert_eq!(stats.total_files, 2);
        assert_eq!(stats.total_size_bytes, 2048); // 1024 * 2
        assert_eq!(stats.top_tags.len(), 2); // csv, important
        assert!(stats.files_by_folder.contains_key("folder_1"));
    }

    #[test]
    fn test_rocksdb_persistence_across_restarts() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().to_path_buf();

        // First session: create data
        {
            let storage = RocksDBFileLibrary::open(&path).unwrap();
            let file = create_test_file(
                "persist_1",
                "persistent.csv",
                vec!["test".to_string()],
                None,
            );
            storage.create_file(file).unwrap();

            let folder = create_test_folder("folder_persist", "Persistent Folder");
            storage.create_folder(folder).unwrap();
        }

        // Second session: verify data persisted
        {
            let storage = RocksDBFileLibrary::open(&path).unwrap();

            let file = storage.get_file("persist_1").unwrap().unwrap();
            assert_eq!(file.name, "persistent.csv");

            let folder = storage.get_folder("folder_persist").unwrap().unwrap();
            assert_eq!(folder.name, "Persistent Folder");

            let tags = storage.list_tags().unwrap();
            assert_eq!(tags.len(), 1);
            assert_eq!(tags[0].name, "test");
        }
    }

    #[test]
    fn test_migration_in_memory_to_rocksdb() {
        let source = FileLibraryStorage::new();
        let temp_dir = TempDir::new().unwrap();

        // Populate in-memory storage
        let folder = create_test_folder("folder_1", "Test Folder");
        source.create_folder(folder).unwrap();

        let file1 = create_test_file(
            "file_1",
            "file1.csv",
            vec!["test".to_string()],
            Some("folder_1".to_string()),
        );
        let file2 = create_test_file(
            "file_2",
            "file2.csv",
            vec!["test".to_string(), "important".to_string()],
            None,
        );

        source.create_file(file1).unwrap();
        source.create_file(file2).unwrap();

        // Migrate
        let stats = migrate_to_rocksdb(&source, temp_dir.path()).unwrap();

        assert_eq!(stats.files_migrated, 2);
        // FileLibraryStorage creates a default root folder, but the files don't specify folders
        // So we get the root folder migrated (1 folder from initialization, possibly 1 more from file parent_folder_id)
        assert_eq!(stats.folders_migrated, 2);
        assert!(stats.errors.is_empty());

        // Verify migration
        let verified = verify_migration(&source, temp_dir.path()).unwrap();
        assert!(verified);

        // Verify data in RocksDB
        let target = RocksDBFileLibrary::open(temp_dir.path()).unwrap();

        let file = target.get_file("file_1").unwrap().unwrap();
        assert_eq!(file.name, "file1.csv");

        let tags = target.list_tags().unwrap();
        assert_eq!(tags.len(), 2); // test, important
    }

    #[test]
    fn test_complex_filtering() {
        let temp_dir = TempDir::new().unwrap();
        let storage = RocksDBFileLibrary::open(temp_dir.path()).unwrap();

        // Create complex test data
        let folder1 = create_test_folder("folder_1", "Folder 1");
        let folder2 = create_test_folder("folder_2", "Folder 2");
        storage.create_folder(folder1).unwrap();
        storage.create_folder(folder2).unwrap();

        let file1 = create_test_file(
            "file_1",
            "file1.csv",
            vec!["csv".to_string(), "important".to_string()],
            Some("folder_1".to_string()),
        );
        let file2 = create_test_file(
            "file_2",
            "file2.csv",
            vec!["csv".to_string()],
            Some("folder_1".to_string()),
        );
        let file3 = create_test_file(
            "file_3",
            "file3.json",
            vec!["json".to_string()],
            Some("folder_2".to_string()),
        );
        let file4 = create_test_file("file_4", "file4.csv", vec!["csv".to_string()], None);

        storage.create_file(file1).unwrap();
        storage.create_file(file2).unwrap();
        storage.create_file(file3).unwrap();
        storage.create_file(file4).unwrap();

        // Filter by folder and tags
        let request = ListFilesRequest {
            folder_id: Some("folder_1".to_string()),
            tags: Some(vec!["important".to_string()]),
            search: None,
            status: None,
            owner: None,
            sort: None,
            order: None,
            limit: None,
            offset: None,
        };

        let files = storage.list_files(&request).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].id, "file_1");
    }

    #[test]
    fn test_pagination() {
        let temp_dir = TempDir::new().unwrap();
        let storage = RocksDBFileLibrary::open(temp_dir.path()).unwrap();

        // Create 10 files
        for i in 1..=10 {
            let file = create_test_file(
                &format!("file_{}", i),
                &format!("file{}.csv", i),
                vec![],
                None,
            );
            storage.create_file(file).unwrap();
        }

        // Get first page
        let request = ListFilesRequest {
            folder_id: None,
            tags: None,
            search: None,
            status: None,
            owner: None,
            sort: None,
            order: None,
            limit: Some(5),
            offset: Some(0),
        };

        let page1 = storage.list_files(&request).unwrap();
        assert_eq!(page1.len(), 5);

        // Get second page
        let request = ListFilesRequest {
            folder_id: None,
            tags: None,
            search: None,
            status: None,
            owner: None,
            sort: None,
            order: None,
            limit: Some(5),
            offset: Some(5),
        };

        let page2 = storage.list_files(&request).unwrap();
        assert_eq!(page2.len(), 5);

        // Verify no overlap
        let page1_ids: Vec<_> = page1.iter().map(|f| &f.id).collect();
        let page2_ids: Vec<_> = page2.iter().map(|f| &f.id).collect();
        assert!(page1_ids.iter().all(|id| !page2_ids.contains(id)));
    }

    #[test]
    fn test_update_preserves_indexes() {
        let temp_dir = TempDir::new().unwrap();
        let storage = RocksDBFileLibrary::open(temp_dir.path()).unwrap();

        let folder1 = create_test_folder("folder_1", "Folder 1");
        let folder2 = create_test_folder("folder_2", "Folder 2");
        storage.create_folder(folder1).unwrap();
        storage.create_folder(folder2).unwrap();

        // Create file in folder_1 with tag "old"
        let file = create_test_file(
            "file_1",
            "file.csv",
            vec!["old".to_string()],
            Some("folder_1".to_string()),
        );
        storage.create_file(file).unwrap();

        // Verify initial state
        let request = ListFilesRequest {
            folder_id: Some("folder_1".to_string()),
            tags: None,
            search: None,
            status: None,
            owner: None,
            sort: None,
            order: None,
            limit: None,
            offset: None,
        };
        assert_eq!(storage.list_files(&request).unwrap().len(), 1);

        // Update file to folder_2 with tag "new"
        let update = UpdateFileRequest {
            name: None,
            description: None,
            folder_id: Some("folder_2".to_string()),
            tags: Some(vec!["new".to_string()]),
            metadata: None,
            schema: None,
            ontology_mappings: vec![],
        };
        storage.update_file("file_1", update).unwrap();

        // Verify file moved from folder_1
        let request = ListFilesRequest {
            folder_id: Some("folder_1".to_string()),
            tags: None,
            search: None,
            status: None,
            owner: None,
            sort: None,
            order: None,
            limit: None,
            offset: None,
        };
        assert_eq!(storage.list_files(&request).unwrap().len(), 0);

        // Verify file in folder_2
        let request = ListFilesRequest {
            folder_id: Some("folder_2".to_string()),
            tags: None,
            search: None,
            status: None,
            owner: None,
            sort: None,
            order: None,
            limit: None,
            offset: None,
        };
        assert_eq!(storage.list_files(&request).unwrap().len(), 1);

        // Verify tag indexes updated
        let tags = storage.list_tags().unwrap();
        assert!(tags.iter().any(|t| t.name == "new"));
        assert!(!tags.iter().any(|t| t.name == "old"));
    }
}
