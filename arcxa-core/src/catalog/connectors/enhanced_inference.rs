//! Enhanced Schema Inference with Statistics and Semantic Types
//!
//! This module provides advanced schema inference that combines:
//! - Database statistics (PostgreSQL pg_stats)
//! - Semantic type detection
//! - Statistical analysis

use anyhow::Result;
use std::collections::HashMap;

use crate::catalog::api_types::ColumnDefinition;
use crate::inference::postgres_stats::PostgresStatsExtractor;
use crate::inference::semantic::{
    ColumnNameDetector, CompositeStrategy, DetectionContext, DetectionStrategy,
};
use crate::inference::types::{ColumnStatistics, SemanticType};

/// Enhanced schema inference for a single column
pub struct ColumnInferenceEngine {
    /// PostgreSQL stats extractor
    stats_extractor: PostgresStatsExtractor,

    /// Semantic type detection strategy
    semantic_detector: CompositeStrategy,
}

impl ColumnInferenceEngine {
    /// Create new inference engine for a table
    pub fn new(schema: impl Into<String>, table: impl Into<String>) -> Self {
        let stats_extractor = PostgresStatsExtractor::new(schema, table);

        // Build semantic detection pipeline
        let semantic_detector = CompositeStrategy::new("column_inference")
            .add_strategy(Box::new(ColumnNameDetector::new()));
        // TODO: Add more strategies (pattern matching, value analysis, etc.)

        Self {
            stats_extractor,
            semantic_detector,
        }
    }

    /// Enrich a column definition with statistics and semantic types
    ///
    /// # Arguments
    /// * `column` - Basic column definition from information_schema
    /// * `pg_stats_row` - Row from pg_stats (if available)
    /// * `sample_values` - Sample column values for semantic detection
    /// * `total_rows` - Total row count in table
    pub async fn enrich_column(
        &self,
        mut column: ColumnDefinition,
        pg_stats_row: Option<&HashMap<String, serde_json::Value>>,
        sample_values: Vec<String>,
        total_rows: Option<u64>,
    ) -> Result<ColumnDefinition> {
        // Extract PostgreSQL statistics if available
        if let Some(stats_row) = pg_stats_row {
            column.statistics = self
                .stats_extractor
                .parse_column_statistics(stats_row, total_rows)
                .ok();
        }

        // Build detection context
        let mut detection_context = DetectionContext::new(&column.name, &column.data_type);
        detection_context.native_type = column.data_type.clone();
        detection_context.nullable = column.nullable;
        detection_context.sample_values = sample_values;

        // Add statistics if available
        if let Some(ref stats) = column.statistics {
            detection_context.distinct_count = stats.distinct_count;
            detection_context.null_percentage = stats.null_percentage;
            detection_context.avg_length = stats.avg_width.map(|w| w as f64);
        }

        detection_context.total_rows = total_rows;

        // Run semantic detection
        if let Ok(Some(detection_result)) = self.semantic_detector.detect(&detection_context).await
        {
            tracing::debug!(
                "Detected semantic type {:?} for column '{}' with confidence {}",
                detection_result.semantic_type,
                column.name,
                detection_result.confidence
            );

            column.semantic_type = Some(detection_result.semantic_type);
        }

        Ok(column)
    }

    /// Get the query to fetch pg_stats for this table
    pub fn get_stats_query(&self) -> String {
        self.stats_extractor.build_stats_query()
    }
}

/// Helper to convert PostgreSQL type to normalized type string
pub fn normalize_postgres_type(pg_type: &str) -> String {
    // Map PostgreSQL-specific types to standard types
    let normalized = match pg_type {
        t if t.starts_with("character varying") => "varchar",
        t if t.starts_with("character") => "char",
        "integer" => "int",
        "bigint" => "bigint",
        "smallint" => "smallint",
        "numeric" | "decimal" => "decimal",
        "real" => "float",
        "double precision" => "double",
        "boolean" => "boolean",
        "date" => "date",
        "timestamp" | "timestamp without time zone" => "timestamp",
        "timestamp with time zone" => "timestamptz",
        "text" => "text",
        "json" => "json",
        "jsonb" => "jsonb",
        "uuid" => "uuid",
        other => other,
    };

    normalized.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_postgres_type() {
        assert_eq!(normalize_postgres_type("character varying(255)"), "varchar");
        assert_eq!(normalize_postgres_type("integer"), "int");
        assert_eq!(
            normalize_postgres_type("timestamp without time zone"),
            "timestamp"
        );
    }

    #[tokio::test]
    async fn test_column_inference_engine() {
        let engine = ColumnInferenceEngine::new("public", "users");

        let column = ColumnDefinition {
            name: "email".to_string(),
            data_type: "varchar".to_string(),
            nullable: false,
            primary_key: false,
            default_value: None,
            semantic_type: None,
            statistics: None,
        };

        let sample_values = vec!["john@example.com".to_string(), "jane@test.com".to_string()];

        let enriched = engine
            .enrich_column(column, None, sample_values, Some(100))
            .await
            .unwrap();

        // Should detect Email semantic type from column name
        assert!(enriched.semantic_type.is_some());
        match enriched.semantic_type {
            Some(SemanticType::Email) => {
                // Correctly detected!
            }
            other => panic!("Expected Email, got {:?}", other),
        }
    }
}
