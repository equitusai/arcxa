//! # RocksDB Storage Implementation
//!
//! Hot tier storage for recent lineage data.

use crate::storage::metrics;
use crate::storage::rocks_config::{self, RocksProfile};
use crate::storage::writer_pool::{WriterPool, WriterPoolConfig};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use graphica_core::core::lineage::{LineageEvent, LineageSink};
use rocksdb::{IteratorMode, Options, WriteBatch, DB};
use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Instant;

#[cfg(test)]
use std::collections::HashMap;

pub struct RocksLineageStore {
    db: DB,
    /// Optional multi-threaded writer pool for high throughput
    writer_pool: Option<Arc<WriterPool>>,
}

// Column families for proper indexing
const CF_PRIMARY: &str = "primary"; // event_id -> LineageEvent
                                    // INVERTED INDEXES (append-only, no read-modify-write):
const CF_RECORD_INDEX: &str = "record_idx"; // (record_id, event_id) -> empty
const CF_MODEL_INDEX: &str = "model_idx"; // (model_id:version, event_id) -> empty
const CF_RUN_INDEX: &str = "run_idx"; // (run_id, event_id) -> empty
const CF_TENANT_INDEX: &str = "tenant_idx"; // (tenant_id, event_id) -> empty
                                            // Already append-only:
const CF_TIME_INDEX: &str = "time_idx"; // timestamp -> event_id
const CF_TIME_TRAVEL_INDEX: &str = "time_travel_idx"; // (record_id, timestamp) -> event_id

impl RocksLineageStore {
    /// Create new RocksDB store with default (production) configuration
    pub fn new(path: &str) -> Result<Self> {
        Self::with_profile(path, RocksProfile::Production)
    }

    /// Create new RocksDB store with specified performance profile
    ///
    /// # Arguments
    /// * `path` - Database directory path
    /// * `profile` - Performance profile (Development, Production, HighThroughput)
    ///
    /// # Example
    /// ```ignore
    /// use graphica::storage::{RocksLineageStore, rocks_config::RocksProfile};
    ///
    /// // For maximum throughput (10K+ events/sec)
    /// let store = RocksLineageStore::with_profile(
    ///     "/data/lineage",
    ///     RocksProfile::HighThroughput
    /// )?;
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn with_profile(path: &str, profile: RocksProfile) -> Result<Self> {
        // Print configuration summary
        rocks_config::print_config_summary(profile);

        // Get optimized options for profile
        let opts = rocks_config::create_options(profile);

        let cfs = vec![
            CF_PRIMARY,
            CF_RECORD_INDEX,
            CF_MODEL_INDEX,
            CF_TIME_INDEX,
            CF_RUN_INDEX,
            CF_TENANT_INDEX,
            CF_TIME_TRAVEL_INDEX,
        ];

        let db = DB::open_cf(&opts, path, cfs).context("Failed to open RocksDB")?;

        Ok(Self {
            db,
            writer_pool: None,
        })
    }

    /// Enable multi-threaded writer pool for high throughput
    /// Must wrap self in Arc before calling this
    pub fn with_writer_pool(self: Arc<Self>, config: WriterPoolConfig) -> Result<Arc<Self>> {
        // Create writer pool with reference to this store
        let pool = WriterPool::new(Arc::clone(&self), config)?;

        // This is a bit tricky - we need to update the writer_pool field
        // but Self is in an Arc. We'll use unsafe to do this safely.
        //
        // Safety: We have exclusive access here since this is called during initialization
        unsafe {
            let self_mut = Arc::as_ptr(&self) as *mut Self;
            (*self_mut).writer_pool = Some(Arc::new(pool));
        }

        Ok(self)
    }

    /// Check if writer pool is enabled
    pub fn is_writer_pool_enabled(&self) -> bool {
        self.writer_pool.is_some()
    }

    /// Get writer pool stats if enabled
    pub fn writer_pool_stats(&self) -> Option<&crate::storage::writer_pool::WriterPoolStats> {
        self.writer_pool.as_ref().map(|pool| pool.stats())
    }

    /// Convert timestamp to sortable binary key
    fn timestamp_to_key(ts: DateTime<Utc>) -> [u8; 8] {
        // Use big-endian for lexicographic ordering
        ts.timestamp().to_be_bytes()
    }

    /// Create time-travel index key: (record_id, timestamp)
    fn time_travel_key(record_id: &str, ts: DateTime<Utc>) -> Vec<u8> {
        let mut key = Vec::with_capacity(record_id.len() + 9);
        key.extend_from_slice(record_id.as_bytes());
        key.push(b'|'); // separator
        key.extend_from_slice(&Self::timestamp_to_key(ts));
        key
    }

    /// Get column family handle
    fn cf(&self, name: &str) -> Result<&rocksdb::ColumnFamily> {
        self.db
            .cf_handle(name)
            .ok_or_else(|| anyhow::anyhow!("Column family {} not found", name))
    }

    /// Get events by ID set (helper for time range queries)
    fn get_events_by_ids(&self, event_ids: &BTreeSet<String>) -> Result<Vec<LineageEvent>> {
        let cf_primary = self.cf(CF_PRIMARY)?;
        let mut events = Vec::new();

        for event_id in event_ids {
            if let Some(data) = self.db.get_cf(cf_primary, event_id.as_bytes())? {
                let event: LineageEvent = serde_json::from_slice(&data)?;
                events.push(event);
            }
        }

        // Sort by timestamp
        events.sort_by_key(|e| e.ts);

        Ok(events)
    }

    /// Add event to inverted index (append-only, no read required)
    /// Key format: (prefix, event_id) -> empty
    fn add_to_inverted_index(
        &self,
        batch: &mut WriteBatch,
        cf: &str,
        prefix: &[u8],
        event_id: &str,
    ) -> Result<()> {
        let cf_handle = self.cf(cf)?;

        // Create composite key: (prefix, event_id)
        let mut key = Vec::with_capacity(prefix.len() + event_id.len() + 1);
        key.extend_from_slice(prefix);
        key.push(b'|'); // separator
        key.extend_from_slice(event_id.as_bytes());

        // Write empty value (inverted index pattern)
        batch.put_cf(cf_handle, &key, &[]);

        // Metrics: Only PUT, no GET (append-only)
        metrics::record_rocksdb_op("put", cf);

        Ok(())
    }

    /// Get events by prefix using inverted index
    /// Scans all keys matching (prefix, *) pattern
    fn get_events_by_prefix(&self, cf: &str, prefix: &[u8]) -> Result<Vec<LineageEvent>> {
        let cf_handle = self.cf(cf)?;
        let cf_primary = self.cf(CF_PRIMARY)?;

        let mut events = Vec::new();

        // Create the prefix key with separator
        let mut prefix_key = Vec::with_capacity(prefix.len() + 1);
        prefix_key.extend_from_slice(prefix);
        prefix_key.push(b'|');

        // Use iterator starting from the prefix key
        let iter = self.db.iterator_cf(
            cf_handle,
            IteratorMode::From(&prefix_key, rocksdb::Direction::Forward),
        );

        for item in iter {
            let (key, _) = item?;

            // Check if still in our prefix
            if !key.starts_with(&prefix_key) {
                break;
            }

            // Extract event_id from composite key (after prefix|)
            if let Some(event_id_bytes) = key.get(prefix_key.len()..) {
                let event_id = String::from_utf8_lossy(event_id_bytes).to_string();

                // Fetch the actual event
                if let Some(data) = self.db.get_cf(cf_primary, event_id.as_bytes())? {
                    let event: LineageEvent = serde_json::from_slice(&data)?;
                    events.push(event);
                }
            }
        }

        // Sort by timestamp
        events.sort_by_key(|e| e.ts);

        Ok(events)
    }

    /// Batch write events
    pub fn write_batch(&self, events: Vec<LineageEvent>) -> Result<()> {
        let start = Instant::now();
        let num_events = events.len();

        let mut batch = WriteBatch::default();
        let cf_primary = self.cf(CF_PRIMARY)?;

        for event in events {
            let event_id = event.id.to_string();
            let serialized = serde_json::to_vec(&event)?;

            // Primary storage
            batch.put_cf(cf_primary, event_id.as_bytes(), &serialized);
            metrics::record_rocksdb_op("put", CF_PRIMARY);

            // Time index: timestamp -> event_id
            let time_key = Self::timestamp_to_key(event.ts);
            batch.put_cf(self.cf(CF_TIME_INDEX)?, &time_key, event_id.as_bytes());
            metrics::record_rocksdb_op("put", CF_TIME_INDEX);

            // Time-travel index: (record_id, timestamp) -> event_id
            let time_travel_key = Self::time_travel_key(&event.record_id, event.ts);
            batch.put_cf(
                self.cf(CF_TIME_TRAVEL_INDEX)?,
                &time_travel_key,
                event_id.as_bytes(),
            );
            metrics::record_rocksdb_op("put", CF_TIME_TRAVEL_INDEX);

            // INVERTED INDEXES - Append-only, no read-modify-write
            // Record index: (record_id, event_id) -> empty
            self.add_to_inverted_index(
                &mut batch,
                CF_RECORD_INDEX,
                event.record_id.as_bytes(),
                &event_id,
            )?;

            // Run index: (run_id, event_id) -> empty
            self.add_to_inverted_index(
                &mut batch,
                CF_RUN_INDEX,
                event.run_id.as_bytes(),
                &event_id,
            )?;

            // Tenant index: (tenant_id, event_id) -> empty
            self.add_to_inverted_index(
                &mut batch,
                CF_TENANT_INDEX,
                event.tenant_id.as_bytes(),
                &event_id,
            )?;

            // Model indexes: (model_id:version, event_id) -> empty
            for model_ref in &event.model_refs {
                let model_key = format!("{}:{}", model_ref.model_id, model_ref.version);
                self.add_to_inverted_index(
                    &mut batch,
                    CF_MODEL_INDEX,
                    model_key.as_bytes(),
                    &event_id,
                )?;
            }
        }

        // Write batch and record metrics
        let write_result = self.db.write(batch);
        let latency_us = start.elapsed().as_micros() as u64;

        // Record batch metrics
        metrics::BATCH_WRITE_SIZE_EVENTS
            .with_label_values(&["rocksdb"])
            .observe(num_events as f64);

        match write_result {
            Ok(_) => {
                // Record successful write for each event
                for _ in 0..num_events {
                    metrics::record_write(
                        "rocksdb",
                        "write_event",
                        latency_us / num_events as u64,
                        true,
                    );
                }
                Ok(())
            }
            Err(e) => {
                // Record errors
                for _ in 0..num_events {
                    metrics::record_write(
                        "rocksdb",
                        "write_event",
                        latency_us / num_events as u64,
                        false,
                    );
                }
                metrics::record_storage_error("rocksdb", "write_failure");
                Err(e.into())
            }
        }
    }

    /// Delete events before a cutoff timestamp (used for tiering)
    pub fn delete_before(&self, cutoff: DateTime<Utc>) -> Result<u64> {
        let cf_time = self.cf(CF_TIME_INDEX)?;
        let cf_primary = self.cf(CF_PRIMARY)?;

        let cutoff_key = Self::timestamp_to_key(cutoff);
        let mut event_ids_to_delete = BTreeSet::new();

        // Find all events before cutoff
        for item in self.db.iterator_cf(cf_time, IteratorMode::Start) {
            let (key, value) = item?;

            // Stop if we've reached cutoff
            if key.as_ref() >= cutoff_key.as_ref() {
                break;
            }

            let event_id = String::from_utf8_lossy(value.as_ref()).to_string();
            event_ids_to_delete.insert(event_id);
        }

        let count = event_ids_to_delete.len();

        // Delete from all column families
        let mut batch = WriteBatch::default();

        for event_id in &event_ids_to_delete {
            // Get the event first to clean up all indexes
            if let Some(data) = self.db.get_cf(cf_primary, event_id.as_bytes())? {
                let event: LineageEvent = serde_json::from_slice(&data)?;

                // Delete from all indexes
                self.delete_event_from_indexes(&mut batch, &event)?;
            }

            // Delete primary record
            batch.delete_cf(cf_primary, event_id.as_bytes());
        }

        self.db.write(batch)?;

        tracing::info!("Deleted {} events older than {}", count, cutoff);

        Ok(count as u64)
    }

    /// Helper to delete event from all indexes (inverted index pattern)
    fn delete_event_from_indexes(
        &self,
        batch: &mut WriteBatch,
        event: &LineageEvent,
    ) -> Result<()> {
        let cf_record_idx = self.cf(CF_RECORD_INDEX)?;
        let cf_model_idx = self.cf(CF_MODEL_INDEX)?;
        let cf_time_idx = self.cf(CF_TIME_INDEX)?;
        let cf_run_idx = self.cf(CF_RUN_INDEX)?;
        let cf_tenant_idx = self.cf(CF_TENANT_INDEX)?;
        let cf_time_travel = self.cf(CF_TIME_TRAVEL_INDEX)?;

        let event_id_str = event.id.to_string();

        // Delete from time index
        let time_key = Self::timestamp_to_key(event.ts);
        batch.delete_cf(cf_time_idx, time_key);

        // Delete from time-travel index
        let time_travel_key = Self::time_travel_key(&event.record_id, event.ts);
        batch.delete_cf(cf_time_travel, &time_travel_key);

        // INVERTED INDEX DELETION - Just delete the composite key directly
        // No read-modify-write needed!

        // Delete from record index: (record_id, event_id)
        let mut record_key = Vec::new();
        record_key.extend_from_slice(event.record_id.as_bytes());
        record_key.push(b'|');
        record_key.extend_from_slice(event_id_str.as_bytes());
        batch.delete_cf(cf_record_idx, &record_key);

        // Delete from run index: (run_id, event_id)
        let mut run_key = Vec::new();
        run_key.extend_from_slice(event.run_id.as_bytes());
        run_key.push(b'|');
        run_key.extend_from_slice(event_id_str.as_bytes());
        batch.delete_cf(cf_run_idx, &run_key);

        // Delete from tenant index: (tenant_id, event_id)
        let mut tenant_key = Vec::new();
        tenant_key.extend_from_slice(event.tenant_id.as_bytes());
        tenant_key.push(b'|');
        tenant_key.extend_from_slice(event_id_str.as_bytes());
        batch.delete_cf(cf_tenant_idx, &tenant_key);

        // Delete from model indexes: (model_id:version, event_id)
        for model_ref in &event.model_refs {
            let model_prefix = format!("{}:{}", model_ref.model_id, model_ref.version);
            let mut model_key = Vec::new();
            model_key.extend_from_slice(model_prefix.as_bytes());
            model_key.push(b'|');
            model_key.extend_from_slice(event_id_str.as_bytes());
            batch.delete_cf(cf_model_idx, &model_key);
        }

        Ok(())
    }
}

impl LineageSink for RocksLineageStore {
    fn write(&self, event: LineageEvent) -> Result<()> {
        // Use writer pool if enabled (high throughput mode)
        if let Some(ref pool) = self.writer_pool {
            pool.write(event)?;
            Ok(())
        } else {
            // Direct write (synchronous mode)
            self.write_batch(vec![event])
        }
    }

    fn get_record_lineage(&self, record_id: &str) -> Result<Vec<LineageEvent>> {
        let start = Instant::now();

        // Use inverted index with prefix scan
        let events = self.get_events_by_prefix(CF_RECORD_INDEX, record_id.as_bytes())?;
        let latency_us = start.elapsed().as_micros() as u64;

        metrics::record_read("rocksdb", "get_by_record", latency_us, events.len());

        Ok(events)
    }

    fn get_model_impact(&self, model_id: &str, version: &str) -> Result<Vec<LineageEvent>> {
        let start = Instant::now();
        let model_key = format!("{}:{}", model_id, version);

        // Use inverted index with prefix scan
        let events = self.get_events_by_prefix(CF_MODEL_INDEX, model_key.as_bytes())?;
        let latency_us = start.elapsed().as_micros() as u64;

        metrics::record_read("rocksdb", "get_by_model", latency_us, events.len());

        Ok(events)
    }

    fn query_by_time_range(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<LineageEvent>> {
        let start_key = Self::timestamp_to_key(start);
        let end_key = Self::timestamp_to_key(end);
        let cf_time = self.cf(CF_TIME_INDEX)?;

        let mut event_ids = BTreeSet::new();

        for item in self.db.iterator_cf(
            cf_time,
            IteratorMode::From(&start_key, rocksdb::Direction::Forward),
        ) {
            let (key, value) = item?;

            if key.as_ref() > end_key.as_slice() {
                break;
            }

            let event_id = String::from_utf8_lossy(value.as_ref()).to_string();
            event_ids.insert(event_id);
        }

        self.get_events_by_ids(&event_ids)
    }

    fn get_run_lineage(&self, run_id: &str) -> Result<Vec<LineageEvent>> {
        // Use inverted index with prefix scan
        self.get_events_by_prefix(CF_RUN_INDEX, run_id.as_bytes())
    }

    /// Time-travel query: Get lineage as it existed at a specific timestamp
    /// This is the KEY DIFFERENTIATOR vs. competitors like Informatica
    fn get_lineage_as_of(
        &self,
        record_id: &str,
        as_of: DateTime<Utc>,
    ) -> Result<Vec<LineageEvent>> {
        let cf_time_travel = self.cf(CF_TIME_TRAVEL_INDEX)?;

        // Build prefix for this record_id
        let prefix_start = format!("{}|", record_id);
        let prefix_end_ts = Self::timestamp_to_key(as_of);

        tracing::debug!(
            "Time-travel query: record_id={}, as_of={}, prefix={}",
            record_id,
            as_of,
            prefix_start
        );

        let mut event_ids = BTreeSet::new();

        // Iterate through time-travel index for this record up to as_of timestamp
        for item in self
            .db
            .prefix_iterator_cf(cf_time_travel, prefix_start.as_bytes())
        {
            let (key, value) = item?;

            tracing::debug!(
                "Time-travel index key: {:?}, value: {}",
                String::from_utf8_lossy(&key),
                String::from_utf8_lossy(&value)
            );

            // Check if still in our record_id prefix
            if !key.starts_with(prefix_start.as_bytes()) {
                tracing::debug!("Key doesn't match prefix, stopping");
                break;
            }

            // Extract timestamp from key (after record_id|)
            if let Some(ts_bytes) = key.get(prefix_start.len()..) {
                // Compare timestamp (big-endian, so lexicographic comparison works)
                if ts_bytes > prefix_end_ts.as_ref() {
                    tracing::debug!(
                        "Timestamp {} > cutoff, stopping",
                        String::from_utf8_lossy(ts_bytes)
                    );
                    break;
                }

                let event_id = String::from_utf8_lossy(value.as_ref()).to_string();
                tracing::debug!("Adding event_id: {}", event_id);
                event_ids.insert(event_id);
            }
        }

        tracing::debug!("Time-travel query found {} event IDs", event_ids.len());

        if event_ids.is_empty() {
            return Ok(vec![]);
        }

        self.get_events_by_ids(&event_ids)
    }
}

impl RocksLineageStore {
    /// Scan time index for events in range (for RDF recovery)
    pub fn scan_time_range(
        &self,
        start_key: &str,
        end_key: &str,
    ) -> Result<Box<dyn Iterator<Item = Result<(Box<[u8]>, Box<[u8]>), rocksdb::Error>> + '_>> {
        let cf_time = self.cf(CF_TIME_INDEX)?;

        let iter = self.db.iterator_cf(
            cf_time,
            IteratorMode::From(start_key.as_bytes(), rocksdb::Direction::Forward),
        );

        // Return a filtered iterator that stops at end_key
        let end_key_bytes = end_key.as_bytes().to_vec();
        Ok(Box::new(iter.take_while(move |item| {
            match item {
                Ok((key, _)) => key.as_ref() <= end_key_bytes.as_slice(),
                Err(_) => true, // Continue on errors (handled by caller)
            }
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphica_core::core::lineage::DataRef;
    use uuid::Uuid;

    #[test]
    fn test_multiple_events_per_record() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let store = RocksLineageStore::new(temp_dir.path().to_str().unwrap())?;

        let record_id = "test-record-123";

        // Create two events for same record
        let event1 = LineageEvent {
            id: Uuid::new_v4(),
            dataset: "customers".to_string(),
            record_id: record_id.to_string(),
            source_refs: vec![],
            transforms: vec![],
            model_refs: vec![],
            output_ref: DataRef {
                system: "test".to_string(),
                path: "test".to_string(),
                version: None,
                extracted_at: Utc::now(),
                cdc_position: None,
            },
            ts: Utc::now(),
            run_id: "run-1".to_string(),
            tenant_id: "tenant-a".to_string(),
            correlation_id: Some(Uuid::new_v4().to_string()),
            metadata: HashMap::new(),
        };

        let event2 = LineageEvent {
            id: Uuid::new_v4(),
            record_id: record_id.to_string(),
            ..event1.clone()
        };

        // Write both events
        store.write(event1.clone())?;
        store.write(event2.clone())?;

        // Should get both events back
        let events = store.get_record_lineage(record_id)?;
        assert_eq!(
            events.len(),
            2,
            "Should retrieve both events for the same record"
        );

        Ok(())
    }

    #[test]
    fn test_batch_write() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let store = RocksLineageStore::new(temp_dir.path().to_str().unwrap())?;

        let events: Vec<LineageEvent> = (0..10)
            .map(|i| LineageEvent {
                id: Uuid::new_v4(),
                dataset: "test".to_string(),
                record_id: format!("record-{}", i),
                source_refs: vec![],
                transforms: vec![],
                model_refs: vec![],
                output_ref: DataRef {
                    system: "test".to_string(),
                    path: "test".to_string(),
                    version: None,
                    extracted_at: Utc::now(),
                    cdc_position: None,
                },
                ts: Utc::now(),
                run_id: "run-1".to_string(),
                tenant_id: "tenant-a".to_string(),
                correlation_id: Some(Uuid::new_v4().to_string()),
                metadata: HashMap::new(),
            })
            .collect();

        store.write_batch(events)?;

        // Verify run index works
        let run_events = store.get_run_lineage("run-1")?;
        assert_eq!(run_events.len(), 10, "Should retrieve all events for run");

        Ok(())
    }

    #[test]
    fn test_metrics_instrumentation() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let store = RocksLineageStore::new(temp_dir.path().to_str().unwrap())?;

        // Create a test event
        let event = LineageEvent {
            id: Uuid::new_v4(),
            dataset: "metrics_test".to_string(),
            record_id: "record-metrics".to_string(),
            source_refs: vec![],
            transforms: vec![],
            model_refs: vec![],
            output_ref: DataRef {
                system: "test".to_string(),
                path: "test".to_string(),
                version: None,
                extracted_at: Utc::now(),
                cdc_position: None,
            },
            ts: Utc::now(),
            run_id: "metrics-run".to_string(),
            tenant_id: "metrics-tenant".to_string(),
            correlation_id: Some(Uuid::new_v4().to_string()),
            metadata: HashMap::new(),
        };

        // Write event
        store.write(event.clone())?;

        // Verify metrics were recorded by checking Prometheus registry
        use crate::storage::metrics;

        let write_metric = metrics::STORAGE_EVENTS_WRITTEN_TOTAL
            .get_metric_with_label_values(&["rocksdb", "success"])
            .expect("Metric should exist");

        assert!(
            write_metric.get() >= 1,
            "Should have recorded at least 1 write"
        );

        let rocksdb_ops = metrics::ROCKSDB_OPERATIONS_TOTAL
            .get_metric_with_label_values(&["put", CF_PRIMARY])
            .expect("RocksDB metric should exist");

        assert!(
            rocksdb_ops.get() >= 1,
            "Should have recorded RocksDB operations"
        );

        // Read the event back and verify read metrics
        let events = store.get_record_lineage("record-metrics")?;
        assert_eq!(events.len(), 1);

        let read_metric = metrics::STORAGE_EVENTS_READ_TOTAL
            .get_metric_with_label_values(&["rocksdb", "get_by_record"])
            .expect("Read metric should exist");

        assert!(
            read_metric.get() >= 1,
            "Should have recorded at least 1 read"
        );

        Ok(())
    }
}
