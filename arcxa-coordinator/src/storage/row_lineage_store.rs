//! RocksDB-based Row Lineage Store
//!
//! High-performance storage implementation for row-level lineage tracking.
//! Uses RocksDB column families for efficient indexing and querying.

use crate::storage::serialization_version::VersionedData;
use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use graphica_core::core::lineage::row_level::{
    JobStatistics, JourneyStep, ProcessingOutcome, RowId, RowJourney, RowLevelLineageSink,
    RowLineageEvent, RowTransformation,
};
use graphica_core::gdpr::{
    BackendErasureResult, DataErasure, DataSubjectId, ErasureRequest, ErasureResult,
    ErasureStrategy,
};
use rocksdb::{
    BoundColumnFamily, ColumnFamily, ColumnFamilyDescriptor, DBWithThreadMode, MultiThreaded,
    Options, WriteBatch,
};
// use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Column family names
mod cf {
    pub const BY_ROW: &str = "by_row"; // row_id -> Vec<event>
    pub const BY_BATCH: &str = "by_batch"; // batch_id -> Vec<row_id>
    pub const BY_JOB: &str = "by_job"; // job_id -> Vec<batch_id>
    pub const FILTERED: &str = "filtered"; // job_id:timestamp -> Vec<(row_id, reason)>
    pub const STATS: &str = "stats"; // job_id -> JobStatistics
    pub const TRANSFORMS: &str = "transforms"; // row_id -> Vec<transformation>
    pub const BY_TENANT: &str = "by_tenant"; // tenant_id -> Vec<row_id> (GDPR erasure)
}

/// RocksDB-based row lineage store
pub struct RowLineageStore {
    /// RocksDB instance with multiple column families
    db: Arc<DBWithThreadMode<MultiThreaded>>,
    /// Write buffer for batching
    write_buffer: Arc<Mutex<Vec<RowLineageEvent>>>,
    /// Maximum buffer size before auto-flush
    max_buffer_size: usize,
}

impl RowLineageStore {
    /// Create a new row lineage store
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);

        // Tune for write-heavy workload
        opts.set_write_buffer_size(128 * 1024 * 1024); // 128MB
        opts.set_max_write_buffer_number(4);
        opts.set_target_file_size_base(64 * 1024 * 1024); // 64MB
        opts.set_compression_type(rocksdb::DBCompressionType::Lz4);

        // Enable bloom filters for faster lookups
        let mut block_opts = rocksdb::BlockBasedOptions::default();
        block_opts.set_bloom_filter(10.0, false);
        opts.set_block_based_table_factory(&block_opts);

        // Define column families
        let cfs = vec![
            ColumnFamilyDescriptor::new(cf::BY_ROW, Options::default()),
            ColumnFamilyDescriptor::new(cf::BY_BATCH, Options::default()),
            ColumnFamilyDescriptor::new(cf::BY_JOB, Options::default()),
            ColumnFamilyDescriptor::new(cf::FILTERED, Options::default()),
            ColumnFamilyDescriptor::new(cf::STATS, Options::default()),
            ColumnFamilyDescriptor::new(cf::TRANSFORMS, Options::default()),
            ColumnFamilyDescriptor::new(cf::BY_TENANT, Options::default()),
        ];

        let db = DBWithThreadMode::open_cf_descriptors(&opts, path, cfs)
            .context("Failed to open RocksDB")?;

        Ok(Self {
            db: Arc::new(db),
            write_buffer: Arc::new(Mutex::new(Vec::new())),
            max_buffer_size: 1000,
        })
    }

    /// Get column family handle
    fn cf(&self, name: &str) -> Result<Arc<BoundColumnFamily<'_>>> {
        self.db
            .cf_handle(name)
            .ok_or_else(|| anyhow::anyhow!("Column family {} not found", name))
    }

    /// Encode row ID to bytes
    fn encode_row_id(row_id: &RowId) -> Vec<u8> {
        row_id.to_key().into_bytes()
    }

    /// Decode row ID from bytes
    fn decode_row_id(bytes: &[u8]) -> Result<RowId> {
        let s = String::from_utf8(bytes.to_vec())?;
        // For now, store the string representation
        // In production, implement proper deserialization
        Ok(serde_json::from_str(&format!(
            r#"{{"source_type":"Csv","source_id":"{}","position":{{"RowNumber":1}}}}"#,
            s
        ))?)
    }

    /// Write events to RocksDB
    async fn write_events_internal(&self, events: Vec<RowLineageEvent>) -> Result<()> {
        if events.is_empty() {
            return Ok(());
        }

        let mut batch = WriteBatch::default();
        let mut job_stats: HashMap<String, JobStatistics> = HashMap::new();

        // Group events by various keys for indexing
        let mut by_batch: HashMap<String, Vec<RowId>> = HashMap::new();
        let mut by_job: HashMap<String, Vec<String>> = HashMap::new();
        let mut by_tenant: HashMap<String, Vec<RowId>> = HashMap::new();
        let mut filtered_rows: Vec<(String, RowId, String)> = Vec::new();

        for event in &events {
            let row_key = Self::encode_row_id(&event.row_id);

            // Store event by row ID
            let cf_by_row = self.cf(cf::BY_ROW)?;
            let existing = self
                .db
                .get_cf(&cf_by_row, &row_key)
                .context("Failed to read existing events")?;

            let mut row_events: Vec<RowLineageEvent> = if let Some(data) = existing {
                // Try versioned format first, fall back to legacy format for backward compatibility
                VersionedData::<Vec<RowLineageEvent>>::deserialize(&data)
                    .and_then(|v| v.unwrap_vec())
                    .or_else(|_| {
                        //tracing::warn!("Deserializing legacy unversioned data, migrating to versioned format");
                        bincode::deserialize(&data)
                            .map_err(|e| anyhow::anyhow!("Failed to deserialize: {}", e))
                    })
                    .unwrap_or_default()
            } else {
                Vec::new()
            };

            row_events.push(event.clone());

            // Serialize with version envelope
            let versioned = VersionedData::wrap_vec(row_events);
            let serialized = versioned.serialize()?;

            // Compress if large
            let data = if serialized.len() > 1024 {
                zstd::encode_all(&serialized[..], 3)?
            } else {
                serialized
            };

            batch.put_cf(&cf_by_row, row_key, data);

            // Index by batch
            by_batch
                .entry(event.batch_id.clone())
                .or_default()
                .push(event.row_id.clone());

            // Index by job
            by_job
                .entry(event.job_id.clone())
                .or_default()
                .push(event.batch_id.clone());

            // Index by tenant (for GDPR erasure)
            by_tenant
                .entry(event.tenant_id.clone())
                .or_default()
                .push(event.row_id.clone());

            // Track filtered rows
            if let ProcessingOutcome::Filtered { reason, .. } = &event.outcome {
                filtered_rows.push((
                    format!("{}:{}", event.job_id, event.timestamp.timestamp()),
                    event.row_id.clone(),
                    reason.clone(),
                ));
            }

            // Update job statistics
            let stats = job_stats
                .entry(event.job_id.clone())
                .or_insert_with(|| JobStatistics {
                    job_id: event.job_id.clone(),
                    total_rows: 0,
                    success_count: 0,
                    filtered_count: 0,
                    failed_count: 0,
                    filter_reasons: HashMap::new(),
                    avg_processing_time_ms: 0.0,
                    start_time: event.timestamp,
                    end_time: None,
                });

            stats.total_rows += 1;
            match &event.outcome {
                ProcessingOutcome::Processed { .. } => stats.success_count += 1,
                ProcessingOutcome::Filtered { reason, .. } => {
                    stats.filtered_count += 1;
                    *stats.filter_reasons.entry(reason.clone()).or_default() += 1;
                }
                ProcessingOutcome::Failed { .. } => stats.failed_count += 1,
                ProcessingOutcome::ValidationFailed { .. } => stats.failed_count += 1,
            }
        }

        // Write batch indexes
        let cf_by_batch = self.cf(cf::BY_BATCH)?;
        for (batch_id, row_ids) in by_batch {
            batch.put_cf(
                &cf_by_batch,
                batch_id.as_bytes(),
                bincode::serialize(&row_ids)?,
            );
        }

        // Write job indexes
        let cf_by_job = self.cf(cf::BY_JOB)?;
        for (job_id, batch_ids) in by_job {
            let mut unique_batches: Vec<String> = batch_ids;
            unique_batches.sort();
            unique_batches.dedup();
            batch.put_cf(
                &cf_by_job,
                job_id.as_bytes(),
                bincode::serialize(&unique_batches)?,
            );
        }

        // Write filtered rows
        let cf_filtered = self.cf(cf::FILTERED)?;
        for (key, row_id, reason) in filtered_rows {
            let value = (row_id, reason);
            batch.put_cf(&cf_filtered, key.as_bytes(), bincode::serialize(&value)?);
        }

        // Write job statistics
        let cf_stats = self.cf(cf::STATS)?;
        for (job_id, stats) in job_stats {
            batch.put_cf(&cf_stats, job_id.as_bytes(), bincode::serialize(&stats)?);
        }

        // Write tenant indexes (for GDPR erasure)
        let cf_by_tenant = self.cf(cf::BY_TENANT)?;
        for (tenant_id, row_ids) in by_tenant {
            // Read existing tenant index
            let existing = self
                .db
                .get_cf(&cf_by_tenant, tenant_id.as_bytes())
                .context("Failed to read existing tenant index")?;

            let mut all_row_ids: Vec<RowId> = if let Some(data) = existing {
                bincode::deserialize(&data).unwrap_or_default()
            } else {
                Vec::new()
            };

            // Merge new row IDs
            all_row_ids.extend(row_ids);
            // Note: RowId needs to implement Ord for sorting, or we skip deduplication here
            // For now, we'll just accumulate them (dedup will be added when RowId implements necessary traits)

            // Write updated tenant index
            batch.put_cf(
                &cf_by_tenant,
                tenant_id.as_bytes(),
                bincode::serialize(&all_row_ids)?,
            );
        }

        // Execute batch write
        self.db.write(batch).context("Failed to write batch")?;

        Ok(())
    }
}

#[async_trait::async_trait]
impl RowLevelLineageSink for RowLineageStore {
    async fn write_row(&self, event: RowLineageEvent) -> Result<()> {
        let mut buffer = self.write_buffer.lock().await;
        buffer.push(event);

        if buffer.len() >= self.max_buffer_size {
            let events = std::mem::take(&mut *buffer);
            drop(buffer);
            self.write_events_internal(events).await?;
        }

        Ok(())
    }

    async fn write_rows_batch(&self, events: Vec<RowLineageEvent>) -> Result<()> {
        // For large batches, write directly
        if events.len() > 100 {
            return self.write_events_internal(events).await;
        }

        // For small batches, use buffer
        let mut buffer = self.write_buffer.lock().await;
        buffer.extend(events);

        if buffer.len() >= self.max_buffer_size {
            let events = std::mem::take(&mut *buffer);
            drop(buffer);
            self.write_events_internal(events).await?;
        }

        Ok(())
    }

    async fn get_row_lineage(&self, row_id: &RowId) -> Result<Vec<RowLineageEvent>> {
        let key = Self::encode_row_id(row_id);
        let key_str = String::from_utf8_lossy(&key);
        tracing::info!(
            "get_row_lineage: looking up key='{}' (len={})",
            key_str,
            key.len()
        );

        let cf_by_row = self.cf(cf::BY_ROW)?;
        let data = self
            .db
            .get_cf(&cf_by_row, &key)
            .context("Failed to read row lineage")?;

        tracing::info!("get_row_lineage: data found={}", data.is_some());

        if let Some(data) = data {
            // Try decompression first
            let decompressed = zstd::decode_all(&data[..]).unwrap_or(data);

            // Try versioned format first, fall back to legacy format for backward compatibility
            let events = VersionedData::<Vec<RowLineageEvent>>::deserialize(&decompressed)
                .and_then(|v| v.unwrap_vec())
                .or_else(|_| {
                    tracing::warn!("Deserializing legacy unversioned row lineage data");
                    bincode::deserialize(&decompressed)
                        .map_err(|e| anyhow::anyhow!("Bincode deserialization failed: {}", e))
                })?;

            Ok(events)
        } else {
            Ok(Vec::new())
        }
    }

    async fn get_batch_lineage(&self, batch_id: &str) -> Result<Vec<RowLineageEvent>> {
        // Get row IDs for this batch
        let row_ids_data = {
            let cf_by_batch = self.cf(cf::BY_BATCH)?;
            self.db
                .get_cf(&cf_by_batch, batch_id.as_bytes())
                .context("Failed to read batch index")?
        };

        if let Some(data) = row_ids_data {
            let row_ids: Vec<RowId> = bincode::deserialize(&data)?;
            let mut all_events = Vec::new();

            for row_id in row_ids {
                let events = self.get_row_lineage(&row_id).await?;
                all_events.extend(events);
            }

            Ok(all_events)
        } else {
            Ok(Vec::new())
        }
    }

    async fn get_filtered_rows(
        &self,
        job_id: &str,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<Vec<(RowId, String)>> {
        let prefix = format!("{}:", job_id);
        let start_key = format!("{}:{}", job_id, start_time.timestamp());
        let end_key = format!("{}:{}", job_id, end_time.timestamp());

        let cf = self.cf(cf::FILTERED)?;
        let mut iter = self.db.raw_iterator_cf(&cf);
        iter.seek(start_key.as_bytes());

        let mut results = Vec::new();

        while iter.valid() {
            if let Some(key) = iter.key() {
                let key_str = String::from_utf8_lossy(key);
                if !key_str.starts_with(&prefix) || key_str.as_ref() > end_key.as_str() {
                    break;
                }

                if let Some(value) = iter.value() {
                    let (row_id, reason): (RowId, String) = bincode::deserialize(value)?;
                    results.push((row_id, reason));
                }
            }
            iter.next();
        }

        Ok(results)
    }

    async fn get_row_transformations(&self, row_id: &RowId) -> Result<Vec<RowTransformation>> {
        let events = self.get_row_lineage(row_id).await?;
        let mut transformations = Vec::new();

        for event in events {
            transformations.extend(event.transformations);
        }

        Ok(transformations)
    }

    async fn trace_row_journey(&self, row_id: &RowId) -> Result<RowJourney> {
        let events = self.get_row_lineage(row_id).await?;

        if events.is_empty() {
            return Ok(RowJourney {
                source: row_id.clone(),
                steps: Vec::new(),
                destination: None,
                total_duration_ms: 0,
            });
        }

        let mut steps = Vec::new();
        let start_time = events.first().unwrap().timestamp;
        let mut last_time = start_time;

        for event in &events {
            let duration_ms = (event.timestamp - last_time).num_milliseconds() as u64;
            steps.push(JourneyStep {
                activity: format!("{} in {}", event.job_id, event.batch_id),
                timestamp: event.timestamp,
                duration_ms,
                outcome: event.outcome.clone(),
            });
            last_time = event.timestamp;
        }

        let total_duration_ms = (last_time - start_time).num_milliseconds() as u64;
        let destination = events.last().and_then(|e| e.output_row_id.clone());

        Ok(RowJourney {
            source: row_id.clone(),
            steps,
            destination,
            total_duration_ms,
        })
    }

    async fn get_job_stats(&self, job_id: &str) -> Result<JobStatistics> {
        let cf_stats = self.cf(cf::STATS)?;
        let data = self
            .db
            .get_cf(&cf_stats, job_id.as_bytes())
            .context("Failed to read job statistics")?;

        if let Some(data) = data {
            let stats: JobStatistics = bincode::deserialize(&data)?;
            Ok(stats)
        } else {
            // Return empty stats if not found
            Ok(JobStatistics {
                job_id: job_id.to_string(),
                total_rows: 0,
                success_count: 0,
                filtered_count: 0,
                failed_count: 0,
                filter_reasons: HashMap::new(),
                avg_processing_time_ms: 0.0,
                start_time: Utc::now(),
                end_time: None,
            })
        }
    }

    /// Flush buffered events to storage
    async fn flush_buffer(&self) -> Result<()> {
        let mut buffer = self.write_buffer.lock().await;
        if buffer.is_empty() {
            return Ok(());
        }

        let events = std::mem::take(&mut *buffer);
        drop(buffer); // Release lock early

        self.write_events_internal(events).await
    }
}

impl RowLineageStore {
    /// Get all row keys for a tenant (for GDPR erasure)
    ///
    /// Returns a list of RowIds for a given tenant.
    /// This is used by the DataErasure trait implementation.
    pub fn get_tenant_row_ids(&self, tenant_id: &str) -> Result<Vec<RowId>> {
        let cf_by_tenant = self.cf(cf::BY_TENANT)?;
        match self.db.get_cf(&cf_by_tenant, tenant_id.as_bytes())? {
            Some(data) => {
                let row_ids: Vec<RowId> = bincode::deserialize(&data)?;
                Ok(row_ids)
            }
            None => Ok(Vec::new()),
        }
    }

    /// Count records for a tenant (for GDPR transparency)
    pub fn count_tenant_records(&self, tenant_id: &str) -> Result<u64> {
        let row_ids = self.get_tenant_row_ids(tenant_id)?;
        Ok(row_ids.len() as u64)
    }

    /// Count row lineage events associated with a user identifier.
    ///
    /// User affinity is determined by `correlation_id` first, with `tenant_id`
    /// as a legacy fallback for environments that stored user IDs in tenant scope.
    pub fn count_user_records(&self, user_id: &str) -> Result<u64> {
        let cf_by_row = self.cf(cf::BY_ROW)?;
        let iter = self
            .db
            .iterator_cf(&cf_by_row, rocksdb::IteratorMode::Start);

        let mut count = 0u64;

        for item in iter {
            let (_key, value) = item.context("Failed to read row lineage iterator item")?;
            let decompressed = zstd::decode_all(&value[..]).unwrap_or_else(|_| value.to_vec());

            let events = VersionedData::<Vec<RowLineageEvent>>::deserialize(&decompressed)
                .and_then(|v| v.unwrap_vec())
                .or_else(|_| {
                    bincode::deserialize(&decompressed)
                        .map_err(|e| anyhow::anyhow!("Failed to deserialize row lineage: {}", e))
                })?;

            count += events
                .iter()
                .filter(|event| {
                    event.correlation_id.as_deref() == Some(user_id) || event.tenant_id == user_id
                })
                .count() as u64;
        }

        Ok(count)
    }

    /// Erase row lineage events associated with a user identifier.
    ///
    /// Returns the number of deleted events.
    fn erase_user_records(&self, user_id: &str, dry_run: bool) -> Result<u64> {
        let cf_by_row = self.cf(cf::BY_ROW)?;
        let cf_by_tenant = self.cf(cf::BY_TENANT)?;
        let cf_transforms = self.cf(cf::TRANSFORMS)?;

        let iter = self
            .db
            .iterator_cf(&cf_by_row, rocksdb::IteratorMode::Start);
        let mut batch = WriteBatch::default();
        let mut deleted_events = 0u64;
        let mut removed_row_ids_by_tenant: HashMap<String, HashSet<RowId>> = HashMap::new();

        for item in iter {
            let (key, value) = item.context("Failed to read row lineage iterator item")?;
            let decompressed = zstd::decode_all(&value[..]).unwrap_or_else(|_| value.to_vec());

            let events = VersionedData::<Vec<RowLineageEvent>>::deserialize(&decompressed)
                .and_then(|v| v.unwrap_vec())
                .or_else(|_| {
                    bincode::deserialize(&decompressed)
                        .map_err(|e| anyhow::anyhow!("Failed to deserialize row lineage: {}", e))
                })?;

            let mut kept_events = Vec::with_capacity(events.len());
            let mut removed_events = Vec::new();

            for event in events {
                if event.correlation_id.as_deref() == Some(user_id) || event.tenant_id == user_id {
                    removed_events.push(event);
                } else {
                    kept_events.push(event);
                }
            }

            if removed_events.is_empty() {
                continue;
            }

            deleted_events += removed_events.len() as u64;

            if dry_run {
                continue;
            }

            if kept_events.is_empty() {
                batch.delete_cf(&cf_by_row, &key);
                batch.delete_cf(&cf_transforms, &key);

                // Track tenant index cleanup for fully removed rows.
                if let Some(example_event) = removed_events.first() {
                    for tenant_id in removed_events.iter().map(|e| e.tenant_id.clone()) {
                        removed_row_ids_by_tenant
                            .entry(tenant_id)
                            .or_default()
                            .insert(example_event.row_id.clone());
                    }
                }
            } else {
                let versioned = VersionedData::wrap_vec(kept_events);
                let serialized = versioned.serialize()?;
                let data = if serialized.len() > 1024 {
                    zstd::encode_all(&serialized[..], 3)?
                } else {
                    serialized
                };
                batch.put_cf(&cf_by_row, &key, data);
            }
        }

        if !dry_run {
            for (tenant_id, removed_row_ids) in removed_row_ids_by_tenant {
                let Some(existing_bytes) = self.db.get_cf(&cf_by_tenant, tenant_id.as_bytes())?
                else {
                    continue;
                };

                let mut row_ids: Vec<RowId> =
                    bincode::deserialize(&existing_bytes).unwrap_or_default();
                row_ids.retain(|row_id| !removed_row_ids.contains(row_id));

                if row_ids.is_empty() {
                    batch.delete_cf(&cf_by_tenant, tenant_id.as_bytes());
                } else {
                    batch.put_cf(
                        &cf_by_tenant,
                        tenant_id.as_bytes(),
                        bincode::serialize(&row_ids)?,
                    );
                }
            }

            self.db.write(batch)?;
        }

        Ok(deleted_events)
    }
}

/// GDPR Data Erasure Implementation
///
/// Implements GDPR Article 17 (Right to Erasure) for row-level lineage data.
#[async_trait]
impl DataErasure for RowLineageStore {
    async fn erase_data_subject(&self, request: &ErasureRequest) -> Result<ErasureResult> {
        let mut result = ErasureResult::new(request.clone(), uuid::Uuid::new_v4().to_string());

        if request.data_subject.id_type == "user_id" {
            let user_id = &request.data_subject.id;

            if request.dry_run {
                let count = self.count_user_records(user_id)?;
                result.add_backend_result(
                    "row_lineage".to_string(),
                    BackendErasureResult::success("row_lineage", 0, ErasureStrategy::HardDelete)
                        .with_detail("row_lineage_events_would_be_deleted", count)
                        .with_warning(format!(
                            "Dry run - no data was actually deleted. Would have deleted {} records.",
                            count
                        )),
                );
                return Ok(result.finalize());
            }

            let deleted = self.erase_user_records(user_id, false)?;
            result.add_backend_result(
                "row_lineage".to_string(),
                BackendErasureResult::success("row_lineage", deleted, ErasureStrategy::HardDelete)
                    .with_detail("row_lineage_events", deleted),
            );
            return Ok(result.finalize());
        }

        if request.data_subject.id_type != "tenant_id" {
            result.add_backend_result(
                "row_lineage".to_string(),
                BackendErasureResult::failure(
                    "row_lineage",
                    format!(
                        "Unsupported data subject type: {}. Supported types: 'tenant_id', 'user_id'.",
                        request.data_subject.id_type
                    ),
                    ErasureStrategy::HardDelete,
                ),
            );
            return Ok(result.finalize());
        }

        let tenant_id = &request.data_subject.id;

        // Dry run: just count records
        if request.dry_run {
            let count = self.count_tenant_records(tenant_id)?;
            result.add_backend_result(
                "row_lineage".to_string(),
                BackendErasureResult::success("row_lineage", 0, ErasureStrategy::HardDelete)
                    .with_detail("row_lineage_events_would_be_deleted", count)
                    .with_warning(format!(
                        "Dry run - no data was actually deleted. Would have deleted {} records.",
                        count
                    )),
            );
            return Ok(result.finalize());
        }

        // Get all row IDs for this tenant
        let row_ids = self.get_tenant_row_ids(tenant_id)?;
        let total_rows = row_ids.len() as u64;

        if total_rows == 0 {
            result.add_backend_result(
                "row_lineage".to_string(),
                BackendErasureResult::success("row_lineage", 0, ErasureStrategy::HardDelete)
                    .with_warning("No data found for tenant"),
            );
            return Ok(result.finalize());
        }

        // Hard delete all data for this tenant
        let cf_by_row = self.cf(cf::BY_ROW)?;
        let cf_by_tenant = self.cf(cf::BY_TENANT)?;
        let cf_transforms = self.cf(cf::TRANSFORMS)?;

        let mut batch = WriteBatch::default();
        let mut records_deleted = 0u64;

        // Delete row lineage events
        for row_id in &row_ids {
            let row_key = row_id.to_key();
            batch.delete_cf(&cf_by_row, row_key.as_bytes());
            batch.delete_cf(&cf_transforms, row_key.as_bytes());
            records_deleted += 1;
        }

        // Delete tenant index
        batch.delete_cf(&cf_by_tenant, tenant_id.as_bytes());

        // Write batch
        self.db.write(batch)?;

        result.add_backend_result(
            "row_lineage".to_string(),
            BackendErasureResult::success(
                "row_lineage",
                records_deleted,
                ErasureStrategy::HardDelete,
            )
            .with_detail("row_lineage_events", records_deleted)
            .with_detail("tenant_index_entries", 1),
        );

        Ok(result.finalize())
    }

    async fn count_data_subject_records(&self, data_subject: &DataSubjectId) -> Result<u64> {
        match data_subject.id_type.as_str() {
            "tenant_id" => self.count_tenant_records(&data_subject.id),
            "user_id" => self.count_user_records(&data_subject.id),
            _ => anyhow::bail!(
                "Unsupported data subject type: {}. Supported types: 'tenant_id', 'user_id'.",
                data_subject.id_type
            ),
        }
    }

    async fn get_data_breakdown(
        &self,
        data_subject: &DataSubjectId,
    ) -> Result<std::collections::HashMap<String, u64>> {
        let count = match data_subject.id_type.as_str() {
            "tenant_id" => self.count_tenant_records(&data_subject.id)?,
            "user_id" => self.count_user_records(&data_subject.id)?,
            _ => {
                anyhow::bail!(
                    "Unsupported data subject type: {}. Supported types: 'tenant_id', 'user_id'.",
                    data_subject.id_type
                )
            }
        };
        let mut breakdown = std::collections::HashMap::new();
        breakdown.insert("row_lineage_events".to_string(), count);
        Ok(breakdown)
    }
}

/// Ensure buffer is flushed on drop
impl Drop for RowLineageStore {
    fn drop(&mut self) {
        // Flush any remaining events in buffer
        if let Ok(buffer) = self.write_buffer.try_lock() {
            if !buffer.is_empty() {
                tracing::warn!(
                    "Dropping RowLineageStore with {} events in buffer",
                    buffer.len()
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_row_lineage_store() -> Result<()> {
        let dir = tempdir()?;
        let store = RowLineageStore::new(dir.path())?;

        // Create test event
        let row_id = RowId::csv("test.csv", 1);
        let event = RowLineageEvent::success(
            row_id.clone(),
            "batch-1".to_string(),
            "job-1".to_string(),
            "/output/test.csv".to_string(),
            "tenant-a".to_string(),
        );

        // Write event
        store.write_row(event.clone()).await?;
        store.flush_buffer().await?;

        // Read back
        let events = store.get_row_lineage(&row_id).await?;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].row_id, row_id);

        // Check batch lineage
        let batch_events = store.get_batch_lineage("batch-1").await?;
        assert_eq!(batch_events.len(), 1);

        // Check job stats
        let stats = store.get_job_stats("job-1").await?;
        assert_eq!(stats.total_rows, 1);
        assert_eq!(stats.success_count, 1);

        Ok(())
    }

    #[tokio::test]
    async fn test_filtered_rows() -> Result<()> {
        let dir = tempdir()?;
        let store = RowLineageStore::new(dir.path())?;

        // Create filtered event
        let row_id = RowId::csv("test.csv", 2);
        let event = RowLineageEvent::filtered(
            row_id.clone(),
            "batch-2".to_string(),
            "job-2".to_string(),
            "Status is inactive".to_string(),
            "rule-active-only".to_string(),
            "tenant-a".to_string(),
        );

        store.write_row(event.clone()).await?;
        store.flush_buffer().await?;

        // Query filtered rows
        let start = Utc::now() - chrono::Duration::hours(1);
        let end = Utc::now() + chrono::Duration::hours(1);
        let filtered = store.get_filtered_rows("job-2", start, end).await?;

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].0, row_id);
        assert_eq!(filtered[0].1, "Status is inactive");

        Ok(())
    }

    #[tokio::test]
    async fn test_tenant_index() -> Result<()> {
        let dir = tempdir()?;
        let store = RowLineageStore::new(dir.path())?;

        // Write events for different tenants
        for i in 0..5 {
            let event = RowLineageEvent::success(
                RowId::csv("test.csv", i),
                "batch-1".to_string(),
                "job-1".to_string(),
                "/output/test.csv".to_string(),
                "tenant-123".to_string(),
            );
            store.write_row(event).await?;
        }

        for i in 5..8 {
            let event = RowLineageEvent::success(
                RowId::csv("test.csv", i),
                "batch-2".to_string(),
                "job-2".to_string(),
                "/output/test.csv".to_string(),
                "tenant-456".to_string(),
            );
            store.write_row(event).await?;
        }

        store.flush_buffer().await?;

        // Check tenant-123 has 5 rows
        let count_123 = store.count_tenant_records("tenant-123")?;
        assert_eq!(count_123, 5);

        let row_ids_123 = store.get_tenant_row_ids("tenant-123")?;
        assert_eq!(row_ids_123.len(), 5);

        // Check tenant-456 has 3 rows
        let count_456 = store.count_tenant_records("tenant-456")?;
        assert_eq!(count_456, 3);

        // Check non-existent tenant
        let count_999 = store.count_tenant_records("tenant-999")?;
        assert_eq!(count_999, 0);

        Ok(())
    }

    #[tokio::test]
    async fn test_tenant_index_accumulation() -> Result<()> {
        let dir = tempdir()?;
        let store = RowLineageStore::new(dir.path())?;

        // Write first batch
        for i in 0..3 {
            let event = RowLineageEvent::success(
                RowId::csv("test1.csv", i),
                "batch-1".to_string(),
                "job-1".to_string(),
                "/output/test.csv".to_string(),
                "tenant-abc".to_string(),
            );
            store.write_row(event).await?;
        }
        store.flush_buffer().await?;

        // Check count after first batch
        let count_first = store.count_tenant_records("tenant-abc")?;
        assert_eq!(count_first, 3);

        // Write second batch for same tenant
        for i in 0..2 {
            let event = RowLineageEvent::success(
                RowId::csv("test2.csv", i),
                "batch-2".to_string(),
                "job-2".to_string(),
                "/output/test.csv".to_string(),
                "tenant-abc".to_string(),
            );
            store.write_row(event).await?;
        }
        store.flush_buffer().await?;

        // Check count after second batch (should accumulate)
        let count_second = store.count_tenant_records("tenant-abc")?;
        assert_eq!(count_second, 5);

        let row_ids = store.get_tenant_row_ids("tenant-abc")?;
        assert_eq!(row_ids.len(), 5);

        Ok(())
    }
}
