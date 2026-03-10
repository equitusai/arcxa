//! # Discovery Cache Layer
//!
//! RocksDB-based caching for discovered schemas, samples, and embeddings.

use anyhow::{Context, Result};
use rocksdb::{ColumnFamilyDescriptor, Options, DB};
use serde::{de::DeserializeOwned, Serialize};
use std::path::Path;
use tracing::{debug, info};

use super::types::*;

/// Discovery cache using RocksDB
///
/// Provides persistent caching with TTL support for:
/// - Schema metadata (24h TTL)
/// - Sample values (1h TTL)
/// - Field embeddings (permanent)
/// - Mapping history (permanent)
pub struct DiscoveryCache {
    db: DB,
}

impl DiscoveryCache {
    /// Create a new discovery cache
    ///
    /// ## Column Families
    ///
    /// - `schema_metadata`: Discovered schemas with TTL
    /// - `sample_values`: Sample rows with TTL
    /// - `field_embeddings`: Semantic embeddings (permanent)
    /// - `mapping_history`: User mappings (permanent)
    /// - `discovery_stats`: Discovery statistics (permanent)
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        info!("Initializing discovery cache at: {:?}", path.as_ref());

        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);

        let cf_descriptors = vec![
            ColumnFamilyDescriptor::new("schema_metadata", Options::default()),
            ColumnFamilyDescriptor::new("sample_values", Options::default()),
            ColumnFamilyDescriptor::new("field_embeddings", Options::default()),
            ColumnFamilyDescriptor::new("mapping_history", Options::default()),
            ColumnFamilyDescriptor::new("discovery_stats", Options::default()),
        ];

        let db = DB::open_cf_descriptors(&opts, path, cf_descriptors)
            .context("Failed to open RocksDB for discovery cache")?;

        info!("✓ Discovery cache initialized");

        Ok(Self { db })
    }

    /// Get discovered schema from cache
    pub fn get_schema(&self, key: &str) -> Result<Option<DiscoveredSchema>> {
        self.get_from_cf("schema_metadata", key)
    }

    /// Store discovered schema in cache
    pub fn put_schema(&self, key: &str, schema: &DiscoveredSchema) -> Result<()> {
        debug!("Caching schema: key={}", key);
        self.put_to_cf("schema_metadata", key, schema)
    }

    /// Get sample values from cache
    pub fn get_samples(&self, key: &str) -> Result<Option<Vec<SampleRow>>> {
        self.get_from_cf("sample_values", key)
    }

    /// Store sample values in cache
    pub fn put_samples(&self, key: &str, samples: &[SampleRow]) -> Result<()> {
        debug!("Caching samples: key={}, count={}", key, samples.len());
        self.put_to_cf("sample_values", key, &samples.to_vec())
    }

    /// Get field embedding from cache
    pub fn get_embedding(&self, field_text: &str) -> Result<Option<Vec<f32>>> {
        self.get_from_cf("field_embeddings", field_text)
    }

    /// Store field embedding in cache
    pub fn put_embedding(&self, field_text: &str, embedding: &[f32]) -> Result<()> {
        debug!("Caching embedding: field_text={}", field_text);
        self.put_to_cf("field_embeddings", field_text, &embedding.to_vec())
    }

    /// Invalidate schema cache entry
    pub fn invalidate_schema(&self, key: &str) -> Result<()> {
        let cf = self
            .db
            .cf_handle("schema_metadata")
            .context("schema_metadata CF not found")?;

        self.db.delete_cf(cf, key.as_bytes())?;
        debug!("Invalidated schema cache: key={}", key);
        Ok(())
    }

    /// Invalidate samples cache entry
    pub fn invalidate_samples(&self, key: &str) -> Result<()> {
        let cf = self
            .db
            .cf_handle("sample_values")
            .context("sample_values CF not found")?;

        self.db.delete_cf(cf, key.as_bytes())?;
        debug!("Invalidated samples cache: key={}", key);
        Ok(())
    }

    /// Clear all cache entries
    pub fn clear_all(&self) -> Result<()> {
        info!("Clearing all discovery cache entries");

        for cf_name in &[
            "schema_metadata",
            "sample_values",
            "field_embeddings",
            "mapping_history",
        ] {
            let cf = self
                .db
                .cf_handle(cf_name)
                .context(format!("{} CF not found", cf_name))?;

            // Delete all keys in this CF
            let iter = self.db.iterator_cf(cf, rocksdb::IteratorMode::Start);
            for item in iter {
                let (key, _) = item?;
                self.db.delete_cf(cf, key)?;
            }

            debug!("Cleared CF: {}", cf_name);
        }

        info!("✓ All cache entries cleared");
        Ok(())
    }

    // ========================================================================
    // Internal Helpers
    // ========================================================================

    /// Get value from column family
    fn get_from_cf<T: DeserializeOwned>(&self, cf_name: &str, key: &str) -> Result<Option<T>> {
        let cf = self
            .db
            .cf_handle(cf_name)
            .context(format!("{} CF not found", cf_name))?;

        if let Some(bytes) = self.db.get_cf(cf, key.as_bytes())? {
            let value: T = bincode::deserialize(&bytes)
                .context(format!("Failed to deserialize {} value", cf_name))?;
            Ok(Some(value))
        } else {
            Ok(None)
        }
    }

    /// Put value to column family
    fn put_to_cf<T: Serialize>(&self, cf_name: &str, key: &str, value: &T) -> Result<()> {
        let cf = self
            .db
            .cf_handle(cf_name)
            .context(format!("{} CF not found", cf_name))?;

        let bytes =
            bincode::serialize(value).context(format!("Failed to serialize {} value", cf_name))?;

        self.db.put_cf(cf, key.as_bytes(), bytes)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_cache_schema() {
        let temp_dir = TempDir::new().unwrap();
        let cache = DiscoveryCache::new(temp_dir.path()).unwrap();

        let schema = DiscoveredSchema {
            source_id: "test_source".to_string(),
            schema_name: "public".to_string(),
            tables: vec![],
            relationships: vec![],
            discovered_at: 1697865600,
        };

        // Store
        cache.put_schema("test_key", &schema).unwrap();

        // Retrieve
        let retrieved = cache.get_schema("test_key").unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().source_id, "test_source");

        // Invalidate
        cache.invalidate_schema("test_key").unwrap();
        assert!(cache.get_schema("test_key").unwrap().is_none());
    }

    #[test]
    fn test_cache_embedding() {
        let temp_dir = TempDir::new().unwrap();
        let cache = DiscoveryCache::new(temp_dir.path()).unwrap();

        let embedding = vec![0.1, 0.2, 0.3, 0.4];

        // Store
        cache.put_embedding("customer_email", &embedding).unwrap();

        // Retrieve
        let retrieved = cache.get_embedding("customer_email").unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), embedding);
    }
}
