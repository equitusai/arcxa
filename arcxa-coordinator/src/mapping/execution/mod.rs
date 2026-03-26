//! External execution ports for migration backends.
//!
//! This module introduces an application-facing execution boundary so orchestration
//! can target external backends (Oracle, DB2) without coupling handlers to
//! backend-specific SDKs/protocols.

pub mod db2;
pub mod oracle;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

pub use db2::Db2Executor;
pub use oracle::OracleExecutor;

/// Supported execution backends for external orchestration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionBackend {
    Db2,
    Oracle,
}

/// Request contract for an external backend execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionRequest {
    pub run_id: String,
    pub session_id: String,
    pub target_host: String,
    pub target_port: u16,
    pub target_database: String,
    pub target_username: String,
    #[serde(default)]
    pub options: HashMap<String, String>,
}

/// Execution state returned by a backend port.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    Submitted,
    Completed,
}

/// Standardized execution result for orchestration telemetry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionOutcome {
    pub backend: ExecutionBackend,
    pub run_id: String,
    pub external_run_id: Option<String>,
    pub status: ExecutionStatus,
    pub message: String,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

/// Structured event emitted for observability/lineage hooks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionTelemetryEvent {
    pub event_id: String,
    pub observed_at: i64,
    pub backend: ExecutionBackend,
    pub run_id: String,
    pub session_id: String,
    pub external_run_id: Option<String>,
    pub status: ExecutionStatus,
    pub message: String,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

impl ExecutionOutcome {
    pub fn to_telemetry_event(&self, request: &ExecutionRequest) -> ExecutionTelemetryEvent {
        ExecutionTelemetryEvent {
            event_id: format!("exec_evt_{}", uuid::Uuid::new_v4().simple()),
            observed_at: Utc::now().timestamp(),
            backend: self.backend,
            run_id: self.run_id.clone(),
            session_id: request.session_id.clone(),
            external_run_id: self.external_run_id.clone(),
            status: self.status,
            message: self.message.clone(),
            metadata: self.metadata.clone(),
        }
    }
}

/// Port abstraction for backend-specific execution providers.
#[async_trait]
pub trait ExternalExecutionPort: Send + Sync {
    fn backend(&self) -> ExecutionBackend;
    async fn execute(&self, request: ExecutionRequest) -> Result<ExecutionOutcome>;
}

/// Runtime registry of external executors.
pub struct ExecutorRegistry {
    executors: HashMap<ExecutionBackend, Arc<dyn ExternalExecutionPort>>,
}

impl ExecutorRegistry {
    pub fn new() -> Self {
        Self {
            executors: HashMap::new(),
        }
    }

    pub fn register<E: ExternalExecutionPort + 'static>(&mut self, executor: E) {
        self.executors.insert(
            executor.backend(),
            Arc::new(executor) as Arc<dyn ExternalExecutionPort>,
        );
    }

    pub async fn execute(
        &self,
        backend: ExecutionBackend,
        request: ExecutionRequest,
    ) -> Result<ExecutionOutcome> {
        let executor = self
            .executors
            .get(&backend)
            .ok_or_else(|| anyhow!("No executor registered for backend {:?}", backend))?;
        executor.execute(request).await
    }

    /// Default scaffold used by API for early integration and contract tests.
    pub fn default_scaffold() -> Self {
        let mut registry = Self::new();
        registry.register(Db2Executor::new());
        registry.register(OracleExecutor::new());
        registry
    }
}

impl Default for ExecutorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn default_registry_executes_db2_backend() {
        let registry = ExecutorRegistry::default_scaffold();
        let outcome = registry
            .execute(
                ExecutionBackend::Db2,
                ExecutionRequest {
                    run_id: "run-1".to_string(),
                    session_id: "session-1".to_string(),
                    target_host: "db2.internal".to_string(),
                    target_port: 50000,
                    target_database: "warehouse".to_string(),
                    target_username: "svc_graphica".to_string(),
                    options: HashMap::new(),
                },
            )
            .await
            .expect("outcome");

        assert_eq!(outcome.backend, ExecutionBackend::Db2);
        assert_eq!(outcome.status, ExecutionStatus::Submitted);
    }
}
