//! Phase 2 Integration Tests - Database Loader Integration
//!
//! Tests end-to-end workflows for:
//! - DatabaseQueryReader (PostgreSQL query execution)
//! - DataLoader (DataSourceReader → DatabaseLoader bridge)
//! - Mixed-source batch jobs (CSV + database queries)
//! - Transaction coordination with database loads
//!
//! NOTE: These tests require a PostgreSQL instance for full E2E testing.
//! Mock tests are provided for CI/CD pipelines without database access.

use graphica_coordinator::workflows::domain::{
    create_reader, DataSource, DatabaseConnectionConfig, DatabaseType, SourceMetadata,
};
use graphica_coordinator::workflows::engine::{DataLoader, LoadConfig};
use std::collections::HashMap;
use std::path::PathBuf;
use tempfile::TempDir;
use tokio::fs;

// ============================================================================
// Test Utilities
// ============================================================================

/// Check if PostgreSQL is available for testing
async fn is_postgres_available() -> bool {
    std::env::var("TEST_POSTGRES_HOST").is_ok()
}

/// Get PostgreSQL connection config from environment
fn get_test_postgres_config() -> DatabaseConnectionConfig {
    DatabaseConnectionConfig {
        host: std::env::var("TEST_POSTGRES_HOST").unwrap_or_else(|_| "localhost".to_string()),
        port: std::env::var("TEST_POSTGRES_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(5432),
        database: std::env::var("TEST_POSTGRES_DB").unwrap_or_else(|_| "graphica_test".to_string()),
        username: std::env::var("TEST_POSTGRES_USER").unwrap_or_else(|_| "postgres".to_string()),
        password: std::env::var("TEST_POSTGRES_PASSWORD")
            .unwrap_or_else(|_| "postgres".to_string()),
        ssl_mode: Some("disable".to_string()),
        extra_params: HashMap::new(),
    }
}

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

/// Sample CSV data for customer loading
const CUSTOMERS_CSV: &str = r#"customer_id,first_name,last_name,email,country
1001,John,Doe,john.doe@example.com,USA
1002,Jane,Smith,jane.smith@example.com,UK
1003,Bob,Johnson,bob@example.com,Canada
1004,Alice,Williams,alice@example.com,Australia
1005,Charlie,Brown,charlie@example.com,USA
"#;

/// Sample CSV data for products
const PRODUCTS_CSV: &str = r#"product_id,name,category,price
101,Widget A,Electronics,29.99
102,Widget B,Home,19.99
103,Widget C,Electronics,49.99
"#;

// ============================================================================
// Module 1: DataSource Creation Tests
// ============================================================================

#[test]
fn test_create_csv_data_source() {
    let source = DataSource::CsvFile {
        file_id: "customers_csv".to_string(),
        file_path: PathBuf::from("/data/customers.csv"),
        encoding: Some("UTF-8".to_string()),
        delimiter: Some(','),
        has_header: true,
    };

    match source {
        DataSource::CsvFile {
            file_id,
            has_header,
            ..
        } => {
            assert_eq!(file_id, "customers_csv");
            assert!(has_header);
        }
        _ => panic!("Expected CsvFile variant"),
    }
}

#[test]
fn test_create_database_query_source() {
    let config = DatabaseConnectionConfig {
        host: "localhost".to_string(),
        port: 5432,
        database: "test_db".to_string(),
        username: "test_user".to_string(),
        password: "test_pass".to_string(),
        ssl_mode: Some("require".to_string()),
        extra_params: HashMap::new(),
    };

    let source = DataSource::DatabaseQuery {
        datasource_id: "postgres_source".to_string(),
        database_type: DatabaseType::Postgres,
        connection_config: config.clone(),
        query: "SELECT * FROM customers".to_string(),
        fetch_size: 10000,
    };

    match source {
        DataSource::DatabaseQuery {
            database_type,
            query,
            fetch_size,
            ..
        } => {
            assert_eq!(database_type, DatabaseType::Postgres);
            assert_eq!(query, "SELECT * FROM customers");
            assert_eq!(fetch_size, 10000);
        }
        _ => panic!("Expected DatabaseQuery variant"),
    }
}

#[test]
fn test_database_connection_config_serialization() {
    let config = DatabaseConnectionConfig {
        host: "prod.example.com".to_string(),
        port: 5432,
        database: "analytics".to_string(),
        username: "etl_user".to_string(),
        password: "secret123".to_string(),
        ssl_mode: Some("require".to_string()),
        extra_params: HashMap::from([
            ("connect_timeout".to_string(), "30".to_string()),
            ("application_name".to_string(), "graphica".to_string()),
        ]),
    };

    // Test serialization
    let json = serde_json::to_string(&config).expect("Failed to serialize");
    assert!(json.contains("prod.example.com"));
    assert!(json.contains("analytics"));

    // Note: password field is skip_serializing, so it won't be in JSON
    assert!(
        !json.contains("secret123"),
        "Password should not be serialized"
    );

    // Deserialization requires password field to be added manually
    // This test verifies serialization works; full round-trip requires password injection
    assert_eq!(config.host, "prod.example.com");
    assert_eq!(config.extra_params.len(), 2);
}

// ============================================================================
// Module 2: CsvFileReader Tests
// ============================================================================

#[tokio::test]
async fn test_csv_reader_metadata() {
    let temp_dir = create_temp_dir();
    let csv_path = create_sample_csv(&temp_dir, "customers.csv", CUSTOMERS_CSV).await;

    let source = DataSource::CsvFile {
        file_id: "test_csv".to_string(),
        file_path: csv_path.clone(),
        encoding: Some("UTF-8".to_string()),
        delimiter: Some(','),
        has_header: true,
    };

    let reader = create_reader(source).expect("Failed to create reader");
    let metadata = reader.metadata().await.expect("Failed to get metadata");

    assert_eq!(metadata.source_identifier, "test_csv");
    assert!(metadata.estimated_rows.is_none()); // CSV doesn't pre-calculate rows
}

#[tokio::test]
async fn test_csv_reader_streaming() {
    use futures::StreamExt;

    let temp_dir = create_temp_dir();
    let csv_path = create_sample_csv(&temp_dir, "customers.csv", CUSTOMERS_CSV).await;

    let source = DataSource::CsvFile {
        file_id: "test_csv".to_string(),
        file_path: csv_path,
        encoding: Some("UTF-8".to_string()),
        delimiter: Some(','),
        has_header: true,
    };

    let mut reader = create_reader(source).expect("Failed to create reader");
    let mut stream = reader.read().await.expect("Failed to read stream");

    let mut row_count = 0;
    let mut first_customer_id = None;

    while let Some(row_result) = stream.next().await {
        let row = row_result.expect("Failed to read row");
        row_count += 1;

        if row_count == 1 {
            // Check first row
            first_customer_id = row.get("customer_id").and_then(|v| {
                if let serde_json::Value::String(s) = v {
                    Some(s.clone())
                } else {
                    None
                }
            });
        }
    }

    assert_eq!(row_count, 5); // 5 customers in CUSTOMERS_CSV
    assert_eq!(first_customer_id, Some("1001".to_string()));
}

#[tokio::test]
async fn test_csv_reader_empty_file() {
    let temp_dir = create_temp_dir();
    let csv_path = create_sample_csv(&temp_dir, "empty.csv", "id,name\n").await;

    let source = DataSource::CsvFile {
        file_id: "empty".to_string(),
        file_path: csv_path,
        encoding: Some("UTF-8".to_string()),
        delimiter: Some(','),
        has_header: true,
    };

    let mut reader = create_reader(source).expect("Failed to create reader");
    let mut stream = reader.read().await.expect("Failed to read stream");

    use futures::StreamExt;
    let row_count = stream.fold(0, |acc, _| async move { acc + 1 }).await;

    assert_eq!(row_count, 0); // No data rows
}

// ============================================================================
// Module 3: DatabaseQueryReader Tests (Mock)
// ============================================================================

#[test]
fn test_database_query_reader_creation() {
    let config = DatabaseConnectionConfig {
        host: "localhost".to_string(),
        port: 5432,
        database: "test".to_string(),
        username: "user".to_string(),
        password: "pass".to_string(),
        ssl_mode: Some("disable".to_string()),
        extra_params: HashMap::new(),
    };

    let source = DataSource::DatabaseQuery {
        datasource_id: "test_source".to_string(),
        database_type: DatabaseType::Postgres,
        connection_config: config,
        query: "SELECT * FROM orders".to_string(),
        fetch_size: 5000,
    };

    let reader_result = create_reader(source);
    assert!(
        reader_result.is_ok(),
        "Failed to create DatabaseQueryReader"
    );
}

#[tokio::test]
async fn test_database_query_reader_metadata() {
    let config = get_test_postgres_config();

    let source = DataSource::DatabaseQuery {
        datasource_id: "analytics_db".to_string(),
        database_type: DatabaseType::Postgres,
        connection_config: config,
        query: "SELECT customer_id, order_total FROM orders WHERE status = 'active'".to_string(),
        fetch_size: 10000,
    };

    let reader = create_reader(source).expect("Failed to create reader");
    let metadata = reader.metadata().await.expect("Failed to get metadata");

    assert_eq!(metadata.source_identifier, "analytics_db");
    assert!(metadata.estimated_rows.is_none()); // Query results size unknown until execution
}

// ============================================================================
// Module 4: DatabaseQueryReader Tests (Real PostgreSQL)
// ============================================================================

#[tokio::test]
#[ignore] // Run with: cargo test --ignored test_postgres_query_execution
async fn test_postgres_query_execution() {
    if !is_postgres_available().await {
        eprintln!("Skipping test: PostgreSQL not available");
        eprintln!(
            "Set TEST_POSTGRES_HOST, TEST_POSTGRES_DB, TEST_POSTGRES_USER, TEST_POSTGRES_PASSWORD"
        );
        return;
    }

    let config = get_test_postgres_config();

    // Create test table and data
    let setup_query = r#"
        DROP TABLE IF EXISTS test_customers;
        CREATE TABLE test_customers (
            id INTEGER PRIMARY KEY,
            name VARCHAR(100),
            email VARCHAR(100),
            age INTEGER
        );
        INSERT INTO test_customers VALUES
            (1, 'Alice', 'alice@test.com', 30),
            (2, 'Bob', 'bob@test.com', 25),
            (3, 'Charlie', 'charlie@test.com', 35);
    "#;

    // Execute setup (requires direct PostgreSQL connection)
    use tokio_postgres::NoTls;
    let ssl_mode_str = config.ssl_mode.as_deref().unwrap_or("disable");
    let connection_string = format!(
        "host={} port={} dbname={} user={} password={} sslmode={}",
        config.host, config.port, config.database, config.username, config.password, ssl_mode_str
    );
    let (client, connection) = tokio_postgres::connect(&connection_string, NoTls)
        .await
        .expect("Failed to connect to PostgreSQL");

    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("Connection error: {}", e);
        }
    });

    client
        .batch_execute(setup_query)
        .await
        .expect("Failed to set up test data");

    // Test DatabaseQueryReader
    let source = DataSource::DatabaseQuery {
        datasource_id: "test_db".to_string(),
        database_type: DatabaseType::Postgres,
        connection_config: config,
        query: "SELECT id, name, email, age FROM test_customers WHERE age > 25 ORDER BY id"
            .to_string(),
        fetch_size: 100,
    };

    let mut reader = create_reader(source).expect("Failed to create reader");
    let mut stream = reader.read().await.expect("Failed to read stream");

    use futures::StreamExt;
    let mut rows = Vec::new();
    while let Some(row_result) = stream.next().await {
        let row = row_result.expect("Failed to read row");
        rows.push(row);
    }

    assert_eq!(rows.len(), 2); // Alice (30) and Charlie (35)

    // Verify first row
    let first_row = &rows[0];
    assert_eq!(
        first_row.get("name"),
        Some(&serde_json::Value::String("Alice".to_string()))
    );
    assert_eq!(
        first_row.get("age"),
        Some(&serde_json::Value::Number(30.into()))
    );

    // Cleanup
    client
        .execute("DROP TABLE test_customers", &[])
        .await
        .expect("Failed to cleanup");
}

// ============================================================================
// Module 5: DataLoader Tests (Mock)
// ============================================================================

#[test]
fn test_load_config_defaults() {
    use graphica_coordinator::etl::loaders::database::LoadMode;

    let config = LoadConfig {
        table_name: "customers".to_string(),
        load_mode: LoadMode::Insert,
        key_fields: None,
        batch_size: 10000,
        max_errors: Some(100),
    };

    assert_eq!(config.table_name, "customers");
    assert_eq!(config.batch_size, 10000);
    assert_eq!(config.max_errors, Some(100));
    assert!(config.key_fields.is_none());
}

#[test]
fn test_data_loader_creation() {
    use graphica_coordinator::etl::loaders::database::LoadMode;

    let db_config = get_test_postgres_config();

    let load_config = LoadConfig {
        table_name: "test_table".to_string(),
        load_mode: LoadMode::Insert,
        key_fields: None,
        batch_size: 5000,
        max_errors: Some(50),
    };

    let _loader = DataLoader::new(DatabaseType::Postgres, db_config, load_config);

    // Just verify creation succeeds
    assert!(true, "DataLoader created successfully");
}

// ============================================================================
// Module 6: DataLoader Integration Tests (Real PostgreSQL)
// ============================================================================

#[tokio::test]
#[ignore] // Run with: cargo test --ignored test_csv_to_postgres_loading
async fn test_csv_to_postgres_loading() {
    if !is_postgres_available().await {
        eprintln!("Skipping test: PostgreSQL not available");
        return;
    }

    use graphica_coordinator::etl::loaders::database::LoadMode;
    use tokio_postgres::NoTls;

    let temp_dir = create_temp_dir();
    let csv_path = create_sample_csv(&temp_dir, "products.csv", PRODUCTS_CSV).await;

    let db_config = get_test_postgres_config();

    // Create target table
    let ssl_mode_str = db_config.ssl_mode.as_deref().unwrap_or("disable");
    let connection_string = format!(
        "host={} port={} dbname={} user={} password={} sslmode={}",
        db_config.host,
        db_config.port,
        db_config.database,
        db_config.username,
        db_config.password,
        ssl_mode_str
    );
    let (client, connection) = tokio_postgres::connect(&connection_string, NoTls)
        .await
        .expect("Failed to connect");

    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("Connection error: {}", e);
        }
    });

    client
        .execute(
            "CREATE TABLE IF NOT EXISTS test_products (
                product_id VARCHAR(50),
                name VARCHAR(100),
                category VARCHAR(50),
                price VARCHAR(20)
            )",
            &[],
        )
        .await
        .expect("Failed to create table");

    // Set up DataLoader
    let load_config = LoadConfig {
        table_name: "test_products".to_string(),
        load_mode: LoadMode::Insert,
        key_fields: None,
        batch_size: 1000,
        max_errors: Some(10),
    };

    let data_loader = DataLoader::new(DatabaseType::Postgres, db_config, load_config);

    // Create CSV reader
    let source = DataSource::CsvFile {
        file_id: "products".to_string(),
        file_path: csv_path,
        encoding: Some("UTF-8".to_string()),
        delimiter: Some(','),
        has_header: true,
    };

    let reader = create_reader(source).expect("Failed to create CSV reader");

    // Execute load
    let stats = data_loader
        .load_from_reader(reader)
        .await
        .expect("Failed to load data");

    // Verify stats
    assert_eq!(stats.rows_read, 3);
    assert_eq!(stats.rows_loaded, 3);
    assert_eq!(stats.rows_failed, 0);

    // Verify data in database
    let count: i64 = client
        .query_one("SELECT COUNT(*) FROM test_products", &[])
        .await
        .expect("Failed to query count")
        .get(0);

    assert_eq!(count, 3);

    // Cleanup
    client
        .execute("DROP TABLE test_products", &[])
        .await
        .expect("Failed to cleanup");
}

// ============================================================================
// Module 7: End-to-End Mixed Source Tests (Real PostgreSQL)
// ============================================================================

#[tokio::test]
#[ignore] // Run with: cargo test --ignored test_postgres_to_postgres_etl
async fn test_postgres_to_postgres_etl() {
    if !is_postgres_available().await {
        eprintln!("Skipping test: PostgreSQL not available");
        return;
    }

    use graphica_coordinator::etl::loaders::database::LoadMode;
    use tokio_postgres::NoTls;

    let db_config = get_test_postgres_config();

    // Set up source and target tables
    let ssl_mode_str = db_config.ssl_mode.as_deref().unwrap_or("disable");
    let connection_string = format!(
        "host={} port={} dbname={} user={} password={} sslmode={}",
        db_config.host,
        db_config.port,
        db_config.database,
        db_config.username,
        db_config.password,
        ssl_mode_str
    );
    let (client, connection) = tokio_postgres::connect(&connection_string, NoTls)
        .await
        .expect("Failed to connect");

    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("Connection error: {}", e);
        }
    });

    // Create source table with test data
    client
        .batch_execute(
            "DROP TABLE IF EXISTS source_orders;
             CREATE TABLE source_orders (
                 order_id INTEGER,
                 customer_id INTEGER,
                 order_total DECIMAL(10,2),
                 status VARCHAR(20)
             );
             INSERT INTO source_orders VALUES
                 (1, 101, 99.99, 'completed'),
                 (2, 102, 149.50, 'pending'),
                 (3, 101, 75.00, 'completed');

             DROP TABLE IF EXISTS target_completed_orders;
             CREATE TABLE target_completed_orders (
                 order_id VARCHAR(50),
                 customer_id VARCHAR(50),
                 order_total VARCHAR(50),
                 status VARCHAR(50)
             );",
        )
        .await
        .expect("Failed to set up tables");

    // Create DatabaseQueryReader for source
    let source = DataSource::DatabaseQuery {
        datasource_id: "source_db".to_string(),
        database_type: DatabaseType::Postgres,
        connection_config: db_config.clone(),
        query: "SELECT order_id, customer_id, order_total, status FROM source_orders WHERE status = 'completed'"
            .to_string(),
        fetch_size: 1000,
    };

    let reader = create_reader(source).expect("Failed to create reader");

    // Create DataLoader for target
    let load_config = LoadConfig {
        table_name: "target_completed_orders".to_string(),
        load_mode: LoadMode::Insert,
        key_fields: None,
        batch_size: 1000,
        max_errors: Some(10),
    };

    let data_loader = DataLoader::new(DatabaseType::Postgres, db_config, load_config);

    // Execute ETL
    let stats = data_loader
        .load_from_reader(reader)
        .await
        .expect("Failed to execute ETL");

    // Verify stats
    assert_eq!(stats.rows_read, 2); // Only 2 completed orders
    assert_eq!(stats.rows_loaded, 2);

    // Verify target data
    let count: i64 = client
        .query_one("SELECT COUNT(*) FROM target_completed_orders", &[])
        .await
        .expect("Failed to query count")
        .get(0);

    assert_eq!(count, 2);

    // Cleanup
    client
        .batch_execute(
            "DROP TABLE source_orders;
             DROP TABLE target_completed_orders;",
        )
        .await
        .expect("Failed to cleanup");
}

// ============================================================================
// Module 8: Connection String Building Tests
// ============================================================================

#[test]
fn test_postgres_connection_string() {
    let config = DatabaseConnectionConfig {
        host: "db.example.com".to_string(),
        port: 5432,
        database: "analytics".to_string(),
        username: "etl_user".to_string(),
        password: "secret123".to_string(),
        ssl_mode: Some("require".to_string()),
        extra_params: HashMap::new(),
    };

    // Connection string format: "host=... port=... dbname=... user=... password=... sslmode=..."
    let ssl_mode_str = config.ssl_mode.as_deref().unwrap_or("disable");
    let conn_str = format!(
        "host={} port={} dbname={} user={} password={} sslmode={}",
        config.host, config.port, config.database, config.username, config.password, ssl_mode_str
    );

    assert!(conn_str.contains("host=db.example.com"));
    assert!(conn_str.contains("port=5432"));
    assert!(conn_str.contains("dbname=analytics"));
    assert!(conn_str.contains("sslmode=require"));
}

#[test]
fn test_db2_connection_string() {
    let config = DatabaseConnectionConfig {
        host: "db2.example.com".to_string(),
        port: 50000,
        database: "PRODDB".to_string(),
        username: "db2inst1".to_string(),
        password: "db2pass".to_string(),
        ssl_mode: None,
        extra_params: HashMap::new(),
    };

    // DB2 ODBC connection string format
    let conn_str = format!(
        "DRIVER={{IBM DB2 ODBC DRIVER}};DATABASE={};HOSTNAME={};PORT={};PROTOCOL=TCPIP;UID={};PWD={};",
        config.database, config.host, config.port, config.username, config.password
    );

    assert!(conn_str.contains("DATABASE=PRODDB"));
    assert!(conn_str.contains("HOSTNAME=db2.example.com"));
    assert!(conn_str.contains("PORT=50000"));
}

// ============================================================================
// Module 9: Error Handling Tests
// ============================================================================

#[tokio::test]
async fn test_csv_reader_missing_file() {
    let source = DataSource::CsvFile {
        file_id: "missing".to_string(),
        file_path: PathBuf::from("/nonexistent/missing.csv"),
        encoding: Some("UTF-8".to_string()),
        delimiter: Some(','),
        has_header: true,
    };

    let reader_result = create_reader(source);
    assert!(reader_result.is_ok(), "Reader creation should succeed");

    let mut reader = reader_result.unwrap();
    let stream_result = reader.read().await;

    // Reading should fail due to missing file
    assert!(stream_result.is_err(), "Reading missing file should fail");
}

#[test]
fn test_database_query_invalid_config() {
    let config = DatabaseConnectionConfig {
        host: "".to_string(), // Invalid: empty host
        port: 0,              // Invalid: port 0
        database: "test".to_string(),
        username: "user".to_string(),
        password: "pass".to_string(),
        ssl_mode: Some("disable".to_string()),
        extra_params: HashMap::new(),
    };

    let source = DataSource::DatabaseQuery {
        datasource_id: "invalid".to_string(),
        database_type: DatabaseType::Postgres,
        connection_config: config,
        query: "SELECT 1".to_string(),
        fetch_size: 1000,
    };

    // Reader creation might succeed, but connection should fail
    let reader = create_reader(source);
    assert!(
        reader.is_ok(),
        "Reader creation should succeed even with invalid config"
    );
}
