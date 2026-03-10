//! Workflow Actions - Operations executed when a route matches
//!
//! Actions are the "then" part of the "if-then" routing logic.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;

/// Unique identifier for an action
pub type ActionId = String;

/// An action to execute when a route matches
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Action {
    /// Transform data using a named transformer
    ///
    /// Example: `{"type": "Transform", "transformer": "normalize_address", "config": {...}}`
    Transform {
        transformer: String,
        #[serde(default)]
        config: JsonValue,
    },

    /// Validate data against a quality rule
    ///
    /// Example: `{"type": "Validate", "rule_id": "completeness_check"}`
    Validate { rule_id: String },

    /// Send data to Kafka topic
    ///
    /// Example: `{"type": "SendToKafka", "topic": "high_quality", "partition_key": "customer_id"}`
    SendToKafka {
        topic: String,
        #[serde(default)]
        partition_key: Option<String>,
    },

    /// Send data via HTTP request
    ///
    /// Example: `{"type": "SendToHttp", "url": "https://api.example.com/ingest", "method": "POST"}`
    SendToHttp {
        url: String,
        #[serde(default = "default_http_method")]
        method: String,
        #[serde(default)]
        headers: HashMap<String, String>,
    },

    /// Record lineage event
    ///
    /// Example: `{"type": "RecordLineage", "event_type": "quality_routing", "metadata": {...}}`
    RecordLineage {
        event_type: String,
        #[serde(default)]
        metadata: JsonValue,
    },

    /// Log message at specified level
    ///
    /// Example: `{"type": "Log", "level": "info", "message": "Processed record"}`
    Log {
        #[serde(default = "default_log_level")]
        level: String,
        message: String,
    },

    /// Set field value in output
    ///
    /// Example: `{"type": "SetField", "field": "processed_at", "value": "2025-10-10T12:00:00Z"}`
    SetField { field: String, value: JsonValue },

    /// Remove field from output
    ///
    /// Example: `{"type": "RemoveField", "field": "internal_id"}`
    RemoveField { field: String },

    /// Execute custom WASM handler
    ///
    /// Example: `{"type": "Custom", "handler": "my_handler", "config": {...}}`
    Custom {
        handler: String,
        #[serde(default)]
        config: JsonValue,
    },

    /// Send notification (email, slack, etc.)
    ///
    /// Example: `{"type": "Notify", "channel": "slack", "recipient": "#alerts", "message": "..."}`
    Notify {
        channel: String,
        recipient: String,
        message: String,
    },

    /// Increment metric counter
    ///
    /// Example: `{"type": "IncrementMetric", "metric": "records_processed", "labels": {"type": "customer"}}`
    IncrementMetric {
        metric: String,
        #[serde(default)]
        labels: HashMap<String, String>,
    },

    /// Wait for human approval before continuing workflow execution
    ///
    /// Pauses workflow execution and creates an approval request. The workflow will
    /// resume only after the request is approved via the approval API. If the request
    /// is rejected or expires, the workflow fails.
    ///
    /// Example: `{"type": "WaitForApproval", "approval_id": "ddl_{{ execution_id }}", "approval_type": "ddl_execution", "approval_payload": {"ddl": "..."}, "timeout_secs": 86400}`
    WaitForApproval {
        /// Unique identifier for this approval request (must be unique across all requests)
        approval_id: String,

        /// Type of approval (e.g., "ddl_execution", "data_deletion", "config_change")
        approval_type: String,

        /// Data to present to the approver (e.g., DDL statements, deletion scope)
        #[serde(default)]
        approval_payload: JsonValue,

        /// Timeout in seconds (workflow fails if not approved within this time)
        /// Valid range: 60 seconds (1 minute) to 2592000 seconds (30 days)
        #[serde(default = "default_approval_timeout")]
        timeout_secs: u64,

        /// Optional condition to skip approval (e.g., non-prod environments, low-risk operations)
        /// If this evaluates to true, approval is skipped and workflow continues immediately
        /// Examples:
        /// - "${env.ENVIRONMENT}" == "dev" || "${env.ENVIRONMENT}" == "staging"
        /// - "${data.risk_level}" == "low"
        /// - "${data.estimated_rows}" < 1000
        #[serde(default)]
        skip_if: Option<String>,
    },
}

fn default_http_method() -> String {
    "POST".to_string()
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_approval_timeout() -> u64 {
    86400 // 24 hours in seconds
}

impl Action {
    /// Get a human-readable name for this action type
    pub fn action_type(&self) -> &'static str {
        match self {
            Self::Transform { .. } => "Transform",
            Self::Validate { .. } => "Validate",
            Self::SendToKafka { .. } => "SendToKafka",
            Self::SendToHttp { .. } => "SendToHttp",
            Self::RecordLineage { .. } => "RecordLineage",
            Self::Log { .. } => "Log",
            Self::SetField { .. } => "SetField",
            Self::RemoveField { .. } => "RemoveField",
            Self::Custom { .. } => "Custom",
            Self::Notify { .. } => "Notify",
            Self::IncrementMetric { .. } => "IncrementMetric",
            Self::WaitForApproval { .. } => "WaitForApproval",
        }
    }

    /// Check if this action can run in parallel with others
    ///
    /// Some actions (like SetField, RemoveField, WaitForApproval) modify data and must run sequentially
    pub fn is_parallel_safe(&self) -> bool {
        match self {
            Self::Transform { .. }
            | Self::SetField { .. }
            | Self::RemoveField { .. }
            | Self::WaitForApproval { .. } => false,
            Self::Validate { .. }
            | Self::SendToKafka { .. }
            | Self::SendToHttp { .. }
            | Self::RecordLineage { .. }
            | Self::Log { .. }
            | Self::Custom { .. }
            | Self::Notify { .. }
            | Self::IncrementMetric { .. } => true,
        }
    }

    /// Estimate action execution time (for scheduling)
    pub fn estimated_duration_ms(&self) -> u64 {
        match self {
            Self::Transform { .. } => 50,       // Data transformation
            Self::Validate { .. } => 10,        // Rule evaluation
            Self::SendToKafka { .. } => 5,      // Network I/O (buffered)
            Self::SendToHttp { .. } => 100,     // HTTP request
            Self::RecordLineage { .. } => 2,    // Write to store
            Self::Log { .. } => 1,              // Logging
            Self::SetField { .. } => 1,         // In-memory modification
            Self::RemoveField { .. } => 1,      // In-memory modification
            Self::Custom { .. } => 20,          // WASM execution
            Self::Notify { .. } => 50,          // Notification service
            Self::IncrementMetric { .. } => 1,  // Metrics update
            Self::WaitForApproval { .. } => 10, // Create approval request (actual wait is async)
        }
    }

    /// Validate action parameters
    ///
    /// Returns Ok(()) if all parameters are valid, otherwise returns error message.
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::WaitForApproval {
                approval_id,
                approval_type,
                timeout_secs,
                ..
            } => {
                // Validate approval_id is not empty
                if approval_id.trim().is_empty() {
                    return Err("approval_id cannot be empty".to_string());
                }

                // Validate approval_type is not empty
                if approval_type.trim().is_empty() {
                    return Err("approval_type cannot be empty".to_string());
                }

                // Validate timeout is within reasonable bounds
                // Min: 60 seconds (1 minute)
                // Max: 2592000 seconds (30 days)
                if *timeout_secs < 60 {
                    return Err("timeout_secs must be at least 60 seconds (1 minute)".to_string());
                }
                if *timeout_secs > 2592000 {
                    return Err("timeout_secs cannot exceed 2592000 seconds (30 days)".to_string());
                }

                Ok(())
            }
            // Other actions don't have validation requirements yet
            _ => Ok(()),
        }
    }
}

/// Result of executing an action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionResult {
    pub action_type: String,
    pub status: ActionStatus,
    pub duration_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<JsonValue>,
}

/// Status of action execution
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActionStatus {
    /// Action completed successfully
    Success,

    /// Action failed with error
    Failed,

    /// Action was skipped (conditional execution)
    Skipped,

    /// Action paused workflow (waiting for approval, external event, etc.)
    ///
    /// When an action returns Paused status, the workflow execution should:
    /// 1. Save checkpoint (action index + intermediate data)
    /// 2. Update execution status to ExecutionStatus::Paused
    /// 3. Stop processing further actions
    /// 4. Wait for external signal to resume (approval, timeout, etc.)
    Paused,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_action_type_names() {
        let action = Action::Transform {
            transformer: "normalize".to_string(),
            config: json!({}),
        };
        assert_eq!(action.action_type(), "Transform");

        let action = Action::SendToKafka {
            topic: "test".to_string(),
            partition_key: None,
        };
        assert_eq!(action.action_type(), "SendToKafka");
    }

    #[test]
    fn test_parallel_safety() {
        // Sequential actions
        assert!(!Action::SetField {
            field: "x".to_string(),
            value: json!(1),
        }
        .is_parallel_safe());

        assert!(!Action::Transform {
            transformer: "x".to_string(),
            config: json!({}),
        }
        .is_parallel_safe());

        // Parallel-safe actions
        assert!(Action::SendToKafka {
            topic: "x".to_string(),
            partition_key: None,
        }
        .is_parallel_safe());

        assert!(Action::Log {
            level: "info".to_string(),
            message: "test".to_string(),
        }
        .is_parallel_safe());
    }

    #[test]
    fn test_serde_send_to_kafka() {
        let action = Action::SendToKafka {
            topic: "test_topic".to_string(),
            partition_key: Some("key".to_string()),
        };

        let json = serde_json::to_string(&action).unwrap();
        let deserialized: Action = serde_json::from_str(&json).unwrap();

        assert_eq!(action, deserialized);
    }

    #[test]
    fn test_serde_send_to_http() {
        let mut headers = HashMap::new();
        headers.insert("Authorization".to_string(), "Bearer token".to_string());

        let action = Action::SendToHttp {
            url: "https://api.example.com".to_string(),
            method: "POST".to_string(),
            headers,
        };

        let json = serde_json::to_string(&action).unwrap();
        let deserialized: Action = serde_json::from_str(&json).unwrap();

        assert_eq!(action, deserialized);
    }

    #[test]
    fn test_serde_default_http_method() {
        let json = json!({
            "type": "SendToHttp",
            "url": "https://example.com"
        });

        let action: Action = serde_json::from_value(json).unwrap();

        match action {
            Action::SendToHttp { method, .. } => {
                assert_eq!(method, "POST");
            }
            _ => panic!("Expected SendToHttp"),
        }
    }

    #[test]
    fn test_action_result_success() {
        let result = ActionResult {
            action_type: "SendToKafka".to_string(),
            status: ActionStatus::Success,
            duration_ms: 45,
            error: None,
            output: Some(json!({"message_id": "12345"})),
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"status\":\"success\""));
        assert!(!json.contains("\"error\""));
    }

    #[test]
    fn test_action_result_failed() {
        let result = ActionResult {
            action_type: "Validate".to_string(),
            status: ActionStatus::Failed,
            duration_ms: 10,
            error: Some("Rule validation failed".to_string()),
            output: None,
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"status\":\"failed\""));
        assert!(json.contains("\"error\""));
    }

    #[test]
    fn test_estimated_durations() {
        // Verify relative ordering makes sense
        assert!(
            Action::SendToHttp {
                url: String::new(),
                method: String::new(),
                headers: HashMap::new(),
            }
            .estimated_duration_ms()
                > Action::SendToKafka {
                    topic: String::new(),
                    partition_key: None,
                }
                .estimated_duration_ms()
        );

        assert!(
            Action::SetField {
                field: String::new(),
                value: json!(null),
            }
            .estimated_duration_ms()
                < Action::Transform {
                    transformer: String::new(),
                    config: json!(null),
                }
                .estimated_duration_ms()
        );
    }
}
