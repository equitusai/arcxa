//! Database-Agnostic Statistics Extraction
//!
//! This module provides a unified interface for extracting column statistics
//! from different database systems (PostgreSQL, DB2, Oracle, etc.).
//!
//! **Architecture:**
//! - `DatabaseStatsExtractor` enum wraps database-specific extractors
//! - Each database has its own implementation (PostgresStatsExtractor, Db2StatsExtractor)
//! - Common interface through enum methods
//! - Semantic detection layer remains database-agnostic

use anyhow::Result;
use serde_json::Value as JsonValue;
use std::collections::HashMap;

use crate::inference::db2_stats::Db2StatsExtractor;
use crate::inference::postgres_stats::PostgresStatsExtractor;
use crate::inference::snowflake_stats::SnowflakeStatsExtractor;
use crate::inference::types::ColumnStatistics;

/// Database-agnostic statistics extractor
///
/// Wraps database-specific statistics extractors and provides a unified interface.
/// The semantic detection framework works with the common ColumnStatistics type.
#[derive(Debug, Clone)]
pub enum DatabaseStatsExtractor {
    /// PostgreSQL statistics from pg_stats
    PostgreSQL(PostgresStatsExtractor),

    /// DB2 statistics from SYSCAT/SYSSTAT
    DB2(Db2StatsExtractor),

    /// Snowflake statistics with special features (clustering, search optimization)
    Snowflake(SnowflakeStatsExtractor),
    // Oracle statistics (placeholder for future implementation)
    // Oracle(OracleStatsExtractor),
}

impl DatabaseStatsExtractor {
    /// Create PostgreSQL statistics extractor
    pub fn postgres(schema: impl Into<String>, table: impl Into<String>) -> Self {
        Self::PostgreSQL(PostgresStatsExtractor::new(schema, table))
    }

    /// Create DB2 statistics extractor
    pub fn db2(schema: impl Into<String>, table: impl Into<String>) -> Self {
        Self::DB2(Db2StatsExtractor::new(schema, table))
    }

    /// Create Snowflake statistics extractor
    pub fn snowflake(schema: impl Into<String>, table: impl Into<String>) -> Self {
        Self::Snowflake(SnowflakeStatsExtractor::new(schema, table))
    }

    /// Create Snowflake statistics extractor with explicit database
    pub fn snowflake_with_db(
        database: impl Into<String>,
        schema: impl Into<String>,
        table: impl Into<String>,
    ) -> Self {
        Self::Snowflake(SnowflakeStatsExtractor::with_database(
            database, schema, table,
        ))
    }

    /// Build the statistics query for this database
    ///
    /// Returns the appropriate SQL query to extract column statistics
    /// from the database's system catalog.
    pub fn build_stats_query(&self) -> String {
        match self {
            Self::PostgreSQL(extractor) => extractor.build_stats_query(),
            Self::DB2(extractor) => extractor.build_stats_query(),
            Self::Snowflake(extractor) => extractor.build_stats_query(),
        }
    }

    /// Parse column statistics from query result
    ///
    /// Converts database-specific result format to common ColumnStatistics type.
    pub fn parse_column_statistics(
        &self,
        row: &HashMap<String, JsonValue>,
        total_rows: Option<u64>,
    ) -> Result<ColumnStatistics> {
        match self {
            Self::PostgreSQL(extractor) => extractor.parse_column_statistics(row, total_rows),
            Self::DB2(extractor) => extractor.parse_column_statistics(row, total_rows),
            Self::Snowflake(extractor) => extractor.parse_column_statistics(row, total_rows),
        }
    }

    /// Get the database type name
    pub fn database_type(&self) -> &'static str {
        match self {
            Self::PostgreSQL(_) => "PostgreSQL",
            Self::DB2(_) => "DB2",
            Self::Snowflake(_) => "Snowflake",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference::types::CardinalityClass;

    #[test]
    fn test_postgres_extractor_creation() {
        let extractor = DatabaseStatsExtractor::postgres("public", "users");
        assert_eq!(extractor.database_type(), "PostgreSQL");

        let query = extractor.build_stats_query();
        assert!(query.contains("pg_stats"));
        assert!(query.contains("public"));
        assert!(query.contains("users"));
    }

    #[test]
    fn test_db2_extractor_creation() {
        let extractor = DatabaseStatsExtractor::db2("MYSCHEMA", "CUSTOMERS");
        assert_eq!(extractor.database_type(), "DB2");

        let query = extractor.build_stats_query();
        assert!(query.contains("SYSCAT.COLUMNS"));
        assert!(query.contains("MYSCHEMA"));
        assert!(query.contains("CUSTOMERS"));
    }

    #[test]
    fn test_postgres_parse_statistics() {
        let extractor = DatabaseStatsExtractor::postgres("public", "users");

        let mut row = HashMap::new();
        row.insert(
            "null_frac".to_string(),
            JsonValue::Number(serde_json::Number::from_f64(0.01).unwrap()),
        );
        row.insert("avg_width".to_string(), JsonValue::Number(50.into()));
        row.insert("n_distinct".to_string(), JsonValue::Number(9500.into()));

        let stats = extractor
            .parse_column_statistics(&row, Some(10000))
            .unwrap();

        assert_eq!(stats.null_percentage, 1.0);
        assert_eq!(stats.avg_width, Some(50));
        assert_eq!(stats.distinct_count, Some(9500));
    }

    #[test]
    fn test_db2_parse_statistics() {
        let extractor = DatabaseStatsExtractor::db2("TESTDB", "ORDERS");

        let mut row = HashMap::new();
        row.insert("nulls".to_string(), JsonValue::String("Y".to_string()));
        row.insert("avgcollen".to_string(), JsonValue::Number(50.into()));
        row.insert("colcard".to_string(), JsonValue::Number(9500.into()));
        row.insert("table_card".to_string(), JsonValue::Number(10000.into()));
        row.insert("numnulls".to_string(), JsonValue::Number(100.into()));

        let stats = extractor
            .parse_column_statistics(&row, Some(10000))
            .unwrap();

        assert_eq!(stats.null_percentage, 1.0);
        assert_eq!(stats.avg_width, Some(50));
        assert_eq!(stats.distinct_count, Some(9500));
    }

    #[test]
    fn test_snowflake_extractor_creation() {
        let extractor = DatabaseStatsExtractor::snowflake("PUBLIC", "EVENTS");
        assert_eq!(extractor.database_type(), "Snowflake");

        let query = extractor.build_stats_query();
        assert!(query.contains("INFORMATION_SCHEMA.COLUMNS"));
        assert!(query.contains("AUTOMATIC_CLUSTERING_INFORMATION"));
        assert!(query.contains("PUBLIC"));
        assert!(query.contains("EVENTS"));
    }

    #[test]
    fn test_snowflake_extractor_with_database() {
        let extractor = DatabaseStatsExtractor::snowflake_with_db("MYDB", "PUBLIC", "USERS");
        assert_eq!(extractor.database_type(), "Snowflake");

        let query = extractor.build_stats_query();
        assert!(query.contains("MYDB.INFORMATION_SCHEMA.COLUMNS"));
        assert!(query.contains("PUBLIC"));
        assert!(query.contains("USERS"));
    }

    #[test]
    fn test_snowflake_parse_statistics_identity() {
        let extractor = DatabaseStatsExtractor::snowflake("PUBLIC", "TRANSACTIONS");

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

        // IDENTITY columns are unique
        assert_eq!(stats.cardinality, Some(CardinalityClass::Unique));
    }

    #[test]
    fn test_snowflake_parse_statistics_clustering_key() {
        let extractor = DatabaseStatsExtractor::snowflake("PUBLIC", "LOGS");

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
            JsonValue::Number(serde_json::Number::from_f64(8.5).unwrap()),
        );
        row.insert("is_search_optimized".to_string(), JsonValue::Bool(false));

        let stats = extractor
            .parse_column_statistics(&row, Some(10000))
            .unwrap();

        // Low clustering depth suggests high cardinality
        assert_eq!(stats.cardinality, Some(CardinalityClass::VeryHigh));
    }

    #[test]
    fn test_snowflake_parse_statistics_search_optimized() {
        let extractor = DatabaseStatsExtractor::snowflake("PUBLIC", "PRODUCTS");

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

        // Search-optimized columns are typically high-cardinality
        assert_eq!(stats.cardinality, Some(CardinalityClass::VeryHigh));
    }
}
