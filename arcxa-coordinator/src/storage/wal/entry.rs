// WAL Entry types and serialization
//
// Entries are the atomic units of the WAL. Each entry contains:
// - Header with metadata (LSN, type, checksum)
// - Payload (serialized data)
// - Transaction context (if applicable)

use bytes::{BufMut, Bytes, BytesMut};
use crc32fast::Hasher;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

use graphica_core::core::lineage::LineageEvent;

/// Log Sequence Number - monotonically increasing identifier
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
pub struct LogSequenceNumber(pub u64);

impl LogSequenceNumber {
    pub const ZERO: Self = Self(0);

    pub fn next(&self) -> Self {
        Self(self.0 + 1)
    }

    pub fn advance(&mut self) -> Self {
        self.0 += 1;
        *self
    }
}

impl std::fmt::Display for LogSequenceNumber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "LSN:{:016x}", self.0)
    }
}

/// Entry types for different operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntryType {
    // Data operations
    LineageWrite,
    QualityViolation,
    ProfileUpdate,

    // RDF operations (NEW for Phase 1)
    RdfInsertTriple,
    RdfDeleteTriple,
    RdfInsertBatch,
    RdfUpdateTriple,

    // Transaction markers
    TransactionBegin,
    TransactionPrepare,
    TransactionCommit,
    TransactionAbort,

    // Storage operations
    RocksDbWrite,
    KafkaPublish,
    ParquetFlush,

    // Control records
    Checkpoint,
    RotationMarker,
    CompactionMarker,
}

/// WAL Entry structure with all metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalEntry {
    // Header (fixed size for efficient reading)
    pub header: EntryHeader,

    // Variable-length payload
    pub payload: EntryPayload,

    // Optional transaction context
    pub transaction: Option<TransactionContext>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryHeader {
    pub lsn: LogSequenceNumber,
    pub previous_lsn: LogSequenceNumber,
    pub entry_type: EntryType,
    pub timestamp_us: u64,
    pub payload_size: u32,
    pub checksum: u32,
    pub flags: EntryFlags,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct EntryFlags {
    pub compressed: bool,
    pub encrypted: bool,
    pub idempotent: bool,
    pub requires_sync: bool,
}

impl Default for EntryFlags {
    fn default() -> Self {
        Self {
            compressed: false,
            encrypted: false,
            idempotent: false,
            requires_sync: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EntryPayload {
    // Lineage operations
    Lineage(Box<LineageEvent>),
    LineageBatch(Vec<LineageEvent>),

    // Quality operations
    QualityViolation {
        dataset: String,
        rule_id: String,
        record_ids: Vec<String>,
        severity: String,
    },

    // RDF operations (NEW for Phase 1)
    RdfTriple(RdfTripleEntry),
    RdfTripleBatch(Vec<RdfTripleEntry>),
    RdfUpdate {
        old_triple: RdfTripleEntry,
        new_triple: RdfTripleEntry,
    },

    // Storage operations
    StorageWrite {
        storage_type: StorageType,
        key: Bytes,
        value: Bytes,
        options: StorageOptions,
    },

    // Transaction operations
    Transaction(TransactionOp),

    // Control operations
    Checkpoint {
        lsn: LogSequenceNumber,
        storage_states: Vec<StorageCheckpoint>,
    },

    // Raw bytes for forward compatibility
    Raw(Bytes),
}

/// RDF Triple Entry for WAL persistence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RdfTripleEntry {
    /// Subject of the triple (URI or blank node)
    pub subject: String,

    /// Predicate of the triple (URI)
    pub predicate: String,

    /// Object of the triple (URI, literal, or blank node)
    pub object: String,

    /// Optional datatype for literal objects (XSD type URI)
    pub datatype: Option<String>,

    /// Optional language tag for literal objects (BCP 47 tag)
    pub language: Option<String>,

    /// Named graph URI (empty string for default graph)
    pub graph: String,

    /// Target shard ID for routing
    pub shard_id: String,

    /// Operation type (insert or delete)
    pub operation: RdfOperation,

    /// Timestamp of the operation
    pub timestamp_us: u64,
}

/// RDF operation type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RdfOperation {
    Insert,
    Delete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StorageType {
    RocksDb,
    Kafka,
    Parquet,
    Archive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageOptions {
    pub partition: Option<String>,
    pub replication: Option<u8>,
    pub compression: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionContext {
    pub tx_id: u64,
    pub parent_tx_id: Option<u64>,
    pub isolation_level: IsolationLevel,
    pub participants: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum IsolationLevel {
    ReadUncommitted,
    ReadCommitted,
    RepeatableRead,
    Serializable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransactionOp {
    Begin {
        tx_id: u64,
        timeout_ms: u64,
    },
    Prepare {
        tx_id: u64,
        participants: Vec<String>,
    },
    Commit {
        tx_id: u64,
        commit_lsn: LogSequenceNumber,
    },
    Abort {
        tx_id: u64,
        reason: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageCheckpoint {
    pub storage_type: StorageType,
    pub last_flushed_lsn: LogSequenceNumber,
    pub pending_writes: u64,
    pub state: Bytes,
}

impl WalEntry {
    /// Create a new lineage entry
    pub fn lineage(lsn: LogSequenceNumber, event: LineageEvent) -> Self {
        let payload = EntryPayload::Lineage(Box::new(event));
        Self::new(lsn, EntryType::LineageWrite, payload)
    }

    /// Create a new RDF triple insert entry
    pub fn rdf_insert(lsn: LogSequenceNumber, triple: RdfTripleEntry) -> Self {
        let payload = EntryPayload::RdfTriple(triple);
        let mut entry = Self::new(lsn, EntryType::RdfInsertTriple, payload);
        entry.header.flags.idempotent = true; // RDF inserts are idempotent
        entry
    }

    /// Create a new RDF triple delete entry
    pub fn rdf_delete(lsn: LogSequenceNumber, triple: RdfTripleEntry) -> Self {
        let payload = EntryPayload::RdfTriple(triple);
        let mut entry = Self::new(lsn, EntryType::RdfDeleteTriple, payload);
        entry.header.flags.idempotent = true; // RDF deletes are idempotent
        entry
    }

    /// Create a new RDF batch insert entry
    pub fn rdf_batch_insert(lsn: LogSequenceNumber, triples: Vec<RdfTripleEntry>) -> Self {
        let payload = EntryPayload::RdfTripleBatch(triples);
        let mut entry = Self::new(lsn, EntryType::RdfInsertBatch, payload);
        entry.header.flags.idempotent = true;
        entry
    }

    /// Create a new entry with automatic header construction
    pub fn new(lsn: LogSequenceNumber, entry_type: EntryType, payload: EntryPayload) -> Self {
        let timestamp_us = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64;

        let header = EntryHeader {
            lsn,
            previous_lsn: LogSequenceNumber(lsn.0.saturating_sub(1)),
            entry_type,
            timestamp_us,
            payload_size: 0, // Will be set during serialization
            checksum: 0,     // Will be computed during serialization
            flags: EntryFlags::default(),
        };

        Self {
            header,
            payload,
            transaction: None,
        }
    }

    /// Serialize entry to bytes with checksum
    pub fn to_bytes(&mut self) -> Bytes {
        let mut buf = BytesMut::new();

        // Serialize payload first to get size
        let payload_bytes = bincode::serialize(&self.payload).unwrap();
        self.header.payload_size = payload_bytes.len() as u32;

        // Compute checksum
        let mut hasher = Hasher::new();
        hasher.update(&payload_bytes);
        if let Some(ref tx) = self.transaction {
            let tx_bytes = bincode::serialize(tx).unwrap();
            hasher.update(&tx_bytes);
        }
        self.header.checksum = hasher.finalize();

        // Write header
        buf.put_slice(&bincode::serialize(&self.header).unwrap());

        // Write payload
        buf.put_slice(&payload_bytes);

        // Write transaction context if present
        if let Some(ref tx) = self.transaction {
            buf.put_slice(&bincode::serialize(tx).unwrap());
        }

        buf.freeze()
    }

    /// Deserialize entry from bytes with checksum validation
    pub fn from_bytes(data: &[u8]) -> Result<Self, String> {
        // Read header
        let header_size = std::mem::size_of::<EntryHeader>();
        if data.len() < header_size {
            return Err("Data too short for header".to_string());
        }

        let header: EntryHeader = bincode::deserialize(&data[..header_size])
            .map_err(|e| format!("Header deserialize error: {}", e))?;

        // Read payload
        let payload_start = header_size;
        let payload_end = payload_start + header.payload_size as usize;
        if data.len() < payload_end {
            return Err("Data too short for payload".to_string());
        }

        let payload: EntryPayload = bincode::deserialize(&data[payload_start..payload_end])
            .map_err(|e| format!("Payload deserialize error: {}", e))?;

        // Read transaction context if present
        let transaction = if data.len() > payload_end {
            Some(
                bincode::deserialize(&data[payload_end..])
                    .map_err(|e| format!("Transaction deserialize error: {}", e))?,
            )
        } else {
            None
        };

        // Validate checksum
        let mut hasher = Hasher::new();
        hasher.update(&data[payload_start..payload_end]);
        if let Some(ref tx) = transaction {
            let tx_bytes = bincode::serialize(tx).unwrap();
            hasher.update(&tx_bytes);
        }

        if hasher.finalize() != header.checksum {
            return Err("Checksum validation failed".to_string());
        }

        Ok(Self {
            header,
            payload,
            transaction,
        })
    }

    /// Check if entry is idempotent (safe to replay)
    pub fn is_idempotent(&self) -> bool {
        self.header.flags.idempotent
            || matches!(
                self.header.entry_type,
                EntryType::Checkpoint | EntryType::RotationMarker | EntryType::CompactionMarker
            )
    }

    /// Get entry size in bytes
    pub fn size_bytes(&self) -> usize {
        std::mem::size_of::<EntryHeader>() + self.header.payload_size as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entry_serialization() {
        let event = LineageEvent {
            id: uuid::Uuid::new_v4(),
            dataset: "test_dataset".to_string(),
            record_id: "rec_123".to_string(),
            source_refs: vec![],
            transforms: vec![],
            model_refs: vec![],
            output_ref: graphica_core::core::lineage::DataRef {
                system: "test".to_string(),
                path: "test/path".to_string(),
                version: None,
                extracted_at: chrono::Utc::now(),
                cdc_position: None,
            },
            ts: chrono::Utc::now(),
            run_id: "run_123".to_string(),
            tenant_id: "test_tenant".to_string(),
            correlation_id: Some(uuid::Uuid::new_v4().to_string()),
            metadata: std::collections::HashMap::new(),
        };

        let mut entry = WalEntry::lineage(LogSequenceNumber(1), event);
        let bytes = entry.to_bytes();

        let deserialized = WalEntry::from_bytes(&bytes).unwrap();
        assert_eq!(entry.header.lsn, deserialized.header.lsn);
        assert_eq!(entry.header.entry_type, deserialized.header.entry_type);
    }

    #[test]
    fn test_checksum_validation() {
        let mut entry = WalEntry::new(
            LogSequenceNumber(1),
            EntryType::Checkpoint,
            EntryPayload::Raw(Bytes::from("test data")),
        );

        let mut bytes = entry.to_bytes().to_vec();

        // Corrupt the data
        let len = bytes.len();
        bytes[len - 1] ^= 0xFF;

        // Should fail checksum validation
        assert!(WalEntry::from_bytes(&bytes).is_err());
    }
}
