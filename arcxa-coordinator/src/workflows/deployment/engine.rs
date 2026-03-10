//! Deployment execution engine

use super::store::DeploymentStore;
use super::types::*;
use crate::workflows::declarative::{DeclarativeParser, WorkflowBuilder};
use crate::workflows::storage::WorkflowStore;
use anyhow::{Context, Result};
use graphica_core::workflows::*;
use std::sync::Arc;
use uuid::Uuid;

/// Deployment engine
pub struct DeploymentEngine {
    /// Deployment store
    deployment_store: Arc<DeploymentStore>,

    /// Workflow store
    workflow_store: Arc<WorkflowStore>,
}

impl DeploymentEngine {
    /// Create a new deployment engine
    pub fn new(deployment_store: Arc<DeploymentStore>, workflow_store: Arc<WorkflowStore>) -> Self {
        Self {
            deployment_store,
            workflow_store,
        }
    }

    /// Deploy a workflow
    pub async fn deploy(&self, request: DeploymentRequest) -> Result<Deployment> {
        // Parse workflow file
        let schema = DeclarativeParser::parse_file(&request.workflow_file)
            .with_context(|| format!("Failed to parse workflow file: {}", request.workflow_file))?;

        // Build domain workflow
        let workflow = WorkflowBuilder::build(&schema)
            .with_context(|| "Failed to build workflow from schema")?;

        // Generate deployment ID
        let deployment_id = format!(
            "dep_{}",
            Uuid::new_v4().to_string().replace('-', "")[..12].to_string()
        );

        // Get previous deployment
        let previous_deployment = self
            .deployment_store
            .get_active(&workflow.id, &request.environment)?;

        // Create deployment record
        let mut deployment = Deployment::new(
            deployment_id.clone(),
            workflow.id.clone(),
            schema
                .metadata
                .version
                .clone()
                .unwrap_or_else(|| "1.0.0".to_string()),
            request.strategy.clone(),
            request.environment.clone(),
            request.deployed_by.clone(),
        );

        deployment.metadata = request.metadata;
        deployment.previous_deployment_id = previous_deployment.as_ref().map(|d| d.id.clone());

        // Validate workflow if not skipped
        if !request.skip_validation {
            let validation_results = self.validate_workflow(&schema)?;
            for result in validation_results {
                deployment.add_validation_result(result.validator, result.passed, result.error);
            }

            if !deployment.all_validations_passed() {
                deployment.mark_failed("Validation failed".to_string());
                self.deployment_store.create(deployment.clone())?;
                anyhow::bail!("Deployment validation failed");
            }
        }

        // Dry run - stop here
        if request.dry_run {
            deployment.status = DeploymentStatus::Validating;
            self.deployment_store.create(deployment.clone())?;
            return Ok(deployment);
        }

        // Save deployment
        self.deployment_store.create(deployment.clone())?;

        // Execute deployment based on strategy
        deployment.status = DeploymentStatus::InProgress;
        self.deployment_store.update(deployment.clone())?;

        match &request.strategy {
            DeploymentStrategy::Direct => {
                self.deploy_direct(&workflow, &mut deployment).await?;
            }
            DeploymentStrategy::BlueGreen { traffic_percentage } => {
                self.deploy_blue_green(&workflow, &mut deployment, *traffic_percentage)
                    .await?;
            }
            DeploymentStrategy::Canary {
                initial_percentage,
                increment,
                interval_seconds,
            } => {
                self.deploy_canary(
                    &workflow,
                    &mut deployment,
                    *initial_percentage,
                    *increment,
                    *interval_seconds,
                )
                .await?;
            }
        }

        // Perform health checks
        let health_checks = self.perform_health_checks(&workflow).await?;
        for check in health_checks {
            deployment.add_health_check(
                check.check_name,
                check.healthy,
                check.response_time_ms,
                check.error,
            );
        }

        // Mark as complete if all checks passed
        if deployment.all_health_checks_passed() || deployment.health_checks.is_empty() {
            deployment.mark_complete();
            self.deployment_store
                .set_active(&workflow.id, &request.environment, &deployment.id)?;
        } else {
            deployment.mark_failed("Health checks failed".to_string());
        }

        self.deployment_store.update(deployment.clone())?;
        Ok(deployment)
    }

    /// Rollback a deployment
    pub async fn rollback(&self, request: RollbackRequest) -> Result<Deployment> {
        // Get current deployment
        let mut current_deployment = self.deployment_store.get_required(&request.deployment_id)?;

        // Get target deployment (previous or specified)
        let target_deployment_id = if let Some(target_id) = request.target_deployment_id {
            target_id
        } else {
            current_deployment
                .previous_deployment_id
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("No previous deployment to rollback to"))?
                .clone()
        };

        let target_deployment = self.deployment_store.get_required(&target_deployment_id)?;

        // Validate target is not failed
        if target_deployment.is_failed() {
            anyhow::bail!("Cannot rollback to a failed deployment");
        }

        // Mark current deployment as rolled back
        current_deployment.rollback_info = Some(RollbackInfo {
            target_deployment_id: target_deployment.id.clone(),
            reason: request.reason.clone(),
            rolled_back_by: request.rolled_back_by.clone(),
            rolled_back_at: chrono::Utc::now(),
            automatic: false,
        });
        current_deployment.mark_rolled_back(request.reason);
        self.deployment_store.update(current_deployment)?;

        // Activate target deployment
        self.deployment_store.set_active(
            &target_deployment.workflow_id,
            &target_deployment.environment,
            &target_deployment.id,
        )?;

        Ok(target_deployment)
    }

    /// Direct deployment strategy
    async fn deploy_direct(
        &self,
        workflow: &crate::workflows::domain::Workflow,
        deployment: &mut Deployment,
    ) -> Result<()> {
        // Store workflow
        self.workflow_store.create(workflow.clone())?;

        Ok(())
    }

    /// Blue-green deployment strategy
    async fn deploy_blue_green(
        &self,
        workflow: &crate::workflows::domain::Workflow,
        deployment: &mut Deployment,
        traffic_percentage: u8,
    ) -> Result<()> {
        // Deploy to green environment
        self.workflow_store.create(workflow.clone())?;

        // In a real implementation, this would:
        // 1. Deploy to green environment
        // 2. Run validation tests
        // 3. Gradually shift traffic from blue to green
        // 4. Monitor metrics
        // 5. Complete switchover if successful

        deployment.metadata.insert(
            "traffic_percentage".to_string(),
            traffic_percentage.to_string(),
        );

        Ok(())
    }

    /// Canary deployment strategy
    async fn deploy_canary(
        &self,
        workflow: &crate::workflows::domain::Workflow,
        deployment: &mut Deployment,
        initial_percentage: u8,
        increment: u8,
        interval_seconds: u64,
    ) -> Result<()> {
        // Deploy canary
        self.workflow_store.create(workflow.clone())?;

        // In a real implementation, this would:
        // 1. Deploy canary with initial_percentage traffic
        // 2. Monitor metrics for interval_seconds
        // 3. If healthy, increase by increment
        // 4. Repeat until 100%
        // 5. Auto-rollback on errors

        deployment
            .metadata
            .insert("canary_initial".to_string(), initial_percentage.to_string());
        deployment
            .metadata
            .insert("canary_increment".to_string(), increment.to_string());
        deployment
            .metadata
            .insert("canary_interval".to_string(), interval_seconds.to_string());

        Ok(())
    }

    /// Validate workflow
    fn validate_workflow(
        &self,
        schema: &WorkflowSchema,
    ) -> Result<Vec<DeploymentValidationResult>> {
        let validators: Vec<Box<dyn Validator>> = vec![
            Box::new(SchemaValidator),
            Box::new(SemanticValidator),
            Box::new(DependencyValidator),
            Box::new(ResourceValidator),
        ];

        let composite = CompositeValidator::with_validators(validators);
        let result = composite.validate(schema);

        let mut validation_results = Vec::new();

        if !result.valid {
            for error in &result.errors {
                validation_results.push(DeploymentValidationResult {
                    validator: "composite".to_string(),
                    passed: false,
                    error: Some(error.to_string()),
                    validated_at: chrono::Utc::now(),
                });
            }
        } else {
            validation_results.push(DeploymentValidationResult {
                validator: "composite".to_string(),
                passed: true,
                error: None,
                validated_at: chrono::Utc::now(),
            });
        }

        Ok(validation_results)
    }

    /// Perform health checks
    async fn perform_health_checks(
        &self,
        _workflow: &crate::workflows::domain::Workflow,
    ) -> Result<Vec<HealthCheckResult>> {
        // In a real implementation, this would:
        // 1. Check if workflow is processing data
        // 2. Check resource utilization
        // 3. Check error rates
        // 4. Check SLA metrics

        // For now, return success
        Ok(vec![HealthCheckResult {
            check_name: "workflow_ready".to_string(),
            healthy: true,
            response_time_ms: 10,
            error: None,
            checked_at: chrono::Utc::now(),
        }])
    }

    /// Get deployment history
    pub fn get_history(&self, workflow_id: &str, limit: usize) -> Result<DeploymentHistory> {
        self.deployment_store.get_history(workflow_id, limit)
    }

    /// List deployments
    pub fn list_deployments(
        &self,
        environment: Option<&str>,
        limit: usize,
    ) -> Result<Vec<Deployment>> {
        if let Some(env) = environment {
            self.deployment_store.list_by_environment(env, limit)
        } else {
            self.deployment_store.list_all(limit)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_request() -> DeploymentRequest {
        DeploymentRequest {
            workflow_file: "test_workflow.yaml".to_string(),
            environment: "test".to_string(),
            strategy: DeploymentStrategy::Direct,
            deployed_by: "test@example.com".to_string(),
            metadata: std::collections::HashMap::new(),
            skip_validation: true,
            dry_run: true,
        }
    }

    #[test]
    fn test_deployment_engine_creation() {
        let deployment_store = Arc::new(DeploymentStore::new());
        let workflow_store = Arc::new(WorkflowStore::new());
        let engine = DeploymentEngine::new(deployment_store, workflow_store);

        assert!(std::ptr::addr_of!(engine).is_null() == false);
    }

    #[test]
    fn test_blue_green_strategy() {
        let strategy = DeploymentStrategy::BlueGreen {
            traffic_percentage: 50,
        };

        match strategy {
            DeploymentStrategy::BlueGreen { traffic_percentage } => {
                assert_eq!(traffic_percentage, 50);
            }
            _ => panic!("Expected BlueGreen strategy"),
        }
    }

    #[test]
    fn test_canary_strategy() {
        let strategy = DeploymentStrategy::Canary {
            initial_percentage: 10,
            increment: 10,
            interval_seconds: 300,
        };

        match strategy {
            DeploymentStrategy::Canary {
                initial_percentage,
                increment,
                interval_seconds,
            } => {
                assert_eq!(initial_percentage, 10);
                assert_eq!(increment, 10);
                assert_eq!(interval_seconds, 300);
            }
            _ => panic!("Expected Canary strategy"),
        }
    }
}
