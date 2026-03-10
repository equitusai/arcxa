//! Workflow Storage - CRUD operations for workflows
//!
//! In-memory storage with future support for RDF persistence.

use crate::workflows::domain::{Workflow, WorkflowId, WorkflowSummary};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// In-memory workflow storage
///
/// Thread-safe storage using RwLock for concurrent access.
/// Future: Replace with RDF triple store backend.
#[derive(Clone)]
pub struct WorkflowStore {
    workflows: Arc<RwLock<HashMap<WorkflowId, Workflow>>>,
}

impl Default for WorkflowStore {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkflowStore {
    /// Create a new empty workflow store
    pub fn new() -> Self {
        Self {
            workflows: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a workflow
    ///
    /// ## Errors
    /// - If workflow ID already exists
    /// - If workflow validation fails
    pub fn create(&self, workflow: Workflow) -> Result<()> {
        // Validate workflow
        workflow.validate()?;

        let mut workflows = self
            .workflows
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;

        if workflows.contains_key(&workflow.id) {
            anyhow::bail!("Workflow '{}' already exists", workflow.id);
        }

        workflows.insert(workflow.id.clone(), workflow);

        Ok(())
    }

    /// Get a workflow by ID
    pub fn get(&self, workflow_id: &str) -> Result<Option<Workflow>> {
        let workflows = self
            .workflows
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;

        Ok(workflows.get(workflow_id).cloned())
    }

    /// Get a workflow by ID (required, returns error if not found)
    pub fn get_required(&self, workflow_id: &str) -> Result<Workflow> {
        self.get(workflow_id)?
            .ok_or_else(|| anyhow::anyhow!("Workflow '{}' not found", workflow_id))
    }

    /// Update a workflow
    ///
    /// Increments version number automatically.
    ///
    /// ## Errors
    /// - If workflow doesn't exist
    /// - If workflow validation fails
    pub fn update(&self, workflow_id: &str, mut workflow: Workflow) -> Result<()> {
        // Validate workflow
        workflow.validate()?;

        let mut workflows = self
            .workflows
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;

        if !workflows.contains_key(workflow_id) {
            anyhow::bail!("Workflow '{}' not found", workflow_id);
        }

        // Increment version
        workflow.increment_version();

        workflows.insert(workflow_id.to_string(), workflow);

        Ok(())
    }

    /// Delete a workflow
    ///
    /// ## Errors
    /// - If workflow doesn't exist
    pub fn delete(&self, workflow_id: &str) -> Result<()> {
        let mut workflows = self
            .workflows
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;

        if workflows.remove(workflow_id).is_none() {
            anyhow::bail!("Workflow '{}' not found", workflow_id);
        }

        Ok(())
    }

    /// List all workflows
    pub fn list(&self) -> Result<Vec<WorkflowSummary>> {
        let workflows = self
            .workflows
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;

        let summaries = workflows.values().map(WorkflowSummary::from).collect();

        Ok(summaries)
    }

    /// List workflows with filters
    pub fn list_filtered(
        &self,
        enabled_only: bool,
        tags: Option<&[String]>,
    ) -> Result<Vec<WorkflowSummary>> {
        let workflows = self
            .workflows
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;

        let filtered: Vec<WorkflowSummary> = workflows
            .values()
            .filter(|w| {
                // Filter by enabled status
                if enabled_only && !w.enabled {
                    return false;
                }

                // Filter by tags
                if let Some(filter_tags) = tags {
                    if !filter_tags.iter().any(|tag| w.tags.contains(tag)) {
                        return false;
                    }
                }

                true
            })
            .map(WorkflowSummary::from)
            .collect();

        Ok(filtered)
    }

    /// Count total workflows
    pub fn count(&self) -> Result<usize> {
        let workflows = self
            .workflows
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;

        Ok(workflows.len())
    }

    /// Check if a workflow exists
    pub fn exists(&self, workflow_id: &str) -> Result<bool> {
        let workflows = self
            .workflows
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;

        Ok(workflows.contains_key(workflow_id))
    }

    /// Clear all workflows (for testing)
    #[cfg(test)]
    pub fn clear(&self) -> Result<()> {
        let mut workflows = self
            .workflows
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;

        workflows.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflows::domain::{Action, Condition, Route};

    fn create_test_workflow(id: &str) -> Workflow {
        let routes = vec![Route::new(
            "rt_001",
            "Test Route",
            Condition::Always,
            vec![Action::Log {
                level: "info".to_string(),
                message: "test".to_string(),
            }],
        )];

        Workflow::new(id, format!("Workflow {}", id), routes)
    }

    #[test]
    fn test_create_workflow() {
        let store = WorkflowStore::new();
        let workflow = create_test_workflow("wf_001");

        assert!(store.create(workflow).is_ok());
    }

    #[test]
    fn test_create_duplicate_workflow() {
        let store = WorkflowStore::new();
        let workflow = create_test_workflow("wf_001");

        store.create(workflow.clone()).unwrap();

        // Second create should fail
        let result = store.create(workflow);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_workflow() {
        let store = WorkflowStore::new();
        let workflow = create_test_workflow("wf_001");

        store.create(workflow.clone()).unwrap();

        let retrieved = store.get("wf_001").unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().id, "wf_001");
    }

    #[test]
    fn test_get_nonexistent_workflow() {
        let store = WorkflowStore::new();

        let retrieved = store.get("wf_999").unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_get_required_workflow() {
        let store = WorkflowStore::new();
        let workflow = create_test_workflow("wf_001");

        store.create(workflow).unwrap();

        let retrieved = store.get_required("wf_001").unwrap();
        assert_eq!(retrieved.id, "wf_001");
    }

    #[test]
    fn test_get_required_nonexistent() {
        let store = WorkflowStore::new();

        let result = store.get_required("wf_999");
        assert!(result.is_err());
    }

    #[test]
    fn test_update_workflow() {
        let store = WorkflowStore::new();
        let mut workflow = create_test_workflow("wf_001");

        store.create(workflow.clone()).unwrap();

        // Modify workflow
        workflow.description = "Updated description".to_string();
        let original_version = workflow.version;

        store.update("wf_001", workflow).unwrap();

        // Verify update
        let updated = store.get("wf_001").unwrap().unwrap();
        assert_eq!(updated.description, "Updated description");
        assert_eq!(updated.version, original_version + 1);
    }

    #[test]
    fn test_update_nonexistent_workflow() {
        let store = WorkflowStore::new();
        let workflow = create_test_workflow("wf_001");

        let result = store.update("wf_001", workflow);
        assert!(result.is_err());
    }

    #[test]
    fn test_delete_workflow() {
        let store = WorkflowStore::new();
        let workflow = create_test_workflow("wf_001");

        store.create(workflow).unwrap();
        assert!(store.exists("wf_001").unwrap());

        store.delete("wf_001").unwrap();
        assert!(!store.exists("wf_001").unwrap());
    }

    #[test]
    fn test_delete_nonexistent_workflow() {
        let store = WorkflowStore::new();

        let result = store.delete("wf_999");
        assert!(result.is_err());
    }

    #[test]
    fn test_list_workflows() {
        let store = WorkflowStore::new();

        store.create(create_test_workflow("wf_001")).unwrap();
        store.create(create_test_workflow("wf_002")).unwrap();
        store.create(create_test_workflow("wf_003")).unwrap();

        let summaries = store.list().unwrap();
        assert_eq!(summaries.len(), 3);
    }

    #[test]
    fn test_list_filtered_by_enabled() {
        let store = WorkflowStore::new();

        let mut wf1 = create_test_workflow("wf_001");
        wf1.enabled = true;
        store.create(wf1).unwrap();

        let mut wf2 = create_test_workflow("wf_002");
        wf2.enabled = false;
        store.create(wf2).unwrap();

        let summaries = store.list_filtered(true, None).unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].id, "wf_001");
    }

    #[test]
    fn test_list_filtered_by_tags() {
        let store = WorkflowStore::new();

        let wf1 = create_test_workflow("wf_001")
            .with_tags(vec!["quality".to_string(), "validation".to_string()]);
        store.create(wf1).unwrap();

        let wf2 = create_test_workflow("wf_002").with_tags(vec!["routing".to_string()]);
        store.create(wf2).unwrap();

        let summaries = store
            .list_filtered(false, Some(&["quality".to_string()]))
            .unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].id, "wf_001");
    }

    #[test]
    fn test_count() {
        let store = WorkflowStore::new();

        assert_eq!(store.count().unwrap(), 0);

        store.create(create_test_workflow("wf_001")).unwrap();
        assert_eq!(store.count().unwrap(), 1);

        store.create(create_test_workflow("wf_002")).unwrap();
        assert_eq!(store.count().unwrap(), 2);

        store.delete("wf_001").unwrap();
        assert_eq!(store.count().unwrap(), 1);
    }

    #[test]
    fn test_exists() {
        let store = WorkflowStore::new();

        assert!(!store.exists("wf_001").unwrap());

        store.create(create_test_workflow("wf_001")).unwrap();
        assert!(store.exists("wf_001").unwrap());

        store.delete("wf_001").unwrap();
        assert!(!store.exists("wf_001").unwrap());
    }

    #[test]
    fn test_concurrent_access() {
        use std::thread;

        let store = WorkflowStore::new();

        // Create workflows concurrently
        let handles: Vec<_> = (0..10)
            .map(|i| {
                let store_clone = store.clone();
                thread::spawn(move || {
                    let workflow = create_test_workflow(&format!("wf_{:03}", i));
                    store_clone.create(workflow)
                })
            })
            .collect();

        // Wait for all threads
        for handle in handles {
            handle.join().unwrap().unwrap();
        }

        // Verify all workflows created
        assert_eq!(store.count().unwrap(), 10);
    }
}
