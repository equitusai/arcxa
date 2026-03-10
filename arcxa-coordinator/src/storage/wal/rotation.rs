// WAL Rotation and Compaction Management
//
// Handles log rotation policies and compaction strategies for space management

use async_trait::async_trait;
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use super::{
    CompactionPolicy, CompactionReport, LogSequenceNumber, RotationError, RotationPolicy,
    WalConfig, WalEntry, WalError, WalResult,
};

/// Manages WAL rotation and compaction
pub struct RotationManager {
    config: WalConfig,
    active_rotations: Arc<RwLock<BTreeMap<u64, RotationState>>>,
}

#[derive(Debug, Clone)]
struct RotationState {
    segment_id: u64,
    started_at: Instant,
    completed: bool,
    new_segment_path: Option<PathBuf>,
}

/// Strategy for compacting segments
#[async_trait]
pub trait CompactionStrategy: Send + Sync {
    /// Determine if compaction is needed
    async fn should_compact(&self, segments: &[SegmentInfo]) -> bool;

    /// Select segments for compaction
    async fn select_segments(&self, segments: &[SegmentInfo]) -> Vec<SegmentInfo>;

    /// Compact selected segments
    async fn compact(
        &self,
        segments: Vec<SegmentInfo>,
        committed_lsn: LogSequenceNumber,
    ) -> WalResult<CompactionReport>;
}

/// Default compaction strategy
pub struct DefaultCompactionStrategy {
    policy: CompactionPolicy,
}

#[derive(Debug, Clone)]
pub struct SegmentInfo {
    pub id: u64,
    pub path: PathBuf,
    pub start_lsn: LogSequenceNumber,
    pub end_lsn: Option<LogSequenceNumber>,
    pub size: u64,
    pub entry_count: u64,
    pub live_entries: u64,
    pub created_at: SystemTime,
    pub last_modified: SystemTime,
    pub archived: bool,
}

impl SegmentInfo {
    /// Calculate dead space ratio
    pub fn dead_space_ratio(&self) -> f64 {
        if self.entry_count == 0 {
            return 0.0;
        }
        1.0 - (self.live_entries as f64 / self.entry_count as f64)
    }

    /// Check if segment is eligible for compaction
    pub fn is_compactable(&self, committed_lsn: LogSequenceNumber) -> bool {
        !self.archived && self.end_lsn.map_or(false, |end| end <= committed_lsn)
    }
}

impl RotationManager {
    pub fn new(config: WalConfig) -> Self {
        Self {
            config,
            active_rotations: Arc::new(RwLock::new(BTreeMap::new())),
        }
    }

    /// Check if rotation is needed based on policy
    pub async fn should_rotate(
        &self,
        current_segment: &SegmentInfo,
        current_time: SystemTime,
    ) -> bool {
        match &self.config.rotation_policy {
            RotationPolicy::Size { max_size } => current_segment.size >= *max_size,
            RotationPolicy::Time { max_age } => {
                current_time
                    .duration_since(current_segment.created_at)
                    .unwrap_or(Duration::ZERO)
                    >= *max_age
            }
            RotationPolicy::SizeAndTime { max_size, max_age } => {
                current_segment.size >= *max_size
                    || current_time
                        .duration_since(current_segment.created_at)
                        .unwrap_or(Duration::ZERO)
                        >= *max_age
            }
            RotationPolicy::EntryCount { max_entries } => {
                current_segment.entry_count >= *max_entries
            }
            RotationPolicy::Custom(name) => {
                // Hook for custom rotation policies
                self.evaluate_custom_policy(name, current_segment).await
            }
        }
    }

    /// Perform segment rotation
    pub async fn rotate(
        &self,
        current_segment: &SegmentInfo,
        next_lsn: LogSequenceNumber,
    ) -> WalResult<PathBuf> {
        let rotation_id = current_segment.id;
        let start = Instant::now();

        // Record rotation start
        {
            let mut rotations = self.active_rotations.write().await;
            rotations.insert(
                rotation_id,
                RotationState {
                    segment_id: current_segment.id,
                    started_at: start,
                    completed: false,
                    new_segment_path: None,
                },
            );
        }

        // Create new segment file
        let new_segment_id = current_segment.id + 1;
        let new_segment_path = self.create_new_segment(new_segment_id, next_lsn)?;

        // Finalize current segment
        self.finalize_segment(current_segment).await?;

        // Update rotation state
        {
            let mut rotations = self.active_rotations.write().await;
            if let Some(state) = rotations.get_mut(&rotation_id) {
                state.completed = true;
                state.new_segment_path = Some(new_segment_path.clone());
            }
        }

        info!(
            "Rotated segment {} -> {} in {:?}",
            current_segment.id,
            new_segment_id,
            start.elapsed()
        );

        Ok(new_segment_path)
    }

    /// Create compaction manager
    pub fn compaction_strategy(&self) -> Box<dyn CompactionStrategy> {
        Box::new(DefaultCompactionStrategy {
            policy: self.config.compaction_policy.clone(),
        })
    }

    /// Archive old segments
    pub async fn archive_segments(
        &self,
        segments: Vec<SegmentInfo>,
        archive_path: &Path,
    ) -> WalResult<Vec<PathBuf>> {
        fs::create_dir_all(archive_path).map_err(|e| {
            WalError::Rotation(RotationError::ArchiveFailed {
                reason: format!("Failed to create archive directory: {}", e),
            })
        })?;

        let mut archived = Vec::new();

        for segment in segments {
            if segment.archived {
                continue;
            }

            let archive_file = archive_path.join(segment.path.file_name().ok_or_else(|| {
                WalError::Rotation(RotationError::ArchiveFailed {
                    reason: "Invalid segment path".to_string(),
                })
            })?);

            // Use hard link if possible, otherwise copy
            match fs::hard_link(&segment.path, &archive_file) {
                Ok(_) => {
                    debug!("Hard linked segment {} to archive", segment.id);
                }
                Err(_) => {
                    // Fall back to copy
                    fs::copy(&segment.path, &archive_file).map_err(|e| {
                        WalError::Rotation(RotationError::ArchiveFailed {
                            reason: format!("Failed to copy segment: {}", e),
                        })
                    })?;
                    debug!("Copied segment {} to archive", segment.id);
                }
            }

            archived.push(archive_file);
        }

        info!("Archived {} segments", archived.len());
        Ok(archived)
    }

    fn create_new_segment(
        &self,
        segment_id: u64,
        start_lsn: LogSequenceNumber,
    ) -> WalResult<PathBuf> {
        let filename = format!("wal_{:08}.log", segment_id);
        let path = self.config.path.join(&filename);

        let mut file = File::create(&path).map_err(|e| WalError::Io {
            source: e,
            path: Some(path.clone()),
        })?;

        // Write segment header
        let header = SegmentHeader {
            magic: SEGMENT_MAGIC,
            version: SEGMENT_VERSION,
            segment_id,
            start_lsn,
            created_at: SystemTime::now(),
        };

        let header_bytes =
            bincode::serialize(&header).map_err(|e| WalError::Serialization(e.to_string()))?;

        file.write_all(&header_bytes).map_err(|e| WalError::Io {
            source: e,
            path: Some(path.clone()),
        })?;

        // Preallocate if configured
        if self.config.preallocate {
            file.set_len(self.config.max_file_size)
                .map_err(|e| WalError::Io {
                    source: e,
                    path: Some(path.clone()),
                })?;
        }

        file.sync_all().map_err(|e| WalError::Io {
            source: e,
            path: Some(path.clone()),
        })?;

        Ok(path)
    }

    async fn finalize_segment(&self, segment: &SegmentInfo) -> WalResult<()> {
        // Write segment footer
        let footer = SegmentFooter {
            end_lsn: segment.end_lsn,
            entry_count: segment.entry_count,
            checksum: 0, // Would compute actual checksum
            finalized_at: SystemTime::now(),
        };

        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(&segment.path)
            .map_err(|e| WalError::Io {
                source: e,
                path: Some(segment.path.clone()),
            })?;

        let footer_bytes =
            bincode::serialize(&footer).map_err(|e| WalError::Serialization(e.to_string()))?;

        file.write_all(&footer_bytes).map_err(|e| WalError::Io {
            source: e,
            path: Some(segment.path.clone()),
        })?;

        file.sync_all().map_err(|e| WalError::Io {
            source: e,
            path: Some(segment.path.clone()),
        })?;

        Ok(())
    }

    async fn evaluate_custom_policy(&self, name: &str, segment: &SegmentInfo) -> bool {
        // Hook for custom rotation policies
        match name {
            "hourly" => {
                segment.created_at.elapsed().unwrap_or(Duration::ZERO) >= Duration::from_secs(3600)
            }
            "daily" => {
                segment.created_at.elapsed().unwrap_or(Duration::ZERO) >= Duration::from_secs(86400)
            }
            _ => false,
        }
    }
}

#[async_trait]
impl CompactionStrategy for DefaultCompactionStrategy {
    async fn should_compact(&self, segments: &[SegmentInfo]) -> bool {
        if !self.policy.enabled {
            return false;
        }

        let compactable: Vec<_> = segments
            .iter()
            .filter(|s| !s.archived && s.end_lsn.is_some())
            .collect();

        compactable.len() >= self.policy.min_segments
    }

    async fn select_segments(&self, segments: &[SegmentInfo]) -> Vec<SegmentInfo> {
        let mut selected = Vec::new();

        for segment in segments {
            if segment.archived || segment.end_lsn.is_none() {
                continue;
            }

            // Select based on dead space ratio
            if segment.dead_space_ratio() >= self.policy.compaction_threshold {
                selected.push(segment.clone());

                if selected.len() >= self.policy.max_segments {
                    break;
                }
            }
        }

        selected
    }

    async fn compact(
        &self,
        segments: Vec<SegmentInfo>,
        committed_lsn: LogSequenceNumber,
    ) -> WalResult<CompactionReport> {
        let start = Instant::now();
        let mut segments_compacted = 0;
        let mut bytes_reclaimed = 0u64;
        let mut entries_removed = 0u64;

        for segment in segments {
            if !segment.is_compactable(committed_lsn) {
                continue;
            }

            // Create compacted segment
            let compacted_path = segment.path.with_extension("compact");

            let (reclaimed, removed) = self
                .compact_segment(&segment, &compacted_path, committed_lsn)
                .await?;

            // Atomic rename
            fs::rename(&compacted_path, &segment.path).map_err(|e| {
                WalError::Rotation(RotationError::CompactionFailed {
                    reason: format!("Failed to replace segment: {}", e),
                })
            })?;

            segments_compacted += 1;
            bytes_reclaimed += reclaimed;
            entries_removed += removed;
        }

        Ok(CompactionReport {
            segments_compacted,
            bytes_reclaimed,
            entries_removed,
            duration_ms: start.elapsed().as_millis() as u64,
        })
    }
}

impl DefaultCompactionStrategy {
    async fn compact_segment(
        &self,
        segment: &SegmentInfo,
        output_path: &Path,
        committed_lsn: LogSequenceNumber,
    ) -> WalResult<(u64, u64)> {
        let input = File::open(&segment.path).map_err(|e| WalError::Io {
            source: e,
            path: Some(segment.path.clone()),
        })?;

        let mut output = File::create(output_path).map_err(|e| WalError::Io {
            source: e,
            path: Some(output_path.to_path_buf()),
        })?;

        let mut bytes_reclaimed = 0u64;
        let mut entries_removed = 0u64;

        // Read and filter entries
        let mut reader = std::io::BufReader::new(input);
        let mut buffer = Vec::new();

        // Skip header
        let mut header_buf = vec![0u8; std::mem::size_of::<SegmentHeader>()];
        use std::io::Read;
        reader
            .read_exact(&mut header_buf)
            .map_err(|e| WalError::Io {
                source: e,
                path: Some(segment.path.clone()),
            })?;

        // Write header to output
        output.write_all(&header_buf).map_err(|e| WalError::Io {
            source: e,
            path: Some(output_path.to_path_buf()),
        })?;

        // Process entries
        loop {
            buffer.clear();

            // Try to read entry size
            let mut size_buf = [0u8; 8];
            match reader.read_exact(&mut size_buf) {
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => {
                    return Err(WalError::Io {
                        source: e,
                        path: Some(segment.path.clone()),
                    })
                }
            }

            let entry_size = u64::from_le_bytes(size_buf);

            // Read entry data
            buffer.resize(entry_size as usize, 0);
            buffer[0..8].copy_from_slice(&size_buf);

            reader
                .read_exact(&mut buffer[8..])
                .map_err(|e| WalError::Io {
                    source: e,
                    path: Some(segment.path.clone()),
                })?;

            // Deserialize to check if entry is committed
            match WalEntry::from_bytes(&buffer) {
                Ok(entry) if entry.header.lsn <= committed_lsn => {
                    // Entry is committed, can be removed
                    bytes_reclaimed += buffer.len() as u64;
                    entries_removed += 1;
                }
                Ok(_) => {
                    // Entry not yet committed, keep it
                    output.write_all(&buffer).map_err(|e| WalError::Io {
                        source: e,
                        path: Some(output_path.to_path_buf()),
                    })?;
                }
                Err(_) => {
                    // Corrupted entry, skip
                    bytes_reclaimed += buffer.len() as u64;
                    entries_removed += 1;
                }
            }
        }

        output.sync_all().map_err(|e| WalError::Io {
            source: e,
            path: Some(output_path.to_path_buf()),
        })?;

        Ok((bytes_reclaimed, entries_removed))
    }
}

// Segment file format structures

const SEGMENT_MAGIC: u32 = 0x57414C47; // "WALG"
const SEGMENT_VERSION: u32 = 1;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct SegmentHeader {
    magic: u32,
    version: u32,
    segment_id: u64,
    start_lsn: LogSequenceNumber,
    created_at: SystemTime,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct SegmentFooter {
    end_lsn: Option<LogSequenceNumber>,
    entry_count: u64,
    checksum: u32,
    finalized_at: SystemTime,
}
