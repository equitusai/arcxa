//! Rule execution engine (heuristics and WASM)
//!
//! Provides integration with existing WASM rule engine and heuristic rules

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;

use crate::core::rules::WasmRuleEngine;

/// Rule executor with WASM engine integration
pub struct RuleExecutor {
    /// WASM rule engine for sandboxed execution
    wasm_engine: Arc<WasmRuleEngine>,
    /// Default timeout for rule execution
    default_timeout: Duration,
}

impl RuleExecutor {
    /// Create new rule executor with WASM engine
    pub fn new() -> Self {
        Self {
            wasm_engine: Arc::new(WasmRuleEngine::new().expect("Failed to initialize WASM engine")),
            default_timeout: Duration::from_secs(5),
        }
    }

    /// Create rule executor with custom timeout
    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            wasm_engine: Arc::new(WasmRuleEngine::new().expect("Failed to initialize WASM engine")),
            default_timeout: timeout,
        }
    }

    /// Load a WASM rule into the engine
    pub fn load_rule(&self, rule_id: &str, wasm_bytes: &[u8]) -> Result<()> {
        self.wasm_engine
            .load_rule(rule_id, wasm_bytes)
            .context("Failed to load WASM rule")
    }

    /// Execute heuristic or WASM rule
    pub async fn execute_heuristic(
        &self,
        rule_id: &str,
        input: &serde_json::Value,
    ) -> Result<RuleResult> {
        // Convert input to JSON string for WASM
        let input_json =
            serde_json::to_string(input).context("Failed to serialize input for rule execution")?;

        // Execute via WASM engine
        let wasm_result = self
            .wasm_engine
            .execute(rule_id, &input_json, self.default_timeout)
            .context("WASM rule execution failed")?;

        // Convert WASM result to RuleResult
        // Confidence: 1.0 if passed, 0.0 if failed (can be enhanced with rule-specific logic)
        let confidence = if wasm_result.passed { 0.95 } else { 0.05 };

        Ok(RuleResult {
            success: wasm_result.passed,
            output: serde_json::json!({
                "passed": wasm_result.passed,
                "message": wasm_result.message,
                "rule_id": rule_id,
            }),
            confidence,
        })
    }

    /// Unload a WASM rule from the engine
    pub fn unload_rule(&self, rule_id: &str) -> Result<()> {
        self.wasm_engine
            .unload_rule(rule_id)
            .context("Failed to unload WASM rule")
    }

    /// Clear rule cache
    pub fn clear_cache(&self) {
        self.wasm_engine.clear_cache();
    }
}

impl Default for RuleExecutor {
    fn default() -> Self {
        Self::new()
    }
}

/// Rule execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleResult {
    pub success: bool,
    pub output: serde_json::Value,
    pub confidence: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rule_executor_creation() {
        let executor = RuleExecutor::new();
        assert_eq!(executor.default_timeout, Duration::from_secs(5));
    }

    #[tokio::test]
    async fn test_rule_executor_with_custom_timeout() {
        let executor = RuleExecutor::with_timeout(Duration::from_secs(10));
        assert_eq!(executor.default_timeout, Duration::from_secs(10));
    }

    // Note: Full integration test requires compiling WASM module
    // See tests/orchestration_integration_test.rs for end-to-end tests
}
