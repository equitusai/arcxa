//! Integration tests for ODBC connection pooling
//!
//! These tests verify that Oracle and SAP HANA connection pools work correctly
//! with the leak-and-reclaim pattern for Environment lifetime management.

#![cfg(feature = "odbc")]

use graphica_coordinator::mapping::discovery::extractors::odbc::*;
use graphica_coordinator::mapping::discovery::extractors::{
    OdbcOracleConnection, OdbcSAPHANAConnection,
};

/// Test Oracle connection pool creation and basic operations
#[tokio::test]
async fn test_oracle_connection_pool_creation() {
    // Skip if Oracle connection string not configured
    if std::env::var("TEST_ORACLE_CONN_STRING").is_err() {
        eprintln!("Skipping Oracle pool test - TEST_ORACLE_CONN_STRING not set");
        eprintln!("Set TEST_ORACLE_CONN_STRING to run this test");
        return;
    }

    let conn_string = std::env::var("TEST_ORACLE_CONN_STRING").unwrap();

    // Create pool with small size for testing
    let mut config = OdbcPoolConfig::new(conn_string);
    config.max_size = 3;

    let pool = create_odbc_pool::<OdbcOracleConnection>(config)
        .await
        .expect("Failed to create Oracle connection pool");

    // Verify pool created successfully
    let stats = get_odbc_pool_stats(&pool);
    assert_eq!(stats.max_size, 3);
    assert_eq!(stats.size, 0); // No connections created yet
}

/// Test connection acquisition and return to pool
#[tokio::test]
async fn test_oracle_connection_acquisition() {
    if std::env::var("TEST_ORACLE_CONN_STRING").is_err() {
        eprintln!("Skipping Oracle acquisition test - TEST_ORACLE_CONN_STRING not set");
        return;
    }

    let conn_string = std::env::var("TEST_ORACLE_CONN_STRING").unwrap();
    let config = OdbcPoolConfig::new(conn_string);
    let pool = create_odbc_pool::<OdbcOracleConnection>(config)
        .await
        .expect("Failed to create pool");

    // Acquire first connection
    let conn1 = pool.get().await.expect("Failed to get conn1");
    let stats = get_odbc_pool_stats(&pool);
    assert_eq!(stats.size, 1);
    assert_eq!(stats.available, 0); // Connection in use

    // Acquire second connection
    let conn2 = pool.get().await.expect("Failed to get conn2");
    let stats = get_odbc_pool_stats(&pool);
    assert_eq!(stats.size, 2);

    // Return connections to pool
    drop(conn1);
    drop(conn2);

    // Give pool time to process returns
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Acquire third connection - should reuse existing
    let _conn3 = pool.get().await.expect("Failed to get conn3");
    let stats = get_odbc_pool_stats(&pool);
    assert!(stats.size <= 2, "Pool should reuse existing connections");
}

/// Test concurrent connection access
#[tokio::test]
async fn test_oracle_concurrent_access() {
    if std::env::var("TEST_ORACLE_CONN_STRING").is_err() {
        eprintln!("Skipping Oracle concurrent test - TEST_ORACLE_CONN_STRING not set");
        return;
    }

    let conn_string = std::env::var("TEST_ORACLE_CONN_STRING").unwrap();
    let mut config = OdbcPoolConfig::new(conn_string);
    config.max_size = 5;

    let pool = std::sync::Arc::new(
        create_odbc_pool::<OdbcOracleConnection>(config)
            .await
            .expect("Failed to create pool"),
    );

    // Spawn multiple concurrent tasks
    let mut handles = vec![];
    for i in 0..10 {
        let pool_clone = pool.clone();
        let handle = tokio::spawn(async move {
            let mut conn = pool_clone.get().await.expect("Failed to get connection");

            // Execute simple query using trait method
            let query = "SELECT 1 FROM DUAL";
            conn.execute_query(query, false)
                .expect("Failed to execute query");

            // Simulate some work
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

            i // Return task ID
        });
        handles.push(handle);
    }

    // Wait for all tasks to complete
    for handle in handles {
        handle.await.expect("Task failed");
    }

    // Verify pool handled concurrent access
    let stats = get_odbc_pool_stats(&pool);
    assert!(stats.size <= 5, "Pool should not exceed max_size");
}

/// Test connection health checks
#[tokio::test]
async fn test_oracle_health_check() {
    if std::env::var("TEST_ORACLE_CONN_STRING").is_err() {
        eprintln!("Skipping Oracle health check test - TEST_ORACLE_CONN_STRING not set");
        return;
    }

    let conn_string = std::env::var("TEST_ORACLE_CONN_STRING").unwrap();
    let mut config = OdbcPoolConfig::new(conn_string);
    config.health_check_enabled = true;

    let pool = create_odbc_pool::<OdbcOracleConnection>(config)
        .await
        .expect("Failed to create pool");

    // Get connection and verify it's alive
    let conn = pool.get().await.expect("Failed to get connection");
    assert!(conn.is_alive(), "Connection should be alive");
}

/// Test SAP HANA connection pool (similar to Oracle tests)
#[tokio::test]
async fn test_saphana_connection_pool_creation() {
    if std::env::var("TEST_SAPHANA_CONN_STRING").is_err() {
        eprintln!("Skipping SAP HANA pool test - TEST_SAPHANA_CONN_STRING not set");
        eprintln!("Set TEST_SAPHANA_CONN_STRING to run this test");
        return;
    }

    let conn_string = std::env::var("TEST_SAPHANA_CONN_STRING").unwrap();
    let mut config = OdbcPoolConfig::new(conn_string);
    config.max_size = 3;

    let pool = create_odbc_pool::<OdbcSAPHANAConnection>(config)
        .await
        .expect("Failed to create SAP HANA connection pool");

    let stats = get_odbc_pool_stats(&pool);
    assert_eq!(stats.max_size, 3);
}

/// Test SAP HANA connection acquisition
#[tokio::test]
async fn test_saphana_connection_acquisition() {
    if std::env::var("TEST_SAPHANA_CONN_STRING").is_err() {
        eprintln!("Skipping SAP HANA acquisition test - TEST_SAPHANA_CONN_STRING not set");
        return;
    }

    let conn_string = std::env::var("TEST_SAPHANA_CONN_STRING").unwrap();
    let config = OdbcPoolConfig::new(conn_string);
    let pool = create_odbc_pool::<OdbcSAPHANAConnection>(config)
        .await
        .expect("Failed to create pool");

    let conn1 = pool.get().await.expect("Failed to get conn1");
    let stats = get_odbc_pool_stats(&pool);
    assert_eq!(stats.size, 1);

    drop(conn1);
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let _conn2 = pool.get().await.expect("Failed to get conn2");
    let stats = get_odbc_pool_stats(&pool);
    assert!(stats.size <= 1, "Pool should reuse connection");
}

/// Test SAP HANA health check
#[tokio::test]
async fn test_saphana_health_check() {
    if std::env::var("TEST_SAPHANA_CONN_STRING").is_err() {
        eprintln!("Skipping SAP HANA health check test - TEST_SAPHANA_CONN_STRING not set");
        return;
    }

    let conn_string = std::env::var("TEST_SAPHANA_CONN_STRING").unwrap();
    let mut config = OdbcPoolConfig::new(conn_string);
    config.health_check_enabled = true;

    let pool = create_odbc_pool::<OdbcSAPHANAConnection>(config)
        .await
        .expect("Failed to create pool");

    let conn = pool.get().await.expect("Failed to get connection");
    assert!(conn.is_alive(), "Connection should be alive");
}

/// Test pool timeout configuration
#[tokio::test]
async fn test_pool_timeout_configuration() {
    if std::env::var("TEST_ORACLE_CONN_STRING").is_err() {
        eprintln!("Skipping timeout config test - TEST_ORACLE_CONN_STRING not set");
        return;
    }

    let conn_string = std::env::var("TEST_ORACLE_CONN_STRING").unwrap();
    let mut config = OdbcPoolConfig::new(conn_string);

    // Configure custom timeouts
    config.timeouts.wait = std::time::Duration::from_secs(5);
    config.timeouts.create = std::time::Duration::from_secs(15);
    config.timeouts.recycle = std::time::Duration::from_secs(3);

    let pool = create_odbc_pool::<OdbcOracleConnection>(config)
        .await
        .expect("Failed to create pool with custom timeouts");

    // Verify pool works with custom timeouts
    let _conn = pool
        .get()
        .await
        .expect("Failed to get connection with custom timeouts");
}

/// Test that pools handle errors gracefully
#[tokio::test]
async fn test_pool_error_handling() {
    // Use invalid connection string
    let invalid_conn_string = "INVALID_CONNECTION_STRING";
    let config = OdbcPoolConfig::new(invalid_conn_string.to_string());

    // Pool creation might succeed, but connection acquisition should fail
    match create_odbc_pool::<OdbcOracleConnection>(config).await {
        Ok(pool) => {
            // If pool created, connection acquisition should fail
            let result = pool.get().await;
            assert!(
                result.is_err(),
                "Should fail with invalid connection string"
            );
        }
        Err(_) => {
            // Pool creation failed, which is also acceptable
        }
    }
}
