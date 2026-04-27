// WAL Recovery Manager with corruption detection and repair
//
// Handles crash recovery, validation, and repair of corrupted WAL segments

use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::Instant;
use tracing::{debug, info, warn};

use super::{
    CorruptionTolerance, EntryType, LogSequenceNumber, RecoveryError, RecoveryMode, RecoveryResult,
    RepairReport, TransactionOp, ValidationReport, WalConfig, WalEntry, WalError, WalResult,
};

/// Manages WAL recovery after crashes
pub struct RecoveryManager {
    config: WalConfig,
    wal_path: PathBuf,
}

/// Strategy for recovery execution
#[async_trait::async_trait]
pub trait RecoveryStrategy: Send + Sync {
    /// Perform recovery
    async fn recover(&self, segments: Vec<SegmentFile>) -> WalResult<RecoveryResult>;

    /// Validate segments
    async fn validate(&self, segments: Vec<SegmentFile>) -> WalResult<ValidationReport>;

    /// Repair corrupted segments
    async fn repair(&self, segments: Vec<SegmentFile>) -> WalResult<RepairReport>;
}

/// Default recovery strategy
pub struct DefaultRecoveryStrategy {
    mode: RecoveryMode,
    corruption_tolerance: CorruptionTolerance,
}

/// Recovery report with detailed information
#[derive(Debug)]
pub struct RecoveryReport {
    pub result: RecoveryResult,
    pub validation: ValidationReport,
    pub repair: Option<RepairReport>,
    pub checkpoint_restored: Option<LogSequenceNumber>,
    pub transactions_recovered: Vec<TransactionRecovery>,
}

#[derive(Debug, Clone)]
pub struct TransactionRecovery {
    pub tx_id: u64,
    pub status: TransactionRecoveryStatus,
    pub entries: Vec<LogSequenceNumber>,
    pub decision: RecoveryDecision,
}

#[derive(Debug, Clone)]
pub enum TransactionRecoveryStatus {
    Active,    // Transaction was active at crash
    Prepared,  // Transaction was prepared but not committed
    Committed, // Transaction was committed
    Aborted,   // Transaction was aborted
}

#[derive(Debug, Clone)]
pub enum RecoveryDecision {
    Commit, // Complete the commit
    Abort,  // Abort the transaction
    Replay, // Replay the entries
    Skip,   // Skip (already processed)
}

/// Represents a WAL segment file for recovery
#[derive(Clone)]
pub struct SegmentFile {
    path: PathBuf,
    id: u64,
    size: u64,
    created: std::time::SystemTime,
}

impl RecoveryManager {
    pub fn new(config: WalConfig) -> Self {
        Self {
            wal_path: config.path.clone(),
            config,
        }
    }

    /// Perform full recovery
    pub async fn recover(&self) -> WalResult<RecoveryReport> {
        let start = Instant::now();
        info!("Starting WAL recovery from {:?}", self.wal_path);

        // Find all segment files
        let segments = self.find_segments()?;
        info!("Found {} WAL segments", segments.len());

        // Create recovery strategy
        let strategy = DefaultRecoveryStrategy {
            mode: self.config.recovery_mode,
            corruption_tolerance: self.config.corruption_tolerance,
        };
        info!("Using WAL recovery mode: {:?}", self.config.recovery_mode);

        // Validate segments
        let validation = strategy.validate(segments.clone()).await?;
        info!(
            "Validation complete: {} valid, {} corrupted segments",
            validation.valid_segments,
            validation.corrupted_segments.len()
        );

        // Perform recovery
        let mut result = strategy.recover(segments.clone()).await?;

        // Handle corrupted entries based on tolerance
        let repair = if !validation.corrupted_segments.is_empty() {
            match self.config.corruption_tolerance {
                CorruptionTolerance::FailOnCorruption => {
                    return Err(WalError::Recovery(RecoveryError::UnrecoverableCorruption {
                        lsn: result
                            .corrupted_entries
                            .first()
                            .map(|(lsn, _)| *lsn)
                            .unwrap_or(LogSequenceNumber::ZERO),
                    }));
                }
                CorruptionTolerance::AttemptRepair => {
                    Some(strategy.repair(segments.clone()).await?)
                }
                _ => None,
            }
        } else {
            None
        };

        // Find last checkpoint
        let checkpoint_restored = self.find_last_checkpoint(&result.recovered_entries);

        // Recover transactions
        let transactions_recovered = self.recover_transactions(&result.recovered_entries);

        // Update result with recovery time
        result.recovery_time_ms = start.elapsed().as_millis() as u64;

        info!(
            "Recovery complete in {}ms: {} entries recovered, {} corrupted, {} transactions",
            result.recovery_time_ms,
            result.recovered_entries.len(),
            result.corrupted_entries.len(),
            transactions_recovered.len()
        );

        Ok(RecoveryReport {
            result,
            validation,
            repair,
            checkpoint_restored,
            transactions_recovered,
        })
    }

    fn find_segments(&self) -> WalResult<Vec<SegmentFile>> {
        let mut segments = Vec::new();

        let entries = fs::read_dir(&self.wal_path).map_err(|e| WalError::Io {
            source: e,
            path: Some(self.wal_path.clone()),
        })?;

        for entry in entries {
            let entry = entry.map_err(|e| WalError::Io {
                source: e,
                path: Some(self.wal_path.clone()),
            })?;

            let path = entry.path();
            let file_name = path.file_name().unwrap().to_str().unwrap();

            // Match WAL segment files (wal_00000000.log)
            if file_name.starts_with("wal_") && file_name.ends_with(".log") {
                let id_str = &file_name[4..12];
                if let Ok(id) = u64::from_str_radix(id_str, 10) {
                    let metadata = entry.metadata().map_err(|e| WalError::Io {
                        source: e,
                        path: Some(path.clone()),
                    })?;

                    segments.push(SegmentFile {
                        path,
                        id,
                        size: metadata.len(),
                        created: metadata.created().unwrap_or(std::time::UNIX_EPOCH),
                    });
                }
            }
        }

        // Sort by segment ID
        segments.sort_by_key(|s| s.id);

        Ok(segments)
    }

    fn find_last_checkpoint(&self, entries: &[WalEntry]) -> Option<LogSequenceNumber> {
        entries
            .iter()
            .rev()
            .find(|e| e.header.entry_type == EntryType::Checkpoint)
            .map(|e| e.header.lsn)
    }

    fn recover_transactions(&self, entries: &[WalEntry]) -> Vec<TransactionRecovery> {
        let mut transactions: HashMap<u64, TransactionRecovery> = HashMap::new();
        let mut committed_txs = HashSet::new();
        let mut aborted_txs = HashSet::new();

        // First pass: identify transaction states
        for entry in entries {
            if let super::EntryPayload::Transaction(ref op) = entry.payload {
                match op {
                    TransactionOp::Begin { tx_id, .. } => {
                        transactions.insert(
                            *tx_id,
                            TransactionRecovery {
                                tx_id: *tx_id,
                                status: TransactionRecoveryStatus::Active,
                                entries: Vec::new(),
                                decision: RecoveryDecision::Abort, // Default to abort
                            },
                        );
                    }
                    TransactionOp::Prepare { tx_id, .. } => {
                        if let Some(tx) = transactions.get_mut(tx_id) {
                            tx.status = TransactionRecoveryStatus::Prepared;
                        }
                    }
                    TransactionOp::Commit { tx_id, .. } => {
                        committed_txs.insert(*tx_id);
                        if let Some(tx) = transactions.get_mut(tx_id) {
                            tx.status = TransactionRecoveryStatus::Committed;
                            tx.decision = RecoveryDecision::Skip; // Already committed
                        }
                    }
                    TransactionOp::Abort { tx_id, .. } => {
                        aborted_txs.insert(*tx_id);
                        if let Some(tx) = transactions.get_mut(tx_id) {
                            tx.status = TransactionRecoveryStatus::Aborted;
                            tx.decision = RecoveryDecision::Skip; // Already aborted
                        }
                    }
                }
            }

            // Track entries belonging to transactions
            if let Some(ref tx_ctx) = entry.transaction {
                if let Some(tx) = transactions.get_mut(&tx_ctx.tx_id) {
                    tx.entries.push(entry.header.lsn);
                }
            }
        }

        // Second pass: determine recovery decisions
        for tx in transactions.values_mut() {
            if !committed_txs.contains(&tx.tx_id) && !aborted_txs.contains(&tx.tx_id) {
                match tx.status {
                    TransactionRecoveryStatus::Prepared => {
                        // Prepared but not committed - need to make a decision
                        // In a real system, would check with participants
                        tx.decision = RecoveryDecision::Abort; // Conservative: abort
                    }
                    TransactionRecoveryStatus::Active => {
                        // Active at crash - must abort
                        tx.decision = RecoveryDecision::Abort;
                    }
                    _ => {}
                }
            }
        }

        transactions.into_values().collect()
    }
}

#[async_trait::async_trait]
impl RecoveryStrategy for DefaultRecoveryStrategy {
    async fn recover(&self, segments: Vec<SegmentFile>) -> WalResult<RecoveryResult> {
        let mut recovered_entries = Vec::new();
        let mut corrupted_entries = Vec::new();
        let mut last_valid_lsn = LogSequenceNumber::ZERO;

        for segment in segments {
            debug!(
                "Recovering segment {} from {:?} (created {:?})",
                segment.id, segment.path, segment.created
            );

            let file = File::open(&segment.path).map_err(|e| WalError::Io {
                source: e,
                path: Some(segment.path.clone()),
            })?;

            let mut reader = BufReader::new(file);
            let mut position = 0u64;

            while position < segment.size {
                // Try to read entry
                match self.read_entry_at(&mut reader, position) {
                    Ok(Some(entry)) => {
                        // Validate entry
                        if self.validate_entry(&entry) {
                            last_valid_lsn = entry.header.lsn;
                            recovered_entries.push(entry.clone());
                            position += entry.size_bytes() as u64;
                        } else {
                            corrupted_entries.push((
                                entry.header.lsn,
                                WalError::Corruption {
                                    lsn: entry.header.lsn,
                                    details: "Entry validation failed".to_string(),
                                    recoverable: true,
                                },
                            ));

                            // Handle corruption based on tolerance
                            match self.corruption_tolerance {
                                CorruptionTolerance::SkipCorrupted => {
                                    position += entry.size_bytes() as u64;
                                    continue;
                                }
                                CorruptionTolerance::TruncateAtCorruption => {
                                    break; // Stop recovering this segment
                                }
                                _ => {
                                    position += entry.size_bytes() as u64;
                                }
                            }
                        }
                    }
                    Ok(None) => {
                        // End of valid data in segment
                        break;
                    }
                    Err(e) => {
                        warn!("Error reading entry at position {}: {}", position, e);

                        // Try to find next valid entry
                        match self.find_next_entry(&mut reader, position) {
                            Some(next_pos) => {
                                warn!("Found next valid entry at position {}", next_pos);
                                position = next_pos;
                            }
                            None => {
                                warn!("No more valid entries found in segment");
                                break;
                            }
                        }
                    }
                }
            }
        }

        Ok(RecoveryResult {
            recovered_entries,
            last_valid_lsn,
            corrupted_entries,
            recovery_time_ms: 0, // Will be set by caller
        })
    }

    async fn validate(&self, segments: Vec<SegmentFile>) -> WalResult<ValidationReport> {
        debug!("Validating WAL segments in {:?} mode", self.mode);
        let mut valid_segments = 0;
        let mut corrupted_segments = Vec::new();
        let mut total_entries = 0u64;
        let mut valid_entries = 0u64;
        let mut checksum_failures = Vec::new();

        for segment in segments {
            let mut segment_valid = true;

            let file = File::open(&segment.path).map_err(|e| WalError::Io {
                source: e,
                path: Some(segment.path.clone()),
            })?;

            let mut reader = BufReader::new(file);
            let mut position = 0u64;

            while position < segment.size {
                match self.read_entry_at(&mut reader, position) {
                    Ok(Some(entry)) => {
                        total_entries += 1;

                        if self.validate_entry(&entry) {
                            valid_entries += 1;
                        } else {
                            checksum_failures.push(entry.header.lsn);
                            segment_valid = false;
                        }

                        position += entry.size_bytes() as u64;
                    }
                    Ok(None) => break,
                    Err(_) => {
                        segment_valid = false;
                        break;
                    }
                }
            }

            if segment_valid {
                valid_segments += 1;
            } else {
                corrupted_segments.push(segment.path.to_string_lossy().to_string());
            }
        }

        Ok(ValidationReport {
            valid_segments,
            corrupted_segments,
            total_entries,
            valid_entries,
            checksum_failures,
        })
    }

    async fn repair(&self, segments: Vec<SegmentFile>) -> WalResult<RepairReport> {
        let mut repaired_segments = Vec::new();
        let mut unrecoverable_segments = Vec::new();
        let mut data_loss = false;
        let mut recovered_bytes = 0u64;

        for segment in segments {
            let repair_path = segment.path.with_extension("repair");

            match self.repair_segment(&segment, &repair_path) {
                Ok(bytes_recovered) => {
                    // Replace original with repaired
                    fs::rename(&repair_path, &segment.path).map_err(|e| WalError::Io {
                        source: e,
                        path: Some(segment.path.clone()),
                    })?;

                    repaired_segments.push(segment.path.to_string_lossy().to_string());
                    recovered_bytes += bytes_recovered;
                }
                Err(_) => {
                    unrecoverable_segments.push(segment.path.to_string_lossy().to_string());
                    data_loss = true;

                    // Clean up repair attempt
                    let _ = fs::remove_file(&repair_path);
                }
            }
        }

        Ok(RepairReport {
            repaired_segments,
            unrecoverable_segments,
            data_loss,
            recovered_bytes,
        })
    }
}

impl DefaultRecoveryStrategy {
    fn read_entry_at(
        &self,
        reader: &mut BufReader<File>,
        position: u64,
    ) -> WalResult<Option<WalEntry>> {
        reader
            .seek(SeekFrom::Start(position))
            .map_err(|e| WalError::Io {
                source: e,
                path: None,
            })?;

        // Read entry size first (assuming fixed header size)
        let mut size_buf = [0u8; 8];
        match reader.read_exact(&mut size_buf) {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(e) => {
                return Err(WalError::Io {
                    source: e,
                    path: None,
                })
            }
        }

        let entry_size = u64::from_le_bytes(size_buf);
        if entry_size == 0 || entry_size > 1024 * 1024 * 10 {
            // Invalid size, likely end of valid data
            return Ok(None);
        }

        // Read full entry
        let mut entry_buf = vec![0u8; entry_size as usize];
        reader
            .seek(SeekFrom::Start(position))
            .map_err(|e| WalError::Io {
                source: e,
                path: None,
            })?;

        reader
            .read_exact(&mut entry_buf)
            .map_err(|e| WalError::Io {
                source: e,
                path: None,
            })?;

        // Deserialize entry
        match WalEntry::from_bytes(&entry_buf) {
            Ok(entry) => Ok(Some(entry)),
            Err(e) => Err(WalError::Serialization(e)),
        }
    }

    fn validate_entry(&self, entry: &WalEntry) -> bool {
        // Validate checksum
        let mut test_entry = entry.clone();
        let computed_checksum = {
            test_entry.header.checksum = 0;
            let bytes = test_entry.to_bytes();
            crc32fast::hash(&bytes)
        };

        computed_checksum == entry.header.checksum
    }

    fn find_next_entry(&self, reader: &mut BufReader<File>, start_position: u64) -> Option<u64> {
        let mut position = start_position + 1;
        let max_search = 1024 * 1024; // Search up to 1MB

        while position < start_position + max_search {
            if let Ok(Some(_)) = self.read_entry_at(reader, position) {
                return Some(position);
            }
            position += 1;
        }

        None
    }

    fn repair_segment(&self, segment: &SegmentFile, repair_path: &Path) -> WalResult<u64> {
        let mut recovered_bytes = 0u64;

        let input = File::open(&segment.path).map_err(|e| WalError::Io {
            source: e,
            path: Some(segment.path.clone()),
        })?;

        let mut output = File::create(repair_path).map_err(|e| WalError::Io {
            source: e,
            path: Some(repair_path.to_path_buf()),
        })?;

        let mut reader = BufReader::new(input);
        let mut position = 0u64;

        while position < segment.size {
            match self.read_entry_at(&mut reader, position) {
                Ok(Some(entry)) if self.validate_entry(&entry) => {
                    // Write valid entry to repair file
                    let mut entry_mut = entry.clone();
                    let bytes = entry_mut.to_bytes();
                    use std::io::Write;
                    output.write_all(&bytes).map_err(|e| WalError::Io {
                        source: e,
                        path: Some(repair_path.to_path_buf()),
                    })?;

                    recovered_bytes += bytes.len() as u64;
                    position += entry.size_bytes() as u64;
                }
                _ => {
                    // Skip corrupted data
                    if let Some(next_pos) = self.find_next_entry(&mut reader, position) {
                        position = next_pos;
                    } else {
                        break;
                    }
                }
            }
        }

        output.sync_all().map_err(|e| WalError::Io {
            source: e,
            path: Some(repair_path.to_path_buf()),
        })?;

        Ok(recovered_bytes)
    }
}
