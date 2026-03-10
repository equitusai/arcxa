//! Manual Mapping Metrics
//!
//! Tracks manual mapping operations and usage patterns:
//! - Mapping creation, updates, deletes
//! - Bulk import/export operations
//! - Query patterns (by source, by pattern, suggestions)
//! - Usage statistics (applies, accepts, rejects)
//! - Cache hit rates

use anyhow::Result;
use prometheus::{
    exponential_buckets, HistogramOpts, HistogramVec, IntCounter, IntCounterVec, IntGauge,
    IntGaugeVec, Opts, Registry,
};
use std::sync::Arc;

/// Manual mapping metrics
///
/// Monitors manual mapping store operations, usage patterns, and performance.
#[derive(Clone)]
pub struct ManualMappingMetrics {
    // Operation counters
    operations_total: IntCounterVec,
    operation_errors_total: IntCounterVec,
    operation_duration_seconds: HistogramVec,

    // Storage metrics
    mappings_total: IntGauge,
    mappings_by_source: IntGaugeVec,
    mappings_by_user: IntGaugeVec,

    // Query metrics
    queries_total: IntCounterVec,
    query_results: HistogramVec,

    // Cache metrics
    cache_hits_total: IntCounter,
    cache_misses_total: IntCounter,
    cache_size: IntGauge,

    // Bulk operation metrics
    bulk_import_mappings_total: IntCounterVec,
    bulk_import_duration_seconds: HistogramVec,
    bulk_export_mappings_total: IntCounter,

    // Usage statistics
    mapping_applies_total: IntCounter,
    mapping_accepts_total: IntCounter,
    mapping_rejects_total: IntCounter,

    // Suggestion metrics
    suggestions_generated_total: IntCounterVec,
    suggestions_relevance_score: HistogramVec,
}

impl ManualMappingMetrics {
    /// Create and register manual mapping metrics
    pub fn new(registry: &Registry) -> Result<Self> {
        // Operation counters
        let operations_total = IntCounterVec::new(
            Opts::new(
                "graphica_manual_mapping_operations_total",
                "Total manual mapping operations by type",
            ),
            &["operation"], // store, update, delete, find_by_source, find_by_pattern, etc.
        )?;

        let operation_errors_total = IntCounterVec::new(
            Opts::new(
                "graphica_manual_mapping_operation_errors_total",
                "Total manual mapping operation errors by type",
            ),
            &["operation", "error_type"],
        )?;

        let operation_duration_seconds = HistogramVec::new(
            HistogramOpts::new(
                "graphica_manual_mapping_operation_duration_seconds",
                "Manual mapping operation duration in seconds",
            )
            .buckets(exponential_buckets(0.0001, 2.0, 12)?), // 0.1ms to ~400ms
            &["operation"],
        )?;

        // Storage metrics
        let mappings_total = IntGauge::new(
            "graphica_manual_mappings_total",
            "Total number of manual mappings stored",
        )?;

        let mappings_by_source = IntGaugeVec::new(
            Opts::new(
                "graphica_manual_mappings_by_source",
                "Number of manual mappings by source system",
            ),
            &["source_id"],
        )?;

        let mappings_by_user = IntGaugeVec::new(
            Opts::new(
                "graphica_manual_mappings_by_user",
                "Number of manual mappings by user",
            ),
            &["user"],
        )?;

        // Query metrics
        let queries_total = IntCounterVec::new(
            Opts::new(
                "graphica_manual_mapping_queries_total",
                "Total manual mapping queries by type",
            ),
            &["query_type"], // by_source, by_pattern, by_target, suggest
        )?;

        let query_results = HistogramVec::new(
            HistogramOpts::new(
                "graphica_manual_mapping_query_results",
                "Number of results returned by query type",
            )
            .buckets(vec![0.0, 1.0, 5.0, 10.0, 25.0, 50.0, 100.0, 250.0]),
            &["query_type"],
        )?;

        // Cache metrics
        let cache_hits_total = IntCounter::new(
            "graphica_manual_mapping_cache_hits_total",
            "Total manual mapping cache hits",
        )?;

        let cache_misses_total = IntCounter::new(
            "graphica_manual_mapping_cache_misses_total",
            "Total manual mapping cache misses",
        )?;

        let cache_size = IntGauge::new(
            "graphica_manual_mapping_cache_size",
            "Current number of mappings in cache",
        )?;

        // Bulk operation metrics
        let bulk_import_mappings_total = IntCounterVec::new(
            Opts::new(
                "graphica_manual_mapping_bulk_import_mappings_total",
                "Total mappings processed in bulk imports by result",
            ),
            &["result"], // successful, skipped, failed
        )?;

        let bulk_import_duration_seconds = HistogramVec::new(
            HistogramOpts::new(
                "graphica_manual_mapping_bulk_import_duration_seconds",
                "Bulk import operation duration in seconds",
            )
            .buckets(exponential_buckets(0.1, 2.0, 10)?), // 100ms to ~100s
            &["conflict_resolution"], // skip, overwrite, merge, fail
        )?;

        let bulk_export_mappings_total = IntCounter::new(
            "graphica_manual_mapping_bulk_export_mappings_total",
            "Total mappings exported in bulk exports",
        )?;

        // Usage statistics
        let mapping_applies_total = IntCounter::new(
            "graphica_manual_mapping_applies_total",
            "Total times manual mappings were applied",
        )?;

        let mapping_accepts_total = IntCounter::new(
            "graphica_manual_mapping_accepts_total",
            "Total times manual mapping suggestions were accepted",
        )?;

        let mapping_rejects_total = IntCounter::new(
            "graphica_manual_mapping_rejects_total",
            "Total times manual mapping suggestions were rejected",
        )?;

        // Suggestion metrics
        let suggestions_generated_total = IntCounterVec::new(
            Opts::new(
                "graphica_manual_mapping_suggestions_generated_total",
                "Total mapping suggestions generated by reason",
            ),
            &["reason"], // exact_match, similar_field, similar_profile, frequent_pattern
        )?;

        let suggestions_relevance_score = HistogramVec::new(
            HistogramOpts::new(
                "graphica_manual_mapping_suggestions_relevance_score",
                "Relevance scores of generated suggestions",
            )
            .buckets(vec![0.5, 0.6, 0.7, 0.75, 0.8, 0.85, 0.9, 0.95, 1.0]),
            &["reason"],
        )?;

        // Register all metrics
        registry.register(Box::new(operations_total.clone()))?;
        registry.register(Box::new(operation_errors_total.clone()))?;
        registry.register(Box::new(operation_duration_seconds.clone()))?;
        registry.register(Box::new(mappings_total.clone()))?;
        registry.register(Box::new(mappings_by_source.clone()))?;
        registry.register(Box::new(mappings_by_user.clone()))?;
        registry.register(Box::new(queries_total.clone()))?;
        registry.register(Box::new(query_results.clone()))?;
        registry.register(Box::new(cache_hits_total.clone()))?;
        registry.register(Box::new(cache_misses_total.clone()))?;
        registry.register(Box::new(cache_size.clone()))?;
        registry.register(Box::new(bulk_import_mappings_total.clone()))?;
        registry.register(Box::new(bulk_import_duration_seconds.clone()))?;
        registry.register(Box::new(bulk_export_mappings_total.clone()))?;
        registry.register(Box::new(mapping_applies_total.clone()))?;
        registry.register(Box::new(mapping_accepts_total.clone()))?;
        registry.register(Box::new(mapping_rejects_total.clone()))?;
        registry.register(Box::new(suggestions_generated_total.clone()))?;
        registry.register(Box::new(suggestions_relevance_score.clone()))?;

        Ok(Self {
            operations_total,
            operation_errors_total,
            operation_duration_seconds,
            mappings_total,
            mappings_by_source,
            mappings_by_user,
            queries_total,
            query_results,
            cache_hits_total,
            cache_misses_total,
            cache_size,
            bulk_import_mappings_total,
            bulk_import_duration_seconds,
            bulk_export_mappings_total,
            mapping_applies_total,
            mapping_accepts_total,
            mapping_rejects_total,
            suggestions_generated_total,
            suggestions_relevance_score,
        })
    }

    /// Record an operation completion
    pub fn record_operation(&self, operation: &str, duration_secs: f64) {
        self.operations_total.with_label_values(&[operation]).inc();

        self.operation_duration_seconds
            .with_label_values(&[operation])
            .observe(duration_secs);
    }

    /// Record an operation error
    pub fn record_error(&self, operation: &str, error_type: &str) {
        self.operation_errors_total
            .with_label_values(&[operation, error_type])
            .inc();
    }

    /// Update total mappings count
    pub fn set_mappings_total(&self, count: i64) {
        self.mappings_total.set(count);
    }

    /// Update mappings count by source
    pub fn set_mappings_by_source(&self, source_id: &str, count: i64) {
        self.mappings_by_source
            .with_label_values(&[source_id])
            .set(count);
    }

    /// Update mappings count by user
    pub fn set_mappings_by_user(&self, user: &str, count: i64) {
        self.mappings_by_user.with_label_values(&[user]).set(count);
    }

    /// Record a query
    pub fn record_query(&self, query_type: &str, result_count: usize) {
        self.queries_total.with_label_values(&[query_type]).inc();

        self.query_results
            .with_label_values(&[query_type])
            .observe(result_count as f64);
    }

    /// Record cache hit
    pub fn record_cache_hit(&self) {
        self.cache_hits_total.inc();
    }

    /// Record cache miss
    pub fn record_cache_miss(&self) {
        self.cache_misses_total.inc();
    }

    /// Update cache size
    pub fn set_cache_size(&self, size: i64) {
        self.cache_size.set(size);
    }

    /// Record bulk import results
    pub fn record_bulk_import(
        &self,
        conflict_resolution: &str,
        duration_secs: f64,
        successful: usize,
        skipped: usize,
        failed: usize,
    ) {
        self.bulk_import_duration_seconds
            .with_label_values(&[conflict_resolution])
            .observe(duration_secs);

        if successful > 0 {
            self.bulk_import_mappings_total
                .with_label_values(&["successful"])
                .inc_by(successful as u64);
        }

        if skipped > 0 {
            self.bulk_import_mappings_total
                .with_label_values(&["skipped"])
                .inc_by(skipped as u64);
        }

        if failed > 0 {
            self.bulk_import_mappings_total
                .with_label_values(&["failed"])
                .inc_by(failed as u64);
        }
    }

    /// Record bulk export
    pub fn record_bulk_export(&self, count: usize) {
        self.bulk_export_mappings_total.inc_by(count as u64);
    }

    /// Record mapping apply
    pub fn record_apply(&self) {
        self.mapping_applies_total.inc();
    }

    /// Record suggestion accept
    pub fn record_accept(&self) {
        self.mapping_accepts_total.inc();
    }

    /// Record suggestion reject
    pub fn record_reject(&self) {
        self.mapping_rejects_total.inc();
    }

    /// Record suggestion generation
    pub fn record_suggestion(&self, reason: &str, relevance_score: f64) {
        self.suggestions_generated_total
            .with_label_values(&[reason])
            .inc();

        self.suggestions_relevance_score
            .with_label_values(&[reason])
            .observe(relevance_score);
    }
}

impl Default for ManualMappingMetrics {
    fn default() -> Self {
        let registry = Registry::new();
        Self::new(&registry).expect("Failed to create manual mapping metrics")
    }
}

/// Wrapper to make metrics optional (for testing without metrics)
#[derive(Clone)]
pub struct OptionalMetrics {
    inner: Option<Arc<ManualMappingMetrics>>,
}

impl OptionalMetrics {
    pub fn new(metrics: Option<Arc<ManualMappingMetrics>>) -> Self {
        Self { inner: metrics }
    }

    pub fn none() -> Self {
        Self { inner: None }
    }

    pub fn record_operation(&self, operation: &str, duration_secs: f64) {
        if let Some(m) = &self.inner {
            m.record_operation(operation, duration_secs);
        }
    }

    pub fn record_error(&self, operation: &str, error_type: &str) {
        if let Some(m) = &self.inner {
            m.record_error(operation, error_type);
        }
    }

    pub fn set_mappings_total(&self, count: i64) {
        if let Some(m) = &self.inner {
            m.set_mappings_total(count);
        }
    }

    pub fn record_query(&self, query_type: &str, result_count: usize) {
        if let Some(m) = &self.inner {
            m.record_query(query_type, result_count);
        }
    }

    pub fn record_cache_hit(&self) {
        if let Some(m) = &self.inner {
            m.record_cache_hit();
        }
    }

    pub fn record_cache_miss(&self) {
        if let Some(m) = &self.inner {
            m.record_cache_miss();
        }
    }

    pub fn set_cache_size(&self, size: i64) {
        if let Some(m) = &self.inner {
            m.set_cache_size(size);
        }
    }

    pub fn record_bulk_import(
        &self,
        conflict_resolution: &str,
        duration_secs: f64,
        successful: usize,
        skipped: usize,
        failed: usize,
    ) {
        if let Some(m) = &self.inner {
            m.record_bulk_import(
                conflict_resolution,
                duration_secs,
                successful,
                skipped,
                failed,
            );
        }
    }

    pub fn record_bulk_export(&self, count: usize) {
        if let Some(m) = &self.inner {
            m.record_bulk_export(count);
        }
    }

    pub fn record_suggestion(&self, reason: &str, relevance_score: f64) {
        if let Some(m) = &self.inner {
            m.record_suggestion(reason, relevance_score);
        }
    }
}
