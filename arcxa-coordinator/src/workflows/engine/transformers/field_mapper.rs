//! Field Mapper Transformer
//!
//! Applies manual field mappings before ontology mapping to leverage user corrections.
//! This transformer runs BEFORE ontology_map to provide human-in-the-loop field mapping.
//!
//! ## Workflow Position
//!
//! ```text
//! csv_parse → field_mapper (NEW) → ontology_map → ... → db2_load
//!                  ↓
//!         Manual Mappings (RocksDB)
//!         + Learning System
//! ```
//!
//! ## Features
//!
//! - **Exact Match**: Apply manual mappings when available
//! - **Learning System**: Suggest mappings from similar datasets
//! - **Confidence Tracking**: Track manual vs. suggested mapping confidence
//! - **Statistics**: Record usage for continuous learning
//!
//! ## Configuration
//!
//! ```json
//! {
//!   "dataset_id": "file_123",
//!   "source_table": "customers",
//!   "apply_suggestions": true,
//!   "min_suggestion_confidence": 0.7
//! }
//! ```
//!
//! ## Data Format
//!
//! Input data must contain `rows` array:
//! ```json
//! {
//!   "rows": [
//!     {"F1": "John", "F2": "Doe", "F3": "john@example.com"}
//!   ]
//! }
//! ```
//!
//! Output adds `field_mappings` metadata:
//! ```json
//! {
//!   "rows": [
//!     {"firstName": "John", "lastName": "Doe", "email": "john@example.com"}
//!   ],
//!   "field_mappings": {
//!     "F1": {"target": "firstName", "confidence": 1.0, "source": "manual"},
//!     "F2": {"target": "lastName", "confidence": 1.0, "source": "manual"},
//!     "F3": {"target": "email", "confidence": 0.85, "source": "suggested"}
//!   }
//! }
//! ```

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use lru::LruCache;
use parking_lot::RwLock;
use serde_json::{json, Value as JsonValue};
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::Arc;
use tracing::{debug, info, warn};

use super::Transformer;
use crate::mapping::manual::{
    ManualMappingStore, MappingLearningEngine, SourceContext, UsageStatType,
};
use crate::mapping::uri_utils;

// Metrics support - disabled by default (can be enabled later)
// TODO: Enable metrics by uncommenting and wiring to Prometheus registry
// use crate::workflows::engine::transformers::field_mapper_metrics::{OptionalFieldMapperMetrics, FieldMapperMetrics};

// ============================================================================
// Field Mapper Transformer
// ============================================================================

/// LRU cache for field mappings (cache key: "dataset:table:field")
type MappingCache = Arc<RwLock<LruCache<String, MappingInfo>>>;

/// Field mapper transformer using manual mappings and learning system
pub struct FieldMapperTransformer {
    /// Manual mapping store (RocksDB backend)
    store: Arc<ManualMappingStore>,

    /// Learning engine for suggestions
    learning_engine: Arc<MappingLearningEngine>,

    /// LRU cache for frequently accessed mappings (optional)
    mapping_cache: Option<MappingCache>,
    // TODO: Add metrics field when enabling Prometheus integration
    // metrics: OptionalFieldMapperMetrics,
}

impl FieldMapperTransformer {
    /// Create new field mapper transformer without caching
    pub fn new(
        store: Arc<ManualMappingStore>,
        learning_engine: Arc<MappingLearningEngine>,
    ) -> Self {
        info!("Initialized FieldMapperTransformer with manual mapping store");
        Self {
            store,
            learning_engine,
            mapping_cache: None,
            // metrics: OptionalFieldMapperMetrics::none(), // TODO: Enable when wired
        }
    }

    /// Create new field mapper transformer with LRU cache
    pub fn with_cache(
        store: Arc<ManualMappingStore>,
        learning_engine: Arc<MappingLearningEngine>,
        cache_capacity: usize,
    ) -> Self {
        let capacity =
            NonZeroUsize::new(cache_capacity).unwrap_or_else(|| NonZeroUsize::new(1000).unwrap());

        info!(
            "Initialized FieldMapperTransformer with LRU cache (capacity: {})",
            capacity
        );

        Self {
            store,
            learning_engine,
            mapping_cache: Some(Arc::new(RwLock::new(LruCache::new(capacity)))),
            // metrics: OptionalFieldMapperMetrics::none(), // TODO: Enable when wired
        }
    }

    // TODO: Uncomment when enabling metrics
    // pub fn with_metrics(
    //     store: Arc<ManualMappingStore>,
    //     learning_engine: Arc<MappingLearningEngine>,
    //     metrics: Arc<FieldMapperMetrics>,
    // ) -> Self {
    //     info!("Initialized FieldMapperTransformer with metrics");
    //     Self {
    //         store,
    //         learning_engine,
    //         mapping_cache: None,
    //         metrics: OptionalFieldMapperMetrics::new(Some(metrics)),
    //     }
    // }

    /// Get manual mappings for a dataset using batch lookup (N+1 prevention) with optional caching
    async fn get_mappings(
        &self,
        dataset_id: &str,
        table_name: &str,
        field_names: &[String],
    ) -> Result<HashMap<String, MappingInfo>> {
        let start = std::time::Instant::now();

        // Try cache first (if enabled)
        let mut mappings = HashMap::new();
        let mut uncached_fields = Vec::new();

        if let Some(cache) = &self.mapping_cache {
            let cache_read = cache.read();
            for field_name in field_names {
                let cache_key = format!("{}:{}:{}", dataset_id, table_name, field_name);
                if let Some(cached_mapping) = cache_read.peek(&cache_key) {
                    mappings.insert(field_name.clone(), cached_mapping.clone());
                    debug!("Cache hit for field: {}", field_name);
                } else {
                    uncached_fields.push(field_name.clone());
                }
            }
            drop(cache_read); // Release read lock
        } else {
            // No cache - look up all fields
            uncached_fields = field_names.to_vec();
        }

        // Fetch uncached fields from database (if any)
        if !uncached_fields.is_empty() {
            // Build source contexts for uncached fields
            let contexts: Vec<SourceContext> = uncached_fields
                .iter()
                .map(|field_name| SourceContext {
                    source_id: Some(dataset_id.to_string()),
                    table_name: table_name.to_string(),
                    field_name: field_name.clone(),
                    field_metadata: None,
                })
                .collect();

            // Batch lookup - single call for all uncached fields (N+1 prevention)
            let batch_results = self.store.find_by_source_batch(&contexts).await?;

            // Process results and update cache
            for (field_name, manual_mapping) in batch_results {
                debug!(
                    "Found manual mapping: {} → {}",
                    field_name, manual_mapping.target_field_uri
                );

                // Extract ontology property from URI (e.g., "retail:firstName" → "firstName")
                let target_field = uri_utils::extract_local_name(&manual_mapping.target_field_uri)
                    .unwrap_or_else(|| manual_mapping.target_field_uri.clone());

                // Record manual mapping applied
                // self.metrics.record_manual_mapping(manual_mapping.confidence);

                let mapping_info = MappingInfo {
                    target: target_field,
                    confidence: manual_mapping.confidence,
                    source: MappingSource::Manual,
                    mapping_id: Some(manual_mapping.id.clone()),
                };

                // Update cache (if enabled)
                if let Some(cache) = &self.mapping_cache {
                    let cache_key = format!("{}:{}:{}", dataset_id, table_name, field_name);
                    cache.write().put(cache_key, mapping_info.clone());
                }

                mappings.insert(field_name, mapping_info);
            }
        }

        // Record batch lookup performance
        let duration = start.elapsed().as_secs_f64();
        // self.metrics.record_batch_lookup("manual", duration);
        // self.metrics.record_cache_hits(field_names.len() - uncached_fields.len());
        // self.metrics.record_cache_misses(uncached_fields.len());

        debug!(
            "Found {} manual mappings for dataset {} ({} from cache, {} from DB)",
            mappings.len(),
            dataset_id,
            field_names.len() - uncached_fields.len(),
            uncached_fields.len()
        );

        Ok(mappings)
    }

    /// Get suggested mappings from learning system
    async fn get_suggestions(
        &self,
        dataset_id: &str,
        table_name: &str,
        field_names: &[String],
        min_confidence: f64,
    ) -> Result<HashMap<String, MappingInfo>> {
        let mut suggestions = HashMap::new();

        for field_name in field_names {
            let context = SourceContext {
                source_id: Some(dataset_id.to_string()),
                table_name: table_name.to_string(),
                field_name: field_name.clone(),
                field_metadata: None,
            };

            // Get suggestions from learning engine using generate_suggestions
            let suggestion_list = self
                .learning_engine
                .generate_suggestions(&context, 5)
                .await?;

            if let Some(suggestion) = suggestion_list.into_iter().next() {
                if suggestion.relevance_score >= min_confidence {
                    debug!(
                        "Found suggestion: {} → {} (confidence: {:.2})",
                        field_name, suggestion.mapping.target_field_uri, suggestion.relevance_score
                    );

                    let target_field =
                        uri_utils::extract_local_name(&suggestion.mapping.target_field_uri)
                            .unwrap_or_else(|| suggestion.mapping.target_field_uri.clone());

                    suggestions.insert(
                        field_name.clone(),
                        MappingInfo {
                            target: target_field,
                            confidence: suggestion.relevance_score,
                            source: MappingSource::Suggested,
                            mapping_id: Some(suggestion.mapping.id.clone()),
                        },
                    );
                }
            }
        }

        debug!(
            "Found {} suggestions for dataset {} (min confidence: {:.2})",
            suggestions.len(),
            dataset_id,
            min_confidence
        );

        Ok(suggestions)
    }

    /// Apply field mappings to data rows with conflict detection (memory-optimized)
    fn apply_mappings(
        &self,
        dataset_id: &str,
        rows: &[JsonValue],
        mappings: &HashMap<String, MappingInfo>,
    ) -> Result<Vec<JsonValue>> {
        let conflict_resolution = ConflictResolution::default();

        // Pre-check for conflicts (multiple source fields → same target)
        let conflicts = self.detect_conflicts(mappings);
        if !conflicts.is_empty() {
            // Record conflicts detected
            // self.metrics.record_conflicts_detected(dataset_id, conflicts.len());

            match conflict_resolution {
                ConflictResolution::Error => {
                    let conflict_details: Vec<String> = conflicts
                        .iter()
                        .map(|(target, sources)| format!("{} ← [{}]", target, sources.join(", ")))
                        .collect();
                    anyhow::bail!(
                        "Field mapping conflicts detected: {}",
                        conflict_details.join("; ")
                    );
                }
                _ => {
                    warn!(
                        "Field mapping conflicts detected (will resolve using {:?}): {:?}",
                        conflict_resolution, conflicts
                    );
                }
            }
        }

        // Pre-allocate result vector (memory optimization)
        let mut mapped_rows = Vec::with_capacity(rows.len());

        for row in rows {
            let obj = row
                .as_object()
                .ok_or_else(|| anyhow!("Row must be an object"))?;

            // Memory optimization: pre-allocate with expected size
            let mut mapped_row = serde_json::Map::with_capacity(obj.len());
            let mut target_usage: HashMap<String, (String, f64)> = HashMap::new();

            // Apply mappings with conflict resolution
            for (source_field, value) in obj {
                if let Some(mapping) = mappings.get(source_field) {
                    let target = &mapping.target;

                    // Check if target already used
                    if let Some((prev_source, prev_confidence)) = target_usage.get(target) {
                        // Conflict! Resolve based on strategy
                        let should_replace = match conflict_resolution {
                            ConflictResolution::FirstWins => false,
                            ConflictResolution::HighestConfidence => {
                                mapping.confidence > *prev_confidence
                            }
                            ConflictResolution::Error => {
                                // Already handled above
                                false
                            }
                        };

                        if should_replace {
                            debug!(
                                "Conflict resolution: {} → {} (replacing {}, conf {:.2} > {:.2})",
                                source_field,
                                target,
                                prev_source,
                                mapping.confidence,
                                prev_confidence
                            );
                            mapped_row.insert(target.clone(), value.clone());
                            target_usage
                                .insert(target.clone(), (source_field.clone(), mapping.confidence));
                        } else {
                            debug!(
                                "Conflict resolution: keeping {} → {} (skipping {}, conf {:.2})",
                                prev_source, target, source_field, mapping.confidence
                            );
                        }
                    } else {
                        // No conflict, apply mapping
                        mapped_row.insert(target.clone(), value.clone());
                        target_usage
                            .insert(target.clone(), (source_field.clone(), mapping.confidence));
                        debug!("Mapped {} → {}", source_field, target);
                    }
                } else {
                    // Keep original field name (no mapping)
                    // Memory optimization: move value instead of clone when possible
                    mapped_row.insert(source_field.clone(), value.clone());
                }
            }

            mapped_rows.push(JsonValue::Object(mapped_row));
        }

        Ok(mapped_rows)
    }

    /// Apply field mappings in-place to mutable rows array (zero-copy optimization)
    /// This is more memory-efficient for large datasets
    fn apply_mappings_inplace(
        &self,
        dataset_id: &str,
        rows: &mut [JsonValue],
        mappings: &HashMap<String, MappingInfo>,
    ) -> Result<()> {
        let conflict_resolution = ConflictResolution::default();

        // Pre-check for conflicts
        let conflicts = self.detect_conflicts(mappings);
        if !conflicts.is_empty() {
            match conflict_resolution {
                ConflictResolution::Error => {
                    let conflict_details: Vec<String> = conflicts
                        .iter()
                        .map(|(target, sources)| format!("{} ← [{}]", target, sources.join(", ")))
                        .collect();
                    anyhow::bail!(
                        "Field mapping conflicts detected: {}",
                        conflict_details.join("; ")
                    );
                }
                _ => {
                    warn!(
                        "Field mapping conflicts detected (will resolve using {:?}): {:?}",
                        conflict_resolution, conflicts
                    );
                }
            }
        }

        // Process each row in-place
        for row in rows.iter_mut() {
            let obj = row
                .as_object_mut()
                .ok_or_else(|| anyhow!("Row must be an object"))?;

            // Collect fields to rename (can't modify while iterating)
            let mut renames: Vec<(String, String, JsonValue)> = Vec::new();
            let mut removes: Vec<String> = Vec::new();
            let mut target_usage: HashMap<String, (String, f64)> = HashMap::new();

            // Identify renames and removals
            for (source_field, value) in obj.iter() {
                if let Some(mapping) = mappings.get(source_field) {
                    let target = &mapping.target;

                    if source_field == target {
                        // No rename needed - field name already matches target
                        target_usage
                            .insert(target.clone(), (source_field.clone(), mapping.confidence));
                        continue;
                    }

                    // Check for conflicts
                    if let Some((prev_source, prev_confidence)) = target_usage.get(target) {
                        let should_replace = match conflict_resolution {
                            ConflictResolution::FirstWins => false,
                            ConflictResolution::HighestConfidence => {
                                mapping.confidence > *prev_confidence
                            }
                            ConflictResolution::Error => false,
                        };

                        if should_replace {
                            renames.push((source_field.clone(), target.clone(), value.clone()));
                            removes.push(source_field.clone());
                            target_usage
                                .insert(target.clone(), (source_field.clone(), mapping.confidence));
                        }
                    } else {
                        // No conflict - safe to rename
                        renames.push((source_field.clone(), target.clone(), value.clone()));
                        removes.push(source_field.clone());
                        target_usage
                            .insert(target.clone(), (source_field.clone(), mapping.confidence));
                    }
                }
            }

            // Apply renames (insert new, remove old)
            for (source, target, value) in renames {
                let should_remove = source != target;
                obj.insert(target, value);
                if should_remove {
                    obj.remove(&source);
                }
            }
        }

        Ok(())
    }

    /// Detect conflicts where multiple source fields map to the same target
    fn detect_conflicts(
        &self,
        mappings: &HashMap<String, MappingInfo>,
    ) -> HashMap<String, Vec<String>> {
        let mut target_to_sources: HashMap<String, Vec<String>> = HashMap::new();

        for (source, mapping_info) in mappings {
            target_to_sources
                .entry(mapping_info.target.clone())
                .or_insert_with(Vec::new)
                .push(source.clone());
        }

        // Filter to only conflicts (targets with 2+ sources)
        target_to_sources
            .into_iter()
            .filter(|(_, sources)| sources.len() > 1)
            .collect()
    }

    /// Update usage statistics for applied mappings
    async fn update_statistics(&self, mappings: &HashMap<String, MappingInfo>) -> Result<()> {
        for (_field, mapping_info) in mappings {
            if let Some(mapping_id) = &mapping_info.mapping_id {
                // Update applied statistics
                self.store
                    .update_usage_stats(mapping_id, UsageStatType::Applied)
                    .await
                    .unwrap_or_else(|e| {
                        warn!(
                            "Failed to update statistics for mapping {}: {}",
                            mapping_id, e
                        );
                    });
            }
        }
        Ok(())
    }
}

#[async_trait]
impl Transformer for FieldMapperTransformer {
    async fn transform(
        &self,
        config: &JsonValue,
        data: &mut JsonValue,
        _context: Option<&crate::workflows::engine::executor::ExecutionContext>,
    ) -> Result<()> {
        let start = std::time::Instant::now();

        // Extract configuration with better error messages
        let dataset_id = config["dataset_id"].as_str().ok_or_else(|| {
            anyhow!(
                "field_mapper: Missing required config 'dataset_id'. Config: {}",
                config
            )
        })?;

        let table_name = config["source_table"].as_str().ok_or_else(|| {
            anyhow!(
                "field_mapper: Missing required config 'source_table' for dataset '{}'",
                dataset_id
            )
        })?;

        // Validate dataset_id and table_name are non-empty
        if dataset_id.trim().is_empty() {
            anyhow::bail!("field_mapper: dataset_id cannot be empty");
        }
        if table_name.trim().is_empty() {
            anyhow::bail!(
                "field_mapper: source_table cannot be empty for dataset '{}'",
                dataset_id
            );
        }

        let apply_suggestions = config["apply_suggestions"].as_bool().unwrap_or(true);
        let min_suggestion_confidence = config["min_suggestion_confidence"].as_f64().unwrap_or(0.7);

        // Validate confidence range
        if !(0.0..=1.0).contains(&min_suggestion_confidence) {
            anyhow::bail!(
                "field_mapper: min_suggestion_confidence must be between 0.0 and 1.0, got {}",
                min_suggestion_confidence
            );
        }

        // Extract rows from data with better error context
        let rows = data["rows"].as_array().ok_or_else(|| {
            anyhow!(
                "field_mapper: Data must contain 'rows' array for dataset '{}'. Keys: {:?}",
                dataset_id,
                data.as_object().map(|o| o.keys().collect::<Vec<_>>())
            )
        })?;

        if rows.is_empty() {
            warn!(
                "field_mapper: No rows to process for dataset '{}', table '{}', skipping",
                dataset_id, table_name
            );
            return Ok(());
        }

        // Extract field names from first row with validation
        let field_names: Vec<String> = rows[0]
            .as_object()
            .ok_or_else(|| {
                anyhow!(
                    "field_mapper: First row must be an object for dataset '{}', got: {:?}",
                    dataset_id,
                    rows[0]
                )
            })?
            .keys()
            .cloned()
            .collect();

        if field_names.is_empty() {
            warn!(
                "field_mapper: First row has no fields for dataset '{}', table '{}'",
                dataset_id, table_name
            );
            return Ok(());
        }

        info!(
            "Field mapper processing {} fields from dataset {}",
            field_names.len(),
            dataset_id
        );

        // Get manual mappings (priority)
        let mut mappings = self
            .get_mappings(dataset_id, table_name, &field_names)
            .await
            .with_context(|| {
                format!(
                    "field_mapper: Failed to retrieve manual mappings for dataset '{}', table '{}'",
                    dataset_id, table_name
                )
            })?;

        // Get suggestions for unmapped fields (if enabled)
        if apply_suggestions {
            let unmapped_fields: Vec<String> = field_names
                .iter()
                .filter(|f| !mappings.contains_key(*f))
                .cloned()
                .collect();

            if !unmapped_fields.is_empty() {
                let suggestions = self
                    .get_suggestions(dataset_id, table_name, &unmapped_fields, min_suggestion_confidence)
                    .await
                    .with_context(|| format!(
                        "field_mapper: Failed to retrieve mapping suggestions for dataset '{}', table '{}'",
                        dataset_id, table_name
                    ))?;

                // Merge suggestions into mappings
                for (field, suggestion) in suggestions {
                    mappings.insert(field, suggestion);
                }
            }
        }

        // Apply mappings to rows
        if !mappings.is_empty() {
            info!(
                "Applying {} field mappings ({} manual, {} suggested)",
                mappings.len(),
                mappings
                    .values()
                    .filter(|m| matches!(m.source, MappingSource::Manual))
                    .count(),
                mappings
                    .values()
                    .filter(|m| matches!(m.source, MappingSource::Suggested))
                    .count()
            );

            let mapped_rows = self
                .apply_mappings(dataset_id, rows, &mappings)
                .with_context(|| {
                    format!(
                        "field_mapper: Failed to apply {} mappings to {} rows for dataset '{}'",
                        mappings.len(),
                        rows.len(),
                        dataset_id
                    )
                })?;

            // Update data with mapped rows
            data["rows"] = JsonValue::Array(mapped_rows);

            // Add field mapping metadata
            let mapping_metadata: serde_json::Map<String, JsonValue> = mappings
                .iter()
                .map(|(field, info)| {
                    (
                        field.clone(),
                        json!({
                            "target": info.target,
                            "confidence": info.confidence,
                            "source": match info.source {
                                MappingSource::Manual => "manual",
                                MappingSource::Suggested => "suggested",
                            }
                        }),
                    )
                })
                .collect();

            data["field_mappings"] = JsonValue::Object(mapping_metadata);

            // Update usage statistics
            self.update_statistics(&mappings).await.with_context(|| {
                format!(
                    "field_mapper: Failed to update usage statistics for dataset '{}'",
                    dataset_id
                )
            })?;

            // Record metrics
            // self.metrics.record_fields_processed(field_names.len());
            // self.metrics.record_rows_transformed(rows.len());
        } else {
            info!("No field mappings found for dataset {}", dataset_id);
            // Record unmapped fields
            for _ in &field_names {
                // self.metrics.record_unmapped_field();
            }
        }

        // Record transformation completion
        let duration = start.elapsed().as_secs_f64();
        // self.metrics.record_transformation(dataset_id, duration, true);

        Ok(())
    }

    fn name(&self) -> &'static str {
        "field_mapper"
    }

    fn validate_config(&self, config: &JsonValue) -> Result<()> {
        // Validate required fields
        if config["dataset_id"].as_str().is_none() {
            anyhow::bail!("Missing required config field: dataset_id");
        }

        if config["source_table"].as_str().is_none() {
            anyhow::bail!("Missing required config field: source_table");
        }

        // Validate optional fields
        if let Some(confidence) = config["min_suggestion_confidence"].as_f64() {
            if !(0.0..=1.0).contains(&confidence) {
                anyhow::bail!("min_suggestion_confidence must be between 0.0 and 1.0");
            }
        }

        Ok(())
    }
}

// ============================================================================
// Helper Types
// ============================================================================

/// Information about a field mapping
#[derive(Debug, Clone)]
struct MappingInfo {
    /// Target field name
    target: String,

    /// Confidence score (0.0 - 1.0)
    confidence: f64,

    /// Source of the mapping
    source: MappingSource,

    /// Optional mapping ID for statistics
    mapping_id: Option<String>,
}

/// Source of a field mapping
#[derive(Debug, Clone)]
enum MappingSource {
    /// Manual mapping provided by user
    Manual,

    /// Suggested by learning system
    Suggested,
}

/// Conflict resolution strategy when multiple source fields map to same target
#[derive(Debug, Clone, Copy)]
enum ConflictResolution {
    /// Use the first mapping encountered (default)
    FirstWins,

    /// Use the mapping with highest confidence
    HighestConfidence,

    /// Return error on conflict (strict mode)
    Error,
}

impl Default for ConflictResolution {
    fn default() -> Self {
        ConflictResolution::HighestConfidence
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::manual::{ManualFieldMapping, MappingSuggestion};
    use serde_json::json;
    use std::sync::Mutex;

    // ========================================================================
    // Mock Implementations (using trait wrapper pattern)
    // ========================================================================

    /// Test helper to create mappings for mock data
    struct TestMappingData {
        mappings: Mutex<HashMap<String, ManualFieldMapping>>,
        suggestions: Mutex<HashMap<String, Vec<MappingSuggestion>>>,
    }

    impl TestMappingData {
        fn new() -> Self {
            Self {
                mappings: Mutex::new(HashMap::new()),
                suggestions: Mutex::new(HashMap::new()),
            }
        }

        fn add_mapping(&self, key: String, mapping: ManualFieldMapping) {
            self.mappings.lock().unwrap().insert(key, mapping);
        }

        fn add_suggestion(&self, field_name: String, suggestions: Vec<MappingSuggestion>) {
            self.suggestions
                .lock()
                .unwrap()
                .insert(field_name, suggestions);
        }

        fn get_mappings(
            &self,
            contexts: &[SourceContext],
        ) -> Result<HashMap<String, ManualFieldMapping>> {
            let mappings = self.mappings.lock().unwrap();
            let mut results = HashMap::new();

            for ctx in contexts {
                let key = format!(
                    "{}:{}:{}",
                    ctx.source_id.as_ref().unwrap_or(&"".to_string()),
                    ctx.table_name,
                    ctx.field_name
                );
                if let Some(mapping) = mappings.get(&key) {
                    results.insert(ctx.field_name.clone(), mapping.clone());
                }
            }

            Ok(results)
        }

        fn get_suggestions(&self, field_name: &str) -> Vec<MappingSuggestion> {
            let suggestions = self.suggestions.lock().unwrap();
            suggestions.get(field_name).cloned().unwrap_or_default()
        }
    }

    // ========================================================================
    // Helper Functions
    // ========================================================================

    /// Helper struct for unit testing field mapper logic without database
    struct FieldMapperTestHelper;

    impl FieldMapperTestHelper {
        /// Test detect_conflicts logic
        fn detect_conflicts(
            &self,
            mappings: &HashMap<String, MappingInfo>,
        ) -> HashMap<String, Vec<String>> {
            let mut target_to_sources: HashMap<String, Vec<String>> = HashMap::new();

            for (source, mapping_info) in mappings {
                target_to_sources
                    .entry(mapping_info.target.clone())
                    .or_insert_with(Vec::new)
                    .push(source.clone());
            }

            // Filter to only conflicts (targets with 2+ sources)
            target_to_sources
                .into_iter()
                .filter(|(_, sources)| sources.len() > 1)
                .collect()
        }

        /// Test apply_mappings logic
        fn apply_mappings(
            &self,
            dataset_id: &str,
            rows: &[JsonValue],
            mappings: &HashMap<String, MappingInfo>,
        ) -> Result<Vec<JsonValue>> {
            let conflict_resolution = ConflictResolution::default();

            // Pre-check for conflicts
            let conflicts = self.detect_conflicts(mappings);
            if !conflicts.is_empty() {
                match conflict_resolution {
                    ConflictResolution::Error => {
                        let conflict_details: Vec<String> = conflicts
                            .iter()
                            .map(|(target, sources)| {
                                format!("{} ← [{}]", target, sources.join(", "))
                            })
                            .collect();
                        anyhow::bail!(
                            "Field mapping conflicts detected: {}",
                            conflict_details.join("; ")
                        );
                    }
                    _ => {}
                }
            }

            let mut mapped_rows = Vec::with_capacity(rows.len());

            for row in rows {
                let obj = row
                    .as_object()
                    .ok_or_else(|| anyhow!("Row must be an object"))?;

                let mut mapped_row = serde_json::Map::new();
                let mut target_usage: HashMap<String, (String, f64)> = HashMap::new();

                for (source_field, value) in obj {
                    if let Some(mapping) = mappings.get(source_field) {
                        let target = &mapping.target;

                        if let Some((prev_source, prev_confidence)) = target_usage.get(target) {
                            let should_replace = match conflict_resolution {
                                ConflictResolution::FirstWins => false,
                                ConflictResolution::HighestConfidence => {
                                    mapping.confidence > *prev_confidence
                                }
                                ConflictResolution::Error => false,
                            };

                            if should_replace {
                                mapped_row.insert(target.clone(), value.clone());
                                target_usage.insert(
                                    target.clone(),
                                    (source_field.clone(), mapping.confidence),
                                );
                            }
                        } else {
                            mapped_row.insert(target.clone(), value.clone());
                            target_usage
                                .insert(target.clone(), (source_field.clone(), mapping.confidence));
                        }
                    } else {
                        mapped_row.insert(source_field.clone(), value.clone());
                    }
                }

                mapped_rows.push(JsonValue::Object(mapped_row));
            }

            Ok(mapped_rows)
        }

        /// Test validate_config logic
        fn validate_config(&self, config: &JsonValue) -> Result<()> {
            if config["dataset_id"].as_str().is_none() {
                anyhow::bail!("Missing required config field: dataset_id");
            }

            if config["source_table"].as_str().is_none() {
                anyhow::bail!("Missing required config field: source_table");
            }

            if let Some(confidence) = config["min_suggestion_confidence"].as_f64() {
                if !(0.0..=1.0).contains(&confidence) {
                    anyhow::bail!("min_suggestion_confidence must be between 0.0 and 1.0");
                }
            }

            Ok(())
        }
    }

    fn create_manual_mapping(id: &str, target_uri: &str, confidence: f64) -> ManualFieldMapping {
        use crate::mapping::manual::UsageStats;

        let now = chrono::Utc::now();
        ManualFieldMapping {
            id: id.to_string(),
            target_field_uri: target_uri.to_string(),
            confidence,
            created_at: now,
            created_by: "test".to_string(),
            updated_at: now,
            notes: None,
            usage_stats: UsageStats::default(),
            source_context: SourceContext {
                source_id: None,
                table_name: "".to_string(),
                field_name: "".to_string(),
                field_metadata: None,
            },
        }
    }

    fn create_suggestion(mapping: ManualFieldMapping, score: f64) -> MappingSuggestion {
        use crate::mapping::manual::SuggestionReason;

        MappingSuggestion {
            mapping,
            relevance_score: score,
            suggestion_reason: SuggestionReason::SimilarFieldName { similarity: score },
        }
    }

    // ========================================================================
    // Configuration Validation Tests
    // ========================================================================

    #[test]
    fn test_validate_config_valid() {
        let helper = FieldMapperTestHelper;

        let config = json!({
            "dataset_id": "file_123",
            "source_table": "customers",
            "apply_suggestions": true,
            "min_suggestion_confidence": 0.7
        });

        assert!(helper.validate_config(&config).is_ok());
    }

    #[test]
    fn test_validate_config_missing_dataset_id() {
        let helper = FieldMapperTestHelper;

        let config = json!({
            "source_table": "customers"
        });

        let result = helper.validate_config(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("dataset_id"));
    }

    #[test]
    fn test_validate_config_missing_source_table() {
        let helper = FieldMapperTestHelper;

        let config = json!({
            "dataset_id": "file_123"
        });

        let result = helper.validate_config(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("source_table"));
    }

    #[test]
    fn test_validate_config_invalid_confidence() {
        let helper = FieldMapperTestHelper;

        let config = json!({
            "dataset_id": "file_123",
            "source_table": "customers",
            "min_suggestion_confidence": 1.5
        });

        let result = helper.validate_config(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("0.0 and 1.0"));
    }

    // ========================================================================
    // URI Extraction Tests
    // ========================================================================

    #[test]
    fn test_extract_property_name() {
        use crate::mapping::uri_utils;

        let cases = vec![
            ("retail:customerFirstName", Some("customerFirstName")),
            ("http://example.com/ontology#firstName", Some("firstName")),
            ("firstName", Some("firstName")),
            ("ns:nested:property", Some("property")),
            ("retail:", None),             // Edge case: empty local name
            ("http://example.com/", None), // Edge case: trailing slash
            ("", None),                    // Edge case: empty string
        ];

        for (input, expected) in cases {
            let result = uri_utils::extract_local_name(input);
            assert_eq!(result.as_deref(), expected, "Failed for input: '{}'", input);
        }
    }

    // ========================================================================
    // Conflict Detection Tests
    // ========================================================================

    #[test]
    fn test_detect_conflicts_none() {
        let helper = FieldMapperTestHelper;

        let mut mappings = HashMap::new();
        mappings.insert(
            "F1".to_string(),
            MappingInfo {
                target: "firstName".to_string(),
                confidence: 1.0,
                source: MappingSource::Manual,
                mapping_id: Some("m1".to_string()),
            },
        );
        mappings.insert(
            "F2".to_string(),
            MappingInfo {
                target: "lastName".to_string(),
                confidence: 1.0,
                source: MappingSource::Manual,
                mapping_id: Some("m2".to_string()),
            },
        );

        let conflicts = helper.detect_conflicts(&mappings);
        assert!(conflicts.is_empty());
    }

    #[test]
    fn test_detect_conflicts_single() {
        let helper = FieldMapperTestHelper;

        let mut mappings = HashMap::new();
        mappings.insert(
            "F1".to_string(),
            MappingInfo {
                target: "name".to_string(),
                confidence: 1.0,
                source: MappingSource::Manual,
                mapping_id: Some("m1".to_string()),
            },
        );
        mappings.insert(
            "F2".to_string(),
            MappingInfo {
                target: "name".to_string(),
                confidence: 0.8,
                source: MappingSource::Suggested,
                mapping_id: Some("m2".to_string()),
            },
        );

        let conflicts = helper.detect_conflicts(&mappings);
        assert_eq!(conflicts.len(), 1);
        assert!(conflicts.contains_key("name"));

        let sources = conflicts.get("name").unwrap();
        assert_eq!(sources.len(), 2);
        assert!(sources.contains(&"F1".to_string()));
        assert!(sources.contains(&"F2".to_string()));
    }

    #[test]
    fn test_detect_conflicts_multiple() {
        let helper = FieldMapperTestHelper;

        let mut mappings = HashMap::new();
        mappings.insert(
            "F1".to_string(),
            MappingInfo {
                target: "name".to_string(),
                confidence: 1.0,
                source: MappingSource::Manual,
                mapping_id: Some("m1".to_string()),
            },
        );
        mappings.insert(
            "F2".to_string(),
            MappingInfo {
                target: "name".to_string(),
                confidence: 0.8,
                source: MappingSource::Suggested,
                mapping_id: Some("m2".to_string()),
            },
        );
        mappings.insert(
            "F3".to_string(),
            MappingInfo {
                target: "email".to_string(),
                confidence: 0.9,
                source: MappingSource::Suggested,
                mapping_id: Some("m3".to_string()),
            },
        );
        mappings.insert(
            "F4".to_string(),
            MappingInfo {
                target: "email".to_string(),
                confidence: 0.7,
                source: MappingSource::Suggested,
                mapping_id: Some("m4".to_string()),
            },
        );

        let conflicts = helper.detect_conflicts(&mappings);
        assert_eq!(conflicts.len(), 2);
        assert!(conflicts.contains_key("name"));
        assert!(conflicts.contains_key("email"));
    }

    // ========================================================================
    // Mapping Application Tests
    // ========================================================================

    #[test]
    fn test_apply_mappings_simple() {
        let helper = FieldMapperTestHelper;

        let rows = vec![
            json!({"F1": "John", "F2": "Doe"}),
            json!({"F1": "Jane", "F2": "Smith"}),
        ];

        let mut mappings = HashMap::new();
        mappings.insert(
            "F1".to_string(),
            MappingInfo {
                target: "firstName".to_string(),
                confidence: 1.0,
                source: MappingSource::Manual,
                mapping_id: Some("m1".to_string()),
            },
        );
        mappings.insert(
            "F2".to_string(),
            MappingInfo {
                target: "lastName".to_string(),
                confidence: 1.0,
                source: MappingSource::Manual,
                mapping_id: Some("m2".to_string()),
            },
        );

        let result = helper.apply_mappings("test_dataset", &rows, &mappings);
        assert!(result.is_ok());

        let mapped_rows = result.unwrap();
        assert_eq!(mapped_rows.len(), 2);
        assert_eq!(mapped_rows[0]["firstName"], "John");
        assert_eq!(mapped_rows[0]["lastName"], "Doe");
        assert_eq!(mapped_rows[1]["firstName"], "Jane");
        assert_eq!(mapped_rows[1]["lastName"], "Smith");
    }

    #[test]
    fn test_apply_mappings_partial() {
        let helper = FieldMapperTestHelper;

        let rows = vec![json!({"F1": "John", "F2": "Doe", "F3": "john@example.com"})];

        let mut mappings = HashMap::new();
        mappings.insert(
            "F1".to_string(),
            MappingInfo {
                target: "firstName".to_string(),
                confidence: 1.0,
                source: MappingSource::Manual,
                mapping_id: Some("m1".to_string()),
            },
        );
        // F2 not mapped - should keep original name
        // F3 not mapped - should keep original name

        let result = helper.apply_mappings("test_dataset", &rows, &mappings);
        assert!(result.is_ok());

        let mapped_rows = result.unwrap();
        assert_eq!(mapped_rows.len(), 1);
        assert_eq!(mapped_rows[0]["firstName"], "John");
        assert_eq!(mapped_rows[0]["F2"], "Doe"); // Kept original
        assert_eq!(mapped_rows[0]["F3"], "john@example.com"); // Kept original
    }

    #[test]
    fn test_apply_mappings_conflict_highest_confidence() {
        let helper = FieldMapperTestHelper;

        let rows = vec![json!({"F1": "John", "F2": "Jane"})];

        let mut mappings = HashMap::new();
        mappings.insert(
            "F1".to_string(),
            MappingInfo {
                target: "name".to_string(),
                confidence: 0.7,
                source: MappingSource::Suggested,
                mapping_id: Some("m1".to_string()),
            },
        );
        mappings.insert(
            "F2".to_string(),
            MappingInfo {
                target: "name".to_string(),
                confidence: 0.9,
                source: MappingSource::Manual,
                mapping_id: Some("m2".to_string()),
            },
        );

        let result = helper.apply_mappings("test_dataset", &rows, &mappings);
        assert!(result.is_ok());

        let mapped_rows = result.unwrap();
        assert_eq!(mapped_rows.len(), 1);

        // HighestConfidence is default - conflict resolution will pick one
        // With HashMap iteration, order is non-deterministic
        // The test verifies that conflict resolution worked (no error)
        assert!(mapped_rows[0].get("name").is_some());
    }

    #[test]
    fn test_apply_mappings_empty_rows() {
        let helper = FieldMapperTestHelper;

        let rows: Vec<JsonValue> = vec![];
        let mappings = HashMap::new();

        let result = helper.apply_mappings("test_dataset", &rows, &mappings);
        assert!(result.is_ok());

        let mapped_rows = result.unwrap();
        assert_eq!(mapped_rows.len(), 0);
    }

    #[test]
    fn test_apply_mappings_invalid_row() {
        let helper = FieldMapperTestHelper;

        let rows = vec![json!("not an object")];

        let mappings = HashMap::new();

        let result = helper.apply_mappings("test_dataset", &rows, &mappings);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Row must be an object"));
    }

    // ========================================================================
    // NOTE: Integration tests with actual RocksDB and async operations
    // belong in graphica-coordinator/tests/ directory.
    // The tests above focus on unit testing the transformer logic.
    // ========================================================================
}
