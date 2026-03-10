//! Oracle execution port adapter (incremental implementation).
//!
//! Validates Oracle execution contract and produces a submitted execution
//! outcome that can be correlated by lineage/audit systems.

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use std::collections::HashMap;
use uuid::Uuid;

use super::{
    ExecutionBackend, ExecutionOutcome, ExecutionRequest, ExecutionStatus, ExternalExecutionPort,
};

pub struct OracleExecutor;

impl OracleExecutor {
    pub fn new() -> Self {
        Self
    }

    fn validate_request(request: &ExecutionRequest) -> Result<()> {
        if request.target_host.trim().is_empty() {
            return Err(anyhow!("Oracle executor requires target_host"));
        }
        if request.target_database.trim().is_empty() {
            return Err(anyhow!(
                "Oracle executor requires target_database (service/SID)"
            ));
        }
        if request.target_port == 0 {
            return Err(anyhow!("Oracle executor requires a valid target_port"));
        }
        Ok(())
    }
}

impl Default for OracleExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ExternalExecutionPort for OracleExecutor {
    fn backend(&self) -> ExecutionBackend {
        ExecutionBackend::Oracle
    }

    async fn execute(&self, request: ExecutionRequest) -> Result<ExecutionOutcome> {
        Self::validate_request(&request)?;

        let mut metadata = HashMap::new();
        metadata.insert(
            "connect_descriptor".to_string(),
            format!(
                "(DESCRIPTION=(ADDRESS=(PROTOCOL=TCP)(HOST={})(PORT={}))(CONNECT_DATA=(SERVICE_NAME={})))",
                request.target_host, request.target_port, request.target_database
            ),
        );
        metadata.insert("target_user".to_string(), request.target_username.clone());
        metadata.insert(
            "integration_state".to_string(),
            "adapter_submitted_no_network_call".to_string(),
        );

        Ok(ExecutionOutcome {
            backend: ExecutionBackend::Oracle,
            run_id: request.run_id,
            external_run_id: Some(format!("ora_job_{}", Uuid::new_v4().simple())),
            status: ExecutionStatus::Submitted,
            message: "Oracle execution submitted (adapter scaffold mode)".to_string(),
            metadata,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn submits_oracle_execution() {
        let executor = OracleExecutor::new();
        let outcome = executor
            .execute(ExecutionRequest {
                run_id: "run-1".to_string(),
                session_id: "session-1".to_string(),
                target_host: "oracle.internal".to_string(),
                target_port: 1521,
                target_database: "ORCLPDB1".to_string(),
                target_username: "system".to_string(),
                options: HashMap::new(),
            })
            .await
            .expect("outcome");

        assert_eq!(outcome.backend, ExecutionBackend::Oracle);
        assert_eq!(outcome.status, ExecutionStatus::Submitted);
        assert!(outcome.external_run_id.is_some());
    }
}
