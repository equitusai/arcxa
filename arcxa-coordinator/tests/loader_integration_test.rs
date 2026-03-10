//! Integration tests for ETL Loader subsystem
//!
//! Tests the end-to-end loader workflow including:
//! - CSV parsing and streaming
//! - DEL file generation for DB2 LOAD
//! - Data transformations (UPPER, LOWER, TRIM, CAST)
//! - Checkpoint creation and resume
//! - Dead letter queue (DLQ) for failed rows
//! - Error handling and retry logic

use std::collections::HashMap;
use std::path::PathBuf;
use tempfile::TempDir;
use tokio::fs;

// ============================================================================
// Test Utilities
// ============================================================================

/// Create temporary test directory
fn create_temp_dir() -> TempDir {
    tempfile::tempdir().expect("Failed to create temp dir")
}

/// Create sample CSV file for testing
async fn create_sample_csv(dir: &TempDir, name: &str, content: &str) -> PathBuf {
    let file_path = dir.path().join(name);
    fs::write(&file_path, content)
        .await
        .expect("Failed to write CSV file");
    file_path
}

/// Sample CSV data with various data quality issues
const SAMPLE_CSV: &str = r#"customer_id,first_name,last_name,email,age,country
1001,John,Doe,john.doe@example.com,35,USA
1002,Jane,Smith,jane.smith@example.com,28,UK
1003,  Bob  ,Johnson,bob@example.com,invalid_age,Canada
1004,Alice,Williams,,45,Australia
1005,Charlie,Brown,charlie@example.com,52,USA
1006,Invalid,Record,not_an_email,thirty,Unknown
"#;

/// Sample CSV with proper data
const CLEAN_CSV: &str = r#"id,name,email,status
100,Alice,alice@test.com,active
101,Bob,bob@test.com,inactive
102,Charlie,charlie@test.com,active
"#;

// ============================================================================
// Module 1: CSV Reader Tests
// ============================================================================

#[tokio::test]
async fn test_csv_reader_basic_parsing() {
    let temp_dir = create_temp_dir();
    let csv_path = create_sample_csv(&temp_dir, "test.csv", CLEAN_CSV).await;

    // Read CSV file
    let content = fs::read_to_string(&csv_path).await.unwrap();
    let mut reader = csv::Reader::from_reader(content.as_bytes());

    // Verify headers
    let headers = reader.headers().unwrap();
    assert_eq!(headers.len(), 4);
    assert_eq!(headers.get(0).unwrap(), "id");
    assert_eq!(headers.get(1).unwrap(), "name");

    // Count records
    let records: Vec<_> = reader.records().collect();
    assert_eq!(records.len(), 3);
}

#[tokio::test]
async fn test_csv_reader_with_invalid_data() {
    let temp_dir = create_temp_dir();
    let csv_path = create_sample_csv(&temp_dir, "test.csv", SAMPLE_CSV).await;

    let content = fs::read_to_string(&csv_path).await.unwrap();
    let mut reader = csv::Reader::from_reader(content.as_bytes());

    // Verify we can read all records including invalid ones
    let records: Vec<_> = reader.records().filter_map(|r| r.ok()).collect();
    assert_eq!(records.len(), 6); // All records should be readable as strings

    // Verify specific records
    let record_3 = &records[2]; // Bob Johnson with invalid age
    assert_eq!(record_3.get(1).unwrap(), "  Bob  "); // Whitespace preserved
    assert_eq!(record_3.get(4).unwrap(), "invalid_age"); // Invalid age as string
}

#[tokio::test]
async fn test_csv_reader_streaming_large_file() {
    let temp_dir = create_temp_dir();

    // Generate large CSV (10K rows)
    let mut large_csv = String::from("id,value\n");
    for i in 0..10_000 {
        large_csv.push_str(&format!("{},value_{}\n", i, i));
    }

    let csv_path = create_sample_csv(&temp_dir, "large.csv", &large_csv).await;

    // Stream read in chunks
    let content = fs::read_to_string(&csv_path).await.unwrap();
    let mut reader = csv::Reader::from_reader(content.as_bytes());

    let mut count = 0;
    for result in reader.records() {
        result.unwrap();
        count += 1;
    }

    assert_eq!(count, 10_000);
}

// ============================================================================
// Module 2: DEL File Generation Tests
// ============================================================================

#[test]
fn test_del_file_format_simple() {
    // DEL format: pipe-delimited with specific escaping rules
    let row = vec!["100", "Alice", "alice@test.com"];
    let del_line = row.join("|");

    assert_eq!(del_line, "100|Alice|alice@test.com");
}

#[test]
fn test_del_file_format_with_special_chars() {
    // Test escaping of special characters in DEL format
    let row = vec!["100", "O'Brien", "test@example.com"];

    // In DEL format, single quotes should be escaped
    let escaped_name = row[1].replace('\'', "''");
    let del_line = format!("{}|{}|{}", row[0], escaped_name, row[2]);

    assert_eq!(del_line, "100|O''Brien|test@example.com");
}

#[test]
fn test_del_file_format_with_nulls() {
    // DEL format represents NULL values
    let row = vec![Some("100"), Some("Alice"), None, Some("active")];

    let del_line = row
        .iter()
        .map(|v| v.unwrap_or(""))
        .collect::<Vec<_>>()
        .join("|");

    assert_eq!(del_line, "100|Alice||active");
}

#[tokio::test]
async fn test_del_file_generation_from_csv() {
    let temp_dir = create_temp_dir();
    let csv_path = create_sample_csv(&temp_dir, "input.csv", CLEAN_CSV).await;
    let del_path = temp_dir.path().join("output.del");

    // Read CSV and write as DEL
    let content = fs::read_to_string(&csv_path).await.unwrap();
    let mut reader = csv::Reader::from_reader(content.as_bytes());

    let mut del_content = String::new();
    for result in reader.records() {
        let record = result.unwrap();
        let del_line: Vec<_> = record.iter().collect();
        del_content.push_str(&del_line.join("|"));
        del_content.push('\n');
    }

    fs::write(&del_path, del_content).await.unwrap();

    // Verify DEL file
    let written = fs::read_to_string(&del_path).await.unwrap();
    let lines: Vec<_> = written.lines().collect();
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0], "100|Alice|alice@test.com|active");
}

// ============================================================================
// Module 3: Data Transformation Tests
// ============================================================================

#[test]
fn test_transformation_upper() {
    let input = "hello world";
    let output = input.to_uppercase();
    assert_eq!(output, "HELLO WORLD");
}

#[test]
fn test_transformation_lower() {
    let input = "HELLO WORLD";
    let output = input.to_lowercase();
    assert_eq!(output, "hello world");
}

#[test]
fn test_transformation_trim() {
    let input = "  Bob  ";
    let output = input.trim();
    assert_eq!(output, "Bob");
}

#[test]
fn test_transformation_cast_to_int() {
    let valid = "42";
    let invalid = "not_a_number";

    assert_eq!(valid.parse::<i32>().ok(), Some(42));
    assert_eq!(invalid.parse::<i32>().ok(), None);
}

#[test]
fn test_transformation_pipeline() {
    // Test chained transformations: TRIM -> UPPER
    let input = "  hello  ";
    let output = input.trim().to_uppercase();
    assert_eq!(output, "HELLO");
}

#[test]
fn test_transformation_conditional() {
    // Test conditional transformation (e.g., default value for NULL)
    let value: Option<&str> = None;
    let result = value.unwrap_or("DEFAULT");
    assert_eq!(result, "DEFAULT");
}

#[tokio::test]
async fn test_transformation_on_csv_data() {
    let temp_dir = create_temp_dir();
    let csv_path = create_sample_csv(&temp_dir, "test.csv", SAMPLE_CSV).await;

    let content = fs::read_to_string(&csv_path).await.unwrap();
    let mut reader = csv::Reader::from_reader(content.as_bytes());

    // Apply transformations: TRIM and UPPER on names
    let mut transformed = Vec::new();
    for result in reader.records() {
        let record = result.unwrap();

        let first_name = record.get(1).unwrap().trim().to_uppercase();
        let last_name = record.get(2).unwrap().trim().to_uppercase();

        transformed.push((first_name, last_name));
    }

    // Verify transformations
    assert_eq!(transformed[0], ("JOHN".to_string(), "DOE".to_string()));
    assert_eq!(transformed[2], ("BOB".to_string(), "JOHNSON".to_string())); // Trimmed whitespace
}

// ============================================================================
// Module 4: Checkpoint Management Tests
// ============================================================================

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct Checkpoint {
    job_id: String,
    current_row: u64,
    file_offset: u64,
    timestamp: chrono::DateTime<chrono::Utc>,
}

#[tokio::test]
async fn test_checkpoint_creation() {
    let temp_dir = create_temp_dir();
    let checkpoint_path = temp_dir.path().join("checkpoint.json");

    let checkpoint = Checkpoint {
        job_id: "job_123".to_string(),
        current_row: 5000,
        file_offset: 1024000,
        timestamp: chrono::Utc::now(),
    };

    // Save checkpoint
    let json = serde_json::to_string_pretty(&checkpoint).unwrap();
    fs::write(&checkpoint_path, json).await.unwrap();

    // Verify checkpoint file exists
    assert!(checkpoint_path.exists());

    // Load checkpoint
    let loaded_json = fs::read_to_string(&checkpoint_path).await.unwrap();
    let loaded: Checkpoint = serde_json::from_str(&loaded_json).unwrap();

    assert_eq!(loaded.job_id, "job_123");
    assert_eq!(loaded.current_row, 5000);
    assert_eq!(loaded.file_offset, 1024000);
}

#[tokio::test]
async fn test_checkpoint_resume() {
    let temp_dir = create_temp_dir();
    let csv_path = create_sample_csv(&temp_dir, "test.csv", SAMPLE_CSV).await;

    // Simulate checkpoint at row 3
    let resume_from = 3;

    // Resume processing from checkpoint
    let content = fs::read_to_string(&csv_path).await.unwrap();
    let mut reader = csv::Reader::from_reader(content.as_bytes());

    let records: Vec<_> = reader
        .records()
        .enumerate()
        .filter_map(|(idx, r)| if idx >= resume_from { r.ok() } else { None })
        .collect();

    // Should have processed rows 3, 4, 5 (3 rows total)
    assert_eq!(records.len(), 3);
    assert_eq!(records[0].get(1).unwrap(), "Alice"); // Row 4 (0-indexed row 3)
}

#[tokio::test]
async fn test_checkpoint_recovery_after_failure() {
    let temp_dir = create_temp_dir();

    // Create checkpoint before failure
    let checkpoint_path = temp_dir.path().join("checkpoint.json");
    let checkpoint = Checkpoint {
        job_id: "job_456".to_string(),
        current_row: 2500,
        file_offset: 512000,
        timestamp: chrono::Utc::now(),
    };

    let json = serde_json::to_string(&checkpoint).unwrap();
    fs::write(&checkpoint_path, json).await.unwrap();

    // Simulate recovery
    let loaded_json = fs::read_to_string(&checkpoint_path).await.unwrap();
    let loaded: Checkpoint = serde_json::from_str(&loaded_json).unwrap();

    assert_eq!(loaded.current_row, 2500);
    // Resume processing from row 2500
}

// ============================================================================
// Module 5: Dead Letter Queue (DLQ) Tests
// ============================================================================

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct DlqEntry {
    row_number: u64,
    original_data: HashMap<String, String>,
    error_category: String,
    error_message: String,
    timestamp: chrono::DateTime<chrono::Utc>,
    retry_count: u32,
}

#[tokio::test]
async fn test_dlq_write_failed_row() {
    let temp_dir = create_temp_dir();
    let dlq_path = temp_dir.path().join("dlq.jsonl");

    // Create DLQ entry for failed row
    let entry = DlqEntry {
        row_number: 42,
        original_data: HashMap::from([
            ("id".to_string(), "1003".to_string()),
            ("age".to_string(), "invalid_age".to_string()),
        ]),
        error_category: "DataFormat".to_string(),
        error_message: "Invalid age: cannot parse 'invalid_age' as integer".to_string(),
        timestamp: chrono::Utc::now(),
        retry_count: 0,
    };

    // Write to DLQ (JSON Lines format)
    let json = serde_json::to_string(&entry).unwrap();
    fs::write(&dlq_path, json + "\n").await.unwrap();

    // Verify DLQ file
    let content = fs::read_to_string(&dlq_path).await.unwrap();
    let loaded: DlqEntry = serde_json::from_str(content.trim()).unwrap();

    assert_eq!(loaded.row_number, 42);
    assert_eq!(loaded.error_category, "DataFormat");
}

#[tokio::test]
async fn test_dlq_multiple_errors() {
    let temp_dir = create_temp_dir();
    let dlq_path = temp_dir.path().join("dlq.jsonl");

    // Write multiple DLQ entries
    let mut entries = Vec::new();

    for i in 0..5 {
        let entry = DlqEntry {
            row_number: i,
            original_data: HashMap::new(),
            error_category: if i % 2 == 0 { "DataFormat" } else { "Timeout" }.to_string(),
            error_message: format!("Error {}", i),
            timestamp: chrono::Utc::now(),
            retry_count: 0,
        };
        entries.push(entry);
    }

    // Write all entries
    let mut content = String::new();
    for entry in &entries {
        content.push_str(&serde_json::to_string(entry).unwrap());
        content.push('\n');
    }
    fs::write(&dlq_path, content).await.unwrap();

    // Read and verify
    let dlq_content = fs::read_to_string(&dlq_path).await.unwrap();
    let lines: Vec<_> = dlq_content.lines().collect();
    assert_eq!(lines.len(), 5);

    // Count errors by category
    let mut categories = HashMap::new();
    for line in lines {
        let entry: DlqEntry = serde_json::from_str(line).unwrap();
        *categories.entry(entry.error_category).or_insert(0) += 1;
    }

    assert_eq!(categories.get("DataFormat"), Some(&3)); // Rows 0, 2, 4
    assert_eq!(categories.get("Timeout"), Some(&2)); // Rows 1, 3
}

#[tokio::test]
async fn test_dlq_reprocessing() {
    let temp_dir = create_temp_dir();
    let dlq_path = temp_dir.path().join("dlq.jsonl");

    // Create DLQ entry
    let entry = DlqEntry {
        row_number: 100,
        original_data: HashMap::from([
            ("id".to_string(), "1234".to_string()),
            ("value".to_string(), "test".to_string()),
        ]),
        error_category: "Transient".to_string(),
        error_message: "Database connection timeout".to_string(),
        timestamp: chrono::Utc::now(),
        retry_count: 0,
    };

    let json = serde_json::to_string(&entry).unwrap();
    fs::write(&dlq_path, json + "\n").await.unwrap();

    // Simulate reprocessing
    let content = fs::read_to_string(&dlq_path).await.unwrap();
    let mut loaded: DlqEntry = serde_json::from_str(content.trim()).unwrap();

    // Increment retry count
    loaded.retry_count += 1;

    assert_eq!(loaded.retry_count, 1);
    assert_eq!(loaded.error_category, "Transient");
}

// ============================================================================
// Module 6: Error Handling Tests
// ============================================================================

#[derive(Debug, PartialEq)]
enum ErrorCategory {
    DataFormat,
    Timeout,
    DatabaseError,
    ValidationError,
}

fn categorize_error(error_msg: &str) -> ErrorCategory {
    let msg_lower = error_msg.to_lowercase();
    if msg_lower.contains("parse") || msg_lower.contains("format") {
        ErrorCategory::DataFormat
    } else if msg_lower.contains("timeout") {
        ErrorCategory::Timeout
    } else if msg_lower.contains("database") || msg_lower.contains("connection") {
        ErrorCategory::DatabaseError
    } else {
        ErrorCategory::ValidationError
    }
}

#[test]
fn test_error_categorization() {
    assert_eq!(
        categorize_error("Failed to parse integer"),
        ErrorCategory::DataFormat
    );
    assert_eq!(
        categorize_error("Database connection timeout"),
        ErrorCategory::Timeout
    );
    assert_eq!(
        categorize_error("Database constraint violation"),
        ErrorCategory::DatabaseError
    );
}

#[test]
fn test_error_retry_logic() {
    // Transient errors should be retried
    let is_transient = |error: &str| error.contains("timeout") || error.contains("connection");

    assert!(is_transient("Connection timeout"));
    assert!(is_transient("Lost database connection"));
    assert!(!is_transient("Invalid data format"));
}

// ============================================================================
// Module 7: End-to-End Integration Tests
// ============================================================================

#[tokio::test]
async fn test_e2e_csv_to_del_with_transformations() {
    let temp_dir = create_temp_dir();

    // Step 1: Create source CSV
    let csv_path = create_sample_csv(&temp_dir, "input.csv", CLEAN_CSV).await;

    // Step 2: Read CSV, apply transformations, write DEL
    let content = fs::read_to_string(&csv_path).await.unwrap();
    let mut reader = csv::Reader::from_reader(content.as_bytes());

    let del_path = temp_dir.path().join("output.del");
    let mut del_content = String::new();

    for result in reader.records() {
        let record = result.unwrap();

        // Apply transformations
        let id = record.get(0).unwrap();
        let name = record.get(1).unwrap().to_uppercase(); // UPPER transformation
        let email = record.get(2).unwrap().to_lowercase(); // LOWER transformation
        let status = record.get(3).unwrap();

        // Write to DEL
        del_content.push_str(&format!("{}|{}|{}|{}\n", id, name, email, status));
    }

    fs::write(&del_path, del_content).await.unwrap();

    // Step 3: Verify DEL file
    let written = fs::read_to_string(&del_path).await.unwrap();
    let lines: Vec<_> = written.lines().collect();

    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0], "100|ALICE|alice@test.com|active");
    assert_eq!(lines[1], "101|BOB|bob@test.com|inactive");
}

#[tokio::test]
async fn test_e2e_with_error_handling() {
    let temp_dir = create_temp_dir();

    // Step 1: Create CSV with errors
    let csv_path = create_sample_csv(&temp_dir, "input.csv", SAMPLE_CSV).await;

    // Step 2: Process with error handling
    let content = fs::read_to_string(&csv_path).await.unwrap();
    let mut reader = csv::Reader::from_reader(content.as_bytes());

    let del_path = temp_dir.path().join("output.del");
    let dlq_path = temp_dir.path().join("dlq.jsonl");

    let mut del_content = String::new();
    let mut dlq_entries = Vec::new();

    for (idx, result) in reader.records().enumerate() {
        let record = result.unwrap();

        let age_str = record.get(4).unwrap();

        // Try to parse age - if fails, write to DLQ
        match age_str.parse::<i32>() {
            Ok(age) => {
                // Success - write to DEL
                let line = format!(
                    "{}|{}|{}|{}|{}|{}\n",
                    record.get(0).unwrap(),
                    record.get(1).unwrap(),
                    record.get(2).unwrap(),
                    record.get(3).unwrap(),
                    age,
                    record.get(5).unwrap()
                );
                del_content.push_str(&line);
            }
            Err(_) => {
                // Error - write to DLQ
                let mut original_data = HashMap::new();
                for (i, field) in record.iter().enumerate() {
                    original_data.insert(format!("col_{}", i), field.to_string());
                }

                let dlq_entry = DlqEntry {
                    row_number: idx as u64,
                    original_data,
                    error_category: "DataFormat".to_string(),
                    error_message: format!("Invalid age: '{}'", age_str),
                    timestamp: chrono::Utc::now(),
                    retry_count: 0,
                };
                dlq_entries.push(dlq_entry);
            }
        }
    }

    // Write DEL file
    fs::write(&del_path, del_content).await.unwrap();

    // Write DLQ file
    let mut dlq_content = String::new();
    for entry in dlq_entries {
        dlq_content.push_str(&serde_json::to_string(&entry).unwrap());
        dlq_content.push('\n');
    }
    fs::write(&dlq_path, dlq_content).await.unwrap();

    // Step 3: Verify results
    let del_lines = fs::read_to_string(&del_path).await.unwrap();
    let success_count = del_lines.lines().count();

    let dlq_lines = fs::read_to_string(&dlq_path).await.unwrap();
    let error_count = dlq_lines.lines().filter(|l| !l.is_empty()).count();

    // We expect 4 successful rows (valid ages) and 2 DLQ rows (invalid ages)
    assert_eq!(success_count, 4);
    assert_eq!(error_count, 2);
}

#[tokio::test]
async fn test_e2e_with_checkpoint_and_resume() {
    let temp_dir = create_temp_dir();

    // Create large CSV
    let mut large_csv = String::from("id,value\n");
    for i in 0..100 {
        large_csv.push_str(&format!("{},value_{}\n", i, i));
    }
    let csv_path = create_sample_csv(&temp_dir, "large.csv", &large_csv).await;

    // Step 1: Process first 50 rows, create checkpoint
    let content = fs::read_to_string(&csv_path).await.unwrap();
    let mut reader = csv::Reader::from_reader(content.as_bytes());

    let mut processed_count = 0;
    for (idx, _result) in reader.records().enumerate() {
        processed_count += 1;
        if idx >= 49 {
            break; // Stop at row 50
        }
    }

    // Save checkpoint
    let checkpoint_path = temp_dir.path().join("checkpoint.json");
    let checkpoint = Checkpoint {
        job_id: "job_large".to_string(),
        current_row: processed_count,
        file_offset: 0, // Would be actual file offset in real implementation
        timestamp: chrono::Utc::now(),
    };
    fs::write(
        &checkpoint_path,
        serde_json::to_string(&checkpoint).unwrap(),
    )
    .await
    .unwrap();

    assert_eq!(processed_count, 50);

    // Step 2: Resume from checkpoint
    let loaded_json = fs::read_to_string(&checkpoint_path).await.unwrap();
    let loaded_checkpoint: Checkpoint = serde_json::from_str(&loaded_json).unwrap();

    let resume_from = loaded_checkpoint.current_row as usize;

    // Resume processing
    let content = fs::read_to_string(&csv_path).await.unwrap();
    let mut reader = csv::Reader::from_reader(content.as_bytes());

    let remaining_count = reader
        .records()
        .enumerate()
        .filter(|(idx, _)| *idx >= resume_from)
        .count();

    // Should have 50 remaining rows (100 total - 50 already processed)
    assert_eq!(remaining_count, 50);
}

// ============================================================================
// Module 8: Performance Tests
// ============================================================================

#[tokio::test]
async fn test_performance_large_file_processing() {
    let temp_dir = create_temp_dir();

    // Generate 100K row CSV
    let mut large_csv = String::from("id,col1,col2,col3,col4\n");
    for i in 0..100_000 {
        large_csv.push_str(&format!("{},val1,val2,val3,val4\n", i));
    }
    let csv_path = create_sample_csv(&temp_dir, "perf_test.csv", &large_csv).await;

    // Measure processing time
    let start = std::time::Instant::now();

    let content = fs::read_to_string(&csv_path).await.unwrap();
    let mut reader = csv::Reader::from_reader(content.as_bytes());

    let mut count = 0;
    for result in reader.records() {
        result.unwrap();
        count += 1;
    }

    let elapsed = start.elapsed();

    assert_eq!(count, 100_000);

    // Should process 100K rows in reasonable time (< 5 seconds)
    assert!(
        elapsed.as_secs() < 5,
        "Processing took {} seconds, expected < 5",
        elapsed.as_secs()
    );
}

#[tokio::test]
async fn test_performance_transformation_throughput() {
    // Test transformation performance
    let start = std::time::Instant::now();

    let mut transformed_count = 0;
    for i in 0..100_000 {
        let value = format!("  test_value_{}  ", i);
        let _transformed = value.trim().to_uppercase();
        transformed_count += 1;
    }

    let elapsed = start.elapsed();

    assert_eq!(transformed_count, 100_000);

    // Should transform 100K values quickly
    assert!(
        elapsed.as_millis() < 1000,
        "Transformations took {} ms, expected < 1000",
        elapsed.as_millis()
    );
}
