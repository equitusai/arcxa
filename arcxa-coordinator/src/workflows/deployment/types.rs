//! Deployment types and state management

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Deployment record
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Deployment {
    /// Unique deployment ID
    pub id: String,

    /// Workflow ID being deployed
    pub workflow_id: String,

    /// Workflow version
    pub version: String,

    /// Deployment strategy used
    pub strategy: DeploymentStrategy,

    /// Current status
    pub status: DeploymentStatus,

    /// Environment (dev, staging, prod)
    pub environment: String,

    /// Deployed by
    pub deployed_by: String,

    /// Deployment started at
    pub started_at: DateTime<Utc>,

    /// Deployment completed at
    pub completed_at: Option<DateTime<Utc>>,

    /// Previous deployment ID (for rollback)
    pub previous_deployment_id: Option<String>,

    /// Deployment metadata
    #[serde(default)]
    pub metadata: HashMap<String, String>,

    /// Validation results
    #[serde(default)]
    pub validation_results: Vec<DeploymentValidationResult>,

    /// Health check results
    #[serde(default)]
    pub health_checks: Vec<HealthCheckResult>,

    /// Rollback information (if applicable)
    pub rollback_info: Option<RollbackInfo>,
}

/// Deployment strategy
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DeploymentStrategy {
    /// Direct deployment - immediate replacement
    Direct,

    /// Blue-green deployment - switch traffic after validation
    BlueGreen {
        /// Traffic percentage to new version
        traffic_percentage: u8,
    },

    /// Canary deployment - gradual rollout
    Canary {
        /// Initial percentage
        initial_percentage: u8,
        /// Increment step
        increment: u8,
        /// Duration between increments (seconds)
        interval_seconds: u64,
    },
}

/// Deployment status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DeploymentStatus {
    /// Deployment is pending
    Pending,

    /// Deployment is in progress
    InProgress,

    /// Deployment is validating
    Validating,

    /// Deployment is complete and active
    Active,

    /// Deployment failed
    Failed { reason: String },

    /// Deployment was rolled back
    RolledBack {
        reason: String,
        rolled_back_at: DateTime<Utc>,
    },

    /// Deployment is paused (canary)
    Paused,
}

/// Deployment validation result
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeploymentValidationResult {
    /// Validator name
    pub validator: String,

    /// Validation passed
    pub passed: bool,

    /// Error message if failed
    pub error: Option<String>,

    /// Validation time
    pub validated_at: DateTime<Utc>,
}

/// Health check result
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HealthCheckResult {
    /// Check name
    pub check_name: String,

    /// Check passed
    pub healthy: bool,

    /// Response time (ms)
    pub response_time_ms: u64,

    /// Error message if unhealthy
    pub error: Option<String>,

    /// Check time
    pub checked_at: DateTime<Utc>,
}

/// Rollback information
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RollbackInfo {
    /// Rolled back to deployment ID
    pub target_deployment_id: String,

    /// Rollback reason
    pub reason: String,

    /// Rolled back by
    pub rolled_back_by: String,

    /// Rollback time
    pub rolled_back_at: DateTime<Utc>,

    /// Automatic rollback (vs manual)
    pub automatic: bool,
}

/// Deployment request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentRequest {
    /// Workflow file path
    pub workflow_file: String,

    /// Environment
    pub environment: String,

    /// Strategy
    pub strategy: DeploymentStrategy,

    /// Deployed by
    pub deployed_by: String,

    /// Additional metadata
    #[serde(default)]
    pub metadata: HashMap<String, String>,

    /// Skip validation
    #[serde(default)]
    pub skip_validation: bool,

    /// Dry run only
    #[serde(default)]
    pub dry_run: bool,
}

/// Rollback request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackRequest {
    /// Deployment ID to rollback
    pub deployment_id: String,

    /// Target deployment ID (optional, defaults to previous)
    pub target_deployment_id: Option<String>,

    /// Rollback reason
    pub reason: String,

    /// Rolled back by
    pub rolled_back_by: String,

    /// Force rollback without confirmation
    #[serde(default)]
    pub force: bool,
}

/// Deployment history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentHistory {
    /// Workflow ID
    pub workflow_id: String,

    /// Total deployments
    pub total_deployments: usize,

    /// Active deployment
    pub active_deployment: Option<Deployment>,

    /// Recent deployments
    pub recent_deployments: Vec<Deployment>,

    /// Rollback count
    pub rollback_count: usize,
}

impl Deployment {
    /// Create a new deployment
    pub fn new(
        id: String,
        workflow_id: String,
        version: String,
        strategy: DeploymentStrategy,
        environment: String,
        deployed_by: String,
    ) -> Self {
        Self {
            id,
            workflow_id,
            version,
            strategy,
            status: DeploymentStatus::Pending,
            environment,
            deployed_by,
            started_at: Utc::now(),
            completed_at: None,
            previous_deployment_id: None,
            metadata: HashMap::new(),
            validation_results: Vec::new(),
            health_checks: Vec::new(),
            rollback_info: None,
        }
    }

    /// Check if deployment is active
    pub fn is_active(&self) -> bool {
        matches!(self.status, DeploymentStatus::Active)
    }

    /// Check if deployment failed
    pub fn is_failed(&self) -> bool {
        matches!(self.status, DeploymentStatus::Failed { .. })
    }

    /// Check if deployment was rolled back
    pub fn is_rolled_back(&self) -> bool {
        matches!(self.status, DeploymentStatus::RolledBack { .. })
    }

    /// Get duration
    pub fn duration_seconds(&self) -> Option<i64> {
        self.completed_at
            .map(|completed| (completed - self.started_at).num_seconds())
    }

    /// Mark as complete
    pub fn mark_complete(&mut self) {
        self.status = DeploymentStatus::Active;
        self.completed_at = Some(Utc::now());
    }

    /// Mark as failed
    pub fn mark_failed(&mut self, reason: String) {
        self.status = DeploymentStatus::Failed { reason };
        self.completed_at = Some(Utc::now());
    }

    /// Mark as rolled back
    pub fn mark_rolled_back(&mut self, reason: String) {
        self.status = DeploymentStatus::RolledBack {
            reason,
            rolled_back_at: Utc::now(),
        };
    }

    /// Add validation result
    pub fn add_validation_result(
        &mut self,
        validator: String,
        passed: bool,
        error: Option<String>,
    ) {
        self.validation_results.push(DeploymentValidationResult {
            validator,
            passed,
            error,
            validated_at: Utc::now(),
        });
    }

    /// Add health check result
    pub fn add_health_check(
        &mut self,
        check_name: String,
        healthy: bool,
        response_time_ms: u64,
        error: Option<String>,
    ) {
        self.health_checks.push(HealthCheckResult {
            check_name,
            healthy,
            response_time_ms,
            error,
            checked_at: Utc::now(),
        });
    }

    /// All validations passed
    pub fn all_validations_passed(&self) -> bool {
        !self.validation_results.is_empty() && self.validation_results.iter().all(|v| v.passed)
    }

    /// All health checks passed
    pub fn all_health_checks_passed(&self) -> bool {
        !self.health_checks.is_empty() && self.health_checks.iter().all(|h| h.healthy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deployment_creation() {
        let deployment = Deployment::new(
            "dep_001".to_string(),
            "wf_001".to_string(),
            "1.0.0".to_string(),
            DeploymentStrategy::Direct,
            "production".to_string(),
            "user@example.com".to_string(),
        );

        assert_eq!(deployment.id, "dep_001");
        assert_eq!(deployment.workflow_id, "wf_001");
        assert!(matches!(deployment.status, DeploymentStatus::Pending));
        assert!(!deployment.is_active());
    }

    #[test]
    fn test_deployment_lifecycle() {
        let mut deployment = Deployment::new(
            "dep_001".to_string(),
            "wf_001".to_string(),
            "1.0.0".to_string(),
            DeploymentStrategy::Direct,
            "production".to_string(),
            "user@example.com".to_string(),
        );

        // Add validation
        deployment.add_validation_result("schema".to_string(), true, None);
        assert!(deployment.all_validations_passed());

        // Add health check
        deployment.add_health_check("api".to_string(), true, 100, None);
        assert!(deployment.all_health_checks_passed());

        // Mark complete
        deployment.mark_complete();
        assert!(deployment.is_active());
        assert!(deployment.completed_at.is_some());
    }

    #[test]
    fn test_deployment_strategies() {
        let direct = DeploymentStrategy::Direct;
        let blue_green = DeploymentStrategy::BlueGreen {
            traffic_percentage: 100,
        };
        let canary = DeploymentStrategy::Canary {
            initial_percentage: 10,
            increment: 10,
            interval_seconds: 300,
        };

        assert!(matches!(direct, DeploymentStrategy::Direct));
        assert!(matches!(blue_green, DeploymentStrategy::BlueGreen { .. }));
        assert!(matches!(canary, DeploymentStrategy::Canary { .. }));
    }

    #[test]
    fn test_deployment_failure() {
        let mut deployment = Deployment::new(
            "dep_001".to_string(),
            "wf_001".to_string(),
            "1.0.0".to_string(),
            DeploymentStrategy::Direct,
            "production".to_string(),
            "user@example.com".to_string(),
        );

        deployment.mark_failed("Health check failed".to_string());
        assert!(deployment.is_failed());
        assert!(!deployment.is_active());
    }

    #[test]
    fn test_rollback_info() {
        let rollback = RollbackInfo {
            target_deployment_id: "dep_000".to_string(),
            reason: "Critical bug".to_string(),
            rolled_back_by: "admin@example.com".to_string(),
            rolled_back_at: Utc::now(),
            automatic: false,
        };

        assert_eq!(rollback.target_deployment_id, "dep_000");
        assert!(!rollback.automatic);
    }

    #[test]
    fn test_deployment_serialization() {
        let deployment = Deployment::new(
            "dep_001".to_string(),
            "wf_001".to_string(),
            "1.0.0".to_string(),
            DeploymentStrategy::BlueGreen {
                traffic_percentage: 50,
            },
            "staging".to_string(),
            "user@example.com".to_string(),
        );

        let json = serde_json::to_string(&deployment).unwrap();
        let deserialized: Deployment = serde_json::from_str(&json).unwrap();

        assert_eq!(deployment.id, deserialized.id);
        assert_eq!(deployment.workflow_id, deserialized.workflow_id);
    }
}
