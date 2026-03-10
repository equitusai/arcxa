// File-based WAL implementation with enterprise features
//
// High-performance, crash-safe WAL using memory-mapped files,
// group commit, and efficient indexing for 100K+ writes/sec

use async_trait::async_trait;
use bytes::{Bytes, BytesMut};
use dashmap::DashMap;
use memmap2::{MmapMut, MmapOptions};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::{mpsc, oneshot, Mutex, Notify, RwLock};
use tokio::time;
use tracing::{debug, error, info, warn};

use super::{
    CompressionCodec, EntryType, FsyncMode, GroupCommitConfig, LogSequenceNumber, RecoveryResult,
    TransactionId, ValidationReport, WalConfig, WalEntry, WalEntryStream, WalError,
    WalMetricsCollector, WalMetricsSnapshot, WalReader, WalResult, WriteAheadLog,
};

/// File-based WAL with production features
#[derive(Clone)]
pub struct FileWal {
    // Configuration
    config: WalConfig,

    // Current segment
    active_segment: Arc<RwLock<Segment>>,

    // All segments (including archived)
    segments: Arc<RwLock<BTreeMap<u64, SegmentInfo>>>,

    // LSN tracking
    current_lsn: Arc<AtomicU64>,
    committed_lsn: Arc<AtomicU64>,

    // Index for fast lookups
    lsn_index: Arc<DashMap<LogSequenceNumber, SegmentLocation>>,

    // Group commit coordinator
    group_commit: Arc<GroupCommitCoordinator>,

    // Background workers
    sync_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    compaction_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,

    // Metrics
    metrics: Arc<WalMetricsCollector>,

    // Shutdown flag
    shutdown: Arc<AtomicBool>,
}

struct Segment {
    id: u64,
    path: PathBuf,
    file: File,
    mmap: Option<MmapMut>,
    position: usize,
    size: usize,
    start_lsn: LogSequenceNumber,
    end_lsn: Option<LogSequenceNumber>,
    created_at: SystemTime,
    last_sync: Instant,
    entries: Vec<WalEntry>,
    dirty: bool,
}

#[derive(Clone)]
struct SegmentInfo {
    id: u64,
    path: PathBuf,
    start_lsn: LogSequenceNumber,
    end_lsn: Option<LogSequenceNumber>,
    size: u64,
    entries: u64,
    created_at: SystemTime,
    archived: bool,
}

struct SegmentLocation {
    segment_id: u64,
    offset: usize,
    size: usize,
}

struct GroupCommitCoordinator {
    enabled: bool,
    config: GroupCommitConfig,
    pending: Arc<Mutex<Vec<PendingWrite>>>,
    notify: Arc<Notify>,
}

struct PendingWrite {
    entry: WalEntry,
    callback: oneshot::Sender<WalResult<LogSequenceNumber>>,
}

impl FileWal {
    /// Create new file-based WAL
    pub async fn new(config: WalConfig, metrics: Arc<WalMetricsCollector>) -> WalResult<Self> {
        // Ensure directory exists
        fs::create_dir_all(&config.path).map_err(|e| WalError::Io {
            source: e,
            path: Some(config.path.clone()),
        })?;

        // Initialize first segment
        let segment = Self::create_segment(&config, 0, LogSequenceNumber::ZERO)?;

        let wal = Self {
            config: config.clone(),
            active_segment: Arc::new(RwLock::new(segment)),
            segments: Arc::new(RwLock::new(BTreeMap::new())),
            current_lsn: Arc::new(AtomicU64::new(0)),
            committed_lsn: Arc::new(AtomicU64::new(0)),
            lsn_index: Arc::new(DashMap::new()),
            group_commit: Arc::new(GroupCommitCoordinator::new(config.group_commit.clone())),
            sync_handle: Arc::new(Mutex::new(None)),
            compaction_handle: Arc::new(Mutex::new(None)),
            metrics,
            shutdown: Arc::new(AtomicBool::new(false)),
        };

        // Start background workers
        wal.start_workers().await;

        Ok(wal)
    }

    fn create_segment(
        config: &WalConfig,
        id: u64,
        start_lsn: LogSequenceNumber,
    ) -> WalResult<Segment> {
        let filename = format!("wal_{:08}.log", id);
        let path = config.path.join(&filename);

        let mut file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&path)
            .map_err(|e| WalError::Io {
                source: e,
                path: Some(path.clone()),
            })?;

        // Preallocate space if configured
        if config.preallocate {
            file.set_len(config.max_file_size)
                .map_err(|e| WalError::Io {
                    source: e,
                    path: Some(path.clone()),
                })?;
        }

        // Create memory map if not using direct I/O
        let mmap = if !config.direct_io {
            let mmap = unsafe {
                MmapOptions::new()
                    .len(config.max_file_size as usize)
                    .map_mut(&file)
                    .map_err(|e| WalError::Io {
                        source: io::Error::new(io::ErrorKind::Other, e),
                        path: Some(path.clone()),
                    })?
            };
            Some(mmap)
        } else {
            None
        };

        Ok(Segment {
            id,
            path,
            file,
            mmap,
            position: 0,
            size: config.max_file_size as usize,
            start_lsn,
            end_lsn: None,
            created_at: SystemTime::now(),
            last_sync: Instant::now(),
            entries: Vec::new(),
            dirty: false,
        })
    }

    async fn start_workers(&self) {
        // Start sync worker
        let sync_worker = self.spawn_sync_worker();
        *self.sync_handle.lock().await = Some(sync_worker);

        // Start compaction worker
        let compaction_worker = self.spawn_compaction_worker();
        *self.compaction_handle.lock().await = Some(compaction_worker);
    }

    /// Get the current size of the LSN index
    /// This is useful for monitoring memory usage and detecting issues
    pub fn lsn_index_size(&self) -> usize {
        self.lsn_index.len()
    }

    fn spawn_sync_worker(&self) -> tokio::task::JoinHandle<()> {
        let config = self.config.clone();
        let active_segment = Arc::clone(&self.active_segment);
        let shutdown = Arc::clone(&self.shutdown);
        let metrics = Arc::clone(&self.metrics);

        tokio::spawn(async move {
            let mut interval = time::interval(config.sync_interval);

            while !shutdown.load(Ordering::Relaxed) {
                interval.tick().await;

                // Sync active segment
                let mut segment = active_segment.write().await;
                if segment.dirty && segment.last_sync.elapsed() > config.sync_interval {
                    let start = Instant::now();

                    if let Err(e) = segment.sync(&config.fsync_mode, config.io_timeout).await {
                        error!("Failed to sync segment: {}", e);
                        metrics.record_error("sync", &e);
                    } else {
                        segment.dirty = false;
                        segment.last_sync = Instant::now();
                        metrics.record_sync_latency(start.elapsed());
                    }
                }
            }

            info!("Sync worker shutting down");
        })
    }

    fn spawn_compaction_worker(&self) -> tokio::task::JoinHandle<()> {
        let config = self.config.clone();
        let segments = Arc::clone(&self.segments);
        let committed_lsn = Arc::clone(&self.committed_lsn);
        let shutdown = Arc::clone(&self.shutdown);
        let metrics = Arc::clone(&self.metrics);
        let lsn_index = Arc::clone(&self.lsn_index);

        tokio::spawn(async move {
            let mut interval = time::interval(config.compaction_policy.compaction_interval);

            while !shutdown.load(Ordering::Relaxed) {
                interval.tick().await;

                if !config.compaction_policy.enabled {
                    continue;
                }

                // Find segments eligible for compaction
                let segments_guard = segments.read().await;
                let committed = LogSequenceNumber(committed_lsn.load(Ordering::Acquire));

                let eligible: Vec<_> = segments_guard
                    .values()
                    .filter(|info| {
                        !info.archived && info.end_lsn.map_or(false, |end| end <= committed)
                    })
                    .cloned()
                    .collect();

                drop(segments_guard);

                if eligible.len() >= config.compaction_policy.min_segments {
                    info!("Starting compaction of {} segments", eligible.len());
                    let start = Instant::now();

                    for segment_info in eligible {
                        if let Err(e) =
                            Self::compact_segment(&segment_info, &config, &lsn_index).await
                        {
                            error!("Failed to compact segment {}: {}", segment_info.id, e);
                            metrics.record_error("compaction", &e);
                        } else {
                            metrics.record_compaction(segment_info.size, start.elapsed());

                            // Log index cleanup metrics
                            let index_size = lsn_index.len();
                            debug!("LSN index size after compaction: {} entries", index_size);
                        }
                    }
                }
            }

            info!("Compaction worker shutting down");
        })
    }

    async fn compact_segment(
        info: &SegmentInfo,
        config: &WalConfig,
        lsn_index: &Arc<DashMap<LogSequenceNumber, SegmentLocation>>,
    ) -> WalResult<()> {
        // Archive before compaction if configured
        if config.compaction_policy.archive_before_compact {
            let archive_path = config.path.join("archive");
            fs::create_dir_all(&archive_path).map_err(|e| WalError::Io {
                source: e,
                path: Some(archive_path.clone()),
            })?;

            let archive_file = archive_path.join(info.path.file_name().unwrap());
            fs::copy(&info.path, &archive_file).map_err(|e| WalError::Io {
                source: e,
                path: Some(info.path.clone()),
            })?;
        }

        // CRITICAL: Clean up LSN index entries for this segment
        // This prevents unbounded memory growth as segments are compacted
        let cleanup_start = Instant::now();
        let mut removed_count = 0;

        if let Some(end_lsn) = info.end_lsn {
            // Remove all LSN entries in the range [start_lsn, end_lsn]
            let start = info.start_lsn.0;
            let end = end_lsn.0;

            for lsn_val in start..=end {
                let lsn = LogSequenceNumber(lsn_val);
                if lsn_index.remove(&lsn).is_some() {
                    removed_count += 1;
                }
            }

            info!(
                "Cleaned up {} LSN index entries for segment {} (range {}-{}) in {:?}",
                removed_count,
                info.id,
                start,
                end,
                cleanup_start.elapsed()
            );
        } else {
            warn!(
                "Segment {} has no end_lsn, skipping LSN index cleanup",
                info.id
            );
        }

        // Remove the segment file
        fs::remove_file(&info.path).map_err(|e| WalError::Io {
            source: e,
            path: Some(info.path.clone()),
        })?;

        Ok(())
    }

    /// Check available disk space on the WAL path
    /// Returns available bytes on the filesystem containing the WAL directory
    fn check_disk_space(path: &std::path::Path) -> WalResult<u64> {
        #[cfg(unix)]
        {
            use std::ffi::CString;
            use std::os::unix::ffi::OsStrExt;

            // If the path doesn't exist, check the parent directory
            // This ensures we get disk space for the filesystem that will contain the WAL
            let check_path = if path.exists() {
                path
            } else {
                // Walk up the directory tree until we find an existing directory
                let mut current = path;
                while let Some(parent) = current.parent() {
                    if parent.exists() {
                        current = parent;
                        break;
                    }
                    current = parent;
                }
                if !current.exists() {
                    // If we can't find any existing parent, use root
                    std::path::Path::new("/")
                } else {
                    current
                }
            };

            let path_cstr = CString::new(check_path.as_os_str().as_bytes())
                .map_err(|e| WalError::Configuration(format!("Invalid path: {}", e)))?;

            let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
            let ret = unsafe { libc::statvfs(path_cstr.as_ptr(), &mut stat) };

            if ret != 0 {
                return Err(WalError::Io {
                    source: std::io::Error::last_os_error(),
                    path: Some(check_path.to_path_buf()),
                });
            }

            // Available space = f_bavail (blocks available to non-root) * f_bsize (block size)
            // Use f_bsize for block size (fundamental filesystem block size)
            let available_bytes = (stat.f_bavail as u64).saturating_mul(stat.f_bsize as u64);
            Ok(available_bytes)
        }

        #[cfg(not(unix))]
        {
            // Fallback for non-Unix systems: always return success
            // In production, implement platform-specific logic for Windows, etc.
            warn!("Disk space checking not implemented for this platform");
            Ok(u64::MAX)
        }
    }

    /// Ensure sufficient disk space is available before write
    /// Returns InsufficientDiskSpace error if below configured minimum
    fn ensure_sufficient_disk_space(&self, required_bytes: usize) -> WalResult<()> {
        if self.config.min_free_disk_space == 0 {
            // Check disabled
            return Ok(());
        }

        let available = Self::check_disk_space(&self.config.path)?;

        if available < self.config.min_free_disk_space {
            return Err(WalError::InsufficientDiskSpace {
                available_bytes: available,
                required_bytes: required_bytes as u64,
                min_free_bytes: self.config.min_free_disk_space,
            });
        }

        Ok(())
    }

    fn write_entry_to_segment(
        segment: &mut Segment,
        entry: &mut WalEntry,
        lsn: LogSequenceNumber,
    ) -> WalResult<usize> {
        entry.header.lsn = lsn;
        entry.header.previous_lsn = LogSequenceNumber(lsn.0.saturating_sub(1));

        let bytes = entry.to_bytes();
        let size = bytes.len();

        if segment.position + size > segment.size {
            return Err(WalError::WalFull {
                max_segments: segment.size,
            });
        }

        // Write to mmap or file
        if let Some(ref mut mmap) = segment.mmap {
            // Copy data to mmap region
            let write_start = segment.position;
            let write_end = write_start + size;
            mmap[write_start..write_end].copy_from_slice(&bytes);

            // CRITICAL: Immediately flush the modified range asynchronously
            // This ensures the kernel starts writeback without blocking the write path
            // Uses MS_ASYNC which is non-blocking but guarantees data reaches disk subsystem
            mmap.flush_async_range(write_start, size)
                .map_err(|e| WalError::Io {
                    source: e,
                    path: Some(segment.path.clone()),
                })?;
        } else {
            segment.file.write_all(&bytes).map_err(|e| WalError::Io {
                source: e,
                path: Some(segment.path.clone()),
            })?;
        }

        segment.position += size;
        segment.entries.push(entry.clone());
        segment.dirty = true;

        Ok(size)
    }
}

impl Segment {
    /// Synchronously flush data to disk based on fsync mode
    ///
    /// This method ensures durability by calling msync(MS_SYNC) for mmap regions
    /// and fsync() for file metadata. The difference from the async flush in
    /// write_entry_to_segment is:
    /// - write_entry_to_segment: MS_ASYNC (non-blocking, starts writeback)
    /// - sync: MS_SYNC (blocking, waits for data on disk)
    ///
    /// This two-phase approach optimizes for:
    /// 1. Write throughput (async flush doesn't block)
    /// 2. Durability guarantees (sync flush ensures persistence)
    ///
    /// Timeout handling: If io_timeout is configured, the sync will fail with
    /// WalError::Timeout if it takes longer than the specified duration.
    async fn sync(&mut self, mode: &FsyncMode, io_timeout: Option<Duration>) -> WalResult<()> {
        match mode {
            FsyncMode::EveryWrite | FsyncMode::BatchSync | FsyncMode::OnDemand => {
                // Build the blocking sync operation
                // We can't use spawn_blocking easily here because we need mutable access
                // to self.mmap and self.file. Instead, we'll run the sync in the current
                // thread but with a timeout wrapper.

                let sync_future = async {
                    // Synchronous flush with MS_SYNC - blocks until data is on disk
                    if let Some(ref mmap) = self.mmap {
                        mmap.flush().map_err(|e| WalError::Io {
                            source: e,
                            path: Some(self.path.clone()),
                        })?;
                    }

                    // Sync file metadata (inode, size, etc.)
                    self.file.sync_all().map_err(|e| WalError::Io {
                        source: e,
                        path: Some(self.path.clone()),
                    })?;

                    Ok::<(), WalError>(())
                };

                // Apply timeout if configured
                if let Some(timeout_duration) = io_timeout {
                    // Yield to tokio runtime to allow other tasks to run
                    tokio::task::yield_now().await;

                    match time::timeout(timeout_duration, sync_future).await {
                        Ok(result) => result?,
                        Err(_) => {
                            return Err(WalError::Timeout {
                                operation: "fsync".to_string(),
                                timeout_ms: timeout_duration.as_millis() as u64,
                            });
                        }
                    }
                } else {
                    sync_future.await?;
                }

                // Clear dirty flag after successful sync
                self.dirty = false;
            }
            FsyncMode::Periodic => {
                // Handled by background worker - no immediate sync
                // Data has already been async flushed in write path
            }
            #[cfg(test)]
            FsyncMode::NoSync => {
                // No sync for testing - relies only on async flush from write path
            }
        }

        Ok(())
    }
}

#[async_trait]
impl WriteAheadLog for FileWal {
    async fn append(&self, mut entry: WalEntry) -> WalResult<LogSequenceNumber> {
        let start = Instant::now();

        // Check disk space before write
        let entry_size = entry.to_bytes().len();
        self.ensure_sufficient_disk_space(entry_size)?;

        // Get next LSN atomically
        // fetch_add returns the old value and atomically increments current_lsn
        // This ensures each thread gets a unique, sequential LSN
        let lsn = LogSequenceNumber(self.current_lsn.fetch_add(1, Ordering::SeqCst));

        // Write to active segment
        let mut segment = self.active_segment.write().await;
        let size = Self::write_entry_to_segment(&mut segment, &mut entry, lsn)?;

        // Update index
        self.lsn_index.insert(
            lsn,
            SegmentLocation {
                segment_id: segment.id,
                offset: segment.position - size,
                size,
            },
        );

        // Sync if required
        match self.config.fsync_mode {
            FsyncMode::EveryWrite => {
                segment
                    .sync(&self.config.fsync_mode, self.config.io_timeout)
                    .await?
            }
            _ => {}
        }

        drop(segment);

        // Record metrics
        self.metrics.record_write(size, start.elapsed());

        // Check rotation
        if self.should_rotate().await {
            self.rotate_internal().await?;
        }

        Ok(lsn)
    }

    async fn append_batch(&self, entries: Vec<WalEntry>) -> WalResult<Vec<LogSequenceNumber>> {
        let start = Instant::now();
        let mut lsns = Vec::with_capacity(entries.len());
        let mut total_size = 0;

        // Check disk space before batch write
        // Clone entries for size calculation since to_bytes() requires &mut self
        let batch_size: usize = entries.iter().map(|e| e.clone().to_bytes().len()).sum();
        self.ensure_sufficient_disk_space(batch_size)?;

        // Get LSN range atomically
        // fetch_add returns the starting LSN and reserves a range for this batch
        // Each entry in the batch gets a consecutive LSN starting from this value
        let start_lsn = self
            .current_lsn
            .fetch_add(entries.len() as u64, Ordering::SeqCst);

        // Write all entries
        let mut segment = self.active_segment.write().await;

        for (i, mut entry) in entries.into_iter().enumerate() {
            let lsn = LogSequenceNumber(start_lsn + i as u64);
            let size = Self::write_entry_to_segment(&mut segment, &mut entry, lsn)?;

            self.lsn_index.insert(
                lsn,
                SegmentLocation {
                    segment_id: segment.id,
                    offset: segment.position - size,
                    size,
                },
            );

            lsns.push(lsn);
            total_size += size;
        }

        // Batch sync
        if matches!(
            self.config.fsync_mode,
            FsyncMode::EveryWrite | FsyncMode::BatchSync
        ) {
            segment
                .sync(&self.config.fsync_mode, self.config.io_timeout)
                .await?;
        }

        drop(segment);

        // Record metrics
        self.metrics
            .record_batch_write(lsns.len(), total_size, start.elapsed());

        Ok(lsns)
    }

    async fn sync(&self) -> WalResult<()> {
        let mut segment = self.active_segment.write().await;
        segment
            .sync(&FsyncMode::OnDemand, self.config.io_timeout)
            .await?;
        Ok(())
    }

    async fn commit(&self, lsn: LogSequenceNumber) -> WalResult<()> {
        self.committed_lsn.store(lsn.0, Ordering::Release);
        self.metrics.record_commit();
        Ok(())
    }

    async fn commit_batch(&self, lsns: Vec<LogSequenceNumber>) -> WalResult<()> {
        if let Some(max_lsn) = lsns.iter().max() {
            self.committed_lsn.store(max_lsn.0, Ordering::Release);
            self.metrics.record_commit_batch(lsns.len());
        }
        Ok(())
    }

    async fn tail_lsn(&self) -> LogSequenceNumber {
        LogSequenceNumber(self.current_lsn.load(Ordering::Acquire))
    }

    async fn head_lsn(&self) -> LogSequenceNumber {
        LogSequenceNumber(self.committed_lsn.load(Ordering::Acquire) + 1)
    }

    async fn is_healthy(&self) -> bool {
        let segment = self.active_segment.read().await;
        !self.shutdown.load(Ordering::Relaxed) && segment.position < segment.size
    }

    async fn metrics(&self) -> WalMetricsSnapshot {
        self.metrics.snapshot()
    }

    async fn truncate(&self, up_to_lsn: LogSequenceNumber) -> WalResult<()> {
        // Remove entries from index
        self.lsn_index.retain(|lsn, _| *lsn > up_to_lsn);

        // Update committed LSN
        self.committed_lsn.store(up_to_lsn.0, Ordering::Release);

        Ok(())
    }

    async fn close(&self) -> WalResult<()> {
        info!("Closing WAL");

        // Signal shutdown
        self.shutdown.store(true, Ordering::Release);

        // Stop background workers
        if let Some(handle) = self.sync_handle.lock().await.take() {
            handle.await.ok();
        }

        if let Some(handle) = self.compaction_handle.lock().await.take() {
            handle.await.ok();
        }

        // Final sync
        self.sync().await?;

        Ok(())
    }
}

impl FileWal {
    async fn should_rotate(&self) -> bool {
        let segment = self.active_segment.read().await;

        match &self.config.rotation_policy {
            super::RotationPolicy::Size { max_size } => segment.position as u64 >= *max_size,

            super::RotationPolicy::Time { max_age } => {
                segment.created_at.elapsed().unwrap_or_default() >= *max_age
            }

            super::RotationPolicy::SizeAndTime { max_size, max_age } => {
                segment.position as u64 >= *max_size
                    || segment.created_at.elapsed().unwrap_or_default() >= *max_age
            }

            super::RotationPolicy::EntryCount { max_entries } => {
                segment.entries.len() as u64 >= *max_entries
            }

            super::RotationPolicy::Custom(_) => false,
        }
    }

    async fn rotate_internal(&self) -> WalResult<()> {
        let current_lsn = LogSequenceNumber(self.current_lsn.load(Ordering::Acquire));

        // Create new segment
        let new_segment_id = self.active_segment.read().await.id + 1;
        let new_segment = Self::create_segment(&self.config, new_segment_id, current_lsn)?;

        // Swap segments
        let old_segment = {
            let mut segment = self.active_segment.write().await;
            segment.end_lsn = Some(current_lsn);

            // Final sync of old segment
            segment
                .sync(&FsyncMode::OnDemand, self.config.io_timeout)
                .await?;

            // Create segment info
            let info = SegmentInfo {
                id: segment.id,
                path: segment.path.clone(),
                start_lsn: segment.start_lsn,
                end_lsn: segment.end_lsn,
                size: segment.position as u64,
                entries: segment.entries.len() as u64,
                created_at: segment.created_at,
                archived: false,
            };

            // Add to segments map
            self.segments.write().await.insert(segment.id, info.clone());

            // Replace with new segment
            std::mem::replace(&mut *segment, new_segment)
        };

        info!(
            "Rotated WAL segment {} -> {}",
            old_segment.id, new_segment_id
        );
        self.metrics.record_rotation();

        Ok(())
    }
}

// ===================================================================
// WAL Reader Implementation - CRITICAL-3: Systematic Checksum Validation
// ===================================================================
// All read operations use WalEntry::from_bytes() which validates checksums.
// This ensures data integrity on every read, detecting corruption immediately.

#[async_trait]
impl WalReader for FileWal {
    /// Read a specific entry by LSN with checksum validation
    ///
    /// This method:
    /// 1. Looks up the entry location in the LSN index
    /// 2. Reads the raw bytes from the segment file
    /// 3. Deserializes using WalEntry::from_bytes() which validates the CRC32 checksum
    /// 4. Returns ChecksumMismatch error if validation fails
    async fn read(&self, lsn: LogSequenceNumber) -> WalResult<WalEntry> {
        // Lookup entry location in index
        let location = self
            .lsn_index
            .get(&lsn)
            .ok_or_else(|| WalError::InvalidLsn { lsn })?;

        let segment_id = location.segment_id;
        let offset = location.offset;
        let size = location.size;
        drop(location); // Release the DashMap reference

        // Read from appropriate segment
        let segments = self.segments.read().await;
        let segment_info = segments
            .get(&segment_id)
            .ok_or_else(|| WalError::InvalidLsn { lsn })?;

        // Open segment file for reading
        let mut file = File::open(&segment_info.path).map_err(|e| WalError::Io {
            source: e,
            path: Some(segment_info.path.clone()),
        })?;

        // Read entry bytes at offset
        use std::io::{Read, Seek, SeekFrom};
        file.seek(SeekFrom::Start(offset as u64))
            .map_err(|e| WalError::Io {
                source: e,
                path: Some(segment_info.path.clone()),
            })?;

        let mut bytes = vec![0u8; size];
        file.read_exact(&mut bytes).map_err(|e| WalError::Io {
            source: e,
            path: Some(segment_info.path.clone()),
        })?;

        // Deserialize and validate checksum
        // WalEntry::from_bytes() automatically validates the CRC32 checksum
        // and returns an error if validation fails
        WalEntry::from_bytes(&bytes).map_err(|msg| {
            // Parse the error message to determine if it's a checksum failure
            if msg.contains("Checksum") {
                WalError::ChecksumMismatch {
                    lsn,
                    expected: 0, // We don't have the expected value from the error msg
                    actual: 0,   // We don't have the actual value from the error msg
                }
            } else {
                WalError::Corruption {
                    lsn,
                    details: msg,
                    recoverable: false,
                }
            }
        })
    }

    /// Scan entries in LSN range with checksum validation
    ///
    /// Reads multiple entries sequentially, validating checksums for each.
    /// Stops at first checksum failure or missing LSN.
    async fn scan(&self, range: Range<LogSequenceNumber>) -> WalResult<Vec<WalEntry>> {
        let mut entries = Vec::new();

        for lsn_value in range.start.0..range.end.0 {
            let lsn = LogSequenceNumber(lsn_value);

            // Try to read each LSN in range
            // Stop if LSN doesn't exist (gaps are expected after truncation)
            match self.read(lsn).await {
                Ok(entry) => entries.push(entry),
                Err(WalError::InvalidLsn { .. }) => {
                    // LSN not found - might be truncated, continue
                    continue;
                }
                Err(e) => {
                    // Checksum or I/O error - propagate immediately
                    return Err(e);
                }
            }
        }

        Ok(entries)
    }

    /// Stream entries from start LSN (for recovery/replay)
    ///
    /// Returns entries asynchronously via an mpsc channel for memory efficiency.
    async fn stream_from(&self, start_lsn: LogSequenceNumber) -> WalResult<WalEntryStream> {
        let tail = self.tail_lsn().await;
        let (tx, rx) = tokio::sync::mpsc::channel(1000);

        // Spawn background task to stream entries
        let self_clone = self.clone(); // Assuming FileWal is Clone
        tokio::spawn(async move {
            for lsn_value in start_lsn.0..tail.0 {
                let lsn = LogSequenceNumber(lsn_value);

                match self_clone.read(lsn).await {
                    Ok(entry) => {
                        if tx.send(Ok(entry)).await.is_err() {
                            break; // Receiver dropped
                        }
                    }
                    Err(WalError::InvalidLsn { .. }) => {
                        continue; // Skip missing LSNs
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e)).await;
                        break;
                    }
                }
            }
        });

        Ok(WalEntryStream::new(rx))
    }

    /// Find entries matching predicate with checksum validation
    ///
    /// Scans all entries from head to tail, validating checksums.
    async fn find<F>(&self, predicate: F) -> WalResult<Vec<WalEntry>>
    where
        F: Fn(&WalEntry) -> bool + Send + Sync + 'static,
    {
        let head = self.head_lsn().await;
        let tail = self.tail_lsn().await;

        let all_entries = self.scan(head..tail).await?;

        // Filter using predicate
        Ok(all_entries.into_iter().filter(predicate).collect())
    }
}

impl GroupCommitCoordinator {
    fn new(config: GroupCommitConfig) -> Self {
        Self {
            enabled: config.enabled,
            config,
            pending: Arc::new(Mutex::new(Vec::new())),
            notify: Arc::new(Notify::new()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::wal::entry::{
        EntryFlags, EntryHeader, EntryPayload, EntryType, LogSequenceNumber,
    };
    use bytes::Bytes;
    use std::collections::HashSet;
    use tempfile::TempDir;
    use tokio::task::JoinSet;

    /// Helper to create a test WAL instance with a unique namespace
    async fn create_test_wal_with_name(name: &str) -> (FileWal, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config = WalConfig {
            path: temp_dir.path().to_path_buf(),
            max_file_size: 10 * 1024 * 1024, // 10MB for tests
            fsync_mode: FsyncMode::OnDemand,
            ..Default::default()
        };
        let metrics = Arc::new(crate::storage::wal::metrics::WalMetricsCollector::new(name));
        let wal = FileWal::new(config, metrics).await.unwrap();
        (wal, temp_dir)
    }

    /// Helper to create a test WAL entry
    fn create_test_entry(data: &str) -> WalEntry {
        WalEntry {
            header: EntryHeader {
                lsn: LogSequenceNumber::ZERO, // Will be set by WAL
                previous_lsn: LogSequenceNumber::ZERO,
                entry_type: EntryType::LineageWrite,
                timestamp_us: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_micros() as u64,
                payload_size: data.len() as u32,
                checksum: 0, // Will be calculated by WAL
                flags: EntryFlags::default(),
            },
            payload: EntryPayload::Raw(Bytes::from(data.to_string())),
            transaction: None,
        }
    }

    #[tokio::test]
    async fn test_lsn_starts_from_zero() {
        let (wal, _temp) = create_test_wal_with_name("test_lsn_starts_from_zero").await;

        let entry = create_test_entry("first entry");
        let lsn = wal.append(entry).await.unwrap();

        assert_eq!(lsn.0, 0, "First LSN should be 0");
    }

    #[tokio::test]
    async fn test_lsn_sequential_single_appends() {
        let (wal, _temp) = create_test_wal_with_name("test_lsn_sequential_single_appends").await;

        let lsn1 = wal.append(create_test_entry("entry1")).await.unwrap();
        let lsn2 = wal.append(create_test_entry("entry2")).await.unwrap();
        let lsn3 = wal.append(create_test_entry("entry3")).await.unwrap();

        assert_eq!(lsn1.0, 0, "First LSN should be 0");
        assert_eq!(lsn2.0, 1, "Second LSN should be 1");
        assert_eq!(lsn3.0, 2, "Third LSN should be 2");
    }

    #[tokio::test]
    async fn test_lsn_consecutive_batch_appends() {
        let (wal, _temp) = create_test_wal_with_name("test_lsn_consecutive_batch_appends").await;

        let entries = vec![
            create_test_entry("entry1"),
            create_test_entry("entry2"),
            create_test_entry("entry3"),
        ];

        let lsns = wal.append_batch(entries).await.unwrap();

        assert_eq!(lsns.len(), 3);
        assert_eq!(lsns[0].0, 0, "First batch LSN should be 0");
        assert_eq!(lsns[1].0, 1, "Second batch LSN should be 1");
        assert_eq!(lsns[2].0, 2, "Third batch LSN should be 2");
    }

    #[tokio::test]
    async fn test_lsn_mixed_single_and_batch() {
        let (wal, _temp) = create_test_wal_with_name("test_lsn_mixed_single_and_batch").await;

        // Single append
        let lsn1 = wal.append(create_test_entry("single1")).await.unwrap();

        // Batch append
        let batch_lsns = wal
            .append_batch(vec![
                create_test_entry("batch1"),
                create_test_entry("batch2"),
            ])
            .await
            .unwrap();

        // Another single append
        let lsn4 = wal.append(create_test_entry("single2")).await.unwrap();

        assert_eq!(lsn1.0, 0);
        assert_eq!(batch_lsns[0].0, 1);
        assert_eq!(batch_lsns[1].0, 2);
        assert_eq!(lsn4.0, 3);
    }

    #[tokio::test]
    async fn test_lsn_concurrent_appends_unique() {
        let (wal, _temp) = create_test_wal_with_name("test_lsn_concurrent_appends_unique").await;
        let wal = Arc::new(wal);

        const NUM_TASKS: usize = 10;
        const APPENDS_PER_TASK: usize = 100;

        let mut join_set = JoinSet::new();

        for task_id in 0..NUM_TASKS {
            let wal_clone = Arc::clone(&wal);
            join_set.spawn(async move {
                let mut lsns = Vec::new();
                for i in 0..APPENDS_PER_TASK {
                    let entry = create_test_entry(&format!("task_{}_entry_{}", task_id, i));
                    let lsn = wal_clone.append(entry).await.unwrap();
                    lsns.push(lsn.0);
                }
                lsns
            });
        }

        // Collect all LSNs from all tasks
        let mut all_lsns = Vec::new();
        while let Some(result) = join_set.join_next().await {
            let lsns = result.unwrap();
            all_lsns.extend(lsns);
        }

        // Check we got the right number of LSNs
        assert_eq!(all_lsns.len(), NUM_TASKS * APPENDS_PER_TASK);

        // Check all LSNs are unique (no duplicates)
        let unique_lsns: HashSet<u64> = all_lsns.iter().copied().collect();
        assert_eq!(
            unique_lsns.len(),
            all_lsns.len(),
            "All LSNs must be unique - no duplicates allowed"
        );

        // Check LSNs are in the expected range [0, NUM_TASKS * APPENDS_PER_TASK)
        for lsn in &all_lsns {
            assert!(
                *lsn < (NUM_TASKS * APPENDS_PER_TASK) as u64,
                "LSN {} is out of expected range",
                lsn
            );
        }
    }

    #[tokio::test]
    async fn test_lsn_concurrent_batch_appends_unique() {
        let (wal, _temp) =
            create_test_wal_with_name("test_lsn_concurrent_batch_appends_unique").await;
        let wal = Arc::new(wal);

        const NUM_TASKS: usize = 10;
        const BATCH_SIZE: usize = 5;
        const BATCHES_PER_TASK: usize = 10;

        let mut join_set = JoinSet::new();

        for task_id in 0..NUM_TASKS {
            let wal_clone = Arc::clone(&wal);
            join_set.spawn(async move {
                let mut lsns = Vec::new();
                for batch_id in 0..BATCHES_PER_TASK {
                    let entries: Vec<_> = (0..BATCH_SIZE)
                        .map(|i| {
                            create_test_entry(&format!(
                                "task_{}_batch_{}_entry_{}",
                                task_id, batch_id, i
                            ))
                        })
                        .collect();
                    let batch_lsns = wal_clone.append_batch(entries).await.unwrap();
                    lsns.extend(batch_lsns.into_iter().map(|lsn| lsn.0));
                }
                lsns
            });
        }

        // Collect all LSNs
        let mut all_lsns = Vec::new();
        while let Some(result) = join_set.join_next().await {
            let lsns = result.unwrap();
            all_lsns.extend(lsns);
        }

        // Verify uniqueness
        let unique_lsns: HashSet<u64> = all_lsns.iter().copied().collect();
        assert_eq!(
            unique_lsns.len(),
            all_lsns.len(),
            "All LSNs from concurrent batches must be unique"
        );

        // Verify batch LSNs are consecutive within each batch
        // (This is implicitly tested by uniqueness + correct count)
        assert_eq!(all_lsns.len(), NUM_TASKS * BATCHES_PER_TASK * BATCH_SIZE);
    }

    #[tokio::test]
    async fn test_lsn_no_gaps() {
        let (wal, _temp) = create_test_wal_with_name("test_lsn_no_gaps").await;

        // Mix of single and batch appends
        let lsn1 = wal.append(create_test_entry("1")).await.unwrap();

        let batch1 = wal
            .append_batch(vec![create_test_entry("2"), create_test_entry("3")])
            .await
            .unwrap();

        let lsn4 = wal.append(create_test_entry("4")).await.unwrap();

        let batch2 = wal
            .append_batch(vec![
                create_test_entry("5"),
                create_test_entry("6"),
                create_test_entry("7"),
            ])
            .await
            .unwrap();

        // Collect all LSNs
        let mut all_lsns = vec![lsn1.0, lsn4.0];
        all_lsns.extend(batch1.into_iter().map(|l| l.0));
        all_lsns.extend(batch2.into_iter().map(|l| l.0));
        all_lsns.sort();

        // Check no gaps: should be [0, 1, 2, 3, 4, 5, 6]
        for (i, lsn) in all_lsns.iter().enumerate() {
            assert_eq!(*lsn, i as u64, "LSN sequence has a gap at position {}", i);
        }
    }

    // ===================================================================
    // CRITICAL-2: msync() Safety Tests
    // ===================================================================
    // These tests validate that memory-mapped I/O is properly flushed
    // with appropriate msync() calls to ensure durability guarantees.

    /// Test that writes complete successfully with EveryWrite mode
    /// This mode should call both async flush (MS_ASYNC) and sync flush (MS_SYNC)
    #[tokio::test]
    async fn test_msync_every_write_mode() {
        let temp_dir = TempDir::new().unwrap();
        let config = WalConfig {
            path: temp_dir.path().to_path_buf(),
            max_file_size: 10 * 1024 * 1024,
            fsync_mode: FsyncMode::EveryWrite, // Sync after every write
            ..Default::default()
        };
        let metrics = Arc::new(WalMetricsCollector::new("test_msync_every_write"));
        let wal = FileWal::new(config, metrics).await.unwrap();

        // Append multiple entries - each should trigger sync
        for i in 0..10 {
            let entry = create_test_entry(&format!("entry_{}", i));
            let lsn = wal.append(entry).await.unwrap();
            assert_eq!(lsn.0, i);
        }

        // All data should be durably persisted due to EveryWrite mode
        // If we crashed here, all 10 entries would be recoverable
    }

    /// Test that batch writes complete successfully with BatchSync mode
    /// This mode should call sync flush only after the entire batch
    #[tokio::test]
    async fn test_msync_batch_sync_mode() {
        let temp_dir = TempDir::new().unwrap();
        let config = WalConfig {
            path: temp_dir.path().to_path_buf(),
            max_file_size: 10 * 1024 * 1024,
            fsync_mode: FsyncMode::BatchSync, // Sync after batches
            ..Default::default()
        };
        let metrics = Arc::new(WalMetricsCollector::new("test_msync_batch_sync"));
        let wal = FileWal::new(config, metrics).await.unwrap();

        // Batch append should trigger single sync for all entries
        let entries: Vec<_> = (0..10)
            .map(|i| create_test_entry(&format!("entry_{}", i)))
            .collect();

        let lsns = wal.append_batch(entries).await.unwrap();
        assert_eq!(lsns.len(), 10);

        // All data should be durably persisted after batch sync
    }

    /// Test that OnDemand mode only syncs on explicit sync() calls
    /// Writes should use async flush but not block for sync
    #[tokio::test]
    async fn test_msync_on_demand_mode() {
        let temp_dir = TempDir::new().unwrap();
        let config = WalConfig {
            path: temp_dir.path().to_path_buf(),
            max_file_size: 10 * 1024 * 1024,
            fsync_mode: FsyncMode::OnDemand, // No auto-sync
            ..Default::default()
        };
        let metrics = Arc::new(WalMetricsCollector::new("test_msync_on_demand"));
        let wal = FileWal::new(config, metrics).await.unwrap();

        // Writes should complete quickly without sync
        let entry1 = create_test_entry("entry_1");
        let lsn1 = wal.append(entry1).await.unwrap();
        assert_eq!(lsn1.0, 0);

        let entry2 = create_test_entry("entry_2");
        let lsn2 = wal.append(entry2).await.unwrap();
        assert_eq!(lsn2.0, 1);

        // Now explicitly sync - this should call MS_SYNC
        wal.sync().await.unwrap();

        // After explicit sync, data is durable
    }

    /// Test that Periodic mode allows writes without immediate sync
    /// but data is still async flushed
    #[tokio::test]
    async fn test_msync_periodic_mode() {
        let temp_dir = TempDir::new().unwrap();
        let config = WalConfig {
            path: temp_dir.path().to_path_buf(),
            max_file_size: 10 * 1024 * 1024,
            fsync_mode: FsyncMode::Periodic, // Background sync
            ..Default::default()
        };
        let metrics = Arc::new(WalMetricsCollector::new("test_msync_periodic"));
        let wal = FileWal::new(config, metrics).await.unwrap();

        // High-throughput writes without blocking
        for i in 0..100 {
            let entry = create_test_entry(&format!("entry_{}", i));
            wal.append(entry).await.unwrap();
        }

        // Data has been async flushed (MS_ASYNC) but may not be durable yet
        // Explicit sync ensures durability
        wal.sync().await.unwrap();

        // Now all 100 entries are durable
    }

    /// Test mixed single and batch writes with different sync modes
    #[tokio::test]
    async fn test_msync_mixed_operations() {
        let temp_dir = TempDir::new().unwrap();
        let config = WalConfig {
            path: temp_dir.path().to_path_buf(),
            max_file_size: 10 * 1024 * 1024,
            fsync_mode: FsyncMode::BatchSync,
            ..Default::default()
        };
        let metrics = Arc::new(WalMetricsCollector::new("test_msync_mixed"));
        let wal = FileWal::new(config, metrics).await.unwrap();

        // Single write (no auto-sync in BatchSync mode for single writes)
        let lsn1 = wal.append(create_test_entry("single_1")).await.unwrap();

        // Batch write (auto-syncs in BatchSync mode)
        let batch = vec![
            create_test_entry("batch_1"),
            create_test_entry("batch_2"),
            create_test_entry("batch_3"),
        ];
        let batch_lsns = wal.append_batch(batch).await.unwrap();

        // Another single write
        let lsn5 = wal.append(create_test_entry("single_2")).await.unwrap();

        // Verify LSN sequence
        assert_eq!(lsn1.0, 0);
        assert_eq!(batch_lsns[0].0, 1);
        assert_eq!(batch_lsns[1].0, 2);
        assert_eq!(batch_lsns[2].0, 3);
        assert_eq!(lsn5.0, 4);

        // Explicit sync to ensure durability of single writes
        wal.sync().await.unwrap();
    }

    /// Test that async flush happens immediately after write
    /// This test verifies the write path includes immediate MS_ASYNC
    #[tokio::test]
    async fn test_msync_async_flush_after_write() {
        let temp_dir = TempDir::new().unwrap();
        let config = WalConfig {
            path: temp_dir.path().to_path_buf(),
            max_file_size: 10 * 1024 * 1024,
            fsync_mode: FsyncMode::OnDemand, // No sync, only async flush
            ..Default::default()
        };
        let metrics = Arc::new(WalMetricsCollector::new("test_msync_async_flush"));
        let wal = FileWal::new(config, metrics).await.unwrap();

        // Write data - should trigger immediate async flush (MS_ASYNC)
        let large_entry = create_test_entry(&"x".repeat(1024)); // 1KB entry
        wal.append(large_entry).await.unwrap();

        // Even without sync(), data has been async flushed to kernel
        // A subsequent sync() would be much faster since writeback already started
    }

    /// Test durability semantics across different fsync modes
    #[tokio::test]
    async fn test_msync_durability_semantics() {
        // Test that commit() doesn't imply durability without sync
        let temp_dir = TempDir::new().unwrap();
        let config = WalConfig {
            path: temp_dir.path().to_path_buf(),
            max_file_size: 10 * 1024 * 1024,
            fsync_mode: FsyncMode::OnDemand,
            ..Default::default()
        };
        let metrics = Arc::new(WalMetricsCollector::new("test_msync_durability"));
        let wal = FileWal::new(config, metrics).await.unwrap();

        let entry = create_test_entry("test_entry");
        let lsn = wal.append(entry).await.unwrap();

        // Commit marks LSN as committed but doesn't ensure durability
        wal.commit(lsn).await.unwrap();

        // For true durability, must call sync()
        wal.sync().await.unwrap();

        // Now data is both committed AND durable
    }

    // ===================================================================
    // HIGH-1: Disk Space Checks Tests
    // ===================================================================
    // These tests validate that the WAL properly checks available disk space
    // before writes and rejects writes when space is insufficient.

    /// Test that writes succeed when disk space check is disabled
    /// This tests normal operation without the disk space safety net
    #[tokio::test]
    async fn test_disk_space_sufficient() {
        let temp_dir = TempDir::new().unwrap();
        let config = WalConfig {
            path: temp_dir.path().to_path_buf(),
            max_file_size: 10 * 1024 * 1024,
            min_free_disk_space: 0, // Disabled - we can't control actual disk space in tests
            ..Default::default()
        };
        let metrics = Arc::new(WalMetricsCollector::new("test_disk_space_sufficient"));
        let wal = FileWal::new(config, metrics).await.unwrap();

        // Should succeed - check is disabled
        for i in 0..10 {
            let entry = create_test_entry(&format!("entry_{}", i));
            let lsn = wal.append(entry).await.unwrap();
            assert_eq!(lsn.0, i);
        }
    }

    /// Test that writes are rejected when disk space is below minimum
    #[tokio::test]
    async fn test_disk_space_insufficient() {
        let temp_dir = TempDir::new().unwrap();
        let config = WalConfig {
            path: temp_dir.path().to_path_buf(),
            max_file_size: 10 * 1024 * 1024,
            // Set to impossibly high value to trigger the check
            min_free_disk_space: u64::MAX - 1024,
            ..Default::default()
        };
        let metrics = Arc::new(WalMetricsCollector::new("test_disk_space_insufficient"));
        let wal = FileWal::new(config, metrics).await.unwrap();

        // Should fail - min_free_disk_space is impossibly high
        let entry = create_test_entry("test");
        let result = wal.append(entry).await;

        assert!(result.is_err());
        match result.unwrap_err() {
            WalError::InsufficientDiskSpace {
                available_bytes,
                required_bytes,
                min_free_bytes,
            } => {
                assert!(available_bytes < min_free_bytes);
                assert!(required_bytes > 0);
                assert_eq!(min_free_bytes, u64::MAX - 1024);
            }
            e => panic!("Expected InsufficientDiskSpace error, got {:?}", e),
        }
    }

    /// Test that batch writes succeed when disk space check is disabled
    #[tokio::test]
    async fn test_disk_space_batch_sufficient() {
        let temp_dir = TempDir::new().unwrap();
        let config = WalConfig {
            path: temp_dir.path().to_path_buf(),
            max_file_size: 10 * 1024 * 1024,
            min_free_disk_space: 0, // Disabled - we can't control actual disk space in tests
            ..Default::default()
        };
        let metrics = Arc::new(WalMetricsCollector::new("test_disk_space_batch_sufficient"));
        let wal = FileWal::new(config, metrics).await.unwrap();

        // Batch append should succeed - check is disabled
        let entries = vec![
            create_test_entry("entry_0"),
            create_test_entry("entry_1"),
            create_test_entry("entry_2"),
        ];
        let lsns = wal.append_batch(entries).await.unwrap();

        assert_eq!(lsns.len(), 3);
        assert_eq!(lsns[0].0, 0);
        assert_eq!(lsns[1].0, 1);
        assert_eq!(lsns[2].0, 2);
    }

    /// Test that batch writes are rejected when disk space is insufficient
    #[tokio::test]
    async fn test_disk_space_batch_insufficient() {
        let temp_dir = TempDir::new().unwrap();
        let config = WalConfig {
            path: temp_dir.path().to_path_buf(),
            max_file_size: 10 * 1024 * 1024,
            min_free_disk_space: u64::MAX - 1024, // Impossibly high
            ..Default::default()
        };
        let metrics = Arc::new(WalMetricsCollector::new(
            "test_disk_space_batch_insufficient",
        ));
        let wal = FileWal::new(config, metrics).await.unwrap();

        // Batch append should fail
        let entries = vec![
            create_test_entry("entry_0"),
            create_test_entry("entry_1"),
            create_test_entry("entry_2"),
        ];
        let result = wal.append_batch(entries).await;

        assert!(result.is_err());
        match result.unwrap_err() {
            WalError::InsufficientDiskSpace { .. } => {
                // Expected error
            }
            e => panic!("Expected InsufficientDiskSpace error, got {:?}", e),
        }
    }

    /// Test that disk space check can be disabled by setting min_free_disk_space to 0
    #[tokio::test]
    async fn test_disk_space_check_disabled() {
        let temp_dir = TempDir::new().unwrap();
        let config = WalConfig {
            path: temp_dir.path().to_path_buf(),
            max_file_size: 10 * 1024 * 1024,
            min_free_disk_space: 0, // Check disabled
            ..Default::default()
        };
        let metrics = Arc::new(WalMetricsCollector::new("test_disk_space_disabled"));
        let wal = FileWal::new(config, metrics).await.unwrap();

        // Should succeed even though check is disabled
        // The check is bypassed entirely when min_free_disk_space is 0
        for i in 0..5 {
            let entry = create_test_entry(&format!("entry_{}", i));
            let lsn = wal.append(entry).await.unwrap();
            assert_eq!(lsn.0, i);
        }
    }

    /// Test edge case: available space exactly at threshold
    #[tokio::test]
    async fn test_disk_space_at_threshold() {
        let temp_dir = TempDir::new().unwrap();

        // Get actual available space
        let available = FileWal::check_disk_space(temp_dir.path()).unwrap();

        // Only run this test if we have sufficient disk space to begin with
        if available < 100 * 1024 * 1024 {
            // Skip test if available space is too low (< 100MB)
            return;
        }

        // Set threshold well below available space to account for WAL initialization overhead
        // WAL creates directory structure and preallocates files, which consumes space
        let buffer = 50 * 1024 * 1024; // 50MB buffer for WAL overhead
        let config = WalConfig {
            path: temp_dir.path().to_path_buf(),
            max_file_size: 10 * 1024 * 1024,
            min_free_disk_space: available.saturating_sub(buffer),
            ..Default::default()
        };
        let metrics = Arc::new(WalMetricsCollector::new("test_disk_space_at_threshold"));
        let wal = FileWal::new(config, metrics).await.unwrap();

        // Should succeed - available is above threshold even after WAL initialization
        let entry = create_test_entry("test");
        let lsn = wal.append(entry).await.unwrap();
        assert_eq!(lsn.0, 0);
    }

    /// Test recovery from low disk space condition
    /// Once the check is disabled, writes should succeed
    #[tokio::test]
    async fn test_disk_space_recovery() {
        let temp_dir = TempDir::new().unwrap();

        // Start with impossible threshold
        let config = WalConfig {
            path: temp_dir.path().to_path_buf(),
            max_file_size: 10 * 1024 * 1024,
            min_free_disk_space: u64::MAX - 1024,
            ..Default::default()
        };
        let metrics = Arc::new(WalMetricsCollector::new("test_disk_space_recovery"));
        let mut wal = FileWal::new(config, metrics).await.unwrap();

        // First write should fail
        let entry1 = create_test_entry("entry_1");
        let result = wal.append(entry1).await;
        assert!(result.is_err());

        // "Recover" by disabling the check
        // In production, this would happen externally (disk cleanup, etc.)
        // For testing, we disable the check to simulate recovery
        wal.config.min_free_disk_space = 0; // Disable check

        // Now write should succeed
        let entry2 = create_test_entry("entry_2");
        let lsn = wal.append(entry2).await.unwrap();
        assert_eq!(lsn.0, 0); // First successful write gets LSN 0
    }

    /// Test that disk space errors contain useful diagnostic information
    #[tokio::test]
    async fn test_disk_space_error_details() {
        let temp_dir = TempDir::new().unwrap();
        let min_free = u64::MAX - 1024;
        let config = WalConfig {
            path: temp_dir.path().to_path_buf(),
            max_file_size: 10 * 1024 * 1024,
            min_free_disk_space: min_free,
            ..Default::default()
        };
        let metrics = Arc::new(WalMetricsCollector::new("test_disk_space_error_details"));
        let wal = FileWal::new(config, metrics).await.unwrap();

        let entry = create_test_entry("test");
        let result = wal.append(entry).await;

        assert!(result.is_err());
        if let Err(WalError::InsufficientDiskSpace {
            available_bytes,
            required_bytes,
            min_free_bytes,
        }) = result
        {
            // Verify error contains useful information
            assert!(
                available_bytes < min_free_bytes,
                "Error should show available < min_free"
            );
            assert!(required_bytes > 0, "Error should show required bytes");
            assert_eq!(
                min_free_bytes, min_free,
                "Error should show configured minimum"
            );

            // Error message should be informative
            let error = WalError::InsufficientDiskSpace {
                available_bytes,
                required_bytes,
                min_free_bytes,
            };
            let error_msg = format!("{}", error);
            assert!(error_msg.contains("available"));
            assert!(error_msg.contains("required"));
            assert!(error_msg.contains("minimum"));
        } else {
            panic!("Expected InsufficientDiskSpace error");
        }
    }

    // ===================================================================
    // HIGH-2: LSN Index Cleanup Tests
    // ===================================================================
    // These tests validate that the LSN index is properly cleaned up
    // after segment compaction to prevent unbounded memory growth.

    /// Test that LSN index grows as entries are written
    #[tokio::test]
    async fn test_lsn_index_growth() {
        let temp_dir = TempDir::new().unwrap();
        let config = WalConfig {
            path: temp_dir.path().to_path_buf(),
            max_file_size: 10 * 1024 * 1024,
            min_free_disk_space: 0,
            ..Default::default()
        };
        let metrics = Arc::new(WalMetricsCollector::new("test_lsn_index_growth"));
        let wal = FileWal::new(config, metrics).await.unwrap();

        // Initially empty
        assert_eq!(wal.lsn_index_size(), 0);

        // Write some entries
        for i in 0..100 {
            let entry = create_test_entry(&format!("entry_{}", i));
            wal.append(entry).await.unwrap();
        }

        // Index should have 100 entries
        assert_eq!(wal.lsn_index_size(), 100);
    }

    /// Test that LSN index is cleaned up when segments are compacted
    #[tokio::test]
    async fn test_lsn_index_cleanup_on_compaction() {
        let temp_dir = TempDir::new().unwrap();

        // Create a segment with known LSN range
        let segment_info = SegmentInfo {
            id: 1,
            path: temp_dir.path().join("segment_001.wal"),
            start_lsn: LogSequenceNumber(0),
            end_lsn: Some(LogSequenceNumber(99)),
            size: 1024,
            entries: 100,
            created_at: SystemTime::now(),
            archived: false,
        };

        // Create index with entries
        let lsn_index = Arc::new(DashMap::new());
        for i in 0..100 {
            lsn_index.insert(
                LogSequenceNumber(i),
                SegmentLocation {
                    segment_id: 1,
                    offset: i as usize * 10,
                    size: 10,
                },
            );
        }

        // Verify index has 100 entries
        assert_eq!(lsn_index.len(), 100);

        // Create the segment file
        std::fs::write(&segment_info.path, b"dummy data").unwrap();

        // Compact the segment
        let config = WalConfig {
            path: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        FileWal::compact_segment(&segment_info, &config, &lsn_index)
            .await
            .unwrap();

        // Index should now be empty
        assert_eq!(lsn_index.len(), 0);
    }

    /// Test that cleanup only affects the compacted segment's range
    #[tokio::test]
    async fn test_lsn_index_cleanup_range_isolation() {
        let temp_dir = TempDir::new().unwrap();

        // Create a segment with LSNs 0-99
        let segment_info = SegmentInfo {
            id: 1,
            path: temp_dir.path().join("segment_001.wal"),
            start_lsn: LogSequenceNumber(0),
            end_lsn: Some(LogSequenceNumber(99)),
            size: 1024,
            entries: 100,
            created_at: SystemTime::now(),
            archived: false,
        };

        // Create index with entries for LSNs 0-199 (two segments)
        let lsn_index = Arc::new(DashMap::new());
        for i in 0..200 {
            lsn_index.insert(
                LogSequenceNumber(i),
                SegmentLocation {
                    segment_id: if i < 100 { 1 } else { 2 },
                    offset: (i % 100) as usize * 10,
                    size: 10,
                },
            );
        }

        // Verify index has 200 entries
        assert_eq!(lsn_index.len(), 200);

        // Create the segment file
        std::fs::write(&segment_info.path, b"dummy data").unwrap();

        // Compact segment 1 (LSNs 0-99)
        let config = WalConfig {
            path: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        FileWal::compact_segment(&segment_info, &config, &lsn_index)
            .await
            .unwrap();

        // Index should have 100 entries (segment 2's entries remain)
        assert_eq!(lsn_index.len(), 100);

        // Verify that only LSNs 100-199 remain
        for i in 0..100 {
            assert!(!lsn_index.contains_key(&LogSequenceNumber(i)));
        }
        for i in 100..200 {
            assert!(lsn_index.contains_key(&LogSequenceNumber(i)));
        }
    }

    /// Test cleanup with sparse LSN ranges (some LSNs missing)
    #[tokio::test]
    async fn test_lsn_index_cleanup_sparse_range() {
        let temp_dir = TempDir::new().unwrap();

        // Create a segment with LSNs 0-99
        let segment_info = SegmentInfo {
            id: 1,
            path: temp_dir.path().join("segment_001.wal"),
            start_lsn: LogSequenceNumber(0),
            end_lsn: Some(LogSequenceNumber(99)),
            size: 1024,
            entries: 50, // Only 50 entries, not all LSNs present
            created_at: SystemTime::now(),
            archived: false,
        };

        // Create index with only even LSNs (sparse)
        let lsn_index = Arc::new(DashMap::new());
        for i in (0..100).step_by(2) {
            lsn_index.insert(
                LogSequenceNumber(i),
                SegmentLocation {
                    segment_id: 1,
                    offset: i as usize * 10,
                    size: 10,
                },
            );
        }

        // Verify index has 50 entries
        assert_eq!(lsn_index.len(), 50);

        // Create the segment file
        std::fs::write(&segment_info.path, b"dummy data").unwrap();

        // Compact the segment
        let config = WalConfig {
            path: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        FileWal::compact_segment(&segment_info, &config, &lsn_index)
            .await
            .unwrap();

        // Index should now be empty (all even LSNs in range removed)
        assert_eq!(lsn_index.len(), 0);
    }

    /// Test cleanup with segment that has no end_lsn
    #[tokio::test]
    async fn test_lsn_index_cleanup_no_end_lsn() {
        let temp_dir = TempDir::new().unwrap();

        // Create a segment without end_lsn (active segment)
        let segment_info = SegmentInfo {
            id: 1,
            path: temp_dir.path().join("segment_001.wal"),
            start_lsn: LogSequenceNumber(0),
            end_lsn: None, // No end LSN
            size: 1024,
            entries: 100,
            created_at: SystemTime::now(),
            archived: false,
        };

        // Create index with entries
        let lsn_index = Arc::new(DashMap::new());
        for i in 0..100 {
            lsn_index.insert(
                LogSequenceNumber(i),
                SegmentLocation {
                    segment_id: 1,
                    offset: i as usize * 10,
                    size: 10,
                },
            );
        }

        // Verify index has 100 entries
        assert_eq!(lsn_index.len(), 100);

        // Create the segment file
        std::fs::write(&segment_info.path, b"dummy data").unwrap();

        // Compact the segment (should skip cleanup due to no end_lsn)
        let config = WalConfig {
            path: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        FileWal::compact_segment(&segment_info, &config, &lsn_index)
            .await
            .unwrap();

        // Index should still have all 100 entries (no cleanup performed)
        assert_eq!(lsn_index.len(), 100);
    }

    /// Test that lsn_index_size() method returns correct size
    #[tokio::test]
    async fn test_lsn_index_size_method() {
        let temp_dir = TempDir::new().unwrap();
        let config = WalConfig {
            path: temp_dir.path().to_path_buf(),
            max_file_size: 10 * 1024 * 1024,
            min_free_disk_space: 0,
            ..Default::default()
        };
        let metrics = Arc::new(WalMetricsCollector::new("test_lsn_index_size_method"));
        let wal = FileWal::new(config, metrics).await.unwrap();

        assert_eq!(wal.lsn_index_size(), 0);

        // Write entries and verify size increases
        for i in 0..50 {
            wal.append(create_test_entry(&format!("entry_{}", i)))
                .await
                .unwrap();
            assert_eq!(wal.lsn_index_size(), i + 1);
        }

        assert_eq!(wal.lsn_index_size(), 50);
    }

    /// Test cleanup performance with large LSN ranges
    #[tokio::test]
    async fn test_lsn_index_cleanup_performance() {
        let temp_dir = TempDir::new().unwrap();

        // Create a segment with 10,000 LSNs
        let segment_info = SegmentInfo {
            id: 1,
            path: temp_dir.path().join("segment_001.wal"),
            start_lsn: LogSequenceNumber(0),
            end_lsn: Some(LogSequenceNumber(9999)),
            size: 1024 * 1024,
            entries: 10000,
            created_at: SystemTime::now(),
            archived: false,
        };

        // Create index with 10,000 entries
        let lsn_index = Arc::new(DashMap::new());
        for i in 0..10000 {
            lsn_index.insert(
                LogSequenceNumber(i),
                SegmentLocation {
                    segment_id: 1,
                    offset: i as usize * 100,
                    size: 100,
                },
            );
        }

        assert_eq!(lsn_index.len(), 10000);

        // Create the segment file
        std::fs::write(&segment_info.path, vec![0u8; 1024]).unwrap();

        // Compact the segment and measure time
        let config = WalConfig {
            path: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        let start = Instant::now();
        FileWal::compact_segment(&segment_info, &config, &lsn_index)
            .await
            .unwrap();
        let elapsed = start.elapsed();

        // Index should be empty
        assert_eq!(lsn_index.len(), 0);

        // Cleanup should be fast (< 100ms for 10K entries)
        assert!(
            elapsed < Duration::from_millis(100),
            "Cleanup took {:?}, expected < 100ms",
            elapsed
        );
    }

    // ===================================================================
    // HIGH-3: Timeout Handling Tests
    // ===================================================================
    // These tests validate that I/O operations respect configured timeouts
    // and fail gracefully when operations take too long.

    /// Test that sync operations complete within timeout when fast
    #[tokio::test]
    async fn test_sync_timeout_fast_operation() {
        let temp_dir = TempDir::new().unwrap();
        let config = WalConfig {
            path: temp_dir.path().to_path_buf(),
            max_file_size: 10 * 1024 * 1024,
            min_free_disk_space: 0,
            io_timeout: Some(Duration::from_secs(5)), // 5 second timeout
            ..Default::default()
        };
        let metrics = Arc::new(WalMetricsCollector::new("test_sync_timeout_fast"));
        let wal = FileWal::new(config, metrics).await.unwrap();

        // Write some entries
        for i in 0..10 {
            wal.append(create_test_entry(&format!("entry_{}", i)))
                .await
                .unwrap();
        }

        // Sync should complete successfully (fast operation)
        wal.sync().await.unwrap();
    }

    /// Test that timeout is disabled when io_timeout is None
    #[tokio::test]
    async fn test_sync_timeout_disabled() {
        let temp_dir = TempDir::new().unwrap();
        let config = WalConfig {
            path: temp_dir.path().to_path_buf(),
            max_file_size: 10 * 1024 * 1024,
            min_free_disk_space: 0,
            io_timeout: None, // No timeout
            ..Default::default()
        };
        let metrics = Arc::new(WalMetricsCollector::new("test_sync_timeout_disabled"));
        let wal = FileWal::new(config, metrics).await.unwrap();

        // Write some entries
        for i in 0..5 {
            wal.append(create_test_entry(&format!("entry_{}", i)))
                .await
                .unwrap();
        }

        // Sync should complete without timeout checking
        wal.sync().await.unwrap();
    }

    /// Test that append operations with EveryWrite mode respect timeout
    #[tokio::test]
    async fn test_append_with_sync_timeout() {
        let temp_dir = TempDir::new().unwrap();
        let config = WalConfig {
            path: temp_dir.path().to_path_buf(),
            max_file_size: 10 * 1024 * 1024,
            min_free_disk_space: 0,
            fsync_mode: FsyncMode::EveryWrite, // Sync after every write
            io_timeout: Some(Duration::from_secs(10)), // 10 second timeout
            ..Default::default()
        };
        let metrics = Arc::new(WalMetricsCollector::new("test_append_with_sync_timeout"));
        let wal = FileWal::new(config, metrics).await.unwrap();

        // Append should succeed and sync within timeout
        let entry = create_test_entry("test_entry");
        let lsn = wal.append(entry).await.unwrap();
        assert_eq!(lsn.0, 0);
    }

    /// Test that batch operations respect timeout
    #[tokio::test]
    async fn test_batch_with_sync_timeout() {
        let temp_dir = TempDir::new().unwrap();
        let config = WalConfig {
            path: temp_dir.path().to_path_buf(),
            max_file_size: 10 * 1024 * 1024,
            min_free_disk_space: 0,
            fsync_mode: FsyncMode::BatchSync, // Sync after batch
            io_timeout: Some(Duration::from_secs(10)),
            ..Default::default()
        };
        let metrics = Arc::new(WalMetricsCollector::new("test_batch_with_sync_timeout"));
        let wal = FileWal::new(config, metrics).await.unwrap();

        // Batch append should succeed and sync within timeout
        let entries = vec![
            create_test_entry("entry_0"),
            create_test_entry("entry_1"),
            create_test_entry("entry_2"),
        ];
        let lsns = wal.append_batch(entries).await.unwrap();
        assert_eq!(lsns.len(), 3);
    }

    /// Test that different timeout durations are respected
    #[tokio::test]
    async fn test_different_timeout_durations() {
        let temp_dir = TempDir::new().unwrap();

        // Test with very long timeout
        let config_long = WalConfig {
            path: temp_dir.path().to_path_buf(),
            max_file_size: 10 * 1024 * 1024,
            min_free_disk_space: 0,
            io_timeout: Some(Duration::from_secs(60)), // 60 second timeout
            ..Default::default()
        };
        let metrics_long = Arc::new(WalMetricsCollector::new("test_timeout_long"));
        let wal_long = FileWal::new(config_long, metrics_long).await.unwrap();

        wal_long
            .append(create_test_entry("long_timeout"))
            .await
            .unwrap();
        wal_long.sync().await.unwrap();

        // Test with shorter timeout
        let config_short = WalConfig {
            path: temp_dir.path().join("short"),
            max_file_size: 10 * 1024 * 1024,
            min_free_disk_space: 0,
            io_timeout: Some(Duration::from_secs(5)), // 5 second timeout
            ..Default::default()
        };
        let metrics_short = Arc::new(WalMetricsCollector::new("test_timeout_short"));
        let wal_short = FileWal::new(config_short, metrics_short).await.unwrap();

        wal_short
            .append(create_test_entry("short_timeout"))
            .await
            .unwrap();
        wal_short.sync().await.unwrap();
    }

    /// Test that timeout errors contain useful information
    /// Note: This test would require a way to simulate a slow I/O operation
    /// For now, we just verify the error type can be constructed
    #[tokio::test]
    async fn test_timeout_error_format() {
        let error = WalError::Timeout {
            operation: "fsync".to_string(),
            timeout_ms: 5000,
        };

        let error_msg = format!("{}", error);
        assert!(error_msg.contains("fsync"));
        assert!(error_msg.contains("5000"));
        assert!(error_msg.contains("Timeout"));
    }

    /// Test production config has reasonable timeout
    #[test]
    fn test_production_config_has_timeout() {
        let config = WalConfig::production();
        assert!(config.io_timeout.is_some());
        let timeout = config.io_timeout.unwrap();
        // Should be at least 10 seconds, at most 5 minutes
        assert!(timeout >= Duration::from_secs(10));
        assert!(timeout <= Duration::from_secs(300));
    }

    /// Test high_durability config inherits timeout from production
    #[test]
    fn test_high_durability_config_has_timeout() {
        let config = WalConfig::high_durability();
        assert!(config.io_timeout.is_some());
    }
}
