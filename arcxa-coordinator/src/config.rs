//! Configuration Management for Graphica Coordinator
//!
//! Centralized configuration loading and validation from environment variables.
//! Provides type-safe access to all configuration options with defaults.

use anyhow::{Context, Result};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use crate::storage::wal::{CompressionCodec, FsyncMode, WalConfig};

/// Complete coordinator configuration
#[derive(Debug, Clone)]
pub struct CoordinatorConfig {
    pub network: NetworkConfig,
    pub storage: StorageConfig,
    pub shards: ShardConfig,
    pub kafka: KafkaConfig,
    pub auth: AuthConfig,
    pub logging: LoggingConfig,
    pub performance: PerformanceConfig,
    pub model_services: ModelServicesConfig,
    pub rdf_wal: Option<RdfWalConfig>,
}

/// Network configuration
#[derive(Debug, Clone)]
pub struct NetworkConfig {
    pub rest_port: u16,
    pub grpc_port: u16,
    pub rest_bind_address: String,
    pub grpc_bind_address: String,
    pub metrics_port: u16,
}

/// Storage configuration
#[derive(Debug, Clone)]
pub struct StorageConfig {
    pub rocksdb_path: String,
    pub parquet_path: String,
    pub archive_path: String,
    pub rocksdb_block_cache_mb: usize,
    pub rocksdb_write_buffer_mb: usize,
    pub rocksdb_max_background_jobs: usize,
}

/// Shard configuration
#[derive(Debug, Clone)]
pub struct ShardConfig {
    pub urls: Vec<String>,
    pub connect_timeout: Duration,
    pub request_timeout: Duration,
    pub max_failures: u32,
    pub circuit_reset_duration: Duration,
    pub pool_size: usize,
    pub replication_factor: u32,
    pub heartbeat_timeout_seconds: u64,
}

/// Kafka configuration
#[derive(Debug, Clone)]
pub struct KafkaConfig {
    pub brokers: String,
    pub consumer_group: String,
    pub lineage_topic: String,
    pub quality_topic: String,
}

/// Authentication configuration
#[derive(Debug, Clone)]
pub struct AuthConfig {
    pub jwt_secret: Option<String>,
    pub jwt_expiration_seconds: u64,
    pub enable_auth: bool,
    pub cors_allowed_origins: Vec<String>,
}

/// Logging configuration
#[derive(Debug, Clone)]
pub struct LoggingConfig {
    pub rust_log: String,
    pub enable_metrics: bool,
    pub enable_tracing: bool,
    pub jaeger_agent_endpoint: Option<String>,
}

/// Performance tuning configuration
#[derive(Debug, Clone)]
pub struct PerformanceConfig {
    pub query_timeout: Duration,
    pub query_max_results_per_shard: usize,
    pub max_request_body_size: usize,
    pub http_request_timeout: Duration,
}

/// Model services configuration (Phase 2: Distributed semantic matching)
#[derive(Debug, Clone)]
pub struct ModelServicesConfig {
    pub enabled: bool,
    pub service_urls: Vec<String>,
    pub model_names: Vec<String>,
    pub cache_dir: String,
    pub connect_timeout_secs: u64,
    pub request_timeout_secs: u64,
    pub circuit_breaker_threshold: u32,
    pub circuit_breaker_timeout_secs: u64,
}

/// RDF Write-Ahead Log configuration
///
/// Provides durability for RDF triple operations. When enabled, all RDF insert/delete
/// operations are written to a write-ahead log before being forwarded to shards.
/// This ensures triples survive coordinator crashes.
#[derive(Debug, Clone)]
pub struct RdfWalConfig {
    /// Whether RDF WAL is enabled (opt-in for backwards compatibility)
    pub enabled: bool,

    /// Core WAL configuration (path, segment size, fsync mode, etc.)
    pub wal: WalConfig,

    /// Whether to automatically recover from WAL on coordinator startup
    pub auto_recover: bool,

    /// Optional LSN to start recovery from (None = start from beginning)
    pub recovery_start_lsn: Option<u64>,

    /// Optional limit on number of entries to recover (None = no limit)
    pub max_recovery_entries: Option<usize>,
}

impl CoordinatorConfig {
    /// Load configuration from environment variables
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            network: NetworkConfig::from_env()?,
            storage: StorageConfig::from_env()?,
            shards: ShardConfig::from_env()?,
            kafka: KafkaConfig::from_env()?,
            auth: AuthConfig::from_env()?,
            logging: LoggingConfig::from_env()?,
            performance: PerformanceConfig::from_env()?,
            model_services: ModelServicesConfig::from_env()?,
            rdf_wal: RdfWalConfig::from_env_optional(),
        })
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<()> {
        // Validate ports
        if self.network.rest_port == 0 {
            anyhow::bail!("REST_PORT must be greater than 0");
        }
        if self.network.grpc_port == 0 {
            anyhow::bail!("GRPC_PORT must be greater than 0");
        }
        if self.network.rest_port == self.network.grpc_port {
            anyhow::bail!("REST_PORT and GRPC_PORT must be different");
        }

        // Validate shard URLs format
        for url in &self.shards.urls {
            if !url.contains(':') {
                anyhow::bail!("Invalid shard URL format: {}. Expected host:port", url);
            }
        }

        // Validate Kafka brokers
        if self.kafka.brokers.is_empty() {
            anyhow::bail!("KAFKA_BROKERS cannot be empty");
        }

        // Validate JWT secret in production
        if self.auth.enable_auth && self.auth.jwt_secret.is_none() {
            tracing::warn!("JWT_SECRET not set - using development authentication (INSECURE)");
        }

        // Validate paths exist or can be created
        for path in &[
            &self.storage.rocksdb_path,
            &self.storage.parquet_path,
            &self.storage.archive_path,
        ] {
            if let Err(e) = std::fs::create_dir_all(path) {
                anyhow::bail!("Failed to create directory {}: {}", path, e);
            }
        }

        Ok(())
    }

    /// Get REST socket address
    pub fn rest_addr(&self) -> Result<SocketAddr> {
        format!(
            "{}:{}",
            self.network.rest_bind_address, self.network.rest_port
        )
        .parse()
        .context("Failed to parse REST address")
    }

    /// Get gRPC socket address
    pub fn grpc_addr(&self) -> Result<SocketAddr> {
        format!(
            "{}:{}",
            self.network.grpc_bind_address, self.network.grpc_port
        )
        .parse()
        .context("Failed to parse gRPC address")
    }
}

impl NetworkConfig {
    fn from_env() -> Result<Self> {
        Ok(Self {
            rest_port: env_var_parse("REST_PORT", 8080)?,
            grpc_port: env_var_parse("GRPC_PORT", 9090)?,
            rest_bind_address: env_var("REST_BIND_ADDRESS", "0.0.0.0"),
            grpc_bind_address: env_var("GRPC_BIND_ADDRESS", "0.0.0.0"),
            metrics_port: env_var_parse("METRICS_PORT", 9091)?,
        })
    }
}

impl StorageConfig {
    fn from_env() -> Result<Self> {
        Ok(Self {
            rocksdb_path: env_var("ROCKSDB_PATH", "./data/coordinator/rocksdb"),
            parquet_path: env_var("PARQUET_PATH", "./data/parquet"),
            archive_path: env_var("ARCHIVE_PATH", "./data/archive"),
            rocksdb_block_cache_mb: env_var_parse("ROCKSDB_BLOCK_CACHE_MB", 256)?,
            rocksdb_write_buffer_mb: env_var_parse("ROCKSDB_WRITE_BUFFER_MB", 64)?,
            rocksdb_max_background_jobs: env_var_parse("ROCKSDB_MAX_BACKGROUND_JOBS", 4)?,
        })
    }
}

impl ShardConfig {
    fn from_env() -> Result<Self> {
        let urls_str = env_var("SHARD_URLS", "");
        let urls = if urls_str.is_empty() {
            Vec::new()
        } else {
            urls_str
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        };

        Ok(Self {
            urls,
            connect_timeout: Duration::from_millis(env_var_parse(
                "SHARD_CONNECT_TIMEOUT_MS",
                5000,
            )?),
            request_timeout: Duration::from_millis(env_var_parse(
                "SHARD_REQUEST_TIMEOUT_MS",
                30000,
            )?),
            max_failures: env_var_parse("SHARD_MAX_FAILURES", 3)?,
            circuit_reset_duration: Duration::from_secs(env_var_parse(
                "SHARD_CIRCUIT_RESET_SECONDS",
                30,
            )?),
            pool_size: env_var_parse("SHARD_POOL_SIZE", 10)?,
            replication_factor: env_var_parse("SHARD_REPLICATION_FACTOR", 2)?,
            heartbeat_timeout_seconds: env_var_parse("SHARD_HEARTBEAT_TIMEOUT_SECONDS", 60)?,
        })
    }
}

impl KafkaConfig {
    fn from_env() -> Result<Self> {
        Ok(Self {
            brokers: env_var("KAFKA_BROKERS", "localhost:9092"),
            consumer_group: env_var("KAFKA_CONSUMER_GROUP", "arcxa-coordinator"),
            lineage_topic: env_var("KAFKA_LINEAGE_TOPIC", "graphica.lineage"),
            quality_topic: env_var("KAFKA_QUALITY_TOPIC", "graphica.quality"),
        })
    }
}

impl AuthConfig {
    fn from_env() -> Result<Self> {
        let cors_origins = env_var(
            "CORS_ALLOWED_ORIGINS",
            "http://localhost:3000,http://localhost:5173",
        );
        let origins = cors_origins
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        Ok(Self {
            jwt_secret: std::env::var("JWT_SECRET").ok(),
            jwt_expiration_seconds: env_var_parse("JWT_EXPIRATION_SECONDS", 3600)?,
            enable_auth: env_var_parse("ENABLE_AUTH", true)?,
            cors_allowed_origins: origins,
        })
    }
}

impl LoggingConfig {
    fn from_env() -> Result<Self> {
        Ok(Self {
            rust_log: env_var("RUST_LOG", "info"),
            enable_metrics: env_var_parse("ENABLE_METRICS", true)?,
            enable_tracing: env_var_parse("ENABLE_TRACING", false)?,
            jaeger_agent_endpoint: std::env::var("JAEGER_AGENT_ENDPOINT").ok(),
        })
    }
}

impl PerformanceConfig {
    fn from_env() -> Result<Self> {
        Ok(Self {
            query_timeout: Duration::from_millis(env_var_parse("QUERY_TIMEOUT_MS", 60000)?),
            query_max_results_per_shard: env_var_parse("QUERY_MAX_RESULTS_PER_SHARD", 10000)?,
            max_request_body_size: env_var_parse("MAX_REQUEST_BODY_SIZE", 10485760)?,
            http_request_timeout: Duration::from_secs(env_var_parse(
                "HTTP_REQUEST_TIMEOUT_SECONDS",
                60,
            )?),
        })
    }
}

impl ModelServicesConfig {
    fn from_env() -> Result<Self> {
        // Check if model services are enabled
        let enabled = env_var_parse("GRAPHICA_MODEL_SERVICES_ENABLED", true)?;

        // Parse service URLs
        let urls_str = env_var(
            "GRAPHICA_MODEL_SERVICE_URLS",
            &env_var("GRAPHICA_MODEL_SERVICE_URL", "http://localhost:50051"),
        );
        let service_urls: Vec<String> = urls_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        // Parse model names
        let names_str = env_var("GRAPHICA_MODEL_NAMES", "minilm");
        let model_names: Vec<String> = names_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        Ok(Self {
            enabled,
            service_urls,
            model_names,
            cache_dir: env_var("GRAPHICA_SEMANTIC_CACHE_DIR", "./data/semantic_cache"),
            connect_timeout_secs: env_var_parse("GRAPHICA_MODEL_CONNECT_TIMEOUT_SECS", 5)?,
            request_timeout_secs: env_var_parse("GRAPHICA_MODEL_REQUEST_TIMEOUT_SECS", 10)?,
            circuit_breaker_threshold: env_var_parse(
                "GRAPHICA_MODEL_CIRCUIT_BREAKER_THRESHOLD",
                5,
            )?,
            circuit_breaker_timeout_secs: env_var_parse(
                "GRAPHICA_MODEL_CIRCUIT_BREAKER_TIMEOUT_SECS",
                30,
            )?,
        })
    }
}

impl RdfWalConfig {
    /// Load RDF WAL configuration from environment variables (optional)
    ///
    /// Returns None if RDF_WAL_ENABLED is not set or is false.
    /// This maintains backwards compatibility.
    pub fn from_env_optional() -> Option<Self> {
        // Check if RDF WAL is enabled
        let enabled = env_var_parse("RDF_WAL_ENABLED", false).ok()?;

        if !enabled {
            return None;
        }

        // If enabled, construct the configuration
        Some(Self {
            enabled: true,
            wal: Self::build_wal_config(),
            auto_recover: env_var_parse("RDF_WAL_AUTO_RECOVER", true).unwrap_or(true),
            recovery_start_lsn: env_var_parse("RDF_WAL_RECOVERY_START_LSN", 0u64)
                .ok()
                .filter(|&lsn| lsn > 0),
            max_recovery_entries: env_var_parse("RDF_WAL_MAX_RECOVERY_ENTRIES", 0usize)
                .ok()
                .filter(|&max| max > 0),
        })
    }

    /// Build WalConfig from environment variables
    fn build_wal_config() -> WalConfig {
        use crate::storage::wal::{
            CompactionPolicy, CompressionCodec, CorruptionTolerance, GroupCommitConfig,
            RecoveryMode, RotationPolicy,
        };

        WalConfig {
            path: PathBuf::from(env_var("RDF_WAL_PATH", "/var/lib/graphica/rdf_wal")),
            max_file_size: env_var_parse("RDF_WAL_MAX_FILE_SIZE", 100 * 1024 * 1024)
                .unwrap_or(100 * 1024 * 1024),
            max_segments: env_var_parse("RDF_WAL_MAX_SEGMENTS", 10).unwrap_or(10),
            preallocate: env_var_parse("RDF_WAL_PREALLOCATE", false).unwrap_or(false),
            direct_io: env_var_parse("RDF_WAL_DIRECT_IO", false).unwrap_or(false),
            min_free_disk_space: env_var_parse("RDF_WAL_MIN_FREE_DISK_SPACE", 1024 * 1024 * 1024)
                .unwrap_or(1024 * 1024 * 1024),

            fsync_mode: Self::parse_fsync_mode(&env_var("RDF_WAL_FSYNC_MODE", "batch_sync")),
            sync_interval: Duration::from_millis(
                env_var_parse("RDF_WAL_SYNC_INTERVAL_MS", 10).unwrap_or(10),
            ),
            group_commit: GroupCommitConfig {
                enabled: env_var_parse("RDF_WAL_GROUP_COMMIT_ENABLED", true).unwrap_or(true),
                max_wait: Duration::from_millis(
                    env_var_parse("RDF_WAL_GROUP_COMMIT_MAX_WAIT_MS", 10).unwrap_or(10),
                ),
                max_batch: env_var_parse("RDF_WAL_GROUP_COMMIT_MAX_BATCH", 100).unwrap_or(100),
            },

            rotation_policy: RotationPolicy::SizeAndTime {
                max_size: env_var_parse("RDF_WAL_ROTATION_MAX_SIZE", 100 * 1024 * 1024)
                    .unwrap_or(100 * 1024 * 1024),
                max_age: Duration::from_secs(
                    env_var_parse("RDF_WAL_ROTATION_MAX_AGE_SECS", 3600).unwrap_or(3600),
                ),
            },
            compaction_policy: CompactionPolicy::default(),

            write_buffer_size: env_var_parse("RDF_WAL_WRITE_BUFFER_SIZE", 64 * 1024)
                .unwrap_or(64 * 1024),
            max_batch_size: env_var_parse("RDF_WAL_MAX_BATCH_SIZE", 1000).unwrap_or(1000),
            pipeline_depth: env_var_parse("RDF_WAL_PIPELINE_DEPTH", 100).unwrap_or(100),
            compression: Self::parse_compression(&env_var("RDF_WAL_COMPRESSION", "none")),

            recovery_mode: RecoveryMode::BestEffort,
            corruption_tolerance: CorruptionTolerance::SkipCorrupted,
            checkpoint_interval: Duration::from_secs(
                env_var_parse("RDF_WAL_CHECKPOINT_INTERVAL_SECS", 60).unwrap_or(60),
            ),

            tenant_isolation: false,
            quota_per_tenant: None,

            metrics_enabled: env_var_parse("RDF_WAL_METRICS_ENABLED", true).unwrap_or(true),
            metrics_prefix: env_var("RDF_WAL_METRICS_PREFIX", "graphica_rdf_wal"),
            slow_write_threshold: Duration::from_millis(
                env_var_parse("RDF_WAL_SLOW_WRITE_THRESHOLD_MS", 100).unwrap_or(100),
            ),
            enable_tracing: env_var_parse("RDF_WAL_ENABLE_TRACING", true).unwrap_or(true),

            io_timeout: Some(Duration::from_secs(
                env_var_parse("RDF_WAL_IO_TIMEOUT_SECS", 30).unwrap_or(30),
            )),
        }
    }

    /// Parse FsyncMode from string
    fn parse_fsync_mode(mode_str: &str) -> FsyncMode {
        match mode_str.to_lowercase().as_str() {
            "every_write" => FsyncMode::EveryWrite,
            "batch_sync" | "batch" => FsyncMode::BatchSync,
            "on_demand" | "demand" => FsyncMode::OnDemand,
            "periodic" => FsyncMode::Periodic,
            _ => {
                tracing::warn!("Unknown fsync mode '{}', defaulting to BatchSync", mode_str);
                FsyncMode::BatchSync
            }
        }
    }

    /// Parse CompressionCodec from string
    fn parse_compression(codec_str: &str) -> Option<CompressionCodec> {
        use crate::storage::wal::CompressionCodec;
        match codec_str.to_lowercase().as_str() {
            "none" | "" => Some(CompressionCodec::None),
            "lz4" => Some(CompressionCodec::Lz4),
            "zstd" => Some(CompressionCodec::Zstd(3)), // Default level 3
            "snappy" => Some(CompressionCodec::Snappy),
            _ => {
                tracing::warn!(
                    "Unknown compression codec '{}', defaulting to None",
                    codec_str
                );
                Some(CompressionCodec::None)
            }
        }
    }
}

/// Helper: Get environment variable with default
fn env_var(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

/// Helper: Get and parse environment variable with default
fn env_var_parse<T: std::str::FromStr>(name: &str, default: T) -> Result<T>
where
    T::Err: std::fmt::Display,
{
    match std::env::var(name) {
        Ok(val) => val
            .parse()
            .map_err(|e| anyhow::anyhow!("Failed to parse {} = '{}': {}", name, val, e)),
        Err(_) => Ok(default),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        // Should succeed with all defaults
        let config = CoordinatorConfig::from_env().unwrap();
        assert_eq!(config.network.rest_port, 8080);
        assert_eq!(config.network.grpc_port, 9090);
        assert_eq!(config.kafka.brokers, "localhost:9092");
    }

    #[test]
    fn test_shard_urls_parsing() {
        std::env::set_var("SHARD_URLS", "shard-0:9090,shard-1:9091,  shard-2:9092  ");
        let config = ShardConfig::from_env().unwrap();
        assert_eq!(config.urls.len(), 3);
        assert_eq!(config.urls[0], "shard-0:9090");
        assert_eq!(config.urls[1], "shard-1:9091");
        assert_eq!(config.urls[2], "shard-2:9092");
        std::env::remove_var("SHARD_URLS");
    }

    #[test]
    fn test_empty_shard_urls() {
        std::env::remove_var("SHARD_URLS");
        let config = ShardConfig::from_env().unwrap();
        assert_eq!(config.urls.len(), 0);
    }

    #[test]
    fn test_cors_origins_parsing() {
        std::env::set_var(
            "CORS_ALLOWED_ORIGINS",
            "http://localhost:3000, http://example.com",
        );
        let config = AuthConfig::from_env().unwrap();
        assert_eq!(config.cors_allowed_origins.len(), 2);
        assert_eq!(config.cors_allowed_origins[0], "http://localhost:3000");
        assert_eq!(config.cors_allowed_origins[1], "http://example.com");
        std::env::remove_var("CORS_ALLOWED_ORIGINS");
    }

    #[test]
    fn test_validation_fails_on_same_ports() {
        std::env::set_var("REST_PORT", "8080");
        std::env::set_var("GRPC_PORT", "8080");
        let config = CoordinatorConfig::from_env().unwrap();
        assert!(config.validate().is_err());
        std::env::remove_var("REST_PORT");
        std::env::remove_var("GRPC_PORT");
    }
}
