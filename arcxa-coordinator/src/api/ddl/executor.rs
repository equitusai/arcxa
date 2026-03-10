//! DDL Execution Engine
//!
//! Executes DDL statements against target databases with connection pooling,
//! transaction support, and comprehensive error handling.

use super::types::{DatabaseConnectionConfig, DatabaseType, DdlExecutionError};
use tracing::{debug, error, info, warn};

#[cfg(feature = "odbc")]
use odbc_api::{Connection, ConnectionOptions, Cursor, Environment};

/// Result type for DDL execution
pub type ExecutionResult = Result<ExecutionStats, Vec<DdlExecutionError>>;

/// Statistics from DDL execution
#[derive(Debug, Clone)]
pub struct ExecutionStats {
    pub statements_executed: usize,
    pub tables_affected: usize,
    pub execution_time_ms: u64,
}

/// Trait for database-specific DDL executors
#[async_trait::async_trait]
pub trait DdlExecutor: Send + Sync {
    /// Execute DDL statements against the target database
    async fn execute(
        &self,
        statements: Vec<String>,
        transactional: bool,
        continue_on_error: bool,
    ) -> ExecutionResult;

    /// Test connection to the database
    async fn test_connection(&self) -> Result<(), String>;

    /// Check if a table exists in the database
    async fn table_exists(&self, table_name: &str) -> Result<bool, String>;

    /// Get database type
    fn database_type(&self) -> DatabaseType;
}

/// DB2 DDL Executor using ODBC
#[cfg(feature = "odbc")]
pub struct Db2DdlExecutor {
    connection_string: String,
}

#[cfg(feature = "odbc")]
impl Db2DdlExecutor {
    /// Create new DB2 executor
    pub fn new(config: &DatabaseConnectionConfig) -> Result<Self, String> {
        // Build DB2 connection string
        let connection_string = format!(
            "DRIVER={{IBM DB2 ODBC DRIVER}};DATABASE={};HOSTNAME={};PORT={};PROTOCOL=TCPIP;UID={};PWD={}",
            config.database,
            config.host,
            config.port,
            config.username,
            config.password
        );

        // Add any additional options
        let mut full_connection_string = connection_string;
        for (key, value) in &config.options {
            full_connection_string.push_str(&format!(";{}={}", key, value));
        }

        Ok(Self {
            connection_string: full_connection_string,
        })
    }

    /// Create connection to database
    fn create_connection(&self) -> Result<Connection<'static>, String> {
        // Create environment and leak it to get 'static lifetime
        // This is acceptable for DDL operations which are infrequent
        let env =
            Box::leak(Box::new(Environment::new().map_err(|e| {
                format!("Failed to create ODBC environment: {}", e)
            })?));

        env.connect_with_connection_string(&self.connection_string, ConnectionOptions::default())
            .map_err(|e| format!("Failed to connect to DB2: {}", e))
    }

    /// Execute a single statement
    fn execute_statement(conn: &Connection, statement: &str) -> Result<usize, String> {
        debug!("Executing DDL: {}", statement);

        match conn.execute(statement, (), None) {
            Ok(cursor_opt) => {
                // DDL statements typically don't return cursors
                if let Some(cursor) = cursor_opt {
                    drop(cursor);
                }
                info!("DDL executed successfully");
                Ok(0) // DDL operations don't have meaningful row counts
            }
            Err(e) => {
                error!("Failed to execute DDL: {}", e);
                Err(format!("Execution error: {}", e))
            }
        }
    }

    /// Count tables affected by parsing DDL statements
    fn count_affected_tables(statements: &[String]) -> usize {
        let mut tables = std::collections::HashSet::new();

        for stmt in statements {
            let upper = stmt.to_uppercase();

            // Extract table name from CREATE TABLE
            if let Some(idx) = upper.find("CREATE TABLE") {
                let after = &stmt[idx + 12..];
                if let Some(table_name) = after.trim().split_whitespace().next() {
                    tables.insert(
                        table_name
                            .trim_matches(|c| c == '(' || c == '"')
                            .to_string(),
                    );
                }
            }

            // Extract table name from ALTER TABLE
            if let Some(idx) = upper.find("ALTER TABLE") {
                let after = &stmt[idx + 11..];
                if let Some(table_name) = after.trim().split_whitespace().next() {
                    tables.insert(
                        table_name
                            .trim_matches(|c| c == '(' || c == '"')
                            .to_string(),
                    );
                }
            }

            // Extract table name from DROP TABLE
            if let Some(idx) = upper.find("DROP TABLE") {
                let after = &stmt[idx + 10..];
                if let Some(table_name) = after.trim().split_whitespace().next() {
                    tables.insert(
                        table_name
                            .trim_matches(|c| c == '(' || c == '"')
                            .to_string(),
                    );
                }
            }
        }

        tables.len()
    }
}

#[cfg(feature = "odbc")]
#[async_trait::async_trait]
impl DdlExecutor for Db2DdlExecutor {
    async fn execute(
        &self,
        statements: Vec<String>,
        transactional: bool,
        continue_on_error: bool,
    ) -> ExecutionResult {
        let start_time = std::time::Instant::now();
        let mut errors = Vec::new();
        let mut statements_executed = 0;

        info!(
            "Executing {} DDL statements (transactional: {}, continue_on_error: {})",
            statements.len(),
            transactional,
            continue_on_error
        );

        // Connect to database
        let conn = match self.create_connection() {
            Ok(conn) => conn,
            Err(e) => {
                error!("Failed to connect to DB2: {}", e);
                return Err(vec![DdlExecutionError {
                    statement_index: 0,
                    statement: "CONNECTION".to_string(),
                    error: format!("Failed to connect to database: {}", e),
                    sql_state: None,
                }]);
            }
        };

        // Set autocommit based on transaction mode
        if transactional {
            if let Err(e) = conn.set_autocommit(false) {
                error!("Failed to disable autocommit: {}", e);
                return Err(vec![DdlExecutionError {
                    statement_index: 0,
                    statement: "SET AUTOCOMMIT".to_string(),
                    error: format!("Failed to configure transaction mode: {}", e),
                    sql_state: None,
                }]);
            }
        }

        // Execute statements
        for (idx, statement) in statements.iter().enumerate() {
            match Self::execute_statement(&conn, statement) {
                Ok(_) => {
                    statements_executed += 1;
                    debug!("Statement {} executed successfully", idx);
                }
                Err(error_msg) => {
                    error!("Statement {} failed: {}", idx, error_msg);

                    errors.push(DdlExecutionError {
                        statement_index: idx,
                        statement: statement.clone(),
                        error: error_msg,
                        sql_state: None, // Could extract from ODBC diagnostics
                    });

                    // Handle error based on mode
                    if transactional {
                        // Rollback and abort
                        info!("Rolling back transaction due to error");
                        if let Err(e) = conn.rollback() {
                            error!("Rollback failed: {}", e);
                        }
                        return Err(errors);
                    } else if !continue_on_error {
                        // Abort but no rollback
                        return Err(errors);
                    }
                    // Otherwise continue to next statement
                }
            }
        }

        // Commit if transactional and no errors
        if transactional {
            match conn.commit() {
                Ok(_) => {
                    info!("Transaction committed successfully");
                }
                Err(e) => {
                    error!("Commit failed: {}", e);
                    if let Err(e) = conn.rollback() {
                        error!("Rollback after commit failure failed: {}", e);
                    }
                    return Err(vec![DdlExecutionError {
                        statement_index: statements.len(),
                        statement: "COMMIT".to_string(),
                        error: format!("Failed to commit transaction: {}", e),
                        sql_state: None,
                    }]);
                }
            }
        }

        let execution_time_ms = start_time.elapsed().as_millis() as u64;
        let tables_affected = Self::count_affected_tables(&statements);

        // Return results
        if errors.is_empty() {
            info!(
                "DDL execution completed: {} statements, {} tables, {}ms",
                statements_executed, tables_affected, execution_time_ms
            );
            Ok(ExecutionStats {
                statements_executed,
                tables_affected,
                execution_time_ms,
            })
        } else {
            // Partial success in continue_on_error mode
            warn!(
                "DDL execution completed with errors: {}/{} statements succeeded",
                statements_executed,
                statements.len()
            );
            Err(errors)
        }
    }

    async fn test_connection(&self) -> Result<(), String> {
        match self.create_connection() {
            Ok(conn) => {
                // Test with simple query
                match conn.execute("SELECT 1 FROM SYSIBM.SYSDUMMY1", (), None) {
                    Ok(_) => {
                        info!("DB2 connection test successful");
                        Ok(())
                    }
                    Err(e) => {
                        error!("DB2 test query failed: {}", e);
                        Err(format!("Test query failed: {}", e))
                    }
                }
            }
            Err(e) => {
                error!("DB2 connection test failed: {}", e);
                Err(e)
            }
        }
    }

    async fn table_exists(&self, table_name: &str) -> Result<bool, String> {
        use crate::mapping::ddl::dialects::db2::Db2Dialect;
        use crate::mapping::ddl::dialects::SqlDialect;

        let conn = self.create_connection()?;
        let dialect = Db2Dialect;
        let check_sql = dialect.check_table_exists(table_name);

        debug!("Checking if table exists: {}", check_sql);

        let result = conn.execute(&check_sql, (), None);
        match result {
            Ok(Some(mut cursor)) => {
                // If the query returns rows, the table exists
                match cursor.next_row() {
                    Ok(Some(_row)) => {
                        info!("Table {} exists", table_name);
                        Ok(true)
                    }
                    Ok(None) => {
                        info!("Table {} does not exist", table_name);
                        Ok(false)
                    }
                    Err(e) => {
                        error!("Error checking table existence: {}", e);
                        Err(format!("Failed to check table existence: {}", e))
                    }
                }
            }
            Ok(None) => {
                // No cursor means no results
                info!("Table {} does not exist", table_name);
                Ok(false)
            }
            Err(e) => {
                error!("Error executing table existence check: {}", e);
                Err(format!("Failed to execute check query: {}", e))
            }
        }
    }

    fn database_type(&self) -> DatabaseType {
        DatabaseType::DB2
    }
}

/// PostgreSQL DDL Executor
pub struct PostgresqlDdlExecutor {
    connection_string: String,
}

impl PostgresqlDdlExecutor {
    pub fn new(config: &DatabaseConnectionConfig) -> Result<Self, String> {
        let connection_string = format!(
            "host={} port={} dbname={} user={} password={}",
            config.host, config.port, config.database, config.username, config.password
        );

        Ok(Self { connection_string })
    }
}

#[async_trait::async_trait]
impl DdlExecutor for PostgresqlDdlExecutor {
    async fn execute(
        &self,
        _statements: Vec<String>,
        _transactional: bool,
        _continue_on_error: bool,
    ) -> ExecutionResult {
        // TODO: Implement PostgreSQL executor using tokio-postgres
        Err(vec![DdlExecutionError {
            statement_index: 0,
            statement: "N/A".to_string(),
            error: "PostgreSQL executor not yet implemented".to_string(),
            sql_state: None,
        }])
    }

    async fn test_connection(&self) -> Result<(), String> {
        Err("PostgreSQL executor not yet implemented".to_string())
    }

    async fn table_exists(&self, _table_name: &str) -> Result<bool, String> {
        Err("PostgreSQL executor not yet implemented".to_string())
    }

    fn database_type(&self) -> DatabaseType {
        DatabaseType::PostgreSQL
    }
}

/// Factory for creating DdlExecutor instances
pub struct DdlExecutorFactory;

impl DdlExecutorFactory {
    /// Create appropriate executor for the database type
    pub fn create(config: &DatabaseConnectionConfig) -> Result<Box<dyn DdlExecutor>, String> {
        match config.db_type {
            DatabaseType::DB2 => {
                #[cfg(feature = "odbc")]
                {
                    let executor = Db2DdlExecutor::new(config)?;
                    Ok(Box::new(executor))
                }
                #[cfg(not(feature = "odbc"))]
                {
                    Err("DB2 support requires 'odbc' feature to be enabled. Build with: cargo build --features odbc".to_string())
                }
            }
            DatabaseType::PostgreSQL => {
                let executor = PostgresqlDdlExecutor::new(config)?;
                Ok(Box::new(executor))
            }
            DatabaseType::Oracle | DatabaseType::MySQL => {
                Err(format!("{:?} executor not yet implemented", config.db_type))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(feature = "odbc")]
    fn test_count_affected_tables() {
        let statements = vec![
            "CREATE TABLE users (id INT, name VARCHAR(100))".to_string(),
            "CREATE TABLE orders (id INT, user_id INT)".to_string(),
            "ALTER TABLE users ADD COLUMN email VARCHAR(255)".to_string(),
            "CREATE INDEX idx_users_email ON users(email)".to_string(),
        ];

        let count = Db2DdlExecutor::count_affected_tables(&statements);
        assert_eq!(count, 2); // users and orders
    }

    #[test]
    #[cfg(feature = "odbc")]
    fn test_factory_db2() {
        let config = DatabaseConnectionConfig {
            db_type: DatabaseType::DB2,
            host: "localhost".to_string(),
            port: 50000,
            database: "testdb".to_string(),
            username: "user".to_string(),
            password: "pass".to_string(),
            options: Default::default(),
        };

        let executor = DdlExecutorFactory::create(&config);
        assert!(executor.is_ok());
        assert_eq!(executor.unwrap().database_type(), DatabaseType::DB2);
    }
}
