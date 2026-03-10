//! Integration Tests for Profiling Module
//!
//! Comprehensive test suite for source profiling functionality.

use graphica_coordinator::mapping::profiling::{ProfileConfig, SourceProfiler};
use std::io::Write;
use tempfile::NamedTempFile;

/// Test profiling a simple CSV file
#[tokio::test]
async fn test_profile_simple_csv() {
    // Create test CSV
    let mut csv_file = NamedTempFile::new().unwrap();
    writeln!(csv_file, "id,name,age").unwrap();
    writeln!(csv_file, "1,Alice,30").unwrap();
    writeln!(csv_file, "2,Bob,25").unwrap();
    writeln!(csv_file, "3,Charlie,35").unwrap();
    csv_file.flush().unwrap();

    // Profile the file
    let profiler = SourceProfiler::new(ProfileConfig::default());
    let profile = profiler.profile_csv(csv_file.path()).await.unwrap();

    // Verify basic properties
    assert_eq!(profile.column_count, 3);
    assert_eq!(profile.rows_profiled, 3);
    assert_eq!(profile.format, "csv");

    // Verify columns were detected
    assert_eq!(profile.columns.len(), 3);
    assert!(profile.columns.iter().any(|c| c.name == "id"));
    assert!(profile.columns.iter().any(|c| c.name == "name"));
    assert!(profile.columns.iter().any(|c| c.name == "age"));
}

/// Test type inference
#[tokio::test]
async fn test_type_inference() {
    let mut csv_file = NamedTempFile::new().unwrap();
    writeln!(csv_file, "int_col,float_col,string_col,bool_col,date_col").unwrap();
    writeln!(csv_file, "100,3.14,hello,true,2023-01-15").unwrap();
    writeln!(csv_file, "200,2.71,world,false,2023-02-20").unwrap();
    writeln!(csv_file, "300,1.41,test,true,2023-03-10").unwrap();
    csv_file.flush().unwrap();

    let profiler = SourceProfiler::new(ProfileConfig::default());
    let profile = profiler.profile_csv(csv_file.path()).await.unwrap();

    // Check inferred types
    let int_col = profile
        .columns
        .iter()
        .find(|c| c.name == "int_col")
        .unwrap();
    assert_eq!(
        int_col.data_type,
        graphica_coordinator::mapping::profiling::types::DataType::Integer
    );

    let float_col = profile
        .columns
        .iter()
        .find(|c| c.name == "float_col")
        .unwrap();
    assert_eq!(
        float_col.data_type,
        graphica_coordinator::mapping::profiling::types::DataType::Float
    );

    let string_col = profile
        .columns
        .iter()
        .find(|c| c.name == "string_col")
        .unwrap();
    assert_eq!(
        string_col.data_type,
        graphica_coordinator::mapping::profiling::types::DataType::String
    );

    let bool_col = profile
        .columns
        .iter()
        .find(|c| c.name == "bool_col")
        .unwrap();
    assert_eq!(
        bool_col.data_type,
        graphica_coordinator::mapping::profiling::types::DataType::Boolean
    );

    let date_col = profile
        .columns
        .iter()
        .find(|c| c.name == "date_col")
        .unwrap();
    assert_eq!(
        date_col.data_type,
        graphica_coordinator::mapping::profiling::types::DataType::Date
    );
}

/// Test null detection
#[tokio::test]
async fn test_null_detection() {
    let mut csv_file = NamedTempFile::new().unwrap();
    writeln!(csv_file, "col1,col2,col3").unwrap();
    writeln!(csv_file, "1,value,data").unwrap();
    writeln!(csv_file, "2,,data").unwrap(); // col2 null
    writeln!(csv_file, "3,null,data").unwrap(); // col2 null (string "null")
    writeln!(csv_file, "4,value,").unwrap(); // col3 empty
    csv_file.flush().unwrap();

    let profiler = SourceProfiler::new(ProfileConfig::default());
    let profile = profiler.profile_csv(csv_file.path()).await.unwrap();

    let col2 = profile.columns.iter().find(|c| c.name == "col2").unwrap();
    assert_eq!(col2.null_count, 2); // Empty and "null" string

    let col3 = profile.columns.iter().find(|c| c.name == "col3").unwrap();
    assert_eq!(col3.null_count, 1); // Empty value
}

/// Test distinct count and cardinality
#[tokio::test]
async fn test_distinct_count() {
    let mut csv_file = NamedTempFile::new().unwrap();
    writeln!(csv_file, "unique_col,duplicate_col").unwrap();
    writeln!(csv_file, "1,A").unwrap();
    writeln!(csv_file, "2,A").unwrap();
    writeln!(csv_file, "3,B").unwrap();
    writeln!(csv_file, "4,B").unwrap();
    writeln!(csv_file, "5,C").unwrap();
    csv_file.flush().unwrap();

    let profiler = SourceProfiler::new(ProfileConfig::default());
    let profile = profiler.profile_csv(csv_file.path()).await.unwrap();

    let unique_col = profile
        .columns
        .iter()
        .find(|c| c.name == "unique_col")
        .unwrap();
    assert_eq!(unique_col.distinct_count, 5);
    assert_eq!(unique_col.cardinality, 1.0); // 5/5 = 100% unique

    let duplicate_col = profile
        .columns
        .iter()
        .find(|c| c.name == "duplicate_col")
        .unwrap();
    assert_eq!(duplicate_col.distinct_count, 3);
    assert_eq!(duplicate_col.cardinality, 0.6); // 3/5 = 60% unique
}

/// Test candidate key detection
#[tokio::test]
async fn test_candidate_key_detection() {
    let mut csv_file = NamedTempFile::new().unwrap();
    writeln!(csv_file, "id,email,category").unwrap();
    writeln!(csv_file, "1,alice@example.com,A").unwrap();
    writeln!(csv_file, "2,bob@example.com,B").unwrap();
    writeln!(csv_file, "3,charlie@example.com,A").unwrap();
    writeln!(csv_file, "4,diana@example.com,B").unwrap();
    writeln!(csv_file, "5,eve@example.com,C").unwrap();
    csv_file.flush().unwrap();

    let profiler = SourceProfiler::new(ProfileConfig {
        candidate_key_threshold: 0.95,
        ..Default::default()
    });
    let profile = profiler.profile_csv(csv_file.path()).await.unwrap();

    // id and email should be candidate keys (100% unique, 0% null)
    assert!(profile.candidate_keys.contains(&"id".to_string()));
    assert!(profile.candidate_keys.contains(&"email".to_string()));

    // category should NOT be a candidate key (only 60% unique)
    assert!(!profile.candidate_keys.contains(&"category".to_string()));
}

/// Test sampling (only profile first N rows)
#[tokio::test]
async fn test_sampling() {
    let mut csv_file = NamedTempFile::new().unwrap();
    writeln!(csv_file, "id,name").unwrap();
    for i in 1..=1000 {
        writeln!(csv_file, "{},Name{}", i, i).unwrap();
    }
    csv_file.flush().unwrap();

    let profiler = SourceProfiler::new(ProfileConfig {
        sample_size: Some(100), // Only sample first 100 rows
        ..Default::default()
    });
    let profile = profiler.profile_csv(csv_file.path()).await.unwrap();

    assert_eq!(profile.rows_profiled, 100); // Should only profile 100 rows
    assert_eq!(profile.total_rows, None); // Total unknown without full scan
}

/// Test numeric statistics (mean, median, stddev)
#[tokio::test]
async fn test_numeric_statistics() {
    let mut csv_file = NamedTempFile::new().unwrap();
    writeln!(csv_file, "scores").unwrap();
    writeln!(csv_file, "10").unwrap();
    writeln!(csv_file, "20").unwrap();
    writeln!(csv_file, "30").unwrap();
    writeln!(csv_file, "40").unwrap();
    writeln!(csv_file, "50").unwrap();
    csv_file.flush().unwrap();

    let profiler = SourceProfiler::new(ProfileConfig::default());
    let profile = profiler.profile_csv(csv_file.path()).await.unwrap();

    let scores = profile.columns.iter().find(|c| c.name == "scores").unwrap();
    assert_eq!(scores.mean, Some(30.0));
    assert_eq!(scores.median, Some(30.0));
    assert!(scores.std_dev.is_some());
    assert_eq!(scores.min_value, Some("10".to_string()));
    assert_eq!(scores.max_value, Some("50".to_string()));
}

/// Test RDF serialization
#[tokio::test]
async fn test_rdf_serialization() {
    let mut csv_file = NamedTempFile::new().unwrap();
    writeln!(csv_file, "id,name").unwrap();
    writeln!(csv_file, "1,Alice").unwrap();
    writeln!(csv_file, "2,Bob").unwrap();
    csv_file.flush().unwrap();

    let profiler = SourceProfiler::new(ProfileConfig::default());
    let profile = profiler.profile_csv(csv_file.path()).await.unwrap();
    let dataset_uri = profiler.generate_dataset_uri(csv_file.path());

    let rdf_turtle = profiler.profile_to_rdf(&profile, &dataset_uri).unwrap();

    // Verify RDF contains key elements
    assert!(rdf_turtle.contains("@prefix dcat:"));
    assert!(rdf_turtle.contains("@prefix void:"));
    assert!(rdf_turtle.contains("@prefix gph:"));
    assert!(rdf_turtle.contains("dcat:Dataset"));
    assert!(rdf_turtle.contains("gph:hasColumn"));
    assert!(rdf_turtle.contains("void:distinctValues"));
}
