//! Comprehensive tests for File Library
//!
//! Tests cover storage, scanning, handlers, and integration scenarios.

#[cfg(test)]
mod storage_tests {
    use super::super::storage::FileLibraryStorage;
    use super::super::types::*;
    use chrono::Utc;

    fn create_test_file(id: &str, name: &str) -> DataFile {
        DataFile {
            id: id.to_string(),
            name: name.to_string(),
            file_path: format!("/test/{}", id),
            folder_id: None,
            description: None,
            owner: FileOwner {
                user_id: "test_user".to_string(),
                email: "test@example.com".to_string(),
                name: "Test User".to_string(),
            },
            size_bytes: 1024,
            encoding: "UTF-8".to_string(),
            delimiter: ",".to_string(),
            has_header: true,
            schema: None,
            ontology_mappings: vec![],
            status: FileStatus::Pending,
            validation_errors: Vec::new(),
            validation_warnings: Vec::new(),
            tags: vec!["test".to_string()],
            metadata: std::collections::HashMap::new(),
            sensitivity_level: None,
            retention_policy: None,
            access_control: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_accessed: None,
            version: Some(1),
            previous_versions: Vec::new(),
        }
    }

    #[test]
    fn test_storage_create_and_get() {
        let storage = FileLibraryStorage::new();
        let file = create_test_file("file1", "test.csv");

        storage.create_file(file.clone()).unwrap();

        let retrieved = storage.get_file("file1").unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "test.csv");
    }

    #[test]
    fn test_storage_update() {
        let storage = FileLibraryStorage::new();
        let file = create_test_file("file1", "test.csv");

        storage.create_file(file.clone()).unwrap();

        let update_req = UpdateFileRequest {
            name: Some("updated.csv".to_string()),
            description: Some("Updated description".to_string()),
            tags: None,
            folder_id: None,
            metadata: None,
            schema: None,
            ontology_mappings: vec![],
        };

        let updated = storage.update_file("file1", update_req).unwrap();
        assert_eq!(updated.name, "updated.csv");
        assert_eq!(updated.description, Some("Updated description".to_string()));
    }

    #[test]
    fn test_storage_delete() {
        let storage = FileLibraryStorage::new();
        let file = create_test_file("file1", "test.csv");

        storage.create_file(file.clone()).unwrap();
        assert!(storage.get_file("file1").unwrap().is_some());

        storage.delete_file("file1").unwrap();
        assert!(storage.get_file("file1").unwrap().is_none());
    }

    #[test]
    fn test_storage_list_with_filters() {
        let storage = FileLibraryStorage::new();

        // Create test files
        let mut file1 = create_test_file("file1", "sales.csv");
        file1.tags = vec!["sales".to_string(), "2024".to_string()];
        file1.status = FileStatus::Validated;

        let mut file2 = create_test_file("file2", "marketing.csv");
        file2.tags = vec!["marketing".to_string()];
        file2.status = FileStatus::Warning;

        storage.create_file(file1).unwrap();
        storage.create_file(file2).unwrap();

        // Test tag filtering
        let req = ListFilesRequest {
            folder_id: None,
            tags: Some(vec!["sales".to_string()]),
            search: None,
            status: None,
            owner: None,
            sort: None,
            order: None,
            limit: None,
            offset: None,
        };

        let results = storage.list_files(&req).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "sales.csv");

        // Test status filtering
        let req = ListFilesRequest {
            folder_id: None,
            tags: None,
            search: None,
            status: Some(FileStatus::Warning),
            owner: None,
            sort: None,
            order: None,
            limit: None,
            offset: None,
        };

        let results = storage.list_files(&req).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "marketing.csv");
    }

    #[test]
    fn test_storage_search() {
        let storage = FileLibraryStorage::new();

        let mut file1 = create_test_file("file1", "customer_data.csv");
        file1.description = Some("Customer information".to_string());

        let mut file2 = create_test_file("file2", "product_catalog.csv");
        file2.description = Some("Product listings".to_string());

        storage.create_file(file1).unwrap();
        storage.create_file(file2).unwrap();

        // Search by name
        let req = SearchRequest {
            query: "customer".to_string(),
            filters: None,
            sort: None,
            limit: None,
            offset: None,
        };

        let results = storage.search_files(&req).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "customer_data.csv");

        // Search by description
        let req = SearchRequest {
            query: "Product".to_string(),
            filters: None,
            sort: None,
            limit: None,
            offset: None,
        };

        let results = storage.search_files(&req).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "product_catalog.csv");
    }

    #[test]
    fn test_folder_operations() {
        let storage = FileLibraryStorage::new();

        let folder = Folder {
            id: "folder1".to_string(),
            name: "Sales".to_string(),
            parent_id: None,
            description: Some("Sales data folder".to_string()),
            path: "/Sales".to_string(),
            file_count: 0,
            subfolder_count: 0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            children: None,
        };

        storage.create_folder(folder).unwrap();

        let folders = storage.list_folders().unwrap();
        assert!(folders.len() >= 2); // root + our folder

        let retrieved = storage.get_folder("folder1").unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "Sales");
    }

    #[test]
    fn test_tag_index() {
        let storage = FileLibraryStorage::new();

        let mut file1 = create_test_file("file1", "test1.csv");
        file1.tags = vec!["urgent".to_string(), "sales".to_string()];

        let mut file2 = create_test_file("file2", "test2.csv");
        file2.tags = vec!["urgent".to_string(), "marketing".to_string()];

        storage.create_file(file1).unwrap();
        storage.create_file(file2).unwrap();

        let tags = storage.list_tags().unwrap();

        // Find "urgent" tag
        let urgent_tag = tags.iter().find(|t| t.name == "urgent");
        assert!(urgent_tag.is_some());
        assert_eq!(urgent_tag.unwrap().count, 2);

        // Find "sales" tag
        let sales_tag = tags.iter().find(|t| t.name == "sales");
        assert!(sales_tag.is_some());
        assert_eq!(sales_tag.unwrap().count, 1);
    }

    #[test]
    fn test_statistics() {
        let storage = FileLibraryStorage::new();

        let mut file1 = create_test_file("file1", "test1.csv");
        file1.size_bytes = 1000;
        file1.status = FileStatus::Validated;

        let mut file2 = create_test_file("file2", "test2.csv");
        file2.size_bytes = 2000;
        file2.status = FileStatus::Warning;

        storage.create_file(file1).unwrap();
        storage.create_file(file2).unwrap();

        let stats = storage.get_statistics().unwrap();

        assert_eq!(stats.total_files, 2);
        assert_eq!(stats.total_size_bytes, 3000);
        assert!(stats.files_by_status.len() > 0);
    }

    #[test]
    fn test_concurrent_access() {
        use std::sync::Arc;
        use std::thread;

        let storage = Arc::new(FileLibraryStorage::new());
        let mut handles = vec![];

        // Spawn 10 threads creating files concurrently
        for i in 0..10 {
            let storage_clone = storage.clone();
            let handle = thread::spawn(move || {
                let file = create_test_file(&format!("file{}", i), &format!("test{}.csv", i));
                storage_clone.create_file(file).unwrap();
            });
            handles.push(handle);
        }

        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }

        // Verify all files were created
        let req = ListFilesRequest {
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

        let files = storage.list_files(&req).unwrap();
        assert_eq!(files.len(), 10);
    }
}

#[cfg(test)]
mod scanner_tests {
    use super::super::scanner::FileScanner;
    use super::super::types::*;
    use std::io::Write;

    #[test]
    fn test_csv_scanning() {
        // Create a temporary CSV file
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join("test_scan.csv");

        let csv_content = "name,age,email\nJohn,30,john@example.com\nJane,25,jane@example.com\nBob,35,bob@example.com\n";
        std::fs::write(&file_path, csv_content).unwrap();

        let scanner = FileScanner::new();
        let result = scanner
            .scan_file(
                file_path.to_str().unwrap(),
                ScanFileRequest {
                    delimiter: None,
                    encoding: None,
                    has_header: None,
                    sample_rows: None,
                    auto_save: None,
                    map_to_ontology: None,
                    ontology_id: None,
                },
            )
            .unwrap();

        // Verify detection
        assert_eq!(result.delimiter_detected, Some(",".to_string()));
        assert_eq!(result.has_header_detected, Some(true));
        assert_eq!(result.detected_fields.len(), 3);

        // Check field types
        let name_field = result.detected_fields.iter().find(|f| f.name == "name");
        assert!(name_field.is_some());

        let age_field = result.detected_fields.iter().find(|f| f.name == "age");
        assert!(age_field.is_some());
        assert_eq!(age_field.unwrap().field_type, FieldType::Integer);

        let email_field = result.detected_fields.iter().find(|f| f.name == "email");
        assert!(email_field.is_some());
        assert_eq!(email_field.unwrap().is_pii, Some(true));
        assert_eq!(email_field.unwrap().pii_type, Some(PiiType::Email));

        // Verify row count
        assert_eq!(result.total_rows, Some(3));

        // Cleanup
        std::fs::remove_file(file_path).ok();
    }

    #[test]
    fn test_tsv_scanning() {
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join("test_scan.tsv");

        let tsv_content = "id\tproduct\tprice\n1\tLaptop\t999.99\n2\tMouse\t29.99\n";
        std::fs::write(&file_path, tsv_content).unwrap();

        let scanner = FileScanner::new();
        let result = scanner
            .scan_file(
                file_path.to_str().unwrap(),
                ScanFileRequest {
                    delimiter: None,
                    encoding: None,
                    has_header: None,
                    sample_rows: None,
                    auto_save: None,
                    map_to_ontology: None,
                    ontology_id: None,
                },
            )
            .unwrap();

        // Verify tab delimiter detected
        assert_eq!(result.delimiter_detected, Some("\t".to_string()));
        assert_eq!(result.detected_fields.len(), 3);

        // Check field types
        let id_field = result.detected_fields.iter().find(|f| f.name == "id");
        assert!(id_field.is_some());
        assert_eq!(id_field.unwrap().field_type, FieldType::Integer);

        let price_field = result.detected_fields.iter().find(|f| f.name == "price");
        assert!(price_field.is_some());
        assert_eq!(price_field.unwrap().field_type, FieldType::Float);

        std::fs::remove_file(file_path).ok();
    }

    #[test]
    fn test_pii_detection() {
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join("test_pii.csv");

        let csv_content = "name,email,phone,ssn\nJohn,john@test.com,555-1234,123-45-6789\n";
        std::fs::write(&file_path, csv_content).unwrap();

        let scanner = FileScanner::new();
        let result = scanner
            .scan_file(
                file_path.to_str().unwrap(),
                ScanFileRequest {
                    delimiter: Some(",".to_string()),
                    encoding: None,
                    has_header: Some(true),
                    sample_rows: None,
                    auto_save: None,
                    map_to_ontology: None,
                    ontology_id: None,
                },
            )
            .unwrap();

        // Check PII detection
        let email_field = result.detected_fields.iter().find(|f| f.name == "email");
        assert!(email_field.is_some());
        assert_eq!(email_field.unwrap().is_pii, Some(true));
        assert_eq!(email_field.unwrap().pii_type, Some(PiiType::Email));

        let phone_field = result.detected_fields.iter().find(|f| f.name == "phone");
        assert!(phone_field.is_some());
        assert_eq!(phone_field.unwrap().is_pii, Some(true));

        let ssn_field = result.detected_fields.iter().find(|f| f.name == "ssn");
        assert!(ssn_field.is_some());
        assert_eq!(ssn_field.unwrap().is_pii, Some(true));
        assert_eq!(ssn_field.unwrap().pii_type, Some(PiiType::Ssn));

        // Verify warnings
        assert!(!result.warnings.is_empty());
        assert!(result.warnings.iter().any(|w| w.contains("PII")));

        std::fs::remove_file(file_path).ok();
    }

    #[test]
    fn test_empty_file() {
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join("test_empty.csv");

        std::fs::write(&file_path, "").unwrap();

        let scanner = FileScanner::new();
        let result = scanner
            .scan_file(
                file_path.to_str().unwrap(),
                ScanFileRequest {
                    delimiter: None,
                    encoding: None,
                    has_header: None,
                    sample_rows: None,
                    auto_save: None,
                    map_to_ontology: None,
                    ontology_id: None,
                },
            )
            .unwrap();

        assert_eq!(result.total_rows, Some(0));
        assert!(!result.warnings.is_empty());

        std::fs::remove_file(file_path).ok();
    }

    #[test]
    fn test_type_inference() {
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join("test_types.csv");

        let csv_content = "int_col,float_col,bool_col,string_col\n1,1.5,true,hello\n2,2.5,false,world\n3,3.5,true,test\n";
        std::fs::write(&file_path, csv_content).unwrap();

        let scanner = FileScanner::new();
        let result = scanner
            .scan_file(
                file_path.to_str().unwrap(),
                ScanFileRequest {
                    delimiter: Some(",".to_string()),
                    encoding: None,
                    has_header: Some(true),
                    sample_rows: None,
                    auto_save: None,
                    map_to_ontology: None,
                    ontology_id: None,
                },
            )
            .unwrap();

        let int_field = result.detected_fields.iter().find(|f| f.name == "int_col");
        assert_eq!(int_field.unwrap().field_type, FieldType::Integer);

        let float_field = result
            .detected_fields
            .iter()
            .find(|f| f.name == "float_col");
        assert_eq!(float_field.unwrap().field_type, FieldType::Float);

        let bool_field = result.detected_fields.iter().find(|f| f.name == "bool_col");
        assert_eq!(bool_field.unwrap().field_type, FieldType::Boolean);

        let string_field = result
            .detected_fields
            .iter()
            .find(|f| f.name == "string_col");
        assert_eq!(string_field.unwrap().field_type, FieldType::String);

        std::fs::remove_file(file_path).ok();
    }
}

#[cfg(test)]
mod integration_tests {
    use super::super::scanner::FileScanner;
    use super::super::storage::FileLibraryStorage;
    use super::super::types::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_upload_and_scan_workflow() {
        let storage = Arc::new(FileLibraryStorage::new());
        let scanner = FileScanner::new();

        // Create test CSV
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join("integration_test.csv");
        let csv_content = "name,age,email\nAlice,28,alice@example.com\nBob,32,bob@example.com\n";
        std::fs::write(&file_path, csv_content).unwrap();

        // Scan file
        let scan_result = scanner
            .scan_file(
                file_path.to_str().unwrap(),
                ScanFileRequest {
                    delimiter: None,
                    encoding: None,
                    has_header: None,
                    sample_rows: None,
                    auto_save: None,
                    map_to_ontology: None,
                    ontology_id: None,
                },
            )
            .unwrap();

        // Create file metadata
        let file = DataFile {
            id: "test_file_1".to_string(),
            name: "integration_test.csv".to_string(),
            file_path: file_path.to_str().unwrap().to_string(),
            folder_id: None,
            description: Some("Integration test file".to_string()),
            owner: FileOwner {
                user_id: "test_user".to_string(),
                email: "test@example.com".to_string(),
                name: "Test User".to_string(),
            },
            size_bytes: csv_content.len() as u64,
            encoding: scan_result
                .encoding_detected
                .clone()
                .unwrap_or_else(|| "UTF-8".to_string()),
            delimiter: scan_result
                .delimiter_detected
                .clone()
                .unwrap_or_else(|| ",".to_string()),
            has_header: scan_result.has_header_detected.unwrap_or(true),
            schema: Some(FileSchema {
                fields: scan_result.detected_fields.clone(),
                total_rows: scan_result.total_rows.unwrap_or(0),
                estimated_rows: scan_result.estimated_rows,
                last_scanned: scan_result.scan_timestamp,
            }),
            ontology_mappings: vec![],
            status: if scan_result.warnings.is_empty() {
                FileStatus::Validated
            } else {
                FileStatus::Warning
            },
            validation_errors: scan_result.errors.clone(),
            validation_warnings: scan_result.warnings.clone(),
            tags: vec!["integration-test".to_string()],
            metadata: std::collections::HashMap::new(),
            sensitivity_level: None,
            retention_policy: None,
            access_control: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            last_accessed: None,
            version: Some(1),
            previous_versions: Vec::new(),
        };

        // Store file
        storage.create_file(file.clone()).unwrap();

        // Retrieve and verify
        let retrieved = storage.get_file("test_file_1").unwrap();
        assert!(retrieved.is_some());

        let retrieved_file = retrieved.unwrap();
        assert_eq!(retrieved_file.name, "integration_test.csv");
        assert!(retrieved_file.schema.is_some());

        let schema = retrieved_file.schema.unwrap();
        assert_eq!(schema.fields.len(), 3);
        assert_eq!(schema.total_rows, 2);

        // Check PII was detected
        let email_field = schema.fields.iter().find(|f| f.name == "email");
        assert!(email_field.is_some());
        assert_eq!(email_field.unwrap().is_pii, Some(true));

        // Cleanup
        std::fs::remove_file(file_path).ok();
    }

    #[tokio::test]
    async fn test_folder_organization() {
        let storage = Arc::new(FileLibraryStorage::new());

        // Create folders
        let parent = Folder {
            id: "parent_1".to_string(),
            name: "Data".to_string(),
            parent_id: None,
            description: Some("Data folder".to_string()),
            path: "/Data".to_string(),
            file_count: 0,
            subfolder_count: 0,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            children: None,
        };

        storage.create_folder(parent).unwrap();

        let child = Folder {
            id: "child_1".to_string(),
            name: "Sales".to_string(),
            parent_id: Some("parent_1".to_string()),
            description: Some("Sales data".to_string()),
            path: "/Data/Sales".to_string(),
            file_count: 0,
            subfolder_count: 0,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            children: None,
        };

        storage.create_folder(child).unwrap();

        // Create file in child folder
        let file = DataFile {
            id: "file_in_folder".to_string(),
            name: "sales_q1.csv".to_string(),
            file_path: "/test/sales_q1.csv".to_string(),
            folder_id: Some("child_1".to_string()),
            description: None,
            owner: FileOwner {
                user_id: "test".to_string(),
                email: "test@test.com".to_string(),
                name: "Test".to_string(),
            },
            size_bytes: 1024,
            encoding: "UTF-8".to_string(),
            delimiter: ",".to_string(),
            has_header: true,
            schema: None,
            ontology_mappings: vec![],
            status: FileStatus::Pending,
            validation_errors: Vec::new(),
            validation_warnings: Vec::new(),
            tags: Vec::new(),
            metadata: std::collections::HashMap::new(),
            sensitivity_level: None,
            retention_policy: None,
            access_control: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            last_accessed: None,
            version: Some(1),
            previous_versions: Vec::new(),
        };

        storage.create_file(file).unwrap();

        // List files in folder
        let req = ListFilesRequest {
            folder_id: Some("child_1".to_string()),
            tags: None,
            search: None,
            status: None,
            owner: None,
            sort: None,
            order: None,
            limit: None,
            offset: None,
        };

        let files = storage.list_files(&req).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].name, "sales_q1.csv");
    }
}

#[cfg(test)]
mod lineage_tests {
    use super::super::lineage::{FileLineageTracker, FileUsageType};
    use crate::storage::LineageStorage;
    use graphica_core::core::lineage::LineageSink;
    use std::sync::Arc;

    fn create_test_lineage_storage() -> Arc<LineageStorage> {
        use uuid::Uuid;
        let temp_dir = std::env::temp_dir();
        let unique_id = Uuid::new_v4();
        let rocks_path = temp_dir
            .join(format!("test_rocks_{}", unique_id))
            .to_string_lossy()
            .to_string();
        let parquet_path = temp_dir
            .join(format!("test_parquet_{}", unique_id))
            .to_string_lossy()
            .to_string();
        let cold_path = temp_dir
            .join(format!("test_cold_{}", unique_id))
            .to_string_lossy()
            .to_string();
        Arc::new(LineageStorage::new_for_tests(&rocks_path, &parquet_path, &cold_path).unwrap())
    }

    #[tokio::test]
    async fn test_track_file_usage_workflow_read() {
        let lineage_storage = create_test_lineage_storage();
        let tracker = FileLineageTracker::new(lineage_storage.clone());

        // Track workflow reading a file
        let result = tracker
            .track_file_usage(
                "file_123",
                "/data/customers.csv",
                FileUsageType::WorkflowRead {
                    workflow_id: "wf_001".to_string(),
                    step_id: "extract".to_string(),
                },
            )
            .await;

        assert!(result.is_ok());

        // Verify lineage event was recorded
        let events = lineage_storage.get_record_lineage("file_123").unwrap();
        assert_eq!(events.len(), 1);

        let event = &events[0];
        assert_eq!(event.dataset, "file_123");
        assert_eq!(event.record_id, "file_123");
        assert_eq!(event.transforms.len(), 1);
        assert_eq!(event.transforms[0].transform_type, "workflow_step");
        assert!(event.transforms[0].rule_id.contains("workflow:wf_001"));
    }

    #[tokio::test]
    async fn test_track_file_usage_api_query() {
        let lineage_storage = create_test_lineage_storage();
        let tracker = FileLineageTracker::new(lineage_storage.clone());

        // Track API query of a file
        let result = tracker
            .track_file_usage(
                "file_456",
                "/data/products.csv",
                FileUsageType::ApiQuery {
                    endpoint: "/api/v1/files/preview".to_string(),
                    user_id: "user_789".to_string(),
                },
            )
            .await;

        assert!(result.is_ok());

        let events = lineage_storage.get_record_lineage("file_456").unwrap();
        assert_eq!(events.len(), 1);

        let event = &events[0];
        assert_eq!(event.transforms[0].transform_type, "api_query");
        assert!(event.transforms[0].rule_id.contains("user:user_789"));
    }

    #[tokio::test]
    async fn test_track_file_usage_download() {
        let lineage_storage = create_test_lineage_storage();
        let tracker = FileLineageTracker::new(lineage_storage.clone());

        // Track file download
        let result = tracker
            .track_file_usage(
                "file_789",
                "/data/sales.csv",
                FileUsageType::Download {
                    user_id: "analyst_001".to_string(),
                },
            )
            .await;

        assert!(result.is_ok());

        let events = lineage_storage.get_record_lineage("file_789").unwrap();
        assert_eq!(events.len(), 1);

        let event = &events[0];
        assert_eq!(event.transforms[0].transform_type, "file_download");
    }

    #[tokio::test]
    async fn test_get_file_impact_no_dependencies() {
        let lineage_storage = create_test_lineage_storage();
        let tracker = FileLineageTracker::new(lineage_storage.clone());

        // Get impact for file with no usage
        let impact = tracker.get_file_impact("unused_file").await.unwrap();

        assert_eq!(impact.file_id, "unused_file");
        assert_eq!(impact.dependent_workflows.len(), 0);
        assert_eq!(impact.dependent_transforms.len(), 0);
        assert_eq!(impact.recent_usage_count, 0);
        assert!(impact.can_safely_delete);
        assert!(impact.can_safely_modify);
        assert_eq!(impact.warnings.len(), 0);
    }

    #[tokio::test]
    async fn test_get_file_impact_with_workflow_dependency() {
        let lineage_storage = create_test_lineage_storage();
        let tracker = FileLineageTracker::new(lineage_storage.clone());

        // Track workflow usage
        tracker
            .track_file_usage(
                "file_with_deps",
                "/data/important.csv",
                FileUsageType::WorkflowRead {
                    workflow_id: "critical_workflow".to_string(),
                    step_id: "load_data".to_string(),
                },
            )
            .await
            .unwrap();

        // Get impact analysis
        let impact = tracker.get_file_impact("file_with_deps").await.unwrap();

        assert_eq!(impact.file_id, "file_with_deps");
        assert_eq!(impact.dependent_workflows.len(), 1);
        assert_eq!(
            impact.dependent_workflows[0].workflow_id,
            "critical_workflow"
        );
        assert_eq!(impact.dependent_workflows[0].step_id, "load_data");
        assert!(impact.dependent_workflows[0].is_active);
        assert_eq!(impact.recent_usage_count, 1);
        assert!(!impact.can_safely_delete); // Has active workflow
        assert!(!impact.can_safely_modify); // Has active workflow
        assert!(!impact.warnings.is_empty());
    }

    #[tokio::test]
    async fn test_get_file_impact_with_multiple_workflows() {
        let lineage_storage = create_test_lineage_storage();
        let tracker = FileLineageTracker::new(lineage_storage.clone());

        let file_id = "shared_file";

        // Track multiple workflow usages
        tracker
            .track_file_usage(
                file_id,
                "/data/shared.csv",
                FileUsageType::WorkflowRead {
                    workflow_id: "workflow_a".to_string(),
                    step_id: "step1".to_string(),
                },
            )
            .await
            .unwrap();

        tracker
            .track_file_usage(
                file_id,
                "/data/shared.csv",
                FileUsageType::WorkflowRead {
                    workflow_id: "workflow_b".to_string(),
                    step_id: "step2".to_string(),
                },
            )
            .await
            .unwrap();

        // Get impact
        let impact = tracker.get_file_impact(file_id).await.unwrap();

        assert_eq!(impact.dependent_workflows.len(), 2);
        assert_eq!(impact.recent_usage_count, 2);

        // Verify both workflows are tracked
        let workflow_ids: Vec<String> = impact
            .dependent_workflows
            .iter()
            .map(|w| w.workflow_id.clone())
            .collect();
        assert!(workflow_ids.contains(&"workflow_a".to_string()));
        assert!(workflow_ids.contains(&"workflow_b".to_string()));
    }

    #[tokio::test]
    async fn test_get_usage_stats() {
        let lineage_storage = create_test_lineage_storage();
        let tracker = FileLineageTracker::new(lineage_storage.clone());

        let file_id = "analytics_file";

        // Track various types of usage
        tracker
            .track_file_usage(
                file_id,
                "/data/analytics.csv",
                FileUsageType::WorkflowRead {
                    workflow_id: "wf_analytics".to_string(),
                    step_id: "analyze".to_string(),
                },
            )
            .await
            .unwrap();

        tracker
            .track_file_usage(
                file_id,
                "/data/analytics.csv",
                FileUsageType::Download {
                    user_id: "analyst_1".to_string(),
                },
            )
            .await
            .unwrap();

        tracker
            .track_file_usage(
                file_id,
                "/data/analytics.csv",
                FileUsageType::Download {
                    user_id: "analyst_2".to_string(),
                },
            )
            .await
            .unwrap();

        tracker
            .track_file_usage(
                file_id,
                "/data/analytics.csv",
                FileUsageType::Preview {
                    user_id: "analyst_1".to_string(),
                },
            )
            .await
            .unwrap();

        // Get usage stats for last 30 days
        let stats = tracker.get_usage_stats(file_id, 30).await.unwrap();

        assert_eq!(stats.file_id, file_id);
        assert_eq!(stats.total_accesses, 4);
        assert_eq!(stats.unique_workflows, 1);
        assert_eq!(stats.unique_users, 2); // analyst_1 and analyst_2
        assert!(stats.last_accessed.is_some());
        assert_eq!(stats.time_window_days, 30);
    }

    #[tokio::test]
    async fn test_get_file_lineage() {
        let lineage_storage = create_test_lineage_storage();
        let tracker = FileLineageTracker::new(lineage_storage.clone());

        let file_id = "lineage_file";

        // Track some usage
        tracker
            .track_file_usage(
                file_id,
                "/data/lineage.csv",
                FileUsageType::TransformInput {
                    transform_id: "transform_123".to_string(),
                },
            )
            .await
            .unwrap();

        // Get lineage
        let (upstream, downstream) = tracker.get_file_lineage(file_id).await.unwrap();

        // Should have upstream events (our tracked usage)
        assert_eq!(upstream.len(), 1);

        // Downstream is currently a placeholder (empty)
        assert_eq!(downstream.len(), 0);
    }

    #[tokio::test]
    async fn test_comprehensive_file_lifecycle_lineage() {
        let lineage_storage = create_test_lineage_storage();
        let tracker = FileLineageTracker::new(lineage_storage.clone());

        let file_id = "customer_data.csv";

        // Simulate a complete file lifecycle

        // 1. File uploaded and previewed
        tracker
            .track_file_usage(
                file_id,
                "/uploads/customer_data.csv",
                FileUsageType::Preview {
                    user_id: "uploader".to_string(),
                },
            )
            .await
            .unwrap();

        // 2. File used in workflow
        tracker
            .track_file_usage(
                file_id,
                "/uploads/customer_data.csv",
                FileUsageType::WorkflowRead {
                    workflow_id: "customer_enrichment".to_string(),
                    step_id: "load".to_string(),
                },
            )
            .await
            .unwrap();

        // 3. File used in transformation
        tracker
            .track_file_usage(
                file_id,
                "/uploads/customer_data.csv",
                FileUsageType::TransformInput {
                    transform_id: "standardize_names".to_string(),
                },
            )
            .await
            .unwrap();

        // 4. File downloaded by analyst
        tracker
            .track_file_usage(
                file_id,
                "/uploads/customer_data.csv",
                FileUsageType::Download {
                    user_id: "data_analyst".to_string(),
                },
            )
            .await
            .unwrap();

        // 5. File queried via API
        tracker
            .track_file_usage(
                file_id,
                "/uploads/customer_data.csv",
                FileUsageType::ApiQuery {
                    endpoint: "/api/v1/files/data".to_string(),
                    user_id: "api_consumer".to_string(),
                },
            )
            .await
            .unwrap();

        // Get comprehensive impact analysis
        let impact = tracker.get_file_impact(file_id).await.unwrap();
        assert_eq!(impact.recent_usage_count, 5);
        assert_eq!(impact.dependent_workflows.len(), 1);
        assert_eq!(impact.dependent_transforms.len(), 1);
        assert!(!impact.can_safely_delete);

        // Get usage statistics
        let stats = tracker.get_usage_stats(file_id, 30).await.unwrap();
        assert_eq!(stats.total_accesses, 5);
        assert_eq!(stats.unique_workflows, 1);
        assert_eq!(stats.unique_users, 3); // uploader, data_analyst, api_consumer
        assert!(stats.last_accessed.is_some());

        // Get lineage
        let (upstream, _) = tracker.get_file_lineage(file_id).await.unwrap();
        assert_eq!(upstream.len(), 5);
    }
}
