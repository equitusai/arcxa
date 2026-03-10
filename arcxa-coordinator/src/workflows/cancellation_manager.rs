//! Cancellation manager for workflow executions
//!
//! Provides centralized cancellation token management for running workflows.

use anyhow::Result;
use dashmap::DashMap;
use graphica_core::orchestration::workflow::CancellationToken;
use std::sync::Arc;

/// Manager for workflow execution cancellation tokens
pub struct CancellationManager {
    /// Map of execution_id -> CancellationToken
    tokens: Arc<DashMap<String, CancellationToken>>,
}

impl CancellationManager {
    /// Create a new cancellation manager
    pub fn new() -> Self {
        Self {
            tokens: Arc::new(DashMap::new()),
        }
    }

    /// Register a cancellation token for an execution
    pub fn register(&self, execution_id: String, token: CancellationToken) {
        self.tokens.insert(execution_id, token);
    }

    /// Get a cancellation token for an execution
    pub fn get(&self, execution_id: &str) -> Option<CancellationToken> {
        self.tokens
            .get(execution_id)
            .map(|entry| entry.value().clone())
    }

    /// Cancel an execution by ID
    pub async fn cancel_execution(&self, execution_id: &str) -> Result<()> {
        let token = self.tokens.get(execution_id).ok_or_else(|| {
            anyhow::anyhow!("Execution not found or not running: {}", execution_id)
        })?;

        token.cancel();

        tracing::info!("Cancelled execution: {}", execution_id);

        Ok(())
    }

    /// Remove a token after execution completes
    pub fn unregister(&self, execution_id: &str) {
        self.tokens.remove(execution_id);
    }

    /// Get count of registered executions
    pub fn active_count(&self) -> usize {
        self.tokens.len()
    }
}

impl Default for CancellationManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_register_and_cancel() {
        let manager = CancellationManager::new();
        let token = CancellationToken::new();

        assert!(!token.is_cancelled());

        manager.register("exec-1".to_string(), token.clone());
        assert_eq!(manager.active_count(), 1);

        manager.cancel_execution("exec-1").await.unwrap();
        assert!(token.is_cancelled());

        manager.unregister("exec-1");
        assert_eq!(manager.active_count(), 0);
    }

    #[tokio::test]
    async fn test_cancel_nonexistent() {
        let manager = CancellationManager::new();

        let result = manager.cancel_execution("nonexistent").await;
        assert!(result.is_err());
    }
}
