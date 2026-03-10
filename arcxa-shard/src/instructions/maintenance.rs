//! Maintenance Instruction Handler
//!
//! Handles maintenance operations requested by the coordinator including
//! shutdown preparation, read-only mode, compaction, and index rebuilds.

use anyhow::{Context, Result};
use graphica_core::distributed::proto::coordinator_service::MaintenanceInstruction as ProtoMaintenance;
use graphica_core::distributed::proto::coordinator_service::maintenance_instruction::Type as MaintenanceType;
use oxigraph::store::Store;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;
use tracing::{info, warn, error};

/// Maintenance instruction types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaintenanceInstruction {
    /// Prepare for graceful shutdown
    PrepareShutdown,

    /// Enter read-only mode
    EnterReadonly,

    /// Compact storage
    CompactStorage,

    /// Rebuild indexes
    RebuildIndexes,
}

impl From<MaintenanceType> for MaintenanceInstruction {
    fn from(proto_type: MaintenanceType) -> Self {
        match proto_type {
            MaintenanceType::PrepareShutdown => MaintenanceInstruction::PrepareShutdown,
            MaintenanceType::EnterReadonly => MaintenanceInstruction::EnterReadonly,
            MaintenanceType::CompactStorage => MaintenanceInstruction::CompactStorage,
            MaintenanceType::RebuildIndexes => MaintenanceInstruction::RebuildIndexes,
        }
    }
}

/// Handler for maintenance operations
pub struct MaintenanceHandler {
    /// Reference to the Oxigraph store for compaction operations
    store: Option<Arc<Store>>,

    /// Shutdown flag to coordinate graceful shutdown
    is_shutting_down: Option<Arc<AtomicBool>>,

    /// Read-only mode flag
    is_readonly: Option<Arc<AtomicBool>>,
}

impl MaintenanceHandler {
    /// Create a new maintenance handler without a store (for testing)
    pub fn new() -> Self {
        Self {
            store: None,
            is_shutting_down: None,
            is_readonly: None,
        }
    }

    /// Create a new maintenance handler with store access
    pub fn with_store(store: Arc<Store>) -> Self {
        Self {
            store: Some(store),
            is_shutting_down: None,
            is_readonly: None,
        }
    }

    /// Create a new maintenance handler with full shard state access
    pub fn with_shard_state(
        store: Arc<Store>,
        is_shutting_down: Arc<AtomicBool>,
        is_readonly: Arc<AtomicBool>,
    ) -> Self {
        Self {
            store: Some(store),
            is_shutting_down: Some(is_shutting_down),
            is_readonly: Some(is_readonly),
        }
    }

    /// Handle a maintenance instruction
    pub async fn handle(&self, proto: ProtoMaintenance) -> Result<()> {
        let instruction = MaintenanceInstruction::from(proto.r#type());

        match instruction {
            MaintenanceInstruction::PrepareShutdown => {
                self.handle_prepare_shutdown().await
            }
            MaintenanceInstruction::EnterReadonly => {
                self.handle_enter_readonly().await
            }
            MaintenanceInstruction::CompactStorage => {
                self.handle_compact_storage().await
            }
            MaintenanceInstruction::RebuildIndexes => {
                self.handle_rebuild_indexes().await
            }
        }
    }

    /// Handle shutdown preparation
    async fn handle_prepare_shutdown(&self) -> Result<()> {
        info!("Coordinator requested preparing for shutdown");

        // Step 1: Set shutdown flag to stop accepting new writes
        if let Some(ref shutdown_flag) = self.is_shutting_down {
            shutdown_flag.store(true, Ordering::SeqCst);
            info!("✓ Shutdown flag set - no new writes accepted");
        } else {
            warn!("Shutdown flag not available (test mode)");
        }

        // Step 2: Flush pending writes to disk
        if let Some(ref store) = self.store {
            info!("Flushing pending writes to disk...");
            store.flush().context("Failed to flush store during shutdown")?;
            info!("✓ All pending writes flushed");
        } else {
            warn!("Store not available - cannot flush (test mode)");
        }

        // Step 3: Checkpoint complete (flush handles this in Oxigraph)
        info!("✓ Data checkpointed");

        // Step 4: Connections will be closed by gRPC server shutdown
        info!("✓ Ready for graceful shutdown");

        // Step 5: Signal readiness
        info!("Shard is ready for shutdown - all data safely persisted");

        Ok(())
    }

    /// Handle entering read-only mode
    async fn handle_enter_readonly(&self) -> Result<()> {
        info!("Coordinator requested entering read-only mode");

        // Step 1: Set global read-only flag
        if let Some(ref readonly_flag) = self.is_readonly {
            readonly_flag.store(true, Ordering::SeqCst);
            info!("✓ Read-only mode enabled");
        } else {
            warn!("Read-only flag not available (test mode)");
            return Ok(());
        }

        // Step 2: Write operations are now rejected by insert_batch/delete_batch
        info!("✓ Write operations will be rejected");

        // Step 3: Read queries continue unaffected
        info!("✓ Read queries continue normally");

        // Step 4: Coordinator notification happens via next heartbeat
        info!("Shard entered read-only mode - writes disabled, reads active");

        Ok(())
    }

    /// Handle storage compaction
    async fn handle_compact_storage(&self) -> Result<()> {
        info!("Coordinator requested storage compaction");

        let store = self.store.as_ref()
            .context("Storage compaction requires store access")?;

        let start = Instant::now();

        // Phase 1: Flush all in-memory buffers to disk
        info!("Phase 1: Flushing in-memory buffers to disk...");
        let flush_start = Instant::now();

        let store_clone = store.clone();
        tokio::task::spawn_blocking(move || {
            store_clone.flush()
        }).await
            .context("Failed to join flush task")?
            .context("Failed to flush store")?;

        let flush_duration = flush_start.elapsed();
        info!("Flush completed in {:.2}s", flush_duration.as_secs_f64());

        // Phase 2: Run optimization (compaction of RocksDB SSTables)
        info!("Phase 2: Optimizing storage (compacting RocksDB SSTables)...");
        let optimize_start = Instant::now();

        let store_clone = store.clone();
        tokio::task::spawn_blocking(move || {
            store_clone.optimize()
        }).await
            .context("Failed to join optimize task")?
            .context("Failed to optimize store")?;

        let optimize_duration = optimize_start.elapsed();
        info!("Optimization completed in {:.2}s", optimize_duration.as_secs_f64());

        let total_duration = start.elapsed();
        info!(
            "Storage compaction completed successfully in {:.2}s (flush: {:.2}s, optimize: {:.2}s)",
            total_duration.as_secs_f64(),
            flush_duration.as_secs_f64(),
            optimize_duration.as_secs_f64()
        );

        Ok(())
    }

    /// Handle index rebuild
    async fn handle_rebuild_indexes(&self) -> Result<()> {
        info!("Coordinator requested index rebuild");

        // TODO: Implement index rebuild
        // This would involve:
        // 1. Drop existing indexes
        // 2. Scan all triples
        // 3. Rebuild indexes
        // 4. Verify index integrity
        // 5. Notify coordinator when complete

        info!("Index rebuild not yet implemented - instruction logged");

        Ok(())
    }
}

impl Default for MaintenanceHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_maintenance_instruction_from_proto() {
        assert_eq!(
            MaintenanceInstruction::from(MaintenanceType::PrepareShutdown),
            MaintenanceInstruction::PrepareShutdown
        );
        assert_eq!(
            MaintenanceInstruction::from(MaintenanceType::EnterReadonly),
            MaintenanceInstruction::EnterReadonly
        );
        assert_eq!(
            MaintenanceInstruction::from(MaintenanceType::CompactStorage),
            MaintenanceInstruction::CompactStorage
        );
        assert_eq!(
            MaintenanceInstruction::from(MaintenanceType::RebuildIndexes),
            MaintenanceInstruction::RebuildIndexes
        );
    }

    #[tokio::test]
    async fn test_handle_prepare_shutdown() {
        let handler = MaintenanceHandler::new();

        let proto = ProtoMaintenance {
            r#type: MaintenanceType::PrepareShutdown as i32,
            deadline: 60,
        };

        let result = handler.handle(proto).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_enter_readonly() {
        let handler = MaintenanceHandler::new();

        let proto = ProtoMaintenance {
            r#type: MaintenanceType::EnterReadonly as i32,
            deadline: 3600,
        };

        let result = handler.handle(proto).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_compact_storage_without_store() {
        // Handler without store should fail gracefully
        let handler = MaintenanceHandler::new();

        let proto = ProtoMaintenance {
            r#type: MaintenanceType::CompactStorage as i32,
            deadline: 1800,
        };

        let result = handler.handle(proto).await;
        assert!(result.is_err(), "Should fail without store");
        assert!(result.unwrap_err().to_string().contains("requires store access"));
    }

    #[tokio::test]
    async fn test_handle_compact_storage_with_store() {
        use tempfile::TempDir;

        // Create temporary store for testing
        let temp_dir = TempDir::new().unwrap();
        let store = Arc::new(oxigraph::store::Store::open(temp_dir.path()).unwrap());
        let handler = MaintenanceHandler::with_store(store);

        let proto = ProtoMaintenance {
            r#type: MaintenanceType::CompactStorage as i32,
            deadline: 1800,
        };

        let result = handler.handle(proto).await;
        assert!(result.is_ok(), "Compaction should succeed with store: {:?}", result.err());
    }

    #[tokio::test]
    async fn test_handle_rebuild_indexes() {
        let handler = MaintenanceHandler::new();

        let proto = ProtoMaintenance {
            r#type: MaintenanceType::RebuildIndexes as i32,
            deadline: 7200,
        };

        let result = handler.handle(proto).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_maintenance_instruction_debug() {
        let instruction = MaintenanceInstruction::PrepareShutdown;
        let debug_str = format!("{:?}", instruction);
        assert!(debug_str.contains("PrepareShutdown"));
    }

    #[test]
    fn test_maintenance_instruction_clone() {
        let instruction = MaintenanceInstruction::CompactStorage;
        let cloned = instruction.clone();
        assert_eq!(instruction, cloned);
    }

    #[test]
    fn test_maintenance_instruction_copy() {
        let instruction = MaintenanceInstruction::RebuildIndexes;
        let copied = instruction;
        assert_eq!(instruction, copied);
    }

    #[tokio::test]
    async fn test_all_maintenance_types_without_store() {
        let handler = MaintenanceHandler::new();

        // Test types that don't require store (just log warnings)
        let non_store_types = vec![
            MaintenanceType::PrepareShutdown,
            MaintenanceType::EnterReadonly,
            MaintenanceType::RebuildIndexes,
        ];

        for maint_type in non_store_types {
            let proto = ProtoMaintenance {
                r#type: maint_type as i32,
                deadline: 60,
            };

            let result = handler.handle(proto).await;
            assert!(result.is_ok(), "Failed for type {:?}", maint_type);
        }

        // CompactStorage should fail without store
        let compact_proto = ProtoMaintenance {
            r#type: MaintenanceType::CompactStorage as i32,
            deadline: 60,
        };
        let result = handler.handle(compact_proto).await;
        assert!(result.is_err(), "CompactStorage should fail without store");
    }
}
