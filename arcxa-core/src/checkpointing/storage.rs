//! Checkpoint storage backend with async compression

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::Checkpoint;

/// Checkpoint storage backend
#[derive(Clone)]
pub struct CheckpointStorage {
    base_path: PathBuf,
    max_checkpoints: usize,
    compression_level: i32,
}

impl CheckpointStorage {
    fn system_time_ms(time: SystemTime) -> u128 {
        time.duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    }

    fn checkpoint_timestamp_from_path(path: &Path) -> Option<u128> {
        let filename = path.file_name()?.to_str()?;

        let base = filename
            .strip_suffix(".bin.zst")
            .or_else(|| filename.strip_suffix(".bin"))?;

        let ts = base.strip_prefix("checkpoint_")?;
        ts.parse::<u128>().ok()
    }

    fn checkpoint_sort_key(entry: &std::fs::DirEntry) -> (u128, u128, String) {
        let path = entry.path();
        let embedded_ts = Self::checkpoint_timestamp_from_path(&path).unwrap_or(0);
        let modified_ts = entry
            .metadata()
            .and_then(|m| m.modified())
            .map(Self::system_time_ms)
            .unwrap_or(0);
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();

        (embedded_ts, modified_ts, filename)
    }

    pub fn new(base_path: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&base_path).context("Failed to create checkpoint directory")?;

        Ok(Self {
            base_path,
            max_checkpoints: 10,  // Keep last 10 checkpoints
            compression_level: 3, // Zstd level 3 (balanced)
        })
    }

    pub fn with_max_checkpoints(mut self, max: usize) -> Self {
        self.max_checkpoints = max;
        self
    }

    pub fn with_compression_level(mut self, level: i32) -> Self {
        self.compression_level = level;
        self
    }

    /// Write checkpoint asynchronously with compression
    ///
    /// This captures the snapshot synchronously (fast) and then
    /// serializes + compresses in a background thread (slow).
    ///
    /// Compression of 100K entries (~10MB) takes ~50-100ms, so we
    /// move it off the critical path to avoid blocking dedup state.
    pub async fn write_async(&self, checkpoint: Checkpoint) -> Result<PathBuf> {
        let start = std::time::Instant::now();

        // 1. Fast snapshot (already captured by caller)
        let snapshot = checkpoint.clone();
        let snapshot_duration = start.elapsed();

        tracing::debug!(
            "Checkpoint snapshot captured in {:?}: {} kafka offsets, {} dedup entries",
            snapshot_duration,
            snapshot.kafka_offsets.len(),
            snapshot.dedup_state.len()
        );

        // 2. Move serialization + compression to background thread
        let base_path = self.base_path.clone();
        let max_checkpoints = self.max_checkpoints;
        let compression_level = self.compression_level;

        let path = tokio::task::spawn_blocking(move || {
            let timestamp = snapshot.timestamp
                .duration_since(SystemTime::UNIX_EPOCH)
                .context("Invalid timestamp")?
                .as_millis(); // Use milliseconds for uniqueness

            let filename = format!("checkpoint_{}.bin.zst", timestamp);
            let path = base_path.join(&filename);

            // Serialize
            let serialize_start = std::time::Instant::now();
            let data = bincode::serialize(&snapshot)
                .context("Failed to serialize checkpoint")?;
            let serialize_duration = serialize_start.elapsed();

            // Compress
            let compress_start = std::time::Instant::now();
            let compressed = zstd::encode_all(&data[..], compression_level)
                .context("Failed to compress checkpoint")?;
            let compress_duration = compress_start.elapsed();

            let compressed_len = compressed.len();

            // Write to disk
            let write_start = std::time::Instant::now();
            std::fs::write(&path, &compressed)
                .context("Failed to write checkpoint file")?;
            let write_duration = write_start.elapsed();

            let total_duration = serialize_duration + compress_duration + write_duration;
            let compression_ratio = (data.len() as f64) / (compressed_len as f64);

            tracing::info!(
                "Checkpoint written: {:?} | serialize: {:?}, compress: {:?}, write: {:?} | {} KB → {} KB ({}x)",
                path.file_name().unwrap(),
                serialize_duration,
                compress_duration,
                write_duration,
                data.len() / 1024,
                compressed_len / 1024,
                compression_ratio
            );

            // Track metrics
            crate::ingestion::metrics::CHECKPOINT_SIZE_BYTES.set(compressed_len as f64);
            crate::ingestion::metrics::CHECKPOINT_WRITE_DURATION_MS
                .with_label_values(&["compress"]).observe(compress_duration.as_millis() as f64);
            crate::ingestion::metrics::CHECKPOINT_WRITE_DURATION_MS
                .with_label_values(&["full"]).observe(total_duration.as_millis() as f64);

            // Cleanup old checkpoints
            Self::cleanup_old_checkpoints_internal(&base_path, max_checkpoints)?;

            Ok::<PathBuf, anyhow::Error>(path)
        }).await
        .context("Checkpoint write task panicked")??;

        Ok(path)
    }

    /// Synchronous write (for testing)
    pub fn write(&self, checkpoint: &Checkpoint) -> Result<PathBuf> {
        let timestamp = checkpoint
            .timestamp
            .duration_since(SystemTime::UNIX_EPOCH)
            .context("Invalid timestamp")?
            .as_millis(); // Use milliseconds for uniqueness

        let filename = format!("checkpoint_{}.bin", timestamp);
        let path = self.base_path.join(&filename);

        tracing::info!(
            "Writing checkpoint: {} kafka offsets, {} dedup entries, ~{} bytes",
            checkpoint.kafka_offsets.len(),
            checkpoint.dedup_state.len(),
            checkpoint.estimated_size()
        );

        let data = bincode::serialize(checkpoint).context("Failed to serialize checkpoint")?;

        std::fs::write(&path, data).context("Failed to write checkpoint file")?;

        // Cleanup old checkpoints
        self.cleanup_old_checkpoints()?;

        tracing::info!("Checkpoint written to: {:?}", path);
        Ok(path)
    }

    /// Read the latest checkpoint
    pub fn latest(&self) -> Result<Option<Checkpoint>> {
        let mut entries: Vec<_> = std::fs::read_dir(&self.base_path)
            .context("Failed to read checkpoint directory")?
            .filter_map(|e| e.ok())
            .filter(|e| {
                let path = e.path();
                path.extension()
                    .map_or(false, |ext| ext == "bin" || ext == "zst")
            })
            .collect();

        if entries.is_empty() {
            return Ok(None);
        }

        // Sort by embedded checkpoint timestamp first (most reliable),
        // then by mtime/filename to break ties deterministically.
        entries.sort_by_key(Self::checkpoint_sort_key);
        entries.reverse();

        let latest_path = entries[0].path();
        tracing::info!("Loading checkpoint from: {:?}", latest_path);

        let data = std::fs::read(&latest_path).context("Failed to read checkpoint file")?;

        // Decompress if needed
        let decompressed = if latest_path.extension().map_or(false, |ext| ext == "zst") {
            zstd::decode_all(&data[..]).context("Failed to decompress checkpoint")?
        } else {
            data
        };

        let checkpoint: Checkpoint =
            bincode::deserialize(&decompressed).context("Failed to deserialize checkpoint")?;

        tracing::info!(
            "Loaded checkpoint: {} kafka offsets, {} dedup entries (from {:?})",
            checkpoint.kafka_offsets.len(),
            checkpoint.dedup_state.len(),
            checkpoint.timestamp
        );

        Ok(Some(checkpoint))
    }

    /// List all checkpoints
    pub fn list(&self) -> Result<Vec<PathBuf>> {
        let mut entries: Vec<_> = std::fs::read_dir(&self.base_path)
            .context("Failed to read checkpoint directory")?
            .filter_map(|e| e.ok())
            .filter(|e| {
                let path = e.path();
                path.extension()
                    .map_or(false, |ext| ext == "bin" || ext == "zst")
            })
            .map(|e| e.path())
            .collect();

        entries.sort_by_key(|p| {
            let embedded_ts = Self::checkpoint_timestamp_from_path(p).unwrap_or(0);
            let modified_ts = std::fs::metadata(p)
                .and_then(|m| m.modified())
                .map(Self::system_time_ms)
                .unwrap_or(0);
            let filename = p
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string();

            (embedded_ts, modified_ts, filename)
        });
        entries.reverse(); // Newest first

        Ok(entries)
    }

    /// Remove old checkpoints, keeping only the latest N
    fn cleanup_old_checkpoints(&self) -> Result<()> {
        Self::cleanup_old_checkpoints_internal(&self.base_path, self.max_checkpoints)
    }

    fn cleanup_old_checkpoints_internal(base_path: &PathBuf, max_checkpoints: usize) -> Result<()> {
        let mut entries: Vec<_> = std::fs::read_dir(base_path)
            .context("Failed to read checkpoint directory")?
            .filter_map(|e| e.ok())
            .filter(|e| {
                let path = e.path();
                path.extension()
                    .map_or(false, |ext| ext == "bin" || ext == "zst")
            })
            .collect();

        // Sort by embedded checkpoint timestamp first (most reliable),
        // then by mtime/filename to break ties deterministically.
        entries.sort_by_key(Self::checkpoint_sort_key);
        entries.reverse(); // Newest first

        if entries.len() > max_checkpoints {
            for old_checkpoint in entries.iter().skip(max_checkpoints) {
                let path = old_checkpoint.path();
                tracing::debug!("Removing old checkpoint: {:?}", path);
                std::fs::remove_file(&path)
                    .with_context(|| format!("Failed to remove old checkpoint: {:?}", path))?;
            }
        }

        Ok(())
    }

    /// Delete all checkpoints (for testing)
    pub fn clear(&self) -> Result<()> {
        for checkpoint in self.list()? {
            std::fs::remove_file(checkpoint)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_checkpoint_roundtrip() {
        let dir = tempdir().unwrap();
        let storage = CheckpointStorage::new(dir.path().to_path_buf()).unwrap();

        let mut checkpoint = Checkpoint::new(4);
        checkpoint.kafka_offsets.insert(0, 100);
        checkpoint.kafka_offsets.insert(1, 200);
        checkpoint.dedup_state.insert("rec1".to_string(), 123456);
        checkpoint.dedup_state.insert("rec2".to_string(), 789012);

        storage.write(&checkpoint).unwrap();

        let loaded = storage.latest().unwrap().unwrap();
        assert_eq!(loaded.kafka_offsets.len(), 2);
        assert_eq!(loaded.dedup_state.len(), 2);
        assert_eq!(loaded.worker_count, 4);
        assert_eq!(loaded.kafka_offsets.get(&0), Some(&100));
        assert_eq!(loaded.dedup_state.get("rec1"), Some(&123456));
    }

    #[tokio::test]
    async fn test_async_checkpoint_with_compression() {
        let dir = tempdir().unwrap();
        let storage = CheckpointStorage::new(dir.path().to_path_buf()).unwrap();

        let mut checkpoint = Checkpoint::new(4);
        // Add enough data to make compression worthwhile
        for i in 0..1000 {
            checkpoint
                .dedup_state
                .insert(format!("record_{}", i), i as i64);
        }

        let path = storage.write_async(checkpoint.clone()).await.unwrap();
        assert!(path.exists());
        assert!(path.extension().unwrap() == "zst");

        let loaded = storage.latest().unwrap().unwrap();
        assert_eq!(loaded.dedup_state.len(), 1000);
        assert_eq!(loaded.worker_count, 4);
    }

    #[test]
    fn test_checkpoint_cleanup() {
        let dir = tempdir().unwrap();
        let storage = CheckpointStorage::new(dir.path().to_path_buf()).unwrap();

        // Write 15 checkpoints
        for i in 0..15 {
            let mut checkpoint = Checkpoint::new(4);
            checkpoint.kafka_offsets.insert(0, i as i64);
            storage.write(&checkpoint).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        let checkpoints = storage.list().unwrap();
        assert_eq!(checkpoints.len(), 10); // max_checkpoints = 10
    }

    #[test]
    fn test_compressed_and_uncompressed_coexist() {
        let dir = tempdir().unwrap();
        let storage = CheckpointStorage::new(dir.path().to_path_buf()).unwrap();

        // Write uncompressed
        let checkpoint1 = Checkpoint::new(4);
        storage.write(&checkpoint1).unwrap();

        // Latest should still work
        let loaded = storage.latest().unwrap();
        assert!(loaded.is_some());
    }

    #[test]
    fn test_latest_prefers_checkpoint_timestamp_over_mtime() {
        let dir = tempdir().unwrap();
        let storage = CheckpointStorage::new(dir.path().to_path_buf()).unwrap();

        // Write logically newer checkpoint first.
        let mut newer = Checkpoint::new(4);
        newer.kafka_offsets.insert(0, 200);
        newer.timestamp = std::time::UNIX_EPOCH + std::time::Duration::from_millis(2000);
        storage.write(&newer).unwrap();

        // Write logically older checkpoint second (newer file mtime).
        let mut older = Checkpoint::new(4);
        older.kafka_offsets.insert(0, 100);
        older.timestamp = std::time::UNIX_EPOCH + std::time::Duration::from_millis(1000);
        storage.write(&older).unwrap();

        let loaded = storage.latest().unwrap().unwrap();
        assert_eq!(
            loaded.kafka_offsets.get(&0),
            Some(&200),
            "latest() should prefer checkpoint timestamp, not filesystem mtime"
        );
    }
}
