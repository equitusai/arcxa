//! Deployment state storage

use super::types::*;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// In-memory deployment store
pub struct DeploymentStore {
    /// Deployments by ID
    deployments: Arc<RwLock<HashMap<String, Deployment>>>,

    /// Active deployments by workflow ID and environment
    active_deployments: Arc<RwLock<HashMap<(String, String), String>>>,
}

impl DeploymentStore {
    /// Create a new deployment store
    pub fn new() -> Self {
        Self {
            deployments: Arc::new(RwLock::new(HashMap::new())),
            active_deployments: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a new deployment
    pub fn create(&self, deployment: Deployment) -> Result<()> {
        let mut deployments = self
            .deployments
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;

        deployments.insert(deployment.id.clone(), deployment);
        Ok(())
    }

    /// Get deployment by ID
    pub fn get(&self, deployment_id: &str) -> Result<Option<Deployment>> {
        let deployments = self
            .deployments
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;

        Ok(deployments.get(deployment_id).cloned())
    }

    /// Get deployment (required)
    pub fn get_required(&self, deployment_id: &str) -> Result<Deployment> {
        self.get(deployment_id)?
            .ok_or_else(|| anyhow::anyhow!("Deployment '{}' not found", deployment_id))
    }

    /// Update deployment
    pub fn update(&self, deployment: Deployment) -> Result<()> {
        let mut deployments = self
            .deployments
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;

        deployments.insert(deployment.id.clone(), deployment);
        Ok(())
    }

    /// Set active deployment for workflow and environment
    pub fn set_active(
        &self,
        workflow_id: &str,
        environment: &str,
        deployment_id: &str,
    ) -> Result<()> {
        let mut active = self
            .active_deployments
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;

        let key = (workflow_id.to_string(), environment.to_string());
        active.insert(key, deployment_id.to_string());
        Ok(())
    }

    /// Get active deployment for workflow and environment
    pub fn get_active(&self, workflow_id: &str, environment: &str) -> Result<Option<Deployment>> {
        let active = self
            .active_deployments
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;

        let key = (workflow_id.to_string(), environment.to_string());
        if let Some(deployment_id) = active.get(&key) {
            self.get(deployment_id)
        } else {
            Ok(None)
        }
    }

    /// Get deployment history for workflow
    pub fn get_history(&self, workflow_id: &str, limit: usize) -> Result<DeploymentHistory> {
        let deployments = self
            .deployments
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;

        let mut workflow_deployments: Vec<Deployment> = deployments
            .values()
            .filter(|d| d.workflow_id == workflow_id)
            .cloned()
            .collect();

        // Sort by started_at descending
        workflow_deployments.sort_by(|a, b| b.started_at.cmp(&a.started_at));

        let total_deployments = workflow_deployments.len();
        let rollback_count = workflow_deployments
            .iter()
            .filter(|d| d.is_rolled_back())
            .count();
        let active_deployment = workflow_deployments.iter().find(|d| d.is_active()).cloned();

        let recent_deployments = workflow_deployments.into_iter().take(limit).collect();

        Ok(DeploymentHistory {
            workflow_id: workflow_id.to_string(),
            total_deployments,
            active_deployment,
            recent_deployments,
            rollback_count,
        })
    }

    /// List all deployments
    pub fn list_all(&self, limit: usize) -> Result<Vec<Deployment>> {
        let deployments = self
            .deployments
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;

        let mut all: Vec<Deployment> = deployments.values().cloned().collect();
        all.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        Ok(all.into_iter().take(limit).collect())
    }

    /// List deployments by environment
    pub fn list_by_environment(&self, environment: &str, limit: usize) -> Result<Vec<Deployment>> {
        let deployments = self
            .deployments
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;

        let mut filtered: Vec<Deployment> = deployments
            .values()
            .filter(|d| d.environment == environment)
            .cloned()
            .collect();

        filtered.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        Ok(filtered.into_iter().take(limit).collect())
    }

    /// Get previous deployment
    pub fn get_previous(
        &self,
        workflow_id: &str,
        environment: &str,
        current_deployment_id: &str,
    ) -> Result<Option<Deployment>> {
        let deployments = self
            .deployments
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;

        let mut workflow_deployments: Vec<Deployment> = deployments
            .values()
            .filter(|d| {
                d.workflow_id == workflow_id
                    && d.environment == environment
                    && d.id != current_deployment_id
                    && d.is_active()
            })
            .cloned()
            .collect();

        workflow_deployments.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        Ok(workflow_deployments.first().cloned())
    }

    /// Delete deployment
    pub fn delete(&self, deployment_id: &str) -> Result<()> {
        let mut deployments = self
            .deployments
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;

        deployments.remove(deployment_id);
        Ok(())
    }

    /// Get deployment count
    pub fn count(&self) -> Result<usize> {
        let deployments = self
            .deployments
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;

        Ok(deployments.len())
    }
}

impl Default for DeploymentStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_and_get_deployment() {
        let store = DeploymentStore::new();
        let deployment = Deployment::new(
            "dep_001".to_string(),
            "wf_001".to_string(),
            "1.0.0".to_string(),
            DeploymentStrategy::Direct,
            "production".to_string(),
            "user@example.com".to_string(),
        );

        store.create(deployment.clone()).unwrap();

        let retrieved = store.get("dep_001").unwrap().unwrap();
        assert_eq!(retrieved.id, "dep_001");
        assert_eq!(retrieved.workflow_id, "wf_001");
    }

    #[test]
    fn test_active_deployment() {
        let store = DeploymentStore::new();
        let deployment = Deployment::new(
            "dep_001".to_string(),
            "wf_001".to_string(),
            "1.0.0".to_string(),
            DeploymentStrategy::Direct,
            "production".to_string(),
            "user@example.com".to_string(),
        );

        store.create(deployment).unwrap();
        store.set_active("wf_001", "production", "dep_001").unwrap();

        let active = store.get_active("wf_001", "production").unwrap().unwrap();
        assert_eq!(active.id, "dep_001");
    }

    #[test]
    fn test_deployment_history() {
        let store = DeploymentStore::new();

        // Create multiple deployments
        for i in 1..=5 {
            let deployment = Deployment::new(
                format!("dep_{:03}", i),
                "wf_001".to_string(),
                format!("1.0.{}", i),
                DeploymentStrategy::Direct,
                "production".to_string(),
                "user@example.com".to_string(),
            );
            store.create(deployment).unwrap();
        }

        let history = store.get_history("wf_001", 10).unwrap();
        assert_eq!(history.total_deployments, 5);
        assert_eq!(history.recent_deployments.len(), 5);
    }

    #[test]
    fn test_list_by_environment() {
        let store = DeploymentStore::new();

        // Production deployments
        for i in 1..=3 {
            let deployment = Deployment::new(
                format!("dep_prod_{}", i),
                "wf_001".to_string(),
                format!("1.0.{}", i),
                DeploymentStrategy::Direct,
                "production".to_string(),
                "user@example.com".to_string(),
            );
            store.create(deployment).unwrap();
        }

        // Staging deployments
        for i in 1..=2 {
            let deployment = Deployment::new(
                format!("dep_staging_{}", i),
                "wf_001".to_string(),
                format!("1.0.{}", i),
                DeploymentStrategy::Direct,
                "staging".to_string(),
                "user@example.com".to_string(),
            );
            store.create(deployment).unwrap();
        }

        let prod_deployments = store.list_by_environment("production", 10).unwrap();
        let staging_deployments = store.list_by_environment("staging", 10).unwrap();

        assert_eq!(prod_deployments.len(), 3);
        assert_eq!(staging_deployments.len(), 2);
    }

    #[test]
    fn test_update_deployment() {
        let store = DeploymentStore::new();
        let mut deployment = Deployment::new(
            "dep_001".to_string(),
            "wf_001".to_string(),
            "1.0.0".to_string(),
            DeploymentStrategy::Direct,
            "production".to_string(),
            "user@example.com".to_string(),
        );

        store.create(deployment.clone()).unwrap();

        deployment.mark_complete();
        store.update(deployment).unwrap();

        let updated = store.get("dep_001").unwrap().unwrap();
        assert!(updated.is_active());
    }

    #[test]
    fn test_delete_deployment() {
        let store = DeploymentStore::new();
        let deployment = Deployment::new(
            "dep_001".to_string(),
            "wf_001".to_string(),
            "1.0.0".to_string(),
            DeploymentStrategy::Direct,
            "production".to_string(),
            "user@example.com".to_string(),
        );

        store.create(deployment).unwrap();
        assert_eq!(store.count().unwrap(), 1);

        store.delete("dep_001").unwrap();
        assert_eq!(store.count().unwrap(), 0);
    }
}
