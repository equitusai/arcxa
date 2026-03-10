//! SLA metric validators for Data Contract management
//!
//! Provides validation for SLA (Service Level Agreement) metrics including:
//! - Operator validation (<=, >=, ==, <, >, !=)
//! - Metric name validation (standard metrics like latency, throughput, etc.)
//! - Value validation (range checks, business logic)
//! - Batch validation for multiple metrics

use anyhow::{bail, Result};

/// Valid SLA operators for metric comparisons
const VALID_OPERATORS: &[&str] = &["<=", ">=", "==", "<", ">", "!="];

/// Standard SLA metric names
const VALID_METRIC_NAMES: &[&str] = &[
    // Latency metrics
    "latency_ms",
    "latency_p50_ms",
    "latency_p95_ms",
    "latency_p99_ms",
    "jitter_ms",
    // Throughput and bandwidth
    "throughput_rps",
    "bandwidth_mbps",
    "disk_io_mbps",
    // Reliability metrics
    "error_rate_percent",
    "availability_percent",
    "uptime_percent",
    "packet_loss_percent",
    // Resource utilization
    "cpu_percent",
    "memory_percent",
];

/// Validate SLA operator
///
/// Ensures the operator is one of the supported comparison operators.
///
/// # Examples
/// ```
/// use graphica_coordinator::api::sos_validation::validators::validate_sla_operator;
///
/// assert!(validate_sla_operator("<=").is_ok());
/// assert!(validate_sla_operator(">=").is_ok());
/// assert!(validate_sla_operator("invalid").is_err());
/// ```
pub fn validate_sla_operator(operator: &str) -> Result<()> {
    if VALID_OPERATORS.contains(&operator) {
        Ok(())
    } else {
        bail!(
            "Invalid SLA operator '{}'. Must be one of: {}",
            operator,
            VALID_OPERATORS.join(", ")
        )
    }
}

/// Validate SLA metric name
///
/// Ensures the metric name is a recognized standard metric.
///
/// # Examples
/// ```
/// use graphica_coordinator::api::sos_validation::validators::validate_sla_metric_name;
///
/// assert!(validate_sla_metric_name("latency_ms").is_ok());
/// assert!(validate_sla_metric_name("throughput_rps").is_ok());
/// assert!(validate_sla_metric_name("custom_metric").is_err());
/// ```
pub fn validate_sla_metric_name(name: &str) -> Result<()> {
    if VALID_METRIC_NAMES.contains(&name) {
        Ok(())
    } else {
        bail!(
            "Invalid SLA metric name '{}'. Must be one of: {}",
            name,
            VALID_METRIC_NAMES.join(", ")
        )
    }
}

/// Validate SLA metric value with context-aware checks
///
/// Performs comprehensive validation including:
/// - Non-negative check
/// - Percentage bounds (0-100)
/// - Reasonable upper limits for latency and throughput
///
/// # Arguments
/// * `value` - The metric value to validate
/// * `operator` - The comparison operator (for context)
/// * `name` - The metric name (determines specific validation rules)
///
/// # Examples
/// ```
/// use graphica_coordinator::api::sos_validation::validators::validate_sla_metric_value;
///
/// assert!(validate_sla_metric_value(100.0, "<=", "latency_ms").is_ok());
/// assert!(validate_sla_metric_value(99.9, ">=", "availability_percent").is_ok());
/// assert!(validate_sla_metric_value(-1.0, "<=", "latency_ms").is_err());
/// assert!(validate_sla_metric_value(101.0, ">=", "cpu_percent").is_err());
/// ```
pub fn validate_sla_metric_value(value: f64, _operator: &str, name: &str) -> Result<()> {
    // Value must be non-negative
    if value < 0.0 {
        bail!("SLA metric value must be non-negative, got {}", value);
    }

    // Special validation for percentage metrics
    if name.ends_with("_percent") && value > 100.0 {
        bail!(
            "Percentage metric '{}' cannot exceed 100.0, got {}",
            name,
            value
        );
    }

    // Validate reasonable ranges for specific metric types
    match name {
        // Latency metrics should be reasonable (< 1 minute)
        "latency_ms" | "latency_p50_ms" | "latency_p95_ms" | "latency_p99_ms" | "jitter_ms" => {
            if value > 60000.0 {
                bail!(
                    "Latency metric '{}' exceeds reasonable maximum (60000ms/1 minute), got {}",
                    name,
                    value
                );
            }
        }

        // Throughput should be reasonable (< 1M requests per second)
        "throughput_rps" => {
            if value > 1_000_000.0 {
                bail!(
                    "Throughput exceeds reasonable maximum (1,000,000 rps), got {}",
                    value
                );
            }
        }

        // Bandwidth should be reasonable (< 100 Gbps)
        "bandwidth_mbps" => {
            if value > 100_000.0 {
                bail!(
                    "Bandwidth exceeds reasonable maximum (100,000 Mbps/100 Gbps), got {}",
                    value
                );
            }
        }

        // Disk I/O should be reasonable (< 10 Gbps)
        "disk_io_mbps" => {
            if value > 10_000.0 {
                bail!(
                    "Disk I/O exceeds reasonable maximum (10,000 Mbps/10 Gbps), got {}",
                    value
                );
            }
        }

        _ => {}
    }

    Ok(())
}

/// Validate a collection of SLA metrics
///
/// Performs batch validation on all metrics in a contract, ensuring:
/// - At least one metric is present
/// - All operators are valid
/// - All metric names are recognized
/// - All values are valid for their respective metrics
///
/// # Examples
/// ```
/// use graphica_coordinator::api::sos_validation::storage::SlaMetric;
/// use graphica_coordinator::api::sos_validation::validators::validate_sla_metrics;
///
/// let metrics = vec![
///     SlaMetric {
///         name: "latency_ms".to_string(),
///         value: 100.0,
///         operator: "<=".to_string(),
///         unit: Some("ms".to_string()),
///     },
/// ];
/// assert!(validate_sla_metrics(&metrics).is_ok());
/// ```
pub fn validate_sla_metrics(
    metrics: &[crate::api::sos_validation::storage::SlaMetric],
) -> Result<()> {
    if metrics.is_empty() {
        bail!("Contract must have at least one SLA metric");
    }

    for (idx, metric) in metrics.iter().enumerate() {
        // Validate operator
        if let Err(e) = validate_sla_operator(&metric.operator) {
            bail!("SLA metric #{}: {}", idx + 1, e);
        }

        // Validate metric name
        if let Err(e) = validate_sla_metric_name(&metric.name) {
            bail!("SLA metric #{}: {}", idx + 1, e);
        }

        // Validate metric value
        if let Err(e) = validate_sla_metric_value(metric.value, &metric.operator, &metric.name) {
            bail!("SLA metric #{}: {}", idx + 1, e);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Operator Validation Tests
    // ========================================================================

    #[test]
    fn test_valid_operators() {
        assert!(validate_sla_operator("<=").is_ok());
        assert!(validate_sla_operator(">=").is_ok());
        assert!(validate_sla_operator("==").is_ok());
        assert!(validate_sla_operator("<").is_ok());
        assert!(validate_sla_operator(">").is_ok());
        assert!(validate_sla_operator("!=").is_ok());
    }

    #[test]
    fn test_invalid_operator() {
        assert!(validate_sla_operator("~=").is_err());
        assert!(validate_sla_operator("===").is_err());
        assert!(validate_sla_operator("").is_err());
        assert!(validate_sla_operator("GREATER_THAN").is_err());
    }

    // ========================================================================
    // Metric Name Validation Tests
    // ========================================================================

    #[test]
    fn test_valid_latency_metrics() {
        assert!(validate_sla_metric_name("latency_ms").is_ok());
        assert!(validate_sla_metric_name("latency_p50_ms").is_ok());
        assert!(validate_sla_metric_name("latency_p95_ms").is_ok());
        assert!(validate_sla_metric_name("latency_p99_ms").is_ok());
        assert!(validate_sla_metric_name("jitter_ms").is_ok());
    }

    #[test]
    fn test_valid_throughput_metrics() {
        assert!(validate_sla_metric_name("throughput_rps").is_ok());
        assert!(validate_sla_metric_name("bandwidth_mbps").is_ok());
        assert!(validate_sla_metric_name("disk_io_mbps").is_ok());
    }

    #[test]
    fn test_valid_reliability_metrics() {
        assert!(validate_sla_metric_name("error_rate_percent").is_ok());
        assert!(validate_sla_metric_name("availability_percent").is_ok());
        assert!(validate_sla_metric_name("uptime_percent").is_ok());
        assert!(validate_sla_metric_name("packet_loss_percent").is_ok());
    }

    #[test]
    fn test_valid_resource_metrics() {
        assert!(validate_sla_metric_name("cpu_percent").is_ok());
        assert!(validate_sla_metric_name("memory_percent").is_ok());
    }

    #[test]
    fn test_invalid_metric_names() {
        assert!(validate_sla_metric_name("custom_metric").is_err());
        assert!(validate_sla_metric_name("invalid").is_err());
        assert!(validate_sla_metric_name("").is_err());
        assert!(validate_sla_metric_name("Latency_MS").is_err()); // Case sensitive
    }

    // ========================================================================
    // Metric Value Validation Tests
    // ========================================================================

    #[test]
    fn test_valid_latency_values() {
        assert!(validate_sla_metric_value(0.0, "<=", "latency_ms").is_ok());
        assert!(validate_sla_metric_value(100.0, "<=", "latency_ms").is_ok());
        assert!(validate_sla_metric_value(1000.0, "<=", "latency_p95_ms").is_ok());
        assert!(validate_sla_metric_value(59999.0, "<=", "latency_p99_ms").is_ok());
    }

    #[test]
    fn test_invalid_latency_values() {
        assert!(validate_sla_metric_value(-1.0, "<=", "latency_ms").is_err());
        assert!(validate_sla_metric_value(60001.0, "<=", "latency_ms").is_err());
        assert!(validate_sla_metric_value(100000.0, "<=", "jitter_ms").is_err());
    }

    #[test]
    fn test_valid_percentage_values() {
        assert!(validate_sla_metric_value(0.0, ">=", "availability_percent").is_ok());
        assert!(validate_sla_metric_value(50.0, ">=", "cpu_percent").is_ok());
        assert!(validate_sla_metric_value(99.9, ">=", "uptime_percent").is_ok());
        assert!(validate_sla_metric_value(100.0, ">=", "availability_percent").is_ok());
    }

    #[test]
    fn test_invalid_percentage_values() {
        assert!(validate_sla_metric_value(-1.0, ">=", "availability_percent").is_err());
        assert!(validate_sla_metric_value(101.0, ">=", "cpu_percent").is_err());
        assert!(validate_sla_metric_value(200.0, "<=", "error_rate_percent").is_err());
    }

    #[test]
    fn test_valid_throughput_values() {
        assert!(validate_sla_metric_value(100.0, ">=", "throughput_rps").is_ok());
        assert!(validate_sla_metric_value(10000.0, ">=", "throughput_rps").is_ok());
        assert!(validate_sla_metric_value(999999.0, ">=", "throughput_rps").is_ok());
    }

    #[test]
    fn test_invalid_throughput_values() {
        assert!(validate_sla_metric_value(-1.0, ">=", "throughput_rps").is_err());
        assert!(validate_sla_metric_value(1_000_001.0, ">=", "throughput_rps").is_err());
    }

    #[test]
    fn test_valid_bandwidth_values() {
        assert!(validate_sla_metric_value(100.0, ">=", "bandwidth_mbps").is_ok());
        assert!(validate_sla_metric_value(10000.0, ">=", "bandwidth_mbps").is_ok());
        assert!(validate_sla_metric_value(9999.0, ">=", "disk_io_mbps").is_ok());
    }

    #[test]
    fn test_invalid_bandwidth_values() {
        assert!(validate_sla_metric_value(-1.0, ">=", "bandwidth_mbps").is_err());
        assert!(validate_sla_metric_value(100_001.0, ">=", "bandwidth_mbps").is_err());
        assert!(validate_sla_metric_value(10_001.0, ">=", "disk_io_mbps").is_err());
    }

    // ========================================================================
    // Batch Validation Tests
    // ========================================================================

    #[test]
    fn test_validate_single_metric() {
        use crate::api::sos_validation::storage::SlaMetric;

        let metrics = vec![SlaMetric {
            name: "latency_ms".to_string(),
            value: 100.0,
            operator: "<=".to_string(),
            unit: Some("ms".to_string()),
        }];
        assert!(validate_sla_metrics(&metrics).is_ok());
    }

    #[test]
    fn test_validate_multiple_metrics() {
        use crate::api::sos_validation::storage::SlaMetric;

        let metrics = vec![
            SlaMetric {
                name: "latency_ms".to_string(),
                value: 100.0,
                operator: "<=".to_string(),
                unit: Some("ms".to_string()),
            },
            SlaMetric {
                name: "throughput_rps".to_string(),
                value: 1000.0,
                operator: ">=".to_string(),
                unit: Some("rps".to_string()),
            },
            SlaMetric {
                name: "availability_percent".to_string(),
                value: 99.9,
                operator: ">=".to_string(),
                unit: Some("percent".to_string()),
            },
        ];
        assert!(validate_sla_metrics(&metrics).is_ok());
    }

    #[test]
    fn test_empty_metrics_list() {
        assert!(validate_sla_metrics(&[]).is_err());
    }

    #[test]
    fn test_batch_validation_with_invalid_operator() {
        use crate::api::sos_validation::storage::SlaMetric;

        let metrics = vec![SlaMetric {
            name: "latency_ms".to_string(),
            value: 100.0,
            operator: "INVALID".to_string(),
            unit: Some("ms".to_string()),
        }];
        assert!(validate_sla_metrics(&metrics).is_err());
    }

    #[test]
    fn test_batch_validation_with_invalid_metric_name() {
        use crate::api::sos_validation::storage::SlaMetric;

        let metrics = vec![SlaMetric {
            name: "custom_metric".to_string(),
            value: 100.0,
            operator: "<=".to_string(),
            unit: Some("ms".to_string()),
        }];
        assert!(validate_sla_metrics(&metrics).is_err());
    }

    #[test]
    fn test_batch_validation_with_invalid_value() {
        use crate::api::sos_validation::storage::SlaMetric;

        let metrics = vec![SlaMetric {
            name: "latency_ms".to_string(),
            value: -100.0,
            operator: "<=".to_string(),
            unit: Some("ms".to_string()),
        }];
        assert!(validate_sla_metrics(&metrics).is_err());
    }

    #[test]
    fn test_batch_validation_identifies_specific_metric() {
        use crate::api::sos_validation::storage::SlaMetric;

        let metrics = vec![
            SlaMetric {
                name: "latency_ms".to_string(),
                value: 100.0,
                operator: "<=".to_string(),
                unit: Some("ms".to_string()),
            },
            SlaMetric {
                name: "throughput_rps".to_string(),
                value: -1.0, // Invalid!
                operator: ">=".to_string(),
                unit: Some("rps".to_string()),
            },
        ];

        let result = validate_sla_metrics(&metrics);
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("metric #2")); // Should identify second metric
    }
}
