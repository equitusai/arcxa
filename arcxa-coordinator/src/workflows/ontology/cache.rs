//! Schema Cache Module
//!
//! Provides LRU caching for entity definitions, table schemas, and DDL statements
//! to optimize ontology-driven data loading workflows.

use anyhow::Result;
use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use super::types::*;

/// Trait for caching schema metadata and DDL
#[async_trait::async_trait]
pub trait SchemaCache: Send + Sync {
    /// Get cached entity definition
    async fn get_entity_def(&self, entity_uri: &str) -> Option<EntityDefinition>;

    /// Cache entity definition
    async fn cache_entity_def(&self, entity_uri: String, def: EntityDefinition);

    /// Get cached table schemas for an entity
    async fn get_table_schemas(&self, entity_uri: &str) -> Option<Vec<TableSchema>>;

    /// Cache table schemas for an entity
    async fn cache_table_schemas(&self, entity_uri: String, schemas: Vec<TableSchema>);

    /// Get cached DDL for a table
    async fn get_ddl(&self, table_name: &str) -> Option<String>;

    /// Cache DDL for a table
    async fn cache_ddl(&self, table_name: String, ddl: String);

    /// Clear all caches
    async fn clear(&self);

    /// Get cache statistics
    async fn statistics(&self) -> CacheStatistics;
}

/// Cache statistics for monitoring
#[derive(Debug, Clone)]
pub struct CacheStatistics {
    /// Total entity definition lookups
    pub entity_lookups: u64,
    /// Entity definition cache hits
    pub entity_hits: u64,
    /// Total table schema lookups
    pub schema_lookups: u64,
    /// Table schema cache hits
    pub schema_hits: u64,
    /// Total DDL lookups
    pub ddl_lookups: u64,
    /// DDL cache hits
    pub ddl_hits: u64,
    /// Current entity cache size
    pub entity_cache_size: usize,
    /// Current schema cache size
    pub schema_cache_size: usize,
    /// Current DDL cache size
    pub ddl_cache_size: usize,
}

impl CacheStatistics {
    /// Calculate overall hit rate
    pub fn overall_hit_rate(&self) -> f64 {
        let total_lookups = self.entity_lookups + self.schema_lookups + self.ddl_lookups;
        let total_hits = self.entity_hits + self.schema_hits + self.ddl_hits;

        if total_lookups == 0 {
            0.0
        } else {
            (total_hits as f64 / total_lookups as f64) * 100.0
        }
    }

    /// Calculate entity cache hit rate
    pub fn entity_hit_rate(&self) -> f64 {
        if self.entity_lookups == 0 {
            0.0
        } else {
            (self.entity_hits as f64 / self.entity_lookups as f64) * 100.0
        }
    }

    /// Calculate schema cache hit rate
    pub fn schema_hit_rate(&self) -> f64 {
        if self.schema_lookups == 0 {
            0.0
        } else {
            (self.schema_hits as f64 / self.schema_lookups as f64) * 100.0
        }
    }

    /// Calculate DDL cache hit rate
    pub fn ddl_hit_rate(&self) -> f64 {
        if self.ddl_lookups == 0 {
            0.0
        } else {
            (self.ddl_hits as f64 / self.ddl_lookups as f64) * 100.0
        }
    }
}

/// Configuration for LRU schema cache
#[derive(Debug, Clone)]
pub struct LruCacheConfig {
    /// Maximum number of entity definitions to cache
    pub max_entities: usize,
    /// Maximum number of table schema sets to cache
    pub max_schemas: usize,
    /// Maximum number of DDL statements to cache
    pub max_ddl: usize,
}

impl Default for LruCacheConfig {
    fn default() -> Self {
        Self {
            max_entities: 1000,
            max_schemas: 5000,
            max_ddl: 5000,
        }
    }
}

/// LRU-based schema cache implementation
///
/// Thread-safe implementation using tokio::sync::RwLock for concurrent access.
/// Provides separate LRU caches for entity definitions, table schemas, and DDL.
pub struct LruSchemaCache {
    /// Cache for entity definitions (entity_uri -> EntityDefinition)
    entity_cache: RwLock<LruCache<String, EntityDefinition>>,

    /// Cache for table schemas (entity_uri -> Vec<TableSchema>)
    schema_cache: RwLock<LruCache<String, Vec<TableSchema>>>,

    /// Cache for DDL statements (table_name -> DDL)
    ddl_cache: RwLock<LruCache<String, String>>,

    /// Statistics tracker
    stats: RwLock<CacheStatistics>,
}

impl LruSchemaCache {
    /// Create a new LRU schema cache with default configuration
    pub fn new() -> Self {
        Self::with_config(LruCacheConfig::default())
    }

    /// Create a new LRU schema cache with custom configuration
    pub fn with_config(config: LruCacheConfig) -> Self {
        let entity_cap =
            NonZeroUsize::new(config.max_entities).unwrap_or(NonZeroUsize::new(1000).unwrap());
        let schema_cap =
            NonZeroUsize::new(config.max_schemas).unwrap_or(NonZeroUsize::new(5000).unwrap());
        let ddl_cap = NonZeroUsize::new(config.max_ddl).unwrap_or(NonZeroUsize::new(5000).unwrap());

        info!(
            "Initializing LRU schema cache: max_entities={}, max_schemas={}, max_ddl={}",
            entity_cap, schema_cap, ddl_cap
        );

        Self {
            entity_cache: RwLock::new(LruCache::new(entity_cap)),
            schema_cache: RwLock::new(LruCache::new(schema_cap)),
            ddl_cache: RwLock::new(LruCache::new(ddl_cap)),
            stats: RwLock::new(CacheStatistics {
                entity_lookups: 0,
                entity_hits: 0,
                schema_lookups: 0,
                schema_hits: 0,
                ddl_lookups: 0,
                ddl_hits: 0,
                entity_cache_size: 0,
                schema_cache_size: 0,
                ddl_cache_size: 0,
            }),
        }
    }

    /// Update cache size statistics
    async fn update_cache_sizes(&self) {
        let entity_size = self.entity_cache.read().await.len();
        let schema_size = self.schema_cache.read().await.len();
        let ddl_size = self.ddl_cache.read().await.len();

        let mut stats = self.stats.write().await;
        stats.entity_cache_size = entity_size;
        stats.schema_cache_size = schema_size;
        stats.ddl_cache_size = ddl_size;
    }
}

impl Default for LruSchemaCache {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl SchemaCache for LruSchemaCache {
    async fn get_entity_def(&self, entity_uri: &str) -> Option<EntityDefinition> {
        let mut cache = self.entity_cache.write().await;
        let mut stats = self.stats.write().await;

        stats.entity_lookups += 1;

        if let Some(def) = cache.get(entity_uri) {
            stats.entity_hits += 1;
            debug!("Cache HIT: entity_def for '{}'", entity_uri);
            Some(def.clone())
        } else {
            debug!("Cache MISS: entity_def for '{}'", entity_uri);
            None
        }
    }

    async fn cache_entity_def(&self, entity_uri: String, def: EntityDefinition) {
        let mut cache = self.entity_cache.write().await;
        cache.put(entity_uri.clone(), def);
        drop(cache);

        self.update_cache_sizes().await;

        debug!("Cached entity_def for '{}'", entity_uri);
    }

    async fn get_table_schemas(&self, entity_uri: &str) -> Option<Vec<TableSchema>> {
        let mut cache = self.schema_cache.write().await;
        let mut stats = self.stats.write().await;

        stats.schema_lookups += 1;

        if let Some(schemas) = cache.get(entity_uri) {
            stats.schema_hits += 1;
            debug!("Cache HIT: table_schemas for '{}'", entity_uri);
            Some(schemas.clone())
        } else {
            debug!("Cache MISS: table_schemas for '{}'", entity_uri);
            None
        }
    }

    async fn cache_table_schemas(&self, entity_uri: String, schemas: Vec<TableSchema>) {
        let mut cache = self.schema_cache.write().await;
        cache.put(entity_uri.clone(), schemas);
        drop(cache);

        self.update_cache_sizes().await;

        debug!("Cached table_schemas for '{}'", entity_uri);
    }

    async fn get_ddl(&self, table_name: &str) -> Option<String> {
        let mut cache = self.ddl_cache.write().await;
        let mut stats = self.stats.write().await;

        stats.ddl_lookups += 1;

        if let Some(ddl) = cache.get(table_name) {
            stats.ddl_hits += 1;
            debug!("Cache HIT: DDL for '{}'", table_name);
            Some(ddl.clone())
        } else {
            debug!("Cache MISS: DDL for '{}'", table_name);
            None
        }
    }

    async fn cache_ddl(&self, table_name: String, ddl: String) {
        let mut cache = self.ddl_cache.write().await;
        cache.put(table_name.clone(), ddl);
        drop(cache);

        self.update_cache_sizes().await;

        debug!("Cached DDL for '{}'", table_name);
    }

    async fn clear(&self) {
        let mut entity_cache = self.entity_cache.write().await;
        let mut schema_cache = self.schema_cache.write().await;
        let mut ddl_cache = self.ddl_cache.write().await;

        entity_cache.clear();
        schema_cache.clear();
        ddl_cache.clear();

        drop(entity_cache);
        drop(schema_cache);
        drop(ddl_cache);

        // Reset statistics
        let mut stats = self.stats.write().await;
        *stats = CacheStatistics {
            entity_lookups: 0,
            entity_hits: 0,
            schema_lookups: 0,
            schema_hits: 0,
            ddl_lookups: 0,
            ddl_hits: 0,
            entity_cache_size: 0,
            schema_cache_size: 0,
            ddl_cache_size: 0,
        };

        info!("Schema cache cleared");
    }

    async fn statistics(&self) -> CacheStatistics {
        self.update_cache_sizes().await;
        self.stats.read().await.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_entity_def(uri: &str) -> EntityDefinition {
        EntityDefinition {
            entity_uri: uri.to_string(),
            label: format!("Test Entity {}", uri),
            properties: vec![],
            relationships: vec![],
        }
    }

    fn create_test_table_schema(name: &str) -> TableSchema {
        TableSchema::new(name.to_string())
    }

    #[tokio::test]
    async fn test_entity_cache_hit() {
        let cache = LruSchemaCache::new();
        let entity_uri = "http://example.org/Patient";
        let def = create_test_entity_def(entity_uri);

        // Cache entity
        cache
            .cache_entity_def(entity_uri.to_string(), def.clone())
            .await;

        // Retrieve should be a hit
        let retrieved = cache.get_entity_def(entity_uri).await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().entity_uri, entity_uri);

        // Check statistics
        let stats = cache.statistics().await;
        assert_eq!(stats.entity_lookups, 1);
        assert_eq!(stats.entity_hits, 1);
        assert_eq!(stats.entity_hit_rate(), 100.0);
    }

    #[tokio::test]
    async fn test_entity_cache_miss() {
        let cache = LruSchemaCache::new();
        let entity_uri = "http://example.org/NonExistent";

        // Retrieve without caching should be a miss
        let retrieved = cache.get_entity_def(entity_uri).await;
        assert!(retrieved.is_none());

        // Check statistics
        let stats = cache.statistics().await;
        assert_eq!(stats.entity_lookups, 1);
        assert_eq!(stats.entity_hits, 0);
        assert_eq!(stats.entity_hit_rate(), 0.0);
    }

    #[tokio::test]
    async fn test_schema_cache_hit() {
        let cache = LruSchemaCache::new();
        let entity_uri = "http://example.org/Patient";
        let schemas = vec![
            create_test_table_schema("patients"),
            create_test_table_schema("patient_addresses"),
        ];

        // Cache schemas
        cache
            .cache_table_schemas(entity_uri.to_string(), schemas.clone())
            .await;

        // Retrieve should be a hit
        let retrieved = cache.get_table_schemas(entity_uri).await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().len(), 2);

        // Check statistics
        let stats = cache.statistics().await;
        assert_eq!(stats.schema_lookups, 1);
        assert_eq!(stats.schema_hits, 1);
    }

    #[tokio::test]
    async fn test_ddl_cache() {
        let cache = LruSchemaCache::new();
        let table_name = "patients";
        let ddl = "CREATE TABLE patients (id INTEGER PRIMARY KEY, name VARCHAR(255))";

        // Cache DDL
        cache
            .cache_ddl(table_name.to_string(), ddl.to_string())
            .await;

        // Retrieve should be a hit
        let retrieved = cache.get_ddl(table_name).await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), ddl);

        // Check statistics
        let stats = cache.statistics().await;
        assert_eq!(stats.ddl_lookups, 1);
        assert_eq!(stats.ddl_hits, 1);
    }

    #[tokio::test]
    async fn test_cache_clear() {
        let cache = LruSchemaCache::new();

        // Add items to all caches
        cache
            .cache_entity_def("uri1".to_string(), create_test_entity_def("uri1"))
            .await;
        cache
            .cache_table_schemas("uri1".to_string(), vec![create_test_table_schema("t1")])
            .await;
        cache
            .cache_ddl("table1".to_string(), "CREATE TABLE...".to_string())
            .await;

        // Verify items are cached
        assert!(cache.get_entity_def("uri1").await.is_some());
        assert!(cache.get_table_schemas("uri1").await.is_some());
        assert!(cache.get_ddl("table1").await.is_some());

        // Clear cache
        cache.clear().await;

        // Verify items are gone
        assert!(cache.get_entity_def("uri1").await.is_none());
        assert!(cache.get_table_schemas("uri1").await.is_none());
        assert!(cache.get_ddl("table1").await.is_none());

        // Verify statistics reset
        let stats = cache.statistics().await;
        assert_eq!(stats.entity_cache_size, 0);
        assert_eq!(stats.schema_cache_size, 0);
        assert_eq!(stats.ddl_cache_size, 0);
    }

    #[tokio::test]
    async fn test_lru_eviction() {
        let config = LruCacheConfig {
            max_entities: 2,
            max_schemas: 2,
            max_ddl: 2,
        };
        let cache = LruSchemaCache::with_config(config);

        // Add 3 entities (should evict the first)
        cache
            .cache_entity_def("uri1".to_string(), create_test_entity_def("uri1"))
            .await;
        cache
            .cache_entity_def("uri2".to_string(), create_test_entity_def("uri2"))
            .await;
        cache
            .cache_entity_def("uri3".to_string(), create_test_entity_def("uri3"))
            .await;

        // First should be evicted
        assert!(cache.get_entity_def("uri1").await.is_none());
        assert!(cache.get_entity_def("uri2").await.is_some());
        assert!(cache.get_entity_def("uri3").await.is_some());
    }

    #[tokio::test]
    async fn test_statistics_calculation() {
        let cache = LruSchemaCache::new();

        // Simulate lookups and hits
        cache
            .cache_entity_def("uri1".to_string(), create_test_entity_def("uri1"))
            .await;

        // 3 lookups: 2 hits, 1 miss
        cache.get_entity_def("uri1").await; // hit
        cache.get_entity_def("uri1").await; // hit
        cache.get_entity_def("uri2").await; // miss

        let stats = cache.statistics().await;
        assert_eq!(stats.entity_lookups, 3);
        assert_eq!(stats.entity_hits, 2);
        assert!((stats.entity_hit_rate() - 66.66).abs() < 0.1);
    }

    #[tokio::test]
    async fn test_concurrent_access() {
        let cache = Arc::new(LruSchemaCache::new());
        let entity_uri = "http://example.org/Patient";

        // Spawn multiple tasks
        let mut handles = vec![];

        for i in 0..10 {
            let cache_clone = cache.clone();
            let uri = format!("{}/{}", entity_uri, i);
            let handle = tokio::spawn(async move {
                cache_clone
                    .cache_entity_def(uri.clone(), create_test_entity_def(&uri))
                    .await;
                cache_clone.get_entity_def(&uri).await
            });
            handles.push(handle);
        }

        // Wait for all tasks
        for handle in handles {
            let result = handle.await.unwrap();
            assert!(result.is_some());
        }

        // Verify cache has items
        let stats = cache.statistics().await;
        assert_eq!(stats.entity_cache_size, 10);
    }
}
