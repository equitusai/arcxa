//! Snowflake Statistics Extractor
//!
//! Extracts column statistics from Snowflake's system views and leverages
//! Snowflake-specific features for enhanced semantic detection.
//!
//! **Snowflake System Views:**
//! - `INFORMATION_SCHEMA.COLUMNS` - Column metadata
//! - `ACCOUNT_USAGE.TABLES` - Table-level statistics
//! - `ACCOUNT_USAGE.COLUMNS` - Column-level statistics (requires ACCOUNTADMIN or IMPORTED PRIVILEGES)
//! - `INFORMATION_SCHEMA.AUTOMATIC_CLUSTERING_INFORMATION` - Clustering key information
//! - `INFORMATION_SCHEMA.TABLE_STORAGE_METRICS` - Micro-partition statistics
//! - `INFORMATION_SCHEMA.SEARCH_OPTIMIZATION` - Search optimization status
//!
//! **Snowflake-Specific Features Leveraged:**
//! - **Automatic Clustering**: Detect high-cardinality clustering keys
//! - **Micro-Partitions**: Use partition metadata for cardinality estimation
//! - **Search Optimization**: Identify search-optimized columns (likely high-value identifiers)
//! - **Null Count**: Direct null_count from metadata (more accurate than sampling)
//!
//! **Architectural Note:**
//! This module is database-specific infrastructure. The extracted statistics
//! are converted to the database-agnostic `ColumnStatistics` type, with
//! Snowflake-specific features enhancing the semantic detection confidence.

use anyhow::Result;
use chrono::Utc;
use serde_json::Value as JsonValue;
use std::collections::HashMap;

use crate::inference::types::{CardinalityClass, ColumnStatistics};

/// Snowflake statistics extractor
///
/// Extracts statistics from Snowflake system views and leverages
/// Snowflake-specific features like automatic clustering and search optimization.
#[derive(Debug, Clone)]
pub struct SnowflakeStatsExtractor {
    schema_name: String,
    table_name: String,
    database_name: Option<String>,
}

impl SnowflakeStatsExtractor {
    /// Create a new Snowflake statistics extractor
    pub fn new(schema: impl Into<String>, table: impl Into<String>) -> Self {
        Self {
            schema_name: schema.into(),
            table_name: table.into(),
            database_name: None,
        }
    }

    /// Create with explicit database name
    pub fn with_database(
        database: impl Into<String>,
        schema: impl Into<String>,
        table: impl Into<String>,
    ) -> Self {
        Self {
            schema_name: schema.into(),
            table_name: table.into(),
            database_name: Some(database.into()),
        }
    }

    /// Build SQL query to extract column statistics from Snowflake
    ///
    /// **Query Strategy:**
    /// - Use INFORMATION_SCHEMA.COLUMNS for basic metadata
    /// - Join with TABLE_STORAGE_METRICS for micro-partition stats
    /// - Join with AUTOMATIC_CLUSTERING_INFORMATION for clustering keys
    /// - Join with SEARCH_OPTIMIZATION for search-optimized columns
    ///
    /// **Returns:**
    /// - `column_name` - Column name
    /// - `data_type` - Snowflake data type
    /// - `is_nullable` - YES/NO
    /// - `character_maximum_length` - For string types
    /// - `numeric_precision` - For numeric types
    /// - `is_identity` - Whether column is IDENTITY
    /// - `clustering_depth` - Clustering depth (0-100, lower is better)
    /// - `is_clustering_key` - Whether column is in clustering key
    /// - `is_search_optimized` - Whether column has search optimization
    /// - `active_bytes` - Active bytes in micro-partitions
    /// - `time_travel_bytes` - Time travel bytes
    pub fn build_stats_query(&self) -> String {
        let db_prefix = self
            .database_name
            .as_ref()
            .map(|db| format!("{}.", db))
            .unwrap_or_default();

        format!(
            r#"
SELECT
    c.column_name,
    c.data_type,
    c.is_nullable,
    c.character_maximum_length,
    c.numeric_precision,
    c.is_identity,
    c.comment,
    -- Clustering information (NULL if not clustered)
    aci.clustering_depth,
    CASE WHEN aci.table_name IS NOT NULL THEN TRUE ELSE FALSE END AS is_clustering_key,
    -- Search optimization (NULL if not optimized)
    so.search_optimization_status,
    CASE WHEN so.table_name IS NOT NULL THEN TRUE ELSE FALSE END AS is_search_optimized,
    -- Micro-partition statistics
    tsm.active_bytes,
    tsm.time_travel_bytes,
    tsm.failsafe_bytes,
    tsm.retained_for_clone_bytes
FROM {db_prefix}INFORMATION_SCHEMA.COLUMNS c
LEFT JOIN {db_prefix}INFORMATION_SCHEMA.AUTOMATIC_CLUSTERING_INFORMATION aci
    ON c.table_schema = aci.schema_name
    AND c.table_name = aci.table_name
    AND c.column_name IN (
        SELECT TRIM(VALUE)
        FROM TABLE(SPLIT_TO_TABLE(aci.clustering_key, ','))
    )
LEFT JOIN {db_prefix}INFORMATION_SCHEMA.SEARCH_OPTIMIZATION so
    ON c.table_schema = so.schema_name
    AND c.table_name = so.table_name
LEFT JOIN {db_prefix}INFORMATION_SCHEMA.TABLE_STORAGE_METRICS tsm
    ON c.table_schema = tsm.schema_name
    AND c.table_name = tsm.table_name
WHERE c.table_schema = '{schema}'
    AND c.table_name = '{table}'
ORDER BY c.ordinal_position
            "#,
            db_prefix = db_prefix,
            schema = self.schema_name.to_uppercase(),
            table = self.table_name.to_uppercase()
        )
    }

    /// Build query to extract detailed column statistics from ACCOUNT_USAGE
    ///
    /// **Note:** Requires ACCOUNTADMIN role or IMPORTED PRIVILEGES on SNOWFLAKE database.
    /// This provides more detailed statistics including distinct counts and null counts.
    ///
    /// **Latency:** ACCOUNT_USAGE views have 45-minute to 3-hour latency.
    /// Use INFORMATION_SCHEMA for real-time metadata.
    pub fn build_account_usage_stats_query(&self) -> String {
        let db_prefix = self
            .database_name
            .as_ref()
            .map(|db| format!("{}.", db))
            .unwrap_or_else(|| "SNOWFLAKE.".to_string());

        format!(
            r#"
SELECT
    c.column_name,
    c.distinct_count,
    c.null_count,
    c.average_length,
    c.max_length,
    t.row_count,
    t.bytes,
    t.last_altered
FROM {db_prefix}ACCOUNT_USAGE.COLUMNS c
JOIN {db_prefix}ACCOUNT_USAGE.TABLES t
    ON c.table_schema = t.table_schema
    AND c.table_name = t.table_name
    AND c.table_catalog = t.table_catalog
WHERE c.table_schema = '{schema}'
    AND c.table_name = '{table}'
    AND c.deleted IS NULL
ORDER BY c.ordinal_position
            "#,
            db_prefix = db_prefix,
            schema = self.schema_name.to_uppercase(),
            table = self.table_name.to_uppercase()
        )
    }

    /// Parse column statistics from Snowflake query result
    ///
    /// **Input Format:**
    /// Snowflake result row as JSON with fields:
    /// - `column_name`: Column name (VARCHAR)
    /// - `is_nullable`: 'YES' or 'NO' (VARCHAR)
    /// - `character_maximum_length`: Max length for strings (NUMBER)
    /// - `is_clustering_key`: Whether column is clustering key (BOOLEAN)
    /// - `clustering_depth`: Clustering depth 0-100 (NUMBER)
    /// - `is_search_optimized`: Whether column has search optimization (BOOLEAN)
    /// - `active_bytes`: Active bytes in micro-partitions (NUMBER)
    ///
    /// **Snowflake-Specific Enhancements:**
    /// - **Clustering keys** with low depth → likely unique or high-cardinality
    /// - **Search-optimized columns** → likely identifiers or frequently queried
    /// - **IDENTITY columns** → automatically unique
    pub fn parse_column_statistics(
        &self,
        row: &HashMap<String, JsonValue>,
        total_rows: Option<u64>,
    ) -> Result<ColumnStatistics> {
        // Extract basic nullable information
        let is_nullable = row
            .get("is_nullable")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_uppercase() == "YES")
            .unwrap_or(true);

        // Extract average width (from character_maximum_length or numeric_precision)
        let avg_width = row
            .get("character_maximum_length")
            .and_then(|v| v.as_i64())
            .or_else(|| row.get("numeric_precision").and_then(|v| v.as_i64()))
            .map(|v| v as i32);

        // Check if column is IDENTITY (automatically unique)
        let is_identity = row
            .get("is_identity")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_uppercase() == "YES")
            .unwrap_or(false);

        // Check if column is a clustering key
        let is_clustering_key = row
            .get("is_clustering_key")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // Get clustering depth (0-100, lower is better organized)
        let clustering_depth = row.get("clustering_depth").and_then(|v| v.as_f64());

        // Check if column is search-optimized
        let is_search_optimized = row
            .get("is_search_optimized")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // Initialize base statistics
        let mut distinct_count: Option<u64> = None;
        let mut null_count: u64 = 0;
        let mut null_percentage: f64 = 0.0;

        // Try to get statistics from ACCOUNT_USAGE if available
        if let Some(distinct) = row.get("distinct_count").and_then(|v| v.as_i64()) {
            if distinct >= 0 {
                distinct_count = Some(distinct as u64);
            }
        }

        if let Some(nulls) = row.get("null_count").and_then(|v| v.as_i64()) {
            null_count = nulls as u64;
        }

        // Calculate null percentage
        if let Some(total) = total_rows {
            if total > 0 {
                null_percentage = (null_count as f64 / total as f64) * 100.0;
            }
        } else if let Some(row_count) = row.get("row_count").and_then(|v| v.as_i64()) {
            let total = row_count as u64;
            if total > 0 {
                null_percentage = (null_count as f64 / total as f64) * 100.0;
            }
        }

        // Infer cardinality from Snowflake-specific features
        let cardinality = self.infer_cardinality_from_snowflake_features(
            is_identity,
            is_clustering_key,
            clustering_depth,
            is_search_optimized,
            distinct_count,
            total_rows,
        );

        // Extract average length from ACCOUNT_USAGE if available
        let avg_length = row
            .get("average_length")
            .and_then(|v| v.as_f64())
            .or_else(|| avg_width.map(|w| w as f64));

        Ok(ColumnStatistics {
            distinct_count,
            null_count,
            null_percentage,
            min_value: None, // Would need separate query
            max_value: None, // Would need separate query
            avg_length,
            histogram: None,          // Snowflake doesn't expose histograms
            most_common_values: None, // Would need separate query
            correlation: None,        // Snowflake doesn't track correlation
            n_distinct: distinct_count.map(|d| d as f64),
            avg_width,
            cardinality,
            sample_size: total_rows,
            last_analyzed: Some(Utc::now()),
            statistics_stale: distinct_count.is_none(), // If no stats, mark as stale
        })
    }

    /// Infer cardinality from Snowflake-specific features
    ///
    /// **Snowflake-Specific Heuristics:**
    /// - **IDENTITY columns**: Always Unique
    /// - **Clustering keys with depth < 10**: Likely High or VeryHigh cardinality
    /// - **Search-optimized columns**: Likely High or VeryHigh cardinality
    /// - **Clustering keys with depth > 50**: Likely Low or Medium cardinality
    fn infer_cardinality_from_snowflake_features(
        &self,
        is_identity: bool,
        is_clustering_key: bool,
        clustering_depth: Option<f64>,
        is_search_optimized: bool,
        distinct_count: Option<u64>,
        total_rows: Option<u64>,
    ) -> Option<CardinalityClass> {
        // IDENTITY columns are always unique
        if is_identity {
            return Some(CardinalityClass::Unique);
        }

        // If we have actual distinct count, use it
        if let (Some(distinct), Some(total)) = (distinct_count, total_rows) {
            if total > 0 {
                let ratio = distinct as f64 / total as f64;
                return Some(Self::classify_cardinality(ratio));
            }
        }

        // Infer from clustering key characteristics
        if is_clustering_key {
            if let Some(depth) = clustering_depth {
                // Low clustering depth (< 10) suggests high cardinality
                // High clustering depth (> 50) suggests low cardinality
                return Some(match depth {
                    d if d < 10.0 => CardinalityClass::VeryHigh,
                    d if d < 30.0 => CardinalityClass::High,
                    d if d < 60.0 => CardinalityClass::Medium,
                    _ => CardinalityClass::Low,
                });
            } else {
                // Clustering key without depth info → assume high cardinality
                return Some(CardinalityClass::High);
            }
        }

        // Search-optimized columns are typically high-cardinality identifiers
        if is_search_optimized {
            return Some(CardinalityClass::VeryHigh);
        }

        None
    }

    /// Classify cardinality ratio into semantic categories
    ///
    /// Same thresholds as PostgreSQL and DB2 for consistency.
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

    /// Build query to get top N most common values for a column
    ///
    /// Uses APPROX_TOP_K for efficient approximate top-K queries.
    /// This is a Snowflake-specific optimization.
    pub fn build_top_values_query(&self, column_name: &str, limit: usize) -> String {
        let db_prefix = self
            .database_name
            .as_ref()
            .map(|db| format!("{}.", db))
            .unwrap_or_default();

        format!(
            r#"
SELECT
    value,
    COUNT(*) AS frequency
FROM {db_prefix}{schema}.{table}
WHERE {column} IS NOT NULL
GROUP BY {column}
ORDER BY frequency DESC
LIMIT {limit}
            "#,
            db_prefix = db_prefix,
            schema = self.schema_name.to_uppercase(),
            table = self.table_name.to_uppercase(),
            column = column_name.to_uppercase(),
            limit = limit
        )
    }

    /// Build query using Snowflake's APPROX_TOP_K for efficient top-K
    ///
    /// **Snowflake-Specific Feature:**
    /// APPROX_TOP_K uses HyperLogLog++ for approximate counting,
    /// which is much faster than exact GROUP BY on large tables.
    pub fn build_approx_top_values_query(&self, column_name: &str, k: usize) -> String {
        let db_prefix = self
            .database_name
            .as_ref()
            .map(|db| format!("{}.", db))
            .unwrap_or_default();

        format!(
            r#"
SELECT
    APPROX_TOP_K({column}, {k}) AS top_values
FROM {db_prefix}{schema}.{table}
            "#,
            db_prefix = db_prefix,
            schema = self.schema_name.to_uppercase(),
            table = self.table_name.to_uppercase(),
            column = column_name.to_uppercase(),
            k = k
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_stats_query() {
        let extractor = SnowflakeStatsExtractor::new("PUBLIC", "CUSTOMERS");
        let query = extractor.build_stats_query();

        assert!(query.contains("INFORMATION_SCHEMA.COLUMNS"));
        assert!(query.contains("AUTOMATIC_CLUSTERING_INFORMATION"));
        assert!(query.contains("SEARCH_OPTIMIZATION"));
        assert!(query.contains("TABLE_STORAGE_METRICS"));
        assert!(query.contains("PUBLIC"));
        assert!(query.contains("CUSTOMERS"));
    }

    #[test]
    fn test_build_stats_query_with_database() {
        let extractor = SnowflakeStatsExtractor::with_database("MYDB", "PUBLIC", "ORDERS");
        let query = extractor.build_stats_query();

        assert!(query.contains("MYDB.INFORMATION_SCHEMA.COLUMNS"));
        assert!(query.contains("PUBLIC"));
        assert!(query.contains("ORDERS"));
    }

    #[test]
    fn test_classify_cardinality() {
        assert_eq!(
            SnowflakeStatsExtractor::classify_cardinality(1.0),
            CardinalityClass::Unique
        );
        assert_eq!(
            SnowflakeStatsExtractor::classify_cardinality(0.97),
            CardinalityClass::VeryHigh
        );
        assert_eq!(
            SnowflakeStatsExtractor::classify_cardinality(0.70),
            CardinalityClass::High
        );
        assert_eq!(
            SnowflakeStatsExtractor::classify_cardinality(0.30),
            CardinalityClass::Medium
        );
        assert_eq!(
            SnowflakeStatsExtractor::classify_cardinality(0.05),
            CardinalityClass::Low
        );
        assert_eq!(
            SnowflakeStatsExtractor::classify_cardinality(0.005),
            CardinalityClass::VeryLow
        );
    }

    #[test]
    fn test_parse_column_statistics_identity() {
        let extractor = SnowflakeStatsExtractor::new("PUBLIC", "USERS");

        let mut row = HashMap::new();
        row.insert(
            "column_name".to_string(),
            JsonValue::String("ID".to_string()),
        );
        row.insert(
            "is_nullable".to_string(),
            JsonValue::String("NO".to_string()),
        );
        row.insert(
            "is_identity".to_string(),
            JsonValue::String("YES".to_string()),
        );
        row.insert("is_clustering_key".to_string(), JsonValue::Bool(false));
        row.insert("is_search_optimized".to_string(), JsonValue::Bool(false));

        let stats = extractor
            .parse_column_statistics(&row, Some(10000))
            .unwrap();

        // IDENTITY columns are automatically unique
        assert_eq!(stats.cardinality, Some(CardinalityClass::Unique));
    }

    #[test]
    fn test_parse_column_statistics_clustering_key_low_depth() {
        let extractor = SnowflakeStatsExtractor::new("PUBLIC", "EVENTS");

        let mut row = HashMap::new();
        row.insert(
            "column_name".to_string(),
            JsonValue::String("USER_ID".to_string()),
        );
        row.insert(
            "is_nullable".to_string(),
            JsonValue::String("NO".to_string()),
        );
        row.insert(
            "is_identity".to_string(),
            JsonValue::String("NO".to_string()),
        );
        row.insert("is_clustering_key".to_string(), JsonValue::Bool(true));
        row.insert(
            "clustering_depth".to_string(),
            JsonValue::Number(serde_json::Number::from_f64(5.0).unwrap()),
        );
        row.insert("is_search_optimized".to_string(), JsonValue::Bool(false));

        let stats = extractor
            .parse_column_statistics(&row, Some(10000))
            .unwrap();

        // Low clustering depth suggests high cardinality
        assert_eq!(stats.cardinality, Some(CardinalityClass::VeryHigh));
    }

    #[test]
    fn test_parse_column_statistics_clustering_key_high_depth() {
        let extractor = SnowflakeStatsExtractor::new("PUBLIC", "EVENTS");

        let mut row = HashMap::new();
        row.insert(
            "column_name".to_string(),
            JsonValue::String("STATUS".to_string()),
        );
        row.insert(
            "is_nullable".to_string(),
            JsonValue::String("YES".to_string()),
        );
        row.insert(
            "is_identity".to_string(),
            JsonValue::String("NO".to_string()),
        );
        row.insert("is_clustering_key".to_string(), JsonValue::Bool(true));
        row.insert(
            "clustering_depth".to_string(),
            JsonValue::Number(serde_json::Number::from_f64(75.0).unwrap()),
        );
        row.insert("is_search_optimized".to_string(), JsonValue::Bool(false));

        let stats = extractor
            .parse_column_statistics(&row, Some(10000))
            .unwrap();

        // High clustering depth suggests low cardinality
        assert_eq!(stats.cardinality, Some(CardinalityClass::Low));
    }

    #[test]
    fn test_parse_column_statistics_search_optimized() {
        let extractor = SnowflakeStatsExtractor::new("PUBLIC", "PRODUCTS");

        let mut row = HashMap::new();
        row.insert(
            "column_name".to_string(),
            JsonValue::String("SKU".to_string()),
        );
        row.insert(
            "is_nullable".to_string(),
            JsonValue::String("NO".to_string()),
        );
        row.insert(
            "is_identity".to_string(),
            JsonValue::String("NO".to_string()),
        );
        row.insert("is_clustering_key".to_string(), JsonValue::Bool(false));
        row.insert("is_search_optimized".to_string(), JsonValue::Bool(true));

        let stats = extractor
            .parse_column_statistics(&row, Some(10000))
            .unwrap();

        // Search-optimized columns are typically high-cardinality identifiers
        assert_eq!(stats.cardinality, Some(CardinalityClass::VeryHigh));
    }

    #[test]
    fn test_parse_column_statistics_with_account_usage() {
        let extractor = SnowflakeStatsExtractor::new("PUBLIC", "CUSTOMERS");

        let mut row = HashMap::new();
        row.insert(
            "column_name".to_string(),
            JsonValue::String("EMAIL".to_string()),
        );
        row.insert(
            "is_nullable".to_string(),
            JsonValue::String("YES".to_string()),
        );
        row.insert(
            "is_identity".to_string(),
            JsonValue::String("NO".to_string()),
        );
        row.insert("is_clustering_key".to_string(), JsonValue::Bool(false));
        row.insert("is_search_optimized".to_string(), JsonValue::Bool(false));
        row.insert("distinct_count".to_string(), JsonValue::Number(9500.into()));
        row.insert("null_count".to_string(), JsonValue::Number(100.into()));
        row.insert("row_count".to_string(), JsonValue::Number(10000.into()));
        row.insert(
            "average_length".to_string(),
            JsonValue::Number(serde_json::Number::from_f64(35.5).unwrap()),
        );

        let stats = extractor
            .parse_column_statistics(&row, Some(10000))
            .unwrap();

        assert_eq!(stats.distinct_count, Some(9500));
        assert_eq!(stats.null_count, 100);
        assert_eq!(stats.null_percentage, 1.0);
        assert_eq!(stats.avg_length, Some(35.5));
        assert_eq!(stats.cardinality, Some(CardinalityClass::VeryHigh)); // 95% distinct
    }

    #[test]
    fn test_build_account_usage_stats_query() {
        let extractor = SnowflakeStatsExtractor::new("PUBLIC", "ORDERS");
        let query = extractor.build_account_usage_stats_query();

        assert!(query.contains("ACCOUNT_USAGE.COLUMNS"));
        assert!(query.contains("ACCOUNT_USAGE.TABLES"));
        assert!(query.contains("distinct_count"));
        assert!(query.contains("null_count"));
        assert!(query.contains("PUBLIC"));
        assert!(query.contains("ORDERS"));
    }

    #[test]
    fn test_build_top_values_query() {
        let extractor = SnowflakeStatsExtractor::new("SALES", "TRANSACTIONS");
        let query = extractor.build_top_values_query("STATUS", 10);

        assert!(query.contains("GROUP BY"));
        assert!(query.contains("ORDER BY frequency DESC"));
        assert!(query.contains("LIMIT 10"));
        assert!(query.contains("STATUS"));
    }

    #[test]
    fn test_build_approx_top_values_query() {
        let extractor = SnowflakeStatsExtractor::new("SALES", "TRANSACTIONS");
        let query = extractor.build_approx_top_values_query("CATEGORY", 20);

        assert!(query.contains("APPROX_TOP_K"));
        assert!(query.contains("CATEGORY"));
        assert!(query.contains("20"));
    }
}
