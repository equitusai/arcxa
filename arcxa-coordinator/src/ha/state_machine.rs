//! Coordinator State Machine
//!
//! This module implements the replicated state machine for the coordinator cluster.
//! All state changes are applied through this state machine to ensure consistency.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use super::consistent_hash::ConsistentHashRing;
use crate::governance::distributed::{ShardId, ShardMetadata, ShardStatus};

/// Commands that can be applied to the coordinator state machine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StateCommand {
    /// Register a new shard
    RegisterShard { shard: ShardMetadata },
    /// Unregister a shard
    UnregisterShard { shard_id: ShardId },
    /// Update shard status
    UpdateShardStatus {
        shard_id: ShardId,
        status: ShardStatus,
    },
    /// Update shard heartbeat
    UpdateHeartbeat { shard_id: ShardId, timestamp: u64 },
    /// Update consistent hash ring
    UpdateHashRing { ring: ConsistentHashRing },
    /// Start migration
    StartMigration {
        migration_id: String,
        source_shard: ShardId,
        target_shard: ShardId,
        virtual_nodes: Vec<u64>,
    },
    /// Complete migration
    CompleteMigration { migration_id: String },
    /// Abort migration
    AbortMigration { migration_id: String },
    /// Update configuration
    UpdateConfig { key: String, value: String },
}

/// Migration state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationState {
    pub id: String,
    pub source_shard: ShardId,
    pub target_shard: ShardId,
    pub virtual_nodes: Vec<u64>,
    pub started_at: u64,
    pub progress: f64,
    pub status: MigrationStatus,
}

/// Migration status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MigrationStatus {
    Planning,
    InProgress,
    Verifying,
    Switching,
    Completed,
    Aborted,
    Failed,
}

/// Coordinator state machine that maintains replicated state
pub struct CoordinatorStateMachine {
    /// Registered shards
    shards: HashMap<ShardId, ShardMetadata>,

    /// Shard heartbeats (shard_id -> last_heartbeat_timestamp)
    heartbeats: HashMap<ShardId, u64>,

    /// Consistent hash ring
    hash_ring: Option<ConsistentHashRing>,

    /// Active migrations
    migrations: HashMap<String, MigrationState>,

    /// Configuration key-value pairs
    config: HashMap<String, String>,

    /// State version (incremented on each change)
    version: u64,
}

impl CoordinatorStateMachine {
    /// Create a new state machine
    pub fn new() -> Self {
        CoordinatorStateMachine {
            shards: HashMap::new(),
            heartbeats: HashMap::new(),
            hash_ring: None,
            migrations: HashMap::new(),
            config: HashMap::new(),
            version: 0,
        }
    }

    /// Apply a command to the state machine
    pub fn apply(&mut self, command: StateCommand) -> Result<()> {
        debug!("Applying command to state machine: {:?}", command);

        match command {
            StateCommand::RegisterShard { shard } => {
                self.register_shard(shard)?;
            }
            StateCommand::UnregisterShard { shard_id } => {
                self.unregister_shard(shard_id)?;
            }
            StateCommand::UpdateShardStatus { shard_id, status } => {
                self.update_shard_status(shard_id, status)?;
            }
            StateCommand::UpdateHeartbeat {
                shard_id,
                timestamp,
            } => {
                self.update_heartbeat(shard_id, timestamp)?;
            }
            StateCommand::UpdateHashRing { ring } => {
                self.update_hash_ring(ring)?;
            }
            StateCommand::StartMigration {
                migration_id,
                source_shard,
                target_shard,
                virtual_nodes,
            } => {
                self.start_migration(migration_id, source_shard, target_shard, virtual_nodes)?;
            }
            StateCommand::CompleteMigration { migration_id } => {
                self.complete_migration(&migration_id)?;
            }
            StateCommand::AbortMigration { migration_id } => {
                self.abort_migration(&migration_id)?;
            }
            StateCommand::UpdateConfig { key, value } => {
                self.update_config(key, value)?;
            }
        }

        // Increment version
        self.version += 1;

        Ok(())
    }

    /// Register a new shard
    fn register_shard(&mut self, shard: ShardMetadata) -> Result<()> {
        let shard_id = shard.shard_id;
        info!("Registering shard {} at {}", shard_id, shard.leader_address);

        if self.shards.contains_key(&shard_id) {
            return Err(anyhow::anyhow!("Shard {} already registered", shard_id));
        }

        self.shards.insert(shard_id, shard);
        self.heartbeats.insert(shard_id, current_timestamp());

        // Rebuild hash ring if we have one
        if self.hash_ring.is_some() {
            self.rebuild_hash_ring()?;
        }

        Ok(())
    }

    /// Unregister a shard
    fn unregister_shard(&mut self, shard_id: ShardId) -> Result<()> {
        info!("Unregistering shard {}", shard_id);

        self.shards
            .remove(&shard_id)
            .ok_or_else(|| anyhow::anyhow!("Shard {} not found", shard_id))?;
        self.heartbeats.remove(&shard_id);

        // Rebuild hash ring if we have one
        if self.hash_ring.is_some() {
            self.rebuild_hash_ring()?;
        }

        Ok(())
    }

    /// Update shard status
    fn update_shard_status(&mut self, shard_id: ShardId, status: ShardStatus) -> Result<()> {
        debug!("Updating shard {} status to {:?}", shard_id, status);

        let shard = self
            .shards
            .get_mut(&shard_id)
            .ok_or_else(|| anyhow::anyhow!("Shard {} not found", shard_id))?;

        shard.status = status;
        Ok(())
    }

    /// Update shard heartbeat
    fn update_heartbeat(&mut self, shard_id: ShardId, timestamp: u64) -> Result<()> {
        if !self.shards.contains_key(&shard_id) {
            return Err(anyhow::anyhow!("Shard {} not found", shard_id));
        }

        self.heartbeats.insert(shard_id, timestamp);
        Ok(())
    }

    /// Update consistent hash ring
    fn update_hash_ring(&mut self, ring: ConsistentHashRing) -> Result<()> {
        info!("Updating consistent hash ring");
        self.hash_ring = Some(ring);
        Ok(())
    }

    /// Rebuild hash ring from current shards
    fn rebuild_hash_ring(&mut self) -> Result<()> {
        if let Some(ref mut ring) = self.hash_ring {
            ring.clear();
            for shard in self.shards.values() {
                if shard.status == ShardStatus::Active {
                    ring.add_shard(shard.shard_id, 150)?; // Default 150 virtual nodes
                }
            }
        }
        Ok(())
    }

    /// Start a migration
    fn start_migration(
        &mut self,
        migration_id: String,
        source_shard: ShardId,
        target_shard: ShardId,
        virtual_nodes: Vec<u64>,
    ) -> Result<()> {
        info!(
            "Starting migration {} from shard {} to shard {}",
            migration_id, source_shard, target_shard
        );

        if self.migrations.contains_key(&migration_id) {
            return Err(anyhow::anyhow!("Migration {} already exists", migration_id));
        }

        let migration = MigrationState {
            id: migration_id.clone(),
            source_shard,
            target_shard,
            virtual_nodes,
            started_at: current_timestamp(),
            progress: 0.0,
            status: MigrationStatus::InProgress,
        };

        self.migrations.insert(migration_id, migration);
        Ok(())
    }

    /// Complete a migration
    fn complete_migration(&mut self, migration_id: &str) -> Result<()> {
        info!("Completing migration {}", migration_id);

        let migration = self
            .migrations
            .get_mut(migration_id)
            .ok_or_else(|| anyhow::anyhow!("Migration {} not found", migration_id))?;

        migration.status = MigrationStatus::Completed;
        migration.progress = 100.0;

        // Update hash ring to reflect completed migration
        if let Some(ref mut ring) = self.hash_ring {
            for &vnode in &migration.virtual_nodes {
                ring.reassign_virtual_node(vnode, migration.target_shard)?;
            }
        }

        Ok(())
    }

    /// Abort a migration
    fn abort_migration(&mut self, migration_id: &str) -> Result<()> {
        info!("Aborting migration {}", migration_id);

        let migration = self
            .migrations
            .get_mut(migration_id)
            .ok_or_else(|| anyhow::anyhow!("Migration {} not found", migration_id))?;

        migration.status = MigrationStatus::Aborted;
        Ok(())
    }

    /// Update configuration
    fn update_config(&mut self, key: String, value: String) -> Result<()> {
        debug!("Updating config: {} = {}", key, value);
        self.config.insert(key, value);
        Ok(())
    }

    // Getters for state inspection

    /// Get all shards
    pub fn get_shards(&self) -> &HashMap<ShardId, ShardMetadata> {
        &self.shards
    }

    /// Get a specific shard
    pub fn get_shard(&self, shard_id: ShardId) -> Option<&ShardMetadata> {
        self.shards.get(&shard_id)
    }

    /// Get shard heartbeat
    pub fn get_heartbeat(&self, shard_id: ShardId) -> Option<u64> {
        self.heartbeats.get(&shard_id).copied()
    }

    /// Get the consistent hash ring
    pub fn get_hash_ring(&self) -> Option<&ConsistentHashRing> {
        self.hash_ring.as_ref()
    }

    /// Get active migrations
    pub fn get_migrations(&self) -> &HashMap<String, MigrationState> {
        &self.migrations
    }

    /// Get configuration value
    pub fn get_config(&self, key: &str) -> Option<&str> {
        self.config.get(key).map(|s| s.as_str())
    }

    /// Get state version
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Create a snapshot of the current state
    pub fn snapshot(&self) -> StateSnapshot {
        StateSnapshot {
            shards: self.shards.clone(),
            heartbeats: self.heartbeats.clone(),
            hash_ring: self.hash_ring.clone(),
            migrations: self.migrations.clone(),
            config: self.config.clone(),
            version: self.version,
        }
    }

    /// Restore from a snapshot
    pub fn restore(&mut self, snapshot: StateSnapshot) -> Result<()> {
        info!(
            "Restoring state machine from snapshot (version {})",
            snapshot.version
        );

        self.shards = snapshot.shards;
        self.heartbeats = snapshot.heartbeats;
        self.hash_ring = snapshot.hash_ring;
        self.migrations = snapshot.migrations;
        self.config = snapshot.config;
        self.version = snapshot.version;

        Ok(())
    }
}

/// Snapshot of the coordinator state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateSnapshot {
    pub shards: HashMap<ShardId, ShardMetadata>,
    pub heartbeats: HashMap<ShardId, u64>,
    pub hash_ring: Option<ConsistentHashRing>,
    pub migrations: HashMap<String, MigrationState>,
    pub config: HashMap<String, String>,
    pub version: u64,
}

/// Get current timestamp in seconds
fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::governance::distributed::HashRange;

    fn create_test_shard(id: u32) -> ShardMetadata {
        ShardMetadata::new(
            ShardId(id),
            HashRange::new(id as u64 * 1000, (id + 1) as u64 * 1000),
            format!("shard-{}:9090", id),
            vec![],
        )
    }

    #[test]
    fn test_register_shard() {
        let mut sm = CoordinatorStateMachine::new();
        let shard = create_test_shard(1);

        sm.apply(StateCommand::RegisterShard {
            shard: shard.clone(),
        })
        .unwrap();

        assert_eq!(sm.get_shards().len(), 1);
        assert_eq!(
            sm.get_shard(ShardId(1)).unwrap().leader_address,
            "shard-1:9090"
        );
    }

    #[test]
    fn test_unregister_shard() {
        let mut sm = CoordinatorStateMachine::new();
        let shard = create_test_shard(1);

        sm.apply(StateCommand::RegisterShard {
            shard: shard.clone(),
        })
        .unwrap();
        assert_eq!(sm.get_shards().len(), 1);

        sm.apply(StateCommand::UnregisterShard {
            shard_id: ShardId(1),
        })
        .unwrap();
        assert_eq!(sm.get_shards().len(), 0);
    }

    #[test]
    fn test_update_shard_status() {
        let mut sm = CoordinatorStateMachine::new();
        let shard = create_test_shard(1);

        sm.apply(StateCommand::RegisterShard {
            shard: shard.clone(),
        })
        .unwrap();

        sm.apply(StateCommand::UpdateShardStatus {
            shard_id: ShardId(1),
            status: ShardStatus::Draining,
        })
        .unwrap();

        assert_eq!(
            sm.get_shard(ShardId(1)).unwrap().status,
            ShardStatus::Draining
        );
    }

    #[test]
    fn test_migration_lifecycle() {
        let mut sm = CoordinatorStateMachine::new();

        // Register shards
        sm.apply(StateCommand::RegisterShard {
            shard: create_test_shard(1),
        })
        .unwrap();
        sm.apply(StateCommand::RegisterShard {
            shard: create_test_shard(2),
        })
        .unwrap();

        // Start migration
        sm.apply(StateCommand::StartMigration {
            migration_id: "mig-1".to_string(),
            source_shard: ShardId(1),
            target_shard: ShardId(2),
            virtual_nodes: vec![100, 200, 300],
        })
        .unwrap();

        assert_eq!(sm.get_migrations().len(), 1);
        assert_eq!(
            sm.get_migrations()["mig-1"].status,
            MigrationStatus::InProgress
        );

        // Complete migration
        sm.apply(StateCommand::CompleteMigration {
            migration_id: "mig-1".to_string(),
        })
        .unwrap();

        assert_eq!(
            sm.get_migrations()["mig-1"].status,
            MigrationStatus::Completed
        );
    }

    #[test]
    fn test_snapshot_restore() {
        let mut sm = CoordinatorStateMachine::new();

        // Add some state
        sm.apply(StateCommand::RegisterShard {
            shard: create_test_shard(1),
        })
        .unwrap();
        sm.apply(StateCommand::UpdateConfig {
            key: "test".to_string(),
            value: "value".to_string(),
        })
        .unwrap();

        // Create snapshot
        let snapshot = sm.snapshot();
        assert_eq!(snapshot.shards.len(), 1);
        assert_eq!(snapshot.config.len(), 1);

        // Create new state machine and restore
        let mut sm2 = CoordinatorStateMachine::new();
        sm2.restore(snapshot).unwrap();

        assert_eq!(sm2.get_shards().len(), 1);
        assert_eq!(sm2.get_config("test"), Some("value"));
    }
}
