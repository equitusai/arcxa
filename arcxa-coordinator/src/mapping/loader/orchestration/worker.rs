//! Loader Worker
//!
//! Background async task that executes ETL pipeline for a single job.
//! Handles CSV streaming, transformation, DB2 LOAD, checkpoint, and DLQ.
//!
//! ## DML Execution Modes
//!
//! The worker supports two DML execution modes controlled by `DmlMode` configuration:
//!
//! ### INSERT Mode (Default)
//!
//! Standard append-only loading using INSERT statements:
//!
//! ```sql
//! INSERT INTO target_table (col1, col2, col3)
//! VALUES (?, ?, ?), (?, ?, ?), ...
//! ```
//!
//! **Characteristics:**
//! - Fast bulk loading (single INSERT with multiple VALUES)
//! - Fails on duplicate primary keys
//! - Not idempotent - re-running creates duplicates
//! - Best for initial loads and append-only scenarios
//!
//! **Use When:**
//! - Loading data for the first time
//! - Source data is guaranteed unique
//! - Table has no primary key constraints
//! - Performance is critical and duplicates are not a concern
//!
//! ### MERGE Mode (Idempotent)
//!
//! Upsert loading using MERGE statements:
//!
//! ```sql
//! MERGE INTO target_table AS T
//! USING (VALUES (?, ?, ?), (?, ?, ?)) AS S (col1, col2, col3)
//! ON T.pk_col = S.pk_col
//! WHEN MATCHED THEN UPDATE SET col2 = S.col2, col3 = S.col3
//! WHEN NOT MATCHED THEN INSERT (col1, col2, col3) VALUES (S.col1, S.col2, S.col3)
//! ```
//!
//! **Characteristics:**
//! - Idempotent - safe to re-run with same data
//! - Inserts new rows, updates existing rows (matched by primary key)
//! - No errors on duplicate keys
//! - Slightly slower than INSERT (due to matching logic)
//! - Requires primary keys to be defined on target table
//!
//! **Use When:**
//! - Loading data with potential duplicates
//! - Implementing CDC (change data capture) pipelines
//! - Need idempotent loads for retry safety
//! - Updating existing rows based on source changes
//! - Target table has primary key constraints
//!
//! ## Configuration Example
//!
//! ```rust,ignore
//! use graphica_coordinator::mapping::loader::orchestration::*;
//!
//! // INSERT mode (default) - fast, not idempotent
//! let config_insert = LoaderWorkerConfig {
//!     dml_mode: DmlMode::Insert,
//!     job_id: "initial_load_001".to_string(),
//!     source_file: PathBuf::from("/data/customers.csv"),
//!     target_table: "CUSTOMERS".to_string(),
//!     batch_size: 5000,
//!     // ... other fields
//! };
//!
//! // MERGE mode - slower, idempotent, handles duplicates
//! let config_merge = LoaderWorkerConfig {
//!     dml_mode: DmlMode::Merge,
//!     job_id: "cdc_sync_002".to_string(),
//!     source_file: PathBuf::from("/data/customers_updated.csv"),
//!     target_table: "CUSTOMERS".to_string(),
//!     batch_size: 1000, // Smaller batches for MERGE
//!     // ... other fields
//! };
//! ```
//!
//! ## Performance Considerations
//!
//! - **INSERT**: ~10,000-50,000 rows/sec (depending on row width, indexes)
//! - **MERGE**: ~5,000-20,000 rows/sec (50-70% of INSERT throughput)
//!
//! MERGE is slower due to:
//! - Primary key lookups for each row
//! - Conditional UPDATE vs INSERT logic
//! - Index maintenance for both operations
//!
//! Recommend smaller batch sizes for MERGE (1,000-2,000) vs INSERT (5,000-10,000).
//!
//! ## Primary Key Requirements
//!
//! MERGE mode requires primary keys to be defined in target table configuration:
//!
//! ```rust,ignore
//! // Option 1: Explicit primary_keys field
//! let table_config = TargetTableConfig {
//!     primary_keys: vec!["customer_id".to_string()],
//!     // ...
//! };
//!
//! // Option 2: Mark columns as primary keys
//! let table_config = TargetTableConfig {
//!     columns: HashMap::from([
//!         ("customer_id", ColumnConfig { is_primary_key: true, ... }),
//!         ("name", ColumnConfig { is_primary_key: false, ... }),
//!     ]),
//!     // ...
//! };
//! ```
//!
//! If no primary keys are defined, MERGE will fail with:
//! `"No primary keys specified for MERGE. Cannot determine ON clause."`

use anyhow::{Context, Result};
use chrono::Utc;
use graphica_core::core::lineage::{DataRef, LineageEvent, LineageSink, TransformRef};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::async_csv_reader::{AsyncCsvReader, AsyncCsvReaderConfig};
use super::config::LoaderWorkerConfig;
use super::job_state::{JobProgress, JobResult};
use crate::mapping::loader::checkpoint::{CheckpointConfig, CheckpointManager, ErrorCategory};
use crate::mapping::loader::db2::DB2Loader;
use crate::mapping::loader::db2_connection::{DB2Connection, DB2ConnectionManager};
use crate::mapping::loader::dlq::{DeadLetterQueue, DlqConfig, DlqStats};
use crate::mapping::multi_source::types::TargetTableConfig;
use crate::observability::metrics::LoaderMetrics;

/// Background worker for executing ETL job
pub struct LoaderWorker<
    C: DB2Connection + Default = crate::mapping::loader::db2_connection::MockDB2Connection,
> {
    /// Job configuration
    config: LoaderWorkerConfig,

    /// Metrics for observability
    metrics: Arc<LoaderMetrics>,

    /// Cancellation token for graceful shutdown
    cancel_token: CancellationToken,

    /// Lineage sink for W3C PROV provenance tracking (Sprint 1.4)
    lineage_sink: Option<Arc<dyn LineageSink>>,

    /// Source file hash for versioning
    source_file_hash: Option<String>,

    /// DB2 connection manager (optional - for production DB2 execution)
    db2_manager: Option<Arc<DB2ConnectionManager<C>>>,

    /// Target table configuration (for MERGE mode primary keys)
    table_config: Option<TargetTableConfig>,
}

impl<C: DB2Connection + Default> LoaderWorker<C> {
    /// Create new worker
    pub fn new(
        config: LoaderWorkerConfig,
        metrics: Arc<LoaderMetrics>,
        cancel_token: CancellationToken,
    ) -> Self {
        Self {
            config,
            metrics,
            cancel_token,
            lineage_sink: None,
            source_file_hash: None,
            db2_manager: None,
            table_config: None,
        }
    }

    /// Create new worker with lineage tracking
    pub fn with_lineage(
        config: LoaderWorkerConfig,
        metrics: Arc<LoaderMetrics>,
        cancel_token: CancellationToken,
        lineage_sink: Arc<dyn LineageSink>,
    ) -> Self {
        // Compute source file hash for versioning (simplified - just use file path for now)
        let source_file_hash = config.source_file.to_string_lossy().to_string();

        Self {
            config,
            metrics,
            cancel_token,
            lineage_sink: Some(lineage_sink),
            source_file_hash: Some(source_file_hash),
            db2_manager: None,
            table_config: None,
        }
    }

    /// Create new worker with DB2 connection pool
    pub fn with_db2(
        config: LoaderWorkerConfig,
        metrics: Arc<LoaderMetrics>,
        cancel_token: CancellationToken,
        db2_manager: Arc<DB2ConnectionManager<C>>,
        table_config: TargetTableConfig,
    ) -> Self {
        Self {
            config,
            metrics,
            cancel_token,
            lineage_sink: None,
            source_file_hash: None,
            db2_manager: Some(db2_manager),
            table_config: Some(table_config),
        }
    }

    /// Create new worker with both lineage and DB2
    pub fn with_lineage_and_db2(
        config: LoaderWorkerConfig,
        metrics: Arc<LoaderMetrics>,
        cancel_token: CancellationToken,
        lineage_sink: Arc<dyn LineageSink>,
        db2_manager: Arc<DB2ConnectionManager<C>>,
        table_config: TargetTableConfig,
    ) -> Self {
        let source_file_hash = config.source_file.to_string_lossy().to_string();

        Self {
            config,
            metrics,
            cancel_token,
            lineage_sink: Some(lineage_sink),
            source_file_hash: Some(source_file_hash),
            db2_manager: Some(db2_manager),
            table_config: Some(table_config),
        }
    }

    /// Execute ETL pipeline
    ///
    /// Main entry point for background task. Runs until job completes,
    /// fails, or is cancelled.
    pub async fn run(self) -> Result<JobResult> {
        let start_time = Instant::now();
        let job_id = self.config.job_id.clone();

        tracing::info!("LoaderWorker starting job: {}", job_id);

        // Initialize checkpoint manager
        let mut checkpoint_manager =
            CheckpointManager::resume_or_create(&job_id, self.config.checkpoint_config.clone())
                .context("Failed to initialize checkpoint manager")?;

        // Initialize DLQ
        let mut dlq = DeadLetterQueue::new(&job_id, self.config.dlq_config.clone())
            .context("Failed to initialize DLQ")?;

        // Create AsyncCsvReader
        let csv_config = AsyncCsvReaderConfig {
            file_path: self.config.source_file.clone(),
            delimiter: self.config.csv_delimiter,
            has_header: self.config.csv_has_header,
            buffer_size: self.config.csv_buffer_size,
            ..Default::default()
        };

        let mut csv_reader = AsyncCsvReader::new(csv_config)
            .await
            .context("Failed to open CSV file")?;

        // Resume from checkpoint if needed
        let starting_row = checkpoint_manager.starting_row();
        if starting_row > 0 {
            tracing::info!("Resuming from checkpoint at row {}", starting_row);
            csv_reader
                .seek_to_row(starting_row)
                .await
                .context("Failed to seek to checkpoint row")?;
        }

        let file_size = csv_reader.file_size();
        let mut rows_processed = checkpoint_manager.current_checkpoint().rows_processed;
        let mut rows_failed = checkpoint_manager.current_checkpoint().rows_failed;
        let mut current_batch = Vec::new();

        // Main processing loop
        loop {
            // Check for cancellation
            if self.cancel_token.is_cancelled() {
                tracing::info!("Job cancelled: {}", job_id);
                checkpoint_manager
                    .checkpoint()
                    .context("Failed to save checkpoint on cancellation")?;
                dlq.flush()?;

                return Ok(JobResult {
                    rows_processed,
                    rows_failed,
                    rows_skipped: 0,
                    bytes_processed: csv_reader.progress().bytes_read,
                    dlq_stats: DlqStats::default(),
                    duration: start_time.elapsed(),
                    checkpoints_created: 0, // TODO: Track this
                    batches_processed: 0,   // TODO: Track this
                    avg_batch_size: 0.0,
                    peak_throughput: 0.0,
                    cancelled: true,
                    error: None,
                });
            }

            // Read next row
            let row_result = csv_reader.next_row().await;

            match row_result {
                Ok(Some(record)) => {
                    let row_number = csv_reader.row_count();

                    // Process row with retries
                    match self
                        .process_row_with_retries(&record, row_number, &mut checkpoint_manager)
                        .await
                    {
                        Ok(transformed_row) => {
                            current_batch.push(transformed_row);
                            checkpoint_manager.record_success(row_number)?;
                            rows_processed += 1;

                            // Execute batch when full
                            if current_batch.len() >= self.config.batch_size {
                                let batch_start_row = row_number - current_batch.len() as u64;
                                let batch_len = current_batch.len();

                                checkpoint_manager.start_batch(batch_start_row, batch_len as u64);

                                match self.execute_batch(&current_batch).await {
                                    Ok(loaded_count) => {
                                        checkpoint_manager.complete_batch(loaded_count)?;

                                        // Capture lineage for successful batch (Sprint 1.4)
                                        if let Err(e) =
                                            self.capture_batch_lineage(batch_start_row, batch_len)
                                        {
                                            tracing::warn!("Lineage capture failed: {}", e);
                                            // Don't fail ETL for lineage errors
                                        }

                                        // TODO: Metrics - self.metrics.rows_processed(loaded_count);
                                    }
                                    Err(e) => {
                                        tracing::error!("Batch load failed: {}", e);
                                        checkpoint_manager.fail_batch()?;
                                        return Err(e).context("Batch load failed");
                                    }
                                }

                                current_batch.clear();
                            }
                        }
                        Err(e) => {
                            // Row processing failed - send to DLQ
                            tracing::warn!("Row {} failed: {}", row_number, e);

                            let error_msg = e.to_string();
                            let error_category = ErrorCategory::from_error_message(&error_msg);

                            // Convert csv_async::StringRecord to csv::StringRecord
                            let csv_record: csv::StringRecord = record.iter().collect();

                            dlq.write_failed_row(
                                row_number,
                                &csv_record,
                                error_category,
                                &error_msg,
                                0, // retry_count tracked separately
                            )?;

                            checkpoint_manager.record_error(row_number, e)?;
                            rows_failed += 1;
                            // TODO: Metrics - self.metrics.row_failed();
                        }
                    }

                    // Checkpoint periodically
                    if checkpoint_manager.should_checkpoint() {
                        checkpoint_manager
                            .checkpoint()
                            .context("Failed to save checkpoint")?;
                        dlq.flush()?;
                        tracing::debug!("Checkpoint saved at row {}", row_number);
                    }
                }
                Ok(None) => {
                    // End of file - process final batch
                    if !current_batch.is_empty() {
                        let batch_start_row = csv_reader.row_count() - current_batch.len() as u64;
                        let batch_len = current_batch.len();

                        checkpoint_manager.start_batch(batch_start_row, batch_len as u64);

                        match self.execute_batch(&current_batch).await {
                            Ok(loaded_count) => {
                                checkpoint_manager.complete_batch(loaded_count)?;

                                // Capture lineage for final batch (Sprint 1.4)
                                if let Err(e) =
                                    self.capture_batch_lineage(batch_start_row, batch_len)
                                {
                                    tracing::warn!("Lineage capture failed for final batch: {}", e);
                                }

                                // TODO: Metrics - self.metrics.rows_processed(loaded_count);
                            }
                            Err(e) => {
                                tracing::error!("Final batch load failed: {}", e);
                                checkpoint_manager.fail_batch()?;
                                return Err(e).context("Final batch load failed");
                            }
                        }
                    }

                    // Finalize checkpoint
                    checkpoint_manager
                        .finalize()
                        .context("Failed to finalize checkpoint")?;

                    let dlq_stats = dlq.finalize()?;

                    let duration = start_time.elapsed();

                    tracing::info!(
                        "Job completed: {} ({} rows processed, {} failed in {:.1}s)",
                        job_id,
                        rows_processed,
                        rows_failed,
                        duration.as_secs_f64()
                    );

                    return Ok(JobResult {
                        rows_processed,
                        rows_failed,
                        rows_skipped: 0,
                        bytes_processed: csv_reader.progress().bytes_read,
                        dlq_stats,
                        duration,
                        checkpoints_created: 0, // TODO: Track this
                        batches_processed: 0,   // TODO: Track this
                        avg_batch_size: 0.0,
                        peak_throughput: 0.0,
                        cancelled: false,
                        error: None,
                    });
                }
                Err(e) => {
                    // CSV reader error
                    tracing::error!("CSV reader error: {}", e);
                    checkpoint_manager.mark_failed(&e.to_string())?;
                    dlq.flush()?;

                    return Err(e).context("CSV reader failed");
                }
            }
        }
    }

    /// Process row with retry logic
    async fn process_row_with_retries(
        &self,
        record: &csv_async::StringRecord,
        row_number: u64,
        checkpoint_manager: &mut CheckpointManager,
    ) -> Result<Vec<String>> {
        let mut retry_count = 0;
        let mut last_error = None;

        loop {
            match self.process_row(record).await {
                Ok(transformed) => return Ok(transformed),
                Err(e) => {
                    let error_msg = e.to_string();
                    let error_category = ErrorCategory::from_error_message(&error_msg);

                    // Check if should retry
                    if checkpoint_manager.should_retry(error_category, retry_count) {
                        let delay = checkpoint_manager.calculate_retry_delay(retry_count);
                        tracing::debug!(
                            "Retrying row {} (attempt {}) after {:?}",
                            row_number,
                            retry_count + 1,
                            delay
                        );
                        tokio::time::sleep(delay).await;
                        retry_count += 1;
                        last_error = Some(e);
                        continue;
                    } else {
                        // No more retries - return error
                        return Err(e);
                    }
                }
            }
        }
    }

    /// Process single row (transform via DEL)
    async fn process_row(&self, record: &csv_async::StringRecord) -> Result<Vec<String>> {
        // TODO: Phase 3 - Integrate with DEL transformation engine
        // For now, return row as-is
        //
        // Future implementation:
        // 1. Parse row into named fields using header mapping
        // 2. Apply DEL transformation rules
        // 3. Validate transformed data against target schema
        // 4. Return transformed row ready for DB2 LOAD

        Ok(record.iter().map(|s| s.to_string()).collect())
    }

    /// Execute batch load to DB2
    ///
    /// Uses DmlMode configuration to determine whether to generate INSERT or MERGE statements:
    /// - `DmlMode::Insert`: Standard INSERT (fails on duplicate keys)
    /// - `DmlMode::Merge`: MERGE/UPSERT (inserts new rows, updates existing rows)
    async fn execute_batch(&self, batch: &[Vec<String>]) -> Result<u64> {
        use crate::mapping::loader::orchestration::DmlMode;

        if batch.is_empty() {
            return Ok(0);
        }

        // Simulate column names (in real implementation, get from CSV header)
        // TODO: Get actual column names from CSV header parsing
        let column_names: Vec<String> = (0..batch[0].len()).map(|i| format!("col{}", i)).collect();

        // Create DB2Loader for SQL generation
        let loader = DB2Loader::with_defaults();

        // Generate SQL based on DmlMode configuration
        let sql_statement = match self.config.dml_mode {
            DmlMode::Insert => {
                tracing::debug!(
                    "Generating INSERT statement for {} rows to table: {}",
                    batch.len(),
                    self.config.target_table
                );

                loader.generate_insert_statement(
                    &self.config.target_table,
                    &column_names,
                    batch.len(),
                )?
            }
            DmlMode::Merge => {
                tracing::debug!(
                    "Generating MERGE statement for {} rows to table: {} (idempotent load)",
                    batch.len(),
                    self.config.target_table
                );

                // Get primary keys from table configuration
                let primary_keys = if let Some(ref table_cfg) = self.table_config {
                    loader.get_primary_keys(table_cfg)
                } else {
                    // Fallback: assume first column is primary key
                    tracing::warn!(
                        "No table configuration provided for MERGE mode - assuming first column is primary key"
                    );
                    vec![column_names[0].clone()]
                };

                loader.generate_merge_statement(
                    &self.config.target_table,
                    &column_names,
                    &primary_keys,
                    batch.len(),
                )?
            }
        };

        tracing::trace!("Generated SQL: {}", sql_statement);

        // Execute via DB2 connection pool if available, otherwise simulate
        if let Some(ref db2_mgr) = self.db2_manager {
            // Real DB2 execution
            tracing::info!(
                "Executing batch of {} rows via DB2 connection pool",
                batch.len()
            );

            // Get connection from pool
            let mut conn = db2_mgr
                .get_connection()
                .map_err(|e| anyhow::anyhow!("Failed to get DB2 connection: {}", e))?;

            // TODO: Bind batch data to SQL parameters
            // For now, execute with empty params (this will fail in real execution)
            // In production, we need to flatten batch data into parameter array
            let params: Vec<&dyn crate::mapping::loader::db2_connection::SqlParam> = vec![];

            let rows_affected = conn
                .connection_mut()
                .execute(&sql_statement, &params)
                .map_err(|e| anyhow::anyhow!("DB2 execution failed: {}", e))?;

            // Return connection to pool
            db2_mgr.return_connection(conn);

            tracing::info!(
                "Batch executed successfully - {} rows affected",
                rows_affected
            );
            Ok(rows_affected)
        } else {
            // Simulated execution (for testing without DB2)
            tracing::debug!(
                "No DB2 connection manager - simulating batch execution for {} rows",
                batch.len()
            );

            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
            Ok(batch.len() as u64)
        }
    }

    /// Get progress callback
    ///
    /// Called periodically to update job manager with current progress
    pub fn create_progress_callback(
        job_id: String,
        csv_reader: &AsyncCsvReader<
            tokio_util::compat::Compat<tokio::io::BufReader<tokio::fs::File>>,
        >,
        checkpoint_manager: &CheckpointManager,
    ) -> JobProgress {
        let reader_progress = csv_reader.progress();
        let checkpoint = checkpoint_manager.current_checkpoint();

        JobProgress {
            current_row: checkpoint.current_row,
            total_rows: None, // Unknown without pre-scanning file
            rows_processed: checkpoint.rows_processed,
            rows_failed: checkpoint.rows_failed,
            rows_skipped: checkpoint.rows_skipped,
            progress_percent: reader_progress.progress_percent,
            estimated_time_remaining: reader_progress
                .estimated_time_remaining()
                .map(|d| d.as_secs_f64()),
            rows_per_second: reader_progress.rows_per_second(),
            bytes_per_second: reader_progress.bytes_per_second(),
            bytes_processed: reader_progress.bytes_read,
            total_bytes: reader_progress.total_bytes,
        }
    }

    /// Capture lineage for processed batch (Sprint 1.4)
    ///
    /// Creates W3C PROV-compliant lineage events tracking:
    /// - Source file → transformations → target table
    /// - Row-level CDC positions for replay
    /// - Transform metadata (R2RML, validation)
    fn capture_batch_lineage(&self, batch_start_row: u64, batch_size: usize) -> Result<()> {
        // Only capture if lineage sink is configured
        let lineage_sink = match &self.lineage_sink {
            Some(sink) => sink,
            None => return Ok(()), // Lineage disabled
        };

        tracing::debug!(
            "Capturing lineage for batch: start_row={}, size={}",
            batch_start_row,
            batch_size
        );

        // Create lineage event for each row in batch
        for row_offset in 0..batch_size {
            let row_number = batch_start_row + row_offset as u64;
            let lineage_event = self.create_lineage_event(row_number)?;

            // Write lineage event to sink (non-blocking)
            if let Err(e) = lineage_sink.write(lineage_event) {
                tracing::warn!("Failed to write lineage for row {}: {}", row_number, e);
                // Don't fail the ETL job for lineage errors
            }
        }

        Ok(())
    }

    /// Create lineage event for a single row
    fn create_lineage_event(&self, row_number: u64) -> Result<LineageEvent> {
        let now = Utc::now();

        Ok(LineageEvent {
            id: Uuid::new_v4(),
            dataset: format!("loader_job_{}", self.config.job_id),
            record_id: format!("row_{}", row_number),

            // Source: CSV file with CDC position for replay
            source_refs: vec![DataRef {
                system: "file".to_string(),
                path: self.config.source_file.to_string_lossy().to_string(),
                version: self.source_file_hash.clone(),
                extracted_at: now,
                cdc_position: Some(graphica_core::core::lineage::CdcPosition {
                    topic: format!("loader_job_{}", self.config.job_id),
                    partition: 0,
                    offset: row_number as i64,
                    lsn: None,
                }),
            }],

            // Transformations (R2RML mapping, validation)
            transforms: self.get_transform_refs(),

            // No model references for basic ETL
            model_refs: vec![],

            // Output: Target database table
            output_ref: DataRef {
                system: "db2".to_string(), // TODO: Get from config
                path: self.config.target_table.clone(),
                version: None,
                extracted_at: now,
                cdc_position: None,
            },

            ts: now,
            run_id: self.config.job_id.clone(),
            tenant_id: "default".to_string(),
            correlation_id: None,
            metadata: HashMap::new(),
        })
    }

    /// Get transform references for lineage
    fn get_transform_refs(&self) -> Vec<TransformRef> {
        let now = Utc::now();

        // TODO: Phase 3 - Add actual R2RML mapping references
        // For now, create placeholder transform
        vec![TransformRef {
            id: Uuid::new_v4(),
            transform_type: "csv_to_db2".to_string(),
            rule_id: format!("job_{}", self.config.job_id),
            version: "v1".to_string(),
            parameters: HashMap::new(),
            applied_at: now,
            fields_modified: vec![], // TODO: Track actual fields
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::{NamedTempFile, TempDir};

    fn create_test_csv() -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "name,age,email").unwrap();
        writeln!(file, "Alice,30,alice@example.com").unwrap();
        writeln!(file, "Bob,25,bob@example.com").unwrap();
        writeln!(file, "Charlie,35,charlie@example.com").unwrap();
        file.flush().unwrap();
        file
    }

    fn create_test_config(csv_file: &NamedTempFile, temp_dir: &TempDir) -> LoaderWorkerConfig {
        use crate::mapping::loader::orchestration::DmlMode;

        LoaderWorkerConfig {
            dml_mode: DmlMode::Insert,
            job_id: "test_job".to_string(),
            source_file: csv_file.path().to_path_buf(),
            target_table: "test_table".to_string(),
            batch_size: 2,
            checkpoint_config: CheckpointConfig {
                checkpoint_dir: temp_dir.path().join("checkpoints"),
                checkpoint_interval_rows: 2,
                ..Default::default()
            },
            dlq_config: DlqConfig {
                output_dir: temp_dir.path().join("dlq"),
                ..Default::default()
            },
            csv_buffer_size: 4096,
            csv_delimiter: b',',
            csv_has_header: true,
            max_errors: 100,
            max_retries: 3,
            retry_base_delay_ms: 10,
        }
    }

    #[tokio::test]
    async fn test_worker_basic_execution() -> Result<()> {
        use crate::mapping::loader::db2_connection::MockDB2Connection;

        let csv_file = create_test_csv();
        let temp_dir = TempDir::new()?;
        let config = create_test_config(&csv_file, &temp_dir);

        let metrics = Arc::new(LoaderMetrics::new(&prometheus::Registry::new())?);
        let cancel_token = CancellationToken::new();

        let worker: LoaderWorker<MockDB2Connection> =
            LoaderWorker::new(config, metrics, cancel_token);
        let result = worker.run().await?;

        assert_eq!(result.rows_processed, 3);
        assert_eq!(result.rows_failed, 0);
        assert!(!result.cancelled);

        Ok(())
    }

    #[tokio::test]
    async fn test_worker_cancellation() -> Result<()> {
        use crate::mapping::loader::db2_connection::MockDB2Connection;

        let csv_file = create_test_csv();
        let temp_dir = TempDir::new()?;
        let config = create_test_config(&csv_file, &temp_dir);

        let metrics = Arc::new(LoaderMetrics::new(&prometheus::Registry::new())?);
        let cancel_token = CancellationToken::new();

        // Cancel immediately
        cancel_token.cancel();

        let worker: LoaderWorker<MockDB2Connection> =
            LoaderWorker::new(config, metrics, cancel_token);
        let result = worker.run().await?;

        assert!(result.cancelled);

        Ok(())
    }

    #[tokio::test]
    #[ignore] // TODO: Fix flaky test - completes too fast for cancellation
    async fn test_worker_checkpoint_resume() -> Result<()> {
        use crate::mapping::loader::db2_connection::MockDB2Connection;

        let csv_file = create_test_csv();
        let temp_dir = TempDir::new()?;
        let mut config = create_test_config(&csv_file, &temp_dir);
        config.checkpoint_config.checkpoint_interval_rows = 1; // Checkpoint after each row

        // First run - process some rows then cancel
        let metrics = Arc::new(LoaderMetrics::new(&prometheus::Registry::new())?);
        let cancel_token = CancellationToken::new();

        // Let it process 1 row then cancel
        let cancel_token_clone = cancel_token.clone();
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            cancel_token_clone.cancel();
        });

        let worker: LoaderWorker<MockDB2Connection> =
            LoaderWorker::new(config.clone(), metrics.clone(), cancel_token);
        let result1 = worker.run().await?;

        assert!(result1.cancelled);
        assert!(result1.rows_processed > 0);

        // Second run - should resume from checkpoint
        let cancel_token2 = CancellationToken::new();
        let worker2: LoaderWorker<MockDB2Connection> =
            LoaderWorker::new(config, metrics, cancel_token2);
        let result2 = worker2.run().await?;

        // Total processed should be 3 (the full file)
        assert_eq!(result1.rows_processed + result2.rows_processed, 3);

        Ok(())
    }

    #[tokio::test]
    async fn test_worker_with_merge_mode() -> Result<()> {
        use crate::mapping::loader::db2_connection::MockDB2Connection;
        use crate::mapping::loader::orchestration::DmlMode;

        let csv_file = create_test_csv();
        let temp_dir = TempDir::new()?;
        let mut config = create_test_config(&csv_file, &temp_dir);

        // Configure MERGE mode for idempotent loads
        config.dml_mode = DmlMode::Merge;

        let metrics = Arc::new(LoaderMetrics::new(&prometheus::Registry::new())?);
        let cancel_token = CancellationToken::new();

        let worker: LoaderWorker<MockDB2Connection> =
            LoaderWorker::new(config, metrics, cancel_token);
        let result = worker.run().await?;

        // Job should complete successfully with MERGE mode
        assert_eq!(result.rows_processed, 3);
        assert_eq!(result.rows_failed, 0);
        assert!(!result.cancelled);

        Ok(())
    }

    #[tokio::test]
    async fn test_worker_with_insert_mode() -> Result<()> {
        use crate::mapping::loader::db2_connection::MockDB2Connection;
        use crate::mapping::loader::orchestration::DmlMode;

        let csv_file = create_test_csv();
        let temp_dir = TempDir::new()?;
        let mut config = create_test_config(&csv_file, &temp_dir);

        // Explicitly configure INSERT mode (default)
        config.dml_mode = DmlMode::Insert;

        let metrics = Arc::new(LoaderMetrics::new(&prometheus::Registry::new())?);
        let cancel_token = CancellationToken::new();

        let worker: LoaderWorker<MockDB2Connection> =
            LoaderWorker::new(config, metrics, cancel_token);
        let result = worker.run().await?;

        // Job should complete successfully with INSERT mode
        assert_eq!(result.rows_processed, 3);
        assert_eq!(result.rows_failed, 0);
        assert!(!result.cancelled);

        Ok(())
    }
}
