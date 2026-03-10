//! Workflow Routes - Conditional branches with associated actions

use super::{Action, Condition};
use serde::{Deserialize, Serialize};

/// Unique identifier for a route
pub type RouteId = String;

/// A conditional route within a workflow
///
/// Routes are evaluated in priority order. The first route whose condition
/// matches is selected for execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Route {
    /// Unique identifier
    pub id: RouteId,

    /// Human-readable name
    pub name: String,

    /// Optional description
    #[serde(default)]
    pub description: String,

    /// Condition that must be true for this route to match
    pub condition: Box<Condition>,

    /// Actions to execute when this route matches
    pub actions: Box<Vec<Action>>,

    /// Priority (higher = evaluated first)
    ///
    /// Routes with higher priority are checked before lower priority routes.
    /// Default priority is 0.
    #[serde(default)]
    pub priority: i32,

    /// Whether this route is enabled
    ///
    /// Disabled routes are skipped during evaluation.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

impl Route {
    /// Create a new route with default priority
    pub fn new(
        id: impl Into<RouteId>,
        name: impl Into<String>,
        condition: Condition,
        actions: Vec<Action>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: String::new(),
            condition: Box::new(condition),
            actions: Box::new(actions),
            priority: 0,
            enabled: true,
        }
    }

    /// Create a new route with specified priority
    pub fn with_priority(
        id: impl Into<RouteId>,
        name: impl Into<String>,
        condition: Condition,
        actions: Vec<Action>,
        priority: i32,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: String::new(),
            condition: Box::new(condition),
            actions: Box::new(actions),
            priority,
            enabled: true,
        }
    }

    /// Set description
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    /// Enable or disable this route
    pub fn set_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Validate route configuration
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.id.is_empty() {
            anyhow::bail!("Route ID cannot be empty");
        }

        if self.name.is_empty() {
            anyhow::bail!("Route name cannot be empty");
        }

        // Validate condition
        self.condition.validate()?;

        // Validate at least one action
        if self.actions.is_empty() {
            anyhow::bail!("Route must have at least one action");
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflows::domain::action::Action;
    use serde_json::json;

    #[test]
    fn test_new_route() {
        let route = Route::new(
            "rt_001",
            "test_route",
            Condition::Always,
            vec![Action::Log {
                level: "info".to_string(),
                message: "test".to_string(),
            }],
        );

        assert_eq!(route.id, "rt_001");
        assert_eq!(route.name, "test_route");
        assert_eq!(route.priority, 0);
        assert!(route.enabled);
    }

    #[test]
    fn test_route_with_priority() {
        let route = Route::with_priority(
            "rt_002",
            "high_priority",
            Condition::Always,
            vec![Action::Log {
                level: "info".to_string(),
                message: "test".to_string(),
            }],
            100,
        );

        assert_eq!(route.priority, 100);
    }

    #[test]
    fn test_route_with_description() {
        let route = Route::new(
            "rt_003",
            "test",
            Condition::Always,
            vec![Action::Log {
                level: "info".to_string(),
                message: "test".to_string(),
            }],
        )
        .with_description("This is a test route");

        assert_eq!(route.description, "This is a test route");
    }

    #[test]
    fn test_validate_valid_route() {
        let route = Route::new(
            "rt_004",
            "valid_route",
            Condition::equals("status", "active"),
            vec![Action::SendToKafka {
                topic: "test".to_string(),
                partition_key: None,
            }],
        );

        assert!(route.validate().is_ok());
    }

    #[test]
    fn test_validate_empty_id() {
        let route = Route::new(
            "",
            "test",
            Condition::Always,
            vec![Action::Log {
                level: "info".to_string(),
                message: "test".to_string(),
            }],
        );

        assert!(route.validate().is_err());
    }

    #[test]
    fn test_validate_empty_name() {
        let route = Route::new(
            "rt_005",
            "",
            Condition::Always,
            vec![Action::Log {
                level: "info".to_string(),
                message: "test".to_string(),
            }],
        );

        assert!(route.validate().is_err());
    }

    #[test]
    fn test_validate_no_actions() {
        let route = Route::new("rt_006", "test", Condition::Always, vec![]);

        assert!(route.validate().is_err());
    }

    #[test]
    fn test_validate_invalid_condition() {
        let route = Route::new(
            "rt_007",
            "test",
            Condition::Equals {
                field: String::new(), // Empty field
                value: json!("value"),
            },
            vec![Action::Log {
                level: "info".to_string(),
                message: "test".to_string(),
            }],
        );

        assert!(route.validate().is_err());
    }

    #[test]
    fn test_serde_route() {
        let route = Route::with_priority(
            "rt_008",
            "serde_test",
            Condition::equals("type", "customer"),
            vec![
                Action::SendToKafka {
                    topic: "customers".to_string(),
                    partition_key: Some("id".to_string()),
                },
                Action::RecordLineage {
                    event_type: "routing".to_string(),
                    metadata: json!({"route": "customer"}),
                },
            ],
            50,
        );

        let json = serde_json::to_string_pretty(&route).unwrap();
        let deserialized: Route = serde_json::from_str(&json).unwrap();

        assert_eq!(route, deserialized);
    }

    #[test]
    fn test_disabled_route() {
        let route = Route::new(
            "rt_009",
            "disabled",
            Condition::Always,
            vec![Action::Log {
                level: "info".to_string(),
                message: "test".to_string(),
            }],
        )
        .set_enabled(false);

        assert!(!route.enabled);
    }
}
