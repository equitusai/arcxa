//! Systems-of-Systems validation metrics
//!
//! Tracks the SoS runtime paths that are expensive or operationally important:
//! - validation execution counts and duration
//! - analytics generation duration
//! - RDF projection duration
//! - validation report persistence, pruning, and history length

use anyhow::Result;
use prometheus::{exponential_buckets, HistogramOpts, HistogramVec, IntCounterVec, Opts, Registry};

/// Systems-of-Systems validation metrics.
#[derive(Clone)]
pub struct SosMetrics {
    validations_total: IntCounterVec,
    validation_duration_seconds: HistogramVec,
    analytics_duration_seconds: HistogramVec,
    projection_duration_seconds: HistogramVec,
    reports_persisted_total: IntCounterVec,
    reports_pruned_total: IntCounterVec,
    validation_history_length: HistogramVec,
}

impl SosMetrics {
    /// Create and register SoS metrics.
    pub fn new(registry: &Registry) -> Result<Self> {
        let validations_total = IntCounterVec::new(
            Opts::new(
                "graphica_sos_validations_total",
                "Total SoS validations by validation type and result",
            ),
            &["validation_type", "result"],
        )?;

        let validation_duration_seconds = HistogramVec::new(
            HistogramOpts::new(
                "graphica_sos_validation_duration_seconds",
                "SoS validation execution latency in seconds",
            )
            .buckets(exponential_buckets(0.001, 2.0, 12)?),
            &["validation_type"],
        )?;

        let analytics_duration_seconds = HistogramVec::new(
            HistogramOpts::new(
                "graphica_sos_analytics_duration_seconds",
                "SoS analytics operation latency in seconds",
            )
            .buckets(exponential_buckets(0.001, 2.0, 14)?),
            &["operation"],
        )?;

        let projection_duration_seconds = HistogramVec::new(
            HistogramOpts::new(
                "graphica_sos_projection_duration_seconds",
                "SoS RDF projection operation latency in seconds",
            )
            .buckets(exponential_buckets(0.0005, 2.0, 13)?),
            &["entity_type", "operation"],
        )?;

        let reports_persisted_total = IntCounterVec::new(
            Opts::new(
                "graphica_sos_validation_reports_persisted_total",
                "Total persisted SoS validation reports",
            ),
            &["validation_type", "subject_type"],
        )?;

        let reports_pruned_total = IntCounterVec::new(
            Opts::new(
                "graphica_sos_validation_reports_pruned_total",
                "Total pruned SoS validation reports",
            ),
            &["reason"],
        )?;

        let validation_history_length = HistogramVec::new(
            HistogramOpts::new(
                "graphica_sos_validation_history_length",
                "Observed SoS validation history length returned by subject type",
            )
            .buckets(vec![
                1.0, 2.0, 5.0, 10.0, 25.0, 50.0, 100.0, 250.0, 500.0, 1_000.0,
            ]),
            &["subject_type"],
        )?;

        registry.register(Box::new(validations_total.clone()))?;
        registry.register(Box::new(validation_duration_seconds.clone()))?;
        registry.register(Box::new(analytics_duration_seconds.clone()))?;
        registry.register(Box::new(projection_duration_seconds.clone()))?;
        registry.register(Box::new(reports_persisted_total.clone()))?;
        registry.register(Box::new(reports_pruned_total.clone()))?;
        registry.register(Box::new(validation_history_length.clone()))?;

        Ok(Self {
            validations_total,
            validation_duration_seconds,
            analytics_duration_seconds,
            projection_duration_seconds,
            reports_persisted_total,
            reports_pruned_total,
            validation_history_length,
        })
    }

    pub fn record_validation(&self, validation_type: &str, result: &str, duration_secs: f64) {
        self.validations_total
            .with_label_values(&[validation_type, result])
            .inc();
        self.validation_duration_seconds
            .with_label_values(&[validation_type])
            .observe(duration_secs);
    }

    pub fn record_analytics(&self, operation: &str, duration_secs: f64) {
        self.analytics_duration_seconds
            .with_label_values(&[operation])
            .observe(duration_secs);
    }

    pub fn record_projection(&self, entity_type: &str, operation: &str, duration_secs: f64) {
        self.projection_duration_seconds
            .with_label_values(&[entity_type, operation])
            .observe(duration_secs);
    }

    pub fn record_report_persisted(&self, validation_type: &str, subject_type: &str) {
        self.reports_persisted_total
            .with_label_values(&[validation_type, subject_type])
            .inc();
    }

    pub fn record_reports_pruned(&self, reason: &str, count: usize) {
        if count == 0 {
            return;
        }

        self.reports_pruned_total
            .with_label_values(&[reason])
            .inc_by(count as u64);
    }

    pub fn observe_history_length(&self, subject_type: &str, count: usize) {
        self.validation_history_length
            .with_label_values(&[subject_type])
            .observe(count as f64);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prometheus::Registry;

    #[test]
    fn sos_metrics_register_and_record() {
        let registry = Registry::new();
        let metrics = SosMetrics::new(&registry).expect("SoS metrics should register");

        metrics.record_validation("interface_compatibility", "passed", 0.01);
        metrics.record_analytics("compatibility_matrix", 0.02);
        metrics.record_projection("interface", "upsert", 0.003);
        metrics.record_report_persisted("interface_compatibility", "interface_pair");
        metrics.record_reports_pruned("retention", 2);
        metrics.observe_history_length("interface_pair", 3);

        let metric_names: Vec<_> = registry
            .gather()
            .into_iter()
            .map(|family| family.name().to_string())
            .collect();

        assert!(metric_names.contains(&"graphica_sos_validations_total".to_string()));
        assert!(metric_names.contains(&"graphica_sos_validation_reports_pruned_total".to_string()));
    }
}
