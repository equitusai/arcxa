//! DB2 execution port adapter (incremental implementation).
//!
//! This adapter validates request contract and emits a submitted execution
//! receipt with DB2-specific metadata. Network/CLI invocation can be added
//! without changing the orchestration port contract.

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use std::collections::HashMap;
use uuid::Uuid;

use super::{
    ExecutionBackend, ExecutionOutcome, ExecutionRequest, ExecutionStatus, ExternalExecutionPort,
};

pub struct Db2Executor;

impl Db2Executor {
    pub fn new() -> Self {
        Self
    }

    fn validate_request(request: &ExecutionRequest) -> Result<()> {
        if request.target_host.trim().is_empty() {
            return Err(anyhow!("DB2 executor requires target_host"));
        }
        if request.target_database.trim().is_empty() {
            return Err(anyhow!("DB2 executor requires target_database"));
        }
        if request.target_port == 0 {
            return Err(anyhow!("DB2 executor requires a valid target_port"));
        }
        Ok(())
    }
}

impl Default for Db2Executor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ExternalExecutionPort for Db2Executor {
    fn backend(&self) -> ExecutionBackend {
        ExecutionBackend::Db2
    }

    async fn execute(&self, request: ExecutionRequest) -> Result<ExecutionOutcome> {
        Self::validate_request(&request)?;

        let mut metadata = HashMap::new();
        metadata.insert(
            "dsn".to_string(),
            format!(
                "DATABASE={};HOSTNAME={};PORT={};PROTOCOL=TCPIP",
                request.target_database, request.target_host, request.target_port
            ),
        );
        metadata.insert("target_user".to_string(), request.target_username.clone());
        metadata.insert(
            "integration_state".to_string(),
            "adapter_submitted_no_network_call".to_string(),
        );

        Ok(ExecutionOutcome {
            backend: ExecutionBackend::Db2,
            run_id: request.run_id,
            external_run_id: Some(format!("db2_job_{}", Uuid::new_v4().simple())),
            status: ExecutionStatus::Submitted,
            message: "DB2 execution submitted (adapter scaffold mode)".to_string(),
            metadata,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn submits_db2_execution() {
        let executor = Db2Executor::new();
        let outcome = executor
            .execute(ExecutionRequest {
                run_id: "run-1".to_string(),
                session_id: "session-1".to_string(),
                target_host: "db2.internal".to_string(),
                target_port: 50000,
                target_database: "SAMPLE".to_string(),
                target_username: "db2inst1".to_string(),
                options: HashMap::new(),
            })
            .await
            .expect("outcome");

        assert_eq!(outcome.backend, ExecutionBackend::Db2);
        assert_eq!(outcome.status, ExecutionStatus::Submitted);
        assert!(outcome.external_run_id.is_some());
    }
}
