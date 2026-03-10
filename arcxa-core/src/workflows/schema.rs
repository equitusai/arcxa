// Declarative workflow schema types
//
// These types define the structure of YAML/JSON workflow definitions.
// They are separate from the domain types to allow for format evolution.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Root workflow definition
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowSchema {
    /// API version (e.g., "graphica.io/v1")
    pub api_version: String,

    /// Resource kind (must be "Workflow")
    pub kind: String,

    /// Workflow metadata
    pub metadata: WorkflowMetadata,

    /// Workflow specification
    pub spec: WorkflowSpec,
}

/// Workflow metadata
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowMetadata {
    /// Workflow name (used as ID)
    pub name: String,

    /// Semantic version
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    /// Human-readable description
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Owning team or user
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,

    /// Tags for categorization
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,

    /// Annotations (key-value metadata)
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub annotations: HashMap<String, String>,
}

/// Workflow specification
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowSpec {
    /// Scheduling configuration
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schedule: Option<ScheduleSpec>,

    /// Execution settings
    #[serde(default)]
    pub execution: ExecutionSpec,

    /// Decision routes
    pub routes: Vec<RouteSpec>,

    /// Default route name if no conditions match
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_route: Option<String>,

    /// Monitoring and alerting
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub monitoring: Option<MonitoringSpec>,

    /// Resource limits
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resources: Option<ResourceSpec>,
}

/// Schedule specification
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleSpec {
    /// Cron expression (e.g., "0 2 * * *")
    pub cron: String,

    /// Timezone (e.g., "America/New_York")
    #[serde(default = "default_timezone")]
    pub timezone: String,

    /// Whether schedule is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_timezone() -> String {
    "UTC".to_string()
}

fn default_true() -> bool {
    true
}

/// Execution settings
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionSpec {
    /// Execution mode (batch or streaming)
    #[serde(default = "default_batch_mode")]
    pub mode: ExecutionMode,

    /// Timeout in seconds
    #[serde(default = "default_timeout")]
    pub timeout: u64,

    /// Number of retries on failure
    #[serde(default)]
    pub retries: u32,

    /// Delay between retries in seconds
    #[serde(default = "default_retry_delay")]
    pub retry_delay: u64,
}

fn default_batch_mode() -> ExecutionMode {
    ExecutionMode::Batch
}

fn default_timeout() -> u64 {
    3600 // 1 hour
}

fn default_retry_delay() -> u64 {
    300 // 5 minutes
}

impl Default for ExecutionSpec {
    fn default() -> Self {
        Self {
            mode: ExecutionMode::Batch,
            timeout: 3600,
            retries: 0,
            retry_delay: 300,
        }
    }
}

/// Execution mode
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ExecutionMode {
    Batch,
    Streaming,
}

/// Route specification (decision path)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RouteSpec {
    /// Route name
    pub name: String,

    /// Description
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Priority (higher = evaluated first)
    #[serde(default = "default_priority")]
    pub priority: i32,

    /// Condition for route activation
    pub condition: ConditionSpec,

    /// Actions to execute if condition matches
    pub actions: Vec<ActionSpec>,
}

fn default_priority() -> i32 {
    0
}

/// Condition specification
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "PascalCase")]
pub enum ConditionSpec {
    /// Always matches
    Always,

    /// Field equals value
    Equals {
        field: String,
        value: serde_json::Value,
    },

    /// Field does not equal value
    NotEquals {
        field: String,
        value: serde_json::Value,
    },

    /// Field greater than value
    GreaterThan {
        field: String,
        value: serde_json::Value,
    },

    /// Field less than value
    LessThan {
        field: String,
        value: serde_json::Value,
    },

    /// Field contains value (for arrays/strings)
    Contains {
        field: String,
        value: serde_json::Value,
    },

    /// Field matches regex
    Regex { field: String, pattern: String },

    /// Field is null
    IsNull { field: String },

    /// Logical AND of conditions
    And { conditions: Vec<ConditionSpec> },

    /// Logical OR of conditions
    Or { conditions: Vec<ConditionSpec> },

    /// Logical NOT of condition
    Not { condition: Box<ConditionSpec> },
}

/// Action specification
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "PascalCase")]
pub enum ActionSpec {
    /// Log message
    Log { level: String, message: String },

    /// Transform data
    Transform {
        transformer: String,
        config: serde_json::Value,
    },

    /// Enrich with reference data
    Enrich {
        reference_data: String,
        join_key: String,
    },

    /// Validate with quality rule
    Validate { rule_id: String },

    /// Send to Kafka topic
    SendToKafka {
        topic: String,
        partition_key: Option<String>,
    },

    /// Send to HTTP endpoint
    SendToHttp {
        url: String,
        method: String,
        headers: HashMap<String, String>,
    },

    /// Execute custom code
    ExecuteCode { language: String, code: String },

    /// Call ML model
    CallModel {
        model_id: String,
        input_mapping: HashMap<String, String>,
        output_mapping: HashMap<String, String>,
    },
}

/// Monitoring specification
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MonitoringSpec {
    /// SLA in minutes
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sla_minutes: Option<u64>,

    /// Quality threshold (0.0-1.0)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality_threshold: Option<f64>,

    /// Alert configurations
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alerts: Vec<AlertSpec>,
}

/// Alert specification
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AlertSpec {
    /// Alert type (slack, pagerduty, email)
    #[serde(rename = "type")]
    pub alert_type: String,

    /// Channel/recipient
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,

    /// Severity level
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,

    /// Conditions that trigger alert
    pub conditions: Vec<String>,
}

/// Resource specification
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ResourceSpec {
    /// Maximum parallel workers
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_workers: Option<u32>,

    /// Memory limit in MB
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_mb: Option<u64>,

    /// CPU cores
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_cores: Option<u32>,
}

/// Test specification for workflow testing
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowTestSpec {
    /// API version
    pub api_version: String,

    /// Resource kind (must be "WorkflowTest")
    pub kind: String,

    /// Test metadata
    pub metadata: TestMetadata,

    /// Test specification
    pub spec: TestSpec,
}

/// Test metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestMetadata {
    /// Test name
    pub name: String,

    /// Workflow being tested
    pub workflow: String,
}

/// Test specification
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestSpec {
    /// Test fixtures
    pub fixtures: FixturesSpec,

    /// Mocks for external dependencies
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mocks: Vec<MockSpec>,

    /// Test cases
    pub test_cases: Vec<TestCaseSpec>,
}

/// Fixtures specification
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FixturesSpec {
    /// Path to input data
    pub input_data: String,

    /// Reference data sources
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub reference_data: HashMap<String, String>,
}

/// Mock specification
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MockSpec {
    /// Mock type (kafka, http, database)
    #[serde(rename = "type")]
    pub mock_type: String,

    /// Topic/URL/table being mocked
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,

    /// Whether to capture calls
    #[serde(default)]
    pub capture: bool,
}

/// Test case specification
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestCaseSpec {
    /// Test case name
    pub name: String,

    /// Description
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Input data for this test case
    pub input: serde_json::Value,

    /// Expected outcomes
    pub expectations: Vec<ExpectationSpec>,
}

/// Expectation specification
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpectationSpec {
    /// Expected matched route
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matched_route: Option<String>,

    /// Expected Kafka messages
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kafka_messages: Option<KafkaExpectation>,

    /// Expected execution status
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_status: Option<String>,

    /// Expected quality score
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality_score: Option<String>,
}

/// Kafka message expectation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KafkaExpectation {
    /// Topic name
    pub topic: String,

    /// Expected message count
    pub count: usize,

    /// Fields that should be present
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contains: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workflow_schema_serde() {
        let workflow = WorkflowSchema {
            api_version: "graphica.io/v1".to_string(),
            kind: "Workflow".to_string(),
            metadata: WorkflowMetadata {
                name: "test-workflow".to_string(),
                version: Some("1.0.0".to_string()),
                description: Some("Test workflow".to_string()),
                owner: Some("data-team".to_string()),
                tags: vec!["test".to_string()],
                annotations: HashMap::new(),
            },
            spec: WorkflowSpec {
                schedule: None,
                execution: ExecutionSpec::default(),
                routes: vec![RouteSpec {
                    name: "default".to_string(),
                    description: None,
                    priority: 0,
                    condition: ConditionSpec::Always,
                    actions: vec![ActionSpec::Log {
                        level: "info".to_string(),
                        message: "test".to_string(),
                    }],
                }],
                default_route: Some("default".to_string()),
                monitoring: None,
                resources: None,
            },
        };

        // Test JSON serialization
        let json = serde_json::to_string_pretty(&workflow).unwrap();
        let deserialized: WorkflowSchema = serde_json::from_str(&json).unwrap();
        assert_eq!(workflow, deserialized);

        // Test YAML serialization
        let yaml = serde_yaml::to_string(&workflow).unwrap();
        let deserialized: WorkflowSchema = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(workflow, deserialized);
    }

    #[test]
    fn test_condition_spec_variants() {
        let conditions = vec![
            ConditionSpec::Always,
            ConditionSpec::Equals {
                field: "status".to_string(),
                value: serde_json::json!("active"),
            },
            ConditionSpec::GreaterThan {
                field: "amount".to_string(),
                value: serde_json::json!(1000),
            },
            ConditionSpec::And {
                conditions: vec![
                    ConditionSpec::Equals {
                        field: "type".to_string(),
                        value: serde_json::json!("premium"),
                    },
                    ConditionSpec::GreaterThan {
                        field: "score".to_string(),
                        value: serde_json::json!(80),
                    },
                ],
            },
        ];

        for condition in conditions {
            let json = serde_json::to_string(&condition).unwrap();
            let deserialized: ConditionSpec = serde_json::from_str(&json).unwrap();
            assert_eq!(condition, deserialized);
        }
    }

    #[test]
    fn test_action_spec_variants() {
        let actions = vec![
            ActionSpec::Log {
                level: "info".to_string(),
                message: "Processing record".to_string(),
            },
            ActionSpec::Transform {
                transformer: "uppercase".to_string(),
                config: serde_json::json!({"field": "name"}),
            },
            ActionSpec::SendToKafka {
                topic: "output-topic".to_string(),
                partition_key: Some("customer_id".to_string()),
            },
        ];

        for action in actions {
            let json = serde_json::to_string(&action).unwrap();
            let deserialized: ActionSpec = serde_json::from_str(&json).unwrap();
            assert_eq!(action, deserialized);
        }
    }
}
