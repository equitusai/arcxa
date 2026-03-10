//! Ontology-Driven Data Loader
//!
//! Main orchestration layer for loading data driven by ontology definitions.
//! Coordinates schema generation, caching, DDL execution, and batch loading.

use anyhow::{anyhow, Context, Result};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

use super::cache::*;
use super::data_transformer::*;
use super::ddl_generator::*;
use super::normalization::*;
use super::relationship_resolver::*;
use super::schema_provider::*;
use super::type_mapper::*;
use super::types::*;

/// Trait for database executor (abstracts DB operations for testing)
#[async_trait::async_trait]
pub trait DbExecutor: Send + Sync {
    /// Execute DDL statement (CREATE TABLE, DROP TABLE, etc.)
    async fn execute_ddl(&self, sql: &str) -> Result<()>;

    /// Check if table exists
    async fn table_exists(&self, table_name: &str) -> Result<bool>;

    /// Execute batch insert with parameters
    async fn execute_batch_insert(&self, sql: &str, rows: Vec<Map<String, Value>>) -> Result<u64>;

    /// Begin transaction
    async fn begin_transaction(&self) -> Result<()>;

    /// Commit transaction
    async fn commit(&self) -> Result<()>;

    /// Rollback transaction
    async fn rollback(&self) -> Result<()>;
}

/// Configuration for OntologyDrivenLoader
#[derive(Debug, Clone)]
pub struct LoaderConfig {
    /// Batch size for inserts (default: 100)
    pub batch_size: usize,

    /// Whether to create tables if they don't exist (default: true)
    pub auto_create_tables: bool,

    /// Whether to validate data against schema (default: true)
    pub validate_data: bool,

    /// Whether to resolve relationships (default: true)
    pub resolve_relationships: bool,

    /// Maximum retry attempts for transient errors (default: 3)
    pub max_retries: u32,
}

impl Default for LoaderConfig {
    fn default() -> Self {
        Self {
            batch_size: 100,
            auto_create_tables: true,
            validate_data: true,
            resolve_relationships: true,
            max_retries: 3,
        }
    }
}

/// Main orchestrator for ontology-driven data loading
///
/// Coordinates all components:
/// - Schema provider: retrieves entity definitions from ontology
/// - Type mapper: converts XSD types to SQL types
/// - DDL generator: generates CREATE TABLE statements
/// - Normalization strategy: decides table structure (normalized/denormalized)
/// - Data transformer: transforms JSON data to match schema
/// - Relationship resolver: resolves entity references to foreign keys
/// - Cache: caches entity definitions, schemas, and DDL
/// - DB executor: executes DDL and DML statements
pub struct OntologyDrivenLoader {
    schema_provider: Arc<dyn OntologySchemaProvider>,
    type_mapper: Arc<dyn TypeMapper>,
    ddl_generator: Arc<dyn DDLGenerator>,
    normalization_strategy: Arc<dyn NormalizationStrategy>,
    data_transformer: Arc<dyn DataTransformer>,
    relationship_resolver: Arc<dyn RelationshipResolver>,
    cache: Arc<dyn SchemaCache>,
    db_executor: Arc<dyn DbExecutor>,
    config: LoaderConfig,
}

impl OntologyDrivenLoader {
    /// Create a new ontology-driven loader
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        schema_provider: Arc<dyn OntologySchemaProvider>,
        type_mapper: Arc<dyn TypeMapper>,
        ddl_generator: Arc<dyn DDLGenerator>,
        normalization_strategy: Arc<dyn NormalizationStrategy>,
        data_transformer: Arc<dyn DataTransformer>,
        relationship_resolver: Arc<dyn RelationshipResolver>,
        cache: Arc<dyn SchemaCache>,
        db_executor: Arc<dyn DbExecutor>,
    ) -> Self {
        Self::with_config(
            schema_provider,
            type_mapper,
            ddl_generator,
            normalization_strategy,
            data_transformer,
            relationship_resolver,
            cache,
            db_executor,
            LoaderConfig::default(),
        )
    }

    /// Create a new ontology-driven loader with custom configuration
    #[allow(clippy::too_many_arguments)]
    pub fn with_config(
        schema_provider: Arc<dyn OntologySchemaProvider>,
        type_mapper: Arc<dyn TypeMapper>,
        ddl_generator: Arc<dyn DDLGenerator>,
        normalization_strategy: Arc<dyn NormalizationStrategy>,
        data_transformer: Arc<dyn DataTransformer>,
        relationship_resolver: Arc<dyn RelationshipResolver>,
        cache: Arc<dyn SchemaCache>,
        db_executor: Arc<dyn DbExecutor>,
        config: LoaderConfig,
    ) -> Self {
        info!(
            "Initializing OntologyDrivenLoader: batch_size={}, auto_create={}, validate={}, resolve_rel={}",
            config.batch_size,
            config.auto_create_tables,
            config.validate_data,
            config.resolve_relationships
        );

        Self {
            schema_provider,
            type_mapper,
            ddl_generator,
            normalization_strategy,
            data_transformer,
            relationship_resolver,
            cache,
            db_executor,
            config,
        }
    }

    /// Load ontology data to database
    ///
    /// Main entry point for loading data. This method:
    /// 1. Retrieves entity definition (cached)
    /// 2. Generates table schemas (cached)
    /// 3. Ensures tables exist (creates if needed, cached DDL)
    /// 4. Validates and transforms data
    /// 5. Resolves relationships to foreign keys
    /// 6. Executes batch inserts in a transaction
    ///
    /// # Arguments
    /// * `entity_uri` - URI of the entity in the ontology
    /// * `rows` - JSON data rows to load
    /// * `create_table` - Whether to create tables if they don't exist
    ///
    /// # Returns
    /// Number of rows successfully loaded
    pub async fn load_ontology_data(
        &self,
        entity_uri: &str,
        mut rows: Vec<Map<String, Value>>,
        create_table: bool,
    ) -> Result<u64> {
        info!(
            "Loading {} rows for entity '{}' (create_table={})",
            rows.len(),
            entity_uri,
            create_table
        );

        if rows.is_empty() {
            info!("No rows to load");
            return Ok(0);
        }

        // Step 1: Get entity definition (cached)
        let entity_def = self
            .get_or_fetch_entity_definition(entity_uri)
            .await
            .with_context(|| format!("Failed to get entity definition for '{}'", entity_uri))?;

        debug!(
            "Entity definition: {} properties, {} relationships",
            entity_def.properties.len(),
            entity_def.relationships.len()
        );

        // Step 2: Generate table schemas (cached)
        let schemas = self
            .get_or_generate_schemas(entity_uri, &entity_def)
            .await
            .with_context(|| format!("Failed to generate schemas for '{}'", entity_uri))?;

        info!("Generated {} table schema(s)", schemas.len());

        // Step 3: Ensure tables exist
        if create_table || self.config.auto_create_tables {
            self.ensure_tables_exist(&schemas)
                .await
                .with_context(|| "Failed to ensure tables exist")?;
        }

        // Find main entity table (first non-junction table)
        let main_schema = schemas
            .iter()
            .find(|s| !s.table_name.contains("_junction"))
            .ok_or_else(|| anyhow!("No main entity table found in schemas"))?;

        info!("Main table: {}", main_schema.table_name);

        // Step 4: Validate data if enabled
        if self.config.validate_data {
            for (idx, row) in rows.iter().enumerate() {
                self.data_transformer
                    .validate_row(row, main_schema)
                    .with_context(|| format!("Row {} validation failed", idx))?;
            }
            debug!("All rows validated successfully");
        }

        // Step 5: Transform data to match schema
        let mut transformed_rows = self
            .data_transformer
            .transform_batch(&rows, main_schema)
            .with_context(|| "Failed to transform data batch")?;

        info!("Transformed {} rows", transformed_rows.len());

        // Step 6: Resolve relationships if enabled
        if self.config.resolve_relationships {
            self.relationship_resolver
                .resolve_relationships(&mut transformed_rows, &entity_def, main_schema)
                .await
                .with_context(|| "Failed to resolve relationships")?;

            debug!("Resolved relationships for {} rows", transformed_rows.len());
        }

        // Step 7: Load data in transaction
        let loaded_count = self
            .load_with_transaction(main_schema, transformed_rows)
            .await
            .with_context(|| format!("Failed to load data to {}", main_schema.table_name))?;

        info!(
            "Successfully loaded {} rows to {} (entity: {})",
            loaded_count, main_schema.table_name, entity_uri
        );

        // TODO: Handle junction tables for many-to-many relationships

        Ok(loaded_count)
    }

    /// Get entity definition from cache or fetch from provider
    async fn get_or_fetch_entity_definition(&self, entity_uri: &str) -> Result<EntityDefinition> {
        // Check cache first
        if let Some(cached) = self.cache.get_entity_def(entity_uri).await {
            debug!("Using cached entity definition for '{}'", entity_uri);
            return Ok(cached);
        }

        // Fetch from provider
        debug!(
            "Fetching entity definition for '{}' from provider",
            entity_uri
        );
        let entity_def = self
            .schema_provider
            .get_entity_definition(entity_uri)
            .await?;

        // Cache for future use
        self.cache
            .cache_entity_def(entity_uri.to_string(), entity_def.clone())
            .await;

        Ok(entity_def)
    }

    /// Get table schemas from cache or generate
    async fn get_or_generate_schemas(
        &self,
        entity_uri: &str,
        entity_def: &EntityDefinition,
    ) -> Result<Vec<TableSchema>> {
        // Check cache first
        if let Some(cached) = self.cache.get_table_schemas(entity_uri).await {
            debug!("Using cached table schemas for '{}'", entity_uri);
            return Ok(cached);
        }

        // Generate schemas using normalization strategy
        debug!("Generating table schemas for '{}'", entity_uri);
        let schemas = self
            .normalization_strategy
            .generate_schemas(entity_def)
            .await?;

        // Cache for future use
        self.cache
            .cache_table_schemas(entity_uri.to_string(), schemas.clone())
            .await;

        Ok(schemas)
    }

    /// Ensure all tables exist (create if needed)
    async fn ensure_tables_exist(&self, schemas: &[TableSchema]) -> Result<()> {
        for schema in schemas {
            self.execute_create_table(schema)
                .await
                .with_context(|| format!("Failed to create table '{}'", schema.table_name))?;
        }
        Ok(())
    }

    /// Execute CREATE TABLE if table doesn't exist
    async fn execute_create_table(&self, schema: &TableSchema) -> Result<()> {
        let table_name = &schema.table_name;

        // Check if table exists
        let exists = self
            .db_executor
            .table_exists(table_name)
            .await
            .with_context(|| format!("Failed to check if table '{}' exists", table_name))?;

        if exists {
            debug!("Table '{}' already exists, skipping creation", table_name);
            return Ok(());
        }

        // Check cache for DDL
        let ddl = if let Some(cached_ddl) = self.cache.get_ddl(table_name).await {
            debug!("Using cached DDL for '{}'", table_name);
            cached_ddl
        } else {
            debug!("Generating DDL for '{}'", table_name);
            let generated_ddl = self
                .ddl_generator
                .generate_create_table(schema)
                .with_context(|| format!("Failed to generate DDL for '{}'", table_name))?;

            // Cache DDL
            self.cache
                .cache_ddl(table_name.to_string(), generated_ddl.clone())
                .await;

            generated_ddl
        };

        // Execute DDL
        info!("Creating table '{}': {}", table_name, ddl);
        self.db_executor
            .execute_ddl(&ddl)
            .await
            .with_context(|| format!("Failed to execute CREATE TABLE for '{}'", table_name))?;

        info!("Successfully created table '{}'", table_name);
        Ok(())
    }

    /// Load data with transaction support
    async fn load_with_transaction(
        &self,
        schema: &TableSchema,
        rows: Vec<Map<String, Value>>,
    ) -> Result<u64> {
        // Begin transaction
        self.db_executor
            .begin_transaction()
            .await
            .context("Failed to begin transaction")?;

        // Generate INSERT SQL
        let insert_sql = self.generate_insert_sql(schema)?;
        debug!("Insert SQL: {}", insert_sql);

        // Execute batch inserts
        match self
            .db_executor
            .execute_batch_insert(&insert_sql, rows)
            .await
        {
            Ok(count) => {
                // Commit transaction
                self.db_executor
                    .commit()
                    .await
                    .context("Failed to commit transaction")?;

                Ok(count)
            }
            Err(e) => {
                // Rollback on error
                error!("Batch insert failed, rolling back: {}", e);
                self.db_executor
                    .rollback()
                    .await
                    .context("Failed to rollback transaction after error")?;

                Err(e).context("Batch insert failed")
            }
        }
    }

    /// Generate INSERT SQL for a table schema
    fn generate_insert_sql(&self, schema: &TableSchema) -> Result<String> {
        if schema.columns.is_empty() {
            return Err(anyhow!("Cannot generate INSERT for schema with no columns"));
        }

        let table_name = &schema.table_name;
        let column_names: Vec<String> = schema.columns.iter().map(|c| c.name.clone()).collect();

        let columns_clause = column_names.join(", ");
        let placeholders = column_names
            .iter()
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(", ");

        Ok(format!(
            "INSERT INTO {} ({}) VALUES ({})",
            table_name, columns_clause, placeholders
        ))
    }

    /// Get cache statistics
    pub async fn cache_statistics(&self) -> CacheStatistics {
        self.cache.statistics().await
    }

    /// Clear all caches
    pub async fn clear_cache(&self) {
        self.cache.clear().await;
        info!("Cleared all caches");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tokio::sync::RwLock;

    // Mock implementations for testing

    struct MockSchemaProvider {
        entities: HashMap<String, EntityDefinition>,
    }

    #[async_trait::async_trait]
    impl OntologySchemaProvider for MockSchemaProvider {
        async fn get_entity_definition(&self, entity_uri: &str) -> Result<EntityDefinition> {
            self.entities
                .get(entity_uri)
                .cloned()
                .ok_or_else(|| anyhow!("Entity not found: {}", entity_uri))
        }

        async fn get_all_entities(&self) -> Result<Vec<String>> {
            Ok(self.entities.keys().cloned().collect())
        }

        async fn resolve_relationships(
            &self,
            _entity_uri: &str,
        ) -> Result<Vec<RelationshipDefinition>> {
            Ok(vec![])
        }

        async fn entity_exists(&self, entity_uri: &str) -> Result<bool> {
            Ok(self.entities.contains_key(entity_uri))
        }
    }

    struct MockNormalizationStrategy {
        mode: NormalizationMode,
    }

    #[async_trait::async_trait]
    impl NormalizationStrategy for MockNormalizationStrategy {
        async fn generate_schemas(&self, entity: &EntityDefinition) -> Result<Vec<TableSchema>> {
            let mut schema = TableSchema::new(entity.label.to_lowercase());

            // Add ID column
            schema.add_column(
                ColumnDefinition::new("id".to_string(), "INTEGER".to_string(), false)
                    .as_primary_key(),
            );
            schema.add_primary_key("id".to_string());

            // Add property columns
            for prop in &entity.properties {
                schema.add_column(ColumnDefinition::new(
                    prop.label.to_lowercase(),
                    "VARCHAR(255)".to_string(),
                    !prop.required,
                ));
            }

            Ok(vec![schema])
        }

        fn get_mode(&self) -> NormalizationMode {
            self.mode
        }
    }

    struct MockDbExecutor {
        tables: Arc<RwLock<HashMap<String, bool>>>,
        rows_inserted: Arc<RwLock<u64>>,
        transaction_active: Arc<RwLock<bool>>,
    }

    impl MockDbExecutor {
        fn new() -> Self {
            Self {
                tables: Arc::new(RwLock::new(HashMap::new())),
                rows_inserted: Arc::new(RwLock::new(0)),
                transaction_active: Arc::new(RwLock::new(false)),
            }
        }
    }

    #[async_trait::async_trait]
    impl DbExecutor for MockDbExecutor {
        async fn execute_ddl(&self, sql: &str) -> Result<()> {
            if sql.starts_with("CREATE TABLE") {
                // Extract table name (simplified)
                let parts: Vec<&str> = sql.split_whitespace().collect();
                if parts.len() >= 3 {
                    let qualified = parts[2];
                    let normalized = qualified
                        .split('.')
                        .next_back()
                        .unwrap_or(qualified)
                        .trim_matches('"')
                        .to_lowercase();

                    let mut tables = self.tables.write().await;
                    tables.insert(qualified.to_string(), true);
                    tables.insert(normalized, true);
                }
            }
            Ok(())
        }

        async fn table_exists(&self, table_name: &str) -> Result<bool> {
            Ok(self.tables.read().await.contains_key(table_name))
        }

        async fn execute_batch_insert(
            &self,
            _sql: &str,
            rows: Vec<Map<String, Value>>,
        ) -> Result<u64> {
            let count = rows.len() as u64;
            *self.rows_inserted.write().await += count;
            Ok(count)
        }

        async fn begin_transaction(&self) -> Result<()> {
            *self.transaction_active.write().await = true;
            Ok(())
        }

        async fn commit(&self) -> Result<()> {
            *self.transaction_active.write().await = false;
            Ok(())
        }

        async fn rollback(&self) -> Result<()> {
            *self.transaction_active.write().await = false;
            Ok(())
        }
    }

    fn create_test_loader() -> (OntologyDrivenLoader, Arc<MockDbExecutor>) {
        let mut entities = HashMap::new();
        entities.insert(
            "http://example.org/Patient".to_string(),
            EntityDefinition {
                entity_uri: "http://example.org/Patient".to_string(),
                label: "Patient".to_string(),
                properties: vec![PropertyDefinition {
                    property_uri: "http://example.org/name".to_string(),
                    label: "name".to_string(),
                    range: "xsd:string".to_string(),
                    required: true,
                    multi_valued: false,
                }],
                relationships: vec![],
            },
        );

        let schema_provider = Arc::new(MockSchemaProvider { entities });
        let type_mapper = Arc::new(DB2TypeMapper::new());
        let ddl_generator = Arc::new(DB2DDLGenerator::new("DB2INST1".to_string()));
        let normalization_strategy = Arc::new(MockNormalizationStrategy {
            mode: NormalizationMode::Denormalized,
        });
        let data_transformer = Arc::new(DefaultDataTransformer::new());
        let relationship_resolver = Arc::new(DefaultRelationshipResolver::new());
        let cache = Arc::new(LruSchemaCache::new());
        let db_executor = Arc::new(MockDbExecutor::new());

        let loader = OntologyDrivenLoader::new(
            schema_provider,
            type_mapper,
            ddl_generator,
            normalization_strategy,
            data_transformer,
            relationship_resolver,
            cache as Arc<dyn SchemaCache>,
            db_executor.clone() as Arc<dyn DbExecutor>,
        );

        (loader, db_executor)
    }

    #[tokio::test]
    async fn test_load_ontology_data_success() {
        let (loader, db_executor) = create_test_loader();

        let mut row1 = Map::new();
        row1.insert("id".to_string(), Value::Number(1.into()));
        row1.insert("name".to_string(), Value::String("John Doe".to_string()));

        let mut row2 = Map::new();
        row2.insert("id".to_string(), Value::Number(2.into()));
        row2.insert("name".to_string(), Value::String("Jane Smith".to_string()));

        let rows = vec![row1, row2];

        let result = loader
            .load_ontology_data("http://example.org/Patient", rows, true)
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 2);

        // Verify table was created
        assert!(db_executor.table_exists("patient").await.unwrap());

        // Verify rows were inserted
        let inserted = *db_executor.rows_inserted.read().await;
        assert_eq!(inserted, 2);
    }

    #[tokio::test]
    async fn test_load_empty_rows() {
        let (loader, _) = create_test_loader();

        let rows = vec![];

        let result = loader
            .load_ontology_data("http://example.org/Patient", rows, true)
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_cache_usage() {
        let (loader, _) = create_test_loader();

        // First load - should fetch from provider
        let mut row = Map::new();
        row.insert("id".to_string(), Value::Number(1.into()));
        row.insert("name".to_string(), Value::String("Test".to_string()));

        loader
            .load_ontology_data("http://example.org/Patient", vec![row.clone()], true)
            .await
            .unwrap();

        let stats1 = loader.cache_statistics().await;
        assert_eq!(stats1.entity_cache_size, 1);

        // Second load - should use cache
        loader
            .load_ontology_data("http://example.org/Patient", vec![row], true)
            .await
            .unwrap();

        let stats2 = loader.cache_statistics().await;
        assert!(stats2.entity_hits > 0);
    }

    #[tokio::test]
    async fn test_invalid_entity_uri() {
        let (loader, _) = create_test_loader();

        let mut row = Map::new();
        row.insert("id".to_string(), Value::Number(1.into()));

        let result = loader
            .load_ontology_data("http://example.org/NonExistent", vec![row], true)
            .await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Failed to get entity definition"));
    }

    #[tokio::test]
    async fn test_generate_insert_sql() {
        let (loader, _) = create_test_loader();

        let mut schema = TableSchema::new("test_table".to_string());
        schema.add_column(ColumnDefinition::new(
            "id".to_string(),
            "INTEGER".to_string(),
            false,
        ));
        schema.add_column(ColumnDefinition::new(
            "name".to_string(),
            "VARCHAR(255)".to_string(),
            true,
        ));

        let sql = loader.generate_insert_sql(&schema).unwrap();

        assert!(sql.contains("INSERT INTO test_table"));
        assert!(sql.contains("(id, name)"));
        assert!(sql.contains("VALUES (?, ?)"));
    }
}
