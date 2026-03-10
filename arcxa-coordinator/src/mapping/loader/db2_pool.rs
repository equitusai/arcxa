//! DB2 Connection Pool
//!
//! Async-aware connection pooling for DB2 using deadpool.
//! Provides connection reuse across workflow executions for improved performance.

use super::{DB2Config, DB2Connection, DB2Error, OdbcDB2Connection};
use deadpool::managed::{Manager, Pool, PoolError, RecycleError, RecycleResult};
use std::time::Duration;
use tracing::{debug, info, warn};

/// DB2 connection pool configuration
#[derive(Debug, Clone)]
pub struct DB2PoolConfig {
    /// DB2 connection configuration
    pub db2_config: DB2Config,

    /// Maximum number of connections in the pool
    pub max_size: usize,

    /// Timeouts for pool operations
    pub timeouts: PoolTimeouts,

    /// Enable health checks on recycled connections
    pub health_check_enabled: bool,
}

impl Default for DB2PoolConfig {
    fn default() -> Self {
        Self {
            db2_config: DB2Config::default(),
            max_size: 10,
            timeouts: PoolTimeouts::default(),
            health_check_enabled: true,
        }
    }
}

/// Pool timeout configuration
#[derive(Debug, Clone)]
pub struct PoolTimeouts {
    /// Timeout for waiting to acquire a connection from the pool
    pub wait: Duration,

    /// Timeout for creating a new connection
    pub create: Duration,

    /// Timeout for recycling/health check
    pub recycle: Duration,
}

impl Default for PoolTimeouts {
    fn default() -> Self {
        Self {
            wait: Duration::from_secs(30),
            create: Duration::from_secs(10),
            recycle: Duration::from_secs(5),
        }
    }
}

/// Pool statistics for monitoring
#[derive(Debug, Clone)]
pub struct PoolStats {
    /// Total connections in pool
    pub size: usize,

    /// Available connections
    pub available: usize,

    /// Maximum pool size
    pub max_size: usize,

    /// Threads waiting for connections
    pub waiting: usize,
}

/// Connection pool type alias
pub type DB2Pool = Pool<DB2ConnectionManager>;

/// Pooled connection type alias
pub type PooledDB2Connection = deadpool::managed::Object<DB2ConnectionManager>;

/// DB2 connection manager for deadpool
pub struct DB2ConnectionManager {
    config: DB2Config,
    health_check_enabled: bool,
}

impl DB2ConnectionManager {
    /// Create a new connection manager
    pub fn new(config: DB2Config, health_check_enabled: bool) -> Self {
        Self {
            config,
            health_check_enabled,
        }
    }
}

impl Manager for DB2ConnectionManager {
    type Type = OdbcDB2Connection;
    type Error = DB2Error;

    /// Create a new database connection
    ///
    /// This is called when the pool needs a new connection.
    /// Uses spawn_blocking to avoid blocking the async runtime.
    async fn create(&self) -> Result<Self::Type, Self::Error> {
        debug!("Creating new DB2 connection from pool");

        let config = self.config.clone();

        // Move blocking ODBC connection creation to thread pool
        let conn = tokio::task::spawn_blocking(move || OdbcDB2Connection::connect(&config))
            .await
            .map_err(|e| DB2Error::ConnectionError {
                message: format!("Failed to spawn connection task: {}", e),
            })??;

        info!("DB2 connection created successfully");
        Ok(conn)
    }

    /// Recycle (health check) an existing connection
    ///
    /// This is called periodically to ensure connections are still valid.
    /// Dead connections are removed from the pool.
    async fn recycle(
        &self,
        conn: &mut Self::Type,
        _metrics: &deadpool::managed::Metrics,
    ) -> RecycleResult<Self::Error> {
        if !self.health_check_enabled {
            return Ok(());
        }

        debug!("Recycling DB2 connection (health check)");

        // Simple health check using the is_alive method
        // This is a quick check so we can call it directly without spawn_blocking
        let is_alive = conn.is_alive();

        if is_alive {
            debug!("DB2 connection is healthy");
            Ok(())
        } else {
            warn!("DB2 connection is dead, will be removed from pool");
            Err(RecycleError::Backend(DB2Error::ConnectionError {
                message: "Connection health check failed".to_string(),
            }))
        }
    }
}

/// Create a DB2 connection pool
///
/// This should be called once at application startup and the pool
/// should be shared across all workflow executions.
///
/// # Example
///
/// ```no_run
/// use graphica_coordinator::mapping::loader::{
///     create_db2_pool, DB2Config, DB2PoolConfig, PoolTimeouts,
/// };
/// # async fn example() -> anyhow::Result<()> {
/// let pool_config = DB2PoolConfig {
///     db2_config: DB2Config {
///         host: "localhost".to_string(),
///         port: 50000,
///         database: "GRAPHICA".to_string(),
///         username: "db2inst1".to_string(),
///         password: "password".to_string(),
///         ..Default::default()
///     },
///     max_size: 10,
///     timeouts: PoolTimeouts::default(),
///     health_check_enabled: true,
/// };
///
/// let pool = create_db2_pool(pool_config).await?;
/// # Ok(())
/// # }
/// ```
pub async fn create_db2_pool(config: DB2PoolConfig) -> Result<DB2Pool, PoolError<DB2Error>> {
    info!(
        "Creating DB2 connection pool (max_size={}, db={})",
        config.max_size, config.db2_config.database
    );

    let manager = DB2ConnectionManager::new(config.db2_config.clone(), config.health_check_enabled);

    let pool = Pool::builder(manager)
        .max_size(config.max_size)
        .wait_timeout(Some(config.timeouts.wait))
        .create_timeout(Some(config.timeouts.create))
        .recycle_timeout(Some(config.timeouts.recycle))
        .runtime(deadpool::Runtime::Tokio1) // Specify runtime for deadpool 0.12+
        .build()
        .map_err(|e| {
            PoolError::Backend(DB2Error::ConnectionError {
                message: format!("Failed to build pool: {:?}", e),
            })
        })?;

    info!("DB2 connection pool created successfully");
    Ok(pool)
}

/// Get pool statistics for monitoring
///
/// Returns current pool status including size, available connections,
/// and waiting threads.
pub fn get_pool_stats(pool: &DB2Pool) -> PoolStats {
    let status = pool.status();
    PoolStats {
        size: status.size,
        available: status.available,
        max_size: status.max_size,
        waiting: status.waiting,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pool_config_default() {
        let config = DB2PoolConfig::default();
        assert_eq!(config.max_size, 10);
        assert!(config.health_check_enabled);
    }

    #[test]
    fn test_pool_timeouts_default() {
        let timeouts = PoolTimeouts::default();
        assert_eq!(timeouts.wait, Duration::from_secs(30));
        assert_eq!(timeouts.create, Duration::from_secs(10));
        assert_eq!(timeouts.recycle, Duration::from_secs(5));
    }
}
