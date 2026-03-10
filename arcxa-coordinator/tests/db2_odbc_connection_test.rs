//! ODBC DB2 Connection Integration Test
//!
//! Tests the OdbcDB2Connection against a live DB2 instance.
//!
//! **Prerequisites:**
//! - DB2 Docker container running on localhost:50000
//! - Database: GRAPHICA
//! - User: db2inst1
//! - Password: graphica-db2-pass
//!
//! **Run with:**
//! ```bash
//! cargo test --features odbc --test db2_odbc_connection_test -- --nocapture
//! ```

#[cfg(feature = "odbc")]
mod odbc_tests {
    use graphica_coordinator::mapping::loader::{
        db2_connection::{DB2Config, DB2Connection},
        OdbcDB2Connection,
    };

    fn get_test_config() -> DB2Config {
        use std::time::Duration;
        DB2Config {
            host: std::env::var("GRAPHICA_TEST_DB2_HOST")
                .unwrap_or_else(|_| "localhost".to_string()),
            port: std::env::var("GRAPHICA_TEST_DB2_PORT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(50000),
            database: std::env::var("GRAPHICA_TEST_DB2_DATABASE")
                .unwrap_or_else(|_| "GRAPHICA".to_string()),
            username: std::env::var("GRAPHICA_TEST_DB2_USER")
                .unwrap_or_else(|_| "db2inst1".to_string()),
            password: std::env::var("GRAPHICA_TEST_DB2_PASSWORD")
                .unwrap_or_else(|_| "graphica-db2-pass".to_string()),
            max_connections: 5,
            min_idle_connections: Some(1),
            auto_commit: false,
            connection_timeout: Duration::from_secs(30),
            query_timeout: Duration::from_secs(30),
            max_retry_attempts: 3,
            retry_backoff_ms: 100,
        }
    }

    #[test]
    #[ignore] // Run explicitly with: cargo test --features odbc -- --ignored
    fn test_odbc_connection_basic() {
        // Connect to DB2
        let config = get_test_config();
        let mut conn = OdbcDB2Connection::connect(&config).expect("Failed to connect to DB2");

        // Test basic query
        let rows = conn
            .query("SELECT 1 FROM SYSIBM.SYSDUMMY1", &[])
            .expect("Failed to execute query");

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].len(), 1);
        assert_eq!(rows[0][0], "1");

        println!("✅ Basic query successful");
    }

    #[test]
    #[ignore]
    fn test_odbc_connection_is_alive() {
        let config = get_test_config();
        let mut conn = OdbcDB2Connection::connect(&config).expect("Failed to connect to DB2");

        assert!(conn.is_alive(), "Connection should be alive");

        println!("✅ Connection is alive");
    }

    #[test]
    #[ignore]
    fn test_odbc_create_table_and_insert() {
        let config = get_test_config();
        let mut conn = OdbcDB2Connection::connect(&config).expect("Failed to connect to DB2");

        // Drop table if exists (ignore errors)
        let _ = conn.execute("DROP TABLE TEST_ODBC_TABLE", &[]);

        // Create test table
        let create_sql = r#"
            CREATE TABLE TEST_ODBC_TABLE (
                ID INTEGER NOT NULL,
                NAME VARCHAR(100),
                PRIMARY KEY (ID)
            )
        "#;
        conn.execute(create_sql, &[])
            .expect("Failed to create table");

        println!("✅ Table created");

        // Insert test data
        let insert_sql = "INSERT INTO TEST_ODBC_TABLE (ID, NAME) VALUES (1, 'Test')";
        let affected = conn
            .execute(insert_sql, &[])
            .expect("Failed to insert data");

        println!("✅ Inserted {} row(s)", affected);

        // Query data back
        let rows = conn
            .query("SELECT ID, NAME FROM TEST_ODBC_TABLE ORDER BY ID", &[])
            .expect("Failed to query data");

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0][0], "1");
        assert_eq!(rows[0][1], "Test");

        println!("✅ Query successful: {:?}", rows);

        // Cleanup
        conn.execute("DROP TABLE TEST_ODBC_TABLE", &[])
            .expect("Failed to drop table");

        println!("✅ Table dropped");
    }

    #[test]
    #[ignore]
    fn test_odbc_transactions() {
        let config = get_test_config();
        let mut conn = OdbcDB2Connection::connect(&config).expect("Failed to connect to DB2");

        // Drop table if exists (ignore errors)
        let _ = conn.execute("DROP TABLE TEST_ODBC_TXN", &[]);

        // Create test table
        conn.execute(
            "CREATE TABLE TEST_ODBC_TXN (ID INTEGER NOT NULL, NAME VARCHAR(100), PRIMARY KEY (ID))",
            &[],
        )
        .expect("Failed to create table");

        // Test transaction commit
        conn.begin_transaction()
            .expect("Failed to begin transaction");
        conn.execute(
            "INSERT INTO TEST_ODBC_TXN (ID, NAME) VALUES (1, 'Committed')",
            &[],
        )
        .expect("Failed to insert");
        conn.commit().expect("Failed to commit");

        let rows = conn
            .query("SELECT COUNT(*) FROM TEST_ODBC_TXN", &[])
            .expect("Failed to count");
        assert_eq!(rows[0][0], "1");
        println!("✅ Transaction commit successful");

        // Test transaction rollback
        conn.begin_transaction()
            .expect("Failed to begin transaction");
        conn.execute(
            "INSERT INTO TEST_ODBC_TXN (ID, NAME) VALUES (2, 'Rolled Back')",
            &[],
        )
        .expect("Failed to insert");
        conn.rollback().expect("Failed to rollback");

        let rows = conn
            .query("SELECT COUNT(*) FROM TEST_ODBC_TXN", &[])
            .expect("Failed to count");
        assert_eq!(rows[0][0], "1"); // Still only 1 row
        println!("✅ Transaction rollback successful");

        // Cleanup
        conn.execute("DROP TABLE TEST_ODBC_TXN", &[])
            .expect("Failed to drop table");
    }

    #[test]
    #[ignore]
    fn test_odbc_null_values() {
        let config = get_test_config();
        let mut conn = OdbcDB2Connection::connect(&config).expect("Failed to connect to DB2");

        // Drop table if exists (ignore errors)
        let _ = conn.execute("DROP TABLE TEST_ODBC_NULL", &[]);

        // Create test table
        conn.execute(
            "CREATE TABLE TEST_ODBC_NULL (ID INTEGER NOT NULL, NAME VARCHAR(100), PRIMARY KEY (ID))",
            &[],
        )
        .expect("Failed to create table");

        // Insert row with NULL
        conn.execute(
            "INSERT INTO TEST_ODBC_NULL (ID, NAME) VALUES (1, NULL)",
            &[],
        )
        .expect("Failed to insert");

        // Query data back
        let rows = conn
            .query("SELECT ID, NAME FROM TEST_ODBC_NULL", &[])
            .expect("Failed to query");

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0][0], "1");
        assert_eq!(rows[0][1], "NULL"); // Our NULL representation
        println!("✅ NULL handling successful: {:?}", rows);

        // Cleanup
        conn.execute("DROP TABLE TEST_ODBC_NULL", &[])
            .expect("Failed to drop table");
    }
}

#[cfg(not(feature = "odbc"))]
#[test]
fn test_odbc_feature_disabled() {
    println!("ODBC feature not enabled. Run with: cargo test --features odbc");
}
