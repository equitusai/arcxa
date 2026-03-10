//! Test execution engine for workflows

use crate::workflows::declarative::{DeclarativeParser, WorkflowBuilder};
use crate::workflows::engine::{
    ActionExecutor, ConditionEvaluator, ExecutionContext, WorkflowRouter,
};
use anyhow::{Context, Result};
use graphica_core::workflows::testing::*;
use regex::Regex;
use serde_json::Value;
use std::fs;
use std::time::Instant;

/// Test executor for workflow tests
pub struct TestExecutor {
    /// Verbose output
    verbose: bool,
}

impl TestExecutor {
    /// Create a new test executor
    pub fn new(verbose: bool) -> Self {
        Self { verbose }
    }

    /// Execute a test suite
    pub async fn execute_suite(&self, suite_path: &str) -> Result<TestSuiteResult> {
        let start_time = Instant::now();

        // Load test suite
        let suite = self.load_test_suite(suite_path)?;

        if self.verbose {
            println!("Running test suite: {}", suite.name);
            println!("Workflow: {}", suite.workflow_file);
            println!();
        }

        // Load workflow
        let workflow_schema = DeclarativeParser::parse_file(&suite.workflow_file)
            .with_context(|| format!("Failed to parse workflow file: {}", suite.workflow_file))?;

        let workflow = WorkflowBuilder::build(&workflow_schema)
            .with_context(|| "Failed to build workflow from schema")?;

        // Run setup actions
        for action in &suite.setup {
            self.run_setup_action(action)?;
        }

        // Execute test cases
        let mut test_results = Vec::new();
        let mut passed = 0;
        let mut failed = 0;
        let mut skipped = 0;

        for test_case in &suite.test_cases {
            if test_case.skip {
                if self.verbose {
                    println!("⊗ SKIP: {}", test_case.name);
                    if let Some(ref reason) = test_case.skip_reason {
                        println!("  Reason: {}", reason);
                    }
                }
                skipped += 1;
                continue;
            }

            let result = self.execute_test_case(test_case, &workflow).await?;

            if result.passed {
                passed += 1;
                if self.verbose {
                    println!(
                        "✓ PASS: {} ({} ms)",
                        test_case.name, result.execution_time_ms
                    );
                }
            } else {
                failed += 1;
                if self.verbose {
                    println!(
                        "✗ FAIL: {} ({} ms)",
                        test_case.name, result.execution_time_ms
                    );
                    if let Some(ref error) = result.error {
                        println!("  Error: {}", error);
                    }
                    for assertion_result in &result.assertion_results {
                        if !assertion_result.passed {
                            println!("  ✗ {}", assertion_result.description);
                            if let (Some(expected), Some(actual)) =
                                (&assertion_result.expected, &assertion_result.actual)
                            {
                                println!("    Expected: {}", expected);
                                println!("    Actual:   {}", actual);
                            }
                        }
                    }
                }
            }

            test_results.push(result);
        }

        // Run teardown actions
        for action in &suite.teardown {
            self.run_teardown_action(action)?;
        }

        let total_time_ms = start_time.elapsed().as_millis() as u64;

        Ok(TestSuiteResult {
            suite_name: suite.name.clone(),
            total: suite.test_cases.len(),
            passed,
            failed,
            skipped,
            test_results,
            total_time_ms,
        })
    }

    /// Load test suite from file
    fn load_test_suite(&self, path: &str) -> Result<WorkflowTestSuite> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read test suite file: {}", path))?;

        if path.ends_with(".yaml") || path.ends_with(".yml") {
            serde_yaml::from_str(&content).with_context(|| "Failed to parse test suite YAML")
        } else if path.ends_with(".json") {
            serde_json::from_str(&content).with_context(|| "Failed to parse test suite JSON")
        } else {
            anyhow::bail!("Unsupported test suite format. Use .yaml or .json");
        }
    }

    /// Execute a single test case
    async fn execute_test_case(
        &self,
        test_case: &TestCase,
        workflow: &crate::workflows::domain::Workflow,
    ) -> Result<TestResult> {
        let start_time = Instant::now();
        let mut assertion_results = Vec::new();

        // Load input data
        let input_data = self.load_test_input(&test_case.input)?;

        // Execute workflow for single input
        let input_value = match &input_data {
            TestInput::Single(val) => val.clone(),
            TestInput::Multiple(vals) => {
                if vals.is_empty() {
                    return Ok(TestResult {
                        test_name: test_case.name.clone(),
                        passed: false,
                        assertion_results: vec![],
                        error: Some("No input data provided".to_string()),
                        execution_time_ms: 0,
                    });
                }
                vals[0].clone()
            }
            TestInput::FromFile { .. } => {
                return Ok(TestResult {
                    test_name: test_case.name.clone(),
                    passed: false,
                    assertion_results: vec![],
                    error: Some("FromFile input not yet implemented".to_string()),
                    execution_time_ms: 0,
                });
            }
        };

        // Select route
        let route_match = WorkflowRouter::select_route(workflow, &input_value)?;

        // Check expected route
        if let Some(ref expected_route) = test_case.expect.route {
            let actual_route = route_match.as_ref().map(|r| r.route.id.as_str());
            let passed = actual_route == Some(expected_route.as_str());

            assertion_results.push(AssertionResult {
                description: format!("Route should be '{}'", expected_route),
                passed,
                expected: Some(expected_route.clone()),
                actual: actual_route.map(String::from),
                error: if !passed {
                    Some(format!(
                        "Expected route '{}', got '{}'",
                        expected_route,
                        actual_route.unwrap_or("None")
                    ))
                } else {
                    None
                },
            });
        }

        // Execute actions if route matched
        let mut output_data = input_value.clone();
        if let Some(route_match) = route_match {
            let context = ExecutionContext {
                workflow_id: workflow.id.clone(),
                route_id: route_match.route.id.clone(),
                input_data: input_value.clone(),
                rule_executor: None,
                transformer_registry: None,
                kafka_producer: None,
                http_client: None,
                lineage_generator: None,
                manual_mapping_store: None,
                execution_id: None,
                action_index: 0,
                metrics: None,
                approval_store: None,
                execution_store: None,
                column_lineage_store: None,
                tenant_id: "default".to_string(),
                timeout_config: graphica_core::orchestration::workflow::ExecutionTimeout::default(),
                workflow_start_time: std::time::Instant::now(),
                stage_start_time: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
                db2_pool: None,
                postgres_pool: None,
                memory_monitor: None,
            };

            let _action_results = ActionExecutor::execute_actions(
                &route_match.route.actions,
                &mut output_data,
                &context,
            )
            .await?;
        }

        // Run assertions
        for assertion in &test_case.expect.assertions {
            let result = self.evaluate_assertion(assertion, &output_data);
            assertion_results.push(result);
        }

        let execution_time_ms = start_time.elapsed().as_millis() as u64;
        let all_passed = assertion_results.iter().all(|r| r.passed);

        Ok(TestResult {
            test_name: test_case.name.clone(),
            passed: all_passed,
            assertion_results,
            error: None,
            execution_time_ms,
        })
    }

    /// Load test input data
    fn load_test_input(&self, input: &TestInput) -> Result<TestInput> {
        match input {
            TestInput::FromFile { file, format } => {
                let content = fs::read_to_string(file)
                    .with_context(|| format!("Failed to read input file: {}", file))?;

                let data: Value = if format == "json" {
                    serde_json::from_str(&content)?
                } else {
                    serde_yaml::from_str(&content)?
                };

                Ok(TestInput::Single(data))
            }
            other => Ok(other.clone()),
        }
    }

    /// Evaluate an assertion
    fn evaluate_assertion(&self, assertion: &Assertion, data: &Value) -> AssertionResult {
        match assertion {
            Assertion::FieldEquals { field, value } => {
                let actual = self.get_field(data, field);
                let passed = actual == Some(value);

                AssertionResult {
                    description: format!("Field '{}' should equal {:?}", field, value),
                    passed,
                    expected: Some(format!("{:?}", value)),
                    actual: actual.map(|v| format!("{:?}", v)),
                    error: if !passed {
                        Some(format!("Field '{}' value mismatch", field))
                    } else {
                        None
                    },
                }
            }

            Assertion::FieldContains { field, value } => {
                let actual = self.get_field(data, field);
                let passed = actual
                    .and_then(|v| v.as_str())
                    .map(|s| s.contains(value))
                    .unwrap_or(false);

                AssertionResult {
                    description: format!("Field '{}' should contain '{}'", field, value),
                    passed,
                    expected: Some(format!("contains '{}'", value)),
                    actual: actual.and_then(|v| v.as_str().map(String::from)),
                    error: if !passed {
                        Some(format!("Field '{}' does not contain '{}'", field, value))
                    } else {
                        None
                    },
                }
            }

            Assertion::FieldExists { field } => {
                let exists = self.get_field(data, field).is_some();

                AssertionResult {
                    description: format!("Field '{}' should exist", field),
                    passed: exists,
                    expected: Some("field exists".to_string()),
                    actual: Some(if exists { "exists" } else { "missing" }.to_string()),
                    error: if !exists {
                        Some(format!("Field '{}' does not exist", field))
                    } else {
                        None
                    },
                }
            }

            Assertion::FieldNotExists { field } => {
                let exists = self.get_field(data, field).is_some();

                AssertionResult {
                    description: format!("Field '{}' should not exist", field),
                    passed: !exists,
                    expected: Some("field does not exist".to_string()),
                    actual: Some(if exists { "exists" } else { "missing" }.to_string()),
                    error: if exists {
                        Some(format!("Field '{}' exists but should not", field))
                    } else {
                        None
                    },
                }
            }

            Assertion::FieldMatches { field, pattern } => {
                let actual = self.get_field(data, field);
                let regex = Regex::new(pattern).ok();
                let passed = actual
                    .and_then(|v| v.as_str())
                    .and_then(|s| regex.as_ref().map(|r| r.is_match(s)))
                    .unwrap_or(false);

                AssertionResult {
                    description: format!("Field '{}' should match pattern '{}'", field, pattern),
                    passed,
                    expected: Some(format!("matches /{}/", pattern)),
                    actual: actual.and_then(|v| v.as_str().map(String::from)),
                    error: if !passed {
                        Some(format!("Field '{}' does not match pattern", field))
                    } else {
                        None
                    },
                }
            }

            Assertion::FieldGreaterThan { field, value } => {
                let actual = self.get_field(data, field);
                let passed = self.compare_values(actual, value, |a, b| a > b);

                AssertionResult {
                    description: format!("Field '{}' should be greater than {:?}", field, value),
                    passed,
                    expected: Some(format!("> {:?}", value)),
                    actual: actual.map(|v| format!("{:?}", v)),
                    error: if !passed {
                        Some(format!("Field '{}' is not greater than expected", field))
                    } else {
                        None
                    },
                }
            }

            Assertion::FieldLessThan { field, value } => {
                let actual = self.get_field(data, field);
                let passed = self.compare_values(actual, value, |a, b| a < b);

                AssertionResult {
                    description: format!("Field '{}' should be less than {:?}", field, value),
                    passed,
                    expected: Some(format!("< {:?}", value)),
                    actual: actual.map(|v| format!("{:?}", v)),
                    error: if !passed {
                        Some(format!("Field '{}' is not less than expected", field))
                    } else {
                        None
                    },
                }
            }

            Assertion::ArrayLength { field, length } => {
                let actual = self.get_field(data, field);
                let actual_length = actual.and_then(|v| v.as_array().map(|a| a.len()));
                let passed = actual_length == Some(*length);

                AssertionResult {
                    description: format!("Array '{}' should have length {}", field, length),
                    passed,
                    expected: Some(format!("length = {}", length)),
                    actual: actual_length.map(|l| format!("length = {}", l)),
                    error: if !passed {
                        Some(format!("Array '{}' length mismatch", field))
                    } else {
                        None
                    },
                }
            }

            Assertion::RecordCount { count } => {
                // For single record, count should be 1
                let passed = *count == 1;

                AssertionResult {
                    description: format!("Record count should be {}", count),
                    passed,
                    expected: Some(format!("{}", count)),
                    actual: Some("1".to_string()),
                    error: if !passed {
                        Some("Record count mismatch".to_string())
                    } else {
                        None
                    },
                }
            }

            Assertion::JsonPath { path: _, value: _ } => {
                // JSONPath not yet implemented
                AssertionResult {
                    description: "JSONPath assertion".to_string(),
                    passed: false,
                    expected: Some("JSONPath support".to_string()),
                    actual: Some("not implemented".to_string()),
                    error: Some("JSONPath assertions not yet implemented".to_string()),
                }
            }
        }
    }

    /// Get field from JSON value using dot notation
    fn get_field<'a>(&self, data: &'a Value, field: &str) -> Option<&'a Value> {
        let parts: Vec<&str> = field.split('.').collect();
        let mut current = data;

        for part in parts {
            current = current.get(part)?;
        }

        Some(current)
    }

    /// Compare two values
    fn compare_values<F>(&self, actual: Option<&Value>, expected: &Value, comparator: F) -> bool
    where
        F: Fn(f64, f64) -> bool,
    {
        match (actual, expected) {
            (Some(Value::Number(a)), Value::Number(b)) => {
                if let (Some(a_f64), Some(b_f64)) = (a.as_f64(), b.as_f64()) {
                    comparator(a_f64, b_f64)
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    /// Run setup action
    fn run_setup_action(&self, _action: &TestSetupAction) -> Result<()> {
        // TODO: Implement setup actions
        Ok(())
    }

    /// Run teardown action
    fn run_teardown_action(&self, _action: &TestTeardownAction) -> Result<()> {
        // TODO: Implement teardown actions
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_field_equals_assertion() {
        let executor = TestExecutor::new(false);
        let data = json!({"status": "active", "count": 42});

        let assertion = Assertion::FieldEquals {
            field: "status".to_string(),
            value: json!("active"),
        };

        let result = executor.evaluate_assertion(&assertion, &data);
        assert!(result.passed);
    }

    #[test]
    fn test_field_contains_assertion() {
        let executor = TestExecutor::new(false);
        let data = json!({"email": "user@example.com"});

        let assertion = Assertion::FieldContains {
            field: "email".to_string(),
            value: "@example.com".to_string(),
        };

        let result = executor.evaluate_assertion(&assertion, &data);
        assert!(result.passed);
    }

    #[test]
    fn test_field_exists_assertion() {
        let executor = TestExecutor::new(false);
        let data = json!({"customer_id": "123"});

        let assertion = Assertion::FieldExists {
            field: "customer_id".to_string(),
        };

        let result = executor.evaluate_assertion(&assertion, &data);
        assert!(result.passed);

        let assertion_missing = Assertion::FieldExists {
            field: "missing_field".to_string(),
        };

        let result_missing = executor.evaluate_assertion(&assertion_missing, &data);
        assert!(!result_missing.passed);
    }

    #[test]
    fn test_field_greater_than_assertion() {
        let executor = TestExecutor::new(false);
        let data = json!({"age": 30});

        let assertion = Assertion::FieldGreaterThan {
            field: "age".to_string(),
            value: json!(25),
        };

        let result = executor.evaluate_assertion(&assertion, &data);
        assert!(result.passed);
    }

    #[test]
    fn test_nested_field_access() {
        let executor = TestExecutor::new(false);
        let data = json!({
            "user": {
                "profile": {
                    "name": "John Doe"
                }
            }
        });

        let field = executor.get_field(&data, "user.profile.name");
        assert_eq!(field, Some(&json!("John Doe")));
    }

    #[test]
    fn test_array_length_assertion() {
        let executor = TestExecutor::new(false);
        let data = json!({"items": [1, 2, 3, 4, 5]});

        let assertion = Assertion::ArrayLength {
            field: "items".to_string(),
            length: 5,
        };

        let result = executor.evaluate_assertion(&assertion, &data);
        assert!(result.passed);
    }
}
