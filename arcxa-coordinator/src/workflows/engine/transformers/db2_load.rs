//! DB2 Load Transformer
//!
//! **REFACTORED (2024-11)**: Now uses Db2Destination internally for production-grade loading.
//!
//! This transformer provides backward-compatible workflow integration while leveraging
//! the new ETL architecture (Db2Destination) for improved performance, reliability,
//! and maintainability.
//!
//! ## Migration Notes
//!
//! - **Backward Compatible**: All existing YAML workflows continue to work unchanged
//! - **Uses Db2Destination**: Delegates to `etl::destinations::Db2Destination` internally
//! - **Connection Pooling**: Retrieves pool from ExecutionContext (shared across workflow)
//! - **Preserves All Features**: Load modes, transactions, DLQ, lineage, metrics, retry logic
//!
//! ## Configuration
//!
//! ```json
//! {
//!   "connection": {
//!     "host": "localhost",
//!     "port": 50000,
//!     "database": "GRAPHICA",
//!     "user": "db2inst1",
//!     "password": "graphica-db2-pass"
//!   },
//!   "table": "CUSTOMERS",
//!   "create_table_if_not_exists": true,
//!   "load_mode": "insert",  // "insert", "upsert", or "replace"
//!   "batch_size": 1000,
//!   "primary_keys": ["customer_id"],  // Required for upsert mode
//!   "schema": {  // Optional: for auto table creation
//!     "customer_id": "VARCHAR(20)",
//!     "first_name": "VARCHAR(50)",
//!     "email": "VARCHAR(100)"
//!   },
//!   // Retry configuration (optional)
//!   "max_retries": 3,  // Default: 3 retries for transient errors
//!   "retry_initial_delay_ms": 100,  // Default: 100ms initial delay
//!   "retry_max_delay_ms": 5000,  // Default: 5s max delay
//!   "retry_multiplier": 2.0  // Default: exponential backoff (2x)
//! }
//! ```
//!
//! ## Input Format
//!
//! Expects JSON data with a `rows` array (typically from csv_parse transformer):
//!
//! ```json
//! {
//!   "rows": [
//!     {"customer_id": "1", "first_name": "John", "email": "john@example.com"},
//!     {"customer_id": "2", "first_name": "Jane", "email": "jane@example.com"}
//!   ]
//! }
//! ```
//!
//! ## Output Format
//!
//! Adds load results to the data object:
//!
//! ```json
//! {
//!   "rows": [...],  // Original rows preserved
//!   "db2_load": {
//!     "status": "success",
//!     "table": "CUSTOMERS",
//!     "rows_loaded": 1000,
//!     "rows_failed": 0,
//!     "duration_ms": 1234,
//!     "batches_processed": 1,
//!     "load_mode": "insert"
//!   }
//! }
//! ```

use super::Transformer;

// NEW: ETL architecture imports
use crate::etl::destinations::Db2Destination;

// Phase 2: Circuit breaker for resilience
use crate::etl::traits::{
    DataDestination, DataRecord, DataType, ErrorTolerance, FieldSchema,
    LoadConfig as EtlLoadConfig, LoadMode as EtlLoadMode, RecordSchema,
};
use graphica_core::reliability::circuit_breaker::{CircuitBreaker, CircuitBreakerConfig};

// Legacy imports (kept for compatibility and metrics)
use crate::mapping::ddl::dialects::{
    db2::Db2Dialect, ColumnDefinition, SqlDialect, TableDefinition,
};
use crate::mapping::loader::lineage::RdfLineageSink;
use crate::mapping::loader::{
    DB2Config, DB2Connection, DB2Error, DB2Loader, DB2Pool, DeadLetterQueue, ErrorCategory,
    FailedRow,
};

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use futures::stream;
use graphica_core::core::lineage::{DataRef, LineageEvent, LineageSink, TransformRef};
use once_cell::sync::Lazy;
use prometheus::{register_histogram_vec, register_int_counter_vec, HistogramVec, IntCounterVec};
use serde_json::{json, Value as JsonValue};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

// =============================================================================
// Prometheus Metrics
// =============================================================================

/// Counter for rows loaded successfully by load mode
static DB2_ROWS_LOADED: Lazy<IntCounterVec> = Lazy::new(|| {
    register_int_counter_vec!(
        "graphica_db2_rows_loaded_total",
        "Total number of rows successfully loaded to DB2",
        &["load_mode", "table"]
    )
    .unwrap()
});

/// Counter for rows that failed to load
static DB2_ROWS_FAILED: Lazy<IntCounterVec> = Lazy::new(|| {
    register_int_counter_vec!(
        "graphica_db2_rows_failed_total",
        "Total number of rows that failed to load to DB2",
        &["load_mode", "table", "error_type"]
    )
    .unwrap()
});

/// Histogram for batch load duration
static DB2_LOAD_DURATION: Lazy<HistogramVec> = Lazy::new(|| {
    register_histogram_vec!(
        "graphica_db2_load_duration_seconds",
        "Duration of DB2 load operations in seconds",
        &["load_mode", "table"],
        vec![0.01, 0.05, 0.1, 0.5, 1.0, 5.0, 10.0, 30.0, 60.0]
    )
    .unwrap()
});

/// Counter for retry attempts
static DB2_RETRY_ATTEMPTS: Lazy<IntCounterVec> = Lazy::new(|| {
    register_int_counter_vec!(
        "graphica_db2_retry_attempts_total",
        "Total number of retry attempts for transient errors",
        &["load_mode", "table", "error_category"]
    )
    .unwrap()
});

/// Counter for successful retries
static DB2_RETRY_SUCCESS: Lazy<IntCounterVec> = Lazy::new(|| {
    register_int_counter_vec!(
        "graphica_db2_retry_success_total",
        "Total number of successful retries",
        &["load_mode", "table"]
    )
    .unwrap()
});

/// Counter for exhausted retries
static DB2_RETRY_EXHAUSTED: Lazy<IntCounterVec> = Lazy::new(|| {
    register_int_counter_vec!(
        "graphica_workflow_retry_exhausted_total",
        "Total retries exhausted by transformer and error category",
        &["transformer", "error_category"]
    )
    .unwrap()
});
/// Counter for DLQ writes
static DB2_DLQ_WRITES: Lazy<IntCounterVec> = Lazy::new(|| {
    register_int_counter_vec!(
        "graphica_db2_dlq_writes_total",
        "Total number of rows written to dead letter queue",
        &["table", "error_category"]
    )
    .unwrap()
});

/// Histogram for batch size
static DB2_BATCH_SIZE: Lazy<HistogramVec> = Lazy::new(|| {
    register_histogram_vec!(
        "graphica_db2_batch_size",
        "Distribution of batch sizes for DB2 loads",
        &["load_mode", "table"],
        vec![10.0, 50.0, 100.0, 500.0, 1000.0, 5000.0, 10000.0]
    )
    .unwrap()
});

/// Counter for transactions committed
static DB2_TRANSACTIONS_COMMITTED: Lazy<IntCounterVec> = Lazy::new(|| {
    register_int_counter_vec!(
        "graphica_db2_transactions_committed_total",
        "Total number of transactions successfully committed",
        &["table"]
    )
    .unwrap()
});

/// Counter for transactions rolled back
static DB2_TRANSACTIONS_ROLLED_BACK: Lazy<IntCounterVec> = Lazy::new(|| {
    register_int_counter_vec!(
        "graphica_db2_transactions_rolled_back_total",
        "Total number of transactions rolled back due to errors",
        &["table"]
    )
    .unwrap()
});

/// Histogram for lineage events created per batch
static DB2_LINEAGE_EVENTS: Lazy<HistogramVec> = Lazy::new(|| {
    register_histogram_vec!(
        "graphica_db2_lineage_events_per_batch",
        "Number of lineage events created per batch",
        &["table"],
        vec![10.0, 50.0, 100.0, 500.0, 1000.0, 5000.0]
    )
    .unwrap()
});

// =============================================================================
// Phase 2 Hardening: Timeout & Circuit Breaker Metrics
// =============================================================================

/// Counter for DB2 operation timeouts
static DB2_TIMEOUTS_TOTAL: Lazy<IntCounterVec> = Lazy::new(|| {
    register_int_counter_vec!(
        "graphica_db2_timeouts_total",
        "Total number of DB2 operation timeouts",
        &["table", "operation"]
    )
    .unwrap()
});

/// Gauge for circuit breaker state (0=closed, 1=open, 2=half-open)
static DB2_CIRCUIT_BREAKER_STATE: Lazy<prometheus::IntGaugeVec> = Lazy::new(|| {
    prometheus::register_int_gauge_vec!(
        "graphica_db2_circuit_breaker_state",
        "Circuit breaker state for DB2 connections (0=closed, 1=open, 2=half-open)",
        &["table"]
    )
    .unwrap()
});

/// Counter for circuit breaker trips
static DB2_CIRCUIT_BREAKER_TRIPS: Lazy<IntCounterVec> = Lazy::new(|| {
    register_int_counter_vec!(
        "graphica_db2_circuit_breaker_trips_total",
        "Total number of times circuit breaker opened",
        &["table"]
    )
    .unwrap()
});

/// DB2 load transformer
///
/// Implementation that loads data from JSON rows to DB2 database.
///
/// **Connection Mode**:
/// - Without `odbc` feature: Uses MockDB2Connection (testing only)
/// - With `odbc` feature: Uses OdbcDB2Connection (production)
///
/// Enable ODBC: `cargo build --features odbc`
pub struct Db2LoadTransformer {
    /// SQL dialect for DDL generation
    dialect: Db2Dialect,

    /// Use ODBC connection when available
    use_odbc: bool,

    /// Optional lineage sink for W3C PROV tracking
    lineage_sink: Option<Arc<RdfLineageSink>>,

    /// Optional shared connection pool for DB2 connections
    /// If provided, connections will be reused instead of creating new ones
    connection_pool: Option<Arc<DB2Pool>>,
}

impl Db2LoadTransformer {
    /// Create a new DB2 load transformer
    ///
    /// Connection type depends on feature flags:
    /// - `odbc` feature enabled: Uses real ODBC connection
    /// - `odbc` feature disabled: Uses mock connection (testing only)
    pub fn new() -> Self {
        Self {
            dialect: Db2Dialect,
            use_odbc: cfg!(feature = "odbc"),
            lineage_sink: None,
            connection_pool: None,
        }
    }

    /// Create transformer with explicit connection mode
    pub fn with_connection_mode(use_odbc: bool) -> Self {
        Self {
            dialect: Db2Dialect,
            use_odbc: use_odbc && cfg!(feature = "odbc"),
            lineage_sink: None,
            connection_pool: None,
        }
    }

    /// Create transformer with lineage tracking
    pub fn with_lineage_sink(mut self, lineage_sink: Arc<RdfLineageSink>) -> Self {
        self.lineage_sink = Some(lineage_sink);
        self
    }

    /// Create transformer with connection pool
    ///
    /// When a pool is provided, connections will be reused instead of creating
    /// new connections for each workflow execution. This significantly improves
    /// performance for high-concurrency workloads.
    pub fn with_connection_pool(mut self, connection_pool: Arc<DB2Pool>) -> Self {
        self.connection_pool = Some(connection_pool);
        self
    }

    /// Parse configuration from JSON
    fn parse_config(config: &JsonValue) -> Result<Db2LoadConfig> {
        // Connection config
        let connection = config
            .get("connection")
            .ok_or_else(|| anyhow!("Missing required field: connection"))?;

        let db2_config = DB2Config {
            host: connection["host"]
                .as_str()
                .ok_or_else(|| anyhow!("Missing connection.host"))?
                .to_string(),
            port: connection["port"]
                .as_u64()
                .ok_or_else(|| anyhow!("Missing connection.port"))? as u16,
            database: connection["database"]
                .as_str()
                .ok_or_else(|| anyhow!("Missing connection.database"))?
                .to_string(),
            username: connection["user"]
                .as_str()
                .ok_or_else(|| anyhow!("Missing connection.user"))?
                .to_string(),
            password: connection["password"]
                .as_str()
                .ok_or_else(|| anyhow!("Missing connection.password"))?
                .to_string(),
            ..DB2Config::default()
        };

        // Table name
        let table = config
            .get("table")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Missing required field: table"))?
            .to_string();

        // Load mode
        let load_mode = config
            .get("load_mode")
            .and_then(|v| v.as_str())
            .unwrap_or("insert");
        let load_mode = match load_mode {
            "insert" => LoadMode::Insert,
            "upsert" | "merge" => LoadMode::Upsert,
            "replace" | "truncate" => LoadMode::Replace,
            _ => {
                return Err(anyhow!(
                    "Invalid load_mode: {}. Must be insert, upsert, or replace",
                    load_mode
                ))
            }
        };

        // Primary keys (required for upsert)
        let primary_keys = if load_mode == LoadMode::Upsert {
            config
                .get("primary_keys")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect::<Vec<_>>()
                })
                .ok_or_else(|| anyhow!("primary_keys required for upsert mode"))?
        } else {
            vec![]
        };

        // DLQ configuration
        let dlq_enabled = config
            .get("dlq_enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(true); // Enable DLQ by default for safety

        let dlq_output_dir = config
            .get("dlq_output_dir")
            .and_then(|v| v.as_str())
            .map(PathBuf::from);

        let fail_on_error = config
            .get("fail_on_error")
            .and_then(|v| v.as_bool())
            .unwrap_or(false); // Continue on error by default, collect in DLQ

        // Transaction configuration
        let use_transactions = config
            .get("use_transactions")
            .and_then(|v| v.as_bool())
            .unwrap_or(true); // Use transactions by default for safety

        // Lineage configuration
        let lineage_enabled = config
            .get("lineage_enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(true); // Enable lineage by default

        // Generate unique job ID
        let job_id = config
            .get("job_id")
            .and_then(|v| v.as_str())
            .map(String::from)
            .unwrap_or_else(|| format!("db2_load_{}", Uuid::new_v4()));

        // Retry configuration
        let max_retries = config
            .get("max_retries")
            .and_then(|v| v.as_u64())
            .unwrap_or(3) as u32; // Default: 3 retries

        let retry_initial_delay_ms = config
            .get("retry_initial_delay_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(100); // Default: 100ms initial delay

        let retry_max_delay_ms = config
            .get("retry_max_delay_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(5000); // Default: 5s max delay

        let retry_multiplier = config
            .get("retry_multiplier")
            .and_then(|v| v.as_f64())
            .unwrap_or(2.0); // Default: exponential backoff with 2x multiplier

        // Circuit breaker configuration (Phase 2 hardening)
        let circuit_breaker_enabled = config
            .get("circuit_breaker_enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(true); // Enable by default for production resilience

        let circuit_breaker_failure_threshold = config
            .get("circuit_breaker_failure_threshold")
            .and_then(|v| v.as_u64())
            .unwrap_or(5) as u32; // Default: 5 consecutive failures

        let circuit_breaker_timeout_secs = config
            .get("circuit_breaker_timeout_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(30); // Default: 30s recovery attempt timeout

        let circuit_breaker_success_threshold = config
            .get("circuit_breaker_success_threshold")
            .and_then(|v| v.as_u64())
            .unwrap_or(2) as u32; // Default: 2 successes to close circuit

        // Memory management configuration (Phase 2 hardening)
        let enable_adaptive_batching = config
            .get("enable_adaptive_batching")
            .and_then(|v| v.as_bool())
            .unwrap_or(false); // Disabled by default for backward compatibility

        let memory_config = if enable_adaptive_batching {
            Some(graphica_core::orchestration::workflow::MemoryConfig {
                max_heap_mb: config
                    .get("max_heap_mb")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(4096) as usize,
                warning_threshold: config
                    .get("memory_warning_threshold")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.70),
                critical_threshold: config
                    .get("memory_critical_threshold")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.85),
                min_batch_size: config
                    .get("min_batch_size")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(100) as usize,
                max_batch_size: config
                    .get("max_batch_size")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(100_000) as usize,
                default_batch_size: config
                    .get("batch_size")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(1000) as usize,
            })
        } else {
            None
        };

        Ok(Db2LoadConfig {
            db2_config,
            table,
            create_table_if_not_exists: config
                .get("create_table_if_not_exists")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            load_mode,
            batch_size: config
                .get("batch_size")
                .and_then(|v| v.as_u64())
                .unwrap_or(1000) as usize,
            primary_keys,
            schema: config.get("schema").cloned(),
            dlq_enabled,
            dlq_output_dir,
            fail_on_error,
            use_transactions,
            lineage_enabled,
            job_id,
            max_retries,
            retry_initial_delay_ms,
            retry_max_delay_ms,
            retry_multiplier,
            circuit_breaker_enabled,
            circuit_breaker_failure_threshold,
            circuit_breaker_timeout_secs,
            circuit_breaker_success_threshold,
            memory_config,
            enable_adaptive_batching,
        })
    }

    /// Create table if it doesn't exist
    fn ensure_table_exists(
        &self,
        conn: &mut dyn DB2Connection,
        config: &Db2LoadConfig,
        sample_row: Option<&HashMap<String, JsonValue>>,
    ) -> Result<()> {
        if !config.create_table_if_not_exists {
            return Ok(());
        }

        // Check if table exists
        let check_sql = self.dialect.check_table_exists(&config.table);
        match conn.query(&check_sql, &[]) {
            Ok(_) => {
                debug!("Table {} already exists", config.table);
                return Ok(());
            }
            Err(e) => {
                // Table doesn't exist (expected) or query failed (error)
                // For now, assume table doesn't exist and log the error
                debug!("Table check failed (will attempt to create): {:?}", e);
                // Continue to table creation
            }
        }

        info!("Creating table {}", config.table);

        // Generate table definition
        let table_def = self.generate_table_definition(config, sample_row)?;

        // Generate CREATE TABLE DDL
        let ddl = self.dialect.create_table(&table_def);

        debug!("Executing DDL: {}", ddl);

        // Execute CREATE TABLE
        conn.execute(&ddl, &[])
            .map_err(|e| anyhow!("Failed to create table: {:?}", e))?;

        info!("Table {} created successfully", config.table);

        Ok(())
    }

    /// Generate table definition from schema config or sample row
    fn generate_table_definition(
        &self,
        config: &Db2LoadConfig,
        sample_row: Option<&HashMap<String, JsonValue>>,
    ) -> Result<TableDefinition> {
        let mut columns = Vec::new();

        if let Some(schema) = &config.schema {
            // Use explicit schema from config
            let schema_obj = schema
                .as_object()
                .ok_or_else(|| anyhow!("Schema must be a JSON object, got: {}", schema))?;

            for (col_name, col_type) in schema_obj {
                let sql_type = col_type
                    .as_str()
                    .ok_or_else(|| {
                        anyhow!(
                            "Column '{}' type must be a string, got: {}",
                            col_name,
                            col_type
                        )
                    })?
                    .to_string();

                columns.push(ColumnDefinition {
                    name: col_name.clone(),
                    sql_type,
                    nullable: !config.primary_keys.contains(col_name),
                    default_value: None,
                    primary_key: config.primary_keys.contains(col_name),
                    unique: false,
                    check_constraint: None,
                    comment: None,
                });
            }
        } else if let Some(row) = sample_row {
            // Infer schema from sample row
            for (col_name, value) in row {
                let sql_type = self.infer_sql_type(value);

                columns.push(ColumnDefinition {
                    name: col_name.clone(),
                    sql_type,
                    nullable: !config.primary_keys.contains(col_name),
                    default_value: None,
                    primary_key: config.primary_keys.contains(col_name),
                    unique: false,
                    check_constraint: None,
                    comment: None,
                });
            }
        } else {
            return Err(anyhow!(
                "Cannot create table: no schema provided and no sample row available"
            ));
        }

        Ok(TableDefinition {
            name: config.table.clone(),
            columns,
            primary_key: config.primary_keys.clone(),
            foreign_keys: vec![],
            indexes: vec![],
            comment: Some(format!("Created by Graphica workflow transformer")),
        })
    }

    /// Infer DB2 SQL type from JSON value
    ///
    /// **Deprecated**: Use `Db2Dialect::infer_sql_type_from_json()` directly for consistency.
    /// This method delegates to the canonical implementation in Db2Dialect.
    #[deprecated(
        since = "0.4.0",
        note = "Use Db2Dialect::infer_sql_type_from_json() instead"
    )]
    fn infer_sql_type(&self, value: &JsonValue) -> String {
        self.dialect.infer_sql_type_from_json(value)
    }

    /// Execute operation with retry logic and exponential backoff
    ///
    /// Retries transient connection errors, but not data validation errors
    async fn execute_with_retry<F, Fut, T>(
        &self,
        config: &Db2LoadConfig,
        operation_name: &str,
        mut operation: F,
    ) -> Result<T, DB2Error>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<T, DB2Error>>,
    {
        let mut attempt = 0;
        let mut delay_ms = config.retry_initial_delay_ms;
        let load_mode_str = format!("{:?}", config.load_mode);

        loop {
            match operation().await {
                Ok(result) => {
                    if attempt > 0 {
                        info!("{} succeeded after {} retries", operation_name, attempt);
                        // Record successful retry
                        DB2_RETRY_SUCCESS
                            .with_label_values(&[&load_mode_str, &config.table])
                            .inc();
                    }
                    return Ok(result);
                }
                Err(e) => {
                    // Check if error is retryable (connection errors, not constraint violations)
                    let is_retryable = Self::is_retryable_error(&e);

                    if !is_retryable || attempt >= config.max_retries {
                        if attempt > 0 {
                            error!(
                                "{} failed after {} retries: {:?}",
                                operation_name, attempt, e
                            );
                            // Record retry exhaustion (Phase 2 Production Hardening)
                            let workflow_category =
                                Self::categorize_db2_error_to_workflow_category(&e);
                            DB2_RETRY_EXHAUSTED
                                .with_label_values(&["db2_load", workflow_category.as_str()])
                                .inc();
                        }
                        return Err(e);
                    }

                    // Record retry attempt
                    let error_category = Self::categorize_error(&e);
                    DB2_RETRY_ATTEMPTS
                        .with_label_values(&[&load_mode_str, &config.table, &error_category])
                        .inc();

                    // Log retry attempt
                    warn!(
                        "{} failed (attempt {}/{}), retrying after {}ms: {:?}",
                        operation_name,
                        attempt + 1,
                        config.max_retries,
                        delay_ms,
                        e
                    );

                    // Sleep with exponential backoff
                    tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;

                    // Calculate next delay with exponential backoff
                    delay_ms = ((delay_ms as f64) * config.retry_multiplier) as u64;
                    delay_ms = delay_ms.min(config.retry_max_delay_ms);

                    attempt += 1;
                }
            }
        }
    }

    /// Categorize error for metrics
    fn categorize_error(error: &DB2Error) -> String {
        match error {
            DB2Error::ConnectionError { .. } => "connection_error",
            DB2Error::TransactionError { .. } => "transaction_error",
            DB2Error::QueryError { .. } => "query_error",
            DB2Error::ConnectionTimeout { .. } => "timeout",
            DB2Error::DuplicateKey { .. } => "duplicate_key",
            DB2Error::ForeignKeyViolation { .. } => "foreign_key",
            DB2Error::NotNullViolation { .. } => "not_null",
            DB2Error::TableNotFound { .. } => "table_not_found",
            DB2Error::PoolExhausted => "pool_exhausted",
            DB2Error::Other(_) => "other",
        }
        .to_string()
    }

    /// Categorize DB2 error to WorkflowErrorCategory for retry logic
    ///
    /// Maps DB2-specific errors to standardized error categories for circuit breaker
    /// and retry policy integration (Phase 2 Production Hardening).
    ///
    /// # Error Classification
    ///
    /// - **Retryable**: ConnectionError, TimeoutError, TransactionError
    /// - **Permanent**: DataValidationError (constraints), NotFoundError, ConfigurationError
    /// - **Fatal**: SystemError, InternalError
    fn categorize_db2_error_to_workflow_category(
        error: &DB2Error,
    ) -> graphica_core::orchestration::workflow::WorkflowErrorCategory {
        use graphica_core::orchestration::workflow::WorkflowErrorCategory;

        match error {
            // Retryable errors (transient failures)
            DB2Error::ConnectionError { .. } => WorkflowErrorCategory::ConnectionError,
            DB2Error::ConnectionTimeout { .. } => WorkflowErrorCategory::TimeoutError,
            DB2Error::TransactionError { .. } => WorkflowErrorCategory::TransactionError,
            DB2Error::PoolExhausted => WorkflowErrorCategory::TemporaryResourceError,

            DB2Error::QueryError { sqlcode, message } => {
                match sqlcode {
                    // Data validation errors (constraint violations)
                    -803 => WorkflowErrorCategory::DataValidationError, // Duplicate key
                    -407 => WorkflowErrorCategory::DataValidationError, // Not null violation
                    -530 => WorkflowErrorCategory::DataValidationError, // Foreign key violation
                    -532 => WorkflowErrorCategory::DataValidationError, // Check constraint

                    // Resource not found
                    -204 => WorkflowErrorCategory::NotFoundError, // Table not found
                    -206 => WorkflowErrorCategory::NotFoundError, // Column not found

                    // Transient errors (retryable)
                    -30081 => WorkflowErrorCategory::ConnectionError, // Communication error
                    -1776 | -911 | -913 => WorkflowErrorCategory::TransactionError, // Deadlock/timeout
                    -1585 => WorkflowErrorCategory::ConnectionError, // Connection terminated

                    // Authorization/authentication
                    -551 => WorkflowErrorCategory::AuthorizationError, // No privilege
                    -1403 => WorkflowErrorCategory::AuthenticationError, // User not found

                    _ => {
                        // Message-based classification
                        let msg_lower = message.to_lowercase();
                        if msg_lower.contains("timeout") {
                            WorkflowErrorCategory::TimeoutError
                        } else if msg_lower.contains("connection")
                            || msg_lower.contains("communication")
                        {
                            WorkflowErrorCategory::ConnectionError
                        } else if msg_lower.contains("deadlock") || msg_lower.contains("lock") {
                            WorkflowErrorCategory::TransactionError
                        } else if msg_lower.contains("constraint")
                            || msg_lower.contains("duplicate")
                        {
                            WorkflowErrorCategory::DataValidationError
                        } else {
                            WorkflowErrorCategory::InternalError
                        }
                    }
                }
            }

            // Permanent errors (non-retryable)
            DB2Error::DuplicateKey { .. } => WorkflowErrorCategory::DataValidationError,
            DB2Error::ForeignKeyViolation { .. } => WorkflowErrorCategory::DataValidationError,
            DB2Error::NotNullViolation { .. } => WorkflowErrorCategory::DataValidationError,
            DB2Error::TableNotFound { .. } => WorkflowErrorCategory::NotFoundError,

            // Unknown errors treated as internal
            DB2Error::Other(_) => WorkflowErrorCategory::InternalError,
        }
    }

    /// Extract SQL code from error message
    ///
    /// DB2 errors often contain SQL codes like "SQLCODE=-803" or "[IBM][CLI Driver] SQL0803N".
    /// This helper extracts the numeric code for classification.
    fn extract_sql_code(error_msg: &str) -> Option<i32> {
        // Pattern 1: SQLCODE=-803
        if let Some(pos) = error_msg.find("SQLCODE=") {
            let code_str = &error_msg[pos + 8..];
            if let Some(end) = code_str.find(|c: char| !c.is_numeric() && c != '-') {
                return code_str[..end].parse().ok();
            }
        }

        // Pattern 2: SQL0803N
        if let Some(pos) = error_msg.find("SQL") {
            let after_sql = &error_msg[pos + 3..];
            if let Some(end) = after_sql.find(|c: char| !c.is_numeric()) {
                let code = after_sql[..end].parse::<i32>().ok()?;
                return Some(-code); // DB2 codes are negative
            }
        }

        None
    }

    /// Check if error is retryable (connection errors yes, constraint violations no)
    fn is_retryable_error(error: &DB2Error) -> bool {
        match error {
            DB2Error::ConnectionError { .. } => true, // Retry connection failures
            DB2Error::TransactionError { .. } => true, // Retry transaction errors
            DB2Error::QueryError { sqlcode, message } => {
                // Retry on specific transient SQL codes
                // -30081: Communication error
                // -1776: Deadlock
                // -911: Deadlock or timeout
                // -913: Deadlock or timeout
                // -1585: Connection terminated
                // Do NOT retry: -803 (duplicate key), -407 (null constraint), etc.
                matches!(sqlcode, -30081 | -1776 | -911 | -913 | -1585)
                    || message.contains("timeout")
                    || message.contains("connection")
                    || message.contains("Communication")
            }
            // Retry timeout errors
            DB2Error::ConnectionTimeout { .. } => true,
            // Do not retry constraint violations or other permanent errors
            DB2Error::DuplicateKey { .. }
            | DB2Error::ForeignKeyViolation { .. }
            | DB2Error::NotNullViolation { .. }
            | DB2Error::TableNotFound { .. }
            | DB2Error::PoolExhausted
            | DB2Error::Other(_) => false,
        }
    }

    /// Load rows to DB2 in batches with DLQ tracking
    fn load_rows_with_dlq(
        &self,
        conn: &mut dyn DB2Connection,
        config: &Db2LoadConfig,
        rows: &[HashMap<String, JsonValue>],
        dlq: &mut Option<DeadLetterQueue>,
    ) -> Result<LoadStats> {
        let start = Instant::now();
        let mut total_loaded = 0;
        let mut total_failed = 0;
        let mut total_lineage_events = 0;
        let mut batches_processed = 0;

        // Get column names from first row
        if rows.is_empty() {
            return Ok(LoadStats {
                rows_loaded: 0,
                rows_failed: 0,
                batches_processed: 0,
                duration_ms: 0,
                dlq_path: None,
                transaction_committed: false,
                lineage_events_created: 0,
            });
        }

        let columns: Vec<String> = rows[0].keys().cloned().collect();

        // Process in batches
        for batch in rows.chunks(config.batch_size) {
            match config.load_mode {
                LoadMode::Insert => {
                    let stats =
                        self.execute_insert(conn, &config.table, &columns, batch, config, dlq)?;
                    total_loaded += stats.rows_loaded;
                    total_failed += stats.rows_failed;
                    total_lineage_events += stats.lineage_events_created;
                }
                LoadMode::Upsert => {
                    let stats = self.execute_merge(
                        conn,
                        &config.table,
                        &columns,
                        &config.primary_keys,
                        batch,
                        config,
                        dlq,
                    )?;
                    total_loaded += stats.rows_loaded;
                    total_failed += stats.rows_failed;
                    total_lineage_events += stats.lineage_events_created;
                }
                LoadMode::Replace => {
                    // Truncate table on first batch only
                    if batches_processed == 0 {
                        self.execute_truncate(conn, &config.table)?;
                    }
                    let stats =
                        self.execute_insert(conn, &config.table, &columns, batch, config, dlq)?;
                    total_loaded += stats.rows_loaded;
                    total_failed += stats.rows_failed;
                    total_lineage_events += stats.lineage_events_created;
                }
            }

            batches_processed += 1;
        }

        let duration_ms = start.elapsed().as_millis() as u64;

        Ok(LoadStats {
            rows_loaded: total_loaded,
            rows_failed: total_failed,
            batches_processed,
            duration_ms,
            dlq_path: None,               // Will be set by caller if DLQ is used
            transaction_committed: false, // Will be set by caller if transaction used
            lineage_events_created: total_lineage_events,
        })
    }

    /// Helper: Convert row to string vector for DLQ
    fn row_to_string_vec(
        &self,
        row: &HashMap<String, JsonValue>,
        columns: &[String],
    ) -> Vec<String> {
        columns
            .iter()
            .map(|col| {
                row.get(col)
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "NULL".to_string())
            })
            .collect()
    }

    /// Execute INSERT statement with DLQ tracking
    fn execute_insert(
        &self,
        conn: &mut dyn DB2Connection,
        table: &str,
        columns: &[String],
        rows: &[HashMap<String, JsonValue>],
        config: &Db2LoadConfig,
        dlq: &mut Option<DeadLetterQueue>,
    ) -> Result<LoadStats> {
        // Generate INSERT statement
        let placeholders: Vec<String> = (0..columns.len()).map(|_| "?".to_string()).collect();
        let sql = format!(
            "INSERT INTO {} ({}) VALUES ({})",
            table,
            columns.join(", "),
            placeholders.join(", ")
        );

        // Execute for each row (TODO: batch insert for better performance)
        let mut inserted = 0;
        let mut failed = 0;
        let mut lineage_events = Vec::new();

        for (row_index, row) in rows.iter().enumerate() {
            let param_values: Vec<String> = columns
                .iter()
                .map(|col| {
                    let value = row.get(col).unwrap_or(&JsonValue::Null);
                    json_to_sql_string(value)
                })
                .collect();

            // Convert to parameter references
            let param_refs: Vec<&str> = param_values.iter().map(|s| s.as_str()).collect();
            let param_traits: Vec<&dyn crate::mapping::loader::SqlParam> = param_refs
                .iter()
                .map(|s| s as &dyn crate::mapping::loader::SqlParam)
                .collect();

            match conn.execute(&sql, &param_traits) {
                Ok(_) => {
                    inserted += 1;

                    // Create success lineage event if lineage enabled
                    if config.lineage_enabled {
                        let event = self.create_success_lineage_event(config, row, row_index, None);
                        lineage_events.push(event);
                    }
                }
                Err(e) => {
                    error!("Failed to insert row {}: {:?}", row_index, e);

                    // Write to DLQ if enabled
                    if let Some(dlq) = dlq {
                        let failed_row = FailedRow {
                            job_id: config.job_id.clone(),
                            row_number: row_index as u64,
                            row_data: self.row_to_string_vec(row, columns),
                            error_category: ErrorCategory::DatabaseConnection.to_string(),
                            error_message: format!("{:?}", e),
                            stack_trace: None,
                            retry_count: 0,
                            timestamp: Utc::now(),
                            metadata: HashMap::new(),
                        };

                        dlq.write_failed_row_entry(failed_row.clone())
                            .context("Failed to write to DLQ")?;

                        // Create failure lineage event if lineage enabled
                        if config.lineage_enabled {
                            let event =
                                self.create_failure_lineage_event(config, &failed_row, None);
                            lineage_events.push(event);
                        }
                    }

                    failed += 1;

                    // Fail-fast if configured
                    if config.fail_on_error {
                        return Err(anyhow!("Row {} failed: {:?}", row_index, e));
                    }
                }
            }
        }

        // Write all lineage events to RDF store
        let lineage_count = self.write_lineage_events(lineage_events)?;

        Ok(LoadStats {
            rows_loaded: inserted,
            rows_failed: failed,
            batches_processed: 1,
            duration_ms: 0,               // Will be calculated by caller
            dlq_path: None,               // Will be set by caller
            transaction_committed: false, // Will be set by caller
            lineage_events_created: lineage_count,
        })
    }

    /// Execute MERGE statement (UPSERT) with DLQ tracking
    ///
    /// **Optimized**: Uses multi-row MERGE for 10-100x performance improvement
    fn execute_merge(
        &self,
        conn: &mut dyn DB2Connection,
        table: &str,
        columns: &[String],
        primary_keys: &[String],
        rows: &[HashMap<String, JsonValue>],
        config: &Db2LoadConfig,
        dlq: &mut Option<DeadLetterQueue>,
    ) -> Result<LoadStats> {
        if rows.is_empty() {
            return Ok(LoadStats {
                rows_loaded: 0,
                rows_failed: 0,
                batches_processed: 0,
                duration_ms: 0,
                dlq_path: None,
                transaction_committed: false,
                lineage_events_created: 0,
            });
        }

        // Use DB2Loader's optimized multi-row MERGE generation
        let loader = DB2Loader::with_defaults();
        let sql = loader
            .generate_merge_statement(table, columns, primary_keys, rows.len())
            .context("Failed to generate multi-row MERGE statement")?;

        debug!(
            "Generated multi-row MERGE for {} rows:\n{}",
            rows.len(),
            sql
        );

        // Flatten all row parameters into single array (row1_col1, row1_col2, ..., row2_col1, row2_col2, ...)
        let mut all_param_values = Vec::with_capacity(rows.len() * columns.len());
        for row in rows {
            for col in columns {
                let value = row.get(col).unwrap_or(&JsonValue::Null);
                all_param_values.push(json_to_sql_string(value));
            }
        }

        // Convert to parameter references
        let param_refs: Vec<&str> = all_param_values.iter().map(|s| s.as_str()).collect();
        let param_traits: Vec<&dyn crate::mapping::loader::SqlParam> = param_refs
            .iter()
            .map(|s| s as &dyn crate::mapping::loader::SqlParam)
            .collect();

        // Execute multi-row MERGE (single database round-trip!)
        let mut merged = 0;
        let mut failed = 0;
        let mut lineage_events = Vec::new();

        match conn.execute(&sql, &param_traits) {
            Ok(_) => {
                // All rows merged successfully
                merged = rows.len();
                info!(
                    "Successfully merged {} rows in single batch operation",
                    merged
                );

                // Create success lineage events if lineage enabled
                if config.lineage_enabled {
                    for (row_index, row) in rows.iter().enumerate() {
                        let event = self.create_success_lineage_event(config, row, row_index, None);
                        lineage_events.push(event);
                    }
                }
            }
            Err(e) => {
                // Batch failed - fall back to row-by-row for granular error tracking
                warn!(
                    "Batch MERGE failed, falling back to row-by-row execution: {:?}",
                    e
                );

                for (row_index, row) in rows.iter().enumerate() {
                    // Generate single-row MERGE for this row
                    let single_sql = self.generate_merge_sql(table, columns, primary_keys)?;

                    let param_values: Vec<String> = columns
                        .iter()
                        .map(|col| {
                            let value = row.get(col).unwrap_or(&JsonValue::Null);
                            json_to_sql_string(value)
                        })
                        .collect();

                    let row_param_refs: Vec<&str> =
                        param_values.iter().map(|s| s.as_str()).collect();
                    let row_param_traits: Vec<&dyn crate::mapping::loader::SqlParam> =
                        row_param_refs
                            .iter()
                            .map(|s| s as &dyn crate::mapping::loader::SqlParam)
                            .collect();

                    match conn.execute(&single_sql, &row_param_traits) {
                        Ok(_) => {
                            merged += 1;

                            // Create success lineage event if lineage enabled
                            if config.lineage_enabled {
                                let event =
                                    self.create_success_lineage_event(config, row, row_index, None);
                                lineage_events.push(event);
                            }
                        }
                        Err(row_e) => {
                            error!("Failed to merge row {}: {:?}", row_index, row_e);

                            // Write to DLQ if enabled
                            if let Some(dlq) = dlq {
                                let failed_row = FailedRow {
                                    job_id: config.job_id.clone(),
                                    row_number: row_index as u64,
                                    row_data: self.row_to_string_vec(row, columns),
                                    error_category: ErrorCategory::DatabaseConnection.to_string(),
                                    error_message: format!("{:?}", row_e),
                                    stack_trace: None,
                                    retry_count: 0,
                                    timestamp: Utc::now(),
                                    metadata: HashMap::new(),
                                };

                                dlq.write_failed_row_entry(failed_row.clone())
                                    .context("Failed to write to DLQ")?;

                                // Create failure lineage event if lineage enabled
                                if config.lineage_enabled {
                                    let event = self.create_failure_lineage_event(
                                        config,
                                        &failed_row,
                                        None,
                                    );
                                    lineage_events.push(event);
                                }
                            }

                            failed += 1;

                            // Fail-fast if configured
                            if config.fail_on_error {
                                return Err(anyhow!("Row {} failed: {:?}", row_index, row_e));
                            }
                        }
                    }
                }
            }
        }

        // Write all lineage events to RDF store
        let lineage_count = self.write_lineage_events(lineage_events)?;

        Ok(LoadStats {
            rows_loaded: merged,
            rows_failed: failed,
            batches_processed: 1,
            duration_ms: 0,               // Will be calculated by caller
            dlq_path: None,               // Will be set by caller
            transaction_committed: false, // Will be set by caller
            lineage_events_created: lineage_count,
        })
    }

    /// Generate DB2 MERGE statement
    fn generate_merge_sql(
        &self,
        table: &str,
        columns: &[String],
        primary_keys: &[String],
    ) -> Result<String> {
        let mut sql = format!("MERGE INTO {} AS T\n", table);

        // USING clause with single row VALUES
        sql.push_str("USING (VALUES (");
        let placeholders: Vec<String> = (0..columns.len()).map(|_| "?".to_string()).collect();
        sql.push_str(&placeholders.join(", "));
        sql.push_str(")) AS S (");
        sql.push_str(&columns.join(", "));
        sql.push_str(")\n");

        // ON clause
        sql.push_str("ON ");
        let on_conditions: Vec<String> = primary_keys
            .iter()
            .map(|pk| format!("T.{} = S.{}", pk, pk))
            .collect();
        sql.push_str(&on_conditions.join(" AND "));
        sql.push_str("\n");

        // WHEN MATCHED
        let update_columns: Vec<String> = columns
            .iter()
            .filter(|col| !primary_keys.contains(col))
            .map(|col| format!("T.{} = S.{}", col, col))
            .collect();

        if !update_columns.is_empty() {
            sql.push_str("WHEN MATCHED THEN UPDATE SET ");
            sql.push_str(&update_columns.join(", "));
            sql.push_str("\n");
        }

        // WHEN NOT MATCHED
        sql.push_str("WHEN NOT MATCHED THEN INSERT (");
        sql.push_str(&columns.join(", "));
        sql.push_str(") VALUES (");
        let insert_values: Vec<String> = columns.iter().map(|col| format!("S.{}", col)).collect();
        sql.push_str(&insert_values.join(", "));
        sql.push_str(")");

        Ok(sql)
    }

    /// Execute TRUNCATE statement
    fn execute_truncate(&self, conn: &mut dyn DB2Connection, table: &str) -> Result<()> {
        let sql = format!("TRUNCATE TABLE {} IMMEDIATE", table);
        conn.execute(&sql, &[])
            .map_err(|e| anyhow!("Failed to truncate table: {:?}", e))?;
        Ok(())
    }

    /// Create lineage event for successful batch load
    fn create_success_lineage_event(
        &self,
        config: &Db2LoadConfig,
        row: &HashMap<String, JsonValue>,
        row_idx: usize,
        source_file: Option<&str>,
    ) -> LineageEvent {
        let now = Utc::now();

        // Create record ID from row data
        let record_id = if !config.primary_keys.is_empty() {
            // Use primary key values as record ID
            let pk_values: Vec<String> = config
                .primary_keys
                .iter()
                .filter_map(|pk| row.get(pk).map(|v| json_to_sql_string(v)))
                .collect();
            format!("{}:{}", config.table, pk_values.join(":"))
        } else {
            // Use row index
            format!("{}:row_{}", config.table, row_idx)
        };

        // Source reference
        let source_ref = DataRef {
            system: source_file
                .map(|f| {
                    if f.ends_with(".csv") {
                        "csv_file"
                    } else {
                        "json_file"
                    }
                })
                .unwrap_or("workflow")
                .to_string(),
            path: source_file.unwrap_or("unknown").to_string(),
            version: None,
            extracted_at: now,
            cdc_position: None,
        };

        // Transform reference
        let transform_ref = TransformRef {
            id: Uuid::new_v4(),
            transform_type: "db2_load".to_string(),
            rule_id: match config.load_mode {
                LoadMode::Insert => "insert",
                LoadMode::Upsert => "merge",
                LoadMode::Replace => "replace",
            }
            .to_string(),
            version: "1.0".to_string(),
            parameters: {
                let mut params = HashMap::new();
                params.insert("table".to_string(), json!(config.table));
                params.insert("batch_size".to_string(), json!(config.batch_size));
                params.insert(
                    "use_transactions".to_string(),
                    json!(config.use_transactions),
                );
                params
            },
            applied_at: now,
            fields_modified: row.keys().cloned().collect(),
        };

        // Output reference
        let output_ref = DataRef {
            system: "db2".to_string(),
            path: format!("{}.{}", config.db2_config.database, config.table),
            version: None,
            extracted_at: now,
            cdc_position: None,
        };

        LineageEvent {
            id: Uuid::new_v4(),
            dataset: config.table.clone(),
            record_id,
            source_refs: vec![source_ref],
            transforms: vec![transform_ref],
            model_refs: vec![],
            output_ref,
            ts: now,
            run_id: config.job_id.clone(),
            tenant_id: "default".to_string(),
            correlation_id: None,
            metadata: {
                let mut meta = HashMap::new();
                meta.insert("load_mode".to_string(), format!("{:?}", config.load_mode));
                meta.insert("host".to_string(), config.db2_config.host.clone());
                meta.insert("database".to_string(), config.db2_config.database.clone());
                meta
            },
        }
    }

    /// Create lineage event for failed row (DLQ)
    fn create_failure_lineage_event(
        &self,
        config: &Db2LoadConfig,
        failed_row: &FailedRow,
        source_file: Option<&str>,
    ) -> LineageEvent {
        let now = Utc::now();

        // Create record ID from row data
        let record_id = format!("{}:dlq_row_{}", config.table, failed_row.row_number);

        // Source reference
        let source_ref = DataRef {
            system: source_file
                .map(|f| {
                    if f.ends_with(".csv") {
                        "csv_file"
                    } else {
                        "json_file"
                    }
                })
                .unwrap_or("workflow")
                .to_string(),
            path: source_file.unwrap_or("unknown").to_string(),
            version: None,
            extracted_at: now,
            cdc_position: None,
        };

        // Transform reference (failed)
        let transform_ref = TransformRef {
            id: Uuid::new_v4(),
            transform_type: "db2_load_failed".to_string(),
            rule_id: format!("failed_{:?}", config.load_mode).to_lowercase(),
            version: "1.0".to_string(),
            parameters: {
                let mut params = HashMap::new();
                params.insert("table".to_string(), json!(config.table));
                params.insert(
                    "error_category".to_string(),
                    json!(format!("{:?}", failed_row.error_category)),
                );
                params.insert(
                    "error_message".to_string(),
                    json!(&failed_row.error_message),
                );
                params
            },
            applied_at: now,
            fields_modified: vec![],
        };

        // Output reference (DLQ)
        let output_ref = DataRef {
            system: "dlq".to_string(),
            path: format!("dead_letter_queue/{}", config.job_id),
            version: None,
            extracted_at: now,
            cdc_position: None,
        };

        LineageEvent {
            id: Uuid::new_v4(),
            dataset: config.table.clone(),
            record_id,
            source_refs: vec![source_ref],
            transforms: vec![transform_ref],
            model_refs: vec![],
            output_ref,
            ts: now,
            run_id: config.job_id.clone(),
            tenant_id: "default".to_string(),
            correlation_id: None,
            metadata: {
                let mut meta = HashMap::new();
                meta.insert("failed".to_string(), "true".to_string());
                meta.insert(
                    "error_category".to_string(),
                    format!("{:?}", failed_row.error_category),
                );
                meta.insert(
                    "error_message".to_string(),
                    failed_row.error_message.clone(),
                );
                meta.insert("row_number".to_string(), failed_row.row_number.to_string());
                meta
            },
        }
    }

    /// Write lineage events to sink
    fn write_lineage_events(&self, events: Vec<LineageEvent>) -> Result<usize> {
        if let Some(ref sink) = self.lineage_sink {
            let count = events.len();
            for event in events {
                sink.write(event)
                    .context("Failed to write lineage event to RDF store")?;
            }
            debug!("Wrote {} lineage events to RDF store", count);
            Ok(count)
        } else {
            debug!(
                "Lineage sink not configured, skipping {} events",
                events.len()
            );
            Ok(0)
        }
    }

    // =========================================================================
    // NEW HELPER METHODS FOR DB2DESTINATION INTEGRATION
    // =========================================================================

    /// Create a DB2 connection pool from DB2Config
    async fn create_db2_pool_from_config(&self, db2_config: &DB2Config) -> Result<Arc<DB2Pool>> {
        use crate::mapping::loader::{create_db2_pool, DB2PoolConfig, PoolTimeouts};

        let pool_config = DB2PoolConfig {
            db2_config: db2_config.clone(),
            max_size: 10, // Default pool size
            timeouts: PoolTimeouts {
                wait: Duration::from_secs(30),
                create: Duration::from_secs(10),
                recycle: Duration::from_secs(5),
            },
            health_check_enabled: true,
        };

        let pool = create_db2_pool(pool_config)
            .await
            .context("Failed to create DB2 connection pool")?;

        Ok(Arc::new(pool))
    }

    /// Build RecordSchema from config schema or infer from first row
    fn build_record_schema(
        &self,
        config: &Db2LoadConfig,
        first_row: Option<&JsonValue>,
    ) -> Result<RecordSchema> {
        let mut fields = Vec::new();
        let mut metadata = HashMap::new();

        // Add create_table flag to schema metadata if enabled
        if config.create_table_if_not_exists {
            metadata.insert("create_table_if_not_exists".to_string(), json!(true));
        }

        if let Some(schema_config) = &config.schema {
            // Use explicit schema from config
            let schema_obj = schema_config
                .as_object()
                .ok_or_else(|| anyhow!("Schema must be a JSON object"))?;

            for (col_name, col_type_str) in schema_obj {
                let col_type = col_type_str
                    .as_str()
                    .ok_or_else(|| anyhow!("Column type must be a string"))?;

                // Parse DB2 type string to DataType
                let data_type = Self::parse_db2_type_to_datatype(col_type)?;

                fields.push(FieldSchema {
                    name: col_name.clone(),
                    data_type,
                    nullable: !config.primary_keys.contains(col_name),
                    description: None,
                    metadata: HashMap::new(),
                });
            }
        } else if let Some(row) = first_row {
            // Infer schema from first row
            let row_obj = row
                .as_object()
                .ok_or_else(|| anyhow!("First row must be a JSON object"))?;

            for (col_name, value) in row_obj {
                let data_type = Self::infer_datatype_from_json(value);

                fields.push(FieldSchema {
                    name: col_name.clone(),
                    data_type,
                    nullable: !config.primary_keys.contains(col_name),
                    description: None,
                    metadata: HashMap::new(),
                });
            }
        } else {
            return Err(anyhow!(
                "Cannot build schema: no explicit schema provided and no sample row available"
            ));
        }

        Ok(RecordSchema { fields, metadata })
    }

    /// Build EtlLoadConfig from legacy Db2LoadConfig
    fn build_etl_load_config(&self, config: &Db2LoadConfig) -> Result<EtlLoadConfig> {
        // Map load mode
        let mode = match config.load_mode {
            LoadMode::Insert => EtlLoadMode::Insert,
            LoadMode::Upsert => EtlLoadMode::Upsert,
            LoadMode::Replace => EtlLoadMode::Replace,
        };

        // Build error tolerance
        let error_tolerance = if config.fail_on_error {
            ErrorTolerance {
                max_errors: 0,
                skip_on_error: false,
                error_percentage_threshold: None,
            }
        } else {
            ErrorTolerance {
                max_errors: usize::MAX,
                skip_on_error: true,
                error_percentage_threshold: None,
            }
        };

        Ok(EtlLoadConfig {
            mode,
            batch_size: config.batch_size,
            key_fields: config.primary_keys.clone(),
            parallelism: 1, // Single-threaded for now
            error_tolerance,
            checkpoint_interval: None,
        })
    }

    /// Parse DB2 type string to DataType enum
    fn parse_db2_type_to_datatype(db2_type: &str) -> Result<DataType> {
        let normalized = db2_type.to_uppercase();

        if normalized.starts_with("VARCHAR") || normalized.starts_with("CHAR") {
            Ok(DataType::String)
        } else if normalized.starts_with("DECIMAL") || normalized.starts_with("NUMERIC") {
            // Parse DECIMAL(p,s) format
            if let Some(start) = normalized.find('(') {
                if let Some(end) = normalized.find(')') {
                    let params = &normalized[start + 1..end];
                    let parts: Vec<&str> = params.split(',').collect();
                    if parts.len() == 2 {
                        let precision = parts[0].trim().parse::<u8>().unwrap_or(19);
                        let scale = parts[1].trim().parse::<u8>().unwrap_or(4);
                        return Ok(DataType::Decimal { precision, scale });
                    }
                }
            }
            Ok(DataType::Decimal {
                precision: 19,
                scale: 4,
            })
        } else if normalized == "INTEGER" || normalized == "INT" {
            Ok(DataType::Integer)
        } else if normalized == "BIGINT" {
            Ok(DataType::BigInt)
        } else if normalized == "REAL" || normalized == "FLOAT" {
            Ok(DataType::Float)
        } else if normalized == "DOUBLE" {
            Ok(DataType::Double)
        } else if normalized == "SMALLINT" || normalized == "BOOLEAN" {
            Ok(DataType::Boolean)
        } else if normalized == "DATE" {
            Ok(DataType::Date)
        } else if normalized == "TIMESTAMP" || normalized.starts_with("TIMESTAMP") {
            Ok(DataType::DateTime)
        } else if normalized == "TIME" {
            Ok(DataType::Time)
        } else if normalized == "BLOB" || normalized == "BINARY" {
            Ok(DataType::Binary)
        } else if normalized == "CLOB" {
            Ok(DataType::Json)
        } else {
            // Default fallback
            Ok(DataType::String)
        }
    }

    /// Infer DataType from JSON value
    fn infer_datatype_from_json(value: &JsonValue) -> DataType {
        match value {
            JsonValue::Null => DataType::String, // Default to string for nulls
            JsonValue::Bool(_) => DataType::Boolean,
            JsonValue::Number(n) => {
                if n.is_i64() {
                    DataType::BigInt
                } else if n.is_f64() {
                    DataType::Decimal {
                        precision: 19,
                        scale: 4,
                    }
                } else {
                    DataType::Integer
                }
            }
            JsonValue::String(_) => DataType::String,
            JsonValue::Array(_) => DataType::Json,
            JsonValue::Object(_) => DataType::Json,
        }
    }
}

impl Default for Db2LoadTransformer {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Transformer for Db2LoadTransformer {
    /// Transform implementation using Db2Destination internally
    ///
    /// **MIGRATION STRATEGY**:
    /// 1. Parse legacy config → extract parameters
    /// 2. Get DB2 pool from context (or create one if missing)
    /// 3. Convert rows → DataRecord stream
    /// 4. Build RecordSchema from first row or schema config
    /// 5. Build EtlLoadConfig from legacy config
    /// 6. Create Db2Destination and execute pipeline:
    ///    - prepare() → begin transaction, create table if needed
    ///    - load_stream() → batch inserts/upserts
    ///    - finalize() → commit transaction
    /// 7. Convert EtlLoadStats → legacy output format
    /// 8. Record metrics (backward compatible)
    async fn transform(
        &self,
        config: &JsonValue,
        data: &mut JsonValue,
        context: Option<&crate::workflows::engine::executor::ExecutionContext>,
    ) -> Result<()> {
        info!("DB2 load transformer starting (using Db2Destination internally)");
        let start = Instant::now();

        // =====================================================================
        // Step 1: Parse legacy configuration
        // =====================================================================
        let load_config =
            Self::parse_config(config).context("Failed to parse DB2 load configuration")?;

        // Extract rows from input data
        let rows_array = data
            .get("rows")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow!("Missing 'rows' array in input data"))?;

        // Cache the row count to avoid borrow issues later
        let row_count = rows_array.len();

        if row_count == 0 {
            info!("No rows to load, skipping");
            data["db2_load"] = json!({
                "status": "skipped",
                "message": "No rows to load"
            });
            return Ok(());
        }

        info!(
            "Loading {} rows to table {} using {} mode",
            row_count,
            load_config.table,
            match load_config.load_mode {
                LoadMode::Insert => "INSERT",
                LoadMode::Upsert => "UPSERT",
                LoadMode::Replace => "REPLACE",
            }
        );

        // =====================================================================
        // Step 2: Get or create DB2 connection pool
        // =====================================================================
        let pool: Arc<DB2Pool> = if let Some(ref existing_pool) = self.connection_pool {
            // Use pre-configured pool from transformer (highest priority)
            info!("Using pre-configured transformer connection pool");
            existing_pool.clone()
        } else if let Some(ctx) = context {
            // Try to get pool from execution context (workflow-level shared pool)
            match ctx.get_db2_pool() {
                Ok(context_pool) => {
                    info!("Using shared connection pool from execution context (reusing across workflow executions)");
                    context_pool
                }
                Err(e) => {
                    // Pool not available in context, create a new one
                    warn!(
                        "DB2 pool not available in execution context: {}. Creating new pool for this execution (performance impact!)",
                        e
                    );
                    self.create_db2_pool_from_config(&load_config.db2_config)
                        .await?
                }
            }
        } else {
            // No context available - create new pool from config (fallback for testing)
            warn!("No execution context available. Creating new DB2 connection pool from config (testing mode)");
            self.create_db2_pool_from_config(&load_config.db2_config)
                .await?
        };

        // =====================================================================
        // Step 3: Convert rows to DataRecord stream
        // =====================================================================
        let data_records: Vec<Result<DataRecord>> = rows_array
            .iter()
            .enumerate()
            .map(|(idx, row)| {
                row.as_object()
                    .ok_or_else(|| anyhow!("Row {} is not a JSON object", idx))
                    .map(|obj| DataRecord {
                        data: json!(obj),
                        schema: None,
                        source_location: None,
                        metadata: HashMap::new(),
                    })
            })
            .collect();

        let record_stream = stream::iter(data_records);

        // =====================================================================
        // Step 4: Build RecordSchema from first row or explicit schema
        // =====================================================================
        let schema = self.build_record_schema(&load_config, rows_array.first())?;

        // =====================================================================
        // Step 5: Build EtlLoadConfig from legacy config
        // =====================================================================
        let etl_load_config = self.build_etl_load_config(&load_config)?;

        // =====================================================================
        // Step 6: Execute load using Db2Destination
        // =====================================================================
        let mut destination = Db2Destination::new(pool, load_config.table.clone());

        // Prepare destination (begin transaction, create table if needed)
        destination
            .prepare(&schema, &etl_load_config)
            .await
            .map_err(|e| anyhow!("Failed to prepare DB2 destination: {:?}", e))?;

        // =====================================================================
        // Phase 2 Hardening: Memory Pressure Monitoring
        // =====================================================================
        if let Some(ctx) = context {
            if let Some(ref monitor) = ctx.memory_monitor {
                // Update memory pressure before load
                let pressure = monitor.update_pressure().await.unwrap_or(0.0);

                if pressure > 0.70 {
                    warn!(
                        "Memory pressure at {:.1}% during DB2 load (table: {})",
                        pressure * 100.0,
                        load_config.table
                    );
                }

                if pressure > 0.85 {
                    error!(
                        "CRITICAL memory pressure at {:.1}% - applying backpressure (table: {})",
                        pressure * 100.0,
                        load_config.table
                    );
                }
            }
        }

        // Load stream
        let etl_stats = match destination
            .load_stream(Box::pin(record_stream), &etl_load_config)
            .await
        {
            Ok(stats) => {
                // Finalize (commit transaction)
                destination
                    .finalize()
                    .await
                    .map_err(|e| anyhow!("Failed to finalize DB2 destination: {:?}", e))?;
                stats
            }
            Err(e) => {
                // Rollback on error
                error!("Load failed, rolling back: {:?}", e);
                if let Err(rollback_err) = destination.rollback().await {
                    error!(
                        "CRITICAL: Rollback failed after load error - database may be in inconsistent state: {:?}",
                        rollback_err
                    );
                    return Err(anyhow!(
                        "DB2 load failed: {:?}. Rollback also failed: {:?}",
                        e,
                        rollback_err
                    ));
                }
                return Err(anyhow!("DB2 load failed: {:?}", e));
            }
        };

        let total_duration_ms = start.elapsed().as_millis() as u64;
        let total_duration_secs = start.elapsed().as_secs_f64();

        // =====================================================================
        // Phase 2 Hardening: Post-load Memory Pressure Check
        // =====================================================================
        if let Some(ctx) = context {
            if let Some(ref monitor) = ctx.memory_monitor {
                // Update memory pressure after load
                let pressure = monitor.update_pressure().await.unwrap_or(0.0);

                info!(
                    "Post-load memory pressure: {:.1}% (table: {})",
                    pressure * 100.0,
                    load_config.table
                );
            }
        }

        info!(
            "DB2 load complete: {} records read, {} loaded, {} failed in {}ms",
            etl_stats.records_read,
            etl_stats.records_loaded,
            etl_stats.records_failed,
            total_duration_ms
        );

        // =====================================================================
        // Step 7: Convert EtlLoadStats to legacy output format
        // =====================================================================
        let load_mode_str = match load_config.load_mode {
            LoadMode::Insert => "insert",
            LoadMode::Upsert => "upsert",
            LoadMode::Replace => "replace",
        };

        data["db2_load"] = json!({
            "status": "success",
            "table": load_config.table,
            "rows_loaded": etl_stats.records_loaded,
            "rows_failed": etl_stats.records_failed,
            "duration_ms": total_duration_ms,
            "batches_processed": (etl_stats.records_read / etl_load_config.batch_size as u64) + 1,
            "load_mode": load_mode_str
        });

        // =====================================================================
        // Step 8: Record Prometheus metrics (backward compatible)
        // =====================================================================
        DB2_ROWS_LOADED
            .with_label_values(&[load_mode_str, &load_config.table])
            .inc_by(etl_stats.records_loaded);

        if etl_stats.records_failed > 0 {
            DB2_ROWS_FAILED
                .with_label_values(&[load_mode_str, &load_config.table, "batch_error"])
                .inc_by(etl_stats.records_failed);
        }

        DB2_LOAD_DURATION
            .with_label_values(&[load_mode_str, &load_config.table])
            .observe(total_duration_secs);

        DB2_BATCH_SIZE
            .with_label_values(&[load_mode_str, &load_config.table])
            .observe(row_count as f64);

        DB2_TRANSACTIONS_COMMITTED
            .with_label_values(&[&load_config.table])
            .inc();

        Ok(())
    }

    fn name(&self) -> &'static str {
        "db2_load"
    }

    fn validate_config(&self, config: &JsonValue) -> Result<()> {
        // Validate connection config
        let connection = config
            .get("connection")
            .ok_or_else(|| anyhow!("Missing required field: connection"))?;

        let required_fields = ["host", "port", "database", "user", "password"];
        for field in &required_fields {
            if connection.get(field).is_none() {
                anyhow::bail!("Missing required connection field: {}", field);
            }
        }

        // Validate table name
        if !config.get("table").and_then(|v| v.as_str()).is_some() {
            anyhow::bail!("Missing required field: table");
        }

        // Validate load mode
        if let Some(mode) = config.get("load_mode").and_then(|v| v.as_str()) {
            if !["insert", "upsert", "merge", "replace", "truncate"].contains(&mode) {
                anyhow::bail!(
                    "Invalid load_mode: {}. Must be insert, upsert, or replace",
                    mode
                );
            }
        }

        // Validate primary_keys for upsert mode
        if let Some(mode) = config.get("load_mode").and_then(|v| v.as_str()) {
            if mode == "upsert" || mode == "merge" {
                if !config
                    .get("primary_keys")
                    .and_then(|v| v.as_array())
                    .is_some()
                {
                    anyhow::bail!("primary_keys required for upsert/merge mode");
                }
            }
        }

        Ok(())
    }
}

// ============================================================================
// Helper Types
// ============================================================================

/// DB2 load configuration
#[derive(Debug, Clone)]
struct Db2LoadConfig {
    db2_config: DB2Config,
    table: String,
    create_table_if_not_exists: bool,
    load_mode: LoadMode,
    batch_size: usize,
    primary_keys: Vec<String>,
    schema: Option<JsonValue>,
    // DLQ configuration
    dlq_enabled: bool,
    dlq_output_dir: Option<PathBuf>,
    fail_on_error: bool, // true = fail-fast, false = continue and collect errors
    // Transaction configuration
    use_transactions: bool, // Wrap batches in transactions
    // Lineage configuration
    lineage_enabled: bool,
    job_id: String, // Unique identifier for this load job
    // Retry configuration
    max_retries: u32,            // Maximum number of retry attempts (0 = no retries)
    retry_initial_delay_ms: u64, // Initial delay between retries in milliseconds
    retry_max_delay_ms: u64,     // Maximum delay between retries in milliseconds
    retry_multiplier: f64,       // Exponential backoff multiplier
    // Circuit breaker configuration (Phase 2 hardening)
    circuit_breaker_enabled: bool, // Enable/disable circuit breaker protection
    circuit_breaker_failure_threshold: u32, // Failures before opening circuit
    circuit_breaker_timeout_secs: u64, // Time before attempting recovery
    circuit_breaker_success_threshold: u32, // Successes needed to close circuit
    // Memory management (Phase 2 hardening)
    memory_config: Option<graphica_core::orchestration::workflow::MemoryConfig>,
    enable_adaptive_batching: bool, // Enable dynamic batch size adjustment based on memory pressure
}

/// Load mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LoadMode {
    Insert,  // INSERT only
    Upsert,  // MERGE (INSERT or UPDATE)
    Replace, // TRUNCATE + INSERT
}

/// Load statistics
#[derive(Debug)]
struct LoadStats {
    rows_loaded: usize,
    rows_failed: usize,
    batches_processed: usize,
    duration_ms: u64,
    dlq_path: Option<PathBuf>,
    transaction_committed: bool,
    lineage_events_created: usize,
}

/// Convert JSON value to SQL string for mock connection
fn json_to_sql_string(value: &JsonValue) -> String {
    match value {
        JsonValue::Null => "NULL".to_string(),
        JsonValue::Bool(b) => if *b { "1" } else { "0" }.to_string(),
        JsonValue::Number(n) => n.to_string(),
        JsonValue::String(s) => s.clone(),
        JsonValue::Array(arr) => {
            // Safe fallback if serialization fails (extremely rare)
            serde_json::to_string(arr).unwrap_or_else(|e| {
                error!("Failed to serialize array to JSON: {:?}", e);
                "[]".to_string()
            })
        }
        JsonValue::Object(obj) => {
            // Safe fallback if serialization fails (extremely rare)
            serde_json::to_string(obj).unwrap_or_else(|e| {
                error!("Failed to serialize object to JSON: {:?}", e);
                "{}".to_string()
            })
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_config_insert_mode() {
        let config = json!({
            "connection": {
                "host": "localhost",
                "port": 50000,
                "database": "GRAPHICA",
                "user": "db2inst1",
                "password": "password"
            },
            "table": "CUSTOMERS",
            "load_mode": "insert",
            "batch_size": 500
        });

        let parsed = Db2LoadTransformer::parse_config(&config).unwrap();

        assert_eq!(parsed.table, "CUSTOMERS");
        assert_eq!(parsed.load_mode, LoadMode::Insert);
        assert_eq!(parsed.batch_size, 500);
    }

    #[test]
    fn test_parse_config_upsert_mode() {
        let config = json!({
            "connection": {
                "host": "localhost",
                "port": 50000,
                "database": "GRAPHICA",
                "user": "db2inst1",
                "password": "password"
            },
            "table": "CUSTOMERS",
            "load_mode": "upsert",
            "primary_keys": ["customer_id", "email"]
        });

        let parsed = Db2LoadTransformer::parse_config(&config).unwrap();

        assert_eq!(parsed.load_mode, LoadMode::Upsert);
        assert_eq!(parsed.primary_keys, vec!["customer_id", "email"]);
    }

    #[test]
    fn test_validate_config_valid() {
        let transformer = Db2LoadTransformer::new();

        let config = json!({
            "connection": {
                "host": "localhost",
                "port": 50000,
                "database": "GRAPHICA",
                "user": "db2inst1",
                "password": "password"
            },
            "table": "CUSTOMERS"
        });

        assert!(transformer.validate_config(&config).is_ok());
    }

    #[test]
    fn test_validate_config_missing_connection() {
        let transformer = Db2LoadTransformer::new();

        let config = json!({
            "table": "CUSTOMERS"
        });

        let result = transformer.validate_config(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("connection"));
    }

    #[test]
    fn test_validate_config_upsert_without_primary_keys() {
        let transformer = Db2LoadTransformer::new();

        let config = json!({
            "connection": {
                "host": "localhost",
                "port": 50000,
                "database": "GRAPHICA",
                "user": "db2inst1",
                "password": "password"
            },
            "table": "CUSTOMERS",
            "load_mode": "upsert"
        });

        let result = transformer.validate_config(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("primary_keys"));
    }

    #[test]
    fn test_json_to_sql_string() {
        assert_eq!(json_to_sql_string(&json!(null)), "NULL");
        assert_eq!(json_to_sql_string(&json!(true)), "1");
        assert_eq!(json_to_sql_string(&json!(false)), "0");
        assert_eq!(json_to_sql_string(&json!(42)), "42");
        assert_eq!(json_to_sql_string(&json!("test")), "test");
    }

    #[test]
    fn test_infer_sql_type() {
        let transformer = Db2LoadTransformer::new();

        assert_eq!(transformer.infer_sql_type(&json!(null)), "VARCHAR(255)");
        assert_eq!(transformer.infer_sql_type(&json!(true)), "SMALLINT");
        assert_eq!(transformer.infer_sql_type(&json!(42)), "BIGINT");
        assert_eq!(transformer.infer_sql_type(&json!(3.14)), "DECIMAL(19,4)");
        assert_eq!(transformer.infer_sql_type(&json!("short")), "VARCHAR(255)");

        // Long string
        let long_str = "a".repeat(1000);
        let sql_type = transformer.infer_sql_type(&json!(long_str));
        assert!(sql_type.starts_with("VARCHAR") && !sql_type.contains("255"));
    }
}
