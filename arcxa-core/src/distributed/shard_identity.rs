//! Shard Identity Management
//!
//! This module handles persistent shard identity that survives restarts,
//! enabling auto-discovery and eliminating manual shard ID configuration.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::{error, info, warn};
use uuid::Uuid;

/// Hash range for shard responsibility
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HashRange {
    pub start: u64,
    pub end: u64,
}

impl HashRange {
    pub fn new(start: u64, end: u64) -> Self {
        Self { start, end }
    }
}

/// Persistent shard identity stored on disk
#[derive(Debug, Serialize, Deserialize)]
pub struct ShardIdentity {
    /// Assigned shard ID (None if not yet registered)
    pub shard_id: Option<u32>,

    /// Unique machine identifier (UUID)
    pub machine_id: String,

    /// First registration timestamp
    pub first_registered: chrono::DateTime<chrono::Utc>,

    /// Last startup timestamp
    pub last_started: chrono::DateTime<chrono::Utc>,

    /// Coordinator URL for registration
    pub coordinator_url: String,

    /// Assigned hash range (None if not yet registered)
    pub hash_range: Option<HashRange>,

    /// Identity file format version
    pub version: u32,

    /// Optional metadata
    pub metadata: ShardMetadata,
}

/// Additional shard metadata
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ShardMetadata {
    /// Hostname of the machine
    pub hostname: Option<String>,

    /// IP address (for debugging)
    pub ip_address: Option<String>,

    /// Data center or availability zone
    pub datacenter: Option<String>,

    /// Rack identifier (for topology-aware placement)
    pub rack: Option<String>,

    /// Custom labels
    pub labels: std::collections::HashMap<String, String>,
}

impl ShardIdentity {
    /// Identity file name within data directory
    const IDENTITY_FILE: &'static str = ".graphica/shard_identity.json";

    /// Current identity file format version
    const CURRENT_VERSION: u32 = 1;

    /// Load identity from disk or create new one
    pub fn load_or_create(data_path: &Path, coordinator_url: &str) -> Result<Self> {
        let identity_path = Self::identity_file_path(data_path);

        if identity_path.exists() {
            Self::load_existing(data_path, &identity_path)
        } else {
            Self::create_new(data_path, coordinator_url)
        }
    }

    /// Load existing identity from disk
    fn load_existing(data_path: &Path, identity_path: &Path) -> Result<Self> {
        let contents = std::fs::read_to_string(identity_path)
            .with_context(|| format!("Failed to read identity file: {:?}", identity_path))?;

        let mut identity: Self = serde_json::from_str(&contents)
            .with_context(|| format!("Failed to parse identity file: {:?}", identity_path))?;

        // Update last started timestamp
        identity.last_started = chrono::Utc::now();
        identity.save(data_path)?;

        info!(
            "Loaded existing shard identity: shard_id={:?}, machine_id={}",
            identity.shard_id, identity.machine_id
        );

        // Validate version
        if identity.version != Self::CURRENT_VERSION {
            warn!(
                "Identity file version mismatch: file={}, current={}",
                identity.version,
                Self::CURRENT_VERSION
            );
            // In production, might trigger migration logic here
        }

        Ok(identity)
    }

    /// Create new identity
    fn create_new(data_path: &Path, coordinator_url: &str) -> Result<Self> {
        let machine_id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now();

        // Gather system metadata
        let metadata = ShardMetadata {
            hostname: hostname::get().ok().and_then(|h| h.into_string().ok()),
            ip_address: Self::get_local_ip(),
            datacenter: std::env::var("DATACENTER").ok(),
            rack: std::env::var("RACK").ok(),
            labels: Self::parse_labels_from_env(),
        };

        let identity = Self {
            shard_id: None,
            machine_id: machine_id.clone(),
            first_registered: now,
            last_started: now,
            coordinator_url: coordinator_url.to_string(),
            hash_range: None,
            version: Self::CURRENT_VERSION,
            metadata,
        };

        // Save to disk
        identity.save(data_path)?;

        info!(
            "Created new shard identity: machine_id={}, coordinator_url={}",
            machine_id, coordinator_url
        );

        Ok(identity)
    }

    /// Save identity to disk
    pub fn save(&self, data_path: &Path) -> Result<()> {
        let identity_path = Self::identity_file_path(data_path);

        // Ensure directory exists
        if let Some(parent) = identity_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory: {:?}", parent))?;
        }

        // Write to temporary file first (atomic write)
        let temp_path = identity_path.with_extension("tmp");
        let contents =
            serde_json::to_string_pretty(self).context("Failed to serialize identity")?;

        std::fs::write(&temp_path, contents)
            .with_context(|| format!("Failed to write temporary file: {:?}", temp_path))?;

        // Atomic rename
        std::fs::rename(&temp_path, &identity_path)
            .with_context(|| format!("Failed to rename identity file: {:?}", identity_path))?;

        Ok(())
    }

    /// Update identity after successful registration
    pub fn update_registration(
        &mut self,
        shard_id: u32,
        hash_range: HashRange,
        data_path: &Path,
    ) -> Result<()> {
        self.shard_id = Some(shard_id);
        self.hash_range = Some(hash_range.clone());
        self.save(data_path)?;

        info!(
            "Updated shard identity with registration: shard_id={}, range={:?}-{:?}",
            shard_id, hash_range.start, hash_range.end
        );

        Ok(())
    }

    /// Clear identity file (for forced re-registration)
    pub fn clear(data_path: &Path) -> Result<()> {
        let identity_path = Self::identity_file_path(data_path);

        if identity_path.exists() {
            // Backup old identity before deletion
            let backup_path = identity_path.with_extension("backup");
            std::fs::copy(&identity_path, &backup_path)
                .with_context(|| format!("Failed to backup identity file: {:?}", identity_path))?;

            std::fs::remove_file(&identity_path)
                .with_context(|| format!("Failed to remove identity file: {:?}", identity_path))?;

            info!(
                "Cleared shard identity file (backup saved as {:?})",
                backup_path
            );
        }

        Ok(())
    }

    /// Check if identity needs registration
    pub fn needs_registration(&self) -> bool {
        self.shard_id.is_none() || self.hash_range.is_none()
    }

    /// Validate identity integrity
    pub fn validate(&self) -> Result<()> {
        // Check machine ID format
        if self.machine_id.is_empty() {
            return Err(anyhow::anyhow!("Machine ID is empty"));
        }

        // Validate UUID format
        Uuid::parse_str(&self.machine_id).context("Invalid machine ID format (expected UUID)")?;

        // Check coordinator URL
        if self.coordinator_url.is_empty() {
            return Err(anyhow::anyhow!("Coordinator URL is empty"));
        }

        // If registered, validate shard_id and hash_range
        if let Some(shard_id) = self.shard_id {
            if self.hash_range.is_none() {
                return Err(anyhow::anyhow!(
                    "Shard {} has no hash range assigned",
                    shard_id
                ));
            }
        }

        Ok(())
    }

    /// Get identity file path
    fn identity_file_path(data_path: &Path) -> PathBuf {
        data_path.join(Self::IDENTITY_FILE)
    }

    /// Get local IP address (best effort)
    fn get_local_ip() -> Option<String> {
        // Try to get non-loopback IP
        if let Ok(socket) = std::net::UdpSocket::bind("0.0.0.0:0") {
            // Connect to a public DNS server (doesn't actually send data)
            if socket.connect("8.8.8.8:80").is_ok() {
                if let Ok(addr) = socket.local_addr() {
                    return Some(addr.ip().to_string());
                }
            }
        }
        None
    }

    /// Parse labels from environment variables
    fn parse_labels_from_env() -> std::collections::HashMap<String, String> {
        let mut labels = std::collections::HashMap::new();

        // Look for SHARD_LABEL_* environment variables
        for (key, value) in std::env::vars() {
            if key.starts_with("SHARD_LABEL_") {
                let label_name = key.strip_prefix("SHARD_LABEL_").unwrap().to_lowercase();
                labels.insert(label_name, value);
            }
        }

        labels
    }
}

/// Registration state for tracking registration attempts
#[derive(Debug, Clone)]
pub struct RegistrationState {
    /// Number of attempts
    pub attempts: u32,

    /// Last attempt timestamp
    pub last_attempt: Option<chrono::DateTime<chrono::Utc>>,

    /// Last error message
    pub last_error: Option<String>,

    /// Backoff delay for next attempt
    pub backoff_secs: u64,
}

impl Default for RegistrationState {
    fn default() -> Self {
        Self {
            attempts: 0,
            last_attempt: None,
            last_error: None,
            backoff_secs: 1,
        }
    }
}

impl RegistrationState {
    /// Record successful registration
    pub fn success(&mut self) {
        self.attempts = 0;
        self.last_attempt = Some(chrono::Utc::now());
        self.last_error = None;
        self.backoff_secs = 1;
    }

    /// Record failed registration attempt
    pub fn failure(&mut self, error: String) {
        self.attempts += 1;
        self.last_attempt = Some(chrono::Utc::now());
        self.last_error = Some(error);

        // Exponential backoff with jitter
        self.backoff_secs = std::cmp::min(
            300, // Max 5 minutes
            self.backoff_secs * 2 + rand::random::<u64>() % 3,
        );
    }

    /// Check if we should retry now
    pub fn should_retry(&self) -> bool {
        match self.last_attempt {
            None => true,
            Some(last) => {
                let elapsed = chrono::Utc::now().signed_duration_since(last).num_seconds() as u64;
                elapsed >= self.backoff_secs
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_create_new_identity() {
        let temp_dir = TempDir::new().unwrap();
        let identity = ShardIdentity::load_or_create(temp_dir.path(), "coordinator:9090").unwrap();

        assert!(identity.shard_id.is_none());
        assert!(!identity.machine_id.is_empty());
        assert_eq!(identity.coordinator_url, "coordinator:9090");
        assert!(identity.hash_range.is_none());
        assert_eq!(identity.version, ShardIdentity::CURRENT_VERSION);
    }

    #[test]
    fn test_persist_and_load_identity() {
        let temp_dir = TempDir::new().unwrap();

        // Create identity
        let identity1 = ShardIdentity::load_or_create(temp_dir.path(), "coordinator:9090").unwrap();

        let machine_id = identity1.machine_id.clone();

        // Load same identity
        let identity2 = ShardIdentity::load_or_create(temp_dir.path(), "coordinator:9090").unwrap();

        assert_eq!(identity2.machine_id, machine_id);
        assert!(identity2.last_started > identity1.last_started);
    }

    #[test]
    fn test_update_registration() {
        let temp_dir = TempDir::new().unwrap();

        let mut identity =
            ShardIdentity::load_or_create(temp_dir.path(), "coordinator:9090").unwrap();

        assert!(identity.needs_registration());

        // Update with registration info
        identity
            .update_registration(5, HashRange::new(0, 1000), temp_dir.path())
            .unwrap();

        assert_eq!(identity.shard_id, Some(5));
        assert_eq!(identity.hash_range, Some(HashRange::new(0, 1000)));
        assert!(!identity.needs_registration());

        // Verify persistence
        let loaded = ShardIdentity::load_or_create(temp_dir.path(), "coordinator:9090").unwrap();

        assert_eq!(loaded.shard_id, Some(5));
    }

    #[test]
    fn test_clear_identity() {
        let temp_dir = TempDir::new().unwrap();

        // Create identity
        ShardIdentity::load_or_create(temp_dir.path(), "coordinator:9090").unwrap();

        // Clear it
        ShardIdentity::clear(temp_dir.path()).unwrap();

        // New identity should be created
        let new_identity =
            ShardIdentity::load_or_create(temp_dir.path(), "coordinator:9090").unwrap();

        assert!(new_identity.shard_id.is_none());
    }

    #[test]
    fn test_registration_state() {
        let mut state = RegistrationState::default();

        assert!(state.should_retry());

        // Record failure
        state.failure("Connection refused".to_string());
        assert_eq!(state.attempts, 1);
        assert!(state.backoff_secs > 1);

        // Record success
        state.success();
        assert_eq!(state.attempts, 0);
        assert_eq!(state.backoff_secs, 1);
    }
}
