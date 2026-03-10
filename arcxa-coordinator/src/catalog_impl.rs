//! Persistent Data Source Catalog Implementation
//!
//! Catalog implementation that stores datasources in RocksDB for persistence
//! and uses the ConnectorRegistry to perform real operations.
//! Also syncs datasources to RDF as gph:Dataset triples.

use async_trait::async_trait;
use chrono::Utc;
use graphica_core::catalog::{
    api_types::*,
    client::{CatalogResult, DataSourceCatalog, UsageStatistics},
    connector::{Credentials, DataSourceConnector},
    connectors::ConnectorRegistry,
    types::{normalize_source_type_name, ConnectionDetails, DataSource, SourceConfig},
};
use graphica_core::errors::GraphicaError;
use graphica_core::inference::types::{
    ColumnStatistics as CoreColumnStatistics, SemanticType, ValueFrequency,
};
use graphica_core::secrets::providers::SecretStoreRegistry;
use graphica_core::secrets::SecretValue;
use parking_lot::RwLock;
use rocksdb::{Options, DB};
use std::collections::HashMap;
use std::sync::Arc;

use crate::catalog_to_dataset;
use crate::governance::rdf_store::{GraphicaRdfStore, RdfStore};
use crate::mapping::discovery::service::DiscoveryService;
use crate::mapping::discovery::types::DiscoveredSchema;

const CREDENTIAL_KEYS: &[&str] = &[
    "username",
    "password",
    "user",
    "pass",
    "access_key_id",
    "secret_access_key",
    "token",
    "access_token",
    "session_token",
    "api_key",
    "apikey",
    "client_id",
    "client_secret",
];

/// Persistent data source catalog
///
/// Stores datasource configurations in RocksDB for persistence across restarts.
/// Uses the ConnectorRegistry to perform operations like connection testing, schema inference, etc.
/// Also syncs datasources to RDF as gph:Dataset triples.
pub struct InMemoryDataSourceCatalog {
    /// Map of datasource ID to response (cached in memory)
    sources: RwLock<HashMap<String, DataSourceResponse>>,

    /// Connector registry for accessing connector implementations
    connector_registry: Arc<RwLock<ConnectorRegistry>>,

    /// Optional discovery service for intelligent schema inference
    discovery_service: RwLock<Option<Arc<dyn DiscoveryService>>>,

    /// Optional secret store registry for resolving datasource secretRef credentials.
    secret_store_registry: RwLock<Option<Arc<SecretStoreRegistry>>>,

    /// Optional RDF store for syncing datasources as datasets
    rdf_store: Option<Arc<GraphicaRdfStore>>,

    /// RocksDB instance for persistent storage
    db: Arc<DB>,

    /// ODBC connection pools for Oracle data sources (keyed by source_id)
    #[cfg(feature = "odbc")]
    oracle_pools: tokio::sync::RwLock<
        HashMap<
            String,
            deadpool::managed::Pool<
                crate::mapping::discovery::extractors::odbc::GenericOdbcConnectionManager<
                    crate::mapping::discovery::extractors::OdbcOracleConnection,
                >,
            >,
        >,
    >,

    /// ODBC connection pools for SAP HANA data sources (keyed by source_id)
    #[cfg(feature = "odbc")]
    saphana_pools: tokio::sync::RwLock<
        HashMap<
            String,
            deadpool::managed::Pool<
                crate::mapping::discovery::extractors::odbc::GenericOdbcConnectionManager<
                    crate::mapping::discovery::extractors::OdbcSAPHANAConnection,
                >,
            >,
        >,
    >,

    /// DB2 connection pools for DB2 data sources (keyed by source_id)
    /// This enables connection reuse across workflow executions
    #[cfg(feature = "odbc")]
    db2_pools: tokio::sync::RwLock<HashMap<String, Arc<crate::mapping::loader::DB2Pool>>>,
}

impl InMemoryDataSourceCatalog {
    /// Create a new persistent catalog with connector registry
    pub fn new(connector_registry: Arc<RwLock<ConnectorRegistry>>) -> Self {
        // Use default path if not specified
        let db_path = std::env::var("DATASOURCE_DB_PATH")
            .unwrap_or_else(|_| "./data/datasources".to_string());

        Self::new_with_path(connector_registry, &db_path).expect("Failed to create catalog")
    }

    /// Create a new persistent catalog with custom path
    pub fn new_with_path(
        connector_registry: Arc<RwLock<ConnectorRegistry>>,
        db_path: &str,
    ) -> anyhow::Result<Self> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);

        let db = DB::open(&opts, db_path)?;
        let db = Arc::new(db);

        // Load existing datasources from RocksDB
        let sources = Self::load_all_from_db(&db)?;

        tracing::info!(
            "Loaded {} datasources from persistent storage at {}",
            sources.len(),
            db_path
        );

        Ok(Self {
            sources: RwLock::new(sources),
            connector_registry,
            discovery_service: RwLock::new(None),
            secret_store_registry: RwLock::new(None),
            rdf_store: None,
            db,
            #[cfg(feature = "odbc")]
            oracle_pools: tokio::sync::RwLock::new(HashMap::new()),
            #[cfg(feature = "odbc")]
            saphana_pools: tokio::sync::RwLock::new(HashMap::new()),
            #[cfg(feature = "odbc")]
            db2_pools: tokio::sync::RwLock::new(HashMap::new()),
        })
    }

    /// Create a new persistent catalog with connector registry and RDF store
    pub fn new_with_rdf(
        connector_registry: Arc<RwLock<ConnectorRegistry>>,
        rdf_store: Arc<GraphicaRdfStore>,
    ) -> Self {
        let db_path = std::env::var("DATASOURCE_DB_PATH")
            .unwrap_or_else(|_| "./data/datasources".to_string());

        Self::new_with_rdf_and_path(connector_registry, rdf_store, &db_path)
            .expect("Failed to create catalog with RDF")
    }

    /// Create a new persistent catalog with RDF store and custom path
    pub fn new_with_rdf_and_path(
        connector_registry: Arc<RwLock<ConnectorRegistry>>,
        rdf_store: Arc<GraphicaRdfStore>,
        db_path: &str,
    ) -> anyhow::Result<Self> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);

        let db = DB::open(&opts, db_path)?;
        let db = Arc::new(db);

        // Load existing datasources from RocksDB
        let sources = Self::load_all_from_db(&db)?;

        tracing::info!(
            "Loaded {} datasources from persistent storage at {}",
            sources.len(),
            db_path
        );

        Ok(Self {
            sources: RwLock::new(sources),
            connector_registry,
            discovery_service: RwLock::new(None),
            secret_store_registry: RwLock::new(None),
            rdf_store: Some(rdf_store),
            db,
            #[cfg(feature = "odbc")]
            oracle_pools: tokio::sync::RwLock::new(HashMap::new()),
            #[cfg(feature = "odbc")]
            saphana_pools: tokio::sync::RwLock::new(HashMap::new()),
            #[cfg(feature = "odbc")]
            db2_pools: tokio::sync::RwLock::new(HashMap::new()),
        })
    }

    /// Load all datasources from RocksDB
    fn load_all_from_db(db: &Arc<DB>) -> anyhow::Result<HashMap<String, DataSourceResponse>> {
        let mut sources = HashMap::new();
        let iter = db.iterator(rocksdb::IteratorMode::Start);

        for item in iter {
            let (key, value) = item?;
            let id = String::from_utf8(key.to_vec())?;
            let response: DataSourceResponse = serde_json::from_slice(&value)?;
            sources.insert(id, response);
        }

        Ok(sources)
    }

    /// Save a datasource to RocksDB
    fn save_to_db(&self, id: &str, response: &DataSourceResponse) -> anyhow::Result<()> {
        let value = serde_json::to_vec(response)?;
        self.db.put(id.as_bytes(), &value)?;
        Ok(())
    }

    /// Delete a datasource from RocksDB
    fn delete_from_db(&self, id: &str) -> anyhow::Result<()> {
        self.db.delete(id.as_bytes())?;
        Ok(())
    }

    fn is_credential_key(key: &str) -> bool {
        let lower = key.to_lowercase();
        CREDENTIAL_KEYS.contains(&lower.as_str())
    }

    fn filter_credential_keys(map: &HashMap<String, String>) -> HashMap<String, String> {
        map.iter()
            .filter(|(k, _)| Self::is_credential_key(k))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    fn strip_credential_keys(map: &mut HashMap<String, String>) -> bool {
        let keys: Vec<String> = map
            .keys()
            .filter(|k| Self::is_credential_key(k))
            .cloned()
            .collect();

        if keys.is_empty() {
            return false;
        }

        for key in keys {
            map.remove(&key);
        }

        true
    }

    fn extract_inline_credentials(source: &DataSource) -> Option<HashMap<String, String>> {
        if !source.connection.credentials.is_empty() {
            return Some(source.connection.credentials.clone());
        }

        let filtered = Self::filter_credential_keys(&source.metadata);
        if filtered.is_empty() {
            None
        } else {
            Some(filtered)
        }
    }

    fn default_secret_ref(source: &DataSource) -> String {
        let id_suffix = source.id.rsplit(':').next().unwrap_or(&source.id);
        format!("vault://datasources/{}/credentials", id_suffix)
    }

    async fn promote_inline_credentials(
        &self,
        source: &mut DataSource,
        store: &graphica_core::secrets::SecretStoreRef,
    ) -> CatalogResult<bool> {
        let inline_credentials = Self::extract_inline_credentials(source);
        if inline_credentials.is_none() {
            return Ok(false);
        }

        let mut secret_ref = source.connection.secret_ref.trim().to_string();
        if secret_ref.is_empty() {
            secret_ref = Self::default_secret_ref(source);
        }

        let mut secret_ready = false;
        if !secret_ref.is_empty() {
            match store.exists(&secret_ref).await {
                Ok(exists) => secret_ready = exists,
                Err(e) => {
                    tracing::warn!(
                        "Failed to check secret existence for '{}': {}",
                        secret_ref,
                        e
                    );
                }
            }
        }

        if !secret_ready {
            if let Some(creds) = inline_credentials.clone() {
                match store
                    .put_secret(&secret_ref, SecretValue::KeyValue(creds), None)
                    .await
                {
                    Ok(_) => {
                        secret_ready = true;
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to store credentials for datasource {} at '{}': {}",
                            source.id,
                            secret_ref,
                            e
                        );
                        return Ok(false);
                    }
                }
            }
        }

        if !secret_ready {
            return Ok(false);
        }

        source.connection.secret_ref = secret_ref;
        if !source.connection.credentials.is_empty() {
            source.connection.credentials.clear();
        }
        Self::strip_credential_keys(&mut source.metadata);

        Ok(true)
    }

    async fn migrate_inline_credentials(&self, registry: Arc<SecretStoreRegistry>) {
        let store = match registry.default().or_else(|| registry.get("default")) {
            Some(store) => store,
            None => {
                tracing::warn!("No default secret store configured; skipping credential migration");
                return;
            }
        };

        let snapshot = self.sources.read().clone();
        let mut migrated = 0usize;
        let mut updated = 0usize;

        for (id, response) in snapshot {
            let mut source = response.source.clone();
            let was_inline = Self::extract_inline_credentials(&source).is_some();
            if !was_inline {
                continue;
            }

            match self.promote_inline_credentials(&mut source, &store).await {
                Ok(true) => {
                    migrated += 1;
                    source.updated_at = Some(Utc::now());

                    let mut updated_response = response.clone();
                    updated_response.source = source;

                    {
                        let mut sources = self.sources.write();
                        sources.insert(id.clone(), updated_response.clone());
                    }

                    if let Err(e) = self.save_to_db(&id, &updated_response) {
                        tracing::warn!(
                            "Failed to persist migrated datasource {} to RocksDB: {}",
                            id,
                            e
                        );
                    } else {
                        updated += 1;
                    }
                }
                Ok(false) => {
                    tracing::warn!(
                        "Inline credentials for datasource {} could not be migrated (secret store unavailable or write failed)",
                        id
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        "Inline credential migration failed for datasource {}: {}",
                        id,
                        e
                    );
                }
            }
        }

        if migrated > 0 {
            tracing::info!(
                "Credential migration complete: {} datasources migrated ({} persisted)",
                migrated,
                updated
            );
        }
    }

    /// Resolve credentials from secretRef or metadata fallback.
    async fn extract_credentials(&self, source: &DataSource) -> CatalogResult<Credentials> {
        if !source.connection.credentials.is_empty() {
            return Self::credentials_from_map(
                &source.connection.credentials,
                "connection.credentials",
            );
        }

        let secret_store_registry = { self.secret_store_registry.read().clone() };
        if let Some(registry) = secret_store_registry {
            if !source.connection.secret_ref.trim().is_empty() {
                let store = registry
                    .default()
                    .or_else(|| registry.get("default"))
                    .ok_or_else(|| {
                        GraphicaError::Internal(
                            "No default secret store configured in registry".to_string(),
                        )
                    })?;

                match store.get_secret(&source.connection.secret_ref, None).await {
                    Ok(secret) => {
                        return Self::credentials_from_secret_value(&secret.value);
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to resolve secretRef '{}' for datasource '{}': {}. Falling back to metadata credentials.",
                            source.connection.secret_ref,
                            source.id,
                            e
                        );
                    }
                }
            }
        }

        let metadata_result = Self::credentials_from_metadata(source);
        if metadata_result.is_ok() {
            return metadata_result;
        }

        let allow_empty = {
            let registry = self.connector_registry.read();
            registry
                .get_metadata(&source.source_type)
                .map(|meta| meta.required_credentials.is_empty())
                .unwrap_or(false)
        };

        if allow_empty {
            return Ok(Credentials::new(String::new(), String::new()));
        }

        metadata_result
    }

    fn credentials_from_metadata(source: &DataSource) -> CatalogResult<Credentials> {
        let context = format!(
            "metadata for datasource {} (no secretRef credentials available)",
            source.id
        );
        Self::credentials_from_map(&source.metadata, &context)
    }

    fn credentials_from_secret_value(value: &SecretValue) -> CatalogResult<Credentials> {
        match value {
            SecretValue::KeyValue(map) => Self::credentials_from_map(map, "secret value"),
            SecretValue::String(raw) => Self::credentials_from_json_str(raw),
            SecretValue::Json(json) => Self::credentials_from_json_value(json),
            SecretValue::Binary(_) => Err(GraphicaError::Configuration(
                "Binary secret values are not supported for datasource credentials".to_string(),
            )),
        }
    }

    fn credentials_from_json_str(raw: &str) -> CatalogResult<Credentials> {
        let value: serde_json::Value = serde_json::from_str(raw).map_err(|e| {
            GraphicaError::Configuration(format!("Failed to parse secret JSON credentials: {}", e))
        })?;
        Self::credentials_from_json_value(&value)
    }

    fn credentials_from_json_value(value: &serde_json::Value) -> CatalogResult<Credentials> {
        let obj = value.as_object().ok_or_else(|| {
            GraphicaError::Configuration(
                "Credentials JSON must be an object with credential fields".to_string(),
            )
        })?;

        let mut additional = HashMap::new();
        for (k, v) in obj {
            if let Some(s) = v.as_str() {
                additional.insert(k.clone(), s.to_string());
            } else {
                additional.insert(k.clone(), v.to_string());
            }
        }

        Self::credentials_from_map(&additional, "credentials JSON")
    }

    fn credentials_from_map(
        map: &HashMap<String, String>,
        context: &str,
    ) -> CatalogResult<Credentials> {
        let (username, password) = if let (Some(user), Some(pass)) =
            (map.get("username"), map.get("password"))
        {
            (user.to_string(), pass.to_string())
        } else if let (Some(user), Some(pass)) = (map.get("user"), map.get("pass")) {
            (user.to_string(), pass.to_string())
        } else if let (Some(key), Some(secret)) =
            (map.get("access_key_id"), map.get("secret_access_key"))
        {
            (key.to_string(), secret.to_string())
        } else if let Some(token) = map
            .get("token")
            .or_else(|| map.get("access_token"))
            .cloned()
        {
            ("token".to_string(), token)
        } else {
            return Err(GraphicaError::Configuration(format!(
                "Missing credentials in {} (expected username/password, access_key_id/secret_access_key, or token)",
                context
            )));
        };

        let mut additional = HashMap::new();
        for (k, v) in map {
            if k != "username" && k != "password" {
                additional.insert(k.clone(), v.clone());
            }
        }

        Ok(Credentials {
            username,
            password,
            additional,
        })
    }

    /// Get connector for a datasource
    fn get_connector(&self, source: &DataSource) -> CatalogResult<Arc<dyn DataSourceConnector>> {
        let registry = self.connector_registry.read();
        registry
            .get_connector(&source.connection.config)
            .ok_or_else(|| {
                GraphicaError::NotFound(format!(
                    "No connector found for datasource type: {}",
                    source.source_type
                ))
            })
    }

    fn catalog_capabilities_for_source(&self, source: &DataSource) -> DataSourceCapabilities {
        let connector_capabilities = self
            .get_connector(source)
            .map(|connector| connector.capabilities())
            .unwrap_or_default();

        let source_type = source.connection.config.source_type();
        let discovery = self.discovery_service.read().clone();
        let discovery_supports_source = discovery
            .as_ref()
            .map(|service| service.supports_source_type(source_type))
            .unwrap_or(false);
        let odbc_enabled = cfg!(feature = "odbc");
        let uses_odbc_routing = odbc_enabled && Self::should_use_odbc(source_type);

        let (can_query, base_can_infer_schema, can_read_workflow, can_write_workflow) =
            match source_type {
                "PostgreSQL" => (true, true, true, true),
                "Oracle" => (uses_odbc_routing, true, uses_odbc_routing, false),
                "DB2" => (uses_odbc_routing, true, uses_odbc_routing, true),
                "SAPHANA" => (
                    uses_odbc_routing,
                    uses_odbc_routing && discovery_supports_source,
                    uses_odbc_routing,
                    false,
                ),
                "Snowflake" => (true, true, true, false),
                "CsvFile" => (true, true, true, false),
                _ => (false, false, false, false),
            };

        let supports_parameters = connector_capabilities.parameterized_queries;
        let can_infer_schema = if source_type == "SAPHANA" {
            base_can_infer_schema
        } else {
            base_can_infer_schema && connector_capabilities.schema_inference
        };

        DataSourceCapabilities {
            can_test: true,
            can_infer_schema,
            can_query,
            can_read_workflow,
            can_write_workflow,
            supports_parameters,
            supports_tls: matches!(
                source.connection.config,
                SourceConfig::PostgreSQL(_)
                    | SourceConfig::MySQL(_)
                    | SourceConfig::Snowflake(_)
                    | SourceConfig::Databricks(_)
            ),
            supports_incremental: matches!(
                source.connection.config,
                SourceConfig::PostgreSQL(_)
                    | SourceConfig::Oracle(_)
                    | SourceConfig::DB2(_)
                    | SourceConfig::SAPHANA(_)
                    | SourceConfig::Snowflake(_)
            ),
            supports_cancellation: !uses_odbc_routing && connector_capabilities.query_timeout,
        }
    }

    fn enrich_response(&self, mut response: DataSourceResponse) -> DataSourceResponse {
        response.source.source_type = response.source.connection.config.source_type().to_string();
        response.capabilities = Some(self.catalog_capabilities_for_source(&response.source));
        response
    }

    /// Sync all existing datasources to RDF store (backfill utility)
    ///
    /// This method is useful when:
    /// - RDF store is added to an existing catalog
    /// - Need to re-sync all datasources after schema changes
    /// - Recovering from RDF store data loss
    ///
    /// Returns the number of datasources successfully synced.
    pub async fn sync_all_to_rdf(&self) -> CatalogResult<usize> {
        if self.rdf_store.is_none() {
            return Err(GraphicaError::NotFound(
                "No RDF store configured".to_string(),
            ));
        }

        let rdf_store = self.rdf_store.as_ref().unwrap();
        let sources = self.sources.read();
        let mut synced_count = 0;
        let mut error_count = 0;

        tracing::info!("Starting RDF sync for {} datasources", sources.len());

        for (id, response) in sources.iter() {
            let source = &response.source;

            // Generate and write dataset triples
            let turtle = catalog_to_dataset::datasource_to_dataset_triples(source);
            match rdf_store.load_turtle(&turtle, None) {
                Ok(_) => {
                    synced_count += 1;
                    tracing::debug!("Synced datasource {} to RDF", id);
                }
                Err(e) => {
                    error_count += 1;
                    tracing::warn!("Failed to sync datasource {} to RDF: {}", id, e);
                }
            }
        }

        tracing::info!(
            "RDF sync complete: {} synced, {} errors",
            synced_count,
            error_count
        );

        Ok(synced_count)
    }

    /// Set discovery service for schema inference.
    pub fn set_discovery_service(&self, discovery_service: Arc<dyn DiscoveryService>) {
        let mut guard = self.discovery_service.write();
        *guard = Some(discovery_service);
    }

    /// Set secret store registry for resolving datasource secretRef credentials.
    pub async fn set_secret_store_registry(&self, registry: Arc<SecretStoreRegistry>) {
        {
            let mut guard = self.secret_store_registry.write();
            *guard = Some(registry.clone());
        }

        self.migrate_inline_credentials(registry).await;
    }

    /// Convert a semantic type string to the SemanticType enum
    fn parse_semantic_type(semantic_str: &str) -> SemanticType {
        match semantic_str.to_lowercase().as_str() {
            // Identity & Contact
            "email" => SemanticType::Email,
            "phone" | "phonenumber" | "phone_number" => SemanticType::PhoneNumber,
            "personname" | "person_name" | "name" => SemanticType::PersonName,
            "organizationname" | "organization_name" | "organization" => {
                SemanticType::OrganizationName
            }
            "username" | "user_name" => SemanticType::Username,
            "userid" | "user_id" => SemanticType::UserId,

            // Geographic
            "address" => SemanticType::Address,
            "city" => SemanticType::City,
            "state" => SemanticType::State,
            "postalcode" | "postal_code" | "zipcode" | "zip_code" => SemanticType::PostalCode,
            "country" => SemanticType::Country,
            "countrycode" | "country_code" => SemanticType::CountryCode,
            "coordinates" | "latlong" | "lat_long" => SemanticType::Coordinates,
            "ipaddress" | "ip_address" | "ip" => SemanticType::IPAddress,

            // Financial
            "creditcardnumber" | "credit_card_number" | "creditcard" | "credit_card" => {
                SemanticType::CreditCardNumber
            }
            "bankaccountnumber" | "bank_account_number" | "account_number" => {
                SemanticType::BankAccountNumber
            }
            "iban" | "ibannumber" => SemanticType::IBANNumber,
            "currencyamount" | "currency_amount" | "currency" | "money" => {
                SemanticType::CurrencyAmount
            }
            "currencycode" | "currency_code" => SemanticType::CurrencyCode,
            "taxidentifier" | "tax_identifier" | "tax_id" => SemanticType::TaxIdentifier,

            // Healthcare
            "ssn" | "social_security_number" => SemanticType::SSN,
            "medicalrecordnumber" | "medical_record_number" | "mrn" => {
                SemanticType::MedicalRecordNumber
            }
            "healthinsurancenumber" | "health_insurance_number" => {
                SemanticType::HealthInsuranceNumber
            }
            "drugcode" | "drug_code" => SemanticType::DrugCode,
            "diagnosiscode" | "diagnosis_code" => SemanticType::DiagnosisCode,

            // Temporal
            "timestamp" | "datetime" => SemanticType::Timestamp,
            "date" => SemanticType::Date,
            "time" => SemanticType::Time,
            "duration" => SemanticType::Duration,
            "dateofbirth" | "date_of_birth" | "dob" => SemanticType::DateOfBirth,

            // Technical
            "url" => SemanticType::URL,
            "uri" => SemanticType::URI,
            "uuid" => SemanticType::UUID,
            "hostname" | "host_name" => SemanticType::Hostname,
            "macaddress" | "mac_address" | "mac" => SemanticType::MACAddress,
            "filepath" | "file_path" | "path" => SemanticType::FilePath,
            "mimetype" | "mime_type" => SemanticType::MimeType,

            // Business
            "productcode" | "product_code" => SemanticType::ProductCode,
            "sku" => SemanticType::SKU,
            "ordernumber" | "order_number" => SemanticType::OrderNumber,
            "invoicenumber" | "invoice_number" => SemanticType::InvoiceNumber,
            "accountnumber" | "account_number" => SemanticType::AccountNumber,
            "vin" => SemanticType::VIN,

            // Categorical
            "enum" => SemanticType::Enum,
            "boolean" | "bool" => SemanticType::Boolean,
            "flag" => SemanticType::Flag,
            "status" => SemanticType::Status,
            "category" => SemanticType::Category,

            // Textual
            "freetext" | "free_text" | "text" => SemanticType::FreeText,
            "description" => SemanticType::Description,
            "comment" => SemanticType::Comment,
            "json" | "jsonblob" | "json_blob" => SemanticType::JsonBlob,
            "xml" | "xmlblob" | "xml_blob" => SemanticType::XMLBlob,

            // Measurement
            "quantity" => SemanticType::Quantity,
            "percentage" => SemanticType::Percentage,
            "score" => SemanticType::Score,
            "rating" => SemanticType::Rating,

            // Unknown/Custom
            _ => SemanticType::Unknown,
        }
    }

    /// Check if a datasource should use ODBC for query execution
    fn should_use_odbc(source_type: &str) -> bool {
        let lower = source_type.to_lowercase();
        lower.contains("db2") || lower.contains("oracle") || lower.contains("hana")
    }

    /// Apply LIMIT or FETCH FIRST clause to a query based on source type
    fn apply_limit_to_query(query: &str, limit: usize, source_type: &str) -> String {
        let trimmed = query.trim_end_matches(';').trim();
        let upper = trimmed.to_uppercase();

        // Don't add limit if query already has one
        if upper.contains("LIMIT ") || upper.contains("FETCH FIRST") {
            return query.to_string();
        }

        let lower_type = source_type.to_lowercase();
        if lower_type.contains("oracle") || lower_type.contains("db2") {
            format!("{} FETCH FIRST {} ROWS ONLY", trimmed, limit)
        } else {
            format!("{} LIMIT {}", trimmed, limit)
        }
    }

    /// Extract column definitions from JSON result rows
    fn extract_column_definitions(rows: &[serde_json::Value]) -> Vec<ColumnDefinition> {
        if let Some(first_row) = rows.first() {
            if let Some(obj) = first_row.as_object() {
                return obj
                    .keys()
                    .map(|name| ColumnDefinition {
                        name: name.clone(),
                        data_type: "VARCHAR".to_string(), // Generic type, actual type unknown from results
                        nullable: true,                   // Conservative assumption
                        primary_key: false,
                        default_value: None,
                        semantic_type: None,
                        statistics: None,
                    })
                    .collect();
            }
        }
        vec![]
    }

    /// Get or create Oracle connection pool for a datasource
    #[cfg(feature = "odbc")]
    async fn get_or_create_oracle_pool(
        &self,
        source_id: &str,
        connection_string: &str,
    ) -> CatalogResult<
        deadpool::managed::Pool<
            crate::mapping::discovery::extractors::odbc::GenericOdbcConnectionManager<
                crate::mapping::discovery::extractors::OdbcOracleConnection,
            >,
        >,
    > {
        // Check if pool exists
        {
            let pools = self.oracle_pools.read().await;
            if let Some(pool) = pools.get(source_id) {
                tracing::debug!(
                    "Reusing existing Oracle pool for datasource '{}'",
                    source_id
                );
                return Ok(pool.clone());
            }
        }

        tracing::info!(
            "Creating new Oracle connection pool for datasource '{}'",
            source_id
        );

        // Create new pool
        let config = crate::mapping::discovery::extractors::odbc::OdbcPoolConfig::new(
            connection_string.to_string(),
        );
        let pool = crate::mapping::discovery::extractors::odbc::create_odbc_pool(config)
            .await
            .map_err(|e| {
                GraphicaError::Internal(format!("Failed to create Oracle pool: {:?}", e))
            })?;

        // Store pool
        {
            let mut pools = self.oracle_pools.write().await;
            pools.insert(source_id.to_string(), pool.clone());
        }

        tracing::info!(
            "Oracle connection pool created successfully for datasource '{}'",
            source_id
        );
        Ok(pool)
    }

    /// Get or create SAP HANA connection pool for a datasource
    #[cfg(feature = "odbc")]
    async fn get_or_create_saphana_pool(
        &self,
        source_id: &str,
        connection_string: &str,
    ) -> CatalogResult<
        deadpool::managed::Pool<
            crate::mapping::discovery::extractors::odbc::GenericOdbcConnectionManager<
                crate::mapping::discovery::extractors::OdbcSAPHANAConnection,
            >,
        >,
    > {
        // Check if pool exists
        {
            let pools = self.saphana_pools.read().await;
            if let Some(pool) = pools.get(source_id) {
                tracing::debug!(
                    "Reusing existing SAP HANA pool for datasource '{}'",
                    source_id
                );
                return Ok(pool.clone());
            }
        }

        tracing::info!(
            "Creating new SAP HANA connection pool for datasource '{}'",
            source_id
        );

        // Create new pool
        let config = crate::mapping::discovery::extractors::odbc::OdbcPoolConfig::new(
            connection_string.to_string(),
        );
        let pool = crate::mapping::discovery::extractors::odbc::create_odbc_pool(config)
            .await
            .map_err(|e| {
                GraphicaError::Internal(format!("Failed to create SAP HANA pool: {:?}", e))
            })?;

        // Store pool
        {
            let mut pools = self.saphana_pools.write().await;
            pools.insert(source_id.to_string(), pool.clone());
        }

        tracing::info!(
            "SAP HANA connection pool created successfully for datasource '{}'",
            source_id
        );
        Ok(pool)
    }

    /// Get or create DB2 connection pool for a datasource
    ///
    /// This method enables connection pooling for DB2 datasources, significantly
    /// improving performance for workflow executions by reusing connections.
    #[cfg(feature = "odbc")]
    async fn get_or_create_db2_pool(
        &self,
        source_id: &str,
        db2_config: &crate::mapping::loader::DB2Config,
    ) -> CatalogResult<Arc<crate::mapping::loader::DB2Pool>> {
        // Check if pool exists
        {
            let pools = self.db2_pools.read().await;
            if let Some(pool) = pools.get(source_id) {
                tracing::debug!(
                    "Reusing existing DB2 pool for datasource '{}' (pool stats: size={}, available={})",
                    source_id,
                    pool.status().size,
                    pool.status().available
                );
                return Ok(pool.clone());
            }
        }

        tracing::info!(
            "Creating new DB2 connection pool for datasource '{}'",
            source_id
        );

        // Create new pool
        use crate::mapping::loader::{create_db2_pool, DB2PoolConfig, PoolTimeouts};

        let pool_config = DB2PoolConfig {
            db2_config: db2_config.clone(),
            max_size: std::env::var("DB2_POOL_SIZE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(10),
            timeouts: PoolTimeouts::default(),
            health_check_enabled: true,
        };

        let pool = create_db2_pool(pool_config)
            .await
            .map_err(|e| GraphicaError::Internal(format!("Failed to create DB2 pool: {:?}", e)))?;

        let pool_arc = Arc::new(pool);

        // Store pool
        {
            let mut pools = self.db2_pools.write().await;
            pools.insert(source_id.to_string(), pool_arc.clone());
        }

        tracing::info!(
            "DB2 connection pool created successfully for datasource '{}' (max_size={})",
            source_id,
            pool_arc.status().max_size
        );
        Ok(pool_arc)
    }

    /// Get DB2 pool by datasource ID if it exists
    ///
    /// Returns None if no pool exists for this datasource.
    /// This is useful for external consumers who want to check if a pool is available.
    #[cfg(feature = "odbc")]
    pub async fn get_db2_pool(
        &self,
        source_id: &str,
    ) -> Option<Arc<crate::mapping::loader::DB2Pool>> {
        let pools = self.db2_pools.read().await;
        pools.get(source_id).cloned()
    }

    /// Execute query via ODBC for DB2/Oracle/SAP HANA (with connection pooling)
    async fn execute_query_via_odbc(
        &self,
        source: &DataSource,
        credentials: &Credentials,
        query: &str,
        limit: Option<usize>,
    ) -> CatalogResult<QueryResult> {
        #[cfg(feature = "odbc")]
        use crate::mapping::discovery::extractors::odbc::OdbcPoolableConnection;
        use crate::mapping::discovery::extractors::{
            db2::DB2Extractor, oracle::OracleExtractor, saphana::SAPHANAExtractor,
        };

        let start_time = std::time::Instant::now();

        // Build connection string based on source type
        let connection_string = match &source.connection.config {
            SourceConfig::DB2(_) => DB2Extractor::build_connection_string(source, credentials)
                .map_err(|e| {
                    GraphicaError::Internal(format!("Failed to build DB2 connection: {}", e))
                })?,
            SourceConfig::Oracle(_) => {
                OracleExtractor::build_connection_string(source, credentials).map_err(|e| {
                    GraphicaError::Internal(format!("Failed to build Oracle connection: {}", e))
                })?
            }
            SourceConfig::SAPHANA(_) => {
                SAPHANAExtractor::build_connection_string(source, credentials).map_err(|e| {
                    GraphicaError::Internal(format!("Failed to build SAP HANA connection: {}", e))
                })?
            }
            _ => {
                return Err(GraphicaError::Internal(
                    "Unsupported source type for ODBC execution".to_string(),
                ))
            }
        };

        // Apply limit if specified
        let final_query = if let Some(limit) = limit {
            Self::apply_limit_to_query(query, limit, &source.source_type)
        } else {
            query.to_string()
        };

        tracing::debug!(
            "Executing ODBC query for {} datasource (pooled): {}",
            source.source_type,
            &final_query[..final_query.len().min(100)]
        );

        // Execute via pooled connection or fallback to non-pooled for DB2
        #[cfg(feature = "odbc")]
        let result = match &source.connection.config {
            SourceConfig::Oracle(_) => {
                let pool = self
                    .get_or_create_oracle_pool(&source.id, &connection_string)
                    .await?;
                let mut conn = pool.get().await.map_err(|e| {
                    GraphicaError::Internal(format!(
                        "Failed to acquire Oracle connection from pool: {}",
                        e
                    ))
                })?;
                conn.execute_query_with_metadata(&final_query)
                    .map_err(|e| GraphicaError::Internal(format!("Oracle query failed: {}", e)))?
            }
            SourceConfig::SAPHANA(_) => {
                let pool = self
                    .get_or_create_saphana_pool(&source.id, &connection_string)
                    .await?;
                let mut conn = pool.get().await.map_err(|e| {
                    GraphicaError::Internal(format!(
                        "Failed to acquire SAP HANA connection from pool: {}",
                        e
                    ))
                })?;
                conn.execute_query_with_metadata(&final_query)
                    .map_err(|e| GraphicaError::Internal(format!("SAP HANA query failed: {}", e)))?
            }
            SourceConfig::DB2(_) => {
                // DB2 uses its own dedicated pool via workflow system
                // Fallback to non-pooled execution for catalog queries
                use crate::mapping::discovery::extractors::odbc::execute_odbc_query_with_metadata;
                execute_odbc_query_with_metadata(&connection_string, &final_query)
                    .await
                    .map_err(|e| {
                        GraphicaError::Internal(format!("DB2 query execution failed: {}", e))
                    })?
            }
            _ => unreachable!(),
        };

        #[cfg(not(feature = "odbc"))]
        let result = {
            return Err(GraphicaError::Internal(
                "ODBC feature is not enabled".to_string(),
            ));
        };

        let execution_time_ms = start_time.elapsed().as_millis() as u64;

        // Convert HashMap<String, String> rows to Vec<serde_json::Value>
        let json_rows: Vec<serde_json::Value> = result
            .rows
            .into_iter()
            .map(|row| serde_json::to_value(row).unwrap_or(serde_json::Value::Null))
            .collect();

        let row_count = json_rows.len();

        // Map ODBC column info to ColumnDefinition with actual types
        let columns = if result.columns.is_empty() {
            None
        } else {
            Some(
                result
                    .columns
                    .into_iter()
                    .map(|col| ColumnDefinition {
                        name: col.name,
                        data_type: col.data_type,
                        nullable: col.nullable,
                        primary_key: false, // Not available from query results
                        default_value: None,
                        semantic_type: None,
                        statistics: None,
                    })
                    .collect(),
            )
        };

        let truncated = limit.map(|l| row_count >= l).unwrap_or(false);

        let column_count = columns
            .as_ref()
            .map(|c: &Vec<ColumnDefinition>| c.len())
            .unwrap_or(0);

        tracing::info!(
            "ODBC query executed successfully: {} rows, {} columns in {}ms (truncated: {})",
            row_count,
            column_count,
            execution_time_ms,
            truncated
        );

        Ok(QueryResult {
            rows: json_rows,
            row_count,
            execution_time_ms,
            truncated,
            columns,
        })
    }

    fn discovered_schema_to_definition(discovered: DiscoveredSchema) -> SchemaDefinition {
        let tables = discovered
            .tables
            .into_iter()
            .map(|table| TableDefinition {
                name: table.name,
                columns: table
                    .columns
                    .into_iter()
                    .map(|column| {
                        // Map semantic type from discovery to catalog API type
                        let semantic_type = column
                            .semantic_type
                            .as_ref()
                            .map(|st| Self::parse_semantic_type(st));

                        // Map statistics from discovery to core statistics type
                        let statistics = {
                            let stats = &column.statistics;
                            let null_count =
                                (stats.null_fraction * stats.sample_count as f64) as u64;

                            Some(CoreColumnStatistics {
                                distinct_count: Some(stats.distinct_count as u64),
                                null_count,
                                null_percentage: stats.null_fraction * 100.0,
                                min_value: None, // Not available in discovery stats yet
                                max_value: None, // Not available in discovery stats yet
                                avg_length: stats.avg_length,
                                histogram: None,
                                most_common_values: stats.most_common_values.as_ref().map(
                                    |values| {
                                        values
                                            .iter()
                                            .map(|v| ValueFrequency {
                                                value: v.clone(),
                                                count: 0, // Discovery doesn't track count yet
                                                percentage: 0.0,
                                            })
                                            .collect()
                                    },
                                ),
                                correlation: None,
                                n_distinct: Some(stats.distinct_count as f64),
                                avg_width: stats.avg_length.map(|l| l as i32),
                                cardinality: None, // Could infer from distinct_count/sample_count ratio
                                sample_size: Some(stats.sample_count as u64),
                                last_analyzed: Some(chrono::Utc::now()),
                                statistics_stale: false,
                            })
                        };

                        ColumnDefinition {
                            name: column.name,
                            data_type: column.data_type,
                            nullable: column.nullable,
                            primary_key: column.primary_key,
                            default_value: None,
                            semantic_type,
                            statistics,
                        }
                    })
                    .collect(),
                estimated_rows: table.row_count,
            })
            .collect();

        let relationships = discovered
            .relationships
            .into_iter()
            .map(|rel| TableRelationshipDefinition {
                name: rel.name,
                source_table: rel.source_table,
                source_columns: rel.source_columns,
                target_table: rel.target_table,
                target_columns: rel.target_columns,
                relationship_type: RelationshipType::ForeignKey,
                on_delete: None,
                on_update: None,
            })
            .collect();

        SchemaDefinition {
            name: discovered.schema_name,
            tables,
            relationships,
            indexes: vec![],
            inferred_at: chrono::Utc::now(),
        }
    }
}

#[async_trait]
impl DataSourceCatalog for InMemoryDataSourceCatalog {
    async fn register_source(&self, mut source: DataSource) -> CatalogResult<DataSourceResponse> {
        source.source_type = source.connection.config.source_type().to_string();

        let secret_store_registry = { self.secret_store_registry.read().clone() };

        if let Some(registry) = secret_store_registry {
            if let Some(store) = registry.default().or_else(|| registry.get("default")) {
                if let Ok(true) = self.promote_inline_credentials(&mut source, &store).await {
                    source.updated_at = Some(Utc::now());
                }
            }
        }

        let response = DataSourceResponse {
            source: source.clone(),
            status: DataSourceStatus::Active,
            last_test_result: None,
            capabilities: Some(self.catalog_capabilities_for_source(&source)),
        };

        // Persist to RocksDB first
        if let Err(e) = self.save_to_db(&source.id, &response) {
            tracing::error!(
                "Failed to persist datasource {} to RocksDB: {}",
                source.id,
                e
            );
            return Err(GraphicaError::Internal(format!(
                "Failed to persist datasource: {}",
                e
            )));
        }

        // Update in-memory cache
        let mut sources = self.sources.write();
        sources.insert(source.id.clone(), response.clone());

        tracing::info!("Registered datasource {} (persisted to disk)", source.id);

        // Sync to RDF store as gph:Dataset
        if let Some(ref rdf_store) = self.rdf_store {
            let turtle = catalog_to_dataset::datasource_to_dataset_triples(&source);
            if let Err(e) = rdf_store.load_turtle(&turtle, None) {
                tracing::warn!("Failed to sync datasource {} to RDF: {}", source.id, e);
            } else {
                tracing::info!("Synced datasource {} to RDF as dataset", source.id);
            }
        }

        Ok(response)
    }

    async fn get_source(&self, id: &str) -> CatalogResult<DataSourceResponse> {
        let sources = self.sources.read();
        sources
            .get(id)
            .cloned()
            .map(|response| self.enrich_response(response))
            .ok_or_else(|| GraphicaError::NotFound(format!("Data source not found: {}", id)))
    }

    async fn update_source(
        &self,
        id: &str,
        updates: UpdateDataSourcePatch,
    ) -> CatalogResult<DataSourceResponse> {
        if updates.is_empty() {
            return self.get_source(id).await;
        }

        let updated_response = {
            let mut sources = self.sources.write();

            let response = sources
                .get_mut(id)
                .ok_or_else(|| GraphicaError::NotFound(format!("Data source not found: {}", id)))?;

            if let Some(title) = updates.title {
                response.source.title = title;
            }
            if let Some(description) = updates.description {
                response.source.description = Some(description);
            }
            if let Some(connection) = updates.connection {
                response.source.connection = connection;
            }
            if let Some(schema_ref) = updates.schema_ref {
                response.source.schema_ref = Some(schema_ref);
            }
            if let Some(tags) = updates.tags {
                response.source.tags = tags;
            }
            if let Some(metadata) = updates.metadata {
                response.source.metadata = metadata;
            }

            if let Some(source_type) = updates.source_type {
                let normalized = normalize_source_type_name(&source_type).ok_or_else(|| {
                    GraphicaError::Configuration(format!(
                        "Unsupported source type '{}'",
                        source_type
                    ))
                })?;

                if normalized != response.source.connection.config.source_type() {
                    return Err(GraphicaError::Configuration(format!(
                        "Source type '{}' does not match connection type '{}'",
                        source_type,
                        response.source.connection.config.source_type()
                    )));
                }
            }

            response.source.source_type =
                response.source.connection.config.source_type().to_string();
            response
                .source
                .validate()
                .map_err(|errors| GraphicaError::Configuration(errors.join(", ")))?;

            response.source.updated_at = Some(Utc::now());
            response.capabilities = Some(self.catalog_capabilities_for_source(&response.source));

            response.clone()
        };

        // Persist update to RocksDB
        if let Err(e) = self.save_to_db(id, &updated_response) {
            tracing::error!(
                "Failed to persist datasource {} update to RocksDB: {}",
                id,
                e
            );
            return Err(GraphicaError::Internal(format!(
                "Failed to persist update: {}",
                e
            )));
        }

        tracing::info!("Updated datasource {} (persisted to disk)", id);

        Ok(self.enrich_response(updated_response))
    }

    async fn delete_source(&self, id: &str) -> CatalogResult<()> {
        // Delete from RocksDB first
        if let Err(e) = self.delete_from_db(id) {
            tracing::error!("Failed to delete datasource {} from RocksDB: {}", id, e);
            return Err(GraphicaError::Internal(format!(
                "Failed to delete datasource: {}",
                e
            )));
        }

        // Remove from in-memory cache
        let mut sources = self.sources.write();
        sources
            .remove(id)
            .ok_or_else(|| GraphicaError::NotFound(format!("Data source not found: {}", id)))?;

        tracing::info!("Deleted datasource {} (removed from disk)", id);

        Ok(())
    }

    async fn list_sources(
        &self,
        request: &ListDataSourcesRequest,
    ) -> CatalogResult<ListDataSourcesResponse> {
        let sources = self.sources.read();

        // Apply filters if specified
        let mut filtered: Vec<DataSourceResponse> = sources
            .values()
            .filter(|s| {
                // Filter by status if specified
                if let Some(ref status_filter) = request.status {
                    if s.status != *status_filter {
                        return false;
                    }
                }

                // Filter by source_type if specified
                if let Some(ref type_filter) = request.source_type {
                    let requested = normalize_source_type_name(type_filter).unwrap_or(type_filter);
                    if s.source.connection.config.source_type() != requested {
                        return false;
                    }
                }

                // Filter by tags if specified
                if let Some(ref tags_filter) = request.tags {
                    if !tags_filter.iter().any(|tag| s.source.tags.contains(tag)) {
                        return false;
                    }
                }

                true
            })
            .cloned()
            .collect();

        let total = filtered.len();

        // Apply pagination
        let page = request.page;
        let page_size = request.page_size;
        let start = page * page_size;
        let end = std::cmp::min(start + page_size, total);

        if start < total {
            filtered = filtered[start..end].to_vec();
        } else {
            filtered.clear();
        }

        Ok(ListDataSourcesResponse {
            sources: filtered
                .into_iter()
                .map(|response| self.enrich_response(response))
                .collect(),
            total,
            page,
            page_size,
        })
    }

    async fn test_connection(&self, id: &str) -> CatalogResult<ConnectionTestResult> {
        let source = self.get_source(id).await?.source;
        let connector = self.get_connector(&source)?;
        let credentials = self.extract_credentials(&source).await?;

        // Call the actual connector's test_connection method
        let result = connector.test_connection(&source, credentials).await?;

        // Store the test result and update status
        let updated_response = {
            let mut sources = self.sources.write();
            if let Some(response) = sources.get_mut(id) {
                response.last_test_result = Some(result.clone());
                // Update status based on test result
                response.status = if result.success {
                    DataSourceStatus::Active
                } else {
                    DataSourceStatus::Error
                };
                Some(response.clone())
            } else {
                None
            }
        };

        // Persist status update to RocksDB
        if let Some(response) = updated_response {
            if let Err(e) = self.save_to_db(id, &response) {
                tracing::warn!("Failed to persist test result for datasource {}: {}", id, e);
            }
        }

        Ok(result)
    }

    async fn infer_schema(
        &self,
        id: &str,
        table_name: Option<&str>,
        sample_size: usize,
    ) -> CatalogResult<SchemaDefinition> {
        let source = self.get_source(id).await?.source;
        let discovery = self.discovery_service.read().clone();
        let source_type = source.connection.config.source_type();
        let schema = if let Some(discovery) = discovery {
            if discovery.supports_source_type(source_type) {
                match discovery
                    .discover_by_source_id(id, table_name, sample_size)
                    .await
                {
                    Ok(discovered) => Self::discovered_schema_to_definition(discovered),
                    Err(e) => {
                        tracing::warn!(
                            "Discovery service failed for datasource {}: {}. Falling back to connector.",
                            id,
                            e
                        );
                        let connector = self.get_connector(&source)?;
                        let credentials = self.extract_credentials(&source).await?;
                        connector
                            .infer_schema(&source, credentials, table_name, sample_size)
                            .await?
                    }
                }
            } else {
                let connector = self.get_connector(&source)?;
                let credentials = self.extract_credentials(&source).await?;
                connector
                    .infer_schema(&source, credentials, table_name, sample_size)
                    .await?
            }
        } else {
            let connector = self.get_connector(&source)?;
            let credentials = self.extract_credentials(&source).await?;
            connector
                .infer_schema(&source, credentials, table_name, sample_size)
                .await?
        };

        // Sync schema to RDF store as gph:DatasetColumn triples
        if let Some(ref rdf_store) = self.rdf_store {
            let turtle = catalog_to_dataset::schema_to_column_triples(id, &schema);
            if let Err(e) = rdf_store.load_turtle(&turtle, None) {
                tracing::warn!("Failed to sync schema for datasource {} to RDF: {}", id, e);
            } else {
                tracing::info!(
                    "Synced schema for datasource {} to RDF ({} columns)",
                    id,
                    schema.tables.iter().map(|t| t.columns.len()).sum::<usize>()
                );
            }

            let table_datasets_turtle =
                catalog_to_dataset::schema_to_table_dataset_triples(&source, &schema);
            if let Err(e) = rdf_store.load_turtle(&table_datasets_turtle, None) {
                tracing::warn!(
                    "Failed to sync table datasets for datasource {} to RDF: {}",
                    id,
                    e
                );
            } else {
                tracing::info!(
                    "Synced {} table-level datasets for datasource {} to RDF",
                    schema.tables.len(),
                    id
                );
            }
        }

        Ok(schema)
    }

    async fn execute_query(
        &self,
        id: &str,
        query: &str,
        parameters: HashMap<String, serde_json::Value>,
        limit: Option<usize>,
    ) -> CatalogResult<QueryResult> {
        let source = self.get_source(id).await?.source;
        let credentials = self.extract_credentials(&source).await?;
        let source_type = source.connection.config.source_type();

        // Route ODBC-compatible sources (DB2, Oracle, SAP HANA) to ODBC execution
        if Self::should_use_odbc(source_type) {
            tracing::debug!("Routing {} query to ODBC execution path", source_type);

            if !parameters.is_empty() {
                return Err(GraphicaError::Configuration(format!(
                    "Parameterized queries are not supported for {} datasources until native ODBC bind support is implemented",
                    source_type
                )));
            }

            return self
                .execute_query_via_odbc(&source, &credentials, query, limit)
                .await;
        }

        // For other sources, use the connector execution path
        let connector = self.get_connector(&source)?;
        connector
            .execute_query(&source, credentials, query, parameters, limit, 30)
            .await
    }

    async fn mark_synced(&self, id: &str) -> CatalogResult<()> {
        let updated_response = {
            let mut sources = self.sources.write();

            if let Some(response) = sources.get_mut(id) {
                response.source.last_synced_at = Some(Utc::now());
                Some(response.clone())
            } else {
                None
            }
        };

        // Persist sync timestamp to RocksDB
        if let Some(response) = updated_response {
            if let Err(e) = self.save_to_db(id, &response) {
                tracing::warn!(
                    "Failed to persist sync timestamp for datasource {}: {}",
                    id,
                    e
                );
            }
        }

        Ok(())
    }

    async fn update_status(
        &self,
        id: &str,
        status: DataSourceStatus,
        error_message: Option<String>,
    ) -> CatalogResult<()> {
        let updated_response = {
            let mut sources = self.sources.write();

            let response = sources
                .get_mut(id)
                .ok_or_else(|| GraphicaError::NotFound(format!("Data source not found: {}", id)))?;

            response.status = status;

            if let Some(error) = error_message {
                response
                    .source
                    .metadata
                    .insert("last_error".to_string(), error);
            }

            response.clone()
        };

        // Persist status update to RocksDB
        if let Err(e) = self.save_to_db(id, &updated_response) {
            tracing::error!(
                "Failed to persist status update for datasource {}: {}",
                id,
                e
            );
            return Err(GraphicaError::Internal(format!(
                "Failed to persist status update: {}",
                e
            )));
        }

        Ok(())
    }

    async fn search_sources(
        &self,
        query: &str,
        limit: usize,
    ) -> CatalogResult<Vec<DataSourceResponse>> {
        let sources = self.sources.read();
        let query_lower = query.to_lowercase();

        let mut results: Vec<DataSourceResponse> = sources
            .values()
            .filter(|s| {
                // Search in title, description, tags
                s.source.title.to_lowercase().contains(&query_lower)
                    || s.source
                        .description
                        .as_ref()
                        .map(|d| d.to_lowercase().contains(&query_lower))
                        .unwrap_or(false)
                    || s.source
                        .tags
                        .iter()
                        .any(|t| t.to_lowercase().contains(&query_lower))
            })
            .cloned()
            .collect();

        // Apply limit
        if results.len() > limit {
            results.truncate(limit);
        }

        Ok(results
            .into_iter()
            .map(|response| self.enrich_response(response))
            .collect())
    }

    async fn get_sources_by_tag(&self, tag: &str) -> CatalogResult<Vec<DataSourceResponse>> {
        let sources = self.sources.read();

        let results = sources
            .values()
            .filter(|s| s.source.tags.contains(&tag.to_string()))
            .cloned()
            .map(|response| self.enrich_response(response))
            .collect();

        Ok(results)
    }

    async fn get_usage_stats(&self, _id: &str) -> CatalogResult<UsageStatistics> {
        // In-memory catalog doesn't track usage
        // In production, this would query the lineage graph
        Ok(UsageStatistics {
            workflow_count: 0,
            last_used: None,
            total_records_processed: 0,
            workflow_ids: vec![],
        })
    }

    async fn get_source_by_title(&self, title: &str) -> CatalogResult<DataSourceResponse> {
        let sources = self.sources.read();

        sources
            .values()
            .find(|s| s.source.title == title)
            .cloned()
            .map(|response| self.enrich_response(response))
            .ok_or_else(|| {
                GraphicaError::NotFound(format!("Data source not found with title: {}", title))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_catalog_register_and_get() {
        let registry = Arc::new(RwLock::new(ConnectorRegistry::new()));
        let catalog = InMemoryDataSourceCatalog::new(registry);

        let source = DataSource::new(
            "Test Source".to_string(),
            "PostgreSQL".to_string(),
            ConnectionDetails {
                secret_ref: "vault://test".to_string(),
                config: SourceConfig::PostgreSQL(graphica_core::catalog::types::PostgreSQLConfig {
                    host: "localhost".to_string(),
                    port: 5432,
                    database: "test".to_string(),
                    schema: None,
                    ssl_mode: None,
                }),
                encryption_enabled: true,
                credentials: Default::default(),
            },
        );

        // Register source
        let response = catalog.register_source(source.clone()).await.unwrap();
        assert_eq!(response.status, DataSourceStatus::Active);

        // Retrieve source
        let retrieved = catalog.get_source(&source.id).await.unwrap();
        assert_eq!(retrieved.source.title, "Test Source");
    }

    #[tokio::test]
    async fn test_catalog_list() {
        // Use unique temporary directory for test isolation
        let temp_dir =
            std::env::temp_dir().join(format!("test_catalog_list_{}", uuid::Uuid::new_v4()));
        let registry = Arc::new(RwLock::new(ConnectorRegistry::new()));
        let catalog =
            InMemoryDataSourceCatalog::new_with_path(registry, temp_dir.to_str().unwrap()).unwrap();

        // Register multiple sources
        for i in 0..3 {
            let source = DataSource::new(
                format!("Source {}", i),
                "PostgreSQL".to_string(),
                ConnectionDetails {
                    secret_ref: "vault://test".to_string(),
                    config: SourceConfig::PostgreSQL(
                        graphica_core::catalog::types::PostgreSQLConfig {
                            host: "localhost".to_string(),
                            port: 5432,
                            database: "test".to_string(),
                            schema: None,
                            ssl_mode: None,
                        },
                    ),
                    encryption_enabled: true,
                    credentials: Default::default(),
                },
            );
            catalog.register_source(source).await.unwrap();
        }

        // List sources
        let request = ListDataSourcesRequest::default();
        let response = catalog.list_sources(&request).await.unwrap();
        assert_eq!(response.total, 3);
        assert_eq!(response.sources.len(), 3);

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn test_execute_query_rejects_parameters_for_odbc_sources() {
        let registry = Arc::new(RwLock::new(ConnectorRegistry::new()));
        let catalog = InMemoryDataSourceCatalog::new(registry);

        let source = DataSource::new(
            "Oracle Source".to_string(),
            "Oracle".to_string(),
            ConnectionDetails {
                secret_ref: "vault://oracle".to_string(),
                config: SourceConfig::Oracle(graphica_core::catalog::types::OracleConfig {
                    host: "oracle.example.com".to_string(),
                    port: 1521,
                    service_name: Some("ORCL".to_string()),
                    sid: None,
                    schema: Some("APP".to_string()),
                }),
                encryption_enabled: true,
                credentials: HashMap::from([
                    ("username".to_string(), "scott".to_string()),
                    ("password".to_string(), "tiger".to_string()),
                ]),
            },
        );

        let source_id = source.id.clone();
        catalog.register_source(source).await.unwrap();

        let result = catalog
            .execute_query(
                &source_id,
                "SELECT * FROM DUAL WHERE :id = 1",
                HashMap::from([("id".to_string(), serde_json::json!(1))]),
                Some(10),
            )
            .await;

        assert!(
            matches!(
                &result,
                Err(GraphicaError::Configuration(message))
                    if message.contains("Parameterized queries are not supported")
            ),
            "expected parameter rejection, got {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_update_source_rejects_source_type_mismatch() {
        let registry = Arc::new(RwLock::new(ConnectorRegistry::new()));
        let catalog = InMemoryDataSourceCatalog::new(registry);

        let source = DataSource::new(
            "Postgres Source".to_string(),
            "PostgreSQL".to_string(),
            ConnectionDetails {
                secret_ref: "vault://postgres".to_string(),
                config: SourceConfig::PostgreSQL(graphica_core::catalog::types::PostgreSQLConfig {
                    host: "localhost".to_string(),
                    port: 5432,
                    database: "app".to_string(),
                    schema: Some("public".to_string()),
                    ssl_mode: Some("require".to_string()),
                }),
                encryption_enabled: true,
                credentials: Default::default(),
            },
        );

        let source_id = source.id.clone();
        catalog.register_source(source).await.unwrap();

        let result = catalog
            .update_source(
                &source_id,
                UpdateDataSourcePatch {
                    title: None,
                    description: None,
                    source_type: Some("Oracle".to_string()),
                    connection: None,
                    schema_ref: None,
                    tags: None,
                    metadata: None,
                },
            )
            .await;

        assert!(
            matches!(
                &result,
                Err(GraphicaError::Configuration(message))
                    if message.contains("does not match connection type")
            ),
            "expected source type mismatch, got {:?}",
            result
        );
    }
}
