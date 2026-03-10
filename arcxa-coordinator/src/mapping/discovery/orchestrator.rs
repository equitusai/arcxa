//! # Discovery Orchestrator
//!
//! Main entry point for intelligent schema discovery.
//! Coordinates:
//! - Schema extraction from data sources
//! - Type inference on discovered columns
//! - Caching for performance
//! - Extractor registry for multi-source support
//!
//! ## Usage
//!
//! ```rust,ignore
//! use graphica_coordinator::mapping::discovery::{
//!     DiscoveryOrchestrator, DiscoveryConfig
//! };
//!
//! let orchestrator = DiscoveryOrchestrator::new("/path/to/cache")?;
//!
//! let config = DiscoveryConfig {
//!     schema_filter: Some("public".to_string()),
//!     table_filter: None,
//!     sample_size: 1000,
//!     cache_ttl_secs: 3600,
//! };
//!
//! let schema = orchestrator.discover_schema(
//!     &source,
//!     &credentials,
//!     config,
//! ).await?;
//! ```

use anyhow::{anyhow, Context, Result};
use std::path::Path;
use tracing::{debug, info, warn};

use graphica_core::catalog::connector::Credentials;
use graphica_core::catalog::types::DataSource;

use super::cache::DiscoveryCache;
use super::extractors::{ExtractorRegistry, SchemaExtractor};
use super::inference::TypeInferenceEngine;
use super::types::*;

/// Discovery orchestrator
///
/// Main entry point for schema discovery. Coordinates:
/// 1. Cache lookup for fast responses
/// 2. Extractor selection based on source type
/// 3. Schema metadata extraction
/// 4. Sample value extraction
/// 5. Type inference on columns
/// 6. Cache storage for future requests
pub struct DiscoveryOrchestrator {
    /// Cache for discovered schemas
    cache: DiscoveryCache,

    /// Registry of extractors
    pub registry: ExtractorRegistry,

    /// Type inference engine
    inference_engine: TypeInferenceEngine,
}

impl DiscoveryOrchestrator {
    /// Create a new discovery orchestrator
    ///
    /// ## Parameters
    ///
    /// - `cache_path`: Path to RocksDB cache directory
    pub fn new<P: AsRef<Path>>(cache_path: P) -> Result<Self> {
        info!("Initializing DiscoveryOrchestrator");

        let cache =
            DiscoveryCache::new(cache_path).context("Failed to initialize discovery cache")?;

        let registry = ExtractorRegistry::new();
        let inference_engine = TypeInferenceEngine::new();

        Ok(Self {
            cache,
            registry,
            inference_engine,
        })
    }

    /// Create with custom configuration
    pub fn with_config<P: AsRef<Path>>(
        cache_path: P,
        min_confidence: f64,
        sample_size: usize,
    ) -> Result<Self> {
        let cache =
            DiscoveryCache::new(cache_path).context("Failed to initialize discovery cache")?;

        let registry = ExtractorRegistry::new();
        let inference_engine = TypeInferenceEngine::with_config(min_confidence, sample_size);

        Ok(Self {
            cache,
            registry,
            inference_engine,
        })
    }

    /// Register an extractor for a data source type
    ///
    /// ## Example
    ///
    /// ```rust,ignore
    /// orchestrator.register_extractor(
    ///     "postgresql".to_string(),
    ///     PostgreSQLExtractor::new()
    /// );
    /// ```
    pub fn register_extractor<E: SchemaExtractor + 'static>(
        &mut self,
        source_type: String,
        extractor: E,
    ) {
        info!("Registering extractor for source type: {}", source_type);
        self.registry.register(source_type, extractor);
    }

    /// Discover schema from data source
    ///
    /// Main discovery workflow:
    /// 1. Generate cache key from config
    /// 2. Check cache for existing schema
    /// 3. If cached and not expired, return cached schema
    /// 4. Otherwise, extract schema from source
    /// 5. For each table:
    ///    - Extract sample values
    ///    - Infer semantic types for columns
    /// 6. Store in cache
    /// 7. Return discovered schema
    ///
    /// ## Performance
    ///
    /// - Cache hit: <50ms
    /// - Cache miss: <1s per table (target)
    ///
    /// ## Parameters
    ///
    /// - `source`: Data source configuration
    /// - `credentials`: Connection credentials
    /// - `config`: Discovery configuration (filters, sample size, TTL)
    pub async fn discover_schema(
        &self,
        source: &DataSource,
        credentials: &Credentials,
        config: DiscoveryConfig,
    ) -> Result<DiscoveredSchema> {
        info!(
            "Starting schema discovery for source: {} (type: {})",
            source.id, source.source_type
        );

        // 1. Generate cache key
        let cache_key = format!("{}:{}", source.id, config.cache_key());
        debug!("Cache key: {}", cache_key);

        // 2. Check cache
        if let Some(cached_schema) = self.cache.get_schema(&cache_key)? {
            if !cached_schema.is_expired(config.cache_ttl_secs) {
                info!("✓ Cache hit for source: {}", source.id);
                return Ok(cached_schema);
            } else {
                warn!("Cache expired for source: {}, invalidating", source.id);
                self.cache.invalidate_schema(&cache_key)?;
            }
        }

        debug!("Cache miss for source: {}, extracting schema", source.id);

        // 3. Get extractor for source type
        let extractor = self.registry.get(&source.source_type).ok_or_else(|| {
            anyhow!(
                "No extractor registered for source type: {}. Available extractors: {:?}",
                source.source_type,
                self.registry.list_extractors()
            )
        })?;

        info!(
            "Using extractor: {} for source type: {}",
            extractor.name(),
            source.source_type
        );

        // 4. Extract schema metadata
        let metadata = extractor
            .extract_metadata(
                source,
                credentials,
                config.schema_filter.as_deref(),
                config.table_filter.as_deref(),
            )
            .await
            .context("Failed to extract schema metadata")?;

        info!(
            "✓ Extracted metadata: {} tables in schema '{}'",
            metadata.tables.len(),
            metadata.schema_name
        );

        // 5. Discover each table
        let mut discovered_tables = Vec::new();

        for table_meta in &metadata.tables {
            debug!("Discovering table: {}", table_meta.name);

            let discovered_table = self
                .discover_table(source, credentials, table_meta, extractor, &config)
                .await
                .context(format!("Failed to discover table: {}", table_meta.name))?;

            discovered_tables.push(discovered_table);
        }

        // 6. Build discovered schema
        let discovered_schema = DiscoveredSchema {
            source_id: source.id.clone(),
            schema_name: metadata.schema_name.clone(),
            tables: discovered_tables,
            relationships: metadata
                .relationships
                .iter()
                .map(|rel| DiscoveredRelationship {
                    name: rel.name.clone(),
                    source_table: rel.source_table.clone(),
                    source_columns: rel.source_columns.clone(),
                    target_table: rel.target_table.clone(),
                    target_columns: rel.target_columns.clone(),
                })
                .collect(),
            discovered_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
        };

        // 7. Store in cache
        self.cache.put_schema(&cache_key, &discovered_schema)?;

        info!(
            "✓ Schema discovery complete for source: {} ({} tables)",
            source.id,
            discovered_schema.tables.len()
        );

        Ok(discovered_schema)
    }

    /// Discover a single table
    ///
    /// Workflow:
    /// 1. Extract sample values
    /// 2. For each column:
    ///    - Extract column statistics
    ///    - Infer semantic type
    /// 3. Return discovered table
    async fn discover_table(
        &self,
        source: &DataSource,
        credentials: &Credentials,
        table_meta: &TableMetadata,
        extractor: &dyn SchemaExtractor,
        config: &DiscoveryConfig,
    ) -> Result<DiscoveredTable> {
        debug!("Discovering table: {}", table_meta.name);

        // 1. Extract sample values
        let sample_key = format!("{}:{}:samples", source.id, table_meta.name);

        let sample_rows = if let Some(cached_samples) = self.cache.get_samples(&sample_key)? {
            debug!("✓ Cache hit for samples: {}", table_meta.name);
            cached_samples
        } else {
            let samples = extractor
                .extract_samples(source, credentials, &table_meta.name, config.sample_size)
                .await
                .context(format!(
                    "Failed to extract samples for table: {}",
                    table_meta.name
                ))?;

            self.cache.put_samples(&sample_key, &samples)?;
            samples
        };

        debug!(
            "✓ Extracted {} sample rows for table: {}",
            sample_rows.len(),
            table_meta.name
        );

        // 2. Discover each column
        let mut discovered_columns = Vec::new();

        for column_meta in &table_meta.columns {
            let discovered_column = self
                .discover_column(
                    source,
                    credentials,
                    &table_meta.name,
                    column_meta,
                    &sample_rows,
                    extractor,
                )
                .await
                .context(format!("Failed to discover column: {}", column_meta.name))?;

            discovered_columns.push(discovered_column);
        }

        Ok(DiscoveredTable {
            name: table_meta.name.clone(),
            columns: discovered_columns,
            row_count: table_meta.estimated_rows,
        })
    }

    /// Discover a single column
    ///
    /// Workflow:
    /// 1. Extract sample values for this column
    /// 2. Extract column statistics
    /// 3. Infer semantic type using inference engine
    /// 4. Return discovered column
    async fn discover_column(
        &self,
        source: &DataSource,
        credentials: &Credentials,
        table_name: &str,
        column_meta: &ColumnMetadata,
        sample_rows: &[SampleRow],
        extractor: &dyn SchemaExtractor,
    ) -> Result<DiscoveredColumn> {
        debug!("Discovering column: {}.{}", table_name, column_meta.name);

        // 1. Extract sample values for this column
        let sample_values: Vec<String> = sample_rows
            .iter()
            .filter_map(|row| row.values.get(&column_meta.name).cloned())
            .collect();

        // 2. Extract column statistics
        let stats = extractor
            .extract_statistics(source, credentials, table_name, &column_meta.name)
            .await
            .unwrap_or_else(|e| {
                warn!(
                    "Failed to extract statistics for {}.{}: {}",
                    table_name, column_meta.name, e
                );
                ColumnStats::default()
            });

        // 3. Infer semantic type
        let inference_result = self
            .inference_engine
            .infer_type(column_meta, &sample_values, &stats)
            .context(format!(
                "Failed to infer type for column: {}",
                column_meta.name
            ))?;

        debug!(
            "✓ Inferred type for {}.{}: {:?} (confidence: {:.2})",
            table_name,
            column_meta.name,
            inference_result.semantic_type,
            inference_result.confidence
        );

        Ok(DiscoveredColumn {
            name: column_meta.name.clone(),
            data_type: column_meta.data_type.clone(),
            nullable: column_meta.nullable,
            primary_key: column_meta.primary_key,
            semantic_type: inference_result.semantic_type,
            confidence: inference_result.confidence,
            patterns: inference_result.detected_patterns,
            statistics: inference_result.statistics,
            sample_values: sample_values.into_iter().take(10).collect(),
        })
    }

    /// Discover schema with progress callback
    ///
    /// Enhanced version of discover_schema that accepts a progress callback
    /// for real-time progress updates during discovery.
    ///
    /// ## Progress Callback
    ///
    /// The callback is invoked with:
    /// - `step`: Human-readable description of current step
    /// - `tables_discovered`: Number of tables discovered so far
    /// - `total_tables`: Total number of tables (if known)
    ///
    /// ## Example
    ///
    /// ```rust,ignore
    /// let schema = orchestrator.discover_schema_with_progress(
    ///     &source,
    ///     &credentials,
    ///     config,
    ///     |step, discovered, total| {
    ///         println!("Step: {}, Progress: {}/{:?}", step, discovered, total);
    ///     }
    /// ).await?;
    /// ```
    pub async fn discover_schema_with_progress<F>(
        &self,
        source: &DataSource,
        credentials: &Credentials,
        config: DiscoveryConfig,
        progress_callback: F,
    ) -> Result<DiscoveredSchema>
    where
        F: Fn(String, usize, Option<usize>) + Send + Sync,
    {
        info!(
            "Starting schema discovery with progress tracking for source: {} (type: {})",
            source.id, source.source_type
        );

        progress_callback("Checking cache".to_string(), 0, None);

        // 1. Generate cache key
        let cache_key = format!("{}:{}", source.id, config.cache_key());
        debug!("Cache key: {}", cache_key);

        // 2. Check cache
        if let Some(cached_schema) = self.cache.get_schema(&cache_key)? {
            if !cached_schema.is_expired(config.cache_ttl_secs) {
                info!("✓ Cache hit for source: {}", source.id);
                progress_callback(
                    "Loaded from cache".to_string(),
                    cached_schema.tables.len(),
                    Some(cached_schema.tables.len()),
                );
                return Ok(cached_schema);
            } else {
                warn!("Cache expired for source: {}, invalidating", source.id);
                self.cache.invalidate_schema(&cache_key)?;
            }
        }

        debug!("Cache miss for source: {}, extracting schema", source.id);
        progress_callback("Initializing extractor".to_string(), 0, None);

        // 3. Get extractor for source type
        let extractor = self.registry.get(&source.source_type).ok_or_else(|| {
            anyhow!(
                "No extractor registered for source type: {}. Available extractors: {:?}",
                source.source_type,
                self.registry.list_extractors()
            )
        })?;

        info!(
            "Using extractor: {} for source type: {}",
            extractor.name(),
            source.source_type
        );

        progress_callback("Extracting schema metadata".to_string(), 0, None);

        // 4. Extract schema metadata
        let metadata = extractor
            .extract_metadata(
                source,
                credentials,
                config.schema_filter.as_deref(),
                config.table_filter.as_deref(),
            )
            .await
            .context("Failed to extract schema metadata")?;

        info!(
            "✓ Extracted metadata: {} tables in schema '{}'",
            metadata.tables.len(),
            metadata.schema_name
        );

        let total_tables = metadata.tables.len();
        progress_callback(
            format!("Found {} tables, starting introspection", total_tables),
            0,
            Some(total_tables),
        );

        // 5. Discover each table with progress updates
        let mut discovered_tables = Vec::new();

        for (idx, table_meta) in metadata.tables.iter().enumerate() {
            debug!("Discovering table: {}", table_meta.name);

            progress_callback(
                format!("Introspecting table: {}", table_meta.name),
                idx,
                Some(total_tables),
            );

            let discovered_table = self
                .discover_table(source, credentials, table_meta, extractor, &config)
                .await
                .context(format!("Failed to discover table: {}", table_meta.name))?;

            discovered_tables.push(discovered_table);

            progress_callback(
                format!("Completed table: {}", table_meta.name),
                idx + 1,
                Some(total_tables),
            );
        }

        progress_callback(
            "Building schema result".to_string(),
            total_tables,
            Some(total_tables),
        );

        // 6. Build discovered schema
        let discovered_schema = DiscoveredSchema {
            source_id: source.id.clone(),
            schema_name: metadata.schema_name.clone(),
            tables: discovered_tables,
            relationships: metadata
                .relationships
                .iter()
                .map(|rel| DiscoveredRelationship {
                    name: rel.name.clone(),
                    source_table: rel.source_table.clone(),
                    source_columns: rel.source_columns.clone(),
                    target_table: rel.target_table.clone(),
                    target_columns: rel.target_columns.clone(),
                })
                .collect(),
            discovered_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
        };

        // 7. Store in cache
        progress_callback(
            "Caching results".to_string(),
            total_tables,
            Some(total_tables),
        );
        self.cache.put_schema(&cache_key, &discovered_schema)?;

        info!(
            "✓ Schema discovery complete for source: {} ({} tables)",
            source.id,
            discovered_schema.tables.len()
        );

        progress_callback(
            "Discovery completed".to_string(),
            total_tables,
            Some(total_tables),
        );

        Ok(discovered_schema)
    }

    /// Invalidate cached schema for a source
    pub fn invalidate_cache(&self, source_id: &str) -> Result<()> {
        info!("Invalidating cache for source: {}", source_id);
        // Note: In production, we'd need to track all cache keys for a source
        // For now, this is a placeholder
        warn!("Cache invalidation not fully implemented - use clear_all_cache for now");
        Ok(())
    }

    /// Clear all cached schemas
    pub fn clear_all_cache(&self) -> Result<()> {
        info!("Clearing all discovery cache");
        self.cache.clear_all()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_orchestrator_creation() {
        let temp_dir = TempDir::new().unwrap();
        let orchestrator = DiscoveryOrchestrator::new(temp_dir.path()).unwrap();

        // Verify orchestrator created successfully
        assert_eq!(orchestrator.registry.list_extractors().len(), 0);
    }

    #[test]
    fn test_orchestrator_with_config() {
        let temp_dir = TempDir::new().unwrap();
        let orchestrator = DiscoveryOrchestrator::with_config(temp_dir.path(), 0.7, 500).unwrap();

        assert_eq!(orchestrator.registry.list_extractors().len(), 0);
    }
}
