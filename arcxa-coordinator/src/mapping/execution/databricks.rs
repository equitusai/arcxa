//! Databricks execution port scaffold.
//!
//! This is an infrastructure adapter skeleton. It validates input and returns
//! a submitted execution receipt, giving orchestration and lineage workflows
//! a stable integration point before SQL Statement API wiring is added.

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use std::collections::HashMap;
use uuid::Uuid;

use super::{
    ExecutionBackend, ExecutionOutcome, ExecutionRequest, ExecutionStatus, ExternalExecutionPort,
};

pub struct DatabricksExecutor;

impl DatabricksExecutor {
    pub fn new() -> Self {
        Self
    }

    fn validate_request(request: &ExecutionRequest) -> Result<()> {
        if request.target_host.trim().is_empty() {
            return Err(anyhow!("Databricks executor requires target_host"));
        }
        if request.target_database.trim().is_empty() {
            return Err(anyhow!("Databricks executor requires target_database"));
        }
        Ok(())
    }
}

impl Default for DatabricksExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ExternalExecutionPort for DatabricksExecutor {
    fn backend(&self) -> ExecutionBackend {
        ExecutionBackend::Databricks
    }

    async fn execute(&self, request: ExecutionRequest) -> Result<ExecutionOutcome> {
        Self::validate_request(&request)?;

        // Phase 1 scaffold:
        // - run contract validation
        // - return an external_run_id suitable for lineage/audit references
        // - avoid network I/O until Databricks Statement API client is wired
        let external_run_id = format!("dbx_stmt_{}", Uuid::new_v4().simple());
        let mut metadata = HashMap::new();
        metadata.insert("workspace_host".to_string(), request.target_host.clone());
        metadata.insert(
            "target_database".to_string(),
            request.target_database.clone(),
        );
        metadata.insert("target_port".to_string(), request.target_port.to_string());
        metadata.insert("target_user".to_string(), request.target_username.clone());
        metadata.insert(
            "integration_state".to_string(),
            "scaffold_submitted_no_network_call".to_string(),
        );

        Ok(ExecutionOutcome {
            backend: ExecutionBackend::Databricks,
            run_id: request.run_id,
            external_run_id: Some(external_run_id),
            status: ExecutionStatus::Submitted,
            message: "Databricks execution submitted (scaffold mode)".to_string(),
            metadata,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn submits_scaffold_execution() {
        let executor = DatabricksExecutor::new();
        let outcome = executor
            .execute(ExecutionRequest {
                run_id: "run-1".to_string(),
                session_id: "session-1".to_string(),
                target_host: "https://adb-1.azuredatabricks.net".to_string(),
                target_port: 443,
                target_database: "lakehouse".to_string(),
                target_username: "svc_graphica".to_string(),
                options: HashMap::new(),
            })
            .await
            .expect("outcome");

        assert_eq!(outcome.backend, ExecutionBackend::Databricks);
        assert_eq!(outcome.status, ExecutionStatus::Submitted);
        assert!(outcome.external_run_id.is_some());
    }

    #[tokio::test]
    async fn rejects_missing_host() {
        let executor = DatabricksExecutor::new();
        let result = executor
            .execute(ExecutionRequest {
                run_id: "run-2".to_string(),
                session_id: "session-2".to_string(),
                target_host: String::new(),
                target_port: 443,
                target_database: "lakehouse".to_string(),
                target_username: "svc_graphica".to_string(),
                options: HashMap::new(),
            })
            .await;

        assert!(result.is_err());
    }
}
