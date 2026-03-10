//! Workflow Definition - Complete workflow with routes and metadata

use super::{ExecutionMode, Route};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Unique identifier for a workflow
pub type WorkflowId = String;

/// Resource limits for workflow execution (Proposal 5 - Memory Management)
///
/// Prevents OOM crashes by enforcing memory and row count limits.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowResourceLimits {
    /// Maximum memory usage in bytes (None = unlimited)
    ///
    /// Recommended values:
    /// - Small datasets (<100K rows): 5GB = 5_000_000_000
    /// - Medium datasets (100K-500K rows): 20GB = 20_000_000_000
    /// - Large datasets (500K-1M rows): 50GB = 50_000_000_000
    #[serde(default)]
    pub max_memory_bytes: Option<usize>,

    /// Maximum number of rows to process (None = unlimited)
    ///
    /// Recommended values:
    /// - Development: 10_000
    /// - Testing: 100_000
    /// - Production (with monitoring): 1_000_000
    #[serde(default)]
    pub max_rows: Option<usize>,

    /// Whether to enforce limits strictly (true) or just warn (false)
    #[serde(default = "default_enforce_limits")]
    pub enforce_limits: bool,
}

fn default_enforce_limits() -> bool {
    true // Enforce by default to prevent OOM
}

impl Default for WorkflowResourceLimits {
    fn default() -> Self {
        Self {
            max_memory_bytes: Some(10_000_000_000), // 10GB default
            max_rows: Some(200_000),                // 200K rows default
            enforce_limits: true,
        }
    }
}

impl WorkflowResourceLimits {
    /// Create unlimited resource limits (use with caution!)
    pub fn unlimited() -> Self {
        Self {
            max_memory_bytes: None,
            max_rows: None,
            enforce_limits: false,
        }
    }

    /// Create strict limits for development/testing
    pub fn strict() -> Self {
        Self {
            max_memory_bytes: Some(5_000_000_000), // 5GB
            max_rows: Some(100_000),               // 100K rows
            enforce_limits: true,
        }
    }

    /// Create production limits with monitoring
    pub fn production() -> Self {
        Self {
            max_memory_bytes: Some(50_000_000_000), // 50GB
            max_rows: Some(1_000_000),              // 1M rows
            enforce_limits: true,
        }
    }
}

/// A complete workflow definition
///
/// Workflows contain multiple routes that are evaluated in priority order.
/// The first matching route is executed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Workflow {
    /// Unique identifier
    pub id: WorkflowId,

    /// Human-readable name
    pub name: String,

    /// Detailed description
    #[serde(default)]
    pub description: String,

    /// Ordered list of routes (evaluated by priority)
    pub routes: Box<Vec<Route>>,

    /// Default route ID to use if no conditions match
    #[serde(default)]
    pub default_route: Option<String>,

    /// Execution mode (Batch, Streaming, or MicroBatch)
    ///
    /// Determines how the workflow processes data.
    /// Defaults to Batch for backward compatibility with existing workflows.
    #[serde(default)]
    pub execution_mode: ExecutionMode,

    /// Workflow version (for tracking changes)
    #[serde(default = "default_version")]
    pub version: u32,

    /// Creation timestamp
    #[serde(default = "Utc::now")]
    pub created_at: DateTime<Utc>,

    /// Last update timestamp
    #[serde(default = "Utc::now")]
    pub updated_at: DateTime<Utc>,

    /// Whether this workflow is enabled
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Tags for organization and filtering
    #[serde(default)]
    pub tags: Vec<String>,

    /// Resource limits for execution (Proposal 5 - Memory Management)
    ///
    /// Prevents OOM crashes by enforcing memory and row count limits.
    /// Defaults to 10GB/200K rows if not specified.
    #[serde(default)]
    pub resource_limits: WorkflowResourceLimits,
}

fn default_version() -> u32 {
    1
}

fn default_enabled() -> bool {
    true
}

impl Workflow {
    /// Create a new workflow
    pub fn new(id: impl Into<WorkflowId>, name: impl Into<String>, routes: Vec<Route>) -> Self {
        let now = Utc::now();
        Self {
            id: id.into(),
            name: name.into(),
            description: String::new(),
            routes: Box::new(routes),
            default_route: None,
            execution_mode: ExecutionMode::Batch,
            version: 1,
            created_at: now,
            updated_at: now,
            enabled: true,
            tags: Vec::new(),
            resource_limits: WorkflowResourceLimits::default(),
        }
    }

    /// Set description
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    /// Set default route
    pub fn with_default_route(mut self, route_id: impl Into<String>) -> Self {
        self.default_route = Some(route_id.into());
        self
    }

    /// Add tags
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    /// Set resource limits (Proposal 5 - Memory Management)
    pub fn with_resource_limits(mut self, limits: WorkflowResourceLimits) -> Self {
        self.resource_limits = limits;
        self
    }

    /// Get routes sorted by priority (descending)
    pub fn routes_by_priority(&self) -> Vec<&Route> {
        let mut routes: Vec<&Route> = self.routes.iter().collect();
        routes.sort_by(|a, b| b.priority.cmp(&a.priority));
        routes
    }

    /// Get only enabled routes
    pub fn enabled_routes(&self) -> Vec<&Route> {
        self.routes.iter().filter(|r| r.enabled).collect()
    }

    /// Find route by ID
    pub fn find_route(&self, route_id: &str) -> Option<&Route> {
        self.routes.iter().find(|r| r.id == route_id)
    }

    /// Increment version (for updates)
    pub fn increment_version(&mut self) {
        self.version += 1;
        self.updated_at = Utc::now();
    }

    /// Validate workflow configuration
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.id.is_empty() {
            anyhow::bail!("Workflow ID cannot be empty");
        }

        if self.name.is_empty() {
            anyhow::bail!("Workflow name cannot be empty");
        }

        if self.routes.is_empty() {
            anyhow::bail!("Workflow must have at least one route");
        }

        // Validate all routes
        for route in self.routes.iter() {
            route.validate()?;
        }

        // Validate default route exists
        if let Some(ref default_id) = self.default_route {
            if !self.routes.iter().any(|r| r.id == *default_id) {
                anyhow::bail!(
                    "Default route '{}' not found in workflow routes",
                    default_id
                );
            }
        }

        // Check for duplicate route IDs
        let mut ids = std::collections::HashSet::new();
        for route in self.routes.iter() {
            if !ids.insert(&route.id) {
                anyhow::bail!("Duplicate route ID: {}", route.id);
            }
        }

        Ok(())
    }
}

/// Summary of a workflow (for list views)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowSummary {
    pub id: WorkflowId,
    pub name: String,
    pub description: String,
    pub version: u32,
    pub enabled: bool,
    pub route_count: usize,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub tags: Vec<String>,
}

impl From<&Workflow> for WorkflowSummary {
    fn from(workflow: &Workflow) -> Self {
        Self {
            id: workflow.id.clone(),
            name: workflow.name.clone(),
            description: workflow.description.clone(),
            version: workflow.version,
            enabled: workflow.enabled,
            route_count: workflow.routes.len(),
            created_at: workflow.created_at,
            updated_at: workflow.updated_at,
            tags: workflow.tags.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflows::domain::{Action, Condition};

    fn create_test_route(id: &str, priority: i32) -> Route {
        Route::with_priority(
            id,
            format!("Route {}", id),
            Condition::Always,
            vec![Action::Log {
                level: "info".to_string(),
                message: format!("Route {} executed", id),
            }],
            priority,
        )
    }

    #[test]
    fn test_new_workflow() {
        let workflow = Workflow::new(
            "wf_001",
            "test_workflow",
            vec![create_test_route("rt_001", 10)],
        );

        assert_eq!(workflow.id, "wf_001");
        assert_eq!(workflow.name, "test_workflow");
        assert_eq!(workflow.version, 1);
        assert!(workflow.enabled);
        assert_eq!(workflow.routes.len(), 1);
    }

    #[test]
    fn test_workflow_with_description() {
        let workflow = Workflow::new("wf_002", "test", vec![create_test_route("rt_001", 10)])
            .with_description("This is a test workflow");

        assert_eq!(workflow.description, "This is a test workflow");
    }

    #[test]
    fn test_workflow_with_default_route() {
        let workflow = Workflow::new(
            "wf_003",
            "test",
            vec![
                create_test_route("rt_001", 10),
                create_test_route("rt_002", 5),
            ],
        )
        .with_default_route("rt_002");

        assert_eq!(workflow.default_route, Some("rt_002".to_string()));
    }

    #[test]
    fn test_routes_by_priority() {
        let workflow = Workflow::new(
            "wf_004",
            "test",
            vec![
                create_test_route("rt_001", 5),
                create_test_route("rt_002", 20),
                create_test_route("rt_003", 10),
            ],
        );

        let sorted = workflow.routes_by_priority();
        assert_eq!(sorted[0].id, "rt_002"); // priority 20
        assert_eq!(sorted[1].id, "rt_003"); // priority 10
        assert_eq!(sorted[2].id, "rt_001"); // priority 5
    }

    #[test]
    fn test_enabled_routes() {
        let mut routes = vec![
            create_test_route("rt_001", 10),
            create_test_route("rt_002", 5),
        ];
        routes[1].enabled = false;

        let workflow = Workflow::new("wf_005", "test", routes);

        let enabled = workflow.enabled_routes();
        assert_eq!(enabled.len(), 1);
        assert_eq!(enabled[0].id, "rt_001");
    }

    #[test]
    fn test_find_route() {
        let workflow = Workflow::new(
            "wf_006",
            "test",
            vec![
                create_test_route("rt_001", 10),
                create_test_route("rt_002", 5),
            ],
        );

        assert!(workflow.find_route("rt_001").is_some());
        assert!(workflow.find_route("rt_002").is_some());
        assert!(workflow.find_route("rt_999").is_none());
    }

    #[test]
    fn test_increment_version() {
        let mut workflow = Workflow::new("wf_007", "test", vec![create_test_route("rt_001", 10)]);

        let original_version = workflow.version;
        let original_updated = workflow.updated_at;

        std::thread::sleep(std::time::Duration::from_millis(10));
        workflow.increment_version();

        assert_eq!(workflow.version, original_version + 1);
        assert!(workflow.updated_at > original_updated);
    }

    #[test]
    fn test_validate_valid_workflow() {
        let workflow = Workflow::new(
            "wf_008",
            "valid_workflow",
            vec![
                create_test_route("rt_001", 10),
                create_test_route("rt_002", 5),
            ],
        );

        assert!(workflow.validate().is_ok());
    }

    #[test]
    fn test_validate_empty_id() {
        let workflow = Workflow::new("", "test", vec![create_test_route("rt_001", 10)]);

        assert!(workflow.validate().is_err());
    }

    #[test]
    fn test_validate_empty_name() {
        let workflow = Workflow::new("wf_009", "", vec![create_test_route("rt_001", 10)]);

        assert!(workflow.validate().is_err());
    }

    #[test]
    fn test_validate_no_routes() {
        let workflow = Workflow::new("wf_010", "test", vec![]);

        assert!(workflow.validate().is_err());
    }

    #[test]
    fn test_validate_invalid_default_route() {
        let workflow = Workflow::new("wf_011", "test", vec![create_test_route("rt_001", 10)])
            .with_default_route("rt_999");

        assert!(workflow.validate().is_err());
    }

    #[test]
    fn test_validate_duplicate_route_ids() {
        let workflow = Workflow::new(
            "wf_012",
            "test",
            vec![
                create_test_route("rt_001", 10),
                create_test_route("rt_001", 5), // Duplicate ID
            ],
        );

        assert!(workflow.validate().is_err());
    }

    #[test]
    fn test_workflow_summary() {
        let workflow = Workflow::new(
            "wf_013",
            "summary_test",
            vec![
                create_test_route("rt_001", 10),
                create_test_route("rt_002", 5),
            ],
        )
        .with_description("Test workflow for summary")
        .with_tags(vec!["test".to_string(), "quality".to_string()]);

        let summary = WorkflowSummary::from(&workflow);

        assert_eq!(summary.id, "wf_013");
        assert_eq!(summary.name, "summary_test");
        assert_eq!(summary.route_count, 2);
        assert_eq!(summary.tags.len(), 2);
    }

    #[test]
    fn test_serde_workflow() {
        let workflow = Workflow::new(
            "wf_014",
            "serde_test",
            vec![
                create_test_route("rt_001", 10),
                create_test_route("rt_002", 5),
            ],
        )
        .with_description("Test serialization")
        .with_default_route("rt_002");

        let json = serde_json::to_string_pretty(&workflow).unwrap();
        let deserialized: Workflow = serde_json::from_str(&json).unwrap();

        assert_eq!(workflow.id, deserialized.id);
        assert_eq!(workflow.name, deserialized.name);
        assert_eq!(workflow.routes.len(), deserialized.routes.len());
    }
}
