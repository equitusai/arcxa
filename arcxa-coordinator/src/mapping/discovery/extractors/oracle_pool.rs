//! Poolable Oracle ODBC Connection Implementation
//!
//! Implements OdbcPoolableConnection trait for Oracle databases using
//! the leak-and-reclaim pattern to handle ODBC lifetime constraints.

use super::odbc::{OdbcColumnInfo, OdbcPoolableConnection, OdbcQueryResult};
use super::oracle::OracleExtractor;
use anyhow::{anyhow, Result};
use graphica_core::catalog::connector::Credentials;
use graphica_core::catalog::types::DataSource;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[cfg(feature = "odbc")]
use odbc_api::{
    ColumnDescription, Connection, ConnectionOptions, Cursor, DataType, Environment,
    ResultSetMetadata,
};

/// Poolable Oracle ODBC connection
///
/// Uses leak-and-reclaim pattern to handle Environment lifetime:
/// - Environment is leaked to get 'static lifetime
/// - Connection<'static> is created from leaked Environment
/// - Raw pointer stored for manual cleanup in Drop
/// - Arc<Mutex<>> wrapper for Send + Sync
#[cfg(feature = "odbc")]
pub struct OdbcOracleConnection {
    state: Arc<Mutex<OracleConnectionState>>,
}

#[cfg(feature = "odbc")]
struct OracleConnectionState {
    /// ODBC environment (leaked for 'static lifetime)
    _env_ptr: *const Environment,

    /// ODBC connection with 'static lifetime
    ///
    /// Wrapped in Option so Drop can take and drop the connection explicitly
    /// before releasing the leaked ODBC environment pointer.
    connection: Option<Connection<'static>>,

    /// Whether we're in a transaction
    in_transaction: bool,
}

// SAFETY: We manually ensure thread safety via Mutex
#[cfg(feature = "odbc")]
unsafe impl Send for OracleConnectionState {}
#[cfg(feature = "odbc")]
unsafe impl Sync for OracleConnectionState {}

#[cfg(feature = "odbc")]
impl OdbcOracleConnection {
    /// Execute internal query implementation (used by trait methods)
    fn execute_internal(
        &mut self,
        query: &str,
        normalize_headers: bool,
    ) -> Result<Vec<HashMap<String, String>>> {
        let mut state = self
            .state
            .lock()
            .map_err(|e| anyhow!("Failed to lock connection state: {}", e))?;

        let connection = state
            .connection
            .as_ref()
            .ok_or_else(|| anyhow!("Connection is closed"))?;

        let mut cursor = connection
            .execute(query, (), None)
            .map_err(|e| anyhow!("ODBC query failed: {:?}", e))?
            .ok_or_else(|| anyhow!("ODBC query returned no results"))?;

        let num_cols = cursor
            .num_result_cols()
            .map_err(|e| anyhow!("Failed to get column count: {:?}", e))?
            as usize;

        let mut column_names = Vec::with_capacity(num_cols);
        let mut description = ColumnDescription::default();
        for i in 1..=num_cols {
            cursor
                .describe_col(i as u16, &mut description)
                .map_err(|e| anyhow!("Failed to describe column {}: {:?}", i, e))?;
            let name = description
                .name_to_string()
                .unwrap_or_else(|_| format!("col{}", i));
            let name = if normalize_headers {
                name.to_lowercase()
            } else {
                name
            };
            column_names.push(name);
        }

        let mut rows = Vec::new();
        while let Some(mut row) = cursor
            .next_row()
            .map_err(|e| anyhow!("Failed to fetch row: {:?}", e))?
        {
            let mut row_map = HashMap::with_capacity(num_cols);
            for (idx, col_name) in column_names.iter().enumerate() {
                let mut buffer = Vec::new();
                let not_null = row
                    .get_text((idx + 1) as u16, &mut buffer)
                    .map_err(|e| anyhow!("Failed to get column {}: {:?}", idx + 1, e))?;

                let value = if not_null {
                    String::from_utf8_lossy(&buffer).to_string()
                } else {
                    String::new()
                };
                row_map.insert(col_name.clone(), value);
            }
            rows.push(row_map);
        }

        Ok(rows)
    }

    /// Execute query with metadata
    fn execute_with_metadata_internal(&mut self, query: &str) -> Result<OdbcQueryResult> {
        let mut state = self
            .state
            .lock()
            .map_err(|e| anyhow!("Failed to lock connection state: {}", e))?;

        let connection = state
            .connection
            .as_ref()
            .ok_or_else(|| anyhow!("Connection is closed"))?;

        let mut cursor = connection
            .execute(query, (), None)
            .map_err(|e| anyhow!("ODBC query failed: {:?}", e))?
            .ok_or_else(|| anyhow!("ODBC query returned no results"))?;

        let num_cols = cursor
            .num_result_cols()
            .map_err(|e| anyhow!("Failed to get column count: {:?}", e))?
            as usize;

        // Extract column metadata with types
        let mut columns = Vec::with_capacity(num_cols);
        let mut description = ColumnDescription::default();
        for i in 1..=num_cols {
            cursor
                .describe_col(i as u16, &mut description)
                .map_err(|e| anyhow!("Failed to describe column {}: {:?}", i, e))?;

            let name = description
                .name_to_string()
                .unwrap_or_else(|_| format!("col{}", i));

            let data_type = Self::map_odbc_type_to_sql(&description.data_type);
            let nullable = description.nullability != odbc_api::Nullability::NoNulls;

            columns.push(OdbcColumnInfo {
                name,
                data_type,
                nullable,
            });
        }

        // Fetch rows
        let mut rows = Vec::new();
        while let Some(mut row) = cursor
            .next_row()
            .map_err(|e| anyhow!("Failed to fetch row: {:?}", e))?
        {
            let mut row_map = HashMap::with_capacity(num_cols);
            for (idx, col_info) in columns.iter().enumerate() {
                let mut buffer = Vec::new();
                let not_null = row
                    .get_text((idx + 1) as u16, &mut buffer)
                    .map_err(|e| anyhow!("Failed to get column {}: {:?}", idx + 1, e))?;

                let value = if not_null {
                    String::from_utf8_lossy(&buffer).to_string()
                } else {
                    String::new()
                };
                row_map.insert(col_info.name.clone(), value);
            }
            rows.push(row_map);
        }

        Ok(OdbcQueryResult { columns, rows })
    }

    /// Map ODBC DataType to SQL type string
    fn map_odbc_type_to_sql(data_type: &DataType) -> String {
        match data_type {
            DataType::Integer => "INTEGER".to_string(),
            DataType::SmallInt => "SMALLINT".to_string(),
            DataType::BigInt => "BIGINT".to_string(),
            DataType::Real => "REAL".to_string(),
            DataType::Float { precision } => {
                if *precision > 0 {
                    format!("FLOAT({})", precision)
                } else {
                    "FLOAT".to_string()
                }
            }
            DataType::Double => "DOUBLE".to_string(),
            DataType::Numeric { precision, scale } => {
                format!("NUMERIC({}, {})", precision, scale)
            }
            DataType::Decimal { precision, scale } => {
                format!("DECIMAL({}, {})", precision, scale)
            }
            DataType::Char { length } => match length {
                Some(len) => format!("CHAR({})", len.get()),
                None => "CHAR".to_string(),
            },
            DataType::Varchar { length } => match length {
                Some(len) => format!("VARCHAR({})", len.get()),
                None => "VARCHAR".to_string(),
            },
            DataType::WVarchar { length } => match length {
                Some(len) => format!("NVARCHAR({})", len.get()),
                None => "NVARCHAR".to_string(),
            },
            DataType::LongVarchar { length } => match length {
                Some(len) => format!("LONGVARCHAR({})", len.get()),
                None => "LONGVARCHAR".to_string(),
            },
            DataType::Date => "DATE".to_string(),
            DataType::Time { precision } => {
                if *precision > 0 {
                    format!("TIME({})", precision)
                } else {
                    "TIME".to_string()
                }
            }
            DataType::Timestamp { precision } => {
                if *precision > 0 {
                    format!("TIMESTAMP({})", precision)
                } else {
                    "TIMESTAMP".to_string()
                }
            }
            DataType::Binary { length } => match length {
                Some(len) => format!("BINARY({})", len.get()),
                None => "BINARY".to_string(),
            },
            DataType::Varbinary { length } => match length {
                Some(len) => format!("VARBINARY({})", len.get()),
                None => "VARBINARY".to_string(),
            },
            DataType::LongVarbinary { length } => match length {
                Some(len) => format!("LONGVARBINARY({})", len.get()),
                None => "LONGVARBINARY".to_string(),
            },
            DataType::Bit => "BIT".to_string(),
            DataType::TinyInt => "TINYINT".to_string(),
            DataType::Other {
                data_type,
                column_size,
                decimal_digits,
            } => {
                let size_str = column_size
                    .map(|s| s.get().to_string())
                    .unwrap_or_else(|| "?".to_string());
                format!("UNKNOWN({:?},{},{})", data_type, size_str, decimal_digits)
            }
            _ => "VARCHAR".to_string(), // Conservative fallback
        }
    }
}

#[cfg(feature = "odbc")]
impl OdbcPoolableConnection for OdbcOracleConnection {
    fn connect(connection_string: &str) -> Result<Self> {
        // Create ODBC environment
        let env = Box::new(
            Environment::new()
                .map_err(|e| anyhow!("Failed to create ODBC environment: {:?}", e))?,
        );

        tracing::debug!("Connecting to Oracle via ODBC");

        // SAFETY: We're leaking the environment box to get a 'static lifetime
        // The environment will be cleaned up in Drop
        let env_ptr: *const Environment = Box::leak(env);
        let env_ref: &'static Environment = unsafe { &*env_ptr };

        // Connect to database
        let connection = env_ref
            .connect_with_connection_string(connection_string, ConnectionOptions::default())
            .map_err(|e| {
                // Clean up leaked environment on error
                unsafe {
                    let _ = Box::from_raw(env_ptr as *mut Environment);
                }
                anyhow!("Failed to connect to Oracle: {:?}", e)
            })?;

        let state = OracleConnectionState {
            _env_ptr: env_ptr,
            connection: Some(connection),
            in_transaction: false,
        };

        Ok(Self {
            state: Arc::new(Mutex::new(state)),
        })
    }

    fn execute_query(
        &mut self,
        query: &str,
        normalize_headers: bool,
    ) -> Result<Vec<HashMap<String, String>>> {
        self.execute_internal(query, normalize_headers)
    }

    fn execute_query_with_metadata(&mut self, query: &str) -> Result<OdbcQueryResult> {
        self.execute_with_metadata_internal(query)
    }

    fn is_alive(&self) -> bool {
        // Try to execute simple Oracle health check query
        let query = "SELECT 1 FROM DUAL";

        let state_lock = match self.state.lock() {
            Ok(lock) => lock,
            Err(_) => return false,
        };

        // Execute health check while holding the lock
        match &state_lock.connection {
            Some(connection) => match connection.execute(query, (), None) {
                Ok(Some(_cursor)) => true,
                Ok(None) => false,
                Err(_) => false,
            },
            None => false,
        }
    }

    fn reset(&mut self) -> Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|e| anyhow!("Failed to lock connection state: {}", e))?;

        // If in transaction, rollback
        if state.in_transaction {
            if let Some(connection) = state.connection.as_ref() {
                connection
                    .rollback()
                    .map_err(|e| anyhow!("Failed to rollback transaction: {:?}", e))?;
                state.in_transaction = false;
            }
        }

        Ok(())
    }
}

#[cfg(feature = "odbc")]
impl Drop for OracleConnectionState {
    fn drop(&mut self) {
        // Drop the ODBC connection first, while the environment is still valid.
        if let Some(connection) = self.connection.take() {
            let drop_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                drop(connection);
            }));

            if let Err(panic_info) = drop_result {
                tracing::error!("ODBC library panicked while dropping Oracle connection");
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
                tracing::error!("ODBC library panicked while dropping Oracle environment");
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
impl Drop for OdbcOracleConnection {
    fn drop(&mut self) {
        // Rollback any pending transaction
        let should_rollback = self
            .state
            .lock()
            .map(|state| state.in_transaction)
            .unwrap_or(false);

        if should_rollback {
            let _ = self.reset();
        }
        tracing::debug!("ODBC Oracle connection dropped");
    }
}

// ============================================================================
// Stub Implementation (when ODBC feature is disabled)
// ============================================================================

#[cfg(not(feature = "odbc"))]
pub struct OdbcOracleConnection;

#[cfg(not(feature = "odbc"))]
impl OdbcOracleConnection {
    pub fn connect(_connection_string: &str) -> Result<Self> {
        Err(anyhow!(
            "ODBC support is disabled. Enable the 'odbc' feature to use ODBC pooling."
        ))
    }
}
