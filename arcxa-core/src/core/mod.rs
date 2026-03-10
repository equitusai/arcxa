//! # Core Module
//!
//! Foundation types and domain model for Graphica data governance platform.

pub mod lineage;
pub mod quality;
pub mod rules;

// Re-export key types
pub use lineage::{
    CdcPosition, DataRef, LineageEvent, LineageGraph, LineageSink, ModelMetrics, ModelRef,
    TransformRef,
};
pub use quality::{
    QualityRule, QualityScorecard, QualityViolation, RuleExecutor, RuleResult, RuleType, Severity,
};
pub use rules::{CompiledRule, RuleExecutionResult, WasmRuleEngine};
