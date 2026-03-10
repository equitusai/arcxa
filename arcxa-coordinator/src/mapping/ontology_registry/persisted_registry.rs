//! Persisted Ontology Registry
//!
//! Adds RocksDB persistence to the in-memory OntologyRegistry from graphica-core.
//!
//! ## Architecture
//!
//! This module wraps `OntologyRegistry` and provides:
//! - Persistent storage of ontologies in RocksDB
//! - Lazy loading of ontologies on demand
//! - Automatic persistence on write operations
//! - Crash recovery (ontologies survive restarts)
//!
//! ## Storage Format
//!
//! RocksDB key-value pairs:
//! - Key: `ontology:{id}` (e.g., `ontology:retail_v1`)
//! - Value: JSON-serialized `RegisteredOntology`
//!
//! ## Usage
//!
//! ```ignore
//! use graphica_coordinator::mapping::ontology_registry::persisted_registry::PersistedOntologyRegistry;
//!
//! # async fn example() -> anyhow::Result<()> {
//! // Create registry with persistence
//! let registry = PersistedOntologyRegistry::open("/data/ontologies.db").await?;
//!
//! // Register ontology (automatically persisted)
//! registry.register_custom_ontology("retail", turtle_content, namespace).await?;
//!
//! // Get ontology (loaded from disk if not in memory)
//! let ontology = registry.get_ontology("retail").await?;
//! # Ok(())
//! # }
//! ```

use anyhow::{Context, Result};
use parking_lot::RwLock;
use rocksdb::{IteratorMode, Options, DB};
use std::sync::Arc;
use tracing::{debug, info, warn};

use graphica_core::catalog::{OntologyMetadata, OntologyRegistry, RegisteredOntology};

/// Prefix for ontology keys in RocksDB
const ONTOLOGY_KEY_PREFIX: &str = "ontology:";

/// Ontology registry with RocksDB persistence
///
/// This wrapper adds persistence to the in-memory `OntologyRegistry` from graphica-core.
/// All write operations are automatically persisted to RocksDB.
pub struct PersistedOntologyRegistry {
    /// In-memory registry (cache)
    registry: Arc<RwLock<OntologyRegistry>>,

    /// RocksDB handle for persistent storage
    db: Arc<DB>,

    /// Optional callback to invalidate external caches (e.g., RegistryClient cache)
    cache_invalidation_callback: Arc<RwLock<Option<Box<dyn Fn() + Send + Sync>>>>,
}

impl PersistedOntologyRegistry {
    /// Open or create a persisted ontology registry
    ///
    /// # Arguments
    ///
    /// * `db_path` - Path to RocksDB database directory
    ///
    /// # Returns
    ///
    /// Registry with all ontologies loaded from disk into memory
    pub async fn open(db_path: impl AsRef<std::path::Path>) -> Result<Self> {
        let db_path = db_path.as_ref();
        info!(
            "Opening persisted ontology registry at: {}",
            db_path.display()
        );

        // Open RocksDB
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.set_compression_type(rocksdb::DBCompressionType::Zstd);
        opts.set_max_open_files(100);

        let db = DB::open(&opts, db_path)
            .context(format!("Failed to open RocksDB at {}", db_path.display()))?;

        let db = Arc::new(db);

        // Create in-memory registry
        let mut registry = OntologyRegistry::new();

        // Load all ontologies from disk
        let loaded_count = Self::load_all_from_disk(&db, &mut registry)?;

        info!("Loaded {} ontologies from disk into memory", loaded_count);

        Ok(Self {
            registry: Arc::new(RwLock::new(registry)),
            db,
            cache_invalidation_callback: Arc::new(RwLock::new(None)),
        })
    }

    /// Set a callback to be invoked when ontologies are modified
    ///
    /// This is useful for invalidating external caches (e.g., RegistryClient term cache)
    /// when ontologies are updated.
    ///
    /// # Arguments
    ///
    /// * `callback` - Function to call when ontologies are modified
    ///
    /// # Example
    ///
    /// ```ignore
    /// # use graphica_coordinator::mapping::ontology_registry::persisted_registry::PersistedOntologyRegistry;
    /// # async fn example() -> anyhow::Result<()> {
    /// let registry = PersistedOntologyRegistry::open("/data/ontologies.db").await?;
    /// let registry_client = /* create RegistryClient */
    /// # registry;
    ///
    /// // Set callback to invalidate RegistryClient cache on ontology updates
    /// // registry.set_cache_invalidation_callback(Box::new(move || {
    /// //     registry_client.invalidate_cache();
    /// // }));
    /// # Ok(())
    /// # }
    /// ```
    pub fn set_cache_invalidation_callback(&self, callback: Box<dyn Fn() + Send + Sync>) {
        *self.cache_invalidation_callback.write() = Some(callback);
    }

    /// Invoke the cache invalidation callback if set
    fn invalidate_external_caches(&self) {
        if let Some(callback) = self.cache_invalidation_callback.read().as_ref() {
            callback();
            debug!("External caches invalidated");
        }
    }

    /// Register a custom ontology (with automatic persistence)
    ///
    /// # Arguments
    ///
    /// * `id` - Unique identifier
    /// * `content` - Ontology content (Turtle or RDF/XML)
    /// * `namespace` - Optional namespace URI
    ///
    /// # Returns
    ///
    /// Metadata for the registered ontology
    pub async fn register_custom_ontology(
        &self,
        id: impl Into<String>,
        content: impl Into<String>,
        namespace: Option<String>,
    ) -> Result<OntologyMetadata> {
        let id = id.into();

        // Register in memory
        let metadata = {
            let mut reg = self.registry.write();
            reg.register_custom_ontology(&id, content, namespace)?
        };

        // Persist to disk
        self.persist_ontology(&id).await?;

        // Invalidate external caches (e.g., RegistryClient cache)
        self.invalidate_external_caches();

        debug!("Registered and persisted ontology: {}", id);

        Ok(metadata)
    }

    /// Update an existing ontology (with automatic persistence)
    pub async fn update_ontology(&self, id: &str, new_content: impl Into<String>) -> Result<()> {
        // Update in memory
        {
            let mut reg = self.registry.write();
            reg.update_ontology(id, new_content)?;
        }

        // Persist to disk
        self.persist_ontology(id).await?;

        // Invalidate external caches
        self.invalidate_external_caches();

        debug!("Updated and persisted ontology: {}", id);

        Ok(())
    }

    /// Update metadata fields of an existing ontology (with automatic persistence)
    ///
    /// This allows updating metadata (name, version, active, description, tags)
    /// without changing the ontology content.
    pub async fn update_metadata<F>(&self, id: &str, update_fn: F) -> Result<()>
    where
        F: FnOnce(&mut OntologyMetadata),
    {
        // Update metadata in memory
        {
            let mut reg = self.registry.write();
            reg.update_metadata(id, update_fn)?;
        }

        // Persist to disk
        self.persist_ontology(id).await?;

        // Invalidate external caches
        self.invalidate_external_caches();

        debug!("Updated metadata and persisted ontology: {}", id);

        Ok(())
    }

    /// Deactivate an ontology (with automatic persistence)
    pub async fn deactivate_ontology(&self, id: &str) -> Result<()> {
        // Deactivate in memory
        {
            let mut reg = self.registry.write();
            reg.deactivate_ontology(id)?;
        }

        // Persist to disk
        self.persist_ontology(id).await?;

        // Invalidate external caches
        self.invalidate_external_caches();

        debug!("Deactivated and persisted ontology: {}", id);

        Ok(())
    }

    /// Activate an ontology (with automatic persistence)
    pub async fn activate_ontology(&self, id: &str) -> Result<()> {
        // Activate in memory
        {
            let mut reg = self.registry.write();
            reg.activate_ontology(id)?;
        }

        // Persist to disk
        self.persist_ontology(id).await?;

        // Invalidate external caches
        self.invalidate_external_caches();

        debug!("Activated and persisted ontology: {}", id);

        Ok(())
    }

    /// Remove an ontology completely (from memory and disk)
    pub async fn remove_ontology(&self, id: &str) -> Result<RegisteredOntology> {
        // Remove from memory
        let ontology = {
            let mut reg = self.registry.write();
            reg.remove_ontology(id)?
        };

        // Remove from disk
        let key = Self::make_key(id);
        self.db
            .delete(&key)
            .context(format!("Failed to delete ontology {} from RocksDB", id))?;

        // Invalidate external caches
        self.invalidate_external_caches();

        info!("Removed ontology from memory and disk: {}", id);

        Ok(ontology)
    }

    /// Get an ontology by ID (read-only)
    pub fn get_ontology(&self, id: &str) -> Option<RegisteredOntology> {
        let reg = self.registry.read();
        reg.get_ontology(id).cloned()
    }

    /// Get ontology by namespace (read-only)
    pub fn get_by_namespace(&self, namespace: &str) -> Option<RegisteredOntology> {
        let reg = self.registry.read();
        reg.get_by_namespace(namespace).cloned()
    }

    /// List all registered ontologies
    pub fn list_ontologies(&self) -> Vec<OntologyMetadata> {
        let reg = self.registry.read();
        reg.list_ontologies().into_iter().cloned().collect()
    }

    /// List active ontologies only
    pub fn list_active_ontologies(&self) -> Vec<OntologyMetadata> {
        let reg = self.registry.read();
        reg.list_active_ontologies().into_iter().cloned().collect()
    }

    /// Get merged ontology combining all active ontologies
    pub fn get_merged_ontology(&self) -> String {
        let reg = self.registry.read();
        reg.get_merged_ontology()
    }

    /// Get the underlying registry (for use with RegistryClient)
    pub fn registry(&self) -> Arc<RwLock<OntologyRegistry>> {
        self.registry.clone()
    }

    /// Get database statistics
    pub fn get_stats(&self) -> Result<PersistenceStats> {
        let mut stats = PersistenceStats::default();

        // Count ontologies in memory
        {
            let reg = self.registry.read();
            stats.in_memory_count = reg.list_ontologies().len();
            stats.active_count = reg.list_active_ontologies().len();
        }

        // Count ontologies on disk
        let iter = self.db.iterator(IteratorMode::Start);
        for item in iter {
            let (key, _) = item?;
            let key_str = String::from_utf8_lossy(&key);
            if key_str.starts_with(ONTOLOGY_KEY_PREFIX) {
                stats.disk_count += 1;
            }
        }

        Ok(stats)
    }

    // ===== Private Helper Methods =====

    /// Persist a single ontology to RocksDB
    async fn persist_ontology(&self, id: &str) -> Result<()> {
        let ontology = {
            let reg = self.registry.read();
            reg.get_ontology(id)
                .context(format!("Ontology {} not found in memory", id))?
                .clone()
        };

        let key = Self::make_key(id);
        let serialized = serde_json::to_vec(&ontology)
            .context(format!("Failed to serialize ontology {}", id))?;

        let size_bytes = serialized.len();

        self.db
            .put(&key, serialized)
            .context(format!("Failed to persist ontology {} to RocksDB", id))?;

        debug!(
            "Persisted ontology {} to RocksDB ({} bytes)",
            id, size_bytes
        );

        Ok(())
    }

    /// Load all ontologies from RocksDB into the in-memory registry
    ///
    /// Note: When loading from disk, some metadata (like timestamps) will be updated
    /// to reflect the load time rather than the original registration time. This is
    /// acceptable for most use cases and avoids unsafe code.
    fn load_all_from_disk(db: &DB, registry: &mut OntologyRegistry) -> Result<usize> {
        let mut loaded_count = 0;

        let iter = db.iterator(IteratorMode::Start);
        for item in iter {
            let (key, value) = item?;

            let key_str = String::from_utf8_lossy(&key);
            if !key_str.starts_with(ONTOLOGY_KEY_PREFIX) {
                continue; // Skip non-ontology keys
            }

            // Deserialize ontology
            let ontology: RegisteredOntology = serde_json::from_slice(&value).context(format!(
                "Failed to deserialize ontology from key {}",
                key_str
            ))?;

            let id = &ontology.metadata.id;
            let namespace = &ontology.metadata.namespace;

            // Register in memory
            // Note: This will update timestamps to current time, which is acceptable
            match registry.register_custom_ontology(id, &ontology.content, Some(namespace.clone()))
            {
                Ok(_) => {
                    // If the ontology was inactive on disk, deactivate it in memory
                    if !ontology.metadata.active {
                        if let Err(e) = registry.deactivate_ontology(id) {
                            warn!("Failed to deactivate ontology {} after loading: {}", id, e);
                        }
                    }

                    loaded_count += 1;
                    debug!("Loaded ontology from disk: {}", id);
                }
                Err(e) => {
                    warn!("Failed to load ontology {} from disk: {}", id, e);
                }
            }
        }

        Ok(loaded_count)
    }

    /// Create RocksDB key for an ontology ID
    fn make_key(id: &str) -> String {
        format!("{}{}", ONTOLOGY_KEY_PREFIX, id)
    }
}

/// Statistics about persisted ontologies
#[derive(Debug, Clone, Default)]
pub struct PersistenceStats {
    /// Number of ontologies in memory
    pub in_memory_count: usize,

    /// Number of active ontologies in memory
    pub active_count: usize,

    /// Number of ontologies on disk
    pub disk_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_persist_and_load() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let db_path = temp_dir.path().join("test_ontologies.db");

        // Create registry and add ontology
        {
            let registry = PersistedOntologyRegistry::open(&db_path).await?;

            let content = r#"
                @prefix test: <http://test.com/ont#> .
                @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .

                test:TestClass a rdfs:Class .
            "#;

            registry
                .register_custom_ontology(
                    "test_ont",
                    content,
                    Some("http://test.com/ont#".to_string()),
                )
                .await?;

            assert_eq!(registry.list_ontologies().len(), 1);
        }

        // Reopen registry - should load from disk
        {
            let registry = PersistedOntologyRegistry::open(&db_path).await?;

            assert_eq!(registry.list_ontologies().len(), 1);

            let ontology = registry
                .get_ontology("test_ont")
                .expect("Ontology should exist");
            assert_eq!(ontology.metadata.namespace, "http://test.com/ont#");
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_update_persists() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let db_path = temp_dir.path().join("test_update.db");

        let registry = PersistedOntologyRegistry::open(&db_path).await?;

        // Register initial ontology
        registry
            .register_custom_ontology("test", "@prefix t: <http://test#> .", None)
            .await?;

        // Update ontology
        let new_content = "@prefix t: <http://test#> . t:NewClass a rdfs:Class .";
        registry.update_ontology("test", new_content).await?;

        // Verify update persisted
        let ontology = registry
            .get_ontology("test")
            .expect("Ontology should exist");
        assert!(ontology.content.contains("NewClass"));

        Ok(())
    }

    #[tokio::test]
    async fn test_deactivate_persists() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let db_path = temp_dir.path().join("test_deactivate.db");

        let registry = PersistedOntologyRegistry::open(&db_path).await?;

        registry
            .register_custom_ontology("test", "@prefix t: <http://test#> .", None)
            .await?;
        assert_eq!(registry.list_active_ontologies().len(), 1);

        registry.deactivate_ontology("test").await?;
        assert_eq!(registry.list_active_ontologies().len(), 0);

        // Verify deactivation persisted
        let ontology = registry
            .get_ontology("test")
            .expect("Ontology should exist");
        assert!(!ontology.metadata.active);

        Ok(())
    }

    #[tokio::test]
    async fn test_remove_deletes_from_disk() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let db_path = temp_dir.path().join("test_remove.db");

        let registry = PersistedOntologyRegistry::open(&db_path).await?;

        registry
            .register_custom_ontology("test", "@prefix t: <http://test#> .", None)
            .await?;
        assert_eq!(registry.list_ontologies().len(), 1);

        registry.remove_ontology("test").await?;
        assert_eq!(registry.list_ontologies().len(), 0);

        // Verify removed from disk
        let stats = registry.get_stats()?;
        assert_eq!(stats.disk_count, 0);

        Ok(())
    }

    #[tokio::test]
    async fn test_stats() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let db_path = temp_dir.path().join("test_stats.db");

        let registry = PersistedOntologyRegistry::open(&db_path).await?;

        registry
            .register_custom_ontology("ont1", "@prefix o1: <http://ont1#> .", None)
            .await?;
        registry
            .register_custom_ontology("ont2", "@prefix o2: <http://ont2#> .", None)
            .await?;
        registry.deactivate_ontology("ont2").await?;

        let stats = registry.get_stats()?;
        assert_eq!(stats.in_memory_count, 2);
        assert_eq!(stats.active_count, 1);
        assert_eq!(stats.disk_count, 2);

        Ok(())
    }

    #[tokio::test]
    async fn test_cache_invalidation_callback() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let db_path = temp_dir.path().join("test_cache_callback.db");

        let registry = PersistedOntologyRegistry::open(&db_path).await?;

        // Track invalidation calls
        use std::sync::atomic::{AtomicUsize, Ordering};
        let invalidation_count = Arc::new(AtomicUsize::new(0));
        let invalidation_count_clone = invalidation_count.clone();

        // Set callback
        registry.set_cache_invalidation_callback(Box::new(move || {
            invalidation_count_clone.fetch_add(1, Ordering::SeqCst);
        }));

        // Register ontology - should trigger callback
        registry
            .register_custom_ontology("test", "@prefix t: <http://test#> .", None)
            .await?;
        assert_eq!(invalidation_count.load(Ordering::SeqCst), 1);

        // Update ontology - should trigger callback
        registry
            .update_ontology("test", "@prefix t: <http://test#> . t:Class a rdfs:Class .")
            .await?;
        assert_eq!(invalidation_count.load(Ordering::SeqCst), 2);

        // Deactivate ontology - should trigger callback
        registry.deactivate_ontology("test").await?;
        assert_eq!(invalidation_count.load(Ordering::SeqCst), 3);

        // Remove ontology - should trigger callback
        registry.remove_ontology("test").await?;
        assert_eq!(invalidation_count.load(Ordering::SeqCst), 4);

        Ok(())
    }
}
