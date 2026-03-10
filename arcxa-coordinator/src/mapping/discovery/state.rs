//! # Discovery State Manager
//!
//! Global state management for asynchronous schema discovery operations.
//! Provides thread-safe progress tracking and result caching.
//!
//! ## Architecture
//!
//! ```text
//! ┌──────────────────────────────────┐
//! │     DiscoveryStateManager        │
//! │                                  │
//! │  ┌────────────────────────────┐  │
//! │  │  Active Discoveries        │  │
//! │  │  HashMap<String, Progress> │  │
//! │  └────────────────────────────┘  │
//! │                                  │
//! │  ┌────────────────────────────┐  │
//! │  │  Results Cache (24h TTL)   │  │
//! │  │  HashMap<String, Result>   │  │
//! │  └────────────────────────────┘  │
//! └──────────────────────────────────┘
//! ```
//!
//! ## Usage
//!
//! ```rust,ignore
//! let state = DiscoveryStateManager::new();
//!
//! // Start discovery
//! let discovery_id = state.create_discovery("ds-123".to_string());
//!
//! // Update progress
//! state.update_progress(&discovery_id, |progress| {
//!     progress.percent_complete = 50.0;
//!     progress.current_step = "Introspecting tables".to_string();
//!     progress.tables_discovered = 25;
//! });
//!
//! // Complete discovery
//! state.complete_discovery(&discovery_id, discovered_schema);
//!
//! // Retrieve result
//! if let Some(result) = state.get_result(&discovery_id) {
//!     println!("Found {} tables", result.tables.len());
//! }
//! ```

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, warn};
use uuid::Uuid;

use super::types::DiscoveredSchema;

/// Discovery status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveryStatus {
    /// Discovery is queued but not started
    Queued,
    /// Discovery is currently running
    Running,
    /// Discovery completed successfully
    Completed,
    /// Discovery failed with error
    Failed,
    /// Discovery was cancelled by user
    Cancelled,
}

/// Discovery progress information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryProgress {
    /// Unique discovery ID
    pub discovery_id: String,

    /// Associated data source ID
    pub datasource_id: String,

    /// Current status
    pub status: DiscoveryStatus,

    /// Current step description
    pub current_step: String,

    /// Number of tables discovered so far
    pub tables_discovered: usize,

    /// Total number of tables (if known)
    pub total_tables: Option<usize>,

    /// Completion percentage (0.0 - 100.0)
    pub percent_complete: f64,

    /// Error messages (if any)
    pub errors: Vec<String>,

    /// When discovery started
    pub started_at: DateTime<Utc>,

    /// When discovery was last updated
    pub updated_at: DateTime<Utc>,

    /// When discovery completed (if completed)
    pub completed_at: Option<DateTime<Utc>>,
}

impl DiscoveryProgress {
    /// Create new discovery progress
    pub fn new(discovery_id: String, datasource_id: String) -> Self {
        let now = Utc::now();
        Self {
            discovery_id,
            datasource_id,
            status: DiscoveryStatus::Queued,
            current_step: "Initializing discovery".to_string(),
            tables_discovered: 0,
            total_tables: None,
            percent_complete: 0.0,
            errors: Vec::new(),
            started_at: now,
            updated_at: now,
            completed_at: None,
        }
    }

    /// Update progress percentage based on tables discovered
    pub fn update_percent(&mut self) {
        if let Some(total) = self.total_tables {
            if total > 0 {
                self.percent_complete = (self.tables_discovered as f64 / total as f64) * 100.0;
            }
        }
        self.updated_at = Utc::now();
    }

    /// Mark discovery as completed
    pub fn mark_completed(&mut self) {
        self.status = DiscoveryStatus::Completed;
        self.percent_complete = 100.0;
        self.current_step = "Discovery completed".to_string();
        self.completed_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }

    /// Mark discovery as failed
    pub fn mark_failed(&mut self, error: String) {
        self.status = DiscoveryStatus::Failed;
        self.current_step = "Discovery failed".to_string();
        self.errors.push(error);
        self.completed_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }

    /// Mark discovery as cancelled
    pub fn mark_cancelled(&mut self) {
        self.status = DiscoveryStatus::Cancelled;
        self.current_step = "Discovery cancelled".to_string();
        self.completed_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }
}

/// Cached discovery result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryResult {
    /// Unique discovery ID
    pub discovery_id: String,

    /// Discovered schema
    pub schema: DiscoveredSchema,

    /// When result was cached
    pub cached_at: DateTime<Utc>,
}

impl DiscoveryResult {
    /// Check if result is expired based on TTL (in seconds)
    pub fn is_expired(&self, ttl_secs: u64) -> bool {
        let now = Utc::now();
        let age = now.signed_duration_since(self.cached_at);
        age.num_seconds() > ttl_secs as i64
    }
}

/// Global discovery state manager
///
/// Thread-safe state management for async discovery operations.
/// Uses RwLock for concurrent read access with exclusive writes.
#[derive(Clone)]
pub struct DiscoveryStateManager {
    /// Active discoveries with progress tracking
    discoveries: Arc<RwLock<HashMap<String, DiscoveryProgress>>>,

    /// Completed discovery results (with 24 hour TTL)
    results: Arc<RwLock<HashMap<String, DiscoveryResult>>>,

    /// Result cache TTL in seconds (default: 24 hours)
    cache_ttl_secs: u64,
}

impl DiscoveryStateManager {
    /// Create new discovery state manager
    pub fn new() -> Self {
        Self {
            discoveries: Arc::new(RwLock::new(HashMap::new())),
            results: Arc::new(RwLock::new(HashMap::new())),
            cache_ttl_secs: 24 * 60 * 60, // 24 hours
        }
    }

    /// Create with custom cache TTL
    pub fn with_ttl(cache_ttl_secs: u64) -> Self {
        Self {
            discoveries: Arc::new(RwLock::new(HashMap::new())),
            results: Arc::new(RwLock::new(HashMap::new())),
            cache_ttl_secs,
        }
    }

    /// Create a new discovery and return its ID
    pub fn create_discovery(&self, datasource_id: String) -> String {
        let discovery_id = Uuid::new_v4().to_string();
        let progress = DiscoveryProgress::new(discovery_id.clone(), datasource_id.clone());

        info!(
            discovery_id = %discovery_id,
            datasource_id = %datasource_id,
            "Created new discovery"
        );

        self.discoveries
            .write()
            .insert(discovery_id.clone(), progress);
        discovery_id
    }

    /// Get discovery progress
    pub fn get_progress(&self, discovery_id: &str) -> Option<DiscoveryProgress> {
        self.discoveries.read().get(discovery_id).cloned()
    }

    /// Update discovery progress using a closure
    pub fn update_progress<F>(&self, discovery_id: &str, update_fn: F) -> Result<()>
    where
        F: FnOnce(&mut DiscoveryProgress),
    {
        let mut discoveries = self.discoveries.write();
        let progress = discoveries
            .get_mut(discovery_id)
            .ok_or_else(|| anyhow!("Discovery not found: {}", discovery_id))?;

        update_fn(progress);
        progress.updated_at = Utc::now();

        debug!(
            discovery_id = %discovery_id,
            percent = %progress.percent_complete,
            step = %progress.current_step,
            "Updated discovery progress"
        );

        Ok(())
    }

    /// Mark discovery as running
    pub fn start_discovery(&self, discovery_id: &str) -> Result<()> {
        self.update_progress(discovery_id, |progress| {
            progress.status = DiscoveryStatus::Running;
            progress.current_step = "Starting discovery".to_string();
            progress.started_at = Utc::now();
        })
    }

    /// Complete discovery and cache result
    pub fn complete_discovery(&self, discovery_id: &str, schema: DiscoveredSchema) -> Result<()> {
        // Update progress to completed
        self.update_progress(discovery_id, |progress| {
            progress.mark_completed();
        })?;

        // Cache result
        let result = DiscoveryResult {
            discovery_id: discovery_id.to_string(),
            schema,
            cached_at: Utc::now(),
        };

        self.results
            .write()
            .insert(discovery_id.to_string(), result);

        info!(
            discovery_id = %discovery_id,
            "Discovery completed and result cached"
        );

        Ok(())
    }

    /// Fail discovery with error message
    pub fn fail_discovery(&self, discovery_id: &str, error: String) -> Result<()> {
        self.update_progress(discovery_id, |progress| {
            progress.mark_failed(error.clone());
        })?;

        warn!(
            discovery_id = %discovery_id,
            error = %error,
            "Discovery failed"
        );

        Ok(())
    }

    /// Cancel discovery
    pub fn cancel_discovery(&self, discovery_id: &str) -> Result<()> {
        self.update_progress(discovery_id, |progress| {
            progress.mark_cancelled();
        })?;

        info!(
            discovery_id = %discovery_id,
            "Discovery cancelled"
        );

        Ok(())
    }

    /// Get cached discovery result
    pub fn get_result(&self, discovery_id: &str) -> Option<DiscoveryResult> {
        let results = self.results.read();
        let result = results.get(discovery_id)?;

        // Check if expired
        if result.is_expired(self.cache_ttl_secs) {
            drop(results); // Release read lock
            self.results.write().remove(discovery_id);
            debug!(
                discovery_id = %discovery_id,
                "Removed expired discovery result"
            );
            return None;
        }

        Some(result.clone())
    }

    /// Clean up expired results (run periodically)
    pub fn cleanup_expired_results(&self) -> usize {
        let mut results = self.results.write();
        let initial_count = results.len();

        results.retain(|discovery_id, result| {
            if result.is_expired(self.cache_ttl_secs) {
                debug!(
                    discovery_id = %discovery_id,
                    "Removing expired result during cleanup"
                );
                false
            } else {
                true
            }
        });

        let removed_count = initial_count - results.len();
        if removed_count > 0 {
            info!(
                removed = removed_count,
                remaining = results.len(),
                "Cleaned up expired discovery results"
            );
        }

        removed_count
    }

    /// List all active discoveries for a datasource
    pub fn list_discoveries_for_datasource(&self, datasource_id: &str) -> Vec<DiscoveryProgress> {
        self.discoveries
            .read()
            .values()
            .filter(|p| p.datasource_id == datasource_id)
            .cloned()
            .collect()
    }

    /// Get statistics about discoveries
    pub fn get_stats(&self) -> DiscoveryStats {
        let discoveries = self.discoveries.read();
        let results = self.results.read();

        let mut stats = DiscoveryStats::default();

        for progress in discoveries.values() {
            match progress.status {
                DiscoveryStatus::Queued => stats.queued += 1,
                DiscoveryStatus::Running => stats.running += 1,
                DiscoveryStatus::Completed => stats.completed += 1,
                DiscoveryStatus::Failed => stats.failed += 1,
                DiscoveryStatus::Cancelled => stats.cancelled += 1,
            }
        }

        stats.cached_results = results.len();
        stats
    }
}

impl Default for DiscoveryStateManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Discovery statistics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DiscoveryStats {
    pub queued: usize,
    pub running: usize,
    pub completed: usize,
    pub failed: usize,
    pub cancelled: usize,
    pub cached_results: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_discovery() {
        let state = DiscoveryStateManager::new();
        let discovery_id = state.create_discovery("ds-123".to_string());

        let progress = state.get_progress(&discovery_id).unwrap();
        assert_eq!(progress.datasource_id, "ds-123");
        assert_eq!(progress.status, DiscoveryStatus::Queued);
        assert_eq!(progress.percent_complete, 0.0);
    }

    #[test]
    fn test_update_progress() {
        let state = DiscoveryStateManager::new();
        let discovery_id = state.create_discovery("ds-123".to_string());

        state
            .update_progress(&discovery_id, |progress| {
                progress.percent_complete = 50.0;
                progress.current_step = "Half done".to_string();
                progress.tables_discovered = 10;
            })
            .unwrap();

        let progress = state.get_progress(&discovery_id).unwrap();
        assert_eq!(progress.percent_complete, 50.0);
        assert_eq!(progress.current_step, "Half done");
        assert_eq!(progress.tables_discovered, 10);
    }

    #[test]
    fn test_complete_discovery() {
        let state = DiscoveryStateManager::new();
        let discovery_id = state.create_discovery("ds-123".to_string());

        let schema = DiscoveredSchema {
            source_id: "ds-123".to_string(),
            schema_name: "public".to_string(),
            tables: vec![],
            relationships: vec![],
            discovered_at: 0,
        };

        state
            .complete_discovery(&discovery_id, schema.clone())
            .unwrap();

        let progress = state.get_progress(&discovery_id).unwrap();
        assert_eq!(progress.status, DiscoveryStatus::Completed);
        assert_eq!(progress.percent_complete, 100.0);

        let result = state.get_result(&discovery_id).unwrap();
        assert_eq!(result.schema.source_id, "ds-123");
    }

    #[test]
    fn test_fail_discovery() {
        let state = DiscoveryStateManager::new();
        let discovery_id = state.create_discovery("ds-123".to_string());

        state
            .fail_discovery(&discovery_id, "Connection failed".to_string())
            .unwrap();

        let progress = state.get_progress(&discovery_id).unwrap();
        assert_eq!(progress.status, DiscoveryStatus::Failed);
        assert_eq!(progress.errors.len(), 1);
        assert_eq!(progress.errors[0], "Connection failed");
    }

    #[test]
    fn test_cancel_discovery() {
        let state = DiscoveryStateManager::new();
        let discovery_id = state.create_discovery("ds-123".to_string());

        state.cancel_discovery(&discovery_id).unwrap();

        let progress = state.get_progress(&discovery_id).unwrap();
        assert_eq!(progress.status, DiscoveryStatus::Cancelled);
    }

    #[test]
    fn test_expired_result() {
        let state = DiscoveryStateManager::with_ttl(1); // 1 second TTL
        let discovery_id = state.create_discovery("ds-123".to_string());

        let schema = DiscoveredSchema {
            source_id: "ds-123".to_string(),
            schema_name: "public".to_string(),
            tables: vec![],
            relationships: vec![],
            discovered_at: 0,
        };

        state.complete_discovery(&discovery_id, schema).unwrap();

        // Should be cached
        assert!(state.get_result(&discovery_id).is_some());

        // Wait for expiration
        std::thread::sleep(std::time::Duration::from_secs(2));

        // Should be expired
        assert!(state.get_result(&discovery_id).is_none());
    }

    #[test]
    fn test_get_stats() {
        let state = DiscoveryStateManager::new();

        let id1 = state.create_discovery("ds-1".to_string());
        let id2 = state.create_discovery("ds-2".to_string());
        let id3 = state.create_discovery("ds-3".to_string());

        state.start_discovery(&id1).unwrap();
        state.fail_discovery(&id2, "Error".to_string()).unwrap();

        let stats = state.get_stats();
        assert_eq!(stats.queued, 1);
        assert_eq!(stats.running, 1);
        assert_eq!(stats.failed, 1);
    }
}
