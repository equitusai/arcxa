//! Persistent storage for audit chain using RocksDB.
//!
//! Provides:
//! - Durable storage for signed audit entries
//! - Efficient range queries by index or timestamp
//! - Crash recovery and replay capability
//! - Compaction and archival strategies
//!
//! ## Architecture
//!
//! ```text
//! RocksDB Column Families:
//! - "entries": index -> SignedAuditEntry
//! - "metadata": chain metadata (head, count, etc.)
//! - "archive": archived entries (compacted)
//! ```

use super::crypto::Hash;
use super::entry::SignedAuditEntry;
use anyhow::{Context, Result};
use rocksdb::{ColumnFamilyDescriptor, Options, DB};
use std::path::Path;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Column family names
const CF_ENTRIES: &str = "entries";
const CF_METADATA: &str = "metadata";
const CF_ARCHIVE: &str = "archive";

/// Metadata keys
const KEY_HEAD_HASH: &[u8] = b"head_hash";
const KEY_CHAIN_LENGTH: &[u8] = b"chain_length";
const KEY_GENESIS_HASH: &[u8] = b"genesis_hash";
const KEY_PUBLIC_KEY: &[u8] = b"public_key";

/// Persistent audit chain storage.
pub struct AuditStore {
    db: Arc<DB>,
}

impl AuditStore {
    /// Create a new audit store at the given path.
    ///
    /// # Arguments
    ///
    /// * `path` - Directory path for RocksDB storage
    ///
    /// # Example
    ///
    /// ```ignore
    /// let store = AuditStore::new("./data/audit")?;
    /// ```
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let mut db_opts = Options::default();
        db_opts.create_if_missing(true);
        db_opts.create_missing_column_families(true);

        // Column family descriptors
        let cf_entries = ColumnFamilyDescriptor::new(CF_ENTRIES, Options::default());
        let cf_metadata = ColumnFamilyDescriptor::new(CF_METADATA, Options::default());
        let cf_archive = ColumnFamilyDescriptor::new(CF_ARCHIVE, Options::default());

        let db = DB::open_cf_descriptors(
            &db_opts,
            path.as_ref(),
            vec![cf_entries, cf_metadata, cf_archive],
        )?;

        info!("Audit store initialized at: {}", path.as_ref().display());

        Ok(Self { db: Arc::new(db) })
    }

    /// Append a signed entry to the store.
    ///
    /// Persists the entry with its index and updates metadata.
    pub fn append(&self, index: u64, entry: &SignedAuditEntry) -> Result<()> {
        let cf_entries = self
            .db
            .cf_handle(CF_ENTRIES)
            .context("Failed to get entries column family")?;
        let cf_metadata = self
            .db
            .cf_handle(CF_METADATA)
            .context("Failed to get metadata column family")?;

        // Serialize entry
        let entry_bytes = entry.to_bytes()?;

        // Store entry with index as key
        let key = index.to_be_bytes();
        self.db.put_cf(cf_entries, &key, &entry_bytes)?;

        // Update metadata
        self.db
            .put_cf(cf_metadata, KEY_HEAD_HASH, entry.entry_hash.as_bytes())?;
        self.db
            .put_cf(cf_metadata, KEY_CHAIN_LENGTH, &(index + 1).to_be_bytes())?;

        debug!("Appended entry {} to audit store", index);

        Ok(())
    }

    /// Get an entry by index.
    ///
    /// Checks both the main entries CF and the archive CF.
    pub fn get(&self, index: u64) -> Result<Option<SignedAuditEntry>> {
        let cf_entries = self
            .db
            .cf_handle(CF_ENTRIES)
            .context("Failed to get entries column family")?;
        let cf_archive = self
            .db
            .cf_handle(CF_ARCHIVE)
            .context("Failed to get archive column family")?;

        let key = index.to_be_bytes();

        // Try main entries first
        if let Some(bytes) = self.db.get_cf(cf_entries, &key)? {
            let entry = SignedAuditEntry::from_bytes(&bytes)?;
            return Ok(Some(entry));
        }

        // Fall back to archive
        if let Some(bytes) = self.db.get_cf(cf_archive, &key)? {
            let entry = SignedAuditEntry::from_bytes(&bytes)?;
            return Ok(Some(entry));
        }

        Ok(None)
    }

    /// Get all entries in the chain.
    pub fn get_all(&self) -> Result<Vec<SignedAuditEntry>> {
        let length = self.length()?;
        let mut entries = Vec::with_capacity(length as usize);

        for i in 0..length {
            if let Some(entry) = self.get(i)? {
                entries.push(entry);
            }
        }

        Ok(entries)
    }

    /// Get a range of entries.
    ///
    /// # Arguments
    ///
    /// * `start` - Starting index (inclusive)
    /// * `end` - Ending index (exclusive)
    pub fn get_range(&self, start: u64, end: u64) -> Result<Vec<SignedAuditEntry>> {
        let mut entries = Vec::with_capacity((end - start) as usize);

        for i in start..end {
            if let Some(entry) = self.get(i)? {
                entries.push(entry);
            }
        }

        Ok(entries)
    }

    /// Get the number of entries in the chain.
    pub fn length(&self) -> Result<u64> {
        let cf_metadata = self
            .db
            .cf_handle(CF_METADATA)
            .context("Failed to get metadata column family")?;

        match self.db.get_cf(cf_metadata, KEY_CHAIN_LENGTH)? {
            Some(bytes) => {
                let mut array = [0u8; 8];
                array.copy_from_slice(&bytes);
                Ok(u64::from_be_bytes(array))
            }
            None => Ok(0),
        }
    }

    /// Get the head hash of the chain.
    pub fn head_hash(&self) -> Result<Option<Hash>> {
        let cf_metadata = self
            .db
            .cf_handle(CF_METADATA)
            .context("Failed to get metadata column family")?;

        match self.db.get_cf(cf_metadata, KEY_HEAD_HASH)? {
            Some(bytes) => {
                let hash = Hash::from_bytes(&bytes).context("Invalid hash in metadata")?;
                Ok(Some(hash))
            }
            None => Ok(None),
        }
    }

    /// Set the genesis hash (should only be called once during initialization).
    pub fn set_genesis_hash(&self, hash: Hash) -> Result<()> {
        let cf_metadata = self
            .db
            .cf_handle(CF_METADATA)
            .context("Failed to get metadata column family")?;

        self.db
            .put_cf(cf_metadata, KEY_GENESIS_HASH, hash.as_bytes())?;

        Ok(())
    }

    /// Get the genesis hash.
    pub fn genesis_hash(&self) -> Result<Option<Hash>> {
        let cf_metadata = self
            .db
            .cf_handle(CF_METADATA)
            .context("Failed to get metadata column family")?;

        match self.db.get_cf(cf_metadata, KEY_GENESIS_HASH)? {
            Some(bytes) => {
                let hash = Hash::from_bytes(&bytes).context("Invalid genesis hash in metadata")?;
                Ok(Some(hash))
            }
            None => Ok(None),
        }
    }

    /// Set the public key used for signing.
    pub fn set_public_key(&self, public_key: &[u8; 32]) -> Result<()> {
        let cf_metadata = self
            .db
            .cf_handle(CF_METADATA)
            .context("Failed to get metadata column family")?;

        self.db.put_cf(cf_metadata, KEY_PUBLIC_KEY, public_key)?;

        Ok(())
    }

    /// Get the public key.
    pub fn public_key(&self) -> Result<Option<[u8; 32]>> {
        let cf_metadata = self
            .db
            .cf_handle(CF_METADATA)
            .context("Failed to get metadata column family")?;

        match self.db.get_cf(cf_metadata, KEY_PUBLIC_KEY)? {
            Some(bytes) => {
                if bytes.len() != 32 {
                    return Err(anyhow::anyhow!("Invalid public key length"));
                }
                let mut array = [0u8; 32];
                array.copy_from_slice(&bytes);
                Ok(Some(array))
            }
            None => Ok(None),
        }
    }

    /// Compact old entries to archive column family.
    ///
    /// Moves entries older than `keep_recent` to the archive CF.
    ///
    /// # Arguments
    ///
    /// * `keep_recent` - Number of recent entries to keep in main CF
    pub fn compact(&self, keep_recent: u64) -> Result<usize> {
        let length = self.length()?;

        if length <= keep_recent {
            return Ok(0);
        }

        let archive_count = length - keep_recent;
        let cf_entries = self
            .db
            .cf_handle(CF_ENTRIES)
            .context("Failed to get entries column family")?;
        let cf_archive = self
            .db
            .cf_handle(CF_ARCHIVE)
            .context("Failed to get archive column family")?;

        let mut compacted = 0;

        for i in 0..archive_count {
            let key = i.to_be_bytes();

            if let Some(entry_bytes) = self.db.get_cf(cf_entries, &key)? {
                // Move to archive
                self.db.put_cf(cf_archive, &key, &entry_bytes)?;
                self.db.delete_cf(cf_entries, &key)?;
                compacted += 1;
            }
        }

        if compacted > 0 {
            info!("Compacted {} entries to archive", compacted);
        }

        Ok(compacted)
    }

    /// Get statistics about the store.
    pub fn statistics(&self) -> Result<StoreStatistics> {
        let length = self.length()?;
        let head_hash = self.head_hash()?;
        let genesis_hash = self.genesis_hash()?;

        // Count archived entries
        let cf_archive = self
            .db
            .cf_handle(CF_ARCHIVE)
            .context("Failed to get archive column family")?;

        let mut archived_count = 0;
        let iter = self
            .db
            .iterator_cf(cf_archive, rocksdb::IteratorMode::Start);
        for _ in iter {
            archived_count += 1;
        }

        Ok(StoreStatistics {
            total_entries: length,
            archived_entries: archived_count,
            head_hash,
            genesis_hash,
        })
    }

    /// Flush all pending writes to disk.
    pub fn flush(&self) -> Result<()> {
        self.db.flush()?;
        debug!("Flushed audit store to disk");
        Ok(())
    }
}

/// Store statistics.
#[derive(Debug, Clone)]
pub struct StoreStatistics {
    pub total_entries: u64,
    pub archived_entries: u64,
    pub head_hash: Option<Hash>,
    pub genesis_hash: Option<Hash>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bitemporal::audit::entry::{AuditEntry, AuditOperation};
    use crate::bitemporal::TransactionId;
    use chrono::{TimeZone, Utc};
    use ed25519_dalek::SigningKey;
    use rand::{rngs::OsRng, RngCore};
    use tempfile::TempDir;

    fn create_test_signing_key() -> SigningKey {
        let mut bytes = [0u8; 32];
        OsRng.fill_bytes(&mut bytes);
        SigningKey::from_bytes(&bytes)
    }

    fn create_test_entry(seq: u64) -> AuditEntry {
        AuditEntry::new(
            TransactionId {
                node_id: 1,
                seq,
                timestamp: Utc.timestamp_opt(1234567890 + seq as i64, 0).unwrap(),
            },
            AuditOperation::Insert,
            1,
            "test_user".to_string(),
        )
    }

    #[test]
    fn test_store_creation() {
        let temp_dir = TempDir::new().unwrap();
        let store = AuditStore::new(temp_dir.path()).unwrap();

        assert_eq!(store.length().unwrap(), 0);
        assert!(store.head_hash().unwrap().is_none());
    }

    #[test]
    fn test_store_append_and_get() {
        let temp_dir = TempDir::new().unwrap();
        let store = AuditStore::new(temp_dir.path()).unwrap();
        let signing_key = create_test_signing_key();

        let entry = create_test_entry(100);
        let signed = entry.sign(&signing_key, Hash::ZERO);

        store.append(0, &signed).unwrap();

        assert_eq!(store.length().unwrap(), 1);

        let retrieved = store.get(0).unwrap().unwrap();
        assert_eq!(retrieved.entry.tx_id.seq, 100);
    }

    #[test]
    fn test_store_multiple_entries() {
        let temp_dir = TempDir::new().unwrap();
        let store = AuditStore::new(temp_dir.path()).unwrap();
        let signing_key = create_test_signing_key();

        let mut prev_hash = Hash::ZERO;

        for i in 0..5 {
            let entry = create_test_entry(i);
            let signed = entry.sign(&signing_key, prev_hash);
            prev_hash = signed.entry_hash;

            store.append(i, &signed).unwrap();
        }

        assert_eq!(store.length().unwrap(), 5);

        let all = store.get_all().unwrap();
        assert_eq!(all.len(), 5);
    }

    #[test]
    fn test_store_get_range() {
        let temp_dir = TempDir::new().unwrap();
        let store = AuditStore::new(temp_dir.path()).unwrap();
        let signing_key = create_test_signing_key();

        let mut prev_hash = Hash::ZERO;

        for i in 0..10 {
            let entry = create_test_entry(i);
            let signed = entry.sign(&signing_key, prev_hash);
            prev_hash = signed.entry_hash;

            store.append(i, &signed).unwrap();
        }

        let range = store.get_range(3, 7).unwrap();
        assert_eq!(range.len(), 4);
        assert_eq!(range[0].entry.tx_id.seq, 3);
        assert_eq!(range[3].entry.tx_id.seq, 6);
    }

    #[test]
    fn test_store_metadata() {
        let temp_dir = TempDir::new().unwrap();
        let store = AuditStore::new(temp_dir.path()).unwrap();

        let genesis = Hash::ZERO;
        store.set_genesis_hash(genesis).unwrap();

        let retrieved_genesis = store.genesis_hash().unwrap().unwrap();
        assert_eq!(retrieved_genesis, genesis);

        let public_key = [42u8; 32];
        store.set_public_key(&public_key).unwrap();

        let retrieved_pk = store.public_key().unwrap().unwrap();
        assert_eq!(retrieved_pk, public_key);
    }

    #[test]
    fn test_store_compact() {
        let temp_dir = TempDir::new().unwrap();
        let store = AuditStore::new(temp_dir.path()).unwrap();
        let signing_key = create_test_signing_key();

        let mut prev_hash = Hash::ZERO;

        for i in 0..10 {
            let entry = create_test_entry(i);
            let signed = entry.sign(&signing_key, prev_hash);
            prev_hash = signed.entry_hash;

            store.append(i, &signed).unwrap();
        }

        let compacted = store.compact(5).unwrap();
        assert_eq!(compacted, 5);

        // Should still be able to get all entries
        let all = store.get_all().unwrap();
        assert_eq!(all.len(), 10);
    }

    #[test]
    fn test_store_persistence() {
        let temp_dir = TempDir::new().unwrap();
        let signing_key = create_test_signing_key();

        // Create store and add entries
        {
            let store = AuditStore::new(temp_dir.path()).unwrap();
            let mut prev_hash = Hash::ZERO;

            for i in 0..3 {
                let entry = create_test_entry(i);
                let signed = entry.sign(&signing_key, prev_hash);
                prev_hash = signed.entry_hash;

                store.append(i, &signed).unwrap();
            }

            store.flush().unwrap();
        }

        // Reopen store and verify data persisted
        {
            let store = AuditStore::new(temp_dir.path()).unwrap();
            assert_eq!(store.length().unwrap(), 3);

            let all = store.get_all().unwrap();
            assert_eq!(all.len(), 3);
            assert_eq!(all[0].entry.tx_id.seq, 0);
            assert_eq!(all[2].entry.tx_id.seq, 2);
        }
    }

    #[test]
    fn test_store_statistics() {
        let temp_dir = TempDir::new().unwrap();
        let store = AuditStore::new(temp_dir.path()).unwrap();
        let signing_key = create_test_signing_key();

        store.set_genesis_hash(Hash::ZERO).unwrap();

        let mut prev_hash = Hash::ZERO;

        for i in 0..5 {
            let entry = create_test_entry(i);
            let signed = entry.sign(&signing_key, prev_hash);
            prev_hash = signed.entry_hash;

            store.append(i, &signed).unwrap();
        }

        let stats = store.statistics().unwrap();
        assert_eq!(stats.total_entries, 5);
        assert_eq!(stats.archived_entries, 0);
        assert!(stats.head_hash.is_some());
        assert_eq!(stats.genesis_hash.unwrap(), Hash::ZERO);
    }
}
