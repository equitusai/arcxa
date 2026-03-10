//! PostgreSQL-specific statistics extraction from pg_stats
//!
//! This module extracts rich statistical metadata directly from PostgreSQL's
//! pg_stats system view, which provides query planner statistics.
//!
//! ## Features
//! - Exact distinct counts (n_distinct)
//! - Most common values and their frequencies
//! - Histogram bounds for distribution analysis
//! - Correlation coefficients
//! - Null fraction and average width
//!
//! ## Performance
//! Reading from pg_stats is fast as it's a view over pg_statistic with
//! pre-computed statistics from ANALYZE.

use crate::inference::types::*;
use anyhow::{Context, Result};
use serde_json::Value as JsonValue;
use std::collections::HashMap;

/// PostgreSQL statistics extractor (pg_stats view)
///
/// NOTE: This implementation uses string-based queries rather than sqlx
/// to avoid adding the sqlx dependency. Actual database access should be
/// implemented in the connector layer.
#[derive(Debug, Clone)]
pub struct PostgresStatsExtractor {
    /// Table schema
    pub schema_name: String,
    /// Table name
    pub table_name: String,
}

impl PostgresStatsExtractor {
    /// Create new PostgreSQL stats extractor
    pub fn new(schema_name: impl Into<String>, table_name: impl Into<String>) -> Self {
        Self {
            schema_name: schema_name.into(),
            table_name: table_name.into(),
        }
    }

    /// Build query to extract column statistics from pg_stats
    pub fn build_stats_query(&self) -> String {
        format!(
            r#"
SELECT
    schemaname,
    tablename,
    attname AS column_name,
    inherited,
    null_frac,
    avg_width,
    n_distinct,
    most_common_vals::text AS most_common_vals_text,
    most_common_freqs,
    histogram_bounds::text AS histogram_bounds_text,
    correlation,
    most_common_elems::text AS most_common_elems_text,
    most_common_elem_freqs,
    elem_count_histogram
FROM pg_stats
WHERE schemaname = '{}'
  AND tablename = '{}'
ORDER BY attname
            "#,
            self.schema_name, self.table_name
        )
    }

    /// Build query to get table row count
    pub fn build_row_count_query(&self) -> String {
        format!(
            r#"
SELECT reltuples::bigint AS estimated_rows,
       relpages AS page_count,
       relallvisible AS all_visible_pages
FROM pg_class c
JOIN pg_namespace n ON n.oid = c.relnamespace
WHERE n.nspname = '{}'
  AND c.relname = '{}'
            "#,
            self.schema_name, self.table_name
        )
    }

    /// Parse pg_stats results into ColumnStatistics
    ///
    /// This method processes raw query results from pg_stats and constructs
    /// the enhanced ColumnStatistics structure.
    pub fn parse_column_statistics(
        &self,
        row: &HashMap<String, JsonValue>,
        total_rows: Option<u64>,
    ) -> Result<ColumnStatistics> {
        // Extract basic statistics
        let null_frac = row.get("null_frac").and_then(|v| v.as_f64()).unwrap_or(0.0);

        let avg_width = row
            .get("avg_width")
            .and_then(|v| v.as_i64())
            .map(|v| v as i32);

        let n_distinct = row.get("n_distinct").and_then(|v| v.as_f64());

        let correlation = row.get("correlation").and_then(|v| v.as_f64());

        // Calculate distinct count
        let distinct_count = if let Some(n_dist) = n_distinct {
            if let Some(rows) = total_rows {
                if n_dist < 0.0 {
                    // Negative n_distinct means ratio of distinct values
                    Some((rows as f64 * n_dist.abs()) as u64)
                } else {
                    // Positive n_distinct is actual count
                    Some(n_dist as u64)
                }
            } else {
                None
            }
        } else {
            None
        };

        // Calculate null count
        let null_count = if let Some(rows) = total_rows {
            (rows as f64 * null_frac) as u64
        } else {
            0
        };

        // Parse most common values
        let most_common_values = Self::parse_most_common_values(row)?;

        // Parse histogram bounds
        let histogram = Self::parse_histogram(row)?;

        // Determine cardinality class
        let cardinality = distinct_count
            .and_then(|dc| total_rows.map(|rows| Self::classify_cardinality(dc, rows)));

        Ok(ColumnStatistics {
            distinct_count,
            null_count,
            null_percentage: null_frac * 100.0,
            min_value: None, // Can be extracted from histogram_bounds[0]
            max_value: None, // Can be extracted from histogram_bounds[last]
            avg_length: None,
            histogram,
            most_common_values,
            correlation,
            n_distinct,
            avg_width,
            cardinality,
            sample_size: total_rows,
            last_analyzed: None, // Would need to query pg_stat_all_tables
            statistics_stale: false,
        })
    }

    /// Parse most common values from pg_stats
    fn parse_most_common_values(
        row: &HashMap<String, JsonValue>,
    ) -> Result<Option<Vec<ValueFrequency>>> {
        let vals_text = row.get("most_common_vals_text").and_then(|v| v.as_str());

        let freqs = row.get("most_common_freqs").and_then(|v| v.as_array());

        if let (Some(vals_str), Some(freq_arr)) = (vals_text, freqs) {
            // Parse PostgreSQL array format: {val1,val2,val3}
            let values = Self::parse_pg_array(vals_str)?;

            let frequencies: Vec<f64> = freq_arr.iter().filter_map(|f| f.as_f64()).collect();

            if values.len() == frequencies.len() {
                let mut result = Vec::new();
                for (val, freq) in values.into_iter().zip(frequencies.into_iter()) {
                    result.push(ValueFrequency {
                        value: val,
                        count: 0, // We only have frequency, not absolute count
                        percentage: freq * 100.0,
                    });
                }
                return Ok(Some(result));
            }
        }

        Ok(None)
    }

    /// Parse histogram bounds from pg_stats
    fn parse_histogram(row: &HashMap<String, JsonValue>) -> Result<Option<Histogram>> {
        let bounds_text = row.get("histogram_bounds_text").and_then(|v| v.as_str());

        if let Some(bounds_str) = bounds_text {
            let bounds = Self::parse_pg_array(bounds_str)?;

            if bounds.len() >= 2 {
                let mut buckets = Vec::new();

                // PostgreSQL histogram has N+1 bounds for N buckets
                for i in 0..bounds.len() - 1 {
                    buckets.push(HistogramBucket {
                        lower_bound: bounds[i].clone(),
                        upper_bound: bounds[i + 1].clone(),
                        frequency: 0, // pg_stats doesn't provide bucket frequencies directly
                        distinct_count: 0,
                    });
                }

                return Ok(Some(Histogram {
                    buckets,
                    method: HistogramMethod::EquiDepth, // PostgreSQL uses equi-depth
                }));
            }
        }

        Ok(None)
    }

    /// Parse PostgreSQL array notation: {val1,val2,val3}
    ///
    /// This is a simplified parser that handles basic cases.
    /// Production implementation should use proper PostgreSQL array parsing.
    fn parse_pg_array(array_str: &str) -> Result<Vec<String>> {
        let trimmed = array_str.trim();

        if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
            return Ok(Vec::new());
        }

        let inner = &trimmed[1..trimmed.len() - 1];

        if inner.is_empty() {
            return Ok(Vec::new());
        }

        // Simple split - doesn't handle quoted commas
        // TODO: Implement proper PostgreSQL array parser
        let values: Vec<String> = inner
            .split(',')
            .map(|s| s.trim().trim_matches('"').to_string())
            .collect();

        Ok(values)
    }

    /// Classify cardinality based on distinct count and total rows
    fn classify_cardinality(distinct_count: u64, total_rows: u64) -> CardinalityClass {
        let ratio = distinct_count as f64 / total_rows as f64;

        if ratio > 0.95 {
            CardinalityClass::Unique
        } else if distinct_count > 100_000 {
            CardinalityClass::VeryHigh
        } else if distinct_count > 1_000 {
            CardinalityClass::High
        } else if distinct_count > 100 {
            CardinalityClass::Medium
        } else if distinct_count > 10 {
            CardinalityClass::Low
        } else {
            CardinalityClass::VeryLow
        }
    }

    /// Extract min/max values from histogram bounds
    pub fn extract_min_max_from_histogram(
        histogram: &Option<Histogram>,
    ) -> (Option<String>, Option<String>) {
        if let Some(hist) = histogram {
            if let Some(first_bucket) = hist.buckets.first() {
                if let Some(last_bucket) = hist.buckets.last() {
                    return (
                        Some(first_bucket.lower_bound.clone()),
                        Some(last_bucket.upper_bound.clone()),
                    );
                }
            }
        }
        (None, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_stats_query() {
        let extractor = PostgresStatsExtractor::new("public".to_string(), "customers".to_string());

        let query = extractor.build_stats_query();
        assert!(query.contains("pg_stats"));
        assert!(query.contains("public"));
        assert!(query.contains("customers"));
    }

    #[test]
    fn test_parse_pg_array_simple() {
        let result = PostgresStatsExtractor::parse_pg_array("{1,2,3}").unwrap();
        assert_eq!(result, vec!["1", "2", "3"]);
    }

    #[test]
    fn test_parse_pg_array_empty() {
        let result = PostgresStatsExtractor::parse_pg_array("{}").unwrap();
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_parse_pg_array_strings() {
        let result =
            PostgresStatsExtractor::parse_pg_array(r#"{"Alice","Bob","Charlie"}"#).unwrap();
        assert_eq!(result, vec!["Alice", "Bob", "Charlie"]);
    }

    #[test]
    fn test_classify_cardinality() {
        assert_eq!(
            PostgresStatsExtractor::classify_cardinality(5, 1000),
            CardinalityClass::VeryLow
        );
        assert_eq!(
            PostgresStatsExtractor::classify_cardinality(50, 1000),
            CardinalityClass::Low
        );
        assert_eq!(
            PostgresStatsExtractor::classify_cardinality(500, 1000),
            CardinalityClass::Medium
        );
        assert_eq!(
            PostgresStatsExtractor::classify_cardinality(5000, 10000),
            CardinalityClass::High
        );
        assert_eq!(
            PostgresStatsExtractor::classify_cardinality(200000, 1000000),
            CardinalityClass::VeryHigh
        );
        assert_eq!(
            PostgresStatsExtractor::classify_cardinality(980, 1000),
            CardinalityClass::Unique
        );
    }

    #[test]
    fn test_parse_column_statistics() {
        let extractor = PostgresStatsExtractor::new("public".to_string(), "test".to_string());

        let mut row = HashMap::new();
        row.insert("null_frac".to_string(), JsonValue::from(0.05));
        row.insert("avg_width".to_string(), JsonValue::from(25));
        row.insert("n_distinct".to_string(), JsonValue::from(100.0));
        row.insert("correlation".to_string(), JsonValue::from(0.85));

        let stats = extractor
            .parse_column_statistics(&row, Some(10000))
            .unwrap();

        assert_eq!(stats.distinct_count, Some(100));
        assert_eq!(stats.null_count, 500); // 0.05 * 10000
        assert_eq!(stats.null_percentage, 5.0);
        assert_eq!(stats.avg_width, Some(25));
        assert_eq!(stats.correlation, Some(0.85));
        assert_eq!(stats.cardinality, Some(CardinalityClass::Low)); // 100 is Low (11-100)
    }
}
