//! Workflow testing framework
//!
//! Provides a comprehensive testing system for declarative workflows including:
//! - Test case definitions
//! - Mock data sources and sinks
//! - Assertions and expectations
//! - Test execution engine

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// Test suite for a workflow
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowTestSuite {
    /// Test suite name
    pub name: String,

    /// Workflow file being tested
    pub workflow_file: String,

    /// Test cases
    pub test_cases: Vec<TestCase>,

    /// Setup actions to run before tests
    #[serde(default)]
    pub setup: Vec<TestSetupAction>,

    /// Teardown actions to run after tests
    #[serde(default)]
    pub teardown: Vec<TestTeardownAction>,
}

/// Individual test case
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TestCase {
    /// Test case name
    pub name: String,

    /// Description of what this test validates
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Input data for the test
    pub input: TestInput,

    /// Expected outputs and assertions
    pub expect: TestExpectations,

    /// Skip this test
    #[serde(default)]
    pub skip: bool,

    /// Skip reason
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skip_reason: Option<String>,
}

/// Test input specification
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum TestInput {
    /// Single record
    Single(Value),

    /// Multiple records
    Multiple(Vec<Value>),

    /// Load from file
    FromFile { file: String, format: String },
}

/// Test expectations and assertions
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TestExpectations {
    /// Expected route to be selected
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route: Option<String>,

    /// Expected output data
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<Value>,

    /// Field-level assertions
    #[serde(default)]
    pub assertions: Vec<Assertion>,

    /// Expected action results
    #[serde(default)]
    pub actions: Vec<ActionExpectation>,

    /// Expected errors (for negative tests)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorExpectation>,
}

/// Assertion types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Assertion {
    /// Assert field equals value
    FieldEquals { field: String, value: Value },

    /// Assert field contains value
    FieldContains { field: String, value: String },

    /// Assert field exists
    FieldExists { field: String },

    /// Assert field does not exist
    FieldNotExists { field: String },

    /// Assert field matches regex
    FieldMatches { field: String, pattern: String },

    /// Assert field is greater than value
    FieldGreaterThan { field: String, value: Value },

    /// Assert field is less than value
    FieldLessThan { field: String, value: Value },

    /// Assert array length
    ArrayLength { field: String, length: usize },

    /// Assert count of records
    RecordCount { count: usize },

    /// Custom JSONPath assertion
    JsonPath { path: String, value: Value },
}

/// Expected action result
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActionExpectation {
    /// Action type
    pub action_type: String,

    /// Expected success
    #[serde(default = "default_true")]
    pub success: bool,

    /// Expected output
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<Value>,
}

fn default_true() -> bool {
    true
}

/// Expected error
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ErrorExpectation {
    /// Error message should contain
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_contains: Option<String>,

    /// Error type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_type: Option<String>,
}

/// Setup action to run before tests
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TestSetupAction {
    /// Load reference data
    LoadReferenceData { name: String, data: Value },

    /// Set environment variable
    SetEnv { key: String, value: String },

    /// Create mock endpoint
    MockEndpoint { url: String, response: Value },
}

/// Teardown action to run after tests
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TestTeardownAction {
    /// Clear reference data
    ClearReferenceData { name: String },

    /// Unset environment variable
    UnsetEnv { key: String },

    /// Remove mock endpoint
    RemoveMockEndpoint { url: String },
}

/// Test execution result
#[derive(Debug, Clone)]
pub struct TestResult {
    /// Test case name
    pub test_name: String,

    /// Test passed
    pub passed: bool,

    /// Assertion results
    pub assertion_results: Vec<AssertionResult>,

    /// Error message if failed
    pub error: Option<String>,

    /// Execution time in milliseconds
    pub execution_time_ms: u64,
}

/// Individual assertion result
#[derive(Debug, Clone)]
pub struct AssertionResult {
    /// Assertion description
    pub description: String,

    /// Assertion passed
    pub passed: bool,

    /// Expected value
    pub expected: Option<String>,

    /// Actual value
    pub actual: Option<String>,

    /// Error message if failed
    pub error: Option<String>,
}

/// Test suite execution result
#[derive(Debug, Clone)]
pub struct TestSuiteResult {
    /// Suite name
    pub suite_name: String,

    /// Total tests
    pub total: usize,

    /// Passed tests
    pub passed: usize,

    /// Failed tests
    pub failed: usize,

    /// Skipped tests
    pub skipped: usize,

    /// Individual test results
    pub test_results: Vec<TestResult>,

    /// Total execution time in milliseconds
    pub total_time_ms: u64,
}

impl TestSuiteResult {
    /// Check if all tests passed
    pub fn all_passed(&self) -> bool {
        self.failed == 0
    }

    /// Get success rate as percentage
    pub fn success_rate(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            (self.passed as f64 / self.total as f64) * 100.0
        }
    }

    /// Get summary string
    pub fn summary(&self) -> String {
        format!(
            "{} tests: {} passed, {} failed, {} skipped ({:.1}% success rate)",
            self.total,
            self.passed,
            self.failed,
            self.skipped,
            self.success_rate()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_test_suite_deserialization() {
        let yaml = r#"
name: "Customer Routing Tests"
workflow_file: "customer_workflow.yaml"
test_cases:
  - name: "Enterprise customer routes to high priority"
    description: "Enterprise customers should go to high priority queue"
    input:
      customer_type: "enterprise"
      annual_revenue: 5000000
    expect:
      route: "high_priority"
      assertions:
        - type: field_equals
          field: "tier"
          value: "platinum"
"#;

        let suite: WorkflowTestSuite = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(suite.name, "Customer Routing Tests");
        assert_eq!(suite.test_cases.len(), 1);
        assert_eq!(
            suite.test_cases[0].name,
            "Enterprise customer routes to high priority"
        );
    }

    #[test]
    fn test_assertion_types() {
        let assertions = vec![
            Assertion::FieldEquals {
                field: "status".to_string(),
                value: json!("active"),
            },
            Assertion::FieldContains {
                field: "email".to_string(),
                value: "@example.com".to_string(),
            },
            Assertion::FieldExists {
                field: "customer_id".to_string(),
            },
            Assertion::RecordCount { count: 5 },
        ];

        assert_eq!(assertions.len(), 4);
    }

    #[test]
    fn test_test_input_variants() {
        // Single record
        let single = TestInput::Single(json!({"id": 1}));
        assert!(matches!(single, TestInput::Single(_)));

        // Multiple records
        let multiple = TestInput::Multiple(vec![json!({"id": 1}), json!({"id": 2})]);
        assert!(matches!(multiple, TestInput::Multiple(_)));

        // From file
        let from_file = TestInput::FromFile {
            file: "test_data.json".to_string(),
            format: "json".to_string(),
        };
        assert!(matches!(from_file, TestInput::FromFile { .. }));
    }

    #[test]
    fn test_test_suite_result_summary() {
        let result = TestSuiteResult {
            suite_name: "Test Suite".to_string(),
            total: 10,
            passed: 8,
            failed: 2,
            skipped: 0,
            test_results: vec![],
            total_time_ms: 1000,
        };

        assert!(!result.all_passed());
        assert_eq!(result.success_rate(), 80.0);
        assert!(result.summary().contains("8 passed"));
        assert!(result.summary().contains("2 failed"));
    }

    #[test]
    fn test_setup_teardown_actions() {
        let setup = vec![
            TestSetupAction::LoadReferenceData {
                name: "customers".to_string(),
                data: json!([{"id": 1}]),
            },
            TestSetupAction::SetEnv {
                key: "TEST_MODE".to_string(),
                value: "true".to_string(),
            },
        ];

        let teardown = vec![
            TestTeardownAction::ClearReferenceData {
                name: "customers".to_string(),
            },
            TestTeardownAction::UnsetEnv {
                key: "TEST_MODE".to_string(),
            },
        ];

        assert_eq!(setup.len(), 2);
        assert_eq!(teardown.len(), 2);
    }

    #[test]
    fn test_error_expectation() {
        let error_exp = ErrorExpectation {
            message_contains: Some("invalid".to_string()),
            error_type: Some("ValidationError".to_string()),
        };

        assert_eq!(error_exp.message_contains.unwrap(), "invalid");
    }
}
