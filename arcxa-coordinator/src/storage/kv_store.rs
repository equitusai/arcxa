//! Generic key-value store using RocksDB
//!
//! Provides simple get/put/delete operations for user storage and other use cases.

use anyhow::{Context, Result};
use rocksdb::{IteratorMode, Options, DB};
use std::path::Path;
use std::sync::Arc;

/// Generic RocksDB key-value store
pub struct KvStore {
    db: Arc<DB>,
}

impl KvStore {
    /// Create new key-value store at path
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);

        let db = DB::open(&opts, path).context("Failed to open RocksDB for key-value store")?;

        Ok(Self { db: Arc::new(db) })
    }

    /// Create in-memory store (for testing)
    ///
    /// # Warning
    /// This is intended for testing only. Data is stored in a temporary directory
    /// and will be deleted when the KvStore is dropped.
    pub fn new_in_memory() -> Result<Self> {
        use tempfile::TempDir;
        let temp_dir = TempDir::new()?;
        Self::new(temp_dir.path())
    }

    /// Get value by key
    pub fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        self.db.get(key).context("Failed to get value from RocksDB")
    }

    /// Put key-value pair
    pub fn put(&self, key: &[u8], value: &[u8]) -> Result<()> {
        self.db
            .put(key, value)
            .context("Failed to put value into RocksDB")
    }

    /// Delete key
    pub fn delete(&self, key: &[u8]) -> Result<()> {
        self.db
            .delete(key)
            .context("Failed to delete key from RocksDB")
    }

    /// Check if key exists
    pub fn exists(&self, key: &[u8]) -> Result<bool> {
        Ok(self.get(key)?.is_some())
    }

    /// Scan all keys with a given prefix
    ///
    /// Returns an iterator of (key, value) pairs where key starts with `prefix`.
    /// Keys are returned in lexicographic order.
    ///
    /// # Arguments
    /// * `prefix` - The key prefix to scan for
    ///
    /// # Example
    /// ```ignore
    /// // Get all audit events
    /// for (key, value) in store.prefix_scan(b"audit:")? {
    ///     // Process each event
    /// }
    /// ```
    pub fn prefix_scan(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        let mut results = Vec::new();

        // Use RocksDB prefix iterator for efficient scanning
        let iter = self.db.prefix_iterator(prefix);

        for item in iter {
            let (key, value) = item.context("Failed to read from RocksDB iterator")?;

            // Verify the key still matches our prefix (safety check)
            if !key.starts_with(prefix) {
                break;
            }

            results.push((key.to_vec(), value.to_vec()));
        }

        Ok(results)
    }

    /// Scan keys with prefix in a specific range
    ///
    /// Scans keys that start with `prefix` and are >= `start_key`.
    /// Useful for time-based queries where keys include timestamps.
    ///
    /// # Arguments
    /// * `start_key` - The key to start scanning from (inclusive)
    /// * `prefix` - The prefix that all returned keys must start with
    ///
    /// # Example
    /// ```ignore
    /// // Get audit events from a specific timestamp
    /// let start_key = format!("audit:{}", timestamp_millis);
    /// for (key, value) in store.prefix_scan_from(start_key.as_bytes(), b"audit:")? {
    ///     // Process events >= timestamp
    /// }
    /// ```
    pub fn prefix_scan_from(
        &self,
        start_key: &[u8],
        prefix: &[u8],
    ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        let mut results = Vec::new();

        // Start iterator from the specified key
        let iter = self
            .db
            .iterator(IteratorMode::From(start_key, rocksdb::Direction::Forward));

        for item in iter {
            let (key, value) = item.context("Failed to read from RocksDB iterator")?;

            // Verify the key still matches our prefix
            if !key.starts_with(prefix) {
                break;
            }

            results.push((key.to_vec(), value.to_vec()));
        }

        Ok(results)
    }

    /// Delete all keys with a given prefix
    ///
    /// Useful for bulk deletions (e.g., cleanup of old audit events).
    ///
    /// # Arguments
    /// * `prefix` - The key prefix to delete
    ///
    /// # Returns
    /// Number of keys deleted
    pub fn delete_prefix(&self, prefix: &[u8]) -> Result<usize> {
        let keys_to_delete = self.prefix_scan(prefix)?;
        let count = keys_to_delete.len();

        for (key, _) in keys_to_delete {
            self.delete(&key)?;
        }

        Ok(count)
    }

    /// Get direct access to underlying RocksDB instance
    ///
    /// # Warning
    /// This is intended for advanced use cases. Most operations should use
    /// the provided methods instead.
    pub fn db(&self) -> &Arc<DB> {
        &self.db
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kv_store_operations() {
        let store = KvStore::new_in_memory().unwrap();

        // Put and get
        store.put(b"key1", b"value1").unwrap();
        let value = store.get(b"key1").unwrap().unwrap();
        assert_eq!(value, b"value1");

        // Exists
        assert!(store.exists(b"key1").unwrap());
        assert!(!store.exists(b"key2").unwrap());

        // Delete
        store.delete(b"key1").unwrap();
        assert!(!store.exists(b"key1").unwrap());
    }

    #[test]
    fn test_prefix_scan() {
        let store = KvStore::new_in_memory().unwrap();

        // Insert multiple keys with different prefixes
        store.put(b"user:alice", b"Alice data").unwrap();
        store.put(b"user:bob", b"Bob data").unwrap();
        store.put(b"user:charlie", b"Charlie data").unwrap();
        store.put(b"audit:event1", b"Event 1").unwrap();
        store.put(b"audit:event2", b"Event 2").unwrap();

        // Scan user prefix
        let user_results = store.prefix_scan(b"user:").unwrap();
        assert_eq!(user_results.len(), 3);
        assert!(user_results.iter().all(|(k, _)| k.starts_with(b"user:")));

        // Scan audit prefix
        let audit_results = store.prefix_scan(b"audit:").unwrap();
        assert_eq!(audit_results.len(), 2);
        assert!(audit_results.iter().all(|(k, _)| k.starts_with(b"audit:")));

        // Scan non-existent prefix
        let empty_results = store.prefix_scan(b"nonexistent:").unwrap();
        assert_eq!(empty_results.len(), 0);
    }

    #[test]
    fn test_prefix_scan_from() {
        let store = KvStore::new_in_memory().unwrap();

        // Insert time-sorted keys (using timestamp in key)
        store.put(b"audit:1000:evt1", b"Event 1").unwrap();
        store.put(b"audit:2000:evt2", b"Event 2").unwrap();
        store.put(b"audit:3000:evt3", b"Event 3").unwrap();
        store.put(b"audit:4000:evt4", b"Event 4").unwrap();

        // Scan from timestamp 2000
        let results = store.prefix_scan_from(b"audit:2000", b"audit:").unwrap();
        assert_eq!(results.len(), 3); // Should get 2000, 3000, 4000

        // Scan from timestamp 3500 (between 3000 and 4000)
        let results = store.prefix_scan_from(b"audit:3500", b"audit:").unwrap();
        assert_eq!(results.len(), 1); // Should only get 4000
    }

    #[test]
    fn test_delete_prefix() {
        let store = KvStore::new_in_memory().unwrap();

        // Insert multiple keys
        store.put(b"temp:key1", b"value1").unwrap();
        store.put(b"temp:key2", b"value2").unwrap();
        store.put(b"temp:key3", b"value3").unwrap();
        store.put(b"keep:key1", b"keeper").unwrap();

        // Verify all exist
        assert_eq!(store.prefix_scan(b"temp:").unwrap().len(), 3);
        assert!(store.exists(b"keep:key1").unwrap());

        // Delete all temp keys
        let deleted_count = store.delete_prefix(b"temp:").unwrap();
        assert_eq!(deleted_count, 3);

        // Verify temp keys are gone but keep key remains
        assert_eq!(store.prefix_scan(b"temp:").unwrap().len(), 0);
        assert!(store.exists(b"keep:key1").unwrap());
    }
}
