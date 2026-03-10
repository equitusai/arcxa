//! ODBC-based DB2 Connection Implementation
//!
//! Production implementation of DB2Connection trait using ODBC.
//! Requires IBM DB2 ODBC drivers to be installed on the system.
//!
//! ## Installation (Linux)
//!
//! ```bash
//! # Download DB2 ODBC drivers from IBM
//! # Or use Docker image with drivers pre-installed
//! docker run -d --name db2 -e LICENSE=accept ibmcom/db2
//! ```
//!
//! ## Usage
//!
//! ```rust,ignore
//! use graphica_coordinator::mapping::loader::{DB2Config, OdbcDB2Connection, DB2Connection};
//!
//! let config = DB2Config {
//!     host: "localhost".to_string(),
//!     port: 50000,
//!     database: "GRAPHICA".to_string(),
//!     username: "db2inst1".to_string(),
//!     password: "password".to_string(),
//!     ..Default::default()
//! };
//!
//! let mut conn = OdbcDB2Connection::connect(&config)?;
//! let rows = conn.execute("INSERT INTO customers (name) VALUES (?)", &[&"Alice"])?;
//! ```

use super::{DB2Config, DB2Connection, DB2Error, SqlParam, SqlParamType};
use anyhow::Result;

#[cfg(feature = "odbc")]
use odbc_api::{
    Connection, ConnectionOptions, Cursor, Environment, Error as OdbcError, ResultSetMetadata,
};
#[cfg(feature = "odbc")]
use std::sync::{Arc, Mutex};

/// ODBC-based DB2 connection
///
/// This is the production implementation that actually connects to DB2 via ODBC.
/// Requires the `odbc` feature flag to be enabled.
///
/// Thread Safety: Uses Arc<Mutex<>> internally for Send + Sync compliance.
#[cfg(feature = "odbc")]
pub struct OdbcDB2Connection {
    /// Connection state (Arc<Mutex<>> for thread safety)
    state: Arc<Mutex<ConnectionState>>,
}

#[cfg(feature = "odbc")]
struct ConnectionState {
    /// ODBC environment (leaked for 'static lifetime)
    _env_ptr: *const Environment,

    /// ODBC connection
    ///
    /// Wrapped in Option so Drop can take and drop the connection explicitly
    /// before releasing the leaked ODBC environment pointer.
    connection: Option<Connection<'static>>,

    /// Whether we're in a transaction
    in_transaction: bool,
}

// SAFETY: We manually ensure thread safety via Mutex
#[cfg(feature = "odbc")]
unsafe impl Send for ConnectionState {}
#[cfg(feature = "odbc")]
unsafe impl Sync for ConnectionState {}

#[cfg(feature = "odbc")]
impl OdbcDB2Connection {
    /// Connect to DB2 using ODBC
    pub fn connect(config: &DB2Config) -> Result<Self, DB2Error> {
        // Create ODBC environment
        let env = Box::new(Environment::new().map_err(|e| DB2Error::ConnectionError {
            message: format!("Failed to create ODBC environment: {:?}", e),
        })?);

        // Build connection string
        let conn_string = config.connection_string();

        tracing::debug!(
            "Connecting to DB2: {}",
            conn_string.replace(&config.password, "***")
        );

        // SAFETY: We're leaking the environment box to get a 'static lifetime
        // The environment will be cleaned up in Drop
        let env_ptr: *const Environment = Box::leak(env);
        let env_ref: &'static Environment = unsafe { &*env_ptr };

        // Connect to database
        let connection = env_ref
            .connect_with_connection_string(&conn_string, ConnectionOptions::default())
            .map_err(|e| {
                // Clean up leaked environment on error
                unsafe {
                    let _ = Box::from_raw(env_ptr as *mut Environment);
                }
                let msg = Self::format_odbc_error(&e);
                DB2Error::ConnectionError {
                    message: format!("Failed to connect to DB2: {}", msg),
                }
            })?;

        // Disable auto-commit if configured
        if !config.auto_commit {
            connection
                .set_autocommit(false)
                .map_err(|e| DB2Error::TransactionError {
                    message: format!("Failed to disable autocommit: {:?}", e),
                })?;
        }

        let state = ConnectionState {
            _env_ptr: env_ptr,
            connection: Some(connection),
            in_transaction: false,
        };

        Ok(Self {
            state: Arc::new(Mutex::new(state)),
        })
    }

    /// Format ODBC error into readable error message
    fn format_odbc_error(err: &OdbcError) -> String {
        format!("{:?}", err)
    }

    /// Extract SQLCODE from ODBC error message if present
    fn extract_sqlcode(err: &OdbcError) -> i32 {
        // Try to extract SQLCODE from error message
        let msg = format!("{:?}", err);
        if let Some(start) = msg.find("SQLCODE=") {
            let code_str = &msg[start + 8..];
            if let Some(end) = code_str.find(|c: char| !c.is_numeric() && c != '-') {
                return code_str[..end].parse().unwrap_or(-1);
            }
        }
        -1 // Default unknown SQLCODE
    }

    /// Render a SQL parameter safely for string interpolation fallback.
    ///
    /// NOTE: This path is a compatibility fallback while proper native parameter
    /// binding is being improved. It escapes string values and preserves null/type semantics.
    fn render_param_for_sql(param: &dyn SqlParam) -> String {
        match param.param_type() {
            SqlParamType::Null => "NULL".to_string(),
            SqlParamType::Integer | SqlParamType::Decimal | SqlParamType::Boolean => {
                param.to_sql_string()
            }
            SqlParamType::String | SqlParamType::Date | SqlParamType::Timestamp => {
                let escaped = param.to_sql_string().replace('\'', "''");
                format!("'{}'", escaped)
            }
        }
    }

    /// Validate table exists in DB2 catalog
    ///
    /// Pre-validates table existence to avoid SQL errors that leave ODBC handles in invalid state.
    /// This is a defensive measure to prevent coordinator crashes from ODBC panic bugs.
    pub fn validate_table_exists(&mut self, table_name: &str) -> Result<(), DB2Error> {
        tracing::debug!("Pre-validating table existence: {}", table_name);

        // Parse schema and table from qualified name
        let (schema, table) = if let Some(dot_pos) = table_name.find('.') {
            let schema = &table_name[..dot_pos];
            let table = &table_name[dot_pos + 1..];
            (schema.trim_matches('"'), table.trim_matches('"'))
        } else {
            // If no schema specified, assume default schema
            ("DB2INST1", table_name.trim_matches('"'))
        };

        let check_sql = format!(
            "SELECT 1 FROM SYSCAT.TABLES WHERE UPPER(TABSCHEMA) = UPPER('{}') AND UPPER(TABNAME) = UPPER('{}')",
            schema, table
        );

        tracing::debug!("Table existence check SQL: {}", check_sql);

        // Use mutable guard like other methods for cursor lifetime compatibility
        let mut state = self.state.lock().unwrap();

        // Process cursor and store result before dropping guard (avoids E0597 lifetime issue)
        let connection = state
            .connection
            .as_mut()
            .ok_or_else(|| DB2Error::ConnectionError {
                message: "ODBC connection is closed".to_string(),
            })?;

        let result = match connection.execute(&check_sql, (), None) {
            Ok(Some(mut cursor)) => {
                // Use ? operator to allow early return while guard is held (like execute() method)
                match cursor.next_row().map_err(|e| DB2Error::QueryError {
                    sqlcode: Self::extract_sqlcode(&e),
                    message: format!("Failed to check table existence: {:?}", e),
                })? {
                    Some(_) => {
                        tracing::debug!("Table {}.{} exists - validation passed", schema, table);
                        Ok(())
                    }
                    None => {
                        let error_msg = format!(
                            "Target table does not exist (SQLSTATE 42704): {}.{}\nPlease create the table before attempting to load data.",
                            schema, table
                        );
                        tracing::error!("{}", error_msg);
                        Err(DB2Error::QueryError {
                            sqlcode: -204, // SQL0204N - undefined name
                            message: error_msg,
                        })
                    }
                }
            }
            Ok(None) | Err(_) => {
                tracing::warn!("Table existence check query failed - proceeding anyway");
                Ok(()) // Fail open
            }
        };

        // Explicitly drop guard before returning to satisfy lifetime checker
        drop(state);
        result
    }
}

#[cfg(feature = "odbc")]
impl DB2Connection for OdbcDB2Connection {
    fn execute(&mut self, sql: &str, params: &[&dyn SqlParam]) -> Result<u64, DB2Error> {
        tracing::debug!("Executing SQL: {} (params: {})", sql, params.len());

        let mut state = self.state.lock().unwrap();

        let connection = state
            .connection
            .as_mut()
            .ok_or_else(|| DB2Error::ConnectionError {
                message: "ODBC connection is closed".to_string(),
            })?;

        if params.is_empty() {
            // No parameters - execute directly
            match connection.execute(sql, (), None) {
                Ok(Some(mut cursor)) => {
                    // Query returned results - count rows
                    let mut count = 0u64;
                    while let Some(_row) = cursor.next_row().map_err(|e| DB2Error::QueryError {
                        sqlcode: Self::extract_sqlcode(&e),
                        message: format!("Failed to fetch row: {:?}", e),
                    })? {
                        count += 1;
                    }
                    Ok(count)
                }
                Ok(None) => {
                    // No results - likely DML statement, get row count
                    Ok(0) // ODBC-API doesn't provide row count easily, return 0
                }
                Err(e) => {
                    let sqlcode = Self::extract_sqlcode(&e);
                    let msg = Self::format_odbc_error(&e);
                    Err(DB2Error::QueryError {
                        sqlcode,
                        message: format!("SQL execution failed: {}", msg),
                    })
                }
            }
        } else {
            // With parameters - convert to string interpolation for now
            // TODO: Use proper parameter binding when odbc-api supports it better
            let mut sql_with_params = sql.to_string();
            for param in params {
                sql_with_params =
                    sql_with_params.replacen("?", &Self::render_param_for_sql(*param), 1);
            }

            tracing::debug!("Interpolated SQL: {}", sql_with_params);

            match connection.execute(&sql_with_params, (), None) {
                Ok(Some(mut cursor)) => {
                    let mut count = 0u64;
                    while let Some(_row) = cursor.next_row().map_err(|e| DB2Error::QueryError {
                        sqlcode: Self::extract_sqlcode(&e),
                        message: format!("Failed to fetch row: {:?}", e),
                    })? {
                        count += 1;
                    }
                    Ok(count)
                }
                Ok(None) => Ok(0),
                Err(e) => {
                    let sqlcode = Self::extract_sqlcode(&e);
                    let msg = Self::format_odbc_error(&e);
                    Err(DB2Error::QueryError {
                        sqlcode,
                        message: format!("SQL execution failed: {}", msg),
                    })
                }
            }
        }
    }

    fn query(&mut self, sql: &str, params: &[&dyn SqlParam]) -> Result<Vec<Vec<String>>, DB2Error> {
        tracing::debug!("Querying SQL: {} (params: {})", sql, params.len());

        let mut state = self.state.lock().unwrap();

        // Handle parameters by string interpolation (simplified approach)
        let sql_to_execute = if params.is_empty() {
            sql.to_string()
        } else {
            let mut sql_with_params = sql.to_string();
            for param in params {
                sql_with_params =
                    sql_with_params.replacen("?", &Self::render_param_for_sql(*param), 1);
            }
            sql_with_params
        };

        let connection = state
            .connection
            .as_mut()
            .ok_or_else(|| DB2Error::ConnectionError {
                message: "ODBC connection is closed".to_string(),
            })?;

        // Execute query
        let mut cursor = connection
            .execute(&sql_to_execute, (), None)
            .map_err(|e| {
                let sqlcode = Self::extract_sqlcode(&e);
                let msg = Self::format_odbc_error(&e);
                DB2Error::QueryError {
                    sqlcode,
                    message: format!("Query execution failed: {}", msg),
                }
            })?
            .ok_or_else(|| DB2Error::QueryError {
                sqlcode: -1,
                message: "Query did not return results".to_string(),
            })?;

        // Fetch results
        let mut rows = Vec::new();

        // Get column count
        let num_cols = cursor.num_result_cols().map_err(|e| DB2Error::QueryError {
            sqlcode: Self::extract_sqlcode(&e),
            message: format!("Failed to get column count: {:?}", e),
        })? as usize;

        // Iterate through rows
        while let Some(mut row) = cursor.next_row().map_err(|e| DB2Error::QueryError {
            sqlcode: Self::extract_sqlcode(&e),
            message: format!("Failed to fetch row: {:?}", e),
        })? {
            let mut row_data = Vec::new();

            for col_idx in 1..=num_cols {
                // Get column value as string - need buffer for get_text()
                let mut buffer = Vec::new();
                let not_null = row.get_text(col_idx as u16, &mut buffer).map_err(|e| {
                    DB2Error::QueryError {
                        sqlcode: Self::extract_sqlcode(&e),
                        message: format!("Failed to get column {}: {:?}", col_idx, e),
                    }
                })?;

                if not_null {
                    // Convert buffer to UTF-8 string
                    let value = String::from_utf8_lossy(&buffer).to_string();
                    row_data.push(value);
                } else {
                    row_data.push("NULL".to_string());
                }
            }

            rows.push(row_data);
        }

        Ok(rows)
    }

    fn begin_transaction(&mut self) -> Result<(), DB2Error> {
        let mut state = self.state.lock().unwrap();

        if state.in_transaction {
            return Err(DB2Error::TransactionError {
                message: "Transaction already in progress".to_string(),
            });
        }

        // Disable autocommit to start transaction
        let connection = state
            .connection
            .as_mut()
            .ok_or_else(|| DB2Error::ConnectionError {
                message: "ODBC connection is closed".to_string(),
            })?;

        connection
            .set_autocommit(false)
            .map_err(|e| DB2Error::TransactionError {
                message: format!("Failed to begin transaction: {:?}", e),
            })?;

        state.in_transaction = true;
        tracing::debug!("Transaction started");
        Ok(())
    }

    fn commit(&mut self) -> Result<(), DB2Error> {
        let mut state = self.state.lock().unwrap();

        if !state.in_transaction {
            return Err(DB2Error::TransactionError {
                message: "No transaction in progress".to_string(),
            });
        }

        let connection = state
            .connection
            .as_mut()
            .ok_or_else(|| DB2Error::ConnectionError {
                message: "ODBC connection is closed".to_string(),
            })?;

        connection
            .commit()
            .map_err(|e| DB2Error::TransactionError {
                message: format!("Failed to commit transaction: {:?}", e),
            })?;

        state.in_transaction = false;
        tracing::debug!("Transaction committed");
        Ok(())
    }

    fn rollback(&mut self) -> Result<(), DB2Error> {
        let mut state = self.state.lock().unwrap();

        if !state.in_transaction {
            return Err(DB2Error::TransactionError {
                message: "No transaction in progress".to_string(),
            });
        }

        let connection = state
            .connection
            .as_mut()
            .ok_or_else(|| DB2Error::ConnectionError {
                message: "ODBC connection is closed".to_string(),
            })?;

        connection
            .rollback()
            .map_err(|e| DB2Error::TransactionError {
                message: format!("Failed to rollback transaction: {:?}", e),
            })?;

        state.in_transaction = false;
        tracing::debug!("Transaction rolled back");
        Ok(())
    }

    fn is_alive(&mut self) -> bool {
        let state = self.state.lock().unwrap();
        let Some(connection) = state.connection.as_ref() else {
            return false;
        };

        // Try a simple query to check if connection is alive
        let result = connection.execute("SELECT 1 FROM SYSIBM.SYSDUMMY1", (), None);
        result.is_ok()
    }

    fn validate_table_exists(&mut self, table_name: &str) -> Result<(), DB2Error> {
        // Delegate to the standalone method implementation
        Self::validate_table_exists(self, table_name)
    }
}

#[cfg(feature = "odbc")]
impl Drop for ConnectionState {
    fn drop(&mut self) {
        // Drop the ODBC connection first, while the environment is still valid.
        if let Some(connection) = self.connection.take() {
            let drop_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                drop(connection);
            }));

            if let Err(panic_info) = drop_result {
                tracing::error!("ODBC library panicked while dropping DB2 connection");
                if let Some(s) = panic_info.downcast_ref::<&str>() {
                    tracing::error!("Panic message: {}", s);
                } else if let Some(s) = panic_info.downcast_ref::<String>() {
                    tracing::error!("Panic message: {}", s);
                }
            }
        }

        // Finally release the leaked environment pointer.
        if !self._env_ptr.is_null() {
            let env_ptr = self._env_ptr;
            self._env_ptr = std::ptr::null();

            let env_drop_result =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
                    drop(Box::from_raw(env_ptr as *mut Environment));
                }));

            if let Err(panic_info) = env_drop_result {
                tracing::error!("ODBC library panicked while dropping DB2 environment");
                if let Some(s) = panic_info.downcast_ref::<&str>() {
                    tracing::error!("Panic message: {}", s);
                } else if let Some(s) = panic_info.downcast_ref::<String>() {
                    tracing::error!("Panic message: {}", s);
                }
            }
        }
    }
}

#[cfg(feature = "odbc")]
impl Drop for OdbcDB2Connection {
    fn drop(&mut self) {
        // Rollback any pending transaction
        let should_rollback = self
            .state
            .lock()
            .map(|state| state.in_transaction)
            .unwrap_or(false);

        if should_rollback {
            let rollback_result =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.rollback()));

            match rollback_result {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    tracing::warn!("Failed to rollback transaction on drop: {:?}", e);
                }
                Err(_) => {
                    tracing::warn!("ODBC panic while rolling back transaction during drop");
                }
            }
        }
        tracing::debug!("ODBC DB2 connection dropped");
    }
}

// ============================================================================
// Stub Implementation (when ODBC feature is disabled)
// ============================================================================

#[cfg(not(feature = "odbc"))]
pub struct OdbcDB2Connection;

#[cfg(not(feature = "odbc"))]
impl OdbcDB2Connection {
    pub fn connect(_config: &DB2Config) -> Result<Self, DB2Error> {
        Err(DB2Error::ConnectionError {
            message: "ODBC support not compiled. Enable the 'odbc' feature flag.".to_string(),
        })
    }
}

#[cfg(not(feature = "odbc"))]
impl DB2Connection for OdbcDB2Connection {
    fn execute(&mut self, _sql: &str, _params: &[&dyn SqlParam]) -> Result<u64, DB2Error> {
        Err(DB2Error::QueryError {
            sqlcode: -1, // Sentinel value indicating feature not compiled
            message: "ODBC support not compiled".to_string(),
        })
    }

    fn query(
        &mut self,
        _sql: &str,
        _params: &[&dyn SqlParam],
    ) -> Result<Vec<Vec<String>>, DB2Error> {
        Err(DB2Error::QueryError {
            sqlcode: -1, // Sentinel value indicating feature not compiled
            message: "ODBC support not compiled".to_string(),
        })
    }

    fn begin_transaction(&mut self) -> Result<(), DB2Error> {
        Err(DB2Error::TransactionError {
            message: "ODBC support not compiled".to_string(),
        })
    }

    fn commit(&mut self) -> Result<(), DB2Error> {
        Err(DB2Error::TransactionError {
            message: "ODBC support not compiled".to_string(),
        })
    }

    fn rollback(&mut self) -> Result<(), DB2Error> {
        Err(DB2Error::TransactionError {
            message: "ODBC support not compiled".to_string(),
        })
    }

    fn is_alive(&mut self) -> bool {
        false
    }

    fn validate_table_exists(&mut self, _table_name: &str) -> Result<(), DB2Error> {
        Err(DB2Error::QueryError {
            sqlcode: -1,
            message: "ODBC support not compiled".to_string(),
        })
    }
}
