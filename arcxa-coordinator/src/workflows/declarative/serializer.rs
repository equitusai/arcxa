//! Workflow serializer - converts domain Workflow back to WorkflowSchema
//!
//! This module enables exporting workflows as YAML/JSON for version control.

use super::errors::BuildError;
use crate::workflows::domain::{Action, Condition, ExecutionMode, Route, Workflow};
use graphica_core::workflows::*;

/// Serializer for converting domain workflows to declarative schemas
pub struct WorkflowSerializer;

impl WorkflowSerializer {
    /// Convert a domain Workflow to WorkflowSchema
    pub fn to_schema(workflow: &Workflow) -> Result<WorkflowSchema, BuildError> {
        Ok(WorkflowSchema {
            api_version: "graphica.io/v1".to_string(),
            kind: "Workflow".to_string(),
            metadata: Self::build_metadata(workflow),
            spec: Self::build_spec(workflow)?,
        })
    }

    /// Serialize workflow to YAML string
    pub fn to_yaml(workflow: &Workflow) -> Result<String, BuildError> {
        let schema = Self::to_schema(workflow)?;
        serde_yaml::to_string(&schema)
            .map_err(|e| BuildError::Custom(format!("YAML serialization failed: {}", e)))
    }

    /// Serialize workflow to JSON string
    pub fn to_json(workflow: &Workflow) -> Result<String, BuildError> {
        let schema = Self::to_schema(workflow)?;
        serde_json::to_string_pretty(&schema)
            .map_err(|e| BuildError::Custom(format!("JSON serialization failed: {}", e)))
    }

    fn build_metadata(workflow: &Workflow) -> WorkflowMetadata {
        WorkflowMetadata {
            name: workflow.name.clone(),
            version: Some(workflow.version.to_string()),
            description: if workflow.description.is_empty() {
                None
            } else {
                Some(workflow.description.clone())
            },
            owner: None, // Not stored in domain model
            tags: workflow.tags.clone(),
            annotations: std::collections::HashMap::new(),
        }
    }

    fn build_spec(workflow: &Workflow) -> Result<WorkflowSpec, BuildError> {
        Ok(WorkflowSpec {
            schedule: None, // TODO: Extract from workflow if stored
            execution: Self::build_execution_spec(&workflow.execution_mode),
            routes: Self::build_routes(&workflow.routes)?,
            default_route: workflow.default_route.clone(),
            monitoring: None, // TODO: Extract monitoring config
            resources: None,  // TODO: Extract resource config
        })
    }

    fn build_execution_spec(mode: &ExecutionMode) -> ExecutionSpec {
        ExecutionSpec {
            mode: match mode {
                ExecutionMode::Batch => schema::ExecutionMode::Batch,
                ExecutionMode::Streaming { .. } => schema::ExecutionMode::Streaming,
                ExecutionMode::MicroBatch { .. } => schema::ExecutionMode::Batch, // Map to batch
            },
            timeout: 3600,
            retries: 0,
            retry_delay: 300,
        }
    }

    fn build_routes(routes: &[Route]) -> Result<Vec<RouteSpec>, BuildError> {
        routes.iter().map(|r| Self::build_route(r)).collect()
    }

    fn build_route(route: &Route) -> Result<RouteSpec, BuildError> {
        Ok(RouteSpec {
            name: route.name.clone(),
            description: if route.description.is_empty() {
                None
            } else {
                Some(route.description.clone())
            },
            priority: route.priority,
            condition: Self::build_condition(&route.condition)?,
            actions: Self::build_actions(&route.actions)?,
        })
    }

    fn build_condition(condition: &Condition) -> Result<ConditionSpec, BuildError> {
        match condition {
            Condition::Always => Ok(ConditionSpec::Always),
            Condition::Equals { field, value } => Ok(ConditionSpec::Equals {
                field: field.clone(),
                value: value.clone(),
            }),
            Condition::NotEquals { field, value } => Ok(ConditionSpec::NotEquals {
                field: field.clone(),
                value: value.clone(),
            }),
            Condition::GreaterThan { field, value } => Ok(ConditionSpec::GreaterThan {
                field: field.clone(),
                value: value.clone(),
            }),
            Condition::LessThan { field, value } => Ok(ConditionSpec::LessThan {
                field: field.clone(),
                value: value.clone(),
            }),
            Condition::Contains { field, substring } => Ok(ConditionSpec::Contains {
                field: field.clone(),
                value: serde_json::Value::String(substring.clone()),
            }),
            Condition::Matches { field, pattern } => Ok(ConditionSpec::Regex {
                field: field.clone(),
                pattern: pattern.clone(),
            }),
            Condition::IsNull { field } => Ok(ConditionSpec::IsNull {
                field: field.clone(),
            }),
            Condition::And(conditions) => Ok(ConditionSpec::And {
                conditions: conditions
                    .iter()
                    .map(|c| Self::build_condition(c))
                    .collect::<Result<Vec<_>, _>>()?,
            }),
            Condition::Or(conditions) => Ok(ConditionSpec::Or {
                conditions: conditions
                    .iter()
                    .map(|c| Self::build_condition(c))
                    .collect::<Result<Vec<_>, _>>()?,
            }),
            Condition::Not(condition) => Ok(ConditionSpec::Not {
                condition: Box::new(Self::build_condition(&**condition)?),
            }),

            // Variants not yet supported in declarative schema
            Condition::GreaterThanOrEqual { field, value } => {
                // Map to GreaterThan as approximation
                Ok(ConditionSpec::GreaterThan {
                    field: field.clone(),
                    value: value.clone(),
                })
            }
            Condition::LessThanOrEqual { field, value } => {
                // Map to LessThan as approximation
                Ok(ConditionSpec::LessThan {
                    field: field.clone(),
                    value: value.clone(),
                })
            }
            Condition::Exists { field } => {
                // Map to Not(IsNull) which is semantically equivalent
                Ok(ConditionSpec::Not {
                    condition: Box::new(ConditionSpec::IsNull {
                        field: field.clone(),
                    }),
                })
            }
            Condition::In { field, values } => {
                // Map to OR of Equals conditions
                let conditions: Vec<ConditionSpec> = values
                    .iter()
                    .map(|v| ConditionSpec::Equals {
                        field: field.clone(),
                        value: v.clone(),
                    })
                    .collect();
                Ok(ConditionSpec::Or { conditions })
            }
            Condition::Never => {
                // Map to Not(Always)
                Ok(ConditionSpec::Not {
                    condition: Box::new(ConditionSpec::Always),
                })
            }
        }
    }

    fn build_actions(actions: &[Action]) -> Result<Vec<ActionSpec>, BuildError> {
        actions.iter().map(|a| Self::build_action(a)).collect()
    }

    fn build_action(action: &Action) -> Result<ActionSpec, BuildError> {
        match action {
            Action::Log { level, message } => Ok(ActionSpec::Log {
                level: level.clone(),
                message: message.clone(),
            }),
            Action::Transform {
                transformer,
                config,
            } => Ok(ActionSpec::Transform {
                transformer: transformer.clone(),
                config: config.clone(),
            }),
            // Add other action types as needed
            _ => Err(BuildError::Custom(format!(
                "Action type not yet supported in serializer: {:?}",
                action
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflows::domain::{Action, Condition, Route, Workflow};

    #[test]
    fn test_serialize_simple_workflow() {
        let routes = vec![Route::new(
            "test_route",
            "Test Route",
            Condition::Always,
            vec![Action::Log {
                level: "info".to_string(),
                message: "test".to_string(),
            }],
        )];

        let workflow = Workflow::new("wf_test", "Test Workflow", routes);

        let schema = WorkflowSerializer::to_schema(&workflow).unwrap();
        assert_eq!(schema.metadata.name, "Test Workflow");
        assert_eq!(schema.spec.routes.len(), 1);
        assert!(matches!(
            schema.spec.routes[0].condition,
            ConditionSpec::Always
        ));
    }

    #[test]
    fn test_serialize_to_yaml() {
        let routes = vec![Route::new(
            "test",
            "Test",
            Condition::Always,
            vec![Action::Log {
                level: "info".to_string(),
                message: "test".to_string(),
            }],
        )];

        let workflow = Workflow::new("wf_test", "Test", routes);
        let yaml = WorkflowSerializer::to_yaml(&workflow).unwrap();

        assert!(yaml.contains("apiVersion:"));
        assert!(yaml.contains("kind: Workflow"));
        assert!(yaml.contains("name: Test"));
    }

    #[test]
    fn test_serialize_to_json() {
        let routes = vec![Route::new(
            "test",
            "Test",
            Condition::Always,
            vec![Action::Log {
                level: "info".to_string(),
                message: "test".to_string(),
            }],
        )];

        let workflow = Workflow::new("wf_test", "Test", routes);
        let json = WorkflowSerializer::to_json(&workflow).unwrap();

        assert!(json.contains("\"apiVersion\""));
        assert!(json.contains("\"kind\": \"Workflow\""));
        assert!(json.contains("\"name\": \"Test\""));
    }
}
