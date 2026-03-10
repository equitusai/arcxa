// Manual Mapping Storage - RDF with RocksDB Indexes
use super::types::*;
use crate::governance::rdf_store::{NamedGraph, RdfStore};
use crate::mapping::similarity::StringSimilarity;
use anyhow::{Context, Result};
use rocksdb::{ColumnFamilyDescriptor, Options, WriteBatch, DB};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Column families for manual mappings
const CF_MAPPING_DATA: &str = "mapping_data"; // id -> ManualFieldMapping
const CF_SOURCE_INDEX: &str = "source_to_mapping"; // source_context -> mapping_id
const CF_TARGET_INDEX: &str = "target_to_mappings"; // target_uri -> [mapping_ids]
const CF_PATTERN_INDEX: &str = "pattern_to_mappings"; // pattern -> [mapping_ids]
const CF_USER_INDEX: &str = "user_to_mappings"; // user -> [mapping_ids]
const CF_FIELD_TRIGRAM_INDEX: &str = "field_trigram_to_mappings"; // trigram -> [mapping_ids]

/// Manual mapping store with RDF primary storage and RocksDB indexes
pub struct ManualMappingStore {
    /// RDF store for primary storage (truth)
    rdf_store: Arc<dyn RdfStore>,

    /// RocksDB for fast indexes
    rocksdb: Arc<DB>,

    /// Named graph for manual mappings
    graph_uri: String,

    /// In-memory cache for hot mappings
    cache: Arc<RwLock<HashMap<String, ManualFieldMapping>>>,

    /// Prometheus metrics (optional)
    metrics: super::metrics::OptionalMetrics,
}

impl ManualMappingStore {
    pub fn new(rdf_store: Arc<dyn RdfStore>, rocksdb_path: &str) -> Result<Self> {
        // Open RocksDB with column families
        let cf_opts = Options::default();

        let cfs = vec![
            ColumnFamilyDescriptor::new(CF_MAPPING_DATA, cf_opts.clone()),
            ColumnFamilyDescriptor::new(CF_SOURCE_INDEX, cf_opts.clone()),
            ColumnFamilyDescriptor::new(CF_TARGET_INDEX, cf_opts.clone()),
            ColumnFamilyDescriptor::new(CF_PATTERN_INDEX, cf_opts.clone()),
            ColumnFamilyDescriptor::new(CF_USER_INDEX, cf_opts.clone()),
            ColumnFamilyDescriptor::new(CF_FIELD_TRIGRAM_INDEX, cf_opts.clone()),
        ];

        let mut db_opts = Options::default();
        db_opts.create_if_missing(true);
        db_opts.create_missing_column_families(true);

        let db = DB::open_cf_descriptors(&db_opts, rocksdb_path, cfs)?;

        Ok(Self {
            rdf_store,
            rocksdb: Arc::new(db),
            graph_uri: "http://graphica.io/graphs/manual-mappings".to_string(),
            cache: Arc::new(RwLock::new(HashMap::new())),
            metrics: super::metrics::OptionalMetrics::none(),
        })
    }

    /// Create store with metrics enabled
    pub fn with_metrics(
        rdf_store: Arc<dyn RdfStore>,
        rocksdb_path: &str,
        metrics: Arc<super::metrics::ManualMappingMetrics>,
    ) -> Result<Self> {
        let mut store = Self::new(rdf_store, rocksdb_path)?;
        store.metrics = super::metrics::OptionalMetrics::new(Some(metrics));
        Ok(store)
    }

    /// Add a mapping ID to a multi-value index (handles duplicates)
    fn add_to_index(
        &self,
        cf_handle: &rocksdb::ColumnFamily,
        key: &[u8],
        mapping_id: &str,
    ) -> Result<()> {
        // Get existing IDs
        let mut ids: Vec<String> = if let Some(bytes) = self.rocksdb.get_cf(cf_handle, key)? {
            bincode::deserialize(&bytes)?
        } else {
            Vec::new()
        };

        // Add new ID if not already present (avoid duplicates)
        if !ids.contains(&mapping_id.to_string()) {
            ids.push(mapping_id.to_string());
            let serialized = bincode::serialize(&ids)?;
            self.rocksdb.put_cf(cf_handle, key, &serialized)?;
        }

        Ok(())
    }

    /// Get all mapping IDs from a multi-value index
    fn get_from_index(&self, cf_handle: &rocksdb::ColumnFamily, key: &[u8]) -> Result<Vec<String>> {
        if let Some(bytes) = self.rocksdb.get_cf(cf_handle, key)? {
            Ok(bincode::deserialize(&bytes)?)
        } else {
            Ok(Vec::new())
        }
    }

    /// Remove a mapping ID from a multi-value index
    fn remove_from_index(
        &self,
        cf_handle: &rocksdb::ColumnFamily,
        key: &[u8],
        mapping_id: &str,
    ) -> Result<()> {
        // Get existing IDs
        let mut ids: Vec<String> = if let Some(bytes) = self.rocksdb.get_cf(cf_handle, key)? {
            bincode::deserialize(&bytes)?
        } else {
            Vec::new()
        };

        // Remove the mapping ID
        ids.retain(|id| id != mapping_id);

        // Write back (or delete if empty)
        if ids.is_empty() {
            self.rocksdb.delete_cf(cf_handle, key)?;
        } else {
            let serialized = bincode::serialize(&ids)?;
            self.rocksdb.put_cf(cf_handle, key, &serialized)?;
        }

        Ok(())
    }

    /// Store a manual mapping
    pub async fn store_mapping(&self, mapping: ManualFieldMapping) -> Result<()> {
        let start = std::time::Instant::now();

        // 1. Store in RDF (source of truth)
        let triples = mapping.to_rdf_triples();
        let graph = NamedGraph::new(&self.graph_uri);
        self.rdf_store.insert_triples(triples, Some(&graph))?;

        // 2. Update RocksDB indexes for fast lookup
        let mut batch = WriteBatch::default();

        // Serialize mapping for storage
        let mapping_bytes = bincode::serialize(&mapping)?;

        // Primary data
        let cf_data = self
            .rocksdb
            .cf_handle(CF_MAPPING_DATA)
            .context("CF_MAPPING_DATA not found")?;
        batch.put_cf(cf_data, mapping.id.as_bytes(), &mapping_bytes);

        // Source index
        let cf_source = self
            .rocksdb
            .cf_handle(CF_SOURCE_INDEX)
            .context("CF_SOURCE_INDEX not found")?;
        let source_key = MappingIndexKeys::source_to_mapping(&mapping.source_context);
        batch.put_cf(cf_source, &source_key, mapping.id.as_bytes());

        // Write primary data and source index via batch
        self.rocksdb.write(batch)?;

        // Target index - use helper to append to list
        let cf_target = self
            .rocksdb
            .cf_handle(CF_TARGET_INDEX)
            .context("CF_TARGET_INDEX not found")?;
        let target_key = MappingIndexKeys::target_to_mappings(&mapping.target_field_uri);
        self.add_to_index(cf_target, &target_key, &mapping.id)?;

        // Pattern index (if detected) - use helper to append to list
        if let Some(ref meta) = mapping.source_context.field_metadata {
            if let Some(ref pattern) = meta.detected_pattern {
                let cf_pattern = self
                    .rocksdb
                    .cf_handle(CF_PATTERN_INDEX)
                    .context("CF_PATTERN_INDEX not found")?;
                let pattern_key = MappingIndexKeys::pattern_to_mappings(pattern);
                self.add_to_index(cf_pattern, &pattern_key, &mapping.id)?;
            }
        }

        // User index - use helper to append to list
        let cf_user = self
            .rocksdb
            .cf_handle(CF_USER_INDEX)
            .context("CF_USER_INDEX not found")?;
        let user_key = MappingIndexKeys::user_to_mappings(&mapping.created_by);
        self.add_to_index(cf_user, &user_key, &mapping.id)?;

        // Field trigram index - for optimized fuzzy search
        let cf_trigram = self
            .rocksdb
            .cf_handle(CF_FIELD_TRIGRAM_INDEX)
            .context("CF_FIELD_TRIGRAM_INDEX not found")?;
        let field_lower = mapping.source_context.field_name.to_lowercase();
        let trigrams = StringSimilarity::generate_ngrams(&field_lower, 3);
        for trigram in trigrams {
            self.add_to_index(cf_trigram, trigram.as_bytes(), &mapping.id)?;
        }

        // 3. Update cache
        let mut cache = self.cache.write().await;
        cache.insert(mapping.id.clone(), mapping);

        // Record metrics
        let duration = start.elapsed().as_secs_f64();
        self.metrics.record_operation("store", duration);
        self.metrics.set_cache_size(cache.len() as i64);

        Ok(())
    }

    /// Find mapping by source context (exact match)
    pub async fn find_by_source(
        &self,
        context: &SourceContext,
    ) -> Result<Option<ManualFieldMapping>> {
        let cf_source = self
            .rocksdb
            .cf_handle(CF_SOURCE_INDEX)
            .context("CF_SOURCE_INDEX not found")?;

        let source_key = MappingIndexKeys::source_to_mapping(context);

        if let Some(mapping_id_bytes) = self.rocksdb.get_cf(cf_source, &source_key)? {
            let mapping_id = String::from_utf8(mapping_id_bytes)?;
            return self.get_mapping(&mapping_id).await;
        }

        Ok(None)
    }

    /// Find mappings for multiple source contexts in batch (optimized for N+1 prevention)
    ///
    /// This method efficiently looks up mappings for multiple source contexts at once,
    /// reducing the overhead of multiple sequential async calls.
    ///
    /// # Arguments
    /// * `contexts` - Slice of source contexts to look up
    ///
    /// # Returns
    /// HashMap mapping field_name to ManualFieldMapping for found mappings only
    ///
    /// # Performance
    /// - Uses RocksDB multi_get for batch index lookups (future optimization)
    /// - Batch caching reduces lock contention
    /// - Only returns found mappings (sparse result set)
    pub async fn find_by_source_batch(
        &self,
        contexts: &[SourceContext],
    ) -> Result<HashMap<String, ManualFieldMapping>> {
        let start = std::time::Instant::now();

        // Phase 1: Lookup all mapping IDs from index (no await, cf_handle doesn't escape)
        let mapping_ids_with_fields: Vec<(String, String)> = {
            let cf_source = self
                .rocksdb
                .cf_handle(CF_SOURCE_INDEX)
                .context("CF_SOURCE_INDEX not found")?;

            let mut ids = Vec::new();
            for ctx in contexts {
                let source_key = MappingIndexKeys::source_to_mapping(ctx);
                if let Some(mapping_id_bytes) = self.rocksdb.get_cf(cf_source, &source_key)? {
                    let mapping_id = String::from_utf8(mapping_id_bytes)?;
                    ids.push((mapping_id, ctx.field_name.clone()));
                }
            }
            ids
        }; // cf_source dropped here, safe to await below

        // Phase 2: Fetch all mappings by ID (async calls)
        let mut result = HashMap::new();
        for (mapping_id, field_name) in mapping_ids_with_fields {
            if let Some(mapping) = self.get_mapping(&mapping_id).await? {
                result.insert(field_name, mapping);
            }
        }

        let duration = start.elapsed().as_secs_f64();
        self.metrics
            .record_operation("find_by_source_batch", duration);
        self.metrics.record_query("by_source_batch", result.len());

        debug!(
            "Batch lookup: {} contexts -> {} mappings found ({:.2}ms)",
            contexts.len(),
            result.len(),
            duration * 1000.0
        );

        Ok(result)
    }

    /// Get mapping by ID
    pub async fn get_mapping(&self, id: &str) -> Result<Option<ManualFieldMapping>> {
        let start = std::time::Instant::now();

        // Check cache first
        {
            let cache = self.cache.read().await;
            if let Some(mapping) = cache.get(id) {
                self.metrics.record_cache_hit();
                let duration = start.elapsed().as_secs_f64();
                self.metrics.record_operation("get", duration);
                return Ok(Some(mapping.clone()));
            }
        }

        // Cache miss
        self.metrics.record_cache_miss();

        // Load from RocksDB
        let cf_data = self
            .rocksdb
            .cf_handle(CF_MAPPING_DATA)
            .context("CF_MAPPING_DATA not found")?;

        if let Some(bytes) = self.rocksdb.get_cf(cf_data, id.as_bytes())? {
            let mapping: ManualFieldMapping = bincode::deserialize(&bytes)?;

            // Update cache
            let mut cache = self.cache.write().await;
            cache.insert(id.to_string(), mapping.clone());
            self.metrics.set_cache_size(cache.len() as i64);

            let duration = start.elapsed().as_secs_f64();
            self.metrics.record_operation("get", duration);

            return Ok(Some(mapping));
        }

        let duration = start.elapsed().as_secs_f64();
        self.metrics.record_operation("get", duration);

        Ok(None)
    }

    /// Delete a manual mapping
    pub async fn delete_mapping(&self, id: &str) -> Result<bool> {
        let start = std::time::Instant::now();

        // First, retrieve the mapping to get all index keys
        let mapping = match self.get_mapping(id).await? {
            Some(m) => m,
            None => {
                info!("Mapping {} not found, nothing to delete", id);
                return Ok(false);
            }
        };

        info!("Deleting mapping: {}", id);

        // 1. Remove from RocksDB indexes
        let mut batch = WriteBatch::default();

        // Remove from primary data
        let cf_data = self
            .rocksdb
            .cf_handle(CF_MAPPING_DATA)
            .context("CF_MAPPING_DATA not found")?;
        batch.delete_cf(cf_data, id.as_bytes());

        // Remove from source index
        let cf_source = self
            .rocksdb
            .cf_handle(CF_SOURCE_INDEX)
            .context("CF_SOURCE_INDEX not found")?;
        let source_key = MappingIndexKeys::source_to_mapping(&mapping.source_context);
        batch.delete_cf(cf_source, &source_key);

        // Write batch for primary data and source index
        self.rocksdb.write(batch)?;

        // Remove from target index (multi-value)
        let cf_target = self
            .rocksdb
            .cf_handle(CF_TARGET_INDEX)
            .context("CF_TARGET_INDEX not found")?;
        let target_key = MappingIndexKeys::target_to_mappings(&mapping.target_field_uri);
        self.remove_from_index(cf_target, &target_key, id)?;

        // Remove from pattern index if present (multi-value)
        if let Some(ref meta) = mapping.source_context.field_metadata {
            if let Some(ref pattern) = meta.detected_pattern {
                let cf_pattern = self
                    .rocksdb
                    .cf_handle(CF_PATTERN_INDEX)
                    .context("CF_PATTERN_INDEX not found")?;
                let pattern_key = MappingIndexKeys::pattern_to_mappings(pattern);
                self.remove_from_index(cf_pattern, &pattern_key, id)?;
            }
        }

        // Remove from user index (multi-value)
        let cf_user = self
            .rocksdb
            .cf_handle(CF_USER_INDEX)
            .context("CF_USER_INDEX not found")?;
        let user_key = MappingIndexKeys::user_to_mappings(&mapping.created_by);
        self.remove_from_index(cf_user, &user_key, id)?;

        // Remove from field trigram index (multi-value)
        let cf_trigram = self
            .rocksdb
            .cf_handle(CF_FIELD_TRIGRAM_INDEX)
            .context("CF_FIELD_TRIGRAM_INDEX not found")?;
        let field_lower = mapping.source_context.field_name.to_lowercase();
        let trigrams = StringSimilarity::generate_ngrams(&field_lower, 3);
        for trigram in trigrams {
            self.remove_from_index(cf_trigram, trigram.as_bytes(), id)?;
        }

        // 2. Remove from RDF store (clear all triples for this mapping)
        let mapping_uri = format!("gph:mapping/{}", id);
        let graph = NamedGraph::new(&self.graph_uri);

        // Note: RdfStore trait doesn't have delete_triples_by_subject yet
        // For now, we'll clear the entire graph and re-insert remaining mappings
        // TODO: Add delete_triples_by_subject to RdfStore trait
        debug!(
            "RDF deletion for {} pending (requires RdfStore::delete_triples_by_subject)",
            mapping_uri
        );

        // 3. Remove from cache
        let mut cache = self.cache.write().await;
        cache.remove(id);
        self.metrics.set_cache_size(cache.len() as i64);

        info!("Successfully deleted mapping: {}", id);

        // Record metrics
        let duration = start.elapsed().as_secs_f64();
        self.metrics.record_operation("delete", duration);

        Ok(true)
    }

    /// Find similar mappings for auto-suggestion
    pub async fn find_similar_mappings(
        &self,
        context: &SourceContext,
        limit: usize,
    ) -> Result<Vec<MappingSuggestion>> {
        let mut suggestions = Vec::new();

        // 1. Check exact field name match
        if let Some(exact) = self.find_by_source(context).await? {
            suggestions.push(MappingSuggestion {
                relevance_score: 1.0,
                suggestion_reason: SuggestionReason::ExactFieldMatch {
                    previous_source: format!("{:?}", exact.source_context),
                },
                mapping: exact,
            });
        }

        // 2. Check similar field names (fuzzy match)
        let similar = self.find_by_field_pattern(&context.field_name, 0.7).await?;
        for (mapping, similarity) in similar.into_iter().take(limit - suggestions.len()) {
            suggestions.push(MappingSuggestion {
                relevance_score: similarity,
                suggestion_reason: SuggestionReason::SimilarFieldName { similarity },
                mapping,
            });
        }

        // 3. Check by data pattern if available
        if let Some(ref meta) = context.field_metadata {
            if let Some(ref pattern) = meta.detected_pattern {
                let pattern_matches = self.find_by_pattern(pattern).await?;
                for mapping in pattern_matches.into_iter().take(limit - suggestions.len()) {
                    // Calculate relevance based on usage
                    let relevance = (mapping.usage_stats.apply_count as f64 / 100.0).min(1.0);
                    suggestions.push(MappingSuggestion {
                        relevance_score: relevance,
                        suggestion_reason: SuggestionReason::FrequentPattern {
                            usage_count: mapping.usage_stats.apply_count,
                        },
                        mapping,
                    });
                }
            }
        }

        // Sort by relevance
        suggestions.sort_by(|a, b| b.relevance_score.partial_cmp(&a.relevance_score).unwrap());

        let final_suggestions: Vec<_> = suggestions.into_iter().take(limit).collect();

        // Record suggestion metrics
        for suggestion in &final_suggestions {
            let reason = match &suggestion.suggestion_reason {
                SuggestionReason::ExactFieldMatch { .. } => "exact_match",
                SuggestionReason::SimilarFieldName { .. } => "similar_field",
                SuggestionReason::SimilarDataProfile { .. } => "similar_profile",
                SuggestionReason::FrequentPattern { .. } => "frequent_pattern",
                SuggestionReason::MLModel { .. } => "ml_model",
            };
            self.metrics
                .record_suggestion(reason, suggestion.relevance_score);
        }

        self.metrics
            .record_query("suggest", final_suggestions.len());

        Ok(final_suggestions)
    }

    /// Find mappings by field name pattern (fuzzy match with trigram index optimization)
    async fn find_by_field_pattern(
        &self,
        field_name: &str,
        min_similarity: f64,
    ) -> Result<Vec<(ManualFieldMapping, f64)>> {
        let field_lower = field_name.to_lowercase();

        // Generate trigrams for the query field
        let query_trigrams = StringSimilarity::generate_ngrams(&field_lower, 3);

        // If no trigrams (field name too short), fallback to full scan
        if query_trigrams.is_empty() {
            return self
                .find_by_field_pattern_full_scan(&field_lower, min_similarity)
                .await;
        }

        // Use trigram index to find candidate mappings
        let cf_trigram = self
            .rocksdb
            .cf_handle(CF_FIELD_TRIGRAM_INDEX)
            .context("CF_FIELD_TRIGRAM_INDEX not found")?;

        let mut candidate_ids: HashSet<String> = HashSet::new();

        // For each trigram, get all mapping IDs that contain it
        for trigram in &query_trigrams {
            if let Some(ids_bytes) = self.rocksdb.get_cf(cf_trigram, trigram.as_bytes())? {
                let ids: Vec<String> = bincode::deserialize(&ids_bytes)?;
                candidate_ids.extend(ids);
            }
        }

        // If no candidates found, return empty
        if candidate_ids.is_empty() {
            return Ok(Vec::new());
        }

        // Load candidates and calculate actual similarity
        let mut results = Vec::new();
        for id in candidate_ids {
            if let Some(mapping) = self.get_mapping(&id).await? {
                let mapping_field_lower = mapping.source_context.field_name.to_lowercase();

                // Exact match gets confidence 1.0
                let similarity = if field_lower == mapping_field_lower {
                    1.0
                } else {
                    strsim::jaro_winkler(&field_lower, &mapping_field_lower)
                };

                if similarity >= min_similarity {
                    results.push((mapping, similarity));
                }
            }
        }

        Ok(results)
    }

    /// Fallback: full scan when trigram optimization can't be used
    async fn find_by_field_pattern_full_scan(
        &self,
        field_name_lower: &str,
        min_similarity: f64,
    ) -> Result<Vec<(ManualFieldMapping, f64)>> {
        let mut results = Vec::new();

        let cf_data = self
            .rocksdb
            .cf_handle(CF_MAPPING_DATA)
            .context("CF_MAPPING_DATA not found")?;

        let iter = self
            .rocksdb
            .iterator_cf(cf_data, rocksdb::IteratorMode::Start);

        for item in iter {
            let (_, value) = item?;
            let mapping: ManualFieldMapping = bincode::deserialize(&value)?;

            let mapping_field_lower = mapping.source_context.field_name.to_lowercase();

            // Exact match gets confidence 1.0
            let similarity = if field_name_lower == mapping_field_lower {
                1.0
            } else {
                strsim::jaro_winkler(field_name_lower, &mapping_field_lower)
            };

            if similarity >= min_similarity {
                results.push((mapping, similarity));
            }
        }

        Ok(results)
    }

    /// Find mappings by data pattern
    async fn find_by_pattern(&self, pattern: &str) -> Result<Vec<ManualFieldMapping>> {
        let cf_pattern = self
            .rocksdb
            .cf_handle(CF_PATTERN_INDEX)
            .context("CF_PATTERN_INDEX not found")?;

        let pattern_key = MappingIndexKeys::pattern_to_mappings(pattern);

        // Get mapping IDs from index using helper
        let ids = self.get_from_index(cf_pattern, &pattern_key)?;

        let mut mappings = Vec::new();
        for id in ids {
            if let Some(mapping) = self.get_mapping(&id).await? {
                mappings.push(mapping);
            }
        }

        Ok(mappings)
    }

    /// Update usage statistics
    pub async fn update_usage_stats(
        &self,
        mapping_id: &str,
        stat_type: UsageStatType,
    ) -> Result<()> {
        if let Some(mut mapping) = self.get_mapping(mapping_id).await? {
            match stat_type {
                UsageStatType::Applied => {
                    mapping.usage_stats.apply_count += 1;
                    mapping.usage_stats.last_used = Some(chrono::Utc::now());
                }
                UsageStatType::Accepted => {
                    mapping.usage_stats.accept_count += 1;
                }
                UsageStatType::Rejected => {
                    mapping.usage_stats.reject_count += 1;
                }
            }

            // Re-store with updated stats
            self.store_mapping(mapping).await?;
        }

        Ok(())
    }

    /// Bulk import mappings
    /// Validate a single mapping for import
    fn validate_mapping(&self, mapping: &ManualFieldMapping) -> Option<ValidationError> {
        // Check ID
        if mapping.id.is_empty() {
            return Some(ValidationError {
                mapping_id: mapping.id.clone(),
                error_type: ValidationErrorType::InvalidId,
                message: "Mapping ID cannot be empty".to_string(),
            });
        }

        // Check source context
        if mapping.source_context.table_name.is_empty()
            || mapping.source_context.field_name.is_empty()
        {
            return Some(ValidationError {
                mapping_id: mapping.id.clone(),
                error_type: ValidationErrorType::InvalidSourceContext,
                message: "Source context must have table_name and field_name".to_string(),
            });
        }

        // Check target URI
        if mapping.target_field_uri.is_empty() {
            return Some(ValidationError {
                mapping_id: mapping.id.clone(),
                error_type: ValidationErrorType::InvalidTargetUri,
                message: "Target field URI cannot be empty".to_string(),
            });
        }

        // Check confidence
        if (mapping.confidence - 1.0).abs() > 0.001 {
            return Some(ValidationError {
                mapping_id: mapping.id.clone(),
                error_type: ValidationErrorType::InvalidConfidence,
                message: format!(
                    "Manual mappings must have confidence 1.0, got {}",
                    mapping.confidence
                ),
            });
        }

        None
    }

    /// Enhanced bulk import with validation and conflict resolution
    pub async fn bulk_import_with_options(
        &self,
        import: MappingImportExport,
        options: ImportOptions,
    ) -> Result<ImportResult> {
        let start = std::time::Instant::now();

        let mut result = ImportResult::default();
        result.total = import.mappings.len();

        for mapping in import.mappings {
            // Step 1: Validate structure if enabled
            if options.validate_structure {
                if let Some(error) = self.validate_mapping(&mapping) {
                    result.failed += 1;
                    result.errors.push(error);
                    continue;
                }
            }

            // Step 2: Check for conflicts
            let id_exists = self.get_mapping(&mapping.id).await?.is_some();
            let source_exists = self
                .find_by_source(&mapping.source_context)
                .await?
                .is_some();

            let has_conflict = if options.check_duplicates {
                id_exists || source_exists
            } else {
                id_exists
            };

            if has_conflict {
                match options.conflict_resolution {
                    ConflictResolution::Skip => {
                        result.skipped += 1;
                        result.skipped_ids.push(mapping.id.clone());
                        debug!("Skipping conflicting mapping: {}", mapping.id);
                        continue;
                    }
                    ConflictResolution::Fail => {
                        let error_type = if id_exists {
                            ValidationErrorType::DuplicateId
                        } else {
                            ValidationErrorType::DuplicateSourceContext
                        };
                        result.failed += 1;
                        result.errors.push(ValidationError {
                            mapping_id: mapping.id.clone(),
                            error_type,
                            message: "Conflict detected with existing mapping".to_string(),
                        });
                        // Fail entire import
                        return Ok(result);
                    }
                    ConflictResolution::Overwrite => {
                        // Delete existing and continue with import
                        if id_exists {
                            self.delete_mapping(&mapping.id).await?;
                        }
                    }
                    ConflictResolution::Merge => {
                        // Merge usage stats if mapping exists
                        if let Some(existing) = self.get_mapping(&mapping.id).await? {
                            let mut merged = mapping.clone();
                            merged.usage_stats.apply_count += existing.usage_stats.apply_count;
                            merged.usage_stats.accept_count += existing.usage_stats.accept_count;
                            merged.usage_stats.reject_count += existing.usage_stats.reject_count;

                            // Keep most recent last_used
                            if let Some(existing_last_used) = existing.usage_stats.last_used {
                                if merged.usage_stats.last_used.is_none()
                                    || existing_last_used > merged.usage_stats.last_used.unwrap()
                                {
                                    merged.usage_stats.last_used = Some(existing_last_used);
                                }
                            }

                            // Use newer created_at
                            if existing.created_at > merged.created_at {
                                merged.created_at = existing.created_at;
                            }

                            // Store merged mapping
                            if !options.dry_run {
                                self.store_mapping(merged).await?;
                            }
                            result.successful += 1;
                            result.imported_ids.push(mapping.id.clone());
                            continue;
                        }
                    }
                }
            }

            // Step 3: Store the mapping (if not dry run)
            if !options.dry_run {
                match self.store_mapping(mapping.clone()).await {
                    Ok(_) => {
                        result.successful += 1;
                        result.imported_ids.push(mapping.id.clone());
                    }
                    Err(e) => {
                        result.failed += 1;
                        result.errors.push(ValidationError {
                            mapping_id: mapping.id.clone(),
                            error_type: ValidationErrorType::Other,
                            message: format!("Failed to store mapping: {}", e),
                        });
                    }
                }
            } else {
                // Dry run - just count as successful
                result.successful += 1;
                result.imported_ids.push(mapping.id.clone());
            }
        }

        info!(
            "Bulk import completed: {} successful, {} skipped, {} failed (dry_run={})",
            result.successful, result.skipped, result.failed, options.dry_run
        );

        // Record metrics
        let duration = start.elapsed().as_secs_f64();
        let conflict_resolution_str = match options.conflict_resolution {
            ConflictResolution::Skip => "skip",
            ConflictResolution::Overwrite => "overwrite",
            ConflictResolution::Merge => "merge",
            ConflictResolution::Fail => "fail",
        };
        self.metrics.record_bulk_import(
            conflict_resolution_str,
            duration,
            result.successful,
            result.skipped,
            result.failed,
        );

        Ok(result)
    }

    /// Legacy bulk import (uses default options with Skip conflict resolution)
    pub async fn bulk_import(&self, import: MappingImportExport) -> Result<ImportStats> {
        let options = ImportOptions::default();
        let result = self.bulk_import_with_options(import, options).await?;

        Ok(ImportStats {
            successful: result.successful,
            failed: result.failed + result.skipped,
        })
    }

    /// Bulk export mappings
    pub async fn bulk_export(&self, filter: Option<ExportFilter>) -> Result<MappingImportExport> {
        // TODO: Implement SPARQL query when RdfStore adds query support
        // For now, scan RocksDB for all mappings
        let mut mappings = Vec::new();

        let cf_data = self
            .rocksdb
            .cf_handle(CF_MAPPING_DATA)
            .context("CF_MAPPING_DATA not found")?;

        let iter = self
            .rocksdb
            .iterator_cf(cf_data, rocksdb::IteratorMode::Start);
        for item in iter {
            let (_, value) = item?;
            let mapping: ManualFieldMapping = bincode::deserialize(&value)?;

            // Apply filter
            let include = match &filter {
                Some(ExportFilter::ByUser(user)) => mapping.created_by == *user,
                Some(ExportFilter::BySource(source)) => {
                    mapping.source_context.source_id.as_ref() == Some(source)
                }
                None => true,
            };

            if include {
                mappings.push(mapping);
            }
        }

        // Calculate statistics
        let mut unique_sources = std::collections::HashSet::new();
        let mut unique_tables = std::collections::HashSet::new();
        let mut unique_fields = std::collections::HashSet::new();

        for mapping in &mappings {
            if let Some(ref source) = mapping.source_context.source_id {
                unique_sources.insert(source.clone());
            }
            unique_tables.insert(mapping.source_context.table_name.clone());
            unique_fields.insert(mapping.source_context.field_name.clone());
        }

        let export_count = mappings.len();

        // Record metrics
        self.metrics.record_bulk_export(export_count);

        Ok(MappingImportExport {
            version: "1.0.0".to_string(),
            exported_at: chrono::Utc::now(),
            statistics: ImportExportStats {
                total_mappings: export_count,
                unique_sources: unique_sources.len(),
                unique_tables: unique_tables.len(),
                unique_fields: unique_fields.len(),
            },
            mappings,
        })
    }
}

#[derive(Debug)]
pub enum UsageStatType {
    Applied,
    Accepted,
    Rejected,
}

#[derive(Debug, Default)]
pub struct ImportStats {
    pub successful: usize,
    pub failed: usize,
}

#[derive(Debug)]
pub enum ExportFilter {
    ByUser(String),
    BySource(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::governance::in_memory_rdf_store::InMemoryRdfStore;
    use tempfile::TempDir;

    fn create_test_store() -> (ManualMappingStore, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let rdf_store = Arc::new(InMemoryRdfStore::new());
        let store = ManualMappingStore::new(rdf_store, temp_dir.path().to_str().unwrap()).unwrap();
        (store, temp_dir)
    }

    fn create_test_mapping(id: &str, target_uri: &str, user: &str) -> ManualFieldMapping {
        ManualFieldMapping {
            id: id.to_string(),
            source_context: SourceContext {
                source_id: Some("test_source".to_string()),
                table_name: "test_table".to_string(),
                field_name: "test_field".to_string(),
                field_metadata: None,
            },
            target_field_uri: target_uri.to_string(),
            confidence: 1.0,
            created_by: user.to_string(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            notes: None,
            usage_stats: UsageStats::default(),
        }
    }

    #[tokio::test]
    async fn test_store_and_retrieve_mapping() {
        let (store, _temp_dir) = create_test_store();

        let mapping = create_test_mapping("test_1", "schema:email", "user1");

        // Store mapping
        store.store_mapping(mapping.clone()).await.unwrap();

        // Retrieve mapping
        let retrieved = store.get_mapping("test_1").await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().id, "test_1");
    }

    #[tokio::test]
    async fn test_multiple_mappings_same_target() {
        let (store, _temp_dir) = create_test_store();

        // Create two different mappings targeting the same ontology term
        let mapping1 = create_test_mapping("test_1", "schema:email", "user1");
        let mapping2 = create_test_mapping("test_2", "schema:email", "user2");

        // Store both mappings
        store.store_mapping(mapping1).await.unwrap();
        store.store_mapping(mapping2).await.unwrap();

        // Verify both mappings are stored
        assert!(store.get_mapping("test_1").await.unwrap().is_some());
        assert!(store.get_mapping("test_2").await.unwrap().is_some());

        // Verify target index contains both mapping IDs
        let cf_target = store.rocksdb.cf_handle(CF_TARGET_INDEX).unwrap();
        let target_key = MappingIndexKeys::target_to_mappings("schema:email");
        let ids = store.get_from_index(cf_target, &target_key).unwrap();

        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"test_1".to_string()));
        assert!(ids.contains(&"test_2".to_string()));
    }

    #[tokio::test]
    async fn test_multiple_mappings_same_user() {
        let (store, _temp_dir) = create_test_store();

        // Create two mappings by the same user
        let mapping1 = create_test_mapping("test_1", "schema:email", "alice");
        let mapping2 = create_test_mapping("test_2", "schema:name", "alice");

        // Store both mappings
        store.store_mapping(mapping1).await.unwrap();
        store.store_mapping(mapping2).await.unwrap();

        // Verify user index contains both mapping IDs
        let cf_user = store.rocksdb.cf_handle(CF_USER_INDEX).unwrap();
        let user_key = MappingIndexKeys::user_to_mappings("alice");
        let ids = store.get_from_index(cf_user, &user_key).unwrap();

        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"test_1".to_string()));
        assert!(ids.contains(&"test_2".to_string()));
    }

    #[tokio::test]
    async fn test_no_duplicate_ids_in_index() {
        let (store, _temp_dir) = create_test_store();

        // Create mapping
        let mapping = create_test_mapping("test_1", "schema:email", "user1");

        // Store same mapping twice
        store.store_mapping(mapping.clone()).await.unwrap();
        store.store_mapping(mapping).await.unwrap();

        // Verify target index has only one entry (no duplicates)
        let cf_target = store.rocksdb.cf_handle(CF_TARGET_INDEX).unwrap();
        let target_key = MappingIndexKeys::target_to_mappings("schema:email");
        let ids = store.get_from_index(cf_target, &target_key).unwrap();

        assert_eq!(ids.len(), 1);
        assert_eq!(ids[0], "test_1");
    }

    #[tokio::test]
    async fn test_find_by_source() {
        let (store, _temp_dir) = create_test_store();

        let mapping = create_test_mapping("test_1", "schema:email", "user1");

        // Store mapping
        store.store_mapping(mapping.clone()).await.unwrap();

        // Find by source
        let found = store.find_by_source(&mapping.source_context).await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, "test_1");
    }

    #[tokio::test]
    async fn test_update_usage_stats() {
        let (store, _temp_dir) = create_test_store();

        let mapping = create_test_mapping("test_1", "schema:email", "user1");

        // Store mapping
        store.store_mapping(mapping).await.unwrap();

        // Update usage stats
        store
            .update_usage_stats("test_1", UsageStatType::Applied)
            .await
            .unwrap();

        // Retrieve and verify
        let retrieved = store.get_mapping("test_1").await.unwrap().unwrap();
        assert_eq!(retrieved.usage_stats.apply_count, 1);
    }

    #[tokio::test]
    async fn test_pattern_index() {
        let (store, _temp_dir) = create_test_store();

        // Create mapping with pattern metadata
        let mut mapping1 = create_test_mapping("test_1", "schema:email", "user1");
        mapping1.source_context.field_metadata = Some(FieldCharacteristics {
            data_type: Some("String".to_string()),
            sample_values: vec![],
            detected_pattern: Some("email".to_string()),
            profile_hash: None,
        });

        let mut mapping2 = create_test_mapping("test_2", "schema:workEmail", "user2");
        mapping2.source_context.field_metadata = Some(FieldCharacteristics {
            data_type: Some("String".to_string()),
            sample_values: vec![],
            detected_pattern: Some("email".to_string()),
            profile_hash: None,
        });

        // Store mappings
        store.store_mapping(mapping1).await.unwrap();
        store.store_mapping(mapping2).await.unwrap();

        // Verify pattern index contains both
        let cf_pattern = store.rocksdb.cf_handle(CF_PATTERN_INDEX).unwrap();
        let pattern_key = MappingIndexKeys::pattern_to_mappings("email");
        let ids = store.get_from_index(cf_pattern, &pattern_key).unwrap();

        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"test_1".to_string()));
        assert!(ids.contains(&"test_2".to_string()));
    }

    #[tokio::test]
    async fn test_find_by_pattern() {
        let (store, _temp_dir) = create_test_store();

        // Create mapping with pattern
        let mut mapping = create_test_mapping("test_1", "schema:email", "user1");
        mapping.source_context.field_metadata = Some(FieldCharacteristics {
            data_type: Some("String".to_string()),
            sample_values: vec![],
            detected_pattern: Some("email".to_string()),
            profile_hash: None,
        });

        store.store_mapping(mapping).await.unwrap();

        // Find by pattern
        let found = store.find_by_pattern("email").await.unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, "test_1");
    }

    #[tokio::test]
    async fn test_bulk_export() {
        let (store, _temp_dir) = create_test_store();

        // Store multiple mappings
        let mapping1 = create_test_mapping("test_1", "schema:email", "alice");
        let mapping2 = create_test_mapping("test_2", "schema:name", "bob");

        store.store_mapping(mapping1).await.unwrap();
        store.store_mapping(mapping2).await.unwrap();

        // Export all
        let export = store.bulk_export(None).await.unwrap();
        assert_eq!(export.mappings.len(), 2);
        assert_eq!(export.statistics.total_mappings, 2);

        // Export filtered by user
        let export_alice = store
            .bulk_export(Some(ExportFilter::ByUser("alice".to_string())))
            .await
            .unwrap();
        assert_eq!(export_alice.mappings.len(), 1);
        assert_eq!(export_alice.mappings[0].created_by, "alice");
    }

    #[tokio::test]
    async fn test_delete_mapping() {
        let (store, _temp_dir) = create_test_store();

        let mapping = create_test_mapping("test_1", "schema:email", "user1");

        // Store mapping
        store.store_mapping(mapping.clone()).await.unwrap();

        // Verify it exists
        let retrieved = store.get_mapping("test_1").await.unwrap();
        assert!(retrieved.is_some());

        // Delete mapping
        let deleted = store.delete_mapping("test_1").await.unwrap();
        assert!(deleted);

        // Verify it no longer exists
        let retrieved_after_delete = store.get_mapping("test_1").await.unwrap();
        assert!(retrieved_after_delete.is_none());

        // Verify source index is cleaned up
        let found_by_source = store.find_by_source(&mapping.source_context).await.unwrap();
        assert!(found_by_source.is_none());
    }

    #[tokio::test]
    async fn test_delete_nonexistent_mapping() {
        let (store, _temp_dir) = create_test_store();

        // Try to delete non-existent mapping
        let deleted = store.delete_mapping("nonexistent").await.unwrap();
        assert!(!deleted);
    }

    #[tokio::test]
    async fn test_delete_cleans_up_target_index() {
        let (store, _temp_dir) = create_test_store();

        // Create two mappings targeting the same ontology term
        let mapping1 = create_test_mapping("test_1", "schema:email", "user1");
        let mapping2 = create_test_mapping("test_2", "schema:email", "user2");

        store.store_mapping(mapping1).await.unwrap();
        store.store_mapping(mapping2).await.unwrap();

        // Verify target index has both IDs
        let cf_target = store.rocksdb.cf_handle(CF_TARGET_INDEX).unwrap();
        let target_key = MappingIndexKeys::target_to_mappings("schema:email");
        let ids_before = store.get_from_index(cf_target, &target_key).unwrap();
        assert_eq!(ids_before.len(), 2);

        // Delete first mapping
        store.delete_mapping("test_1").await.unwrap();

        // Verify target index only has one ID now
        let ids_after = store.get_from_index(cf_target, &target_key).unwrap();
        assert_eq!(ids_after.len(), 1);
        assert_eq!(ids_after[0], "test_2");

        // Delete second mapping
        store.delete_mapping("test_2").await.unwrap();

        // Verify target index is empty (or key is deleted)
        let ids_final = store.get_from_index(cf_target, &target_key).unwrap();
        assert_eq!(ids_final.len(), 0);
    }

    #[tokio::test]
    async fn test_delete_cleans_up_user_index() {
        let (store, _temp_dir) = create_test_store();

        // Create two mappings by the same user
        let mapping1 = create_test_mapping("test_1", "schema:email", "alice");
        let mapping2 = create_test_mapping("test_2", "schema:name", "alice");

        store.store_mapping(mapping1).await.unwrap();
        store.store_mapping(mapping2).await.unwrap();

        // Verify user index has both IDs
        let cf_user = store.rocksdb.cf_handle(CF_USER_INDEX).unwrap();
        let user_key = MappingIndexKeys::user_to_mappings("alice");
        let ids_before = store.get_from_index(cf_user, &user_key).unwrap();
        assert_eq!(ids_before.len(), 2);

        // Delete first mapping
        store.delete_mapping("test_1").await.unwrap();

        // Verify user index only has one ID now
        let ids_after = store.get_from_index(cf_user, &user_key).unwrap();
        assert_eq!(ids_after.len(), 1);
        assert_eq!(ids_after[0], "test_2");
    }

    #[tokio::test]
    async fn test_delete_cleans_up_pattern_index() {
        let (store, _temp_dir) = create_test_store();

        // Create mapping with pattern metadata
        let mut mapping = create_test_mapping("test_1", "schema:email", "user1");
        mapping.source_context.field_metadata = Some(FieldCharacteristics {
            data_type: Some("String".to_string()),
            sample_values: vec![],
            detected_pattern: Some("email".to_string()),
            profile_hash: None,
        });

        store.store_mapping(mapping).await.unwrap();

        // Verify pattern index has the ID
        let cf_pattern = store.rocksdb.cf_handle(CF_PATTERN_INDEX).unwrap();
        let pattern_key = MappingIndexKeys::pattern_to_mappings("email");
        let ids_before = store.get_from_index(cf_pattern, &pattern_key).unwrap();
        assert_eq!(ids_before.len(), 1);
        assert_eq!(ids_before[0], "test_1");

        // Delete mapping
        store.delete_mapping("test_1").await.unwrap();

        // Verify pattern index is cleaned up
        let ids_after = store.get_from_index(cf_pattern, &pattern_key).unwrap();
        assert_eq!(ids_after.len(), 0);
    }

    #[tokio::test]
    async fn test_delete_cleans_up_cache() {
        let (store, _temp_dir) = create_test_store();

        let mapping = create_test_mapping("test_1", "schema:email", "user1");

        // Store mapping
        store.store_mapping(mapping).await.unwrap();

        // Access it to ensure it's in cache
        store.get_mapping("test_1").await.unwrap();

        // Verify cache has the mapping
        {
            let cache = store.cache.read().await;
            assert!(cache.contains_key("test_1"));
        }

        // Delete mapping
        store.delete_mapping("test_1").await.unwrap();

        // Verify cache no longer has the mapping
        {
            let cache = store.cache.read().await;
            assert!(!cache.contains_key("test_1"));
        }
    }

    #[tokio::test]
    async fn test_bulk_import_validation() {
        let (store, _temp_dir) = create_test_store();

        // Create import with invalid mappings
        let mut mappings = vec![];

        // Valid mapping
        mappings.push(create_test_mapping("valid_1", "schema:email", "user1"));

        // Invalid: empty ID
        let mut invalid_id = create_test_mapping("", "schema:name", "user1");
        invalid_id.id = "".to_string();
        mappings.push(invalid_id);

        // Invalid: empty target URI
        let mut invalid_uri = create_test_mapping("invalid_2", "", "user1");
        invalid_uri.target_field_uri = "".to_string();
        mappings.push(invalid_uri);

        // Invalid: confidence != 1.0
        let mut invalid_conf = create_test_mapping("invalid_3", "schema:phone", "user1");
        invalid_conf.confidence = 0.8;
        mappings.push(invalid_conf);

        let import = MappingImportExport {
            version: "1.0".to_string(),
            exported_at: chrono::Utc::now(),
            mappings: mappings.clone(),
            statistics: ImportExportStats {
                total_mappings: mappings.len(),
                unique_sources: 1,
                unique_tables: 1,
                unique_fields: 4,
            },
        };

        let options = ImportOptions {
            validate_structure: true,
            ..Default::default()
        };

        let result = store
            .bulk_import_with_options(import, options)
            .await
            .unwrap();

        assert_eq!(result.total, 4);
        assert_eq!(result.successful, 1);
        assert_eq!(result.failed, 3);
        assert_eq!(result.errors.len(), 3);

        // Verify valid mapping was imported
        assert!(store.get_mapping("valid_1").await.unwrap().is_some());
    }

    #[tokio::test]
    async fn test_bulk_import_conflict_skip() {
        let (store, _temp_dir) = create_test_store();

        // Store an existing mapping
        let existing = create_test_mapping("conflict_1", "schema:email", "user1");
        store.store_mapping(existing).await.unwrap();

        // Try to import with same ID
        let import = MappingImportExport {
            version: "1.0".to_string(),
            exported_at: chrono::Utc::now(),
            mappings: vec![create_test_mapping("conflict_1", "schema:phone", "user2")],
            statistics: ImportExportStats {
                total_mappings: 1,
                unique_sources: 1,
                unique_tables: 1,
                unique_fields: 1,
            },
        };

        let options = ImportOptions {
            conflict_resolution: ConflictResolution::Skip,
            ..Default::default()
        };

        let result = store
            .bulk_import_with_options(import, options)
            .await
            .unwrap();

        assert_eq!(result.skipped, 1);
        assert_eq!(result.successful, 0);

        // Verify original mapping unchanged
        let mapping = store.get_mapping("conflict_1").await.unwrap().unwrap();
        assert_eq!(mapping.target_field_uri, "schema:email");
        assert_eq!(mapping.created_by, "user1");
    }

    #[tokio::test]
    async fn test_bulk_import_conflict_overwrite() {
        let (store, _temp_dir) = create_test_store();

        // Store an existing mapping
        let existing = create_test_mapping("conflict_1", "schema:email", "user1");
        store.store_mapping(existing).await.unwrap();

        // Import with overwrite
        let import = MappingImportExport {
            version: "1.0".to_string(),
            exported_at: chrono::Utc::now(),
            mappings: vec![create_test_mapping("conflict_1", "schema:phone", "user2")],
            statistics: ImportExportStats {
                total_mappings: 1,
                unique_sources: 1,
                unique_tables: 1,
                unique_fields: 1,
            },
        };

        let options = ImportOptions {
            conflict_resolution: ConflictResolution::Overwrite,
            ..Default::default()
        };

        let result = store
            .bulk_import_with_options(import, options)
            .await
            .unwrap();

        assert_eq!(result.successful, 1);
        assert_eq!(result.skipped, 0);

        // Verify mapping was overwritten
        let mapping = store.get_mapping("conflict_1").await.unwrap().unwrap();
        assert_eq!(mapping.target_field_uri, "schema:phone");
        assert_eq!(mapping.created_by, "user2");
    }

    #[tokio::test]
    async fn test_bulk_import_conflict_merge() {
        let (store, _temp_dir) = create_test_store();

        // Store an existing mapping with usage stats
        let mut existing = create_test_mapping("conflict_1", "schema:email", "user1");
        existing.usage_stats.apply_count = 10;
        existing.usage_stats.accept_count = 8;
        existing.usage_stats.reject_count = 2;
        store.store_mapping(existing).await.unwrap();

        // Import with merge strategy
        let mut import_mapping = create_test_mapping("conflict_1", "schema:email", "user2");
        import_mapping.usage_stats.apply_count = 5;
        import_mapping.usage_stats.accept_count = 4;
        import_mapping.usage_stats.reject_count = 1;

        let import = MappingImportExport {
            version: "1.0".to_string(),
            exported_at: chrono::Utc::now(),
            mappings: vec![import_mapping],
            statistics: ImportExportStats {
                total_mappings: 1,
                unique_sources: 1,
                unique_tables: 1,
                unique_fields: 1,
            },
        };

        let options = ImportOptions {
            conflict_resolution: ConflictResolution::Merge,
            ..Default::default()
        };

        let result = store
            .bulk_import_with_options(import, options)
            .await
            .unwrap();

        assert_eq!(result.successful, 1);
        assert_eq!(result.skipped, 0);

        // Verify usage stats were merged
        let mapping = store.get_mapping("conflict_1").await.unwrap().unwrap();
        assert_eq!(mapping.usage_stats.apply_count, 15); // 10 + 5
        assert_eq!(mapping.usage_stats.accept_count, 12); // 8 + 4
        assert_eq!(mapping.usage_stats.reject_count, 3); // 2 + 1
    }

    #[tokio::test]
    async fn test_bulk_import_conflict_fail() {
        let (store, _temp_dir) = create_test_store();

        // Store an existing mapping
        let existing = create_test_mapping("conflict_1", "schema:email", "user1");
        store.store_mapping(existing).await.unwrap();

        // Import with fail strategy
        let import = MappingImportExport {
            version: "1.0".to_string(),
            exported_at: chrono::Utc::now(),
            mappings: vec![
                create_test_mapping("conflict_1", "schema:phone", "user2"),
                create_test_mapping("good_1", "schema:address", "user2"),
            ],
            statistics: ImportExportStats {
                total_mappings: 2,
                unique_sources: 1,
                unique_tables: 1,
                unique_fields: 2,
            },
        };

        let options = ImportOptions {
            conflict_resolution: ConflictResolution::Fail,
            ..Default::default()
        };

        let result = store
            .bulk_import_with_options(import, options)
            .await
            .unwrap();

        assert_eq!(result.failed, 1);
        assert_eq!(result.successful, 0);
        assert_eq!(result.errors.len(), 1);
        assert_eq!(
            result.errors[0].error_type,
            ValidationErrorType::DuplicateId
        );

        // Verify no new mappings were imported (fail fast)
        assert!(store.get_mapping("good_1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_bulk_import_dry_run() {
        let (store, _temp_dir) = create_test_store();

        let import = MappingImportExport {
            version: "1.0".to_string(),
            exported_at: chrono::Utc::now(),
            mappings: vec![
                create_test_mapping("dry_1", "schema:email", "user1"),
                create_test_mapping("dry_2", "schema:phone", "user1"),
            ],
            statistics: ImportExportStats {
                total_mappings: 2,
                unique_sources: 1,
                unique_tables: 1,
                unique_fields: 2,
            },
        };

        let options = ImportOptions {
            dry_run: true,
            ..Default::default()
        };

        let result = store
            .bulk_import_with_options(import, options)
            .await
            .unwrap();

        assert_eq!(result.successful, 2);
        assert_eq!(result.failed, 0);

        // Verify no mappings were actually stored
        assert!(store.get_mapping("dry_1").await.unwrap().is_none());
        assert!(store.get_mapping("dry_2").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_bulk_import_source_context_conflict() {
        let (store, _temp_dir) = create_test_store();

        // Store mapping with specific source context
        let existing = create_test_mapping("existing_1", "schema:email", "user1");
        store.store_mapping(existing).await.unwrap();

        // Try to import mapping with same source context but different ID
        let mut conflict_mapping = create_test_mapping("new_id", "schema:phone", "user2");
        conflict_mapping.source_context = SourceContext {
            source_id: Some("test_source".to_string()),
            table_name: "test_table".to_string(),
            field_name: "test_field".to_string(),
            field_metadata: None,
        };

        let import = MappingImportExport {
            version: "1.0".to_string(),
            exported_at: chrono::Utc::now(),
            mappings: vec![conflict_mapping],
            statistics: ImportExportStats {
                total_mappings: 1,
                unique_sources: 1,
                unique_tables: 1,
                unique_fields: 1,
            },
        };

        let options = ImportOptions {
            conflict_resolution: ConflictResolution::Skip,
            check_duplicates: true,
            ..Default::default()
        };

        let result = store
            .bulk_import_with_options(import, options)
            .await
            .unwrap();

        assert_eq!(result.skipped, 1);
        assert_eq!(result.skipped_ids[0], "new_id");
    }
}
