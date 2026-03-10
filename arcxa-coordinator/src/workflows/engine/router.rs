//! Workflow Router - Select matching routes based on conditions
//!
//! Routes workflows based on priority-ordered condition evaluation.

use crate::workflows::domain::{Route, Workflow};
use crate::workflows::engine::evaluator::ConditionEvaluator;
use anyhow::{Context, Result};
use serde_json::Value as JsonValue;
use std::time::Instant;
use tracing::{debug, info, warn};

/// Routes workflow execution based on conditions
pub struct WorkflowRouter;

/// Result of route selection
#[derive(Debug, Clone)]
pub struct RouteMatch<'a> {
    pub route: &'a Route,
    pub evaluation_time_ms: u64,
}

impl WorkflowRouter {
    /// Select the first matching route from a workflow
    ///
    /// Routes are evaluated in priority order (highest first).
    /// Returns the first route whose condition evaluates to true.
    ///
    /// ## Performance
    /// - Best case: O(1) if first route matches
    /// - Worst case: O(n) where n = number of routes
    /// - Average: O(log n) for well-designed workflows
    ///
    /// ## Arguments
    /// * `workflow` - Workflow to evaluate
    /// * `data` - Input data for condition evaluation
    ///
    /// ## Returns
    /// The first matching route, or None if no conditions match
    pub fn select_route<'a>(
        workflow: &'a Workflow,
        data: &JsonValue,
    ) -> Result<Option<RouteMatch<'a>>> {
        let start = Instant::now();

        if !workflow.enabled {
            anyhow::bail!("Workflow '{}' is disabled", workflow.name);
        }

        // Get routes sorted by priority (descending)
        let routes = workflow.routes_by_priority();
        let enabled_routes: Vec<&Route> = routes.into_iter().filter(|r| r.enabled).collect();

        debug!(
            "Evaluating {} enabled routes for workflow '{}'",
            enabled_routes.len(),
            workflow.name
        );

        // Evaluate routes in priority order
        for (idx, route) in enabled_routes.iter().enumerate() {
            debug!(
                "Evaluating route '{}' (priority: {}, position: {})",
                route.name,
                route.priority,
                idx + 1
            );

            match ConditionEvaluator::evaluate(&route.condition, data) {
                Ok(true) => {
                    let elapsed = start.elapsed().as_millis() as u64;
                    info!(
                        "Route '{}' matched for workflow '{}' (evaluation time: {}ms)",
                        route.name, workflow.name, elapsed
                    );

                    return Ok(Some(RouteMatch {
                        route,
                        evaluation_time_ms: elapsed,
                    }));
                }
                Ok(false) => {
                    debug!("Route '{}' condition evaluated to false", route.name);
                    continue;
                }
                Err(e) => {
                    warn!(
                        "Error evaluating route '{}': {}. Skipping route.",
                        route.name, e
                    );
                    continue;
                }
            }
        }

        // No routes matched, check for default route
        if let Some(ref default_id) = workflow.default_route {
            if let Some(default_route) = workflow.find_route(default_id) {
                let elapsed = start.elapsed().as_millis() as u64;
                info!(
                    "No routes matched, using default route '{}' (evaluation time: {}ms)",
                    default_route.name, elapsed
                );

                return Ok(Some(RouteMatch {
                    route: default_route,
                    evaluation_time_ms: elapsed,
                }));
            }
        }

        let elapsed = start.elapsed().as_millis() as u64;
        info!(
            "No routes matched for workflow '{}' (evaluation time: {}ms)",
            workflow.name, elapsed
        );

        Ok(None)
    }

    /// Evaluate all routes and return all matches (for debugging/testing)
    ///
    /// Unlike `select_route`, this evaluates ALL routes regardless of matches.
    /// Useful for workflow testing and validation.
    pub fn evaluate_all_routes<'a>(
        workflow: &'a Workflow,
        data: &JsonValue,
    ) -> Result<Vec<(&'a Route, bool)>> {
        let mut results = Vec::new();

        for route in workflow.routes.iter() {
            if !route.enabled {
                continue;
            }

            match ConditionEvaluator::evaluate(&route.condition, data) {
                Ok(matched) => {
                    results.push((route, matched));
                }
                Err(e) => {
                    warn!("Error evaluating route '{}': {}", route.name, e);
                    results.push((route, false));
                }
            }
        }

        Ok(results)
    }

    /// Get route selection statistics for a workflow
    ///
    /// Simulates route evaluation with sample data to provide insights
    /// into route selection patterns.
    pub fn get_route_stats(workflow: &Workflow, sample_data: &[JsonValue]) -> RouteStats {
        let mut stats = RouteStats {
            total_samples: sample_data.len(),
            route_matches: std::collections::HashMap::new(),
            no_match_count: 0,
            default_route_count: 0,
            error_count: 0,
        };

        for data in sample_data {
            match Self::select_route(workflow, data) {
                Ok(Some(route_match)) => {
                    let counter = stats
                        .route_matches
                        .entry(route_match.route.id.clone())
                        .or_insert(0);
                    *counter += 1;

                    // Check if this was the default route
                    if workflow.default_route.as_ref() == Some(&route_match.route.id) {
                        stats.default_route_count += 1;
                    }
                }
                Ok(None) => {
                    stats.no_match_count += 1;
                }
                Err(_) => {
                    stats.error_count += 1;
                }
            }
        }

        stats
    }
}

/// Statistics about route selection patterns
#[derive(Debug, Clone)]
pub struct RouteStats {
    pub total_samples: usize,
    pub route_matches: std::collections::HashMap<String, usize>,
    pub no_match_count: usize,
    pub default_route_count: usize,
    pub error_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflows::domain::{Action, Condition, Route, Workflow};
    use serde_json::json;

    fn create_test_workflow() -> Workflow {
        let routes = vec![
            Route::with_priority(
                "rt_high_priority",
                "High Priority",
                Condition::equals("priority", "high"),
                vec![Action::Log {
                    level: "info".to_string(),
                    message: "High priority".to_string(),
                }],
                100,
            ),
            Route::with_priority(
                "rt_medium_priority",
                "Medium Priority",
                Condition::equals("priority", "medium"),
                vec![Action::Log {
                    level: "info".to_string(),
                    message: "Medium priority".to_string(),
                }],
                50,
            ),
            Route::with_priority(
                "rt_low_priority",
                "Low Priority",
                Condition::equals("priority", "low"),
                vec![Action::Log {
                    level: "info".to_string(),
                    message: "Low priority".to_string(),
                }],
                10,
            ),
            Route::new(
                "rt_default",
                "Default Route",
                Condition::Always,
                vec![Action::Log {
                    level: "info".to_string(),
                    message: "Default".to_string(),
                }],
            ),
        ];

        Workflow::new("wf_test", "Test Workflow", routes).with_default_route("rt_default")
    }

    #[test]
    fn test_select_route_first_match() {
        let workflow = create_test_workflow();
        let data = json!({"priority": "high"});

        let result = WorkflowRouter::select_route(&workflow, &data).unwrap();

        assert!(result.is_some());
        let route_match = result.unwrap();
        assert_eq!(route_match.route.id, "rt_high_priority");
    }

    #[test]
    fn test_select_route_by_priority() {
        let workflow = create_test_workflow();
        let data = json!({"priority": "medium"});

        let result = WorkflowRouter::select_route(&workflow, &data).unwrap();

        assert!(result.is_some());
        let route_match = result.unwrap();
        assert_eq!(route_match.route.id, "rt_medium_priority");
    }

    #[test]
    fn test_select_route_default() {
        let workflow = create_test_workflow();
        let data = json!({"priority": "unknown"});

        let result = WorkflowRouter::select_route(&workflow, &data).unwrap();

        assert!(result.is_some());
        let route_match = result.unwrap();
        assert_eq!(route_match.route.id, "rt_default");
    }

    #[test]
    fn test_select_route_no_match_no_default() {
        let routes = vec![Route::new(
            "rt_001",
            "Specific Route",
            Condition::equals("type", "specific"),
            vec![Action::Log {
                level: "info".to_string(),
                message: "test".to_string(),
            }],
        )];

        let workflow = Workflow::new("wf_no_default", "No Default", routes);
        let data = json!({"type": "other"});

        let result = WorkflowRouter::select_route(&workflow, &data).unwrap();

        assert!(result.is_none());
    }

    #[test]
    fn test_select_route_disabled_workflow() {
        let mut workflow = create_test_workflow();
        workflow.enabled = false;

        let data = json!({"priority": "high"});

        let result = WorkflowRouter::select_route(&workflow, &data);

        assert!(result.is_err());
    }

    #[test]
    fn test_select_route_disabled_route() {
        let mut routes = vec![
            Route::with_priority(
                "rt_disabled",
                "Disabled",
                Condition::Always,
                vec![Action::Log {
                    level: "info".to_string(),
                    message: "test".to_string(),
                }],
                100,
            ),
            Route::new(
                "rt_enabled",
                "Enabled",
                Condition::Always,
                vec![Action::Log {
                    level: "info".to_string(),
                    message: "test".to_string(),
                }],
            ),
        ];
        routes[0].enabled = false;

        let workflow = Workflow::new("wf_disabled_route", "Test", routes);
        let data = json!({});

        let result = WorkflowRouter::select_route(&workflow, &data).unwrap();

        assert!(result.is_some());
        assert_eq!(result.unwrap().route.id, "rt_enabled");
    }

    #[test]
    fn test_select_route_complex_conditions() {
        let routes = vec![
            Route::with_priority(
                "rt_enterprise",
                "Enterprise",
                Condition::and(vec![
                    Condition::equals("customer_type", "enterprise"),
                    Condition::greater_than("annual_revenue", 1000000),
                ]),
                vec![Action::SendToKafka {
                    topic: "enterprise".to_string(),
                    partition_key: None,
                }],
                100,
            ),
            Route::new(
                "rt_standard",
                "Standard",
                Condition::Always,
                vec![Action::SendToKafka {
                    topic: "standard".to_string(),
                    partition_key: None,
                }],
            ),
        ];

        let workflow = Workflow::new("wf_customer_routing", "Customer Routing", routes);

        // Should match enterprise route
        let data = json!({
            "customer_type": "enterprise",
            "annual_revenue": 5000000
        });
        let result = WorkflowRouter::select_route(&workflow, &data).unwrap();
        assert_eq!(result.unwrap().route.id, "rt_enterprise");

        // Should match standard route
        let data = json!({
            "customer_type": "enterprise",
            "annual_revenue": 500000
        });
        let result = WorkflowRouter::select_route(&workflow, &data).unwrap();
        assert_eq!(result.unwrap().route.id, "rt_standard");
    }

    #[test]
    fn test_evaluate_all_routes() {
        let workflow = create_test_workflow();
        let data = json!({"priority": "high"});

        let results = WorkflowRouter::evaluate_all_routes(&workflow, &data).unwrap();

        // Should have 4 enabled routes
        assert_eq!(results.len(), 4);

        // Only high priority route should match
        assert!(results[0].1); // rt_high_priority
        assert!(!results[1].1); // rt_medium_priority
        assert!(!results[2].1); // rt_low_priority
        assert!(results[3].1); // rt_default (Always)
    }

    #[test]
    fn test_route_stats() {
        let workflow = create_test_workflow();

        let sample_data = vec![
            json!({"priority": "high"}),
            json!({"priority": "high"}),
            json!({"priority": "medium"}),
            json!({"priority": "low"}),
            json!({"priority": "unknown"}),
        ];

        let stats = WorkflowRouter::get_route_stats(&workflow, &sample_data);

        assert_eq!(stats.total_samples, 5);
        assert_eq!(*stats.route_matches.get("rt_high_priority").unwrap(), 2);
        assert_eq!(*stats.route_matches.get("rt_medium_priority").unwrap(), 1);
        assert_eq!(*stats.route_matches.get("rt_low_priority").unwrap(), 1);
        assert_eq!(*stats.route_matches.get("rt_default").unwrap(), 1);
        assert_eq!(stats.default_route_count, 1);
        assert_eq!(stats.no_match_count, 0);
    }

    #[test]
    fn test_route_match_timing() {
        let workflow = create_test_workflow();
        let data = json!({"priority": "high"});

        let result = WorkflowRouter::select_route(&workflow, &data).unwrap();

        assert!(result.is_some());
        let route_match = result.unwrap();

        // Evaluation should be very fast (< 10ms for simple conditions)
        assert!(route_match.evaluation_time_ms < 10);
    }

    #[test]
    fn test_nested_field_routing() {
        let routes = vec![Route::new(
            "rt_nyc",
            "New York",
            Condition::equals("user.address.city", "New York"),
            vec![Action::Log {
                level: "info".to_string(),
                message: "NYC user".to_string(),
            }],
        )];

        let workflow = Workflow::new("wf_location", "Location Routing", routes);

        let data = json!({
            "user": {
                "name": "Alice",
                "address": {
                    "city": "New York",
                    "state": "NY"
                }
            }
        });

        let result = WorkflowRouter::select_route(&workflow, &data).unwrap();

        assert!(result.is_some());
        assert_eq!(result.unwrap().route.id, "rt_nyc");
    }
}
