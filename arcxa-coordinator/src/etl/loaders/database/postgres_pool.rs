//! PostgreSQL Connection Pool
//!
//! Async-aware connection pooling for PostgreSQL using deadpool-postgres.
//! Provides connection reuse across workflow executions for improved performance.

use anyhow::{anyhow, Context, Result};
use deadpool_postgres::{Config, ManagerConfig, Pool, RecyclingMethod, Runtime, SslMode};
use graphica_core::catalog::postgres_tls::{
    connect_postgres_client_with_transport, make_rustls_connector, postgres_ssl_behavior,
    ssl_mode_uses_tls, PostgresSslBehavior,
};
use std::time::Duration;
use tokio_postgres::NoTls;
use tracing::{debug, info, warn};

/// PostgreSQL connection pool configuration
#[derive(Debug, Clone)]
pub struct PostgresPoolConfig {
    /// PostgreSQL connection configuration
    pub postgres_config: PostgresConfig,

    /// Maximum number of connections in the pool
    pub max_size: usize,

    /// Timeouts for pool operations
    pub timeouts: PoolTimeouts,

    /// Enable health checks on recycled connections
    pub health_check_enabled: bool,
}

impl Default for PostgresPoolConfig {
    fn default() -> Self {
        Self {
            postgres_config: PostgresConfig::default(),
            max_size: 10,
            timeouts: PoolTimeouts::default(),
            health_check_enabled: true,
        }
    }
}

/// PostgreSQL connection configuration
#[derive(Debug, Clone)]
pub struct PostgresConfig {
    /// Database host
    pub host: String,

    /// Database port
    pub port: u16,

    /// Database name
    pub database: String,

    /// Username
    pub username: String,

    /// Password
    pub password: String,

    /// SSL mode (disable, prefer, require, verify-ca, verify-full)
    pub ssl_mode: Option<String>,
}

impl Default for PostgresConfig {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 5432,
            database: "graphica".to_string(),
            username: "postgres".to_string(),
            password: String::new(),
            ssl_mode: None,
        }
    }
}

impl PostgresConfig {
    /// Build PostgreSQL connection string
    pub fn to_connection_string(&self) -> String {
        let mut connection_string = format!(
            "host={} port={} dbname={} user={} password={}",
            self.host, self.port, self.database, self.username, self.password
        );

        if let Some(ssl_mode) = &self.ssl_mode {
            connection_string.push_str(&format!(" sslmode={}", ssl_mode));
        }

        connection_string
    }

    /// Build sanitized connection string (without password) for logging
    pub fn to_sanitized_string(&self) -> String {
        let mut connection_string = format!(
            "host={} port={} dbname={} user={}",
            self.host, self.port, self.database, self.username
        );

        if let Some(ssl_mode) = &self.ssl_mode {
            connection_string.push_str(&format!(" sslmode={}", ssl_mode));
        }

        connection_string
    }
}

fn parse_postgres_ssl_mode(ssl_mode: &str) -> Result<SslMode> {
    let normalized = ssl_mode.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "disable" => Ok(SslMode::Disable),
        "prefer" => Ok(SslMode::Prefer),
        "require" | "verify-ca" | "verify-full" => Ok(SslMode::Require),
        _ => Err(anyhow!("Unsupported PostgreSQL sslmode '{}'", ssl_mode)),
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
pub type PostgresPool = Pool;

/// Create a PostgreSQL connection pool
///
/// This should be called once at application startup and the pool
/// should be shared across all workflow executions.
///
/// # Example
///
/// ```no_run
/// use graphica_coordinator::etl::loaders::database::{
///     create_postgres_pool, PoolTimeouts, PostgresConfig, PostgresPoolConfig,
/// };
/// # async fn example() -> anyhow::Result<()> {
/// let pool_config = PostgresPoolConfig {
///     postgres_config: PostgresConfig {
///         host: "localhost".to_string(),
///         port: 5432,
///         database: "graphica".to_string(),
///         username: "postgres".to_string(),
///         password: "password".to_string(),
///     },
///     max_size: 10,
///     timeouts: PoolTimeouts::default(),
///     health_check_enabled: true,
/// };
///
/// let pool = create_postgres_pool(pool_config).await?;
/// # Ok(())
/// # }
/// ```
pub async fn create_postgres_pool(config: PostgresPoolConfig) -> Result<PostgresPool> {
    info!(
        "Creating PostgreSQL connection pool (max_size={}, db={})",
        config.max_size, config.postgres_config.database
    );

    // Build deadpool-postgres configuration
    let mut pg_config = Config::new();
    pg_config.host = Some(config.postgres_config.host.clone());
    pg_config.port = Some(config.postgres_config.port);
    pg_config.dbname = Some(config.postgres_config.database.clone());
    pg_config.user = Some(config.postgres_config.username.clone());
    pg_config.password = Some(config.postgres_config.password.clone());
    pg_config.ssl_mode = config
        .postgres_config
        .ssl_mode
        .as_deref()
        .map(parse_postgres_ssl_mode)
        .transpose()?;

    // Configure pool settings
    pg_config.manager = Some(ManagerConfig {
        recycling_method: if config.health_check_enabled {
            RecyclingMethod::Fast
        } else {
            RecyclingMethod::Verified
        },
    });

    let connection_string = format!(
        "host={} port={} dbname={} user={} password={}",
        config.postgres_config.host,
        config.postgres_config.port,
        config.postgres_config.database,
        config.postgres_config.username,
        config.postgres_config.password
    );
    let ssl_behavior = postgres_ssl_behavior(config.postgres_config.ssl_mode.as_deref());
    let use_tls = match ssl_behavior {
        PostgresSslBehavior::Disable => false,
        PostgresSslBehavior::Require => true,
        PostgresSslBehavior::Prefer => {
            let (client, used_tls) = connect_postgres_client_with_transport(
                &connection_string,
                config.postgres_config.ssl_mode.as_deref(),
            )
            .await
            .context("Failed to probe PostgreSQL transport mode for pool initialization")?;

            client
                .simple_query("SELECT 1")
                .await
                .context("Failed PostgreSQL pool transport probe query")?;

            used_tls
        }
    };

    pg_config.ssl_mode = if use_tls {
        config
            .postgres_config
            .ssl_mode
            .as_deref()
            .map(parse_postgres_ssl_mode)
            .transpose()?
    } else {
        Some(SslMode::Disable)
    };

    // Create pool
    let pool = if use_tls && ssl_mode_uses_tls(config.postgres_config.ssl_mode.as_deref()) {
        let tls = make_rustls_connector().context("Failed to configure PostgreSQL TLS")?;
        pg_config
            .create_pool(Some(Runtime::Tokio1), tls)
            .context("Failed to create PostgreSQL TLS pool")?
    } else {
        pg_config
            .create_pool(Some(Runtime::Tokio1), NoTls)
            .context("Failed to create PostgreSQL pool")?
    };

    info!("PostgreSQL connection pool created successfully");
    Ok(pool)
}

/// Get pool statistics for monitoring
///
/// Returns current pool status including size, available connections,
/// and waiting threads.
pub fn get_pool_stats(pool: &PostgresPool) -> PoolStats {
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
        let config = PostgresPoolConfig::default();
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

    #[test]
    fn test_postgres_config_to_connection_string() {
        let config = PostgresConfig {
            host: "localhost".to_string(),
            port: 5432,
            database: "testdb".to_string(),
            username: "testuser".to_string(),
            password: "testpass".to_string(),
            ssl_mode: None,
        };

        let conn_str = config.to_connection_string();
        assert!(conn_str.contains("host=localhost"));
        assert!(conn_str.contains("port=5432"));
        assert!(conn_str.contains("dbname=testdb"));
        assert!(conn_str.contains("user=testuser"));
        assert!(conn_str.contains("password=testpass"));
    }

    #[test]
    fn test_postgres_config_sanitized_string() {
        let config = PostgresConfig {
            host: "localhost".to_string(),
            port: 5432,
            database: "testdb".to_string(),
            username: "testuser".to_string(),
            password: "secret".to_string(),
            ssl_mode: None,
        };

        let sanitized = config.to_sanitized_string();
        assert!(!sanitized.contains("password"));
        assert!(!sanitized.contains("secret"));
        assert!(sanitized.contains("host=localhost"));
    }

    #[test]
    fn test_postgres_config_connection_string_includes_sslmode() {
        let config = PostgresConfig {
            host: "localhost".to_string(),
            port: 5432,
            database: "testdb".to_string(),
            username: "testuser".to_string(),
            password: "testpass".to_string(),
            ssl_mode: Some("require".to_string()),
        };

        let conn_str = config.to_connection_string();
        let sanitized = config.to_sanitized_string();

        assert!(conn_str.contains("sslmode=require"));
        assert!(sanitized.contains("sslmode=require"));
    }
}
