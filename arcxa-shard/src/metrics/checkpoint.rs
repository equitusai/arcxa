//! Checkpoint Tracking
//!
//! Tracks checkpoint positions for data ingestion and replication,
//! allowing shards to resume from the last known good position after restart.

use anyhow::{Context, Result};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::info;

/// Checkpoint representing a position in the data stream
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Checkpoint {
    /// Source identifier (e.g., Kafka topic, table name)
    pub source: String,

    /// Partition or shard identifier
    pub partition: u32,

    /// Offset/position in the stream
    pub offset: u64,

    /// Timestamp when checkpoint was recorded
    pub timestamp: u64,
}

/// Checkpoint tracker managing multiple checkpoint positions
pub struct CheckpointTracker {
    /// Current checkpoints by source+partition
    checkpoints: Arc<RwLock<Vec<Checkpoint>>>,

    /// Path to persist checkpoints
    checkpoint_file: PathBuf,
}

impl CheckpointTracker {
    /// Create a new checkpoint tracker
    pub fn new(data_path: impl AsRef<Path>) -> Result<Self> {
        let checkpoint_file = data_path.as_ref().join(".graphica").join("checkpoints.json");

        // Load existing checkpoints if available
        let checkpoints = if checkpoint_file.exists() {
            Self::load_from_file(&checkpoint_file)?
        } else {
            Vec::new()
        };

        Ok(Self {
            checkpoints: Arc::new(RwLock::new(checkpoints)),
            checkpoint_file,
        })
    }

    /// Update checkpoint for a specific source and partition
    pub fn update(&self, source: String, partition: u32, offset: u64) -> Result<()> {
        let timestamp = chrono::Utc::now().timestamp() as u64;

        let checkpoint = Checkpoint {
            source: source.clone(),
            partition,
            offset,
            timestamp,
        };

        let mut checkpoints = self.checkpoints.write();

        // Find and update existing checkpoint, or add new one
        if let Some(existing) = checkpoints
            .iter_mut()
            .find(|cp| cp.source == source && cp.partition == partition)
        {
            *existing = checkpoint;
        } else {
            checkpoints.push(checkpoint);
        }

        // Persist to disk
        self.save_to_file(&checkpoints)
    }

    /// Get checkpoint for a specific source and partition
    pub fn get(&self, source: &str, partition: u32) -> Option<Checkpoint> {
        let checkpoints = self.checkpoints.read();
        checkpoints
            .iter()
            .find(|cp| cp.source == source && cp.partition == partition)
            .cloned()
    }

    /// Get all checkpoints
    pub fn get_all(&self) -> Vec<Checkpoint> {
        self.checkpoints.read().clone()
    }

    /// Get the latest checkpoint position (max offset across all sources)
    pub fn get_latest_position(&self) -> u64 {
        let checkpoints = self.checkpoints.read();
        checkpoints.iter().map(|cp| cp.offset).max().unwrap_or(0)
    }

    /// Clear all checkpoints
    pub fn clear(&self) -> Result<()> {
        let mut checkpoints = self.checkpoints.write();
        checkpoints.clear();
        self.save_to_file(&checkpoints)
    }

    /// Load checkpoints from file
    fn load_from_file(path: &Path) -> Result<Vec<Checkpoint>> {
        let contents = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read checkpoint file: {:?}", path))?;

        let checkpoints: Vec<Checkpoint> = serde_json::from_str(&contents)
            .with_context(|| format!("Failed to parse checkpoint file: {:?}", path))?;

        info!(
            "Loaded {} checkpoints from {:?}",
            checkpoints.len(),
            path
        );

        Ok(checkpoints)
    }

    /// Save checkpoints to file
    fn save_to_file(&self, checkpoints: &[Checkpoint]) -> Result<()> {
        // Ensure directory exists
        if let Some(parent) = self.checkpoint_file.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create checkpoint directory: {:?}", parent))?;
        }

        let contents = serde_json::to_string_pretty(checkpoints)
            .context("Failed to serialize checkpoints")?;

        std::fs::write(&self.checkpoint_file, contents)
            .with_context(|| format!("Failed to write checkpoint file: {:?}", self.checkpoint_file))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_new_tracker() {
        let temp_dir = TempDir::new().unwrap();
        let tracker = CheckpointTracker::new(temp_dir.path()).unwrap();

        assert_eq!(tracker.get_all().len(), 0);
        assert_eq!(tracker.get_latest_position(), 0);
    }

    #[test]
    fn test_update_checkpoint() {
        let temp_dir = TempDir::new().unwrap();
        let tracker = CheckpointTracker::new(temp_dir.path()).unwrap();

        tracker
            .update("kafka.topic1".to_string(), 0, 100)
            .unwrap();

        let checkpoint = tracker.get("kafka.topic1", 0).unwrap();
        assert_eq!(checkpoint.source, "kafka.topic1");
        assert_eq!(checkpoint.partition, 0);
        assert_eq!(checkpoint.offset, 100);
    }

    #[test]
    fn test_update_existing_checkpoint() {
        let temp_dir = TempDir::new().unwrap();
        let tracker = CheckpointTracker::new(temp_dir.path()).unwrap();

        // Initial update
        tracker
            .update("kafka.topic1".to_string(), 0, 100)
            .unwrap();

        // Update again with new offset
        tracker
            .update("kafka.topic1".to_string(), 0, 200)
            .unwrap();

        let checkpoint = tracker.get("kafka.topic1", 0).unwrap();
        assert_eq!(checkpoint.offset, 200);

        // Should still have only one checkpoint
        assert_eq!(tracker.get_all().len(), 1);
    }

    #[test]
    fn test_multiple_partitions() {
        let temp_dir = TempDir::new().unwrap();
        let tracker = CheckpointTracker::new(temp_dir.path()).unwrap();

        tracker
            .update("kafka.topic1".to_string(), 0, 100)
            .unwrap();
        tracker
            .update("kafka.topic1".to_string(), 1, 200)
            .unwrap();
        tracker
            .update("kafka.topic2".to_string(), 0, 300)
            .unwrap();

        assert_eq!(tracker.get_all().len(), 3);

        let cp1 = tracker.get("kafka.topic1", 0).unwrap();
        assert_eq!(cp1.offset, 100);

        let cp2 = tracker.get("kafka.topic1", 1).unwrap();
        assert_eq!(cp2.offset, 200);

        let cp3 = tracker.get("kafka.topic2", 0).unwrap();
        assert_eq!(cp3.offset, 300);
    }

    #[test]
    fn test_get_latest_position() {
        let temp_dir = TempDir::new().unwrap();
        let tracker = CheckpointTracker::new(temp_dir.path()).unwrap();

        tracker
            .update("kafka.topic1".to_string(), 0, 100)
            .unwrap();
        tracker
            .update("kafka.topic1".to_string(), 1, 500)
            .unwrap();
        tracker
            .update("kafka.topic2".to_string(), 0, 300)
            .unwrap();

        // Latest should be 500
        assert_eq!(tracker.get_latest_position(), 500);
    }

    #[test]
    fn test_persistence() {
        let temp_dir = TempDir::new().unwrap();

        // Create tracker and add checkpoints
        {
            let tracker = CheckpointTracker::new(temp_dir.path()).unwrap();
            tracker
                .update("kafka.topic1".to_string(), 0, 100)
                .unwrap();
            tracker
                .update("kafka.topic1".to_string(), 1, 200)
                .unwrap();
        }

        // Create new tracker - should load persisted checkpoints
        {
            let tracker = CheckpointTracker::new(temp_dir.path()).unwrap();
            assert_eq!(tracker.get_all().len(), 2);

            let cp1 = tracker.get("kafka.topic1", 0).unwrap();
            assert_eq!(cp1.offset, 100);

            let cp2 = tracker.get("kafka.topic1", 1).unwrap();
            assert_eq!(cp2.offset, 200);
        }
    }

    #[test]
    fn test_clear() {
        let temp_dir = TempDir::new().unwrap();
        let tracker = CheckpointTracker::new(temp_dir.path()).unwrap();

        tracker
            .update("kafka.topic1".to_string(), 0, 100)
            .unwrap();
        assert_eq!(tracker.get_all().len(), 1);

        tracker.clear().unwrap();
        assert_eq!(tracker.get_all().len(), 0);

        // Should persist clear
        let tracker2 = CheckpointTracker::new(temp_dir.path()).unwrap();
        assert_eq!(tracker2.get_all().len(), 0);
    }

    #[test]
    fn test_get_nonexistent_checkpoint() {
        let temp_dir = TempDir::new().unwrap();
        let tracker = CheckpointTracker::new(temp_dir.path()).unwrap();

        let result = tracker.get("nonexistent", 0);
        assert!(result.is_none());
    }

    #[test]
    fn test_checkpoint_timestamp() {
        let temp_dir = TempDir::new().unwrap();
        let tracker = CheckpointTracker::new(temp_dir.path()).unwrap();

        let before = chrono::Utc::now().timestamp() as u64;
        tracker
            .update("kafka.topic1".to_string(), 0, 100)
            .unwrap();
        let after = chrono::Utc::now().timestamp() as u64;

        let checkpoint = tracker.get("kafka.topic1", 0).unwrap();
        assert!(checkpoint.timestamp >= before);
        assert!(checkpoint.timestamp <= after + 1); // Allow 1 second tolerance
    }

    #[test]
    fn test_checkpoint_serialization() {
        let checkpoint = Checkpoint {
            source: "test.source".to_string(),
            partition: 5,
            offset: 12345,
            timestamp: 1234567890,
        };

        let json = serde_json::to_string(&checkpoint).unwrap();
        let deserialized: Checkpoint = serde_json::from_str(&json).unwrap();

        assert_eq!(checkpoint, deserialized);
    }

    #[test]
    fn test_concurrent_updates() {
        use std::sync::Arc;
        use std::thread;

        let temp_dir = TempDir::new().unwrap();
        let tracker = Arc::new(CheckpointTracker::new(temp_dir.path()).unwrap());
        let mut handles = vec![];

        // Spawn multiple threads updating different partitions
        for partition in 0..10 {
            let tracker_clone = tracker.clone();
            let handle = thread::spawn(move || {
                for offset in 0..100 {
                    tracker_clone
                        .update("kafka.topic".to_string(), partition, offset)
                        .unwrap();
                }
            });
            handles.push(handle);
        }

        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }

        // Should have 10 checkpoints (one per partition)
        assert_eq!(tracker.get_all().len(), 10);

        // Each partition should be at offset 99
        for partition in 0..10 {
            let cp = tracker.get("kafka.topic", partition).unwrap();
            assert_eq!(cp.offset, 99);
        }
    }
}
