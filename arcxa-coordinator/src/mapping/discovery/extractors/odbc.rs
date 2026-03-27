//! ODBC helper utilities for schema discovery extractors.

use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::time::Duration;

#[cfg(feature = "odbc")]
use odbc_api::{
    ColumnDescription, ConnectionOptions, Cursor, DataType, Environment, ResultSetMetadata,
};

/// Column metadata extracted from ODBC
#[derive(Debug, Clone)]
pub struct OdbcColumnInfo {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
}

/// Query result with typed column information
#[derive(Debug, Clone)]
pub struct OdbcQueryResult {
    pub columns: Vec<OdbcColumnInfo>,
    pub rows: Vec<HashMap<String, String>>,
}

/// Execute an ODBC query and return rows as string maps.
///
/// - `normalize_headers`: if true, column names are lowercased for stable key lookup.
#[cfg(feature = "odbc")]
pub async fn execute_odbc_query(
    connection_string: &str,
    query: &str,
    normalize_headers: bool,
) -> Result<Vec<HashMap<String, String>>> {
    let connection_string = connection_string.to_string();
    let query = query.to_string();

    tokio::task::spawn_blocking(move || {
        let env = Environment::new()
            .map_err(|e| anyhow!("Failed to create ODBC environment: {:?}", e))?;

        let conn = env
            .connect_with_connection_string(&connection_string, ConnectionOptions::default())
            .map_err(|e| anyhow!("Failed to connect via ODBC: {:?}", e))?;

        let mut cursor = conn
            .execute(&query, (), None)
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
    })
    .await
    .map_err(|e| anyhow!("ODBC task failed: {:?}", e))?
}

/// Execute ODBC query with full column metadata (types, nullability)
#[cfg(feature = "odbc")]
pub async fn execute_odbc_query_with_metadata(
    connection_string: &str,
    query: &str,
) -> Result<OdbcQueryResult> {
    let connection_string = connection_string.to_string();
    let query = query.to_string();

    tokio::task::spawn_blocking(move || {
        let env = Environment::new()
            .map_err(|e| anyhow!("Failed to create ODBC environment: {:?}", e))?;

        let conn = env
            .connect_with_connection_string(&connection_string, ConnectionOptions::default())
            .map_err(|e| anyhow!("Failed to connect via ODBC: {:?}", e))?;

        let mut cursor = conn
            .execute(&query, (), None)
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

            let data_type = map_odbc_type_to_sql(&description.data_type);
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
    })
    .await
    .map_err(|e| anyhow!("ODBC task failed: {:?}", e))?
}

/// Execute ODBC query with named parameter binding and full column metadata.
#[cfg(feature = "odbc")]
pub async fn execute_odbc_query_with_metadata_and_params(
    connection_string: &str,
    query: &str,
    parameters: HashMap<String, serde_json::Value>,
) -> Result<OdbcQueryResult> {
    let connection_string = connection_string.to_string();
    let query = query.to_string();

    tokio::task::spawn_blocking(move || {
        let env = Environment::new()
            .map_err(|e| anyhow!("Failed to create ODBC environment: {:?}", e))?;

        let conn = env
            .connect_with_connection_string(&connection_string, ConnectionOptions::default())
            .map_err(|e| anyhow!("Failed to connect via ODBC: {:?}", e))?;

        let (rewritten_query, ordered_names) =
            crate::common::odbc::rewrite_named_parameters(&query);
        if ordered_names.is_empty() {
            return Err(anyhow!(
                "Query parameters were provided but no named placeholders were found"
            ));
        }

        let bound_parameters =
            crate::common::odbc::build_named_parameters(&parameters, &ordered_names)?;

        let mut cursor = conn
            .execute(&rewritten_query, bound_parameters.as_slice(), None)
            .map_err(|e| anyhow!("ODBC parameterized query failed: {:?}", e))?
            .ok_or_else(|| anyhow!("ODBC query returned no results"))?;

        let num_cols = cursor
            .num_result_cols()
            .map_err(|e| anyhow!("Failed to get column count: {:?}", e))?
            as usize;

        let mut columns = Vec::with_capacity(num_cols);
        let mut description = ColumnDescription::default();
        for i in 1..=num_cols {
            cursor
                .describe_col(i as u16, &mut description)
                .map_err(|e| anyhow!("Failed to describe column {}: {:?}", i, e))?;

            let name = description
                .name_to_string()
                .unwrap_or_else(|_| format!("col{}", i));

            let data_type = map_odbc_type_to_sql(&description.data_type);
            let nullable = description.nullability != odbc_api::Nullability::NoNulls;

            columns.push(OdbcColumnInfo {
                name,
                data_type,
                nullable,
            });
        }

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
    })
    .await
    .map_err(|e| anyhow!("ODBC task failed: {:?}", e))?
}

/// Map ODBC DataType to SQL type string
#[cfg(feature = "odbc")]
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

#[cfg(not(feature = "odbc"))]
pub async fn execute_odbc_query(
    _connection_string: &str,
    _query: &str,
    _normalize_headers: bool,
) -> Result<Vec<HashMap<String, String>>> {
    Err(anyhow!(
        "ODBC support is disabled. Enable the 'odbc' feature to use ODBC extractors."
    ))
}

#[cfg(not(feature = "odbc"))]
pub async fn execute_odbc_query_with_metadata(
    _connection_string: &str,
    _query: &str,
) -> Result<OdbcQueryResult> {
    Err(anyhow!(
        "ODBC support is disabled. Enable the 'odbc' feature to use ODBC extractors."
    ))
}

// ============================================================================
// Connection Pooling Infrastructure
// ============================================================================

#[cfg(feature = "odbc")]
use deadpool::managed::{Manager, Pool, PoolError, RecycleError, RecycleResult};
#[cfg(feature = "odbc")]
use std::marker::PhantomData;

/// Trait for ODBC connections that can be pooled
///
/// Implementations must handle the Environment lifetime via leak-and-reclaim pattern
/// (see OdbcDB2Connection in mapping/loader/odbc_db2_connection.rs for reference)
#[cfg(feature = "odbc")]
pub trait OdbcPoolableConnection: Send + Sync + Sized {
    /// Connect to database using connection string
    ///
    /// Must use leak-and-reclaim pattern:
    /// 1. Create Environment
    /// 2. Leak it to get 'static lifetime
    /// 3. Create Connection<'static>
    /// 4. Wrap in Arc<Mutex<ConnectionState>>
    fn connect(connection_string: &str) -> Result<Self>;

    /// Execute a query and return rows as string maps
    fn execute_query(
        &mut self,
        query: &str,
        normalize_headers: bool,
    ) -> Result<Vec<HashMap<String, String>>>;

    /// Execute a query and return typed column metadata
    fn execute_query_with_metadata(&mut self, query: &str) -> Result<OdbcQueryResult>;

    /// Check if connection is still alive (health check)
    ///
    /// Should execute a lightweight query like:
    /// - Oracle: SELECT 1 FROM DUAL
    /// - SAP HANA: SELECT 1 FROM DUMMY
    /// - DB2: SELECT 1 FROM SYSIBM.SYSDUMMY1
    fn is_alive(&self) -> bool;

    /// Reset connection state before returning to pool
    ///
    /// Should:
    /// - Rollback any open transactions
    /// - Clear temporary tables/session state
    /// - Reset connection parameters if needed
    fn reset(&mut self) -> Result<()>;
}

/// Generic ODBC connection manager for deadpool
///
/// Manages connection lifecycle for any database implementing OdbcPoolableConnection
#[cfg(feature = "odbc")]
pub struct GenericOdbcConnectionManager<C: OdbcPoolableConnection> {
    connection_string: String,
    health_check_enabled: bool,
    _phantom: PhantomData<C>,
}

#[cfg(feature = "odbc")]
impl<C: OdbcPoolableConnection> GenericOdbcConnectionManager<C> {
    /// Create a new connection manager
    pub fn new(connection_string: String, health_check_enabled: bool) -> Self {
        Self {
            connection_string,
            health_check_enabled,
            _phantom: PhantomData,
        }
    }
}

#[cfg(feature = "odbc")]
impl<C: OdbcPoolableConnection + 'static> Manager for GenericOdbcConnectionManager<C> {
    type Type = C;
    type Error = anyhow::Error;

    /// Create a new database connection
    ///
    /// Called when the pool needs a new connection.
    /// Uses spawn_blocking to avoid blocking the async runtime.
    async fn create(&self) -> Result<Self::Type, Self::Error> {
        tracing::debug!("Creating new ODBC connection from pool");

        let conn_string = self.connection_string.clone();

        // Move blocking ODBC connection creation to thread pool
        let conn = tokio::task::spawn_blocking(move || C::connect(&conn_string))
            .await
            .map_err(|e| anyhow!("Failed to spawn connection task: {}", e))??;

        tracing::info!("ODBC connection created successfully");
        Ok(conn)
    }

    /// Recycle (health check) an existing connection
    ///
    /// Called periodically to ensure connections are still valid.
    /// Dead connections are removed from the pool.
    async fn recycle(
        &self,
        conn: &mut Self::Type,
        _metrics: &deadpool::managed::Metrics,
    ) -> RecycleResult<Self::Error> {
        if !self.health_check_enabled {
            return Ok(());
        }

        tracing::debug!("Recycling ODBC connection (health check)");

        // Simple health check using the is_alive method
        let is_alive = conn.is_alive();

        if is_alive {
            // Reset connection state before returning to pool
            conn.reset().map_err(|e| {
                tracing::warn!("Failed to reset connection: {}", e);
                RecycleError::Backend(e)
            })?;
            tracing::debug!("ODBC connection is healthy");
            Ok(())
        } else {
            tracing::warn!("ODBC connection is dead, will be removed from pool");
            Err(RecycleError::Backend(anyhow!(
                "Connection health check failed"
            )))
        }
    }
}

/// Pool configuration for ODBC connections
#[derive(Debug, Clone)]
pub struct OdbcPoolConfig {
    /// Connection string (with credentials)
    pub connection_string: String,

    /// Maximum number of connections in the pool
    pub max_size: usize,

    /// Timeouts for pool operations
    pub timeouts: OdbcPoolTimeouts,

    /// Enable health checks on recycled connections
    pub health_check_enabled: bool,
}

impl OdbcPoolConfig {
    pub fn new(connection_string: String) -> Self {
        Self {
            connection_string,
            max_size: 10,
            timeouts: OdbcPoolTimeouts::default(),
            health_check_enabled: true,
        }
    }
}

/// Pool timeout configuration
#[derive(Debug, Clone)]
pub struct OdbcPoolTimeouts {
    /// Timeout for waiting to acquire a connection from the pool
    pub wait: Duration,

    /// Timeout for creating a new connection
    pub create: Duration,

    /// Timeout for recycling/health check
    pub recycle: Duration,
}

impl Default for OdbcPoolTimeouts {
    fn default() -> Self {
        Self {
            wait: Duration::from_secs(30),
            create: Duration::from_secs(10),
            recycle: Duration::from_secs(5),
        }
    }
}

/// Create a generic ODBC connection pool
///
/// This should be called once per data source and the pool should be cached.
#[cfg(feature = "odbc")]
pub async fn create_odbc_pool<C: OdbcPoolableConnection + 'static>(
    config: OdbcPoolConfig,
) -> Result<Pool<GenericOdbcConnectionManager<C>>, PoolError<anyhow::Error>> {
    tracing::info!(
        "Creating ODBC connection pool (max_size={}, conn_string=***)",
        config.max_size
    );

    let manager = GenericOdbcConnectionManager::new(
        config.connection_string.clone(),
        config.health_check_enabled,
    );

    let pool = Pool::builder(manager)
        .max_size(config.max_size)
        .wait_timeout(Some(config.timeouts.wait))
        .create_timeout(Some(config.timeouts.create))
        .recycle_timeout(Some(config.timeouts.recycle))
        .runtime(deadpool::Runtime::Tokio1)
        .build()
        .map_err(|e| PoolError::Backend(anyhow!("Failed to build pool: {:?}", e)))?;

    tracing::info!("ODBC connection pool created successfully");
    Ok(pool)
}

/// Get pool statistics for monitoring
#[cfg(feature = "odbc")]
pub fn get_odbc_pool_stats<C: OdbcPoolableConnection + 'static>(
    pool: &Pool<GenericOdbcConnectionManager<C>>,
) -> OdbcPoolStats {
    let status = pool.status();
    OdbcPoolStats {
        size: status.size,
        available: status.available,
        max_size: status.max_size,
        waiting: status.waiting,
    }
}

/// Pool statistics for monitoring
#[derive(Debug, Clone)]
pub struct OdbcPoolStats {
    /// Total connections in pool
    pub size: usize,

    /// Available connections
    pub available: usize,

    /// Maximum pool size
    pub max_size: usize,

    /// Threads waiting for connections
    pub waiting: usize,
}
