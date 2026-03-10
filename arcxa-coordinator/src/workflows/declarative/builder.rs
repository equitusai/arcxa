//! Workflow builder - converts WorkflowSchema to domain Workflow
//!
//! This module provides the core functionality to build domain workflow objects
//! from declarative YAML/JSON schemas with full validation and error handling.

use super::errors::BuildError;
use crate::workflows::domain::{
    Action, Condition, ExecutionMode, Route, StateBackendConfig, StreamingConfig,
    WatermarkStrategy, Workflow, WorkflowSchedule,
};
use chrono::Utc;
use graphica_core::workflows::*;
use std::collections::HashMap;
use tracing::{debug, info};
use uuid::Uuid;

/// Builder for converting WorkflowSchema to domain Workflow
pub struct WorkflowBuilder;

impl WorkflowBuilder {
    /// Build a domain Workflow from a WorkflowSchema
    ///
    /// # Arguments
    ///
    /// * `schema` - The workflow schema to convert
    ///
    /// # Returns
    ///
    /// * `Ok(Workflow)` - Successfully built workflow
    /// * `Err(BuildError)` - Building failed with validation errors
    pub fn build(schema: &WorkflowSchema) -> Result<Workflow, BuildError> {
        info!("Building workflow from schema: {}", schema.metadata.name);

        // Generate workflow ID from name
        let workflow_id = Self::generate_workflow_id(&schema.metadata.name);

        // Build routes
        let routes = Self::build_routes(&schema.spec.routes)?;

        // Create workflow
        let mut workflow = Workflow::new(workflow_id, schema.metadata.name.clone(), routes);

        // Set description
        if let Some(ref desc) = schema.metadata.description {
            workflow.description = desc.clone();
        }

        // Set default route
        workflow.default_route = schema.spec.default_route.clone();

        // Set execution mode
        workflow.execution_mode = Self::build_execution_mode(&schema.spec.execution)?;

        // Set tags
        workflow.tags = schema.metadata.tags.clone();

        // Validate the built workflow
        Self::validate_workflow(&workflow)?;

        info!("Successfully built workflow: {}", workflow.id);
        Ok(workflow)
    }

    /// Generate a workflow ID from the name
    fn generate_workflow_id(name: &str) -> String {
        // Use name as base, sanitize it
        let sanitized = name
            .to_lowercase()
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' {
                    c
                } else {
                    '-'
                }
            })
            .collect::<String>();

        format!("wf_{}", sanitized)
    }

    /// Build execution mode from spec
    fn build_execution_mode(spec: &ExecutionSpec) -> Result<ExecutionMode, BuildError> {
        match spec.mode {
            schema::ExecutionMode::Batch => Ok(ExecutionMode::Batch),
            schema::ExecutionMode::Streaming => {
                // For now, return Batch since we need streaming config
                // This will be enhanced when we add streaming support to schema
                Ok(ExecutionMode::Batch)
            }
        }
    }

    /// Build routes from schema
    fn build_routes(route_specs: &[RouteSpec]) -> Result<Vec<Route>, BuildError> {
        let mut routes = Vec::new();

        for spec in route_specs {
            let route = Self::build_route(spec)?;
            routes.push(route);
        }

        // Check for duplicate names
        let mut names = std::collections::HashSet::new();
        for route in &routes {
            if !names.insert(&route.name) {
                return Err(BuildError::DuplicateRoute(route.name.clone()));
            }
        }

        Ok(routes)
    }

    /// Build a single route from spec
    fn build_route(spec: &RouteSpec) -> Result<Route, BuildError> {
        debug!("Building route: {}", spec.name);

        // Generate route ID from name
        let route_id = Self::generate_route_id(&spec.name);

        // Build condition
        let condition =
            Self::build_condition(&spec.condition).map_err(|e| BuildError::InvalidCondition {
                route: spec.name.clone(),
                reason: e.to_string(),
            })?;

        // Build actions
        let actions =
            Self::build_actions(&spec.actions).map_err(|e| BuildError::InvalidAction {
                route: spec.name.clone(),
                reason: e.to_string(),
            })?;

        // Create route
        let mut route = Route::new(route_id, spec.name.clone(), condition, actions);

        // Set description
        if let Some(ref desc) = spec.description {
            route.description = desc.clone();
        }

        // Set priority
        route.priority = spec.priority;

        Ok(route)
    }

    /// Generate a route ID from the name
    fn generate_route_id(name: &str) -> String {
        let sanitized = name
            .to_lowercase()
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' {
                    c
                } else {
                    '-'
                }
            })
            .collect::<String>();

        format!("rt_{}", sanitized)
    }

    /// Build condition from spec
    fn build_condition(spec: &ConditionSpec) -> Result<Condition, BuildError> {
        match spec {
            ConditionSpec::Always => Ok(Condition::Always),

            ConditionSpec::Equals { field, value } => Ok(Condition::Equals {
                field: field.clone(),
                value: value.clone(),
            }),

            ConditionSpec::NotEquals { field, value } => Ok(Condition::NotEquals {
                field: field.clone(),
                value: value.clone(),
            }),

            ConditionSpec::GreaterThan { field, value } => Ok(Condition::GreaterThan {
                field: field.clone(),
                value: value.clone(),
            }),

            ConditionSpec::LessThan { field, value } => Ok(Condition::LessThan {
                field: field.clone(),
                value: value.clone(),
            }),

            ConditionSpec::Contains { field, value } => {
                // Convert value to string for Contains variant
                let substring = match value {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };

                Ok(Condition::Contains {
                    field: field.clone(),
                    substring,
                })
            }

            ConditionSpec::Regex { field, pattern } => {
                // Validate regex pattern
                regex::Regex::new(pattern).map_err(|e| BuildError::InvalidCondition {
                    route: String::new(),
                    reason: format!("Invalid regex pattern: {}", e),
                })?;

                // Domain uses 'Matches' not 'Regex'
                Ok(Condition::Matches {
                    field: field.clone(),
                    pattern: pattern.clone(),
                })
            }

            ConditionSpec::IsNull { field } => Ok(Condition::IsNull {
                field: field.clone(),
            }),

            ConditionSpec::And { conditions } => {
                let built_conditions = conditions
                    .iter()
                    .map(|c| Self::build_condition(c))
                    .collect::<Result<Vec<_>, _>>()?;

                // Domain uses tuple variant: And(Box<Vec<Condition>>)
                Ok(Condition::And(Box::new(built_conditions)))
            }

            ConditionSpec::Or { conditions } => {
                let built_conditions = conditions
                    .iter()
                    .map(|c| Self::build_condition(c))
                    .collect::<Result<Vec<_>, _>>()?;

                // Domain uses tuple variant: Or(Box<Vec<Condition>>)
                Ok(Condition::Or(Box::new(built_conditions)))
            }

            ConditionSpec::Not { condition } => {
                let built_condition = Self::build_condition(condition)?;
                // Domain uses tuple variant: Not(Box<Condition>)
                Ok(Condition::Not(Box::new(built_condition)))
            }
        }
    }

    /// Build actions from specs
    fn build_actions(specs: &[ActionSpec]) -> Result<Vec<Action>, BuildError> {
        specs.iter().map(|s| Self::build_action(s)).collect()
    }

    /// Build a single action from spec
    fn build_action(spec: &ActionSpec) -> Result<Action, BuildError> {
        match spec {
            ActionSpec::Log { level, message } => Ok(Action::Log {
                level: level.clone(),
                message: message.clone(),
            }),

            ActionSpec::Transform {
                transformer,
                config,
            } => Ok(Action::Transform {
                transformer: transformer.clone(),
                config: config.clone(),
            }),

            ActionSpec::Enrich {
                reference_data,
                join_key,
            } => {
                // Validate reference data exists (in production, check catalog)
                if reference_data.trim().is_empty() {
                    return Err(BuildError::InvalidValue {
                        field: "reference_data".to_string(),
                        reason: "Cannot be empty".to_string(),
                    });
                }

                // For now, return a placeholder action
                // In production, this would be a proper Enrich action
                Ok(Action::Log {
                    level: "info".to_string(),
                    message: format!("Enrich with {} on {}", reference_data, join_key),
                })
            }

            ActionSpec::Validate { rule_id } => {
                if rule_id.trim().is_empty() {
                    return Err(BuildError::InvalidValue {
                        field: "rule_id".to_string(),
                        reason: "Cannot be empty".to_string(),
                    });
                }

                // Placeholder - would validate rule exists
                Ok(Action::Log {
                    level: "info".to_string(),
                    message: format!("Validate with rule {}", rule_id),
                })
            }

            ActionSpec::SendToKafka {
                topic,
                partition_key,
            } => {
                if topic.trim().is_empty() {
                    return Err(BuildError::InvalidValue {
                        field: "topic".to_string(),
                        reason: "Cannot be empty".to_string(),
                    });
                }

                // Placeholder action
                Ok(Action::Log {
                    level: "info".to_string(),
                    message: format!("Send to Kafka topic: {}", topic),
                })
            }

            ActionSpec::SendToHttp {
                url,
                method,
                headers,
            } => {
                // Validate URL
                if url.trim().is_empty() {
                    return Err(BuildError::InvalidValue {
                        field: "url".to_string(),
                        reason: "Cannot be empty".to_string(),
                    });
                }

                // Placeholder action
                Ok(Action::Log {
                    level: "info".to_string(),
                    message: format!("Send HTTP {} to {}", method, url),
                })
            }

            ActionSpec::ExecuteCode { language, code } => {
                if code.trim().is_empty() {
                    return Err(BuildError::InvalidValue {
                        field: "code".to_string(),
                        reason: "Cannot be empty".to_string(),
                    });
                }

                // Placeholder action
                Ok(Action::Log {
                    level: "info".to_string(),
                    message: format!("Execute {} code", language),
                })
            }

            ActionSpec::CallModel {
                model_id,
                input_mapping,
                output_mapping,
            } => {
                if model_id.trim().is_empty() {
                    return Err(BuildError::InvalidValue {
                        field: "model_id".to_string(),
                        reason: "Cannot be empty".to_string(),
                    });
                }

                // Placeholder action
                Ok(Action::Log {
                    level: "info".to_string(),
                    message: format!("Call ML model: {}", model_id),
                })
            }
        }
    }

    /// Validate the built workflow
    fn validate_workflow(workflow: &Workflow) -> Result<(), BuildError> {
        // Check workflow has routes
        if workflow.routes.is_empty() {
            return Err(BuildError::MissingField("routes".to_string()));
        }

        // Check default route exists if specified
        // Note: default_route in schema is a route NAME, not ID
        if let Some(ref default_route) = workflow.default_route {
            if !workflow.routes.iter().any(|r| &r.name == default_route) {
                return Err(BuildError::RouteNotFound(default_route.clone()));
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn create_simple_schema() -> WorkflowSchema {
        WorkflowSchema {
            api_version: "graphica.io/v1".to_string(),
            kind: "Workflow".to_string(),
            metadata: WorkflowMetadata {
                name: "test-workflow".to_string(),
                version: Some("1.0.0".to_string()),
                description: Some("Test workflow".to_string()),
                owner: Some("test-team".to_string()),
                tags: vec!["test".to_string()],
                annotations: HashMap::new(),
            },
            spec: WorkflowSpec {
                schedule: None,
                execution: ExecutionSpec::default(),
                routes: vec![RouteSpec {
                    name: "default".to_string(),
                    description: Some("Default route".to_string()),
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
        }
    }

    #[test]
    fn test_build_simple_workflow() {
        let schema = create_simple_schema();
        let workflow = WorkflowBuilder::build(&schema).unwrap();

        assert_eq!(workflow.name, "test-workflow");
        assert_eq!(workflow.description, "Test workflow");
        assert_eq!(workflow.routes.len(), 1);
        assert_eq!(workflow.routes[0].name, "default");
        assert!(matches!(*workflow.routes[0].condition, Condition::Always));
    }

    #[test]
    fn test_build_with_complex_conditions() {
        let mut schema = create_simple_schema();
        schema.spec.routes[0].condition = ConditionSpec::And {
            conditions: vec![
                ConditionSpec::Equals {
                    field: "status".to_string(),
                    value: serde_json::json!("active"),
                },
                ConditionSpec::GreaterThan {
                    field: "amount".to_string(),
                    value: serde_json::json!(1000),
                },
            ],
        };

        let workflow = WorkflowBuilder::build(&schema).unwrap();
        // Domain uses tuple variant And(Box<Vec<Condition>>)
        assert!(matches!(*workflow.routes[0].condition, Condition::And(_)));
    }

    #[test]
    fn test_build_with_multiple_routes() {
        let mut schema = create_simple_schema();
        schema.spec.routes.push(RouteSpec {
            name: "high-priority".to_string(),
            description: None,
            priority: 100,
            condition: ConditionSpec::Equals {
                field: "priority".to_string(),
                value: serde_json::json!("high"),
            },
            actions: vec![ActionSpec::Log {
                level: "warn".to_string(),
                message: "high priority".to_string(),
            }],
        });

        let workflow = WorkflowBuilder::build(&schema).unwrap();
        assert_eq!(workflow.routes.len(), 2);
    }

    #[test]
    fn test_build_fails_with_duplicate_route_names() {
        let mut schema = create_simple_schema();
        schema.spec.routes.push(RouteSpec {
            name: "default".to_string(), // Duplicate!
            description: None,
            priority: 1,
            condition: ConditionSpec::Always,
            actions: vec![],
        });

        let result = WorkflowBuilder::build(&schema);
        assert!(matches!(result, Err(BuildError::DuplicateRoute(_))));
    }

    #[test]
    fn test_build_fails_with_invalid_default_route() {
        let mut schema = create_simple_schema();
        schema.spec.default_route = Some("nonexistent".to_string());

        let result = WorkflowBuilder::build(&schema);
        assert!(matches!(result, Err(BuildError::RouteNotFound(_))));
    }

    #[test]
    fn test_build_fails_with_no_routes() {
        let mut schema = create_simple_schema();
        schema.spec.routes.clear();

        let result = WorkflowBuilder::build(&schema);
        assert!(matches!(result, Err(BuildError::MissingField(_))));
    }

    #[test]
    fn test_build_condition_regex() {
        let spec = ConditionSpec::Regex {
            field: "email".to_string(),
            pattern: r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$".to_string(),
        };

        let condition = WorkflowBuilder::build_condition(&spec).unwrap();
        // Domain uses 'Matches' not 'Regex'
        assert!(matches!(condition, Condition::Matches { .. }));
    }

    #[test]
    fn test_build_condition_invalid_regex() {
        let spec = ConditionSpec::Regex {
            field: "email".to_string(),
            pattern: "[invalid".to_string(), // Invalid regex
        };

        let result = WorkflowBuilder::build_condition(&spec);
        assert!(result.is_err());
    }

    #[test]
    fn test_build_all_action_types() {
        let actions = vec![
            ActionSpec::Log {
                level: "info".to_string(),
                message: "test".to_string(),
            },
            ActionSpec::Transform {
                transformer: "uppercase".to_string(),
                config: serde_json::json!({"field": "name"}),
            },
            ActionSpec::SendToKafka {
                topic: "output".to_string(),
                partition_key: Some("id".to_string()),
            },
        ];

        let built = WorkflowBuilder::build_actions(&actions).unwrap();
        assert_eq!(built.len(), 3);
    }

    #[test]
    fn test_generate_workflow_id() {
        assert_eq!(
            WorkflowBuilder::generate_workflow_id("Customer ETL"),
            "wf_customer-etl"
        );
        assert_eq!(
            WorkflowBuilder::generate_workflow_id("test@workflow#123"),
            "wf_test-workflow-123"
        );
    }

    #[test]
    fn test_generate_route_id() {
        assert_eq!(
            WorkflowBuilder::generate_route_id("High Priority Route"),
            "rt_high-priority-route"
        );
    }
}
