//! RDF operations metrics
//!
//! Tracks RDF store and SPARQL query performance:
//! - Query counts by type (SELECT, INSERT, DELETE, CONSTRUCT)
//! - Query execution latency
//! - Triple operation counts
//! - SPARQL parse errors

use anyhow::Result;
use prometheus::{exponential_buckets, HistogramOpts, HistogramVec, IntCounterVec, Opts, Registry};

/// RDF operations metrics
///
/// Monitors SPARQL query performance and RDF store operations.
pub struct RdfMetrics {
    sparql_queries_total: IntCounterVec,
    sparql_query_duration_seconds: HistogramVec,
    triple_operations_total: IntCounterVec,
    sparql_parse_errors_total: IntCounterVec,
}

impl RdfMetrics {
    /// Create and register RDF metrics
    pub fn new(registry: &Registry) -> Result<Self> {
        let sparql_queries_total = IntCounterVec::new(
            Opts::new(
                "graphica_sparql_queries_total",
                "Total SPARQL queries by type",
            ),
            &["query_type"], // SELECT, INSERT, DELETE, CONSTRUCT, DESCRIBE, ASK
        )?;

        let sparql_query_duration_seconds = HistogramVec::new(
            HistogramOpts::new(
                "graphica_sparql_query_duration_seconds",
                "SPARQL query execution time in seconds",
            )
            .buckets(exponential_buckets(0.001, 2.0, 12)?), // 1ms to ~4s
            &["query_type"],
        )?;

        let triple_operations_total = IntCounterVec::new(
            Opts::new(
                "graphica_triple_operations_total",
                "Total triple operations by type",
            ),
            &["operation"], // insert, delete, query
        )?;

        let sparql_parse_errors_total = IntCounterVec::new(
            Opts::new(
                "graphica_sparql_parse_errors_total",
                "SPARQL parse errors by error type",
            ),
            &["error_type"],
        )?;

        registry.register(Box::new(sparql_queries_total.clone()))?;
        registry.register(Box::new(sparql_query_duration_seconds.clone()))?;
        registry.register(Box::new(triple_operations_total.clone()))?;
        registry.register(Box::new(sparql_parse_errors_total.clone()))?;

        Ok(Self {
            sparql_queries_total,
            sparql_query_duration_seconds,
            triple_operations_total,
            sparql_parse_errors_total,
        })
    }

    /// Record SPARQL query execution
    pub fn record_query(&self, query_type: &str, duration_secs: f64) {
        self.sparql_queries_total
            .with_label_values(&[query_type])
            .inc();

        self.sparql_query_duration_seconds
            .with_label_values(&[query_type])
            .observe(duration_secs);
    }

    /// Record triple operation
    pub fn record_triple_operation(&self, operation: &str, count: u64) {
        self.triple_operations_total
            .with_label_values(&[operation])
            .inc_by(count);
    }

    /// Record SPARQL parse error
    pub fn record_parse_error(&self, error_type: &str) {
        self.sparql_parse_errors_total
            .with_label_values(&[error_type])
            .inc();
    }
}
