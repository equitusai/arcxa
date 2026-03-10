//! Unified Data Profiler Trait
//!
//! Provides a common interface for profiling any datasource type (files, databases, APIs, etc.)
//! Generates UnifiedSchema with field-level statistics and metadata.

use super::field::UnifiedField;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration for profiling operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileConfig {
    /// Maximum number of rows to sample (None = all rows)
    pub sample_size: Option<usize>,

    /// Calculate statistical metrics (min, max, median, percentiles)
    pub compute_statistics: bool,

    /// Detect semantic types (email, phone, SSN, etc.)
    pub detect_semantic_types: bool,

    /// Analyze relationships between tables/files
    pub profile_relationships: bool,

    /// Detect PII and sensitive data
    pub detect_pii: bool,

    /// Timeout for profiling operation (seconds)
    pub timeout_seconds: Option<u64>,

    /// Custom sampling strategy
    pub sampling_strategy: SamplingStrategy,
}

impl Default for ProfileConfig {
    fn default() -> Self {
        Self {
            sample_size: Some(10_000),
            compute_statistics: true,
            detect_semantic_types: true,
            profile_relationships: false,
            detect_pii: true,
            timeout_seconds: Some(300), // 5 minutes
            sampling_strategy: SamplingStrategy::Random,
        }
    }
}

/// Sampling strategy for large datasets
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SamplingStrategy {
    /// Sample first N rows
    Head,
    /// Sample last N rows
    Tail,
    /// Random sampling across entire dataset
    Random,
    /// Systematic sampling (every Nth row)
    Systematic { interval: usize },
    /// Stratified sampling (proportional to data distribution)
    Stratified,
}

/// Relationship between two entities (tables, files, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationshipInfo {
    /// Source entity (table/file name)
    pub source: String,
    /// Source field name
    pub source_field: String,
    /// Target entity (table/file name)
    pub target: String,
    /// Target field name
    pub target_field: String,
    /// Relationship type
    pub relationship_type: RelationshipType,
    /// Confidence score (0.0 - 1.0)
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RelationshipType {
    /// One-to-one relationship
    OneToOne,
    /// One-to-many relationship
    OneToMany,
    /// Many-to-one relationship
    ManyToOne,
    /// Many-to-many relationship
    ManyToMany,
    /// Potential foreign key
    ForeignKey,
}

/// Sample data row (field_name -> value)
pub type SampleRow = HashMap<String, serde_json::Value>;

/// Unified profiler trait for all datasource types
pub trait DataProfiler: Send + Sync {
    /// Profile an entire datasource and return unified schema
    ///
    /// For files: profiles the single file
    /// For databases: profiles all accessible tables
    fn profile_source(
        &self,
        source_ref: &str,
        config: ProfileConfig,
    ) -> Result<Vec<super::UnifiedSchema>>;

    /// Profile a specific table/file/collection
    fn profile_table(
        &self,
        source_ref: &str,
        table_name: &str,
        config: ProfileConfig,
    ) -> Result<super::UnifiedSchema>;

    /// Get sample data for preview/analysis
    fn get_sample_data(
        &self,
        source_ref: &str,
        table_name: Option<&str>,
        limit: usize,
    ) -> Result<Vec<SampleRow>>;

    /// Detect relationships between tables (if applicable)
    fn detect_relationships(&self, _source_ref: &str) -> Result<Vec<RelationshipInfo>> {
        // Default implementation: no relationships
        Ok(vec![])
    }

    /// Validate data quality during profiling
    fn validate_quality(
        &self,
        _source_ref: &str,
        _table_name: Option<&str>,
    ) -> Result<QualityReport> {
        // Default implementation: basic quality report
        Ok(QualityReport::default())
    }
}

/// Quality report from profiling
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct QualityReport {
    /// Overall quality score (0.0 - 1.0)
    pub quality_score: f64,

    /// Issues found during profiling
    pub issues: Vec<QualityIssue>,

    /// Warnings
    pub warnings: Vec<String>,

    /// Completeness percentage
    pub completeness: f64,

    /// Validity percentage
    pub validity: f64,

    /// Uniqueness percentage (for key fields)
    pub uniqueness: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityIssue {
    /// Field name (if applicable)
    pub field_name: Option<String>,

    /// Issue severity
    pub severity: IssueSeverity,

    /// Issue description
    pub description: String,

    /// Number of affected rows
    pub affected_rows: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum IssueSeverity {
    Critical,
    Warning,
    Info,
}

/// Helper trait for converting profiling results to UnifiedField
pub trait ToUnifiedField {
    fn to_unified_field(&self, position: usize) -> UnifiedField;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profile_config_default() {
        let config = ProfileConfig::default();
        assert_eq!(config.sample_size, Some(10_000));
        assert!(config.compute_statistics);
        assert!(config.detect_semantic_types);
        assert!(config.detect_pii);
    }

    #[test]
    fn test_sampling_strategy() {
        let strategy = SamplingStrategy::Random;
        assert_eq!(strategy, SamplingStrategy::Random);

        let systematic = SamplingStrategy::Systematic { interval: 10 };
        match systematic {
            SamplingStrategy::Systematic { interval } => assert_eq!(interval, 10),
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_quality_report_default() {
        let report = QualityReport::default();
        assert_eq!(report.quality_score, 0.0);
        assert!(report.issues.is_empty());
        assert!(report.warnings.is_empty());
    }
}
