//! Streaming deduplication implementation that scales to billions of rows
//!
//! This module provides a memory-efficient deduplicator that processes data in
//! streaming batches, maintaining dedup state in RocksDB for massive datasets.

use bloomfilter::Bloom;
use lru::LruCache;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[cfg(feature = "workflow-storage")]
use rocksdb::{WriteBatch, DB};

use super::{
    definition::{DedupMethod, DeduplicatorConfig, KeepStrategy},
    error::{Result, WorkflowError},
    execution_context_v2::ExecutionContextV2,
    lineage_tracker::TransformationType,
    row_storage::{BatchIterator, RowStorage, StorageManager},
};
use crate::core::lineage::row_level::{RowId, RowLineageEvent};

/// Statistics for deduplication
#[derive(Debug, Default, Clone, Serialize)]
pub struct DedupStats {
    pub total_processed: usize,
    pub kept_count: usize,
    pub duplicate_count: usize,
    pub merge_count: usize,
    pub batches_processed: usize,
    pub cache_hits: usize,
    pub cache_misses: usize,
    pub rocks_lookups: usize,
    pub memory_flushes: usize,
}

/// Decision for each row during deduplication
#[derive(Debug, Clone)]
pub enum DedupDecision {
    Keep {
        row: serde_json::Value,
        merged_from: Option<Vec<String>>,
    },
    Skip {
        reason: String,
        original_key: String,
    },
}

/// Entry tracking duplicate occurrences
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DedupEntry {
    // First occurrence
    pub first_seen_idx: u64,
    pub first_row_id: Option<String>,
    pub first_timestamp: i64,

    // Last occurrence (for Last strategy)
    pub last_seen_idx: u64,
    pub last_row_id: Option<String>,
    pub last_timestamp: i64,

    // Statistics
    pub occurrence_count: u32,
    pub access_count: u32,

    // Quality tracking (for HighestQuality strategy)
    pub quality_score: Option<f64>,
    pub quality_metadata: Option<serde_json::Value>,

    // Merged fields (for Merge strategy)
    pub merged_fields: Option<HashMap<String, serde_json::Value>>,
}

impl DedupEntry {
    fn new(idx: u64, row_id: Option<String>) -> Self {
        let now = chrono::Utc::now().timestamp();
        Self {
            first_seen_idx: idx,
            first_row_id: row_id.clone(),
            first_timestamp: now,
            last_seen_idx: idx,
            last_row_id: row_id,
            last_timestamp: now,
            occurrence_count: 1,
            access_count: 1,
            quality_score: None,
            quality_metadata: None,
            merged_fields: None,
        }
    }

    fn update(&mut self, idx: u64, row_id: Option<String>) {
        self.last_seen_idx = idx;
        self.last_row_id = row_id;
        self.last_timestamp = chrono::Utc::now().timestamp();
        self.occurrence_count += 1;
        self.access_count += 1;
    }
}

/// Configuration for streaming deduplication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamingDedupConfig {
    /// Base deduplicator config
    #[serde(flatten)]
    pub base: DeduplicatorConfig,

    /// Batch size for processing (default: 10,000)
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,

    /// Maximum memory for dedup state in bytes (default: 100MB)
    #[serde(default = "default_max_memory")]
    pub max_memory_bytes: usize,

    /// LRU cache size (number of entries, default: 100,000)
    #[serde(default = "default_cache_size")]
    pub cache_size: usize,

    /// Bloom filter expected items (default: 1,000,000)
    #[serde(default = "default_bloom_size")]
    pub bloom_expected_items: usize,

    /// Bloom filter false positive rate (default: 0.01)
    #[serde(default = "default_bloom_fp_rate")]
    pub bloom_false_positive_rate: f64,

    /// Enable parallel batch processing
    #[serde(default = "default_true")]
    pub parallel_processing: bool,

    /// Number of parallel workers (default: 4)
    #[serde(default = "default_workers")]
    pub num_workers: usize,
}

fn default_batch_size() -> usize {
    50_000
} // Increased from 10K to 50K for better throughput
fn default_max_memory() -> usize {
    100_000_000
} // 100MB
fn default_cache_size() -> usize {
    100_000
}
fn default_bloom_size() -> usize {
    1_000_000
}
fn default_bloom_fp_rate() -> f64 {
    0.01
}
fn default_true() -> bool {
    true
}
fn default_workers() -> usize {
    4
}

/// Manages deduplication state with tiered storage
pub struct DedupStateManager {
    /// Memory cache for hot keys
    memory_cache: LruCache<String, DedupEntry>,

    /// Bloom filter for fast negative lookups
    bloom_filter: Bloom<String>,

    /// RocksDB handle for persistent state
    #[cfg(feature = "workflow-storage")]
    rocks_handle: Option<Arc<DB>>,

    /// Key prefix for RocksDB
    key_prefix: String,

    /// Memory tracking
    memory_limit: usize,
    current_memory: AtomicUsize,

    /// Statistics
    stats: Arc<RwLock<DedupStats>>,

    /// Current row index
    current_idx: AtomicUsize,
}

impl DedupStateManager {
    pub fn new(
        storage_manager: Option<Arc<StorageManager>>,
        execution_id: String,
        config: &StreamingDedupConfig,
    ) -> Result<Self> {
        let cache_size = NonZeroUsize::new(config.cache_size)
            .ok_or_else(|| WorkflowError::InvalidData("Invalid cache size".into()))?;

        #[cfg(feature = "workflow-storage")]
        let rocks_handle = storage_manager.as_ref().map(|sm| sm.rocks_db());

        #[cfg(not(feature = "workflow-storage"))]
        let rocks_handle = None;

        Ok(Self {
            memory_cache: LruCache::new(cache_size),
            bloom_filter: Bloom::new_for_fp_rate(
                config.bloom_expected_items,
                config.bloom_false_positive_rate,
            ),
            #[cfg(feature = "workflow-storage")]
            rocks_handle,
            key_prefix: format!("dedup/{}/{}", execution_id, chrono::Utc::now().timestamp()),
            memory_limit: config.max_memory_bytes,
            current_memory: AtomicUsize::new(0),
            stats: Arc::new(RwLock::new(DedupStats::default())),
            current_idx: AtomicUsize::new(0),
        })
    }

    /// Process a batch of rows and return dedup decisions
    pub fn process_batch(
        &mut self,
        keyed_rows: Vec<(String, serde_json::Value)>,
        keep_strategy: &KeepStrategy,
    ) -> Result<Vec<DedupDecision>> {
        let mut decisions = Vec::with_capacity(keyed_rows.len());

        for (key, row) in keyed_rows {
            let row_idx = self.current_idx.fetch_add(1, Ordering::SeqCst) as u64;

            // Update stats atomically
            {
                let mut stats = self.stats.write();
                stats.total_processed += 1;
            }

            // Extract row ID if present
            let row_id = row
                .get("_row_id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            // 1. Fast path: Check bloom filter
            if !self.bloom_filter.check(&key) {
                // Definitely new key
                self.bloom_filter.set(&key);
                let entry = DedupEntry::new(row_idx, row_id);
                self.insert_entry(key.clone(), entry)?;

                decisions.push(DedupDecision::Keep {
                    row,
                    merged_from: None,
                });

                let mut stats = self.stats.write();
                stats.kept_count += 1;
                drop(stats);
                continue;
            }

            // 2. Check memory cache
            let cache_entry = self.memory_cache.get_mut(&key).map(|entry| {
                {
                    let mut stats = self.stats.write();
                    stats.cache_hits += 1;
                }

                entry.update(row_idx, row_id.clone());
                entry.clone() // Clone to release mutable borrow
            });

            if let Some(entry) = cache_entry {
                let decision = self.apply_keep_strategy(&entry, row, keep_strategy, &key)?;

                if matches!(decision, DedupDecision::Skip { .. }) {
                    let mut stats = self.stats.write();
                    stats.duplicate_count += 1;
                }
                decisions.push(decision);
                continue;
            }

            {
                let mut stats = self.stats.write();
                stats.cache_misses += 1;
            }

            // 3. Check RocksDB (if available)
            #[cfg(feature = "workflow-storage")]
            if let Some(rocks) = &self.rocks_handle {
                {
                    let mut stats = self.stats.write();
                    stats.rocks_lookups += 1;
                }

                let db_key = format!("{}/{}", self.key_prefix, key);

                if let Ok(Some(entry_bytes)) = rocks.get(db_key.as_bytes()) {
                    if let Ok(mut entry) = bincode::deserialize::<DedupEntry>(&entry_bytes) {
                        entry.update(row_idx, row_id.clone());

                        // Promote to cache if hot
                        if entry.access_count > 2 {
                            self.memory_cache.put(key.clone(), entry.clone());
                        }

                        let decision =
                            self.apply_keep_strategy(&entry, row, keep_strategy, &key)?;

                        if matches!(decision, DedupDecision::Skip { .. }) {
                            let mut stats = self.stats.write();
                            stats.duplicate_count += 1;
                        }

                        // Update in RocksDB
                        if let Ok(updated_bytes) = bincode::serialize(&entry) {
                            let _ = rocks.put(db_key.as_bytes(), &updated_bytes);
                        }

                        decisions.push(decision);
                        continue;
                    }
                }
            }

            // 4. New key (bloom filter false positive)
            self.bloom_filter.set(&key);
            let entry = DedupEntry::new(row_idx, row_id);
            self.insert_entry(key.clone(), entry)?;

            decisions.push(DedupDecision::Keep {
                row,
                merged_from: None,
            });

            {
                let mut stats = self.stats.write();
                stats.kept_count += 1;
            }
        }

        {
            let mut stats = self.stats.write();
            stats.batches_processed += 1;
        }

        // Check memory pressure
        if self.estimate_memory() > self.memory_limit {
            self.flush_to_disk()?;
            let mut stats = self.stats.write();
            stats.memory_flushes += 1;
        }

        Ok(decisions)
    }

    /// Apply keep strategy to determine decision
    fn apply_keep_strategy(
        &self,
        entry: &DedupEntry,
        current_row: serde_json::Value,
        strategy: &KeepStrategy,
        key: &str,
    ) -> Result<DedupDecision> {
        match strategy {
            KeepStrategy::First => {
                // Skip current, we already have the first
                Ok(DedupDecision::Skip {
                    reason: format!("Duplicate of row at index {}", entry.first_seen_idx),
                    original_key: key.to_string(),
                })
            }
            KeepStrategy::Last => {
                // Keep current, it's the latest
                Ok(DedupDecision::Keep {
                    row: current_row,
                    merged_from: entry.first_row_id.as_ref().map(|id| vec![id.clone()]),
                })
            }
            KeepStrategy::Merge => {
                // Merge fields from previous occurrences
                let mut merged_row = current_row.clone();

                if let Some(merged_fields) = &entry.merged_fields {
                    if let Some(obj) = merged_row.as_object_mut() {
                        for (field, value) in merged_fields {
                            if !obj.contains_key(field) || obj[field].is_null() {
                                obj.insert(field.clone(), value.clone());
                            }
                        }
                    }
                }

                Ok(DedupDecision::Keep {
                    row: merged_row,
                    merged_from: entry.first_row_id.as_ref().map(|id| vec![id.clone()]),
                })
            }
            KeepStrategy::HighestQuality => {
                // Calculate quality score for current row
                let current_quality = self.calculate_quality_score(&current_row);

                if let Some(existing_quality) = entry.quality_score {
                    if current_quality > existing_quality {
                        Ok(DedupDecision::Keep {
                            row: current_row,
                            merged_from: entry.first_row_id.as_ref().map(|id| vec![id.clone()]),
                        })
                    } else {
                        Ok(DedupDecision::Skip {
                            reason: format!(
                                "Lower quality ({:.2}) than existing ({:.2})",
                                current_quality, existing_quality
                            ),
                            original_key: key.to_string(),
                        })
                    }
                } else {
                    Ok(DedupDecision::Keep {
                        row: current_row,
                        merged_from: None,
                    })
                }
            }
        }
    }

    /// Calculate quality score for a row
    fn calculate_quality_score(&self, row: &serde_json::Value) -> f64 {
        if let Some(obj) = row.as_object() {
            let mut score = 0.0;
            let mut max_score = 0.0;

            for (_key, value) in obj {
                max_score += 1.0;
                if !value.is_null() {
                    score += 1.0;
                    // Bonus for non-empty strings
                    if let Some(s) = value.as_str() {
                        if !s.trim().is_empty() {
                            score += 0.5;
                        }
                    }
                }
            }

            if max_score > 0.0 {
                score / max_score
            } else {
                0.0
            }
        } else {
            0.0
        }
    }

    /// Insert entry into appropriate storage tier
    fn insert_entry(&mut self, key: String, entry: DedupEntry) -> Result<()> {
        // Try to add to memory cache
        if self.memory_cache.len() < self.memory_cache.cap().get() {
            self.memory_cache.put(key, entry);
            Ok(())
        } else {
            // Cache is full, write to RocksDB if available
            #[cfg(feature = "workflow-storage")]
            if let Some(rocks) = &self.rocks_handle {
                let db_key = format!("{}/{}", self.key_prefix, key);
                if let Ok(value_bytes) = bincode::serialize(&entry) {
                    rocks
                        .put(db_key.as_bytes(), &value_bytes)
                        .map_err(|e| WorkflowError::Storage(e.to_string()))?;
                }
            }
            Ok(())
        }
    }

    /// Flush cold entries from memory to disk
    pub fn flush_to_disk(&mut self) -> Result<()> {
        #[cfg(feature = "workflow-storage")]
        if let Some(rocks) = &self.rocks_handle {
            let mut batch = WriteBatch::default();
            let flush_count = self.memory_cache.len() / 2;

            for _ in 0..flush_count {
                if let Some((key, entry)) = self.memory_cache.pop_lru() {
                    let db_key = format!("{}/{}", self.key_prefix, key);
                    if let Ok(value_bytes) = bincode::serialize(&entry) {
                        batch.put(db_key.as_bytes(), &value_bytes);
                    }
                }
            }

            rocks
                .write(batch)
                .map_err(|e| WorkflowError::Storage(e.to_string()))?;

            tracing::info!("Flushed {} entries to RocksDB", flush_count);
        }

        Ok(())
    }

    /// Estimate current memory usage
    fn estimate_memory(&self) -> usize {
        // Rough estimate: 1KB per cache entry + bloom filter size
        let cache_memory = self.memory_cache.len() * 1024;
        let bloom_memory = (self.bloom_filter.number_of_bits() / 8) as usize;
        cache_memory + bloom_memory
    }

    /// Get statistics
    pub fn stats(&self) -> DedupStats {
        self.stats.read().clone()
    }
}

/// Main streaming deduplicator implementation
pub struct StreamingDeduplicator {
    config: StreamingDedupConfig,
    state_manager: DedupStateManager,
    lineage_tracker: Option<Arc<dyn super::lineage_tracker::LineageTracker>>,
    /// Stores output rows from last execution
    output_rows: Option<Vec<serde_json::Value>>,
}

impl StreamingDeduplicator {
    pub fn new(
        config: StreamingDedupConfig,
        context: &ExecutionContextV2,
        lineage_tracker: Option<Arc<dyn super::lineage_tracker::LineageTracker>>,
    ) -> Result<Self> {
        let state_manager = DedupStateManager::new(
            context.storage_manager.clone(),
            context.execution_id.clone(),
            &config,
        )?;

        Ok(Self {
            config,
            state_manager,
            lineage_tracker,
            output_rows: None,
        })
    }

    /// Execute streaming deduplication
    pub async fn execute(
        &mut self,
        context: &ExecutionContextV2,
    ) -> Result<(bool, serde_json::Value, f64)> {
        tracing::info!(
            "Starting streaming deduplication: method={:?}, keys={:?}, batch_size={}",
            self.config.base.method,
            self.config.base.key_fields,
            self.config.batch_size
        );

        // Get row storage from context
        let row_storage = context
            .row_storage
            .as_ref()
            .ok_or_else(|| WorkflowError::DataNotFound("No row storage in context".into()))?;

        let total_rows = row_storage.len();
        tracing::info!("Processing {} total rows", total_rows);

        // Create output storage
        let mut output_rows = Vec::new();
        let mut lineage_events = Vec::new();

        // Process in batches
        let batch_iter = BatchIterator::new(row_storage.clone(), self.config.batch_size);

        for (batch_idx, batch_result) in batch_iter.enumerate() {
            let batch = batch_result?;

            // Build dedup keys for batch
            let keyed_batch = self.build_dedup_keys(batch)?;

            // Process batch through state manager
            let decisions = self
                .state_manager
                .process_batch(keyed_batch, &self.config.base.keep)?;

            // Apply decisions
            for decision in decisions {
                match decision {
                    DedupDecision::Keep { row, merged_from } => {
                        output_rows.push(row.clone());

                        // Track lineage if enabled
                        if let Some(merged) = merged_from {
                            if let Some(tracker) = &self.lineage_tracker {
                                self.track_merge_lineage(&row, merged, &mut lineage_events)?;
                            }
                        }
                    }
                    DedupDecision::Skip {
                        reason,
                        original_key,
                    } => {
                        // Track filtered lineage
                        if let Some(tracker) = &self.lineage_tracker {
                            self.track_skip_lineage(&original_key, reason, &mut lineage_events)?;
                        }
                    }
                }
            }

            // Progress reporting
            if batch_idx % 10 == 0 {
                let stats = self.state_manager.stats();
                tracing::info!(
                    "Dedup progress: batch {}, processed {}/{} rows, {} duplicates found (cache hits: {}, misses: {})",
                    batch_idx,
                    stats.total_processed,
                    total_rows,
                    stats.duplicate_count,
                    stats.cache_hits,
                    stats.cache_misses
                );
            }
        }

        // Record lineage events
        if !lineage_events.is_empty() {
            if let Some(tracker) = &self.lineage_tracker {
                tracker
                    .record_row_lineage_batch(lineage_events)
                    .await
                    .unwrap_or_else(|e| {
                        tracing::warn!("Failed to record lineage: {}", e);
                    });
            }
        }

        // Get final stats
        let stats = self.state_manager.stats();

        tracing::info!(
            "Deduplication complete: {} -> {} rows ({} duplicates removed, {:.1}% dedup rate)",
            stats.total_processed,
            stats.kept_count,
            stats.duplicate_count,
            (stats.duplicate_count as f64 / stats.total_processed as f64) * 100.0
        );

        // Create output metadata
        let output = serde_json::json!({
            "_row_count": stats.kept_count,
            "_original_count": stats.total_processed,
            "_duplicates_removed": stats.duplicate_count,
            "_dedup_rate_pct": (stats.duplicate_count as f64 / stats.total_processed as f64) * 100.0,
            "_cache_hit_rate": stats.cache_hits as f64 / (stats.cache_hits + stats.cache_misses) as f64,
            "_memory_flushes": stats.memory_flushes,
            "_output_rows": output_rows.len(),  // Include count in metadata
        });

        // Store output rows in self for retrieval
        self.output_rows = Some(output_rows);

        Ok((true, output, 1.0))
    }

    /// Get the deduplicated output rows from the last execution
    /// This should be called after execute() to retrieve the results
    pub fn take_output_rows(&mut self) -> Option<Vec<serde_json::Value>> {
        self.output_rows.take()
    }

    /// Execute and return rows directly (convenience method)
    pub async fn execute_and_get_rows(
        &mut self,
        context: &ExecutionContextV2,
    ) -> Result<Vec<serde_json::Value>> {
        let (success, _output, _confidence) = self.execute(context).await?;

        if !success {
            return Err(WorkflowError::Other(
                "Streaming deduplication failed".into(),
            ));
        }

        self.take_output_rows()
            .ok_or_else(|| WorkflowError::DataNotFound("No output rows from deduplication".into()))
    }

    /// Build dedup keys for a batch of rows
    fn build_dedup_keys(
        &self,
        batch: Vec<serde_json::Value>,
    ) -> Result<Vec<(String, serde_json::Value)>> {
        let mut keyed_rows = Vec::with_capacity(batch.len());

        for row in batch {
            let key = self.build_key_for_row(&row)?;
            keyed_rows.push((key, row));
        }

        Ok(keyed_rows)
    }

    /// Build dedup key for a single row
    fn build_key_for_row(&self, row: &serde_json::Value) -> Result<String> {
        let key = self
            .config
            .base
            .key_fields
            .iter()
            .map(|field| {
                row.get(field)
                    .map(|v| match v {
                        serde_json::Value::String(s) => s.clone(),
                        _ => v.to_string(),
                    })
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>()
            .join("|");

        // Apply method-specific normalization
        let normalized = match &self.config.base.method {
            DedupMethod::Exact => key,
            DedupMethod::Fuzzy { .. } => key.to_lowercase().trim().to_string(),
            DedupMethod::Semantic { .. } => {
                // TODO: Implement semantic hashing
                key
            }
        };

        Ok(normalized)
    }

    /// Track merge lineage
    fn track_merge_lineage(
        &self,
        _kept_row: &serde_json::Value,
        _merged_ids: Vec<String>,
        _events: &mut Vec<RowLineageEvent>,
    ) -> Result<()> {
        // TODO: Implement merge lineage tracking
        Ok(())
    }

    /// Track skip lineage
    fn track_skip_lineage(
        &self,
        _key: &str,
        _reason: String,
        _events: &mut Vec<RowLineageEvent>,
    ) -> Result<()> {
        // TODO: Implement skip lineage tracking
        Ok(())
    }
}

/// Build a streaming deduplicator from config
pub fn build_streaming_deduplicator(
    config: DeduplicatorConfig,
    context: &ExecutionContextV2,
    lineage_tracker: Option<Arc<dyn super::lineage_tracker::LineageTracker>>,
) -> Result<StreamingDeduplicator> {
    let streaming_config = StreamingDedupConfig {
        base: config,
        batch_size: 10_000,
        max_memory_bytes: 100_000_000,
        cache_size: 100_000,
        bloom_expected_items: 1_000_000,
        bloom_false_positive_rate: 0.01,
        parallel_processing: true,
        num_workers: 4,
    };

    StreamingDeduplicator::new(streaming_config, context, lineage_tracker)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dedup_entry_creation() {
        let entry = DedupEntry::new(42, Some("row_42".to_string()));
        assert_eq!(entry.first_seen_idx, 42);
        assert_eq!(entry.occurrence_count, 1);
    }

    #[test]
    fn test_quality_score_calculation() {
        let mgr = DedupStateManager::new(
            None,
            "test".to_string(),
            &StreamingDedupConfig {
                base: DeduplicatorConfig {
                    method: DedupMethod::Exact,
                    key_fields: vec!["id".to_string()],
                    threshold: None,
                    keep: KeepStrategy::HighestQuality,
                },
                batch_size: 10_000,
                max_memory_bytes: 100_000_000,
                cache_size: 100_000,
                bloom_expected_items: 1_000_000,
                bloom_false_positive_rate: 0.01,
                parallel_processing: true,
                num_workers: 4,
            },
        )
        .unwrap();

        let row = serde_json::json!({
            "id": "123",
            "name": "John",
            "email": "",
            "phone": null,
        });

        let score = mgr.calculate_quality_score(&row);
        assert!((score - 1.0).abs() < f64::EPSILON);
    }
}
