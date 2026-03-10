//! Data Transformation Registry and Trait
//!
//! Extensible transformer system for workflow data transformations.
//!
//! ## Architecture
//!
//! ```text
//! TransformerRegistry
//!   ├─ CSV Parser (file_store)
//!   ├─ DB2 Migrator (db connection pool)
//!   ├─ Deduplicator (matching algorithms)
//!   └─ Ontology Mapper (ontology store)
//! ```
//!
//! ## Example
//!
//! ```rust,ignore
//! use graphica_coordinator::workflows::engine::transformers::*;
//! use serde_json::json;
//! use std::sync::Arc;
//!
//! # async fn example(file_store: Arc<dyn crate::api::file_library::storage_trait::FileLibraryStore>) -> anyhow::Result<()> {
//! // Create registry with standard transformers
//! let registry = TransformerRegistry::new()
//!     .with_csv_parser(file_store)
//!     .with_db2_migrator();
//!
//! // Execute transformation
//! let mut data = json!({"file_id": "file_123"});
//! let config = json!({"delimiter": ",", "has_header": true});
//!
//! registry.execute("csv_parse", &config, &mut data).await?;
//! # Ok(())
//! # }
//! ```

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, warn};

// Transformer implementations
pub mod csv_parser;
pub mod db2_load; // Production DB2 loader
pub mod db2_migrator; // DEPRECATED: Stub implementation, use db2_load instead
pub mod field_mapper;
pub mod ontology_mapper;
pub mod shacl_ddl; // SHACL-DDL generator // Manual field mapping with learning

// Re-exports for convenience
pub use csv_parser::CsvParserTransformer;
pub use db2_load::Db2LoadTransformer; // Production implementation
pub use db2_migrator::Db2MigratorTransformer; // DEPRECATED
pub use field_mapper::FieldMapperTransformer;
pub use ontology_mapper::OntologyMapperTransformer;
pub use shacl_ddl::ShaclDdlTransformer;

// ============================================================================
// Transformer Trait
// ============================================================================

/// Core transformer trait for data transformations
///
/// Transformers are stateless, side-effect-free functions that modify
/// JSON data in-place based on configuration.
///
/// ## Design Principles
///
/// 1. **Stateless**: Transformers should not maintain internal state
/// 2. **Idempotent**: Same input + config = same output
/// 3. **Composable**: Can be chained in workflows
/// 4. **Testable**: Pure functions with clear contracts
///
/// ## Error Handling
///
/// - Return `Err` for fatal errors (invalid config, missing dependencies)
/// - Log warnings for non-fatal issues (skipped rows, data quality)
/// - Never panic - always return `Result`
#[async_trait]
pub trait Transformer: Send + Sync {
    /// Execute the transformation
    ///
    /// # Arguments
    ///
    /// * `config` - Transformer-specific configuration (JSON)
    /// * `data` - Input/output data (modified in-place)
    /// * `context` - Optional execution context for dependency injection
    ///
    /// # Returns
    ///
    /// - `Ok(())` on success
    /// - `Err(e)` on fatal error
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let mut data = json!({"rows": []});
    /// let config = json!({"file_id": "file_123"});
    ///
    /// transformer.transform(&config, &mut data, None).await?;
    /// // data now contains: {"rows": [...parsed rows...]}
    /// ```
    async fn transform(
        &self,
        config: &JsonValue,
        data: &mut JsonValue,
        context: Option<&crate::workflows::engine::executor::ExecutionContext>,
    ) -> Result<()>;

    /// Get transformer name for logging/debugging
    fn name(&self) -> &'static str;

    /// Validate configuration before execution (optional)
    ///
    /// Override this to provide early validation of transformer config.
    /// Default implementation always returns Ok.
    fn validate_config(&self, _config: &JsonValue) -> Result<()> {
        Ok(())
    }
}

// ============================================================================
// Transformer Registry
// ============================================================================

/// Registry of available data transformers
///
/// Manages transformer lifecycle and provides dependency injection.
/// Transformers are registered once at application startup and reused
/// across all workflow executions.
///
/// ## Thread Safety
///
/// The registry is wrapped in `Arc` for thread-safe sharing across
/// async tasks. Transformers themselves must be `Send + Sync`.
pub struct TransformerRegistry {
    /// Registered transformers by name
    transformers: HashMap<String, Arc<dyn Transformer>>,
}

impl TransformerRegistry {
    /// Create a new empty transformer registry
    pub fn new() -> Self {
        Self {
            transformers: HashMap::new(),
        }
    }

    /// Register a transformer
    ///
    /// # Arguments
    ///
    /// * `name` - Unique transformer identifier (e.g., "csv_parse")
    /// * `transformer` - Transformer implementation
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let csv_parser = Arc::new(CsvParserTransformer::new(file_store));
    /// registry.register("csv_parse", csv_parser);
    /// ```
    pub fn register(&mut self, name: impl Into<String>, transformer: Arc<dyn Transformer>) {
        let name = name.into();
        debug!("Registering transformer: {}", name);
        self.transformers.insert(name, transformer);
    }

    /// Execute a transformation by name
    ///
    /// # Arguments
    ///
    /// * `name` - Registered transformer name
    /// * `config` - Transformer configuration
    /// * `data` - Data to transform (modified in-place)
    /// * `context` - Optional execution context for dependency injection
    ///
    /// # Returns
    ///
    /// - `Ok(())` on success
    /// - `Err` if transformer not found or execution fails
    pub async fn execute(
        &self,
        name: &str,
        config: &JsonValue,
        data: &mut JsonValue,
        context: Option<&crate::workflows::engine::executor::ExecutionContext>,
    ) -> Result<()> {
        let transformer = self
            .transformers
            .get(name)
            .ok_or_else(|| anyhow!("Transformer not found: {}", name))?;

        // Validate config first
        transformer
            .validate_config(config)
            .with_context(|| format!("Invalid configuration for transformer '{}'", name))?;

        info!("Executing transformer: {}", name);

        // === MEMORY TRACKING: Measure before transformation ===
        let memory_before = Self::estimate_json_memory(data);
        let row_count_before = data
            .get("rows")
            .and_then(|v| v.as_array())
            .map(|arr| arr.len())
            .unwrap_or(0);

        // Execute transformation
        transformer
            .transform(config, data, context)
            .await
            .with_context(|| format!("Transformer '{}' execution failed", name))?;

        // === MEMORY TRACKING: Measure after transformation ===
        let memory_after = Self::estimate_json_memory(data);
        let row_count_after = data
            .get("rows")
            .and_then(|v| v.as_array())
            .map(|arr| arr.len())
            .unwrap_or(0);

        let memory_mb = memory_after as f64 / 1_000_000.0;
        let memory_gb = memory_after as f64 / 1_000_000_000.0;
        let memory_delta = memory_after as i64 - memory_before as i64;

        // Log structured memory metrics
        tracing::info!(
            target: "workflow_memory",
            transformer = name,
            memory_bytes = memory_after,
            memory_mb = memory_mb,
            memory_gb = memory_gb,
            memory_delta_bytes = memory_delta,
            row_count_before = row_count_before,
            row_count_after = row_count_after,
            "Transformer memory usage: {:.2} MB ({:.3} GB), delta: {:.2} MB, rows: {} -> {}",
            memory_mb,
            memory_gb,
            memory_delta as f64 / 1_000_000.0,
            row_count_before,
            row_count_after
        );

        debug!("Transformer '{}' completed successfully", name);
        Ok(())
    }

    /// Estimate memory usage of JSON data structures (in bytes)
    ///
    /// Provides rough estimate of heap memory consumption for JSON values.
    /// Used for memory tracking and resource limit enforcement.
    ///
    /// # Memory Model
    ///
    /// - Null: 8 bytes (enum discriminant)
    /// - Bool: 8 bytes (enum discriminant)
    /// - Number: 16 bytes (enum discriminant + f64)
    /// - String: 24 bytes (String header) + UTF-8 data length
    /// - Array: 24 bytes (Vec header) + sum of element sizes
    /// - Object: 24 bytes (Map header) + sum of (key length + value size)
    fn estimate_json_memory(value: &JsonValue) -> usize {
        match value {
            JsonValue::Null => 8,
            JsonValue::Bool(_) => 8,
            JsonValue::Number(_) => 16,
            JsonValue::String(s) => 24 + s.len(),
            JsonValue::Array(arr) => {
                24 + arr
                    .iter()
                    .map(|v| Self::estimate_json_memory(v))
                    .sum::<usize>()
            }
            JsonValue::Object(obj) => {
                24 + obj
                    .iter()
                    .map(|(k, v)| k.len() + Self::estimate_json_memory(v))
                    .sum::<usize>()
            }
        }
    }

    /// Check if a transformer is registered
    pub fn has_transformer(&self, name: &str) -> bool {
        self.transformers.contains_key(name)
    }

    /// Get list of registered transformer names
    pub fn list_transformers(&self) -> Vec<String> {
        self.transformers.keys().cloned().collect()
    }

    /// Get number of registered transformers
    pub fn count(&self) -> usize {
        self.transformers.len()
    }
}

impl Default for TransformerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Builder Pattern for Registry Construction
// ============================================================================

impl TransformerRegistry {
    /// Register CSV parser transformer with file library integration
    ///
    /// # Arguments
    ///
    /// * `file_store` - File library storage for reading uploaded CSVs
    pub fn with_csv_parser(
        mut self,
        file_store: Arc<dyn crate::api::file_library::storage_trait::FileLibraryStore>,
    ) -> Self {
        let transformer = Arc::new(CsvParserTransformer::new(file_store));
        self.register("csv_parse", transformer.clone());
        self.register("parse_csv", transformer); // Alias
        self
    }

    /// Register DB2 migrator transformer (DEPRECATED)
    ///
    /// **DEPRECATED**: This is a stub implementation that always fails.
    /// Use `with_db2_loader()` instead for production DB2 loading.
    ///
    /// # Arguments
    ///
    /// * `connection_pool` - Optional DB2 connection pool for reuse
    #[deprecated(
        since = "0.1.0",
        note = "Use with_db2_loader() instead - this is a stub"
    )]
    pub fn with_db2_migrator(mut self) -> Self {
        let transformer = Arc::new(Db2MigratorTransformer::new());
        self.register("db2_migrate", transformer.clone());
        self.register("migrate_to_db2", transformer); // Alias
        self
    }

    /// Register DB2 loader transformer
    ///
    /// This implementation loads data to DB2 using MockDB2Connection for testing.
    /// For production ODBC support, replace MockDB2Connection with OdbcDB2Connection.
    ///
    /// Features:
    /// - DDL generation (CREATE TABLE) via DB2 dialect
    /// - DML execution (INSERT, MERGE, TRUNCATE)
    /// - Batch processing for performance
    /// - Connection pooling for high-performance concurrent workflows
    /// - Mock connection for testing (no ODBC required)
    ///
    /// # Arguments
    ///
    /// * `connection_pool` - Optional DB2 connection pool for connection reuse
    ///   (production) or None for per-workflow connections (testing)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // With connection pool (production)
    /// let pool = create_db2_pool(pool_config).await?;
    /// let registry = TransformerRegistry::new()
    ///     .with_db2_loader(Some(pool));
    ///
    /// // Without pool (testing)
    /// let registry = TransformerRegistry::new()
    ///     .with_db2_loader(None);
    /// ```
    pub fn with_db2_loader(
        mut self,
        connection_pool: Option<Arc<crate::mapping::loader::DB2Pool>>,
    ) -> Self {
        let mut transformer = Db2LoadTransformer::new();

        // Add connection pool if provided
        if let Some(pool) = connection_pool {
            transformer = transformer.with_connection_pool(pool);
        }

        let transformer_arc = Arc::new(transformer);
        self.register("db2_load", transformer_arc.clone());
        self.register("load_to_db2", transformer_arc); // Alias
        self
    }

    /// Register ontology mapper transformer
    ///
    /// Maps source CSV fields to ontological semantic fields for:
    /// - Consistent model invocation (models use ontological field names)
    /// - Cross-source lineage tracking (semantic field names)
    /// - Data quality rules (ontological field validation)
    ///
    /// # Arguments
    ///
    /// * `mapping_engine` - MappingEngine for semantic field resolution
    pub fn with_ontology_mapper(
        mut self,
        mapping_engine: Arc<crate::mapping::MappingEngine>,
    ) -> Self {
        let transformer = Arc::new(OntologyMapperTransformer::new(mapping_engine));
        self.register("ontology_map", transformer.clone());
        self.register("map_ontology", transformer); // Alias
        self
    }

    /// Register ontology mapper transformer with column lineage tracking
    ///
    /// Same as `with_ontology_mapper` but also records column-level lineage
    /// for semantic mappings (field-to-ontology transformations).
    ///
    /// # Arguments
    ///
    /// * `mapping_engine` - MappingEngine for semantic field resolution
    /// * `column_lineage_store` - Store for recording column lineage events
    pub fn with_ontology_mapper_and_lineage(
        mut self,
        mapping_engine: Arc<crate::mapping::MappingEngine>,
        column_lineage_store: Arc<
            dyn graphica_core::core::lineage::column_level::ColumnLineageSink,
        >,
    ) -> Self {
        let transformer = Arc::new(
            OntologyMapperTransformer::new(mapping_engine)
                .with_column_lineage_store(column_lineage_store),
        );
        self.register("ontology_map", transformer.clone());
        self.register("map_ontology", transformer); // Alias
        self
    }

    /// Register SHACL-DDL generator transformer
    ///
    /// Generates SQL DDL (Data Definition Language) from SHACL shapes stored
    /// in the RDF triple store. This enables schema-first development where
    /// ontological constraints drive database schemas.
    ///
    /// # Features
    ///
    /// - Queries SHACL shapes from RDF store via AsyncRdfStoreAdapter
    /// - Converts SHACL constraints to SQL DDL
    /// - Supports multiple SQL dialects (DB2, PostgreSQL, Oracle)
    /// - Generates CREATE TABLE, INDEX, and FOREIGN KEY statements
    ///
    /// # Architecture
    ///
    /// Uses AsyncRdfStoreAdapter as a thin async wrapper around the existing
    /// RdfStore (single source of truth). This avoids creating duplicate RDF
    /// stores and ensures data consistency.
    ///
    /// # Arguments
    ///
    /// * `rdf_adapter` - Async adapter wrapping the shared RDF store
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Create adapter from existing RDF store (single source of truth)
    /// let rdf_store = Arc::new(GraphicaRdfStore::new_in_memory()?);
    /// let rdf_adapter = Arc::new(AsyncRdfStoreAdapter::new(rdf_store));
    ///
    /// let registry = TransformerRegistry::new()
    ///     .with_shacl_ddl_generator(rdf_adapter);
    /// ```
    pub fn with_shacl_ddl_generator(
        mut self,
        rdf_adapter: Arc<crate::governance::AsyncRdfStoreAdapter>,
    ) -> Self {
        let transformer = Arc::new(ShaclDdlTransformer::new(rdf_adapter));
        self.register("shacl_ddl_generator", transformer.clone());
        self.register("generate_ddl", transformer); // Alias
        self
    }

    /// Register field mapper transformer with manual mapping store
    ///
    /// Applies manual field mappings and learning-based suggestions before
    /// ontology mapping. This enables human-in-the-loop field mapping with
    /// continuous learning from user corrections.
    ///
    /// # Features
    ///
    /// - **Manual Mappings**: Apply user-provided field mappings (confidence: 1.0)
    /// - **Learning System**: Suggest mappings from similar datasets
    /// - **Usage Tracking**: Record mapping usage for continuous improvement
    /// - **Confidence Scoring**: Track manual vs. suggested mapping quality
    ///
    /// # Workflow Position
    ///
    /// This transformer should run BEFORE ontology_map:
    /// ```text
    /// csv_parse → field_mapper → ontology_map → db2_load
    /// ```
    ///
    /// # Arguments
    ///
    /// * `mapping_store` - RocksDB-backed manual mapping store
    /// * `learning_engine` - Learning engine for suggestions
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use graphica_coordinator::mapping::manual::{ManualMappingStore, MappingLearningEngine};
    ///
    /// let store = Arc::new(ManualMappingStore::new("./data/manual_mappings")?);
    /// let learning = Arc::new(MappingLearningEngine::new(store.clone()));
    ///
    /// let registry = TransformerRegistry::new()
    ///     .with_field_mapper(store, learning);
    /// ```
    pub fn with_field_mapper(
        mut self,
        mapping_store: Arc<crate::mapping::manual::ManualMappingStore>,
        learning_engine: Arc<crate::mapping::manual::MappingLearningEngine>,
    ) -> Self {
        let transformer = Arc::new(FieldMapperTransformer::new(mapping_store, learning_engine));
        self.register("field_mapper", transformer.clone());
        self.register("map_fields", transformer); // Alias
        self
    }

    /// Register all standard transformers
    ///
    /// Convenience method to register commonly used transformers.
    ///
    /// Note: Ontology mapping must be registered separately via `with_ontology_mapper()`
    /// as it requires the MappingEngine dependency.
    pub fn with_standard_transformers(
        self,
        file_store: Arc<dyn crate::api::file_library::storage_trait::FileLibraryStore>,
    ) -> Self {
        self.with_csv_parser(file_store).with_db2_migrator()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // Mock transformer for testing
    struct MockTransformer {
        name: &'static str,
    }

    #[async_trait]
    impl Transformer for MockTransformer {
        async fn transform(
            &self,
            config: &JsonValue,
            data: &mut JsonValue,
            _context: Option<&crate::workflows::engine::executor::ExecutionContext>,
        ) -> Result<()> {
            // Simple mock: append config to data
            data["mock_executed"] = json!(true);
            data["config"] = config.clone();
            Ok(())
        }

        fn name(&self) -> &'static str {
            self.name
        }

        fn validate_config(&self, config: &JsonValue) -> Result<()> {
            if config.get("fail_validation").and_then(|v| v.as_bool()) == Some(true) {
                anyhow::bail!("Mock validation failure");
            }
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_register_and_execute() {
        let mut registry = TransformerRegistry::new();

        let transformer = Arc::new(MockTransformer { name: "mock" });
        registry.register("test_transformer", transformer);

        assert!(registry.has_transformer("test_transformer"));
        assert_eq!(registry.count(), 1);

        let mut data = json!({});
        let config = json!({"key": "value"});

        registry
            .execute("test_transformer", &config, &mut data, None)
            .await
            .unwrap();

        assert_eq!(data["mock_executed"], json!(true));
        assert_eq!(data["config"]["key"], json!("value"));
    }

    #[tokio::test]
    async fn test_transformer_not_found() {
        let registry = TransformerRegistry::new();

        let mut data = json!({});
        let config = json!({});

        let result = registry
            .execute("nonexistent", &config, &mut data, None)
            .await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Transformer not found"));
    }

    #[tokio::test]
    async fn test_validation_failure() {
        let mut registry = TransformerRegistry::new();

        let transformer = Arc::new(MockTransformer { name: "mock" });
        registry.register("validator", transformer);

        let mut data = json!({});
        let config = json!({"fail_validation": true});

        let result = registry
            .execute("validator", &config, &mut data, None)
            .await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid configuration"));
    }

    #[test]
    fn test_list_transformers() {
        let mut registry = TransformerRegistry::new();

        registry.register("transformer_a", Arc::new(MockTransformer { name: "a" }));
        registry.register("transformer_b", Arc::new(MockTransformer { name: "b" }));
        registry.register("transformer_c", Arc::new(MockTransformer { name: "c" }));

        let mut names = registry.list_transformers();
        names.sort();

        assert_eq!(
            names,
            vec!["transformer_a", "transformer_b", "transformer_c"]
        );
    }

    #[test]
    fn test_default() {
        let registry = TransformerRegistry::default();
        assert_eq!(registry.count(), 0);
    }
}
