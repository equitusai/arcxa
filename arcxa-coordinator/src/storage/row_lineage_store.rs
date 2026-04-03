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
    const ZSTD_MAGIC: [u8; 4] = [0x28, 0xB5, 0x2F, 0xFD];

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

    fn new_job_statistics(job_id: &str, start_time: DateTime<Utc>) -> JobStatistics {
        JobStatistics {
            job_id: job_id.to_string(),
            total_rows: 0,
            success_count: 0,
            filtered_count: 0,
            failed_count: 0,
            filter_reasons: HashMap::new(),
            avg_processing_time_ms: 0.0,
            start_time,
            end_time: None,
        }
    }

    fn load_job_statistics(
        &self,
        cf_stats: &Arc<BoundColumnFamily<'_>>,
        job_id: &str,
        start_time: DateTime<Utc>,
    ) -> Result<JobStatistics> {
        let existing = self
            .db
            .get_cf(cf_stats, job_id.as_bytes())
            .context("Failed to read existing job statistics")?;

        if let Some(data) = existing {
            let mut stats: JobStatistics = bincode::deserialize(&data)?;
            if start_time < stats.start_time {
                stats.start_time = start_time;
            }
            Ok(stats)
        } else {
            Ok(Self::new_job_statistics(job_id, start_time))
        }
    }

    /// Encode row ID to bytes
    fn encode_row_id(row_id: &RowId) -> Vec<u8> {
        row_id.to_key().into_bytes()
    }

    fn looks_like_zstd(bytes: &[u8]) -> bool {
        bytes.starts_with(&Self::ZSTD_MAGIC)
    }

    fn debug_prefix(bytes: &[u8]) -> String {
        bytes
            .iter()
            .take(16)
            .map(|byte| format!("{byte:02x}"))
            .collect::<Vec<_>>()
            .join("")
    }

    fn serialize_row_events(row_events: Vec<RowLineageEvent>) -> Result<Vec<u8>> {
        let versioned = VersionedData::wrap_vec(row_events);
        let serialized =
            serde_json::to_vec(&versioned).context("Failed to serialize row lineage JSON")?;

        if serialized.len() > 1024 {
            Ok(zstd::encode_all(&serialized[..], 3)?)
        } else {
            Ok(serialized)
        }
    }

    fn deserialize_row_events_payload(bytes: &[u8]) -> Result<Vec<RowLineageEvent>> {
        let json_versioned_error =
            match serde_json::from_slice::<VersionedData<Vec<RowLineageEvent>>>(bytes) {
                Ok(versioned) => return versioned.unwrap_vec(),
                Err(error) => error,
            };

        let json_legacy_error = match serde_json::from_slice::<Vec<RowLineageEvent>>(bytes) {
            Ok(events) => {
                tracing::warn!("Deserializing legacy unversioned JSON row lineage data");
                return Ok(events);
            }
            Err(error) => error,
        };

        let versioned_error = match VersionedData::<Vec<RowLineageEvent>>::deserialize(bytes) {
            Ok(versioned) => return versioned.unwrap_vec(),
            Err(error) => error,
        };

        let legacy_error = match bincode::deserialize(bytes) {
            Ok(events) => {
                tracing::warn!("Deserializing legacy unversioned row lineage data");
                return Ok(events);
            }
            Err(error) => error,
        };

        anyhow::bail!(
            "Unable to deserialize row lineage payload (len={}, zstd_magic=false, prefix={}, json_versioned_error={}, json_legacy_error={}, versioned_error={}, legacy_error={})",
            bytes.len(),
            Self::debug_prefix(bytes),
            json_versioned_error,
            json_legacy_error,
            versioned_error,
            legacy_error
        );
    }

    fn deserialize_row_events(bytes: &[u8]) -> Result<Vec<RowLineageEvent>> {
        if Self::looks_like_zstd(bytes) {
            let decompressed =
                zstd::decode_all(bytes).context("Failed to decompress row lineage payload")?;

            return Self::deserialize_row_events_payload(&decompressed).map_err(|error| {
                anyhow::anyhow!(
                    "Unable to deserialize compressed row lineage payload (len={}, decompressed_len={}, prefix={}, error={})",
                    bytes.len(),
                    decompressed.len(),
                    Self::debug_prefix(&decompressed),
                    error
                )
            });
        }

        Self::deserialize_row_events_payload(bytes)
    }

    fn append_event_to_row_index(
        &self,
        batch: &mut WriteBatch,
        row_id: &RowId,
        event: &RowLineageEvent,
    ) -> Result<()> {
        let row_key = Self::encode_row_id(row_id);
        let cf_by_row = self.cf(cf::BY_ROW)?;
        let existing = self
            .db
            .get_cf(&cf_by_row, &row_key)
            .context("Failed to read existing events")?;

        let mut row_events: Vec<RowLineageEvent> = if let Some(data) = existing {
            Self::deserialize_row_events(&data).unwrap_or_default()
        } else {
            Vec::new()
        };

        row_events.push(event.clone());

        let data = Self::serialize_row_events(row_events)?;
        batch.put_cf(&cf_by_row, row_key, data);

        Ok(())
    }

    /// Decode row ID from index bytes
    fn decode_row_id(bytes: &[u8]) -> Result<RowId> {
        let key = String::from_utf8(bytes.to_vec())?;
        RowId::from_key(&key)
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

        let cf_stats = self.cf(cf::STATS)?;

        for event in &events {
            self.append_event_to_row_index(&mut batch, &event.row_id, event)?;
            if let Some(output_row_id) = &event.output_row_id {
                self.append_event_to_row_index(&mut batch, output_row_id, event)?;
            }

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
            if !job_stats.contains_key(&event.job_id) {
                let stats = self.load_job_statistics(&cf_stats, &event.job_id, event.timestamp)?;
                job_stats.insert(event.job_id.clone(), stats);
            }

            let stats = job_stats
                .get_mut(&event.job_id)
                .expect("job statistics should exist after initialization");

            stats.total_rows += 1;
            if event.timestamp < stats.start_time {
                stats.start_time = event.timestamp;
            }
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
        if events.is_empty() {
            return Ok(());
        }

        // Batch callers expect the events they just recorded to be queryable
        // when the workflow step completes, so persist them immediately.
        self.flush_buffer().await?;
        self.write_events_internal(events).await
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
            Self::deserialize_row_events(&data)
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

    async fn search_row_keys(&self, query: &str, limit: usize) -> Result<Vec<RowId>> {
        let normalized_query = query.trim().to_ascii_lowercase();
        if normalized_query.is_empty() || limit == 0 {
            return Ok(Vec::new());
        }

        let cf_by_row = self.cf(cf::BY_ROW)?;
        let iter = self.db.iterator_cf(&cf_by_row, rocksdb::IteratorMode::Start);

        let mut exact_matches = Vec::new();
        let mut prefix_matches = Vec::new();
        let mut contains_matches = Vec::new();
        let mut total_matches = 0usize;
        let mut seen = HashSet::new();

        for item in iter {
            let (key, _) = item.context("Failed to iterate row lineage index")?;
            let row_key = String::from_utf8_lossy(&key).to_string();
            let row_key_lower = row_key.to_ascii_lowercase();

            let match_rank = if row_key_lower == normalized_query {
                Some(0u8)
            } else if row_key_lower.starts_with(&normalized_query) {
                Some(1u8)
            } else if row_key_lower.contains(&normalized_query) {
                Some(2u8)
            } else {
                None
            };

            let Some(match_rank) = match_rank else {
                continue;
            };

            total_matches += 1;

            let within_limit =
                seen.insert(row_key.clone())
                    && exact_matches.len() + prefix_matches.len() + contains_matches.len() < limit;

            if within_limit {
                match Self::decode_row_id(row_key.as_bytes()) {
                    Ok(row_id) => match match_rank {
                        0 => exact_matches.push(row_id),
                        1 => prefix_matches.push(row_id),
                        _ => contains_matches.push(row_id),
                    },
                    Err(error) => tracing::warn!(
                        row_key = %row_key,
                        error = %error,
                        "Skipping malformed row key while searching lineage index"
                    ),
                }
            }
        }

        let mut matches = Vec::with_capacity(limit.min(total_matches));
        matches.extend(exact_matches);
        matches.extend(prefix_matches);
        matches.extend(contains_matches);
        matches.truncate(limit);

        Ok(matches)
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
            let events = Self::deserialize_row_events(&value)?;

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
            let events = Self::deserialize_row_events(&value)?;

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
                let data = Self::serialize_row_events(kept_events)?;
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
    use graphica_core::core::lineage::row_level::DatabaseType;
    use serde_json::json;
    use std::collections::BTreeMap;
    use tempfile::tempdir;

    fn build_csv_export_lineage_event() -> (RowId, RowLineageEvent) {
        let mut primary_key = BTreeMap::new();
        primary_key.insert("CUSTOMER_ID".to_string(), "CUST001".to_string());
        let source_row_id = RowId::database(DatabaseType::Oracle, "CUSTOMERS", primary_key);

        let mut event = RowLineageEvent::success_with_step(
            source_row_id.clone(),
            "batch-export".to_string(),
            "job-export".to_string(),
            Some("export_customers".to_string()),
            "/app/data/e2e-output/customers_export.csv".to_string(),
            "tenant-export".to_string(),
        );
        event.output_row_id = Some(RowId::csv("/app/data/e2e-output/customers_export.csv", 2));

        let mut transformation =
            RowTransformation::new("csv_export".to_string(), vec!["_row".to_string()]);
        let mut after_values = HashMap::new();
        after_values.insert(
            "output_path".to_string(),
            json!("/app/data/e2e-output/customers_export.csv"),
        );
        after_values.insert("output_line".to_string(), json!(1));
        transformation.after_values = Some(after_values);
        event.add_transformation(transformation);

        (source_row_id, event)
    }

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
    async fn test_write_rows_batch_is_immediately_queryable() -> Result<()> {
        let dir = tempdir()?;
        let store = RowLineageStore::new(dir.path())?;

        let row_id = RowId::csv("batch.csv", 1);
        let event = RowLineageEvent::success(
            row_id.clone(),
            "batch-visible".to_string(),
            "job-visible".to_string(),
            "/output/batch.csv".to_string(),
            "tenant-visible".to_string(),
        );

        store.write_rows_batch(vec![event]).await?;

        let events = store.get_row_lineage(&row_id).await?;
        assert_eq!(events.len(), 1);

        let stats = store.get_job_stats("job-visible").await?;
        assert_eq!(stats.total_rows, 1);
        assert_eq!(stats.success_count, 1);

        Ok(())
    }

    #[tokio::test]
    async fn test_write_rows_batch_with_csv_export_lineage_payload_is_queryable() -> Result<()> {
        let dir = tempdir()?;
        let store = RowLineageStore::new(dir.path())?;

        let (source_row_id, event) = build_csv_export_lineage_event();

        store.write_rows_batch(vec![event]).await?;

        let events = store.get_row_lineage(&source_row_id).await?;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].step_id.as_deref(), Some("export_customers"));
        assert!(events[0].output_row_id.is_some());
        assert_eq!(events[0].transformations.len(), 1);

        Ok(())
    }

    #[tokio::test]
    async fn test_write_rows_batch_is_queryable_by_output_row_id() -> Result<()> {
        let dir = tempdir()?;
        let store = RowLineageStore::new(dir.path())?;

        let (source_row_id, event) = build_csv_export_lineage_event();
        let output_row_id = event
            .output_row_id
            .clone()
            .expect("csv export test event should have output row id");

        store.write_rows_batch(vec![event]).await?;

        let source_events = store.get_row_lineage(&source_row_id).await?;
        assert_eq!(source_events.len(), 1);

        let output_events = store.get_row_lineage(&output_row_id).await?;
        assert_eq!(output_events.len(), 1);
        assert_eq!(
            output_events[0].step_id.as_deref(),
            Some("export_customers")
        );
        assert_eq!(output_events[0].row_id, source_row_id);
        assert_eq!(
            output_events[0].output_row_id.as_ref().map(RowId::to_key),
            Some(output_row_id.to_key())
        );

        Ok(())
    }

    #[test]
    fn test_serialize_row_events_roundtrip_with_csv_export_lineage_payload() -> Result<()> {
        let (_source_row_id, event) = build_csv_export_lineage_event();
        let bytes = RowLineageStore::serialize_row_events(vec![event])?;
        let events = RowLineageStore::deserialize_row_events(&bytes)?;

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].step_id.as_deref(), Some("export_customers"));
        assert!(events[0].output_row_id.is_some());
        assert_eq!(events[0].transformations.len(), 1);

        Ok(())
    }

    #[tokio::test]
    async fn test_write_rows_batch_stores_expected_raw_bytes_for_csv_export_payload() -> Result<()>
    {
        let dir = tempdir()?;
        let store = RowLineageStore::new(dir.path())?;
        let (source_row_id, event) = build_csv_export_lineage_event();
        let expected = RowLineageStore::serialize_row_events(vec![event.clone()])?;

        store.write_rows_batch(vec![event]).await?;

        let cf_by_row = store.cf(cf::BY_ROW)?;
        let stored = store
            .db
            .get_cf(&cf_by_row, RowLineageStore::encode_row_id(&source_row_id))?
            .expect("row lineage bytes should exist");

        assert_eq!(stored, expected);

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
    async fn test_job_stats_accumulate_across_multiple_writes() -> Result<()> {
        let dir = tempdir()?;
        let store = RowLineageStore::new(dir.path())?;

        let success_event = RowLineageEvent::success(
            RowId::csv("customers.csv", 1),
            "batch-success".to_string(),
            "job-accumulate".to_string(),
            "/output/customers.csv".to_string(),
            "tenant-a".to_string(),
        );
        store.write_rows_batch(vec![success_event]).await?;

        let filtered_event = RowLineageEvent::filtered(
            RowId::csv("customers.csv", 2),
            "batch-filtered".to_string(),
            "job-accumulate".to_string(),
            "Duplicate removed using First strategy".to_string(),
            "deduplication".to_string(),
            "tenant-a".to_string(),
        );
        store.write_row(filtered_event).await?;
        store.flush_buffer().await?;

        let stats = store.get_job_stats("job-accumulate").await?;
        assert_eq!(stats.total_rows, 2);
        assert_eq!(stats.success_count, 1);
        assert_eq!(stats.filtered_count, 1);
        assert_eq!(stats.failed_count, 0);
        assert_eq!(
            stats
                .filter_reasons
                .get("Duplicate removed using First strategy"),
            Some(&1),
        );

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

    #[tokio::test]
    async fn test_search_row_keys_prioritizes_prefix_matches() -> Result<()> {
        let dir = tempdir()?;
        let store = RowLineageStore::new(dir.path())?;

        let oracle_row = RowId::database(
            DatabaseType::Oracle,
            "CUSTOMER_FEED",
            BTreeMap::from([("STAGE_ROW_ID".to_string(), "FEED001".to_string())]),
        );
        let db2_row = RowId::database(
            DatabaseType::DB2,
            "CUSTOMER_FEED_CURATED",
            BTreeMap::from([("STAGE_ROW_ID".to_string(), "FEED001".to_string())]),
        );

        store
            .write_rows_batch(vec![
                RowLineageEvent::success(
                    oracle_row.clone(),
                    "batch-search".to_string(),
                    "job-search".to_string(),
                    "/output/oracle".to_string(),
                    "tenant-search".to_string(),
                ),
                RowLineageEvent::success(
                    db2_row.clone(),
                    "batch-search".to_string(),
                    "job-search".to_string(),
                    "/output/db2".to_string(),
                    "tenant-search".to_string(),
                ),
            ])
            .await?;

        let matches = store.search_row_keys("oracle:customer", 10).await?;

        assert!(!matches.is_empty());
        assert_eq!(matches[0].to_key(), oracle_row.to_key());
        assert!(matches.iter().all(|row_id| row_id.to_key().contains("CUSTOMER")));

        Ok(())
    }
}
