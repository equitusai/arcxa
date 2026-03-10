//! # Data Quality Module
//!
//! Manages quality rules, violations, and scorecards.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Quality rule definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityRule {
    pub id: String,
    pub name: String,
    pub description: String,
    pub rule_type: RuleType,
    pub severity: Severity,
    pub dataset: String,
    pub fields: Vec<String>,
    pub expression: String,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum RuleType {
    Completeness,
    Validity,
    Uniqueness,
    Consistency,
    Accuracy,
    Timeliness,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    Info,
    Warning,
    Error,
    Critical,
}

/// Quality violation instance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityViolation {
    pub id: Uuid,
    pub rule_id: String,
    pub dataset: String,
    pub record_id: String,
    pub field: Option<String>,
    pub actual_value: Option<String>,
    pub expected_value: Option<String>,
    pub message: String,
    pub severity: Severity,
    pub detected_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub lineage_ref: Option<Uuid>,
}

/// Quality scorecard for a dataset
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityScorecard {
    pub dataset: String,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub overall_score: f64,
    pub dimension_scores: HashMap<RuleType, f64>,
    pub total_records: u64,
    pub violation_counts: HashMap<Severity, u64>,
    pub rule_results: Vec<RuleResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleResult {
    pub rule_id: String,
    pub passed: u64,
    pub failed: u64,
    pub success_rate: f64,
}

/// Trait for quality rule execution
pub trait RuleExecutor: Send + Sync {
    /// Execute rule against a record
    fn execute(&self, rule: &QualityRule, record: &serde_json::Value)
        -> anyhow::Result<RuleResult>;

    /// Batch execute rules
    fn execute_batch(
        &self,
        rules: &[QualityRule],
        records: &[serde_json::Value],
    ) -> anyhow::Result<Vec<QualityViolation>>;
}
