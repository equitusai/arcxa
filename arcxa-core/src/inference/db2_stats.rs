//! DB2 Statistics Extractor
//!
//! Extracts column statistics from DB2 system catalog views for semantic type detection.
//!
//! **DB2 System Catalog Views:**
//! - `SYSCAT.COLUMNS` - Column metadata (nulls, length, type)
//! - `SYSCAT.COLDIST` - Column distribution statistics (cardinality, frequencies)
//! - `SYSCAT.TABLES` - Table metadata (row count)
//! - `SYSSTAT.COLUMNS` - Detailed statistics (null count, cardinality)
//!
//! **Architectural Note:**
//! This module is database-specific infrastructure. The extracted statistics
//! are converted to the database-agnostic `ColumnStatistics` type, which feeds
//! into the shared semantic detection framework.

use anyhow::Result;
use chrono::Utc;
use serde_json::Value as JsonValue;
use std::collections::HashMap;

use crate::inference::types::{CardinalityClass, ColumnStatistics};

/// DB2 statistics extractor
///
/// Extracts statistics from DB2 system catalog views and converts them
/// to the common ColumnStatistics format for semantic detection.
#[derive(Debug, Clone)]
pub struct Db2StatsExtractor {
    schema_name: String,
    table_name: String,
}

impl Db2StatsExtractor {
    /// Create a new DB2 statistics extractor
    pub fn new(schema_name: impl Into<String>, table_name: impl Into<String>) -> Self {
        Self {
            schema_name: schema_name.into(),
            table_name: table_name.into(),
        }
    }

    /// Build SQL query to extract column statistics from DB2 system catalogs
    ///
    /// **Query Strategy:**
    /// - Join SYSCAT.COLUMNS with SYSCAT.TABLES for base metadata
    /// - Join SYSSTAT.COLUMNS for detailed statistics
    /// - Join SYSCAT.COLDIST for distribution statistics
    ///
    /// **Returns:**
    /// - `colname` - Column name
    /// - `nulls` - Whether column allows nulls (Y/N)
    /// - `avgcollen` - Average column length
    /// - `colcard` - Number of distinct values
    /// - `card` - Total number of rows (from table)
    /// - `numnulls` - Number of null values
    /// - `high2key` - Second highest value (for distribution)
    /// - `low2key` - Second lowest value (for distribution)
    pub fn build_stats_query(&self) -> String {
        format!(
            r#"
SELECT
    c.colname,
    c.nulls,
    c.length,
    c.avgcollen,
    COALESCE(s.colcard, -1) as colcard,
    t.card as table_card,
    COALESCE(s.numnulls, 0) as numnulls,
    s.high2key,
    s.low2key,
    c.typename
FROM SYSCAT.COLUMNS c
JOIN SYSCAT.TABLES t
    ON c.tabschema = t.tabschema
    AND c.tabname = t.tabname
LEFT JOIN SYSSTAT.COLUMNS s
    ON c.tabschema = s.tabschema
    AND c.tabname = s.tabname
    AND c.colname = s.colname
WHERE c.tabschema = '{}'
    AND c.tabname = '{}'
ORDER BY c.colno
            "#,
            self.schema_name.to_uppercase(),
            self.table_name.to_uppercase()
        )
    }

    /// Parse column statistics from DB2 query result
    ///
    /// **Input Format:**
    /// DB2 result row as JSON with fields:
    /// - `colname`: Column name (VARCHAR)
    /// - `nulls`: 'Y' or 'N' (CHAR)
    /// - `avgcollen`: Average length (INTEGER)
    /// - `colcard`: Distinct count (BIGINT, -1 if no stats)
    /// - `table_card`: Total rows (BIGINT)
    /// - `numnulls`: Null count (BIGINT)
    /// - `typename`: DB2 type name (VARCHAR)
    ///
    /// **Cardinality Calculation:**
    /// - DB2 uses `colcard` (column cardinality) for distinct count
    /// - Ratio: colcard / table_card
    /// - Special case: colcard = -1 means statistics not collected
    pub fn parse_column_statistics(
        &self,
        row: &HashMap<String, JsonValue>,
        total_rows: Option<u64>,
    ) -> Result<ColumnStatistics> {
        // Extract average column length
        let avg_width = row
            .get("avgcollen")
            .and_then(|v| v.as_i64())
            .map(|v| v as i32);

        // Extract distinct count (colcard)
        // DB2 uses -1 to indicate no statistics collected
        let distinct_count = row
            .get("colcard")
            .and_then(|v| v.as_i64())
            .filter(|&v| v >= 0) // Filter out -1 (no stats)
            .map(|v| v as u64);

        // Total rows from table statistics
        let table_rows = total_rows.or_else(|| {
            row.get("table_card")
                .and_then(|v| v.as_i64())
                .filter(|&v| v > 0)
                .map(|v| v as u64)
        });

        // Extract null count
        let null_count = row
            .get("numnulls")
            .and_then(|v| v.as_i64())
            .map(|v| v as u64)
            .unwrap_or(0);

        // Calculate null percentage
        let null_percentage = if let Some(total) = table_rows {
            if total > 0 {
                (null_count as f64 / total as f64) * 100.0
            } else {
                0.0
            }
        } else {
            0.0
        };

        // Calculate cardinality ratio for classification
        let cardinality_ratio = if let (Some(distinct), Some(total)) = (distinct_count, table_rows)
        {
            if total > 0 {
                Some(distinct as f64 / total as f64)
            } else {
                None
            }
        } else {
            None
        };

        // Classify cardinality
        let cardinality = cardinality_ratio.map(Self::classify_cardinality);

        Ok(ColumnStatistics {
            distinct_count,
            null_count,
            null_percentage,
            min_value: None, // Would need SYSCAT.COLDIST
            max_value: None, // Would need SYSCAT.COLDIST
            avg_length: avg_width.map(|w| w as f64),
            histogram: None,          // Would need SYSCAT.COLDIST
            most_common_values: None, // Would need SYSCAT.COLDIST
            correlation: None,        // DB2 doesn't track correlation like PostgreSQL
            n_distinct: distinct_count.map(|d| d as f64),
            avg_width,
            cardinality,
            sample_size: table_rows,
            last_analyzed: Some(Utc::now()),
            statistics_stale: distinct_count.is_none(), // If no stats, mark as stale
        })
    }

    /// Classify cardinality ratio into semantic categories
    ///
    /// **Classification Thresholds:**
    /// - `VeryLow`: < 0.01 (< 1% distinct)
    /// - `Low`: 0.01 - 0.10 (1-10% distinct)
    /// - `Medium`: 0.10 - 0.50 (10-50% distinct)
    /// - `High`: 0.50 - 0.95 (50-95% distinct)
    /// - `VeryHigh`: 0.95 - 1.00 (95-100% distinct)
    /// - `Unique`: = 1.00 (100% distinct)
    pub fn classify_cardinality(ratio: f64) -> CardinalityClass {
        match ratio {
            r if r >= 1.0 => CardinalityClass::Unique,
            r if r >= 0.95 => CardinalityClass::VeryHigh,
            r if r >= 0.50 => CardinalityClass::High,
            r if r >= 0.10 => CardinalityClass::Medium,
            r if r >= 0.01 => CardinalityClass::Low,
            _ => CardinalityClass::VeryLow,
        }
    }

    /// Build query to extract most common values from SYSCAT.COLDIST
    ///
    /// **DB2 COLDIST Structure:**
    /// - `SEQNO` - Sequence number for ordering
    /// - `COLVALUE` - The value (as string)
    /// - `VALCOUNT` - Frequency of this value
    /// - `DISTCOUNT` - Number of distinct values in this quantile
    ///
    /// **Note:** DB2 stores distribution statistics as quantiles, not exact frequencies.
    /// This is different from PostgreSQL's most_common_vals.
    pub fn build_coldist_query(&self, column_name: &str) -> String {
        format!(
            r#"
SELECT
    COLVALUE,
    VALCOUNT,
    DISTCOUNT
FROM SYSCAT.COLDIST
WHERE TABSCHEMA = '{}'
    AND TABNAME = '{}'
    AND COLNAME = '{}'
ORDER BY SEQNO
FETCH FIRST 10 ROWS ONLY
            "#,
            self.schema_name.to_uppercase(),
            self.table_name.to_uppercase(),
            column_name.to_uppercase()
        )
    }

    /// Parse most common values from SYSCAT.COLDIST results
    ///
    /// **Returns:** Vector of (value, frequency) tuples
    pub fn parse_coldist_results(&self, rows: &[HashMap<String, JsonValue>]) -> Vec<(String, f64)> {
        rows.iter()
            .filter_map(|row| {
                let value = row.get("COLVALUE")?.as_str()?.to_string();
                let count = row.get("VALCOUNT")?.as_i64()? as f64;
                Some((value, count))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_stats_query() {
        let extractor = Db2StatsExtractor::new("MYSCHEMA", "CUSTOMERS");
        let query = extractor.build_stats_query();

        assert!(query.contains("SYSCAT.COLUMNS"));
        assert!(query.contains("SYSSTAT.COLUMNS"));
        assert!(query.contains("SYSCAT.TABLES"));
        assert!(query.contains("MYSCHEMA"));
        assert!(query.contains("CUSTOMERS"));
        assert!(query.contains("colcard"));
        assert!(query.contains("numnulls"));
    }

    #[test]
    fn test_classify_cardinality() {
        assert_eq!(
            Db2StatsExtractor::classify_cardinality(1.0),
            CardinalityClass::Unique
        );
        assert_eq!(
            Db2StatsExtractor::classify_cardinality(0.97),
            CardinalityClass::VeryHigh
        );
        assert_eq!(
            Db2StatsExtractor::classify_cardinality(0.70),
            CardinalityClass::High
        );
        assert_eq!(
            Db2StatsExtractor::classify_cardinality(0.30),
            CardinalityClass::Medium
        );
        assert_eq!(
            Db2StatsExtractor::classify_cardinality(0.05),
            CardinalityClass::Low
        );
        assert_eq!(
            Db2StatsExtractor::classify_cardinality(0.005),
            CardinalityClass::VeryLow
        );
    }

    #[test]
    fn test_parse_column_statistics() {
        let extractor = Db2StatsExtractor::new("TESTDB", "USERS");

        let mut row = HashMap::new();
        row.insert(
            "colname".to_string(),
            JsonValue::String("EMAIL".to_string()),
        );
        row.insert("nulls".to_string(), JsonValue::String("Y".to_string()));
        row.insert("avgcollen".to_string(), JsonValue::Number(50.into()));
        row.insert("colcard".to_string(), JsonValue::Number(9500.into()));
        row.insert("table_card".to_string(), JsonValue::Number(10000.into()));
        row.insert("numnulls".to_string(), JsonValue::Number(100.into()));

        let stats = extractor
            .parse_column_statistics(&row, Some(10000))
            .unwrap();

        assert_eq!(stats.distinct_count, Some(9500));
        assert_eq!(stats.null_count, 100);
        assert_eq!(stats.null_percentage, 1.0); // 100/10000 = 1%
        assert_eq!(stats.avg_width, Some(50));
        assert_eq!(stats.cardinality, Some(CardinalityClass::VeryHigh)); // 95% distinct
    }

    #[test]
    fn test_parse_column_statistics_no_stats() {
        let extractor = Db2StatsExtractor::new("TESTDB", "USERS");

        let mut row = HashMap::new();
        row.insert("colname".to_string(), JsonValue::String("ID".to_string()));
        row.insert("nulls".to_string(), JsonValue::String("N".to_string()));
        row.insert("colcard".to_string(), JsonValue::Number((-1).into())); // No stats
        row.insert("table_card".to_string(), JsonValue::Number(10000.into()));
        row.insert("numnulls".to_string(), JsonValue::Number(0.into()));

        let stats = extractor
            .parse_column_statistics(&row, Some(10000))
            .unwrap();

        assert_eq!(stats.distinct_count, None); // Filtered out -1
        assert_eq!(stats.null_count, 0);
        assert_eq!(stats.null_percentage, 0.0);
        assert_eq!(stats.cardinality, None); // No distinct count
        assert!(stats.statistics_stale); // Should be marked as stale
    }

    #[test]
    fn test_parse_column_statistics_high_cardinality() {
        let extractor = Db2StatsExtractor::new("TESTDB", "TRANSACTIONS");

        let mut row = HashMap::new();
        row.insert(
            "colname".to_string(),
            JsonValue::String("TRANSACTION_ID".to_string()),
        );
        row.insert("nulls".to_string(), JsonValue::String("N".to_string()));
        row.insert("avgcollen".to_string(), JsonValue::Number(36.into()));
        row.insert("colcard".to_string(), JsonValue::Number(1000000.into()));
        row.insert("table_card".to_string(), JsonValue::Number(1000000.into()));
        row.insert("numnulls".to_string(), JsonValue::Number(0.into()));

        let stats = extractor
            .parse_column_statistics(&row, Some(1000000))
            .unwrap();

        assert_eq!(stats.distinct_count, Some(1000000));
        assert_eq!(stats.cardinality, Some(CardinalityClass::Unique)); // Perfect uniqueness (100%)
    }

    #[test]
    fn test_build_coldist_query() {
        let extractor = Db2StatsExtractor::new("SALES", "ORDERS");
        let query = extractor.build_coldist_query("STATUS");

        assert!(query.contains("SYSCAT.COLDIST"));
        assert!(query.contains("SALES"));
        assert!(query.contains("ORDERS"));
        assert!(query.contains("STATUS"));
        assert!(query.contains("COLVALUE"));
        assert!(query.contains("VALCOUNT"));
        assert!(query.contains("FETCH FIRST 10"));
    }

    #[test]
    fn test_parse_coldist_results() {
        let extractor = Db2StatsExtractor::new("SALES", "ORDERS");

        let mut row1 = HashMap::new();
        row1.insert(
            "COLVALUE".to_string(),
            JsonValue::String("ACTIVE".to_string()),
        );
        row1.insert("VALCOUNT".to_string(), JsonValue::Number(5000.into()));

        let mut row2 = HashMap::new();
        row2.insert(
            "COLVALUE".to_string(),
            JsonValue::String("INACTIVE".to_string()),
        );
        row2.insert("VALCOUNT".to_string(), JsonValue::Number(3000.into()));

        let rows = vec![row1, row2];
        let results = extractor.parse_coldist_results(&rows);

        assert_eq!(results.len(), 2);
        assert_eq!(results[0], ("ACTIVE".to_string(), 5000.0));
        assert_eq!(results[1], ("INACTIVE".to_string(), 3000.0));
    }
}
