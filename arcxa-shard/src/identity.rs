//! Shard Identity Management
//!
//! This module handles machine ID generation and persistence for shard auto-registration.
//!
//! Each shard generates a unique machine ID (UUID v4) on first startup and persists it to:
//! `{data-path}/.graphica/shard_identity.json`
//!
//! On subsequent startups, the same machine ID is loaded, ensuring stable identity
//! across restarts.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::{info, warn};
use uuid::Uuid;

/// Shard identity stored on disk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardIdentity {
    /// Unique machine ID (UUID v4)
    pub machine_id: String,

    /// Timestamp when identity was created
    pub created_at: String,

    /// Data path for this shard
    pub data_path: PathBuf,

    /// Assigned shard ID (from coordinator)
    /// None on first startup, populated after registration
    pub shard_id: Option<u32>,
}

impl ShardIdentity {
    /// Get the path to the identity file
    fn identity_file_path(data_path: &Path) -> PathBuf {
        data_path.join(".graphica").join("shard_identity.json")
    }

    /// Load existing identity or create new one
    pub fn load_or_create(data_path: &Path) -> Result<Self> {
        let identity_path = Self::identity_file_path(data_path);

        if identity_path.exists() {
            // Load existing identity
            info!("Loading existing shard identity from {:?}", identity_path);

            let identity_json = std::fs::read_to_string(&identity_path)
                .with_context(|| format!("Failed to read identity file: {:?}", identity_path))?;

            let identity: ShardIdentity = serde_json::from_str(&identity_json)
                .with_context(|| format!("Failed to parse identity file: {:?}", identity_path))?;

            info!("Loaded identity: machine_id={}, created_at={}",
                identity.machine_id, identity.created_at);

            Ok(identity)
        } else {
            // Create new identity
            info!("No existing identity found, generating new machine ID");

            let machine_id = Uuid::new_v4().to_string();
            let created_at = chrono::Utc::now().to_rfc3339();

            let identity = ShardIdentity {
                machine_id: machine_id.clone(),
                created_at: created_at.clone(),
                data_path: data_path.to_path_buf(),
                shard_id: None, // Will be set after registration
            };

            // Ensure .graphica directory exists
            let graphica_dir = data_path.join(".graphica");
            std::fs::create_dir_all(&graphica_dir)
                .with_context(|| format!("Failed to create .graphica directory: {:?}", graphica_dir))?;

            // Write identity to file
            let identity_json = serde_json::to_string_pretty(&identity)
                .context("Failed to serialize identity")?;

            std::fs::write(&identity_path, identity_json)
                .with_context(|| format!("Failed to write identity file: {:?}", identity_path))?;

            info!("Created new identity: machine_id={}, created_at={}", machine_id, created_at);
            info!("Identity saved to {:?}", identity_path);

            Ok(identity)
        }
    }

    /// Get the machine ID
    pub fn machine_id(&self) -> &str {
        &self.machine_id
    }

    /// Save the assigned shard ID to the identity file
    pub fn save_shard_id(&mut self, shard_id: u32) -> Result<()> {
        self.shard_id = Some(shard_id);

        let identity_path = Self::identity_file_path(&self.data_path);
        let identity_json = serde_json::to_string_pretty(self)
            .context("Failed to serialize identity")?;

        std::fs::write(&identity_path, identity_json)
            .with_context(|| format!("Failed to update identity file: {:?}", identity_path))?;

        info!("Updated identity with shard_id={}", shard_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_create_new_identity() {
        let temp_dir = TempDir::new().unwrap();
        let data_path = temp_dir.path();

        let identity = ShardIdentity::load_or_create(data_path).unwrap();

        assert!(!identity.machine_id.is_empty());
        assert!(!identity.created_at.is_empty());

        // Verify file was created
        let identity_path = data_path.join(".graphica").join("shard_identity.json");
        assert!(identity_path.exists());
    }

    #[test]
    fn test_load_existing_identity() {
        let temp_dir = TempDir::new().unwrap();
        let data_path = temp_dir.path();

        // Create first identity
        let identity1 = ShardIdentity::load_or_create(data_path).unwrap();
        let machine_id1 = identity1.machine_id.clone();

        // Load again - should return same identity
        let identity2 = ShardIdentity::load_or_create(data_path).unwrap();
        let machine_id2 = identity2.machine_id.clone();

        assert_eq!(machine_id1, machine_id2);
    }

    #[test]
    fn test_identity_persistence() {
        let temp_dir = TempDir::new().unwrap();
        let data_path = temp_dir.path();

        let identity = ShardIdentity::load_or_create(data_path).unwrap();

        // Verify UUID format
        assert!(Uuid::parse_str(&identity.machine_id).is_ok());

        // Verify RFC3339 timestamp format
        assert!(chrono::DateTime::parse_from_rfc3339(&identity.created_at).is_ok());
    }

    #[test]
    fn test_save_shard_id() {
        let temp_dir = TempDir::new().unwrap();
        let data_path = temp_dir.path();

        // Create initial identity (no shard_id)
        let mut identity = ShardIdentity::load_or_create(data_path).unwrap();
        assert_eq!(identity.shard_id, None);

        // Save shard_id
        identity.save_shard_id(42).unwrap();
        assert_eq!(identity.shard_id, Some(42));

        // Load identity again - shard_id should be persisted
        let loaded_identity = ShardIdentity::load_or_create(data_path).unwrap();
        assert_eq!(loaded_identity.shard_id, Some(42));
        assert_eq!(loaded_identity.machine_id, identity.machine_id);
    }
}
