//! DB2 Integration Tests
//!
//! Tests the DB2 load transformer with a real DB2 instance.
//!
//! ## Prerequisites
//!
//! 1. DB2 instance running (Docker recommended):
//!    ```bash
//!    docker run -d --name db2-test \
//!      -e LICENSE=accept \
//!      -e DB2INST1_PASSWORD=graphica-db2-pass \
//!      -e DBNAME=TESTDB \
//!      -p 50000:50000 \
//!      ibmcom/db2
//!    ```
//!
//! 2. Set environment variable:
//!    ```bash
//!    export DB2_TEST_ENABLED=1
//!    ```
//!
//! 3. Run tests with ODBC feature:
//!    ```bash
//!    cargo test --test db2_integration_test --features odbc
//!    ```

use anyhow::Result;
use graphica_coordinator::mapping::loader::{create_db2_pool, DB2Config, DB2PoolConfig};
use graphica_coordinator::workflows::engine::transformers::{
    db2_load::Db2LoadTransformer, Transformer,
};
use serde_json::json;
use std::env;
use std::sync::Arc;
use tempfile::TempDir;

/// Check if DB2 integration tests are enabled
fn db2_tests_enabled() -> bool {
    env::var("DB2_TEST_ENABLED").unwrap_or_else(|_| "0".to_string()) == "1"
}

/// Get DB2 test configuration from environment
fn get_test_config() -> DB2Config {
    DB2Config {
        host: env::var("DB2_TEST_HOST").unwrap_or_else(|_| "localhost".to_string()),
        port: env::var("DB2_TEST_PORT")
            .unwrap_or_else(|_| "50000".to_string())
            .parse()
            .unwrap_or(50000),
        database: env::var("DB2_TEST_DATABASE").unwrap_or_else(|_| "TESTDB".to_string()),
        username: env::var("DB2_TEST_USER").unwrap_or_else(|_| "db2inst1".to_string()),
        password: env::var("DB2_TEST_PASSWORD").unwrap_or_else(|_| "graphica-db2-pass".to_string()),
        ..DB2Config::default()
    }
}

/// Create test table and return table name
#[cfg(feature = "odbc")]
fn setup_test_table(_transformer: &Db2LoadTransformer, table_suffix: &str) -> Result<String> {
    use graphica_coordinator::mapping::loader::{DB2Connection, OdbcDB2Connection};

    let config = get_test_config();
    let table_name = format!("TEST_CUSTOMERS_{}", table_suffix);

    let mut conn = OdbcDB2Connection::connect(&config)?;

    // Drop table if exists (ignore errors)
    let _ = conn.execute(&format!("DROP TABLE {}", table_name), &[]);

    // Create fresh table
    let create_sql = format!(
        "CREATE TABLE {} (
            customer_id VARCHAR(20) NOT NULL PRIMARY KEY,
            first_name VARCHAR(50),
            last_name VARCHAR(50),
            email VARCHAR(100),
            age INTEGER
        )",
        table_name
    );

    conn.execute(&create_sql, &[])?;

    Ok(table_name)
}

/// Cleanup test table
#[cfg(feature = "odbc")]
fn cleanup_test_table(table_name: &str) -> Result<()> {
    use graphica_coordinator::mapping::loader::{DB2Connection, OdbcDB2Connection};

    let config = get_test_config();
    let mut conn = OdbcDB2Connection::connect(&config)?;

    let _ = conn.execute(&format!("DROP TABLE {}", table_name), &[]);

    Ok(())
}

#[tokio::test]
#[cfg(feature = "odbc")]
async fn test_basic_insert_operation() -> Result<()> {
    if !db2_tests_enabled() {
        println!("Skipping DB2 integration test (DB2_TEST_ENABLED not set)");
        return Ok(());
    }

    let transformer = Db2LoadTransformer::new();
    let table_name = setup_test_table(&transformer, "INSERT")?;

    let config = get_test_config();
    let config_json = json!({
        "connection": {
            "host": config.host,
            "port": config.port,
            "database": config.database,
            "user": config.username,
            "password": config.password
        },
        "table": table_name,
        "load_mode": "insert",
        "batch_size": 100,
        "use_transactions": true,
        "dlq_enabled": false,
        "max_retries": 0  // No retries for test
    });

    let mut data = json!({
        "rows": [
            {
                "customer_id": "C001",
                "first_name": "Alice",
                "last_name": "Smith",
                "email": "alice@example.com",
                "age": 30
            },
            {
                "customer_id": "C002",
                "first_name": "Bob",
                "last_name": "Jones",
                "email": "bob@example.com",
                "age": 25
            }
        ]
    });

    // Execute transformation
    transformer.transform(&config_json, &mut data, None).await?;

    // Verify results
    let result = data.get("db2_load").unwrap();
    assert_eq!(result["status"], "success");
    assert_eq!(result["rows_loaded"], 2);
    assert_eq!(result["rows_failed"], 0);

    // Cleanup
    cleanup_test_table(&table_name)?;

    Ok(())
}

#[tokio::test]
#[cfg(feature = "odbc")]
async fn test_multi_row_merge_operation() -> Result<()> {
    if !db2_tests_enabled() {
        println!("Skipping DB2 integration test (DB2_TEST_ENABLED not set)");
        return Ok(());
    }

    let transformer = Db2LoadTransformer::new();
    let table_name = setup_test_table(&transformer, "MERGE")?;

    let config = get_test_config();

    // First insert
    let config_json = json!({
        "connection": {
            "host": config.host,
            "port": config.port,
            "database": config.database,
            "user": config.username,
            "password": config.password
        },
        "table": table_name,
        "load_mode": "insert",
        "batch_size": 100
    });

    let mut data = json!({
        "rows": [
            {"customer_id": "C001", "first_name": "Alice", "last_name": "Smith", "email": "alice@example.com", "age": 30},
            {"customer_id": "C002", "first_name": "Bob", "last_name": "Jones", "email": "bob@example.com", "age": 25}
        ]
    });

    transformer.transform(&config_json, &mut data, None).await?;

    // Now MERGE with updates and new row
    let merge_config_json = json!({
        "connection": {
            "host": config.host,
            "port": config.port,
            "database": config.database,
            "user": config.username,
            "password": config.password
        },
        "table": table_name,
        "load_mode": "upsert",
        "batch_size": 100,
        "primary_keys": ["customer_id"],
        "use_transactions": true
    });

    let mut merge_data = json!({
        "rows": [
            {"customer_id": "C001", "first_name": "Alice", "last_name": "Smith-Updated", "email": "alice.new@example.com", "age": 31},
            {"customer_id": "C002", "first_name": "Bob", "last_name": "Jones", "email": "bob@example.com", "age": 26},
            {"customer_id": "C003", "first_name": "Charlie", "last_name": "Brown", "email": "charlie@example.com", "age": 35}
        ]
    });

    transformer
        .transform(&merge_config_json, &mut merge_data, None)
        .await?;

    // Verify results
    let result = merge_data.get("db2_load").unwrap();
    assert_eq!(result["status"], "success");
    assert_eq!(result["rows_loaded"], 3); // 2 updates + 1 insert
    assert_eq!(result["rows_failed"], 0);

    cleanup_test_table(&table_name)?;

    Ok(())
}

#[tokio::test]
#[cfg(feature = "odbc")]
async fn test_transaction_rollback_on_error() -> Result<()> {
    if !db2_tests_enabled() {
        println!("Skipping DB2 integration test (DB2_TEST_ENABLED not set)");
        return Ok(());
    }

    let transformer = Db2LoadTransformer::new();
    let table_name = setup_test_table(&transformer, "ROLLBACK")?;

    let config = get_test_config();
    let config_json = json!({
        "connection": {
            "host": config.host,
            "port": config.port,
            "database": config.database,
            "user": config.username,
            "password": config.password
        },
        "table": table_name,
        "load_mode": "insert",
        "batch_size": 100,
        "use_transactions": true,
        "fail_on_error": true  // Fail fast on any error
    });

    // Insert with invalid data (age as string should fail type conversion)
    let mut data = json!({
        "rows": [
            {"customer_id": "C001", "first_name": "Alice", "last_name": "Smith", "email": "alice@example.com", "age": 30},
            {"customer_id": "C001", "first_name": "Alice Duplicate", "last_name": "Smith", "email": "alice2@example.com", "age": 30}  // Duplicate key
        ]
    });

    // Should fail due to duplicate key
    let result = transformer.transform(&config_json, &mut data, None).await;
    assert!(result.is_err(), "Expected error due to duplicate key");

    // Verify table is empty (transaction rolled back)
    use graphica_coordinator::mapping::loader::{DB2Connection, OdbcDB2Connection};
    let mut conn = OdbcDB2Connection::connect(&config)?;
    let rows = conn.query(&format!("SELECT COUNT(*) FROM {}", table_name), &[])?;
    assert_eq!(rows[0][0], "0", "Table should be empty after rollback");

    cleanup_test_table(&table_name)?;

    Ok(())
}

#[tokio::test]
#[cfg(feature = "odbc")]
async fn test_dlq_functionality() -> Result<()> {
    if !db2_tests_enabled() {
        println!("Skipping DB2 integration test (DB2_TEST_ENABLED not set)");
        return Ok(());
    }

    let transformer = Db2LoadTransformer::new();
    let table_name = setup_test_table(&transformer, "DLQ")?;
    let dlq_dir = TempDir::new()?;

    let config = get_test_config();
    let config_json = json!({
        "connection": {
            "host": config.host,
            "port": config.port,
            "database": config.database,
            "user": config.username,
            "password": config.password
        },
        "table": table_name,
        "load_mode": "insert",
        "batch_size": 100,
        "use_transactions": false,  // No transaction so we can see partial inserts
        "dlq_enabled": true,
        "dlq_output_dir": dlq_dir.path().to_str().unwrap(),
        "fail_on_error": false  // Continue on error, write to DLQ
    });

    // Insert with one good row and one duplicate (should fail)
    let mut data = json!({
        "rows": [
            {"customer_id": "C001", "first_name": "Alice", "last_name": "Smith", "email": "alice@example.com", "age": 30},
            {"customer_id": "C001", "first_name": "Alice Duplicate", "last_name": "Smith", "email": "alice2@example.com", "age": 30}  // Duplicate key
        ]
    });

    transformer.transform(&config_json, &mut data, None).await?;

    // Verify results
    let result = data.get("db2_load").unwrap();
    assert_eq!(result["status"], "success");
    assert_eq!(result["rows_loaded"], 1); // Only first row succeeded
    assert_eq!(result["rows_failed"], 1); // Second row failed

    // Verify DLQ file was created
    let dlq_files: Vec<_> = std::fs::read_dir(dlq_dir.path())?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|s| s == "jsonl").unwrap_or(false))
        .collect();

    assert!(!dlq_files.is_empty(), "DLQ file should be created");

    cleanup_test_table(&table_name)?;

    Ok(())
}

#[tokio::test]
#[cfg(feature = "odbc")]
async fn test_connection_pool_reuse() -> Result<()> {
    if !db2_tests_enabled() {
        println!("Skipping DB2 integration test (DB2_TEST_ENABLED not set)");
        return Ok(());
    }

    let table_name = setup_test_table(&Db2LoadTransformer::new(), "POOL")?;

    let config = get_test_config();

    // Create connection pool
    let pool = Arc::new(
        create_db2_pool(DB2PoolConfig {
            db2_config: config.clone(),
            max_size: 5,
            ..DB2PoolConfig::default()
        })
        .await?,
    );

    // Create transformer with pool
    let transformer = Db2LoadTransformer::new().with_connection_pool(pool.clone());

    let config_json = json!({
        "connection": {
            "host": config.host,
            "port": config.port,
            "database": config.database,
            "user": config.username,
            "password": config.password
        },
        "table": table_name,
        "load_mode": "insert",
        "batch_size": 100
    });

    // Execute multiple transformations using the same pool
    for i in 0..3 {
        let mut data = json!({
            "rows": [
                {
                    "customer_id": format!("C{:03}", i),
                    "first_name": "Test",
                    "last_name": "User",
                    "email": format!("test{}@example.com", i),
                    "age": 25 + i
                }
            ]
        });

        transformer.transform(&config_json, &mut data, None).await?;

        let result = data.get("db2_load").unwrap();
        assert_eq!(result["status"], "success");
        assert_eq!(result["rows_loaded"], 1);
    }

    // Verify all rows were inserted
    use graphica_coordinator::mapping::loader::{DB2Connection, OdbcDB2Connection};
    let mut conn = OdbcDB2Connection::connect(&config)?;
    let rows = conn.query(&format!("SELECT COUNT(*) FROM {}", table_name), &[])?;
    assert_eq!(rows[0][0], "3", "All 3 rows should be inserted");

    cleanup_test_table(&table_name)?;

    Ok(())
}

#[test]
fn test_retry_config_parsing() -> Result<()> {
    let transformer = Db2LoadTransformer::new();

    let config = json!({
        "connection": {
            "host": "localhost",
            "port": 50000,
            "database": "TESTDB",
            "user": "db2inst1",
            "password": "password"
        },
        "table": "TEST_TABLE",
        "max_retries": 5,
        "retry_initial_delay_ms": 200,
        "retry_max_delay_ms": 10000,
        "retry_multiplier": 3.0
    });

    // Should parse without errors
    assert!(transformer.validate_config(&config).is_ok());

    Ok(())
}

#[test]
fn test_config_validation() -> Result<()> {
    let transformer = Db2LoadTransformer::new();

    // Missing connection
    let config = json!({
        "table": "TEST_TABLE"
    });
    assert!(transformer.validate_config(&config).is_err());

    // Missing table
    let config = json!({
        "connection": {
            "host": "localhost",
            "port": 50000,
            "database": "TESTDB",
            "user": "db2inst1",
            "password": "password"
        }
    });
    assert!(transformer.validate_config(&config).is_err());

    // UPSERT without primary_keys
    let config = json!({
        "connection": {
            "host": "localhost",
            "port": 50000,
            "database": "TESTDB",
            "user": "db2inst1",
            "password": "password"
        },
        "table": "TEST_TABLE",
        "load_mode": "upsert"
    });
    assert!(transformer.validate_config(&config).is_err());

    // Valid configuration
    let config = json!({
        "connection": {
            "host": "localhost",
            "port": 50000,
            "database": "TESTDB",
            "user": "db2inst1",
            "password": "password"
        },
        "table": "TEST_TABLE",
        "load_mode": "upsert",
        "primary_keys": ["id"]
    });
    assert!(transformer.validate_config(&config).is_ok());

    Ok(())
}
