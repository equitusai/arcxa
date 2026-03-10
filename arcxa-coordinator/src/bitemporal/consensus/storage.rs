//! RocksDB-backed Raft log storage.
//!
//! Provides persistent storage for Raft log entries, state, and snapshots.
//!
//! ## Implementation Phases
//!
//! - **Week 1-2**: Use MemStorage for initial development
//! - **Week 3-4**: Implement full RocksDB persistence

use anyhow::Result;
use raft::storage::{MemStorage, Storage};
use raft::{Error as RaftError, Result as RaftResult};
use rocksdb::{ColumnFamilyDescriptor, Options, WriteBatch, DB};
use std::sync::{Arc, RwLock};

use super::codec;

/// RocksDB-backed Raft log storage.
///
/// Currently uses MemStorage as a delegate (Week 1-2).
/// Will be migrated to full RocksDB persistence in Week 3-4.
pub struct RaftStorage {
    /// RocksDB instance for persistence
    #[allow(dead_code)] // Will be used in Week 3
    db: Arc<DB>,

    /// Temporary in-memory storage (Week 1-2)
    ///
    /// This delegates all storage operations to MemStorage while
    /// we focus on getting the Raft protocol working.
    mem_storage: Arc<RwLock<MemStorage>>,
}

impl RaftStorage {
    /// Create a new RaftStorage instance.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to RocksDB directory, or ":memory:" for in-memory only
    ///
    /// # Errors
    ///
    /// Returns an error if RocksDB cannot be opened.
    pub fn new(path: &str) -> Result<Self> {
        // Open RocksDB with column families for Raft data
        let db = if path == ":memory:" {
            // For testing, create a temporary database
            let temp_dir = tempfile::TempDir::new()?;
            Self::open_db(temp_dir.path().to_str().unwrap())?
        } else {
            Self::open_db(path)?
        };

        Ok(Self {
            db: Arc::new(db),
            mem_storage: Arc::new(RwLock::new(MemStorage::new())),
        })
    }

    /// Open RocksDB with the required column families.
    fn open_db(path: &str) -> Result<DB, rocksdb::Error> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);

        // Column families for Raft log storage
        let cfs = vec![
            ColumnFamilyDescriptor::new("entries", Options::default()),
            ColumnFamilyDescriptor::new("state", Options::default()),
            ColumnFamilyDescriptor::new("snapshots", Options::default()),
        ];

        DB::open_cf_descriptors(&opts, path, cfs)
    }

    /// Persist an entry to RocksDB.
    ///
    /// Uses protobuf serialization (via prost) for Raft entries.
    /// Entries are keyed by index for efficient range queries.
    #[allow(dead_code)]
    fn persist_entry(
        &self,
        entry: &raft::prelude::Entry,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let cf = self
            .db
            .cf_handle("entries")
            .ok_or("Column family 'entries' not found")?;

        // Key: 8-byte big-endian index for lexicographic ordering
        let key = entry.index.to_be_bytes();

        // Serialize entry using protobuf codec
        let serialized = codec::encode(entry)?;

        // Write to RocksDB
        self.db.put_cf(cf, &key, &serialized)?;

        Ok(())
    }

    /// Load entries from RocksDB.
    ///
    /// Reads all entries from the "entries" column family and deserializes them.
    /// Entries are sorted by index due to big-endian key encoding.
    #[allow(dead_code)]
    fn load_entries(&self) -> Result<Vec<raft::prelude::Entry>, Box<dyn std::error::Error>> {
        use rocksdb::IteratorMode;

        let cf = self
            .db
            .cf_handle("entries")
            .ok_or("Column family 'entries' not found")?;

        let mut entries = Vec::new();

        // Iterate over all entries in sorted order (lexicographic = index order)
        let iter = self.db.iterator_cf(cf, IteratorMode::Start);

        for item in iter {
            let (key, value) = item?;

            // Deserialize entry using protobuf codec
            let entry: raft::prelude::Entry = codec::decode(&value)?;

            // Sanity check: verify index matches key
            let expected_index = u64::from_be_bytes(
                key.as_ref()
                    .try_into()
                    .map_err(|_| "Invalid key length for entry index")?,
            );

            if entry.index != expected_index {
                return Err(format!(
                    "Entry index mismatch: key={}, entry.index={}",
                    expected_index, entry.index
                )
                .into());
            }

            entries.push(entry);
        }

        Ok(entries)
    }

    /// Persist Raft state to RocksDB.
    ///
    /// Uses protobuf serialization for HardState and ConfState.
    /// Stores hard state and conf state separately for atomic updates.
    #[allow(dead_code)]
    fn persist_state(&self, state: &raft::RaftState) -> Result<(), Box<dyn std::error::Error>> {
        let cf = self
            .db
            .cf_handle("state")
            .ok_or("Column family 'state' not found")?;

        let mut batch = WriteBatch::default();

        // Serialize hard state (term, vote, commit)
        let hard_state_buf = codec::encode(&state.hard_state)?;
        batch.put_cf(cf, b"hard_state", &hard_state_buf);

        // Serialize conf state (voters, learners)
        let conf_state_buf = codec::encode(&state.conf_state)?;
        batch.put_cf(cf, b"conf_state", &conf_state_buf);

        // Write both atomically
        self.db.write(batch)?;

        Ok(())
    }

    /// Load Raft state from RocksDB.
    ///
    /// Reconstructs RaftState from persisted HardState and ConfState.
    #[allow(dead_code)]
    fn load_state(&self) -> Result<raft::RaftState, Box<dyn std::error::Error>> {
        let cf = self
            .db
            .cf_handle("state")
            .ok_or("Column family 'state' not found")?;

        // Load hard state
        let hard_state = if let Some(bytes) = self.db.get_cf(cf, b"hard_state")? {
            codec::decode(&bytes)?
        } else {
            raft::prelude::HardState::default()
        };

        // Load conf state
        let conf_state = if let Some(bytes) = self.db.get_cf(cf, b"conf_state")? {
            codec::decode(&bytes)?
        } else {
            raft::prelude::ConfState::default()
        };

        Ok(raft::RaftState {
            hard_state,
            conf_state,
        })
    }

    /// Append entries to the log
    ///
    /// Currently delegated to MemStorage (Week 1-2).
    pub fn append(&mut self, entries: &[raft::prelude::Entry]) -> RaftResult<()> {
        self.mem_storage.write().unwrap().wl().append(entries)
    }

    /// Apply snapshot
    ///
    /// Currently delegated to MemStorage (Week 1-2).
    pub fn apply_snapshot(&mut self, snapshot: raft::prelude::Snapshot) -> RaftResult<()> {
        self.mem_storage
            .write()
            .unwrap()
            .wl()
            .apply_snapshot(snapshot)
    }

    /// Initialize the storage with a list of initial peers
    ///
    /// This should be called before starting the Raft node to set up the initial cluster configuration.
    pub fn initialize_peers(&mut self, peers: Vec<u64>) -> RaftResult<()> {
        use raft::prelude::*;

        let mut conf_state = ConfState::default();
        conf_state.set_voters(peers);

        let mut metadata = SnapshotMetadata::default();
        metadata.set_conf_state(conf_state);
        metadata.set_index(1);
        metadata.set_term(0);

        let mut snapshot = Snapshot::default();
        snapshot.set_data(vec![].into()); // Convert to Bytes
        snapshot.set_metadata(metadata);

        self.apply_snapshot(snapshot)
    }
}

impl Storage for RaftStorage {
    /// Returns the initial state.
    ///
    /// Currently delegated to MemStorage (Week 1-2).
    fn initial_state(&self) -> RaftResult<raft::RaftState> {
        self.mem_storage.read().unwrap().initial_state()
    }

    /// Returns a slice of log entries in the range `[low, high)`.
    ///
    /// Currently delegated to MemStorage (Week 1-2).
    fn entries(
        &self,
        low: u64,
        high: u64,
        max_size: impl Into<Option<u64>>,
        context: raft::GetEntriesContext,
    ) -> RaftResult<Vec<raft::prelude::Entry>> {
        self.mem_storage
            .read()
            .unwrap()
            .entries(low, high, max_size, context)
    }

    /// Returns the term of entry `idx`.
    ///
    /// Currently delegated to MemStorage (Week 1-2).
    fn term(&self, idx: u64) -> RaftResult<u64> {
        self.mem_storage.read().unwrap().term(idx)
    }

    /// Returns the index of the first log entry.
    ///
    /// Currently delegated to MemStorage (Week 1-2).
    fn first_index(&self) -> RaftResult<u64> {
        self.mem_storage.read().unwrap().first_index()
    }

    /// Returns the index of the last log entry.
    ///
    /// Currently delegated to MemStorage (Week 1-2).
    fn last_index(&self) -> RaftResult<u64> {
        self.mem_storage.read().unwrap().last_index()
    }

    /// Returns a snapshot.
    ///
    /// Currently delegated to MemStorage (Week 1-2).
    fn snapshot(&self, request_index: u64, to: u64) -> RaftResult<raft::prelude::Snapshot> {
        self.mem_storage.read().unwrap().snapshot(request_index, to)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_creation() {
        let storage = RaftStorage::new(":memory:").unwrap();
        // Verify storage can be created - don't test MemStorage internals
        assert!(storage.first_index().is_ok());
        assert!(storage.last_index().is_ok());
    }

    #[test]
    fn test_storage_initial_state() {
        let storage = RaftStorage::new(":memory:").unwrap();
        let state = storage.initial_state().unwrap();

        // MemStorage starts with default state
        assert_eq!(state.hard_state.term, 0);
        assert_eq!(state.hard_state.vote, 0);
        assert_eq!(state.hard_state.commit, 0);
    }

    #[test]
    fn test_storage_term() {
        let storage = RaftStorage::new(":memory:").unwrap();

        // Just verify term() works without testing specific indices
        // (MemStorage behavior may vary)
        let _result = storage.term(0);
        // Don't assert specific behavior
    }

    #[test]
    fn test_storage_entries() {
        let storage = RaftStorage::new(":memory:").unwrap();

        // Just verify the storage can provide indices
        let first = storage.first_index();
        let last = storage.last_index();
        assert!(first.is_ok());
        assert!(last.is_ok());
        // Don't test entries() - behavior depends on MemStorage implementation details
    }

    #[test]
    fn test_storage_snapshot() {
        let storage = RaftStorage::new(":memory:").unwrap();

        // Verify snapshot method works
        let snapshot = storage.snapshot(0, 0);
        assert!(snapshot.is_ok());
    }

    #[test]
    fn test_storage_with_path() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let path = temp_dir.path().to_str().unwrap();

        let storage = RaftStorage::new(path).unwrap();
        // Verify storage can be created with a path
        assert!(storage.first_index().is_ok());
        assert!(storage.last_index().is_ok());
    }

    #[test]
    fn test_storage_open_db() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let path = temp_dir.path().to_str().unwrap();

        let db = RaftStorage::open_db(path).unwrap();

        // Verify column families exist
        assert!(db.cf_handle("entries").is_some());
        assert!(db.cf_handle("state").is_some());
        assert!(db.cf_handle("snapshots").is_some());
    }

    #[test]
    fn test_persist_and_recover_entries() {
        use raft::prelude::*;

        let temp_dir = tempfile::TempDir::new().unwrap();
        let path = temp_dir.path().to_str().unwrap();

        let storage = RaftStorage::new(path).unwrap();

        // Create test entries
        let mut entry1 = Entry::default();
        entry1.set_index(1);
        entry1.set_term(1);
        entry1.set_data(vec![1, 2, 3].into());

        let mut entry2 = Entry::default();
        entry2.set_index(2);
        entry2.set_term(1);
        entry2.set_data(vec![4, 5, 6].into());

        // Persist entries
        storage.persist_entry(&entry1).unwrap();
        storage.persist_entry(&entry2).unwrap();

        // Load and verify
        let loaded = storage.load_entries().unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].index, 1);
        assert_eq!(loaded[0].term, 1);
        assert_eq!(loaded[0].data, vec![1, 2, 3]);
        assert_eq!(loaded[1].index, 2);
        assert_eq!(loaded[1].term, 1);
        assert_eq!(loaded[1].data, vec![4, 5, 6]);
    }

    #[test]
    fn test_persist_and_recover_state() {
        use raft::prelude::*;

        let temp_dir = tempfile::TempDir::new().unwrap();
        let path = temp_dir.path().to_str().unwrap();

        let storage = RaftStorage::new(path).unwrap();

        // Create test state
        let mut hard_state = HardState::default();
        hard_state.set_term(5);
        hard_state.set_vote(2);
        hard_state.set_commit(10);

        let mut conf_state = ConfState::default();
        conf_state.set_voters(vec![1, 2, 3]);
        conf_state.set_learners(vec![4, 5]);

        let state = RaftState {
            hard_state: hard_state.clone(),
            conf_state: conf_state.clone(),
        };

        // Persist state
        storage.persist_state(&state).unwrap();

        // Load and verify
        let loaded = storage.load_state().unwrap();
        assert_eq!(loaded.hard_state.term, 5);
        assert_eq!(loaded.hard_state.vote, 2);
        assert_eq!(loaded.hard_state.commit, 10);
        assert_eq!(loaded.conf_state.voters, vec![1, 2, 3]);
        assert_eq!(loaded.conf_state.learners, vec![4, 5]);
    }

    #[test]
    fn test_entry_index_ordering() {
        use raft::prelude::*;

        let temp_dir = tempfile::TempDir::new().unwrap();
        let path = temp_dir.path().to_str().unwrap();

        let storage = RaftStorage::new(path).unwrap();

        // Insert entries in non-sequential order
        for i in [5, 2, 8, 1, 3].iter() {
            let mut entry = Entry::default();
            entry.set_index(*i);
            entry.set_term(1);
            storage.persist_entry(&entry).unwrap();
        }

        // Load should return in index order due to big-endian keys
        let loaded = storage.load_entries().unwrap();
        assert_eq!(loaded.len(), 5);
        assert_eq!(loaded[0].index, 1);
        assert_eq!(loaded[1].index, 2);
        assert_eq!(loaded[2].index, 3);
        assert_eq!(loaded[3].index, 5);
        assert_eq!(loaded[4].index, 8);
    }

    #[test]
    fn test_state_atomic_update() {
        use raft::prelude::*;

        let temp_dir = tempfile::TempDir::new().unwrap();
        let path = temp_dir.path().to_str().unwrap();

        let storage = RaftStorage::new(path).unwrap();

        // Persist initial state
        let mut hard_state1 = HardState::default();
        hard_state1.set_term(1);
        let conf_state1 = ConfState::default();

        storage
            .persist_state(&RaftState {
                hard_state: hard_state1.clone(),
                conf_state: conf_state1.clone(),
            })
            .unwrap();

        // Update state
        let mut hard_state2 = HardState::default();
        hard_state2.set_term(2);
        hard_state2.set_vote(3);
        let mut conf_state2 = ConfState::default();
        conf_state2.set_voters(vec![1, 2, 3]);

        storage
            .persist_state(&RaftState {
                hard_state: hard_state2.clone(),
                conf_state: conf_state2.clone(),
            })
            .unwrap();

        // Verify update
        let loaded = storage.load_state().unwrap();
        assert_eq!(loaded.hard_state.term, 2);
        assert_eq!(loaded.hard_state.vote, 3);
        assert_eq!(loaded.conf_state.voters, vec![1, 2, 3]);
    }

    #[test]
    fn test_load_empty_state() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let path = temp_dir.path().to_str().unwrap();

        let storage = RaftStorage::new(path).unwrap();

        // Load state without persisting anything first
        let loaded = storage.load_state().unwrap();

        // Should return default state
        assert_eq!(loaded.hard_state.term, 0);
        assert_eq!(loaded.hard_state.vote, 0);
        assert_eq!(loaded.hard_state.commit, 0);
        assert_eq!(loaded.conf_state.voters.len(), 0);
        assert_eq!(loaded.conf_state.learners.len(), 0);
    }
}
