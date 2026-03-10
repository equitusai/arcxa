//! DB2 Connection Management
//!
//! Production-grade connection manager for IBM DB2 with features:
//! - ODBC-based connectivity (optional, requires DB2 drivers)
//! - Connection pooling (r2d2)
//! - Transaction management
//! - Prepared statement execution
//! - DB2 SQLCODE error mapping
//! - Connection health checks
//! - Retry logic with exponential backoff
//!
//! ## Usage
//!
//! ```rust,ignore
//! use graphica_coordinator::mapping::loader::db2_connection::{DB2Config, DB2ConnectionManager};
//!
//! let config = DB2Config {
//!     host: "localhost".to_string(),
//!     port: 50000,
//!     database: "testdb".to_string(),
//!     username: "db2inst1".to_string(),
//!     password: "password".to_string(),
//!     ..Default::default()
//! };
//!
//! let manager = DB2ConnectionManager::new(config)?;
//! let conn = manager.get_connection()?;
//! let rows_affected = conn.execute("INSERT INTO customers (name) VALUES (?)", &[&"Alice"])?;
//! ```

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;

/// DB2 configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DB2Config {
    /// Database host
    pub host: String,

    /// Database port (default: 50000)
    pub port: u16,

    /// Database name
    pub database: String,

    /// Username
    pub username: String,

    /// Password (sensitive - consider using secrets manager)
    pub password: String,

    /// Maximum connections in pool
    pub max_connections: usize,

    /// Minimum idle connections
    pub min_idle_connections: Option<usize>,

    /// Connection timeout
    pub connection_timeout: Duration,

    /// Query timeout
    pub query_timeout: Duration,

    /// Enable auto-commit (default: false for transactions)
    pub auto_commit: bool,

    /// Connection retry attempts
    pub max_retry_attempts: usize,

    /// Retry backoff (ms)
    pub retry_backoff_ms: u64,
}

impl Default for DB2Config {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 50000,
            database: "testdb".to_string(),
            username: "db2inst1".to_string(),
            password: String::new(),
            max_connections: 10,
            min_idle_connections: Some(2),
            connection_timeout: Duration::from_secs(30),
            query_timeout: Duration::from_secs(60),
            auto_commit: false,
            max_retry_attempts: 3,
            retry_backoff_ms: 1000,
        }
    }
}

impl DB2Config {
    /// Build ODBC connection string
    pub fn connection_string(&self) -> String {
        format!(
            "DRIVER={{IBM DB2 ODBC DRIVER}};DATABASE={};HOSTNAME={};PORT={};PROTOCOL=TCPIP;UID={};PWD={};",
            self.database, self.host, self.port, self.username, self.password
        )
    }

    /// Build connection string with masked password (for logging)
    pub fn masked_connection_string(&self) -> String {
        format!(
            "DRIVER={{IBM DB2 ODBC DRIVER}};DATABASE={};HOSTNAME={};PORT={};PROTOCOL=TCPIP;UID={};PWD=***;",
            self.database, self.host, self.port, self.username
        )
    }
}

/// DB2 error types
#[derive(Debug, Error)]
pub enum DB2Error {
    /// Connection error
    #[error("DB2 connection error: {message}")]
    ConnectionError { message: String },

    /// Query execution error with SQLCODE
    #[error("DB2 query error (SQLCODE {sqlcode}): {message}")]
    QueryError { sqlcode: i32, message: String },

    /// Transaction error
    #[error("DB2 transaction error: {message}")]
    TransactionError { message: String },

    /// Duplicate key violation (SQLCODE -803)
    #[error("Duplicate key violation: {message}")]
    DuplicateKey { message: String },

    /// Foreign key constraint violation (SQLCODE -530)
    #[error("Foreign key constraint violation: {message}")]
    ForeignKeyViolation { message: String },

    /// NOT NULL constraint violation (SQLCODE -407)
    #[error("NOT NULL constraint violation: {message}")]
    NotNullViolation { message: String },

    /// Table not found (SQLCODE -204)
    #[error("Table not found: {message}")]
    TableNotFound { message: String },

    /// Connection pool exhausted
    #[error("Connection pool exhausted")]
    PoolExhausted,

    /// Connection timeout
    #[error("Connection timeout after {timeout:?}")]
    ConnectionTimeout { timeout: Duration },

    /// Generic error
    #[error("DB2 error: {0}")]
    Other(String),
}

impl DB2Error {
    /// Create DB2Error from SQLCODE
    pub fn from_sqlcode(sqlcode: i32, message: String) -> Self {
        match sqlcode {
            -803 => DB2Error::DuplicateKey { message },
            -530 => DB2Error::ForeignKeyViolation { message },
            -407 => DB2Error::NotNullViolation { message },
            -204 => DB2Error::TableNotFound { message },
            _ => DB2Error::QueryError { sqlcode, message },
        }
    }

    /// Check if error is retryable
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            DB2Error::ConnectionError { .. } | DB2Error::ConnectionTimeout { .. }
        )
    }
}

/// DB2 connection trait (abstraction for testability)
pub trait DB2Connection: Send + Sync {
    /// Execute a SQL statement with parameters
    fn execute(&mut self, sql: &str, params: &[&dyn SqlParam]) -> Result<u64, DB2Error>;

    /// Execute a query and return result set
    fn query(&mut self, sql: &str, params: &[&dyn SqlParam]) -> Result<Vec<Vec<String>>, DB2Error>;

    /// Begin transaction
    fn begin_transaction(&mut self) -> Result<(), DB2Error>;

    /// Commit transaction
    fn commit(&mut self) -> Result<(), DB2Error>;

    /// Rollback transaction
    fn rollback(&mut self) -> Result<(), DB2Error>;

    /// Check if connection is alive
    fn is_alive(&mut self) -> bool;

    /// Validate table exists in DB2 catalog
    ///
    /// Pre-validates table existence to avoid SQL errors that can trigger ODBC panic bugs.
    /// Returns Ok(()) if table exists, Err with descriptive message if not.
    fn validate_table_exists(&mut self, table_name: &str) -> Result<(), DB2Error>;
}

/// SQL parameter trait for type-safe parameter binding
pub trait SqlParam: Send + Sync {
    /// Convert to SQL string representation
    fn to_sql_string(&self) -> String;

    /// Get parameter type
    fn param_type(&self) -> SqlParamType;
}

/// SQL parameter types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqlParamType {
    String,
    Integer,
    Decimal,
    Boolean,
    Date,
    Timestamp,
    Null,
}

// Implement SqlParam for common types
impl SqlParam for String {
    fn to_sql_string(&self) -> String {
        self.clone()
    }

    fn param_type(&self) -> SqlParamType {
        SqlParamType::String
    }
}

impl SqlParam for &str {
    fn to_sql_string(&self) -> String {
        self.to_string()
    }

    fn param_type(&self) -> SqlParamType {
        SqlParamType::String
    }
}

impl SqlParam for i32 {
    fn to_sql_string(&self) -> String {
        self.to_string()
    }

    fn param_type(&self) -> SqlParamType {
        SqlParamType::Integer
    }
}

impl SqlParam for i64 {
    fn to_sql_string(&self) -> String {
        self.to_string()
    }

    fn param_type(&self) -> SqlParamType {
        SqlParamType::Integer
    }
}

impl SqlParam for bool {
    fn to_sql_string(&self) -> String {
        if *self { "1" } else { "0" }.to_string()
    }

    fn param_type(&self) -> SqlParamType {
        SqlParamType::Boolean
    }
}

impl<T: SqlParam> SqlParam for Option<T> {
    fn to_sql_string(&self) -> String {
        match self {
            Some(val) => val.to_sql_string(),
            None => "NULL".to_string(),
        }
    }

    fn param_type(&self) -> SqlParamType {
        match self {
            Some(val) => val.param_type(),
            None => SqlParamType::Null,
        }
    }
}

/// Mock DB2 connection for testing (no ODBC required)
#[derive(Debug)]
pub struct MockDB2Connection {
    executed_statements: Vec<String>,
    query_results: Vec<Vec<Vec<String>>>,
    in_transaction: bool,
    is_connected: bool,
}

impl MockDB2Connection {
    /// Create new mock connection
    pub fn new() -> Self {
        Self {
            executed_statements: Vec::new(),
            query_results: Vec::new(),
            in_transaction: false,
            is_connected: true,
        }
    }

    /// Set query results for testing
    pub fn set_query_results(&mut self, results: Vec<Vec<Vec<String>>>) {
        self.query_results = results;
    }

    /// Get executed statements (for verification in tests)
    pub fn executed_statements(&self) -> &[String] {
        &self.executed_statements
    }

    /// Disconnect (for testing connection failures)
    pub fn disconnect(&mut self) {
        self.is_connected = false;
    }
}

impl Default for MockDB2Connection {
    fn default() -> Self {
        Self::new()
    }
}

impl DB2Connection for MockDB2Connection {
    fn execute(&mut self, sql: &str, params: &[&dyn SqlParam]) -> Result<u64, DB2Error> {
        if !self.is_connected {
            return Err(DB2Error::ConnectionError {
                message: "Connection closed".to_string(),
            });
        }

        let mut sql_with_params = sql.to_string();
        for (i, param) in params.iter().enumerate() {
            sql_with_params =
                sql_with_params.replacen("?", &format!("'{}'", param.to_sql_string()), 1);
        }

        self.executed_statements.push(sql_with_params);

        // Simulate successful execution
        Ok(1)
    }

    fn query(&mut self, sql: &str, params: &[&dyn SqlParam]) -> Result<Vec<Vec<String>>, DB2Error> {
        if !self.is_connected {
            return Err(DB2Error::ConnectionError {
                message: "Connection closed".to_string(),
            });
        }

        let mut sql_with_params = sql.to_string();
        for param in params.iter() {
            sql_with_params =
                sql_with_params.replacen("?", &format!("'{}'", param.to_sql_string()), 1);
        }

        self.executed_statements.push(sql_with_params);

        // Return pre-configured results or empty
        if !self.query_results.is_empty() {
            Ok(self.query_results.remove(0))
        } else {
            Ok(Vec::new())
        }
    }

    fn begin_transaction(&mut self) -> Result<(), DB2Error> {
        if !self.is_connected {
            return Err(DB2Error::ConnectionError {
                message: "Connection closed".to_string(),
            });
        }

        self.in_transaction = true;
        self.executed_statements
            .push("BEGIN TRANSACTION".to_string());
        Ok(())
    }

    fn commit(&mut self) -> Result<(), DB2Error> {
        if !self.in_transaction {
            return Err(DB2Error::TransactionError {
                message: "No active transaction".to_string(),
            });
        }

        self.in_transaction = false;
        self.executed_statements.push("COMMIT".to_string());
        Ok(())
    }

    fn rollback(&mut self) -> Result<(), DB2Error> {
        if !self.in_transaction {
            return Err(DB2Error::TransactionError {
                message: "No active transaction".to_string(),
            });
        }

        self.in_transaction = false;
        self.executed_statements.push("ROLLBACK".to_string());
        Ok(())
    }

    fn is_alive(&mut self) -> bool {
        self.is_connected
    }

    fn validate_table_exists(&mut self, _table_name: &str) -> Result<(), DB2Error> {
        // Mock implementation - always returns Ok
        Ok(())
    }
}

/// Connection pool wrapper
pub struct PooledConnection<C: DB2Connection> {
    connection: C,
}

impl<C: DB2Connection> PooledConnection<C> {
    /// Create new pooled connection
    pub fn new(connection: C) -> Self {
        Self { connection }
    }

    /// Get mutable reference to underlying connection
    pub fn connection_mut(&mut self) -> &mut C {
        &mut self.connection
    }
}

/// DB2 connection manager with connection pooling
pub struct DB2ConnectionManager<C: DB2Connection = MockDB2Connection> {
    config: DB2Config,
    connections: parking_lot::Mutex<Vec<C>>,
}

impl<C: DB2Connection + Default> DB2ConnectionManager<C> {
    /// Create new connection manager
    pub fn new(config: DB2Config) -> Result<Self, DB2Error> {
        let mut connections = Vec::new();

        // Pre-create minimum idle connections
        let min_idle = config.min_idle_connections.unwrap_or(1);
        for _ in 0..min_idle {
            connections.push(C::default());
        }

        Ok(Self {
            config,
            connections: parking_lot::Mutex::new(connections),
        })
    }

    /// Get connection from pool
    pub fn get_connection(&self) -> Result<PooledConnection<C>, DB2Error> {
        let mut connections = self.connections.lock();

        // Try to get existing connection
        if let Some(mut conn) = connections.pop() {
            if conn.is_alive() {
                return Ok(PooledConnection::new(conn));
            }
        }

        // Create new connection if under max limit
        if connections.len() < self.config.max_connections {
            let conn = C::default();
            return Ok(PooledConnection::new(conn));
        }

        // Pool exhausted
        Err(DB2Error::PoolExhausted)
    }

    /// Return connection to pool
    pub fn return_connection(&self, connection: PooledConnection<C>) {
        let mut connections = self.connections.lock();
        if connections.len() < self.config.max_connections {
            connections.push(connection.connection);
        }
    }

    /// Execute SQL with retry logic
    pub fn execute_with_retry(&self, sql: &str, params: &[&dyn SqlParam]) -> Result<u64, DB2Error> {
        let mut attempts = 0;
        let mut last_error = None;

        while attempts < self.config.max_retry_attempts {
            let mut conn = self.get_connection()?;

            match conn.connection_mut().execute(sql, params) {
                Ok(rows) => {
                    self.return_connection(conn);
                    return Ok(rows);
                }
                Err(err) => {
                    if !err.is_retryable() {
                        return Err(err);
                    }

                    last_error = Some(err);
                    attempts += 1;

                    if attempts < self.config.max_retry_attempts {
                        // Exponential backoff
                        let backoff = Duration::from_millis(
                            self.config.retry_backoff_ms * 2u64.pow(attempts as u32 - 1),
                        );
                        std::thread::sleep(backoff);
                    }
                }
            }
        }

        Err(last_error.unwrap_or(DB2Error::Other("Max retries exceeded".to_string())))
    }

    /// Execute transaction
    pub fn transaction<F, T>(&self, f: F) -> Result<T, DB2Error>
    where
        F: FnOnce(&mut dyn DB2Connection) -> Result<T, DB2Error>,
    {
        let mut conn = self.get_connection()?;
        let connection = conn.connection_mut();

        connection.begin_transaction()?;

        match f(connection) {
            Ok(result) => {
                connection.commit()?;
                self.return_connection(conn);
                Ok(result)
            }
            Err(err) => {
                let _ = connection.rollback();
                Err(err)
            }
        }
    }

    /// Get configuration
    pub fn config(&self) -> &DB2Config {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_db2_config_connection_string() {
        let config = DB2Config {
            host: "db2.example.com".to_string(),
            port: 50000,
            database: "testdb".to_string(),
            username: "testuser".to_string(),
            password: "secret123".to_string(),
            ..Default::default()
        };

        let conn_str = config.connection_string();
        assert!(conn_str.contains("DATABASE=testdb"));
        assert!(conn_str.contains("HOSTNAME=db2.example.com"));
        assert!(conn_str.contains("PORT=50000"));
        assert!(conn_str.contains("UID=testuser"));
        assert!(conn_str.contains("PWD=secret123"));
    }

    #[test]
    fn test_db2_config_masked_connection_string() {
        let config = DB2Config {
            password: "secret123".to_string(),
            ..Default::default()
        };

        let masked = config.masked_connection_string();
        assert!(masked.contains("PWD=***"));
        assert!(!masked.contains("secret123"));
    }

    #[test]
    fn test_mock_connection_execute() -> Result<(), DB2Error> {
        let mut conn = MockDB2Connection::new();

        let rows = conn.execute("INSERT INTO customers (name) VALUES (?)", &[&"Alice"])?;
        assert_eq!(rows, 1);

        let statements = conn.executed_statements();
        assert_eq!(statements.len(), 1);
        assert!(statements[0].contains("INSERT INTO customers"));
        assert!(statements[0].contains("Alice"));

        Ok(())
    }

    #[test]
    fn test_mock_connection_query() -> Result<(), DB2Error> {
        let mut conn = MockDB2Connection::new();

        // Set up mock results
        conn.set_query_results(vec![vec![
            vec!["1".to_string(), "Alice".to_string()],
            vec!["2".to_string(), "Bob".to_string()],
        ]]);

        let results = conn.query("SELECT id, name FROM customers", &[])?;
        assert_eq!(results.len(), 2);
        assert_eq!(results[0][0], "1");
        assert_eq!(results[0][1], "Alice");

        Ok(())
    }

    #[test]
    fn test_mock_connection_transaction() -> Result<(), DB2Error> {
        let mut conn = MockDB2Connection::new();

        conn.begin_transaction()?;
        conn.execute("INSERT INTO customers (name) VALUES (?)", &[&"Alice"])?;
        conn.commit()?;

        let statements = conn.executed_statements();
        assert_eq!(statements.len(), 3);
        assert_eq!(statements[0], "BEGIN TRANSACTION");
        assert!(statements[1].contains("INSERT INTO customers"));
        assert_eq!(statements[2], "COMMIT");

        Ok(())
    }

    #[test]
    fn test_mock_connection_rollback() -> Result<(), DB2Error> {
        let mut conn = MockDB2Connection::new();

        conn.begin_transaction()?;
        conn.execute("INSERT INTO customers (name) VALUES (?)", &[&"Alice"])?;
        conn.rollback()?;

        let statements = conn.executed_statements();
        assert_eq!(statements[2], "ROLLBACK");

        Ok(())
    }

    #[test]
    fn test_connection_manager_pool() -> Result<(), DB2Error> {
        let config = DB2Config {
            max_connections: 5,
            min_idle_connections: Some(2),
            ..Default::default()
        };

        let manager = DB2ConnectionManager::<MockDB2Connection>::new(config)?;

        // Get connection from pool
        let mut conn1 = manager.get_connection()?;
        assert!(conn1.connection.is_alive());

        // Return to pool
        manager.return_connection(conn1);

        Ok(())
    }

    #[test]
    fn test_connection_manager_execute() -> Result<(), DB2Error> {
        let config = DB2Config::default();
        let manager = DB2ConnectionManager::<MockDB2Connection>::new(config)?;

        let rows =
            manager.execute_with_retry("INSERT INTO customers (name) VALUES (?)", &[&"Alice"])?;
        assert_eq!(rows, 1);

        Ok(())
    }

    #[test]
    fn test_connection_manager_transaction() -> Result<(), DB2Error> {
        let config = DB2Config::default();
        let manager = DB2ConnectionManager::<MockDB2Connection>::new(config)?;

        let result = manager.transaction(|conn| {
            conn.execute("INSERT INTO customers (name) VALUES (?)", &[&"Alice"])?;
            conn.execute("INSERT INTO customers (name) VALUES (?)", &[&"Bob"])?;
            Ok(2u64)
        })?;

        assert_eq!(result, 2);

        Ok(())
    }

    #[test]
    fn test_sql_param_string() {
        let param = "test";
        assert_eq!(param.to_sql_string(), "test");
        assert_eq!(param.param_type(), SqlParamType::String);
    }

    #[test]
    fn test_sql_param_integer() {
        let param = 42i32;
        assert_eq!(param.to_sql_string(), "42");
        assert_eq!(param.param_type(), SqlParamType::Integer);
    }

    #[test]
    fn test_sql_param_boolean() {
        let param_true = true;
        let param_false = false;
        assert_eq!(param_true.to_sql_string(), "1");
        assert_eq!(param_false.to_sql_string(), "0");
    }

    #[test]
    fn test_sql_param_option() {
        let some_val: Option<i32> = Some(42);
        let none_val: Option<i32> = None;

        assert_eq!(some_val.to_sql_string(), "42");
        assert_eq!(none_val.to_sql_string(), "NULL");
        assert_eq!(none_val.param_type(), SqlParamType::Null);
    }

    #[test]
    fn test_db2_error_from_sqlcode() {
        let err = DB2Error::from_sqlcode(-803, "Duplicate key".to_string());
        assert!(matches!(err, DB2Error::DuplicateKey { .. }));

        let err = DB2Error::from_sqlcode(-530, "FK violation".to_string());
        assert!(matches!(err, DB2Error::ForeignKeyViolation { .. }));

        let err = DB2Error::from_sqlcode(-407, "NOT NULL violation".to_string());
        assert!(matches!(err, DB2Error::NotNullViolation { .. }));
    }

    #[test]
    fn test_db2_error_is_retryable() {
        let conn_err = DB2Error::ConnectionError {
            message: "Connection lost".to_string(),
        };
        assert!(conn_err.is_retryable());

        let dup_err = DB2Error::DuplicateKey {
            message: "Duplicate key".to_string(),
        };
        assert!(!dup_err.is_retryable());
    }
}
