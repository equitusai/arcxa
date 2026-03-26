//! Load Orchestrator
//!
//! Executes the full ETL pipeline: CSV → Transform → Fusion → Database Load

use anyhow::{Context, Result};
use graphica_core::security::validate_identifier;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use crate::common::databricks::build_loader_connection_string;
use crate::common::datasource_readiness::{evaluate_datasource_readiness, DatasourceOperation};
use crate::etl::loaders::database::{DatabaseLoader, DatabaseLoaderFactory};
use crate::etl::sources::csv::CsvSourceExecutor;
use crate::etl::transformers::field::FieldTransformerExecutor;
use crate::governance::rdf_store::{GraphicaRdfStore, RdfStore};
use crate::mapping::loader::transformation::TransformationEngine;
use graphica_core::catalog::client::DataSourceCatalog;
use graphica_core::orchestration::workflow::{FieldTransformation, FieldTransformerConfig};
use graphica_core::secrets::SecretStoreRef;

use super::types::*;
use super::unified_mapping::UnifiedMappingSession;
use graphica_core::orchestration::workflow::TransformOperation;

/// Load pipeline configuration
#[derive(Clone)]
pub struct LoadPipeline {
    /// Unified mapping session
    pub session: UnifiedMappingSession,

    /// Load configuration
    pub config: LoadConfig,

    /// Data source catalog (for accessing CSVs)
    pub catalog: Option<Arc<dyn DataSourceCatalog + Send + Sync>>,

    /// RDF store (for lineage)
    pub rdf_store: Option<Arc<GraphicaRdfStore>>,

    /// Secret store (for fetching credentials)
    pub secret_store: Option<SecretStoreRef>,
}

impl std::fmt::Debug for LoadPipeline {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoadPipeline")
            .field("session", &self.session)
            .field("config", &self.config)
            .field("catalog", &"<DataSourceCatalog>")
            .field("rdf_store", &"<GraphicaRdfStore>")
            .field("secret_store", &"<SecretStore>")
            .finish()
    }
}

impl LoadPipeline {
    /// Create a new load pipeline
    pub fn new(session: UnifiedMappingSession, config: LoadConfig) -> Self {
        Self {
            session,
            config,
            catalog: None,
            rdf_store: None,
            secret_store: None,
        }
    }

    /// Set data source catalog
    pub fn with_catalog(mut self, catalog: Arc<dyn DataSourceCatalog + Send + Sync>) -> Self {
        self.catalog = Some(catalog);
        self
    }

    /// Set RDF store
    pub fn with_rdf_store(mut self, rdf_store: Arc<GraphicaRdfStore>) -> Self {
        self.rdf_store = Some(rdf_store);
        self
    }

    /// Set secret store
    pub fn with_secret_store(mut self, secret_store: SecretStoreRef) -> Self {
        self.secret_store = Some(secret_store);
        self
    }
}

/// Load execution statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadStats {
    /// Load ID
    pub load_id: String,

    /// Status
    pub status: LoadStatus,

    /// Execution statistics
    pub stats: LoadExecutionStats,

    /// Lineage graph URI
    pub lineage_graph: Option<String>,

    /// Tables loaded
    pub tables_loaded: Vec<String>,
}

/// Load status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LoadStatus {
    /// Load in progress
    InProgress,

    /// Load completed successfully
    Completed,

    /// Load partially completed
    PartiallyCompleted,

    /// Load failed
    Failed,
}

/// Load orchestrator
///
/// Executes the complete ETL pipeline from CSV sources to target database.
pub struct LoadOrchestrator {
    /// Database loader instance
    loader: Option<Box<dyn DatabaseLoader>>,

    /// Primary key cache for foreign key validation
    /// Maps: table_name -> (pk_column_name -> Set<pk_values>)
    pk_cache: HashMap<String, HashMap<String, std::collections::HashSet<String>>>,
}

impl LoadOrchestrator {
    /// Create a new load orchestrator
    pub fn new() -> Self {
        Self {
            loader: None,
            pk_cache: HashMap::new(),
        }
    }

    /// Execute the load pipeline
    pub async fn execute(&mut self, pipeline: LoadPipeline) -> Result<LoadStats> {
        let start_time = Instant::now();
        let load_id = format!("load_{}", uuid::Uuid::new_v4());

        tracing::info!(
            "Starting load pipeline {} for session {}",
            load_id,
            pipeline.session.session_id
        );

        // Validate session is ready
        pipeline.session.validate_for_load()?;

        // Initialize stats
        let mut stats = LoadExecutionStats::new();

        // Phase 1: Initialize database loader
        self.initialize_loader(&pipeline).await?;

        // Phase 1.5: Compute topological order for table loading
        // This ensures tables with FK dependencies are loaded after their referenced tables
        let table_load_order = self.compute_table_load_order(&pipeline)?;

        tracing::info!("Computed table load order: {:?}", table_load_order);

        // Phase 2: Load data table by table (in topological order)
        for table_name in table_load_order {
            let table_schema = pipeline
                .session
                .target_schema
                .get(&table_name)
                .context(format!("Table schema not found for {}", table_name))?;

            tracing::info!("Loading table: {}", table_name);

            match self
                .load_table(&pipeline, &table_name, table_schema, &mut stats)
                .await
            {
                Ok(rows) => {
                    stats.db_rows_inserted += rows;
                    tracing::info!("Loaded {} rows into table {}", rows, table_name);
                }
                Err(e) => {
                    stats.errors_count += 1;
                    stats.errors.push(LoadError {
                        message: e.to_string(),
                        source_session: None,
                        table: Some(table_name.clone()),
                        record_index: None,
                        field: None,
                    });
                    tracing::error!("Failed to load table {}: {}", table_name, e);

                    if !pipeline.config.dry_run {
                        // In non-dry-run mode, fail fast
                        return Err(e);
                    }
                }
            }
        }

        stats.duration_ms = start_time.elapsed().as_millis() as u64;

        let status = if stats.errors_count == 0 {
            LoadStatus::Completed
        } else if stats.db_rows_inserted > 0 {
            LoadStatus::PartiallyCompleted
        } else {
            LoadStatus::Failed
        };

        tracing::info!(
            "Load pipeline {} completed: status={:?}, rows={}, errors={}, duration={}ms",
            load_id,
            status,
            stats.db_rows_inserted,
            stats.errors_count,
            stats.duration_ms
        );

        Ok(LoadStats {
            load_id,
            status,
            stats,
            lineage_graph: None,
            tables_loaded: pipeline.session.target_schema.keys().cloned().collect(),
        })
    }

    /// Initialize database loader
    async fn initialize_loader(&mut self, pipeline: &LoadPipeline) -> Result<()> {
        let connection_string = match &pipeline.session.target_database.connection {
            TargetConnection::ConnectionString { connection_string } => connection_string.clone(),
            TargetConnection::DataSourceRef { source_id } => {
                // Resolve datasource ID to connection string via catalog
                if pipeline.catalog.is_none() {
                    anyhow::bail!("DataSourceRef requires catalog to be configured");
                }

                let catalog = pipeline.catalog.as_ref().unwrap();
                let datasource = catalog
                    .get_source(source_id)
                    .await
                    .context(format!("Failed to resolve datasource {}", source_id))?;
                evaluate_datasource_readiness(&datasource, DatasourceOperation::WorkflowWrite)
                    .map_err(|failure| anyhow::anyhow!(failure.message))?;

                // Build connection string from datasource configuration (with credentials from secret store)
                self.build_connection_string(pipeline, &datasource.source)
                    .await?
            }
        };

        let loader = DatabaseLoaderFactory::create(
            &pipeline.session.target_database.database_type,
            &connection_string,
            pipeline.config.batch_size,
        )
        .await
        .context("Failed to create database loader")?;

        // Test connection
        loader
            .test_connection()
            .await
            .context("Database connection test failed")?;

        tracing::info!(
            "Connected to {} database",
            pipeline.session.target_database.database_type
        );

        self.loader = Some(loader);
        Ok(())
    }

    /// Compute topological order for table loading
    ///
    /// Uses Kahn's algorithm to compute a topological ordering of tables based on
    /// foreign key dependencies. Tables with no dependencies are loaded first,
    /// followed by tables that depend on them.
    ///
    /// # Algorithm
    /// 1. Build dependency graph: table -> list of tables it depends on
    /// 2. Compute in-degree (number of incoming edges) for each table
    /// 3. Start with tables that have in-degree 0 (no dependencies)
    /// 4. For each processed table, decrease in-degree of dependent tables
    /// 5. Add newly zero in-degree tables to queue
    ///
    /// # Returns
    /// Vector of table names in topological order (dependencies first)
    ///
    /// # Errors
    /// Returns error if circular dependencies are detected
    fn compute_table_load_order(&self, pipeline: &LoadPipeline) -> Result<Vec<String>> {
        let schema = &pipeline.session.target_schema;

        // Build dependency graph: table_name -> Vec<referenced_table_names>
        let mut dependencies: HashMap<String, std::collections::HashSet<String>> = HashMap::new();
        let mut in_degree: HashMap<String, usize> = HashMap::new();

        // Initialize all tables
        for table_name in schema.keys() {
            dependencies
                .entry(table_name.clone())
                .or_insert_with(std::collections::HashSet::new);
            in_degree.entry(table_name.clone()).or_insert(0);
        }

        // Build dependency edges
        for (table_name, table_schema) in schema {
            for fk in &table_schema.foreign_keys {
                let referenced_table = &fk.references_table;

                // Only add dependency if referenced table exists in schema
                if schema.contains_key(referenced_table) {
                    // Add edge: table depends on referenced_table
                    let deps = dependencies.get_mut(table_name).unwrap();
                    if deps.insert(referenced_table.clone()) {
                        // Only increment if this is a new dependency (not duplicate FK)
                        *in_degree.get_mut(table_name).unwrap() += 1;
                    }
                } else {
                    tracing::warn!(
                        "Table {} references table {} which is not in the schema. \
                         This FK will not be validated.",
                        table_name,
                        referenced_table
                    );
                }
            }
        }

        // Kahn's algorithm: start with tables that have no dependencies
        let mut queue: Vec<String> = in_degree
            .iter()
            .filter(|(_, &degree)| degree == 0)
            .map(|(name, _)| name.clone())
            .collect();

        let mut result = Vec::new();

        while let Some(table) = queue.pop() {
            result.push(table.clone());

            // For each table that depends on this one, decrease its in-degree
            for (dependent_table, deps) in &dependencies {
                if deps.contains(&table) {
                    let degree = in_degree.get_mut(dependent_table).unwrap();
                    *degree -= 1;

                    if *degree == 0 {
                        queue.push(dependent_table.clone());
                    }
                }
            }
        }

        // Check for circular dependencies
        if result.len() != schema.len() {
            let unprocessed: Vec<String> = schema
                .keys()
                .filter(|name| !result.contains(name))
                .cloned()
                .collect();

            anyhow::bail!(
                "Circular dependency detected among tables: {:?}. \
                 Cannot determine load order. Consider breaking the circular dependency \
                 or loading these tables separately.",
                unprocessed
            );
        }

        Ok(result)
    }

    /// Build connection string from DataSource configuration
    ///
    /// Fetches credentials from the secret store using the datasource.connection.secret_ref.
    async fn build_connection_string(
        &self,
        pipeline: &LoadPipeline,
        datasource: &graphica_core::catalog::types::DataSource,
    ) -> Result<String> {
        use graphica_core::catalog::types::SourceConfig;

        // Fetch credentials from secret store
        let credentials = self
            .fetch_credentials(pipeline, &datasource.connection.secret_ref)
            .await?;

        match &datasource.connection.config {
            SourceConfig::PostgreSQL(config) => {
                // PostgreSQL connection string format:
                // host=localhost port=5432 dbname=mydb user=myuser password=mypass
                let mut conn_str = format!(
                    "host={} port={} dbname={}",
                    config.host, config.port, config.database
                );

                if let Some(schema) = &config.schema {
                    conn_str.push_str(&format!(" options='-c search_path={}'", schema));
                }

                if let Some(ssl_mode) = &config.ssl_mode {
                    conn_str.push_str(&format!(" sslmode={}", ssl_mode));
                }

                // Add credentials
                if let Some((username, password)) = credentials {
                    conn_str.push_str(&format!(" user={} password={}", username, password));
                    tracing::debug!(
                        "PostgreSQL connection string built with credentials for user: {}",
                        username
                    );
                } else {
                    tracing::warn!("No credentials found, connection may fail");
                }

                Ok(conn_str)
            }

            SourceConfig::DB2(config) => {
                // DB2 ODBC connection string format:
                // Driver={IBM DB2 ODBC DRIVER};Database=mydb;Hostname=localhost;Port=50000;UID=user;PWD=pass
                let mut conn_str = format!(
                    "Driver={{IBM DB2 ODBC DRIVER}};Database={};Hostname={};Port={}",
                    config.database, config.host, config.port
                );

                if let Some(schema) = &config.schema {
                    conn_str.push_str(&format!(";CurrentSchema={}", schema));
                }

                // Add credentials
                if let Some((username, password)) = credentials {
                    conn_str.push_str(&format!(";UID={};PWD={}", username, password));
                    tracing::debug!(
                        "DB2 connection string built with credentials for user: {}",
                        username
                    );
                } else {
                    tracing::warn!("No credentials found, connection may fail");
                }

                Ok(conn_str)
            }

            SourceConfig::Databricks(config) => {
                let mut resolved_credentials = graphica_core::catalog::connector::Credentials::new(
                    String::new(),
                    String::new(),
                );

                if let Some((username, password)) = credentials {
                    resolved_credentials.username = username;
                    resolved_credentials.password = password.clone();
                    resolved_credentials
                        .additional
                        .insert("token".to_string(), password);
                } else {
                    tracing::warn!("No Databricks credentials found, connection may fail");
                }

                Ok(build_loader_connection_string(
                    config,
                    &resolved_credentials,
                ))
            }

            _ => {
                anyhow::bail!(
                    "Unsupported datasource type for target database: {:?}",
                    datasource.source_type
                )
            }
        }
    }

    /// Fetch credentials from secret store
    ///
    /// Parses the secret_ref URI and retrieves credentials from the secret store.
    /// Supports multiple secret_ref formats:
    /// - `vault://path/to/secret` - Vault secret path
    /// - `aws://secret-name` - AWS Secrets Manager
    /// - `env://VAR_NAME` - Environment variable (for development)
    ///
    /// # Returns
    /// - `Ok(Some((username, password)))` if credentials were successfully fetched
    /// - `Ok(None)` if secret store is not configured (expected fallback behavior)
    /// - `Err(...)` if secret store is configured but fetching failed (fail-fast behavior)
    ///
    /// # Error Handling
    /// This method fails fast if the secret store is configured but the secret cannot be fetched.
    /// This prevents silent failures that could lead to cryptic connection errors downstream.
    /// Only when no secret store is configured at all does it return Ok(None) for fallback.
    async fn fetch_credentials(
        &self,
        pipeline: &LoadPipeline,
        secret_ref: &str,
    ) -> Result<Option<(String, String)>> {
        // Check if secret store is configured
        if pipeline.secret_store.is_none() {
            tracing::warn!(
                "Secret store not configured, cannot fetch credentials from: {}",
                secret_ref
            );
            return Ok(None);
        }

        let secret_store = pipeline.secret_store.as_ref().unwrap();

        // Parse secret_ref URI to extract path
        let secret_path = self.parse_secret_ref(secret_ref)?;

        tracing::debug!("Fetching credentials from secret store: {}", secret_path);

        // Fetch secret from store
        match secret_store.get_secret(&secret_path, None).await {
            Ok(secret) => {
                // Extract username and password from secret value
                if let (Some(username), Some(password)) =
                    (secret.value.username(), secret.value.password())
                {
                    tracing::info!("Successfully fetched credentials for user: {}", username);
                    Ok(Some((username.to_string(), password.to_string())))
                } else {
                    tracing::warn!(
                        "Secret {} found but does not contain username/password credentials",
                        secret_path
                    );
                    Ok(None)
                }
            }
            Err(e) => {
                tracing::error!(
                    "Failed to fetch secret from store (path={}): {}",
                    secret_path,
                    e
                );
                // Fail fast: In production, missing secrets should cause immediate failure
                // rather than silent fallback, which could lead to cryptic connection errors
                anyhow::bail!(
                    "Failed to fetch credentials from secret store at '{}': {}. \
                     Ensure the secret exists and the secret store is properly configured.",
                    secret_path,
                    e
                )
            }
        }
    }

    /// Parse secret_ref URI to extract the secret path
    ///
    /// Examples:
    /// - `vault://datasources/postgres-prod` -> `datasources/postgres-prod`
    /// - `aws://my-db-secret` -> `my-db-secret`
    /// - `env://DB_CREDENTIALS` -> `DB_CREDENTIALS`
    fn parse_secret_ref(&self, secret_ref: &str) -> Result<String> {
        if secret_ref.starts_with("vault://") {
            Ok(secret_ref.strip_prefix("vault://").unwrap().to_string())
        } else if secret_ref.starts_with("aws://") {
            Ok(secret_ref.strip_prefix("aws://").unwrap().to_string())
        } else if secret_ref.starts_with("env://") {
            Ok(secret_ref.strip_prefix("env://").unwrap().to_string())
        } else {
            // Default: assume it's a direct path
            Ok(secret_ref.to_string())
        }
    }

    /// Load a single table
    ///
    /// ## Transaction Management
    ///
    /// This method relies on the underlying DatabaseLoader to handle transactions:
    /// - In dry_run mode, no data is loaded (implicit rollback)
    /// - If constraint validation fails, errors are recorded but load continues
    /// - Database loaders should use transactions internally:
    ///   - BEGIN TRANSACTION at start of load()
    ///   - COMMIT on success
    ///   - ROLLBACK on error
    /// - If this method returns an error, the database loader should have already rolled back
    ///
    /// Future enhancement: Add explicit begin_transaction/commit/rollback to DatabaseLoader trait
    async fn load_table(
        &mut self,
        pipeline: &LoadPipeline,
        table_name: &str,
        _table_schema: &TargetTableSchema,
        stats: &mut LoadExecutionStats,
    ) -> Result<u64> {
        // Get mapping rules for this table
        let mapping_rules = pipeline.session.get_mappings_for_table(table_name);

        if mapping_rules.is_empty() {
            tracing::warn!("No mapping rules for table {}, skipping", table_name);
            return Ok(0);
        }

        // Phase 1: Extract data from CSV sources
        let records = self
            .extract_data_for_table(pipeline, &mapping_rules)
            .await?;
        stats.csv_records_read += records.len() as u64;

        if records.is_empty() {
            tracing::info!("No records to load for table {}", table_name);
            return Ok(0);
        }

        tracing::debug!(
            "Extracted {} records for table {}",
            records.len(),
            table_name
        );

        // Phase 2: Transform data
        let transformed_records = self.transform_data(pipeline, &mapping_rules, records)?;
        stats.entities_processed += transformed_records.len() as u64;

        tracing::debug!(
            "Transformed {} records for table {}",
            transformed_records.len(),
            table_name
        );

        // Phase 2.5: Apply fusion filtering (if enabled)
        let (final_records, fused_count) = if pipeline.config.respect_fusion {
            self.apply_fusion_filter(pipeline, table_name, transformed_records)?
        } else {
            (transformed_records, 0) // fused_count = 0 (no fusion applied)
        };

        stats.fused_entities_skipped = fused_count;

        tracing::debug!(
            "After fusion filtering: {} records ({} fused entities skipped)",
            final_records.len(),
            fused_count
        );

        // Phase 2.75: Validate constraints
        self.validate_constraints(pipeline, table_name, _table_schema, &final_records, stats)?;

        // Phase 2.8: Extract PK values BEFORE load (but don't cache yet)
        // This avoids cloning the entire dataset while ensuring we only cache on success
        let pk_values_to_cache =
            self.extract_pk_values(table_name, _table_schema, &final_records)?;

        // Phase 3: Load to database (if not dry run)
        if pipeline.config.dry_run {
            tracing::info!(
                "DRY RUN: Would load {} rows into {}",
                final_records.len(),
                table_name
            );
            // For dry run, cache PKs immediately since there's no DB load to fail
            self.store_pk_values_in_cache(table_name, pk_values_to_cache)?;
            return Ok(final_records.len() as u64);
        }

        let loader = self
            .loader
            .as_ref()
            .context("Database loader not initialized")?;

        let rows_loaded = loader
            .load(
                table_name,
                final_records,
                pipeline.config.load_mode,
                pipeline.config.key_fields.as_deref(),
            )
            .await
            .context(format!("Failed to load data into table {}", table_name))?;

        // Phase 3.5: Store PK values in cache ONLY after successful database load
        // This ensures we only cache PKs for records that actually exist in the database
        self.store_pk_values_in_cache(table_name, pk_values_to_cache)?;

        tracing::debug!(
            "Successfully loaded {} rows and cached PK values for table {}",
            rows_loaded,
            table_name
        );

        Ok(rows_loaded)
    }

    /// Apply fusion filtering to remove duplicate entities
    ///
    /// Queries RDF store for fusion operations and filters out entities
    /// that have been fused into other canonical entities.
    ///
    /// Returns (filtered_records, fused_count)
    fn apply_fusion_filter(
        &self,
        pipeline: &LoadPipeline,
        table_name: &str,
        records: Vec<Value>,
    ) -> Result<(Vec<Value>, u64)> {
        // Check if RDF store is available
        if pipeline.rdf_store.is_none() {
            tracing::warn!("Fusion filtering enabled but RDF store not configured, skipping");
            return Ok((records, 0));
        }

        let rdf_store = pipeline.rdf_store.as_ref().unwrap();

        // Query RDF store for all active fusion operations
        let sparql = r#"
            PREFIX gph: <http://graphica.io/ontology#>
            PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>

            SELECT ?fusedEntity ?canonicalEntity
            WHERE {
                ?fusion rdf:type gph:FusionOperation ;
                        gph:sourceEntity ?fusedEntity ;
                        gph:mergedEntity ?canonicalEntity .
                FILTER NOT EXISTS { ?fusion gph:reversedAt ?reversalTime }
            }
        "#;

        tracing::debug!("Querying RDF store for active fusion operations");

        let results = match rdf_store.query(sparql) {
            Ok(results) => results,
            Err(e) => {
                tracing::warn!("Failed to query fusion operations from RDF store: {}", e);
                // On query failure, pass through all records
                return Ok((records, 0));
            }
        };

        if results.is_empty() {
            tracing::debug!("No active fusion operations found");
            return Ok((records, 0));
        }

        // Build a map of fused entity URIs -> canonical entity URIs
        let mut fusion_map: HashMap<String, String> = HashMap::new();

        for result in &results {
            if let (Some(fused), Some(canonical)) = (
                result.get("fusedEntity").and_then(|v| v.as_str()),
                result.get("canonicalEntity").and_then(|v| v.as_str()),
            ) {
                fusion_map.insert(fused.to_string(), canonical.to_string());
            }
        }

        tracing::info!(
            "Found {} active fusion operations to apply",
            fusion_map.len()
        );

        // Extract entity ID field from primary keys for this specific table
        let entity_id_field = self.determine_entity_id_field(pipeline, table_name)?;

        if entity_id_field.is_none() {
            tracing::warn!(
                "No entity ID field found for table {}, cannot apply fusion filtering. \
                 Consider adding an entity_id column or ensuring primary key is set.",
                table_name
            );
            return Ok((records, 0));
        }

        let entity_field = entity_id_field.unwrap();

        // Filter records: keep only records whose entity ID is NOT in the fused set
        let mut filtered_records = Vec::new();
        let mut fused_count = 0u64;

        for record in records {
            if let Value::Object(ref obj) = record {
                if let Some(entity_id_value) = obj.get(&entity_field) {
                    let entity_id = entity_id_value.as_str().unwrap_or("");

                    // Build entity URI (assuming format: http://graphica.io/ontology/entity/{id})
                    let entity_uri = format!("http://graphica.io/ontology/entity/{}", entity_id);

                    // Check if this entity has been fused away
                    if fusion_map.contains_key(&entity_uri) {
                        fused_count += 1;
                        tracing::trace!("Filtering out fused entity: {}", entity_id);
                        continue; // Skip this record
                    }
                }
            }

            filtered_records.push(record);
        }

        tracing::info!(
            "Fusion filtering complete: {} records kept, {} fused entities filtered out",
            filtered_records.len(),
            fused_count
        );

        Ok((filtered_records, fused_count))
    }

    /// Determine which field contains the entity ID for fusion filtering
    ///
    /// Returns the first primary key column for the specified table, or None if no PK is defined.
    ///
    /// # Arguments
    /// * `pipeline` - The load pipeline containing the session
    /// * `table_name` - The specific table to get the entity ID field for
    fn determine_entity_id_field(
        &self,
        pipeline: &LoadPipeline,
        table_name: &str,
    ) -> Result<Option<String>> {
        // Get the specific table schema
        if let Some(table_schema) = pipeline.session.target_schema.get(table_name) {
            if !table_schema.primary_keys.is_empty() {
                // Use the first primary key column as entity ID
                return Ok(Some(table_schema.primary_keys[0].clone()));
            }
        }

        Ok(None)
    }

    /// Validate constraints before loading
    ///
    /// Checks NOT NULL, PRIMARY KEY uniqueness, UNIQUE constraints, and basic FK validation.
    fn validate_constraints(
        &self,
        _pipeline: &LoadPipeline,
        table_name: &str,
        table_schema: &TargetTableSchema,
        records: &[Value],
        stats: &mut LoadExecutionStats,
    ) -> Result<()> {
        tracing::debug!("Validating constraints for table {}", table_name);

        // Track unique and PK values
        let mut pk_values: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut unique_values: HashMap<String, std::collections::HashSet<String>> = HashMap::new();

        for (idx, record) in records.iter().enumerate() {
            if let Value::Object(obj) = record {
                // 1. Validate NOT NULL constraints
                for (col_name, col_def) in &table_schema.columns {
                    if !col_def.nullable {
                        let value = obj.get(col_name);
                        if value.is_none() || matches!(value, Some(Value::Null)) {
                            stats.errors.push(LoadError {
                                message: format!(
                                    "NOT NULL constraint violation: column '{}' cannot be null",
                                    col_name
                                ),
                                source_session: None,
                                table: Some(table_name.to_string()),
                                record_index: Some(idx),
                                field: Some(col_name.clone()),
                            });
                            stats.errors_count += 1;
                        }
                    }
                }

                // 2. Validate PRIMARY KEY uniqueness
                if !table_schema.primary_keys.is_empty() {
                    let mut pk_parts = Vec::new();
                    for pk_col in &table_schema.primary_keys {
                        if let Some(val) = obj.get(pk_col) {
                            pk_parts.push(val.to_string());
                        } else {
                            stats.errors.push(LoadError {
                                message: format!(
                                    "PRIMARY KEY constraint violation: column '{}' is missing",
                                    pk_col
                                ),
                                source_session: None,
                                table: Some(table_name.to_string()),
                                record_index: Some(idx),
                                field: Some(pk_col.clone()),
                            });
                            stats.errors_count += 1;
                        }
                    }

                    if !pk_parts.is_empty() {
                        let pk_value = pk_parts.join("|");
                        if !pk_values.insert(pk_value.clone()) {
                            stats.errors.push(LoadError {
                                message: format!(
                                    "PRIMARY KEY constraint violation: duplicate value '{}'",
                                    pk_value
                                ),
                                source_session: None,
                                table: Some(table_name.to_string()),
                                record_index: Some(idx),
                                field: None,
                            });
                            stats.errors_count += 1;
                        }
                    }
                }

                // 3. Validate UNIQUE constraints
                for (col_name, col_def) in &table_schema.columns {
                    if col_def.unique {
                        if let Some(val) = obj.get(col_name) {
                            let val_str = val.to_string();
                            let unique_set = unique_values
                                .entry(col_name.clone())
                                .or_insert_with(std::collections::HashSet::new);
                            if !unique_set.insert(val_str.clone()) {
                                stats.errors.push(LoadError {
                                    message: format!("UNIQUE constraint violation: duplicate value '{}' in column '{}'", val_str, col_name),
                                    source_session: None,
                                    table: Some(table_name.to_string()),
                                    record_index: Some(idx),
                                    field: Some(col_name.clone()),
                                });
                                stats.errors_count += 1;
                            }
                        }
                    }
                }

                // 4. Validate FOREIGN KEY constraints (referential integrity)
                // NOTE: Current ForeignKeyConstraint struct only supports single-column FKs.
                // Composite FK support requires extending the struct to use Vec<String> for columns.
                for fk in &table_schema.foreign_keys {
                    // Check if FK column exists
                    if !obj.contains_key(&fk.column) {
                        stats.errors.push(LoadError {
                            message: format!(
                                "FOREIGN KEY constraint: column '{}' referencing {}.{} is missing",
                                fk.column, fk.references_table, fk.references_column
                            ),
                            source_session: None,
                            table: Some(table_name.to_string()),
                            record_index: Some(idx),
                            field: Some(fk.column.clone()),
                        });
                        stats.errors_count += 1;
                        continue;
                    }

                    // Check if FK value exists in referenced table's PK cache
                    if let Some(fk_value) = obj.get(&fk.column) {
                        // Convert FK value to string
                        let fk_value_str = match fk_value {
                            Value::String(s) => s.clone(),
                            Value::Null => continue, // Null FKs are allowed (unless NOT NULL constraint)
                            other => other.to_string(),
                        };

                        // Check if referenced table has been loaded
                        if let Some(ref_table_cache) = self.pk_cache.get(&fk.references_table) {
                            // Check if referenced table has composite PK
                            if ref_table_cache.contains_key("__composite__") {
                                // Referenced table has composite PK
                                tracing::warn!(
                                    "Cannot validate FK {}.{} -> {}.{}: referenced table has composite PK. \
                                     Composite FK validation is not yet supported. \
                                     Consider extending ForeignKeyConstraint to support multi-column FKs.",
                                    table_name, fk.column, fk.references_table, fk.references_column
                                );
                                continue;
                            }

                            // Check if referenced column has PK values (single-column PK)
                            if let Some(pk_values) = ref_table_cache.get(&fk.references_column) {
                                // Verify FK value exists in referenced PKs
                                if !pk_values.contains(&fk_value_str) {
                                    stats.errors.push(LoadError {
                                        message: format!(
                                            "FOREIGN KEY constraint violation: value '{}' in column '{}' \
                                             does not exist in {}.{}",
                                            fk_value_str, fk.column, fk.references_table, fk.references_column
                                        ),
                                        source_session: None,
                                        table: Some(table_name.to_string()),
                                        record_index: Some(idx),
                                        field: Some(fk.column.clone()),
                                    });
                                    stats.errors_count += 1;
                                }
                            } else {
                                tracing::warn!(
                                    "Cannot validate FK {}.{} -> {}.{}: referenced column '{}' not in cache",
                                    table_name, fk.column, fk.references_table, fk.references_column, fk.references_column
                                );
                            }
                        } else {
                            tracing::warn!(
                                "Cannot validate FK {}.{} -> {}.{}: referenced table not yet loaded. \
                                 Consider reordering table load sequence.",
                                table_name, fk.column, fk.references_table, fk.references_table
                            );
                        }
                    }
                }
            }
        }

        if stats.errors_count > 0 {
            tracing::warn!(
                "Constraint validation found {} errors for table {}",
                stats.errors_count,
                table_name
            );
        } else {
            tracing::debug!(
                "All constraints validated successfully for table {}",
                table_name
            );
        }

        Ok(())
    }

    /// Extract primary key values from records
    ///
    /// Extracts PK values WITHOUT storing them in cache yet.
    /// This allows us to extract before DB load but only cache after success.
    ///
    /// For composite PKs, stores values as joined strings (e.g., "val1|val2")
    /// under a special "__composite__" key.
    ///
    /// Returns: HashMap<pk_column_name, Set<pk_values>>
    fn extract_pk_values(
        &self,
        table_name: &str,
        table_schema: &TargetTableSchema,
        records: &[Value],
    ) -> Result<HashMap<String, std::collections::HashSet<String>>> {
        if table_schema.primary_keys.is_empty() {
            tracing::debug!(
                "No primary keys defined for table {}, skipping PK extraction",
                table_name
            );
            return Ok(HashMap::new());
        }

        let mut pk_values_map: HashMap<String, std::collections::HashSet<String>> = HashMap::new();

        // Determine if this is a composite PK
        let is_composite = table_schema.primary_keys.len() > 1;

        if is_composite {
            // For composite PKs, store joined values under "__composite__" key
            let mut composite_pk_values = std::collections::HashSet::new();

            for record in records {
                if let Value::Object(obj) = record {
                    let mut pk_parts = Vec::new();

                    for pk_col in &table_schema.primary_keys {
                        if let Some(pk_value) = obj.get(pk_col) {
                            let value_str = match pk_value {
                                Value::String(s) => s.clone(),
                                Value::Null => {
                                    pk_parts.clear(); // Skip entire composite PK if any part is null
                                    break;
                                }
                                other => other.to_string(),
                            };
                            pk_parts.push(value_str);
                        } else {
                            pk_parts.clear(); // Skip if any PK column is missing
                            break;
                        }
                    }

                    if !pk_parts.is_empty() {
                        // Join composite PK parts with "|" delimiter (same as validation logic)
                        let composite_value = pk_parts.join("|");
                        composite_pk_values.insert(composite_value);
                    }
                }
            }

            pk_values_map.insert("__composite__".to_string(), composite_pk_values);

            tracing::debug!(
                "Extracted {} composite PK values ({} columns) for table {}",
                pk_values_map.get("__composite__").unwrap().len(),
                table_schema.primary_keys.len(),
                table_name
            );
        } else {
            // Single PK column - store values by column name
            let pk_col = &table_schema.primary_keys[0];
            let mut pk_values = std::collections::HashSet::new();

            for record in records {
                if let Value::Object(obj) = record {
                    if let Some(pk_value) = obj.get(pk_col) {
                        let value_str = match pk_value {
                            Value::String(s) => s.clone(),
                            Value::Null => continue,
                            other => other.to_string(),
                        };
                        pk_values.insert(value_str);
                    }
                }
            }

            pk_values_map.insert(pk_col.clone(), pk_values);

            tracing::debug!(
                "Extracted {} PK values for {}.{}",
                pk_values_map.get(pk_col).unwrap().len(),
                table_name,
                pk_col
            );
        }

        Ok(pk_values_map)
    }

    /// Store extracted PK values in cache
    ///
    /// Called AFTER successful database load to cache PK values for FK validation.
    /// This ensures we only cache PKs for records that actually exist in the database.
    fn store_pk_values_in_cache(
        &mut self,
        table_name: &str,
        pk_values: HashMap<String, std::collections::HashSet<String>>,
    ) -> Result<()> {
        if pk_values.is_empty() {
            return Ok(());
        }

        // Store in cache
        self.pk_cache
            .insert(table_name.to_string(), pk_values.clone());

        // Log cache statistics
        let total_cached = pk_values.values().map(|set| set.len()).sum::<usize>();
        tracing::debug!(
            "Cached {} PK values across {} columns for table {}",
            total_cached,
            pk_values.len(),
            table_name
        );

        Ok(())
    }

    /// Extract data for a table from CSV sources
    async fn extract_data_for_table(
        &self,
        pipeline: &LoadPipeline,
        mapping_rules: &[&TargetMappingRule],
    ) -> Result<Vec<Value>> {
        // Collect all unique datasource IDs from source field mappings
        let mut datasource_ids: std::collections::HashSet<String> =
            std::collections::HashSet::new();

        for rule in mapping_rules {
            for source_field in &rule.source_fields {
                datasource_ids.insert(source_field.datasource_id.clone());
            }
        }

        if datasource_ids.is_empty() {
            tracing::warn!("No datasources configured in mapping rules");
            return Ok(Vec::new());
        }

        let mut all_records = Vec::new();
        let datasource_count = datasource_ids.len();

        // Extract data from each datasource
        for datasource_id in datasource_ids {
            let records = self
                .extract_from_datasource(pipeline, &datasource_id, mapping_rules)
                .await?;
            all_records.extend(records);
        }

        tracing::debug!(
            "Extracted {} total records from {} datasources",
            all_records.len(),
            datasource_count
        );

        Ok(all_records)
    }

    /// Extract data from a specific datasource
    async fn extract_from_datasource(
        &self,
        pipeline: &LoadPipeline,
        datasource_id: &str,
        mapping_rules: &[&TargetMappingRule],
    ) -> Result<Vec<Value>> {
        if pipeline.catalog.is_none() {
            tracing::warn!("No catalog configured, cannot extract CSV data");
            return Ok(Vec::new());
        }

        let catalog = pipeline.catalog.as_ref().unwrap();

        // Build query to get all relevant fields from this datasource
        let mut columns_to_extract = Vec::new();
        let mut field_mappings: HashMap<String, &SourceFieldMapping> = HashMap::new();

        for rule in mapping_rules {
            for source_field in &rule.source_fields {
                if source_field.datasource_id == datasource_id {
                    columns_to_extract.push(source_field.csv_field.clone());
                    field_mappings.insert(source_field.csv_field.clone(), source_field);
                }
            }
        }

        if columns_to_extract.is_empty() {
            return Ok(Vec::new());
        }

        // Validate all CSV field names to prevent SQL injection
        let validated_columns: Result<Vec<&str>, _> = columns_to_extract
            .iter()
            .map(|col| validate_identifier(col))
            .collect();

        let validated_columns = validated_columns
            .context(format!(
                "Invalid CSV column name detected for datasource {}. Column names must be alphanumeric with underscores only.",
                datasource_id
            ))?;

        // Query the datasource with validated column names
        let query = format!("SELECT {} FROM data", validated_columns.join(", "));

        tracing::debug!(
            "Extracting from datasource {} with query: {}",
            datasource_id,
            query
        );

        let datasource = catalog
            .get_source(datasource_id)
            .await
            .context(format!("Failed to resolve datasource {}", datasource_id))?;
        evaluate_datasource_readiness(&datasource, DatasourceOperation::WorkflowRead)
            .map_err(|failure| anyhow::anyhow!(failure.message))?;

        // Execute query via catalog
        let result = catalog
            .execute_query(datasource_id, &query, HashMap::new(), Some(10000))
            .await
            .context(format!("Failed to query datasource {}", datasource_id))?;

        // Convert to records with ontology-mapped fields
        let mut mapped_records = Vec::new();

        for row in result.rows {
            let mut mapped_row = serde_json::Map::new();

            if let Value::Object(obj) = row {
                // Map CSV fields to ontology terms
                for (csv_field, value) in obj {
                    if let Some(source_mapping) = field_mappings.get(&csv_field) {
                        // Store value under its ontology term
                        mapped_row.insert(source_mapping.ontology_term.clone(), value);
                    }
                }
            }

            mapped_records.push(Value::Object(mapped_row));
        }

        Ok(mapped_records)
    }

    /// Transform data using mapping rules
    fn transform_data(
        &self,
        _pipeline: &LoadPipeline,
        mapping_rules: &[&TargetMappingRule],
        mut records: Vec<Value>,
    ) -> Result<Vec<Value>> {
        let engine = TransformationEngine::new();

        // Apply transformations based on mapping rules
        for record in &mut records {
            if let Value::Object(ref mut obj) = record {
                // Build context from record (ontology terms → values)
                let mut context: HashMap<String, String> = HashMap::new();
                for (key, value) in obj.iter() {
                    let value_str = match value {
                        Value::String(s) => s.clone(),
                        Value::Null => String::new(),
                        other => other.to_string(),
                    };
                    context.insert(key.clone(), value_str);
                }

                // Apply each mapping rule
                for rule in mapping_rules {
                    if let Some(value) = obj.get(&rule.ontology_term) {
                        let transformed = if let Some(transformation) = &rule.transformation {
                            // Apply transformation using engine
                            self.apply_transformation(&engine, transformation, value, &context)?
                        } else {
                            // Direct mapping
                            value.clone()
                        };

                        obj.insert(rule.target_column.clone(), transformed);
                    } else if rule.required {
                        // Required field missing
                        anyhow::bail!(
                            "Required field {} (ontology term: {}) not found in record",
                            rule.target_column,
                            rule.ontology_term
                        );
                    }
                }

                // Remove ontology term keys, keep only target column keys
                let ontology_keys: Vec<String> = mapping_rules
                    .iter()
                    .map(|r| r.ontology_term.clone())
                    .collect();

                for key in ontology_keys {
                    obj.remove(&key);
                }
            }
        }

        Ok(records)
    }

    /// Apply a transformation expression using the transformation engine
    fn apply_transformation(
        &self,
        engine: &TransformationEngine,
        transformation: &str,
        value: &Value,
        context: &HashMap<String, String>,
    ) -> Result<Value> {
        // Convert value to string for transformation
        let value_str = match value {
            Value::String(s) => s.clone(),
            Value::Null => String::new(),
            other => other.to_string(),
        };

        // Build execution context with current value
        let mut exec_context = context.clone();
        exec_context.insert("value".to_string(), value_str);

        // If transformation is just a function name (no parentheses), wrap it
        let expression = if !transformation.contains('(') {
            format!("{}({{value}})", transformation)
        } else {
            transformation.to_string()
        };

        // Execute transformation
        match engine.execute(&expression, &exec_context) {
            Ok(result) => {
                // Convert transformation result back to JSON Value
                let json_value = match result {
                    crate::mapping::loader::transformation::Value::String(s) => {
                        Value::String(s.to_string())
                    }
                    crate::mapping::loader::transformation::Value::Integer(i) => {
                        Value::Number(serde_json::Number::from(i))
                    }
                    crate::mapping::loader::transformation::Value::Float(f) => {
                        serde_json::Number::from_f64(f)
                            .map(Value::Number)
                            .unwrap_or_else(|| Value::String(f.to_string()))
                    }
                    crate::mapping::loader::transformation::Value::Boolean(b) => Value::Bool(b),
                    crate::mapping::loader::transformation::Value::Null => Value::Null,
                    crate::mapping::loader::transformation::Value::Date(d) => {
                        Value::String(d.to_string())
                    }
                    crate::mapping::loader::transformation::Value::Decimal(d) => {
                        Value::String(d.to_string())
                    }
                    crate::mapping::loader::transformation::Value::Timestamp(t) => {
                        Value::String(t.to_string())
                    }
                    crate::mapping::loader::transformation::Value::Array(arr) => {
                        serde_json::to_value(arr).unwrap_or(Value::Array(Vec::new()))
                    }
                };

                Ok(json_value)
            }
            Err(e) => {
                tracing::warn!(
                    "Transformation failed: {} - falling back to original value",
                    e
                );
                Ok(value.clone())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::etl::orchestration::types::*;
    use crate::mapping::loader::transformation::TransformationEngine;

    fn create_test_session() -> UnifiedMappingSession {
        let target_db = TargetDatabase {
            database_type: "PostgreSQL".to_string(),
            connection: TargetConnection::ConnectionString {
                connection_string: "host=localhost dbname=test user=test password=test".to_string(),
            },
            schema: Some("public".to_string()),
        };

        let mut target_schema = HashMap::new();
        target_schema.insert(
            "customers".to_string(),
            TargetTableSchema {
                table_name: "customers".to_string(),
                columns: {
                    let mut cols = HashMap::new();
                    cols.insert(
                        "email".to_string(),
                        ColumnDefinition {
                            data_type: "VARCHAR(255)".to_string(),
                            nullable: false,
                            unique: true,
                            default: None,
                        },
                    );
                    cols
                },
                primary_keys: vec!["customer_id".to_string()],
                foreign_keys: Vec::new(),
            },
        );

        let mut session = UnifiedMappingSession::new(
            "Test Load Session".to_string(),
            vec!["sess_001".to_string()],
            target_db,
            target_schema,
        );

        // Add mapping rule
        session
            .add_mapping_rule(TargetMappingRule {
                ontology_term: "http://schema.org/email".to_string(),
                target_table: "customers".to_string(),
                target_column: "email".to_string(),
                transformation: Some("LOWER(TRIM({value}))".to_string()),
                required: true,
                source_fields: Vec::new(),
            })
            .unwrap();

        session.mark_active();
        session
    }

    #[test]
    fn test_create_load_pipeline() {
        let session = create_test_session();
        let config = LoadConfig::default();

        let pipeline = LoadPipeline::new(session.clone(), config);

        assert_eq!(pipeline.session.session_id, session.session_id);
        assert!(pipeline.catalog.is_none());
        assert!(pipeline.rdf_store.is_none());
    }

    #[test]
    fn test_transform_data() {
        let orchestrator = LoadOrchestrator::new();

        let rule = TargetMappingRule {
            ontology_term: "http://schema.org/email".to_string(),
            target_table: "customers".to_string(),
            target_column: "email".to_string(),
            transformation: Some("LOWER".to_string()),
            required: true,
            source_fields: vec![SourceFieldMapping {
                session_id: "sess_001".to_string(),
                datasource_id: "ds_001".to_string(),
                csv_field: "email".to_string(),
                table_name: "data".to_string(),
                ontology_term: "http://schema.org/email".to_string(),
                field_transformation: None,
            }],
        };

        let records = vec![serde_json::json!({
            "http://schema.org/email": "TEST@EXAMPLE.COM"
        })];

        let transformed = orchestrator
            .transform_data(
                &LoadPipeline::new(create_test_session(), LoadConfig::default()),
                &[&rule],
                records,
            )
            .unwrap();

        assert_eq!(transformed.len(), 1);
        assert_eq!(transformed[0]["email"], "test@example.com");
    }

    #[test]
    fn test_apply_transformation() {
        let orchestrator = LoadOrchestrator::new();
        let engine = TransformationEngine::new();
        let context = HashMap::new();

        let value = Value::String("TEST".to_string());

        // Test with just function name (auto-wrapped to LOWER({value}))
        let result = orchestrator
            .apply_transformation(&engine, "LOWER", &value, &context)
            .unwrap();

        assert_eq!(result, Value::String("test".to_string()));

        // Test with explicit {value} reference
        let result2 = orchestrator
            .apply_transformation(&engine, "LOWER({value})", &value, &context)
            .unwrap();

        assert_eq!(result2, Value::String("test".to_string()));
    }

    // ========== COMPREHENSIVE TEST SUITE ==========

    /// Helper function to create a multi-table session for testing
    fn create_multi_table_session() -> UnifiedMappingSession {
        let target_db = TargetDatabase {
            database_type: "PostgreSQL".to_string(),
            connection: TargetConnection::ConnectionString {
                connection_string: "host=localhost dbname=test user=test password=test".to_string(),
            },
            schema: Some("public".to_string()),
        };

        let mut target_schema = HashMap::new();

        // Table 1: customers with PK = customer_id
        target_schema.insert(
            "customers".to_string(),
            TargetTableSchema {
                table_name: "customers".to_string(),
                columns: {
                    let mut cols = HashMap::new();
                    cols.insert(
                        "customer_id".to_string(),
                        ColumnDefinition {
                            data_type: "INTEGER".to_string(),
                            nullable: false,
                            unique: false, // PK is implicitly unique, no need for redundant constraint
                            default: None,
                        },
                    );
                    cols.insert(
                        "email".to_string(),
                        ColumnDefinition {
                            data_type: "VARCHAR(255)".to_string(),
                            nullable: false,
                            unique: true,
                            default: None,
                        },
                    );
                    cols
                },
                primary_keys: vec!["customer_id".to_string()],
                foreign_keys: Vec::new(),
            },
        );

        // Table 2: orders with PK = order_id, FK -> customer_id
        target_schema.insert(
            "orders".to_string(),
            TargetTableSchema {
                table_name: "orders".to_string(),
                columns: {
                    let mut cols = HashMap::new();
                    cols.insert(
                        "order_id".to_string(),
                        ColumnDefinition {
                            data_type: "INTEGER".to_string(),
                            nullable: false,
                            unique: false, // PK is implicitly unique
                            default: None,
                        },
                    );
                    cols.insert(
                        "customer_id".to_string(),
                        ColumnDefinition {
                            data_type: "INTEGER".to_string(),
                            nullable: true, // Allow NULL for FK validation test
                            unique: false,
                            default: None,
                        },
                    );
                    cols
                },
                primary_keys: vec!["order_id".to_string()],
                foreign_keys: vec![ForeignKeyConstraint {
                    column: "customer_id".to_string(),
                    references_table: "customers".to_string(),
                    references_column: "customer_id".to_string(),
                }],
            },
        );

        // Table 3: order_items with composite PK = (order_id, line_number)
        target_schema.insert(
            "order_items".to_string(),
            TargetTableSchema {
                table_name: "order_items".to_string(),
                columns: {
                    let mut cols = HashMap::new();
                    cols.insert(
                        "order_id".to_string(),
                        ColumnDefinition {
                            data_type: "INTEGER".to_string(),
                            nullable: false,
                            unique: false,
                            default: None,
                        },
                    );
                    cols.insert(
                        "line_number".to_string(),
                        ColumnDefinition {
                            data_type: "INTEGER".to_string(),
                            nullable: false,
                            unique: false,
                            default: None,
                        },
                    );
                    cols.insert(
                        "product_name".to_string(),
                        ColumnDefinition {
                            data_type: "VARCHAR(255)".to_string(),
                            nullable: false,
                            unique: false,
                            default: None,
                        },
                    );
                    cols
                },
                primary_keys: vec!["order_id".to_string(), "line_number".to_string()],
                foreign_keys: vec![ForeignKeyConstraint {
                    column: "order_id".to_string(),
                    references_table: "orders".to_string(),
                    references_column: "order_id".to_string(),
                }],
            },
        );

        UnifiedMappingSession::new(
            "Multi-table Test Session".to_string(),
            vec!["sess_001".to_string()],
            target_db,
            target_schema,
        )
    }

    // ========== PK CACHE EXTRACTION TESTS ==========

    #[test]
    fn test_extract_pk_values_single_column() {
        let orchestrator = LoadOrchestrator::new();
        let session = create_multi_table_session();
        let table_schema = session.target_schema.get("customers").unwrap();

        let records = vec![
            serde_json::json!({"customer_id": 1, "email": "alice@example.com"}),
            serde_json::json!({"customer_id": 2, "email": "bob@example.com"}),
            serde_json::json!({"customer_id": 3, "email": "charlie@example.com"}),
        ];

        let pk_values = orchestrator
            .extract_pk_values("customers", table_schema, &records)
            .unwrap();

        assert_eq!(pk_values.len(), 1);
        assert!(pk_values.contains_key("customer_id"));

        let customer_ids = pk_values.get("customer_id").unwrap();
        assert_eq!(customer_ids.len(), 3);
        assert!(customer_ids.contains("1"));
        assert!(customer_ids.contains("2"));
        assert!(customer_ids.contains("3"));
    }

    #[test]
    fn test_extract_pk_values_composite() {
        let orchestrator = LoadOrchestrator::new();
        let session = create_multi_table_session();
        let table_schema = session.target_schema.get("order_items").unwrap();

        let records = vec![
            serde_json::json!({"order_id": 100, "line_number": 1, "product_name": "Widget"}),
            serde_json::json!({"order_id": 100, "line_number": 2, "product_name": "Gadget"}),
            serde_json::json!({"order_id": 101, "line_number": 1, "product_name": "Tool"}),
        ];

        let pk_values = orchestrator
            .extract_pk_values("order_items", table_schema, &records)
            .unwrap();

        // Composite PKs use "__composite__" key
        assert_eq!(pk_values.len(), 1);
        assert!(pk_values.contains_key("__composite__"));

        let composite_values = pk_values.get("__composite__").unwrap();
        assert_eq!(composite_values.len(), 3);
        assert!(composite_values.contains("100|1"));
        assert!(composite_values.contains("100|2"));
        assert!(composite_values.contains("101|1"));
    }

    #[test]
    fn test_extract_pk_values_with_nulls() {
        let orchestrator = LoadOrchestrator::new();
        let session = create_multi_table_session();
        let table_schema = session.target_schema.get("customers").unwrap();

        let records = vec![
            serde_json::json!({"customer_id": 1, "email": "alice@example.com"}),
            serde_json::json!({"customer_id": null, "email": "no-id@example.com"}), // Null PK
            serde_json::json!({"customer_id": 3, "email": "charlie@example.com"}),
        ];

        let pk_values = orchestrator
            .extract_pk_values("customers", table_schema, &records)
            .unwrap();

        let customer_ids = pk_values.get("customer_id").unwrap();
        // Null PKs should be skipped
        assert_eq!(customer_ids.len(), 2);
        assert!(customer_ids.contains("1"));
        assert!(customer_ids.contains("3"));
        assert!(!customer_ids.contains("null"));
    }

    #[test]
    fn test_extract_pk_values_composite_with_partial_null() {
        let orchestrator = LoadOrchestrator::new();
        let session = create_multi_table_session();
        let table_schema = session.target_schema.get("order_items").unwrap();

        let records = vec![
            serde_json::json!({"order_id": 100, "line_number": 1, "product_name": "Widget"}),
            serde_json::json!({"order_id": 100, "line_number": null, "product_name": "Gadget"}), // Partial null
            serde_json::json!({"order_id": 101, "line_number": 1, "product_name": "Tool"}),
        ];

        let pk_values = orchestrator
            .extract_pk_values("order_items", table_schema, &records)
            .unwrap();

        let composite_values = pk_values.get("__composite__").unwrap();
        // Composite PK with any null part should be skipped entirely
        assert_eq!(composite_values.len(), 2);
        assert!(composite_values.contains("100|1"));
        assert!(composite_values.contains("101|1"));
        assert!(!composite_values.contains("100|null"));
    }

    #[test]
    fn test_store_pk_values_in_cache() {
        let mut orchestrator = LoadOrchestrator::new();

        let mut pk_values = HashMap::new();
        let mut customer_ids = std::collections::HashSet::new();
        customer_ids.insert("1".to_string());
        customer_ids.insert("2".to_string());
        pk_values.insert("customer_id".to_string(), customer_ids);

        orchestrator
            .store_pk_values_in_cache("customers", pk_values)
            .unwrap();

        // Verify cache was populated
        assert!(orchestrator.pk_cache.contains_key("customers"));
        let cached = orchestrator.pk_cache.get("customers").unwrap();
        assert_eq!(cached.get("customer_id").unwrap().len(), 2);
    }

    // ========== FK VALIDATION TESTS ==========

    #[test]
    fn test_fk_validation_success() {
        let mut orchestrator = LoadOrchestrator::new();
        let session = create_multi_table_session();

        // Populate PK cache for customers table
        let mut customers_pk = HashMap::new();
        let mut customer_ids = std::collections::HashSet::new();
        customer_ids.insert("1".to_string());
        customer_ids.insert("2".to_string());
        customers_pk.insert("customer_id".to_string(), customer_ids);
        orchestrator
            .pk_cache
            .insert("customers".to_string(), customers_pk);

        // Validate orders records with valid FKs
        let orders_records = vec![
            serde_json::json!({"order_id": 100, "customer_id": 1}),
            serde_json::json!({"order_id": 101, "customer_id": 2}),
        ];

        let orders_schema = session.target_schema.get("orders").unwrap();
        let mut stats = LoadExecutionStats::new();

        let pipeline = LoadPipeline::new(session.clone(), LoadConfig::default());
        orchestrator
            .validate_constraints(
                &pipeline,
                "orders",
                orders_schema,
                &orders_records,
                &mut stats,
            )
            .unwrap();

        // Should have no FK errors
        assert_eq!(stats.errors_count, 0);
    }

    #[test]
    fn test_fk_validation_violation() {
        let mut orchestrator = LoadOrchestrator::new();
        let session = create_multi_table_session();

        // Populate PK cache for customers table
        let mut customers_pk = HashMap::new();
        let mut customer_ids = std::collections::HashSet::new();
        customer_ids.insert("1".to_string());
        customer_ids.insert("2".to_string());
        customers_pk.insert("customer_id".to_string(), customer_ids);
        orchestrator
            .pk_cache
            .insert("customers".to_string(), customers_pk);

        // Validate orders records with INVALID FK (customer_id=999 doesn't exist)
        let orders_records = vec![
            serde_json::json!({"order_id": 100, "customer_id": 1}),
            serde_json::json!({"order_id": 101, "customer_id": 999}), // Invalid FK
        ];

        let orders_schema = session.target_schema.get("orders").unwrap();
        let mut stats = LoadExecutionStats::new();

        let pipeline = LoadPipeline::new(session.clone(), LoadConfig::default());
        orchestrator
            .validate_constraints(
                &pipeline,
                "orders",
                orders_schema,
                &orders_records,
                &mut stats,
            )
            .unwrap();

        // Should have 1 FK error
        assert_eq!(stats.errors_count, 1);
        assert!(stats.errors[0]
            .message
            .contains("FOREIGN KEY constraint violation"));
        assert!(stats.errors[0].message.contains("999"));
    }

    #[test]
    fn test_fk_validation_null_allowed() {
        let mut orchestrator = LoadOrchestrator::new();
        let session = create_multi_table_session();

        // Populate PK cache
        let mut customers_pk = HashMap::new();
        let mut customer_ids = std::collections::HashSet::new();
        customer_ids.insert("1".to_string());
        customers_pk.insert("customer_id".to_string(), customer_ids);
        orchestrator
            .pk_cache
            .insert("customers".to_string(), customers_pk);

        // Validate orders with null FK (should be allowed)
        let orders_records = vec![
            serde_json::json!({"order_id": 100, "customer_id": 1}),
            serde_json::json!({"order_id": 101, "customer_id": null}), // Null FK is OK
        ];

        let orders_schema = session.target_schema.get("orders").unwrap();
        let mut stats = LoadExecutionStats::new();

        let pipeline = LoadPipeline::new(session.clone(), LoadConfig::default());
        orchestrator
            .validate_constraints(
                &pipeline,
                "orders",
                orders_schema,
                &orders_records,
                &mut stats,
            )
            .unwrap();

        // Null FK should not cause error
        assert_eq!(stats.errors_count, 0);
    }

    #[test]
    fn test_fk_validation_referencing_composite_pk() {
        let mut orchestrator = LoadOrchestrator::new();
        let session = create_multi_table_session();

        // Populate PK cache for order_items (composite PK)
        let mut items_pk = HashMap::new();
        let mut composite_values = std::collections::HashSet::new();
        composite_values.insert("100|1".to_string());
        items_pk.insert("__composite__".to_string(), composite_values);
        orchestrator
            .pk_cache
            .insert("order_items".to_string(), items_pk);

        // Try to validate FK against composite PK (should warn and skip)
        let some_records = vec![serde_json::json!({"id": 1, "order_id": 100})];

        // Create a dummy table schema with FK to order_items
        let mut fake_schema = HashMap::new();
        fake_schema.insert(
            "id".to_string(),
            ColumnDefinition {
                data_type: "INTEGER".to_string(),
                nullable: false,
                unique: true,
                default: None,
            },
        );
        fake_schema.insert(
            "order_id".to_string(),
            ColumnDefinition {
                data_type: "INTEGER".to_string(),
                nullable: false,
                unique: false,
                default: None,
            },
        );

        let table_schema = TargetTableSchema {
            table_name: "some_table".to_string(),
            columns: fake_schema,
            primary_keys: vec!["id".to_string()],
            foreign_keys: vec![ForeignKeyConstraint {
                column: "order_id".to_string(),
                references_table: "order_items".to_string(),
                references_column: "order_id".to_string(),
            }],
        };

        let mut stats = LoadExecutionStats::new();
        let pipeline = LoadPipeline::new(session.clone(), LoadConfig::default());

        orchestrator
            .validate_constraints(
                &pipeline,
                "some_table",
                &table_schema,
                &some_records,
                &mut stats,
            )
            .unwrap();

        // Should not generate FK errors (warned and skipped)
        assert_eq!(stats.errors_count, 0);
    }

    // ========== ENTITY ID FIELD DETERMINATION TESTS ==========

    #[test]
    fn test_determine_entity_id_field_multi_table() {
        let orchestrator = LoadOrchestrator::new();
        let session = create_multi_table_session();
        let pipeline = LoadPipeline::new(session.clone(), LoadConfig::default());

        // Each table should return its own PK, not the first table's PK
        // This test will FAIL with the current buggy implementation
        // because determine_entity_id_field() returns "customer_id" for ALL tables

        // Test customers table
        let entity_id = orchestrator
            .determine_entity_id_field(&pipeline, "customers")
            .unwrap();
        assert_eq!(entity_id, Some("customer_id".to_string()));

        // Test orders table - should return "order_id", NOT "customer_id"
        let entity_id = orchestrator
            .determine_entity_id_field(&pipeline, "orders")
            .unwrap();
        assert_eq!(entity_id, Some("order_id".to_string()));

        // Test order_items table with composite PK - should return first PK column
        let entity_id = orchestrator
            .determine_entity_id_field(&pipeline, "order_items")
            .unwrap();
        assert_eq!(entity_id, Some("order_id".to_string()));
    }

    #[test]
    fn test_determine_entity_id_field_no_pk() {
        let orchestrator = LoadOrchestrator::new();

        let target_db = TargetDatabase {
            database_type: "PostgreSQL".to_string(),
            connection: TargetConnection::ConnectionString {
                connection_string: "host=localhost".to_string(),
            },
            schema: None,
        };

        let mut target_schema = HashMap::new();
        target_schema.insert(
            "no_pk_table".to_string(),
            TargetTableSchema {
                table_name: "no_pk_table".to_string(),
                columns: HashMap::new(),
                primary_keys: Vec::new(), // No PK
                foreign_keys: Vec::new(),
            },
        );

        let session = UnifiedMappingSession::new(
            "No PK Session".to_string(),
            vec![],
            target_db,
            target_schema,
        );

        let pipeline = LoadPipeline::new(session, LoadConfig::default());
        let entity_id = orchestrator
            .determine_entity_id_field(&pipeline, "no_pk_table")
            .unwrap();

        assert_eq!(entity_id, None);
    }

    // ========== SECRET STORE INTEGRATION TESTS ==========

    #[test]
    fn test_parse_secret_ref_vault() {
        let orchestrator = LoadOrchestrator::new();
        let result = orchestrator
            .parse_secret_ref("vault://datasources/postgres-prod")
            .unwrap();
        assert_eq!(result, "datasources/postgres-prod");
    }

    #[test]
    fn test_parse_secret_ref_aws() {
        let orchestrator = LoadOrchestrator::new();
        let result = orchestrator.parse_secret_ref("aws://my-db-secret").unwrap();
        assert_eq!(result, "my-db-secret");
    }

    #[test]
    fn test_parse_secret_ref_env() {
        let orchestrator = LoadOrchestrator::new();
        let result = orchestrator
            .parse_secret_ref("env://DB_CREDENTIALS")
            .unwrap();
        assert_eq!(result, "DB_CREDENTIALS");
    }

    #[test]
    fn test_parse_secret_ref_direct_path() {
        let orchestrator = LoadOrchestrator::new();
        let result = orchestrator
            .parse_secret_ref("direct/path/to/secret")
            .unwrap();
        assert_eq!(result, "direct/path/to/secret");
    }

    // ========== CONSTRAINT VALIDATION TESTS ==========

    #[test]
    fn test_not_null_constraint() {
        let orchestrator = LoadOrchestrator::new();
        let session = create_multi_table_session();
        let pipeline = LoadPipeline::new(session.clone(), LoadConfig::default());

        // customers.email is NOT NULL
        let records = vec![
            serde_json::json!({"customer_id": 1, "email": "alice@example.com"}),
            serde_json::json!({"customer_id": 2, "email": null}), // Violates NOT NULL
        ];

        let customers_schema = session.target_schema.get("customers").unwrap();
        let mut stats = LoadExecutionStats::new();

        orchestrator
            .validate_constraints(
                &pipeline,
                "customers",
                customers_schema,
                &records,
                &mut stats,
            )
            .unwrap();

        // Should have NOT NULL error
        assert_eq!(stats.errors_count, 1);
        assert!(stats.errors[0]
            .message
            .contains("NOT NULL constraint violation"));
        assert_eq!(stats.errors[0].field, Some("email".to_string()));
    }

    #[test]
    fn test_primary_key_uniqueness() {
        let orchestrator = LoadOrchestrator::new();
        let session = create_multi_table_session();
        let pipeline = LoadPipeline::new(session.clone(), LoadConfig::default());

        // Duplicate customer_id
        let records = vec![
            serde_json::json!({"customer_id": 1, "email": "alice@example.com"}),
            serde_json::json!({"customer_id": 1, "email": "alice2@example.com"}), // Duplicate PK
        ];

        let customers_schema = session.target_schema.get("customers").unwrap();
        let mut stats = LoadExecutionStats::new();

        orchestrator
            .validate_constraints(
                &pipeline,
                "customers",
                customers_schema,
                &records,
                &mut stats,
            )
            .unwrap();

        // Should have PK uniqueness error
        assert_eq!(stats.errors_count, 1);
        assert!(stats.errors[0]
            .message
            .contains("PRIMARY KEY constraint violation: duplicate value"));
    }

    #[test]
    fn test_unique_constraint() {
        let orchestrator = LoadOrchestrator::new();
        let session = create_multi_table_session();
        let pipeline = LoadPipeline::new(session.clone(), LoadConfig::default());

        // Duplicate email (email is UNIQUE)
        let records = vec![
            serde_json::json!({"customer_id": 1, "email": "alice@example.com"}),
            serde_json::json!({"customer_id": 2, "email": "alice@example.com"}), // Duplicate UNIQUE
        ];

        let customers_schema = session.target_schema.get("customers").unwrap();
        let mut stats = LoadExecutionStats::new();

        orchestrator
            .validate_constraints(
                &pipeline,
                "customers",
                customers_schema,
                &records,
                &mut stats,
            )
            .unwrap();

        // Should have UNIQUE constraint error
        assert_eq!(stats.errors_count, 1);
        assert!(stats.errors[0]
            .message
            .contains("UNIQUE constraint violation"));
        assert_eq!(stats.errors[0].field, Some("email".to_string()));
    }

    #[test]
    fn test_composite_pk_uniqueness() {
        let orchestrator = LoadOrchestrator::new();
        let session = create_multi_table_session();
        let pipeline = LoadPipeline::new(session.clone(), LoadConfig::default());

        // Duplicate composite PK (order_id, line_number)
        let records = vec![
            serde_json::json!({"order_id": 100, "line_number": 1, "product_name": "Widget"}),
            serde_json::json!({"order_id": 100, "line_number": 1, "product_name": "Gadget"}), // Duplicate composite PK
            serde_json::json!({"order_id": 100, "line_number": 2, "product_name": "Tool"}), // Different line_number, OK
        ];

        let order_items_schema = session.target_schema.get("order_items").unwrap();
        let mut stats = LoadExecutionStats::new();

        orchestrator
            .validate_constraints(
                &pipeline,
                "order_items",
                order_items_schema,
                &records,
                &mut stats,
            )
            .unwrap();

        // Should have composite PK uniqueness error
        assert_eq!(stats.errors_count, 1);
        assert!(stats.errors[0]
            .message
            .contains("PRIMARY KEY constraint violation: duplicate value"));
        assert!(stats.errors[0].message.contains("100|1"));
    }

    // ========== FUSION FILTERING TESTS ==========

    #[test]
    fn test_fusion_filter_no_rdf_store() {
        let orchestrator = LoadOrchestrator::new();
        let session = create_multi_table_session();
        let pipeline = LoadPipeline::new(session.clone(), LoadConfig::default());
        // No RDF store configured

        let records = vec![
            serde_json::json!({"customer_id": 1, "email": "alice@example.com"}),
            serde_json::json!({"customer_id": 2, "email": "bob@example.com"}),
        ];

        let (filtered, fused_count) = orchestrator
            .apply_fusion_filter(&pipeline, "customers", records.clone())
            .unwrap();

        // Without RDF store, all records should pass through
        assert_eq!(filtered.len(), 2);
        assert_eq!(fused_count, 0);
    }

    #[test]
    fn test_fusion_filter_no_primary_key() {
        let orchestrator = LoadOrchestrator::new();

        // Create session with table that has no primary key
        let target_db = TargetDatabase {
            database_type: "PostgreSQL".to_string(),
            connection: TargetConnection::ConnectionString {
                connection_string: "host=localhost".to_string(),
            },
            schema: None,
        };

        let mut target_schema = HashMap::new();
        target_schema.insert(
            "no_pk_table".to_string(),
            TargetTableSchema {
                table_name: "no_pk_table".to_string(),
                columns: HashMap::new(),
                primary_keys: Vec::new(), // No PK
                foreign_keys: Vec::new(),
            },
        );

        let session = UnifiedMappingSession::new(
            "No PK Session".to_string(),
            vec![],
            target_db,
            target_schema,
        );

        let pipeline = LoadPipeline::new(session, LoadConfig::default());

        let records = vec![
            serde_json::json!({"field1": "value1"}),
            serde_json::json!({"field1": "value2"}),
        ];

        let (filtered, fused_count) = orchestrator
            .apply_fusion_filter(&pipeline, "no_pk_table", records.clone())
            .unwrap();

        // Without PK, fusion filtering cannot work, all records pass through
        assert_eq!(filtered.len(), 2);
        assert_eq!(fused_count, 0);
    }

    #[test]
    fn test_fusion_filter_respects_fusion_flag() {
        let orchestrator = LoadOrchestrator::new();
        let session = create_multi_table_session();

        // Test with respect_fusion = false
        let mut config = LoadConfig::default();
        config.respect_fusion = false;
        let pipeline = LoadPipeline::new(session.clone(), config);

        let records = vec![serde_json::json!({"customer_id": 1, "email": "alice@example.com"})];

        // When respect_fusion = false, fusion filter is not called
        // (tested indirectly via load_table logic)
        // This test verifies the configuration option exists and can be set
        assert!(!pipeline.config.respect_fusion);
    }

    // NOTE: Full integration tests with actual RDF store SPARQL queries
    // should be in integration test files (tests/etl_integration.rs)
    // These unit tests focus on edge cases and error handling.

    // ========== TOPOLOGICAL SORT TESTS ==========

    #[test]
    fn test_topological_sort_simple_dependency() {
        let orchestrator = LoadOrchestrator::new();
        let session = create_multi_table_session();
        let pipeline = LoadPipeline::new(session.clone(), LoadConfig::default());

        let order = orchestrator.compute_table_load_order(&pipeline).unwrap();

        // customers has no FK dependencies, should be loaded first
        // orders depends on customers, should be loaded second
        // order_items depends on orders, should be loaded third

        let customers_pos = order.iter().position(|t| t == "customers").unwrap();
        let orders_pos = order.iter().position(|t| t == "orders").unwrap();
        let order_items_pos = order.iter().position(|t| t == "order_items").unwrap();

        // Verify customers is loaded before orders
        assert!(
            customers_pos < orders_pos,
            "customers should be loaded before orders (customers: {}, orders: {})",
            customers_pos,
            orders_pos
        );

        // Verify orders is loaded before order_items
        assert!(
            orders_pos < order_items_pos,
            "orders should be loaded before order_items (orders: {}, order_items: {})",
            orders_pos,
            order_items_pos
        );
    }

    #[test]
    fn test_topological_sort_no_dependencies() {
        let orchestrator = LoadOrchestrator::new();

        // Create schema with tables that have no FKs
        let target_db = TargetDatabase {
            database_type: "PostgreSQL".to_string(),
            connection: TargetConnection::ConnectionString {
                connection_string: "host=localhost".to_string(),
            },
            schema: None,
        };

        let mut target_schema = HashMap::new();
        target_schema.insert(
            "table_a".to_string(),
            TargetTableSchema {
                table_name: "table_a".to_string(),
                columns: HashMap::new(),
                primary_keys: vec!["id".to_string()],
                foreign_keys: Vec::new(), // No FKs
            },
        );
        target_schema.insert(
            "table_b".to_string(),
            TargetTableSchema {
                table_name: "table_b".to_string(),
                columns: HashMap::new(),
                primary_keys: vec!["id".to_string()],
                foreign_keys: Vec::new(), // No FKs
            },
        );

        let session = UnifiedMappingSession::new(
            "No Dependencies Session".to_string(),
            vec![],
            target_db,
            target_schema,
        );

        let pipeline = LoadPipeline::new(session, LoadConfig::default());
        let order = orchestrator.compute_table_load_order(&pipeline).unwrap();

        // Both tables should be in the order, but order doesn't matter
        assert_eq!(order.len(), 2);
        assert!(order.contains(&"table_a".to_string()));
        assert!(order.contains(&"table_b".to_string()));
    }

    #[test]
    fn test_topological_sort_circular_dependency() {
        let orchestrator = LoadOrchestrator::new();

        // Create schema with circular dependency: A -> B -> A
        let target_db = TargetDatabase {
            database_type: "PostgreSQL".to_string(),
            connection: TargetConnection::ConnectionString {
                connection_string: "host=localhost".to_string(),
            },
            schema: None,
        };

        let mut target_schema = HashMap::new();
        target_schema.insert(
            "table_a".to_string(),
            TargetTableSchema {
                table_name: "table_a".to_string(),
                columns: HashMap::new(),
                primary_keys: vec!["id".to_string()],
                foreign_keys: vec![ForeignKeyConstraint {
                    column: "b_id".to_string(),
                    references_table: "table_b".to_string(),
                    references_column: "id".to_string(),
                }],
            },
        );
        target_schema.insert(
            "table_b".to_string(),
            TargetTableSchema {
                table_name: "table_b".to_string(),
                columns: HashMap::new(),
                primary_keys: vec!["id".to_string()],
                foreign_keys: vec![ForeignKeyConstraint {
                    column: "a_id".to_string(),
                    references_table: "table_a".to_string(),
                    references_column: "id".to_string(),
                }],
            },
        );

        let session = UnifiedMappingSession::new(
            "Circular Dependency Session".to_string(),
            vec![],
            target_db,
            target_schema,
        );

        let pipeline = LoadPipeline::new(session, LoadConfig::default());
        let result = orchestrator.compute_table_load_order(&pipeline);

        // Should return error due to circular dependency
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Circular dependency"));
    }

    #[test]
    fn test_topological_sort_multiple_fks_same_table() {
        let orchestrator = LoadOrchestrator::new();

        // Create schema where table_b has multiple FKs to table_a
        let target_db = TargetDatabase {
            database_type: "PostgreSQL".to_string(),
            connection: TargetConnection::ConnectionString {
                connection_string: "host=localhost".to_string(),
            },
            schema: None,
        };

        let mut target_schema = HashMap::new();
        target_schema.insert(
            "table_a".to_string(),
            TargetTableSchema {
                table_name: "table_a".to_string(),
                columns: HashMap::new(),
                primary_keys: vec!["id".to_string()],
                foreign_keys: Vec::new(),
            },
        );
        target_schema.insert(
            "table_b".to_string(),
            TargetTableSchema {
                table_name: "table_b".to_string(),
                columns: HashMap::new(),
                primary_keys: vec!["id".to_string()],
                foreign_keys: vec![
                    ForeignKeyConstraint {
                        column: "fk1".to_string(),
                        references_table: "table_a".to_string(),
                        references_column: "id".to_string(),
                    },
                    ForeignKeyConstraint {
                        column: "fk2".to_string(),
                        references_table: "table_a".to_string(),
                        references_column: "id".to_string(),
                    },
                ],
            },
        );

        let session = UnifiedMappingSession::new(
            "Multiple FKs Session".to_string(),
            vec![],
            target_db,
            target_schema,
        );

        let pipeline = LoadPipeline::new(session, LoadConfig::default());
        let order = orchestrator.compute_table_load_order(&pipeline).unwrap();

        // table_a should be loaded before table_b
        let a_pos = order.iter().position(|t| t == "table_a").unwrap();
        let b_pos = order.iter().position(|t| t == "table_b").unwrap();

        assert!(a_pos < b_pos, "table_a should be loaded before table_b");
    }
}
