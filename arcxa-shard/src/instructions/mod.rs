//! Coordinator Instruction Handlers
//!
//! This module handles instructions received from the coordinator via heartbeat
//! responses, including rebalancing, maintenance operations, and configuration
//! updates.

pub mod config;
pub mod maintenance;
pub mod rebalance;

pub use config::{ConfigUpdate, ConfigUpdateHandler};
pub use maintenance::{MaintenanceInstruction, MaintenanceHandler};
pub use rebalance::{RebalanceInstruction, RebalanceHandler};

use anyhow::Result;
use graphica_core::distributed::proto::coordinator_service::*;
use oxigraph::store::Store;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use tracing::{info, warn};

/// Handler for all coordinator instructions
pub struct InstructionHandler {
    rebalance_handler: RebalanceHandler,
    maintenance_handler: MaintenanceHandler,
    config_handler: ConfigUpdateHandler,
}

impl InstructionHandler {
    /// Create a new instruction handler (for testing, without store)
    pub fn new() -> Self {
        Self {
            rebalance_handler: RebalanceHandler::new(),
            maintenance_handler: MaintenanceHandler::new(),
            config_handler: ConfigUpdateHandler::new(),
        }
    }

    /// Create a new instruction handler with store access for maintenance operations
    pub fn with_store(store: Arc<Store>) -> Self {
        Self {
            rebalance_handler: RebalanceHandler::new(),
            maintenance_handler: MaintenanceHandler::with_store(store),
            config_handler: ConfigUpdateHandler::new(),
        }
    }

    /// Create a new instruction handler with full shard state access
    pub fn with_shard_state(
        store: Arc<Store>,
        is_shutting_down: Arc<AtomicBool>,
        is_readonly: Arc<AtomicBool>,
    ) -> Self {
        Self {
            rebalance_handler: RebalanceHandler::new(),
            maintenance_handler: MaintenanceHandler::with_shard_state(store, is_shutting_down, is_readonly),
            config_handler: ConfigUpdateHandler::new(),
        }
    }

    /// Process a coordinator instruction
    pub async fn handle_instruction(&self, instruction: CoordinatorInstruction) -> Result<()> {
        if let Some(inst) = instruction.instruction {
            use coordinator_instruction::Instruction;

            match inst {
                Instruction::Rebalance(rebalance) => {
                    self.rebalance_handler.handle(rebalance).await
                }
                Instruction::Maintenance(maintenance) => {
                    self.maintenance_handler.handle(maintenance).await
                }
                Instruction::ConfigUpdate(config_update) => {
                    self.config_handler.handle(config_update).await
                }
            }
        } else {
            warn!("Received empty coordinator instruction");
            Ok(())
        }
    }

    /// Process multiple instructions
    pub async fn handle_instructions(&self, instructions: Vec<CoordinatorInstruction>) -> Result<()> {
        info!("Processing {} coordinator instructions", instructions.len());

        for instruction in instructions {
            if let Err(e) = self.handle_instruction(instruction).await {
                warn!("Failed to handle coordinator instruction: {}", e);
                // Continue processing other instructions
            }
        }

        Ok(())
    }
}

impl Default for InstructionHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_new_handler() {
        let handler = InstructionHandler::new();
        // Should not panic
        assert!(true);
    }

    #[tokio::test]
    async fn test_empty_instruction() {
        let handler = InstructionHandler::new();

        let instruction = CoordinatorInstruction { instruction: None };

        // Should handle gracefully
        let result = handler.handle_instruction(instruction).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_multiple_instructions() {
        let handler = InstructionHandler::new();

        let instructions = vec![
            CoordinatorInstruction { instruction: None },
            CoordinatorInstruction { instruction: None },
        ];

        let result = handler.handle_instructions(instructions).await;
        assert!(result.is_ok());
    }
}
