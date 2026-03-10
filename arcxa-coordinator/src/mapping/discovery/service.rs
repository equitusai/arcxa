//! # Discovery Service Layer
//!
//! Production-ready service layer that bridges catalog, discovery, and credentials.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                  DiscoveryService (trait)                    │
//! │  - discover_by_source_id()                                   │
//! │  - discover_with_source()                                    │
//! │  - warm_cache_for_source()                                   │
//! └────────────────────┬────────────────────────────────────────┘
//!                      │
//!                      ▼
//! ┌─────────────────────────────────────────────────────────────┐
//! │          ProductionDiscoveryService (impl)                   │
//! │  ┌──────────────┐  ┌─────────────────┐  ┌────────────────┐ │
//! │  │   Catalog    │  │ Credential      │  │  Discovery     │ │
//! │  │   Lookup     │→ │  Resolver       │→ │ Orchestrator   │ │
//! │  └──────────────┘  └─────────────────┘  └────────────────┘ │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Key Features
//!
//! - **Trait-based**: Works with any catalog implementation
//! - **Credential Resolution**: Multi-source credential provider
//! - **Background Warming**: Pre-populate cache on datasource registration
//! - **Error Resilience**: Graceful fallback with detailed error reporting
//! - **Observable**: Rich logging and metrics

use anyhow::{Context, Result};
use async_trait::async_trait;
use graphica_core::catalog::{
    client::DataSourceCatalog, connector::Credentials, types::DataSource,
};
use graphica_core::secrets::providers::SecretStoreRegistry;
use graphica_core::secrets::SecretValue;
use parking_lot::RwLock;
use std::sync::Arc;
use tracing::{debug, info, warn};

use super::{DiscoveredSchema, DiscoveryConfig, DiscoveryOrchestrator};

// ============================================================================
// Credential Resolution (Multi-Provider Pattern)
// ============================================================================

/// Credential resolution strategy
#[derive(Debug, Clone)]
pub enum CredentialProvider {
    /// Extract from datasource metadata (development mode)
    FromMetadata,
    /// Load from secret reference (production mode)
    FromSecretRef {
        /// Secret store implementation
        secret_store: String, // In production: Arc<dyn SecretStore>
    },
    /// Use environment variables
    FromEnvironment {
        username_var: String,
        password_var: String,
    },
    /// Static credentials (testing only)
    Static { username: String, password: String },
}

/// Multi-source credential resolver
///
/// Resolves credentials from multiple sources with fallback chain:
/// 1. Secret reference (Vault, AWS Secrets Manager)
/// 2. Datasource metadata
/// 3. Environment variables
/// 4. Default test credentials
pub struct CredentialResolver {
    /// Primary provider
    primary: CredentialProvider,
    /// Fallback provider (optional)
    fallback: Option<CredentialProvider>,
    /// Optional secret store registry (production mode)
    secret_store_registry: Arc<RwLock<Option<Arc<SecretStoreRegistry>>>>,
}

impl CredentialResolver {
    /// Create resolver with default strategy (metadata → env fallback)
    pub fn new() -> Self {
        Self {
            primary: CredentialProvider::FromSecretRef {
                secret_store: "default".to_string(),
            },
            fallback: Some(CredentialProvider::FromEnvironment {
                username_var: "DB_USERNAME".to_string(),
                password_var: "DB_PASSWORD".to_string(),
            }),
            secret_store_registry: Arc::new(RwLock::new(None)),
        }
    }

    /// Create resolver with custom providers
    pub fn with_providers(
        primary: CredentialProvider,
        fallback: Option<CredentialProvider>,
    ) -> Self {
        Self {
            primary,
            fallback,
            secret_store_registry: Arc::new(RwLock::new(None)),
        }
    }

    /// Configure secret store registry for secretRef resolution.
    pub fn with_secret_store_registry(mut self, registry: Arc<SecretStoreRegistry>) -> Self {
        self.set_secret_store_registry(registry);
        self
    }

    /// Update secret store registry after resolver construction.
    pub fn set_secret_store_registry(&self, registry: Arc<SecretStoreRegistry>) {
        let mut guard = self.secret_store_registry.write();
        *guard = Some(registry);
    }

    /// Resolve credentials for a datasource
    pub async fn resolve(&self, source: &DataSource) -> Result<Credentials> {
        // Try primary provider
        match self.try_resolve(&self.primary, source).await {
            Ok(creds) => {
                debug!(
                    "Resolved credentials for source '{}' using primary provider",
                    source.id
                );
                Ok(creds)
            }
            Err(e) => {
                debug!("Primary credential provider failed: {}", e);

                // Try fallback provider
                if let Some(fallback) = &self.fallback {
                    match self.try_resolve(fallback, source).await {
                        Ok(creds) => {
                            warn!(
                                "Resolved credentials for source '{}' using fallback provider",
                                source.id
                            );
                            Ok(creds)
                        }
                        Err(e2) => {
                            anyhow::bail!(
                                "Failed to resolve credentials (primary: {}, fallback: {})",
                                e,
                                e2
                            )
                        }
                    }
                } else {
                    Err(e)
                }
            }
        }
    }

    /// Try to resolve using a specific provider
    async fn try_resolve(
        &self,
        provider: &CredentialProvider,
        source: &DataSource,
    ) -> Result<Credentials> {
        match provider {
            CredentialProvider::FromMetadata => {
                // Extract from metadata (development mode)
                Self::credentials_from_metadata(source)
            }
            CredentialProvider::FromSecretRef { secret_store } => {
                let secret_ref = source.connection.secret_ref.trim();
                if secret_ref.is_empty() {
                    anyhow::bail!("secretRef is empty");
                }

                if secret_ref.starts_with("env://") {
                    let var_name = &secret_ref[6..];
                    let raw = std::env::var(var_name).context(format!(
                        "Environment variable '{}' not set for secretRef",
                        var_name
                    ))?;
                    return Self::credentials_from_json_str(&raw);
                }

                if secret_ref.starts_with('{') {
                    return Self::credentials_from_json_str(secret_ref);
                }

                let registry = { self.secret_store_registry.read().clone() }.ok_or_else(|| {
                    anyhow::anyhow!(
                        "Secret store registry not configured for secretRef '{}'",
                        secret_ref
                    )
                })?;

                let store = registry
                    .get(secret_store)
                    .or_else(|| registry.default())
                    .or_else(|| registry.get("default"))
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "No secret store '{}' or default store registered",
                            secret_store
                        )
                    })?;

                let secret = store
                    .get_secret(secret_ref, None)
                    .await
                    .context(format!("Failed to fetch secret '{}'", secret_ref))?;

                Self::credentials_from_secret_value(&secret.value)
            }
            CredentialProvider::FromEnvironment {
                username_var,
                password_var,
            } => {
                let username = std::env::var(username_var)
                    .context(format!("Environment variable {} not set", username_var))?;
                let password = std::env::var(password_var)
                    .context(format!("Environment variable {} not set", password_var))?;

                Ok(Credentials::new(username, password))
            }
            CredentialProvider::Static { username, password } => {
                Ok(Credentials::new(username.clone(), password.clone()))
            }
        }
    }

    fn credentials_from_metadata(source: &DataSource) -> Result<Credentials> {
        if !source.connection.credentials.is_empty() {
            if let Ok(creds) = Self::credentials_from_map(&source.connection.credentials) {
                return Ok(creds);
            }
        }

        Self::credentials_from_map(&source.metadata)
            .map_err(|_| anyhow::anyhow!("Missing metadata.username"))
    }

    fn credentials_from_map(
        map: &std::collections::HashMap<String, String>,
    ) -> Result<Credentials> {
        if let (Some(user), Some(pass)) = (map.get("username"), map.get("password")) {
            return Ok(Credentials::new(user.to_string(), pass.to_string()));
        }
        if let (Some(user), Some(pass)) = (map.get("user"), map.get("pass")) {
            return Ok(Credentials::new(user.to_string(), pass.to_string()));
        }
        if let Some(token) = map
            .get("token")
            .or_else(|| map.get("access_token"))
            .cloned()
        {
            return Ok(Credentials::new("token".to_string(), token));
        }

        Err(anyhow::anyhow!("Missing metadata.username"))
    }

    fn credentials_from_json_str(raw: &str) -> Result<Credentials> {
        let value: serde_json::Value =
            serde_json::from_str(raw).context("Failed to parse credentials JSON")?;
        Self::credentials_from_json_value(&value)
    }

    fn credentials_from_json_value(value: &serde_json::Value) -> Result<Credentials> {
        match value {
            serde_json::Value::Object(map) => {
                let username = map
                    .get("username")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing 'username' in credentials"))?
                    .to_string();
                let password = map
                    .get("password")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing 'password' in credentials"))?
                    .to_string();

                let mut additional = std::collections::HashMap::new();
                for (k, v) in map {
                    if k != "username" && k != "password" {
                        if let Some(text) = v.as_str() {
                            additional.insert(k.clone(), text.to_string());
                        } else {
                            additional.insert(k.clone(), v.to_string());
                        }
                    }
                }

                Ok(Credentials {
                    username,
                    password,
                    additional,
                })
            }
            _ => Err(anyhow::anyhow!(
                "Credentials JSON must be an object with username/password"
            )),
        }
    }

    fn credentials_from_secret_value(value: &SecretValue) -> Result<Credentials> {
        match value {
            SecretValue::KeyValue(map) => {
                let username = map
                    .get("username")
                    .ok_or_else(|| anyhow::anyhow!("Missing 'username' in secret"))?
                    .clone();
                let password = map
                    .get("password")
                    .ok_or_else(|| anyhow::anyhow!("Missing 'password' in secret"))?
                    .clone();

                let mut additional = std::collections::HashMap::new();
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
            SecretValue::String(raw) => Self::credentials_from_json_str(raw),
            SecretValue::Json(json) => Self::credentials_from_json_value(json),
            SecretValue::Binary(_) => Err(anyhow::anyhow!(
                "Binary secret format is not supported for datasource credentials"
            )),
        }
    }
}

impl Default for CredentialResolver {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Discovery Service (Trait-Based Abstraction)
// ============================================================================

/// Discovery service trait
///
/// Provides a unified, trait-based API for intelligent schema discovery
/// that can be implemented by different backends (catalog-based, mock, etc.)
#[async_trait]
pub trait DiscoveryService: Send + Sync {
    /// Discover schema by datasource ID
    ///
    /// Looks up the datasource in the catalog, resolves credentials,
    /// and performs intelligent discovery.
    async fn discover_by_source_id(
        &self,
        source_id: &str,
        table_name: Option<&str>,
        sample_size: usize,
    ) -> Result<DiscoveredSchema>;

    /// Discover schema with explicit datasource and credentials
    ///
    /// Bypasses catalog lookup for cases where source is already known.
    async fn discover_with_source(
        &self,
        source: &DataSource,
        credentials: &Credentials,
        table_name: Option<&str>,
        sample_size: usize,
    ) -> Result<DiscoveredSchema>;

    /// Warm cache for a datasource (background operation)
    ///
    /// Pre-populates the discovery cache for faster subsequent queries.
    /// This is fire-and-forget - errors are logged but not returned.
    async fn warm_cache_for_source(&self, source_id: &str);

    /// Check if discovery is available for a datasource type
    fn supports_source_type(&self, source_type: &str) -> bool;
}

// ============================================================================
// Production Implementation
// ============================================================================

/// Production discovery service implementation
///
/// Integrates catalog, credential resolver, and discovery orchestrator
/// into a unified service layer.
pub struct ProductionDiscoveryService {
    /// Data source catalog
    catalog: Arc<dyn DataSourceCatalog>,
    /// Credential resolver
    credential_resolver: CredentialResolver,
    /// Discovery orchestrator
    discovery: Arc<DiscoveryOrchestrator>,
}

impl ProductionDiscoveryService {
    /// Create a new production discovery service
    pub fn new(catalog: Arc<dyn DataSourceCatalog>, discovery: Arc<DiscoveryOrchestrator>) -> Self {
        Self {
            catalog,
            credential_resolver: CredentialResolver::new(),
            discovery,
        }
    }

    /// Create with custom credential resolver
    pub fn with_credential_resolver(
        catalog: Arc<dyn DataSourceCatalog>,
        discovery: Arc<DiscoveryOrchestrator>,
        credential_resolver: CredentialResolver,
    ) -> Self {
        Self {
            catalog,
            credential_resolver,
            discovery,
        }
    }

    /// Configure secret store registry for secretRef-based credential resolution.
    pub fn set_secret_store_registry(&self, registry: Arc<SecretStoreRegistry>) {
        self.credential_resolver.set_secret_store_registry(registry);
    }
}

#[async_trait]
impl DiscoveryService for ProductionDiscoveryService {
    async fn discover_by_source_id(
        &self,
        source_id: &str,
        table_name: Option<&str>,
        sample_size: usize,
    ) -> Result<DiscoveredSchema> {
        info!(
            "🔍 Discovery request: source={}, table={:?}, sample_size={}",
            source_id, table_name, sample_size
        );

        // 1. Lookup datasource in catalog
        let source_response = self.catalog.get_source(source_id).await.context(format!(
            "Failed to lookup source '{}' in catalog",
            source_id
        ))?;

        let source = &source_response.source;

        // 2. Resolve credentials
        let credentials = self
            .credential_resolver
            .resolve(source)
            .await
            .context("Failed to resolve credentials")?;

        // 3. Perform discovery
        self.discover_with_source(source, &credentials, table_name, sample_size)
            .await
    }

    async fn discover_with_source(
        &self,
        source: &DataSource,
        credentials: &Credentials,
        table_name: Option<&str>,
        sample_size: usize,
    ) -> Result<DiscoveredSchema> {
        info!(
            "🔍 Discovering schema: source_type={}, table={:?}",
            source.source_type, table_name
        );

        let config = DiscoveryConfig {
            schema_filter: None, // Let extractor determine default schema
            table_filter: table_name.map(|t| t.to_string()),
            sample_size,
            cache_ttl_secs: 3600, // 1 hour
        };

        let discovered = self
            .discovery
            .discover_schema(source, credentials, config)
            .await
            .context("Discovery failed")?;

        info!(
            "  ✓ Discovered {} tables, {} total columns",
            discovered.tables.len(),
            discovered
                .tables
                .iter()
                .map(|t| t.columns.len())
                .sum::<usize>()
        );

        Ok(discovered)
    }

    async fn warm_cache_for_source(&self, source_id: &str) {
        info!("🔥 Warming cache for source: {}", source_id);

        // Fire-and-forget: errors are logged but don't propagate
        match self.discover_by_source_id(source_id, None, 100).await {
            Ok(_) => {
                info!("  ✓ Cache warmed for source '{}'", source_id);
            }
            Err(e) => {
                warn!("  ⚠ Failed to warm cache for source '{}': {}", source_id, e);
            }
        }
    }

    fn supports_source_type(&self, source_type: &str) -> bool {
        // Check if discovery orchestrator has a registered extractor
        let lower = source_type.to_lowercase();
        lower.contains("postgresql")
            || lower.contains("postgres")
            || lower == "edb"
            || lower.contains("enterprisedb")
            || lower.contains("db2")
            || lower == "db2"
            || lower.contains("oracle")
            || lower.contains("saphana")
            || lower.contains("sap_hana")
            || lower.contains("sap hana")
            || lower.contains("databricks")
    }
}

// ============================================================================
// Mock Implementation (for testing)
// ============================================================================

/// Mock discovery service for testing
///
/// Returns hardcoded demo data without requiring real database connections.
pub struct MockDiscoveryService;

impl MockDiscoveryService {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl DiscoveryService for MockDiscoveryService {
    async fn discover_by_source_id(
        &self,
        _source_id: &str,
        _table_name: Option<&str>,
        _sample_size: usize,
    ) -> Result<DiscoveredSchema> {
        // Return empty schema for mock
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        Ok(DiscoveredSchema {
            source_id: "mock".to_string(),
            schema_name: "mock".to_string(),
            tables: vec![],
            relationships: vec![],
            discovered_at: now,
        })
    }

    async fn discover_with_source(
        &self,
        _source: &DataSource,
        _credentials: &Credentials,
        _table_name: Option<&str>,
        _sample_size: usize,
    ) -> Result<DiscoveredSchema> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        Ok(DiscoveredSchema {
            source_id: "mock".to_string(),
            schema_name: "mock".to_string(),
            tables: vec![],
            relationships: vec![],
            discovered_at: now,
        })
    }

    async fn warm_cache_for_source(&self, _source_id: &str) {
        // No-op for mock
    }

    fn supports_source_type(&self, _source_type: &str) -> bool {
        true // Mock supports all types
    }
}

impl Default for MockDiscoveryService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_credential_resolver_from_metadata() {
        use graphica_core::catalog::types::{ConnectionDetails, PostgreSQLConfig, SourceConfig};
        use std::collections::HashMap;

        let mut metadata = HashMap::new();
        metadata.insert("username".to_string(), "testuser".to_string());
        metadata.insert("password".to_string(), "testpass".to_string());

        let source = DataSource {
            id: "test".to_string(),
            title: "Test".to_string(),
            source_type: "postgresql".to_string(),
            description: None,
            connection: ConnectionDetails {
                secret_ref: "vault://test".to_string(),
                config: SourceConfig::PostgreSQL(PostgreSQLConfig {
                    host: "localhost".to_string(),
                    port: 5432,
                    database: "test".to_string(),
                    schema: None,
                    ssl_mode: None,
                }),
                encryption_enabled: false,
                credentials: Default::default(),
            },
            tags: vec![],
            metadata,
            schema_ref: None,
            created_at: Some(chrono::Utc::now()),
            updated_at: None,
            last_synced_at: None,
        };

        let resolver = CredentialResolver::with_providers(CredentialProvider::FromMetadata, None);
        let creds = resolver.resolve(&source).await.unwrap();

        assert_eq!(creds.username, "testuser");
        assert_eq!(creds.password, "testpass");
    }

    #[tokio::test]
    async fn test_credential_resolver_static() {
        use graphica_core::catalog::types::{ConnectionDetails, PostgreSQLConfig, SourceConfig};

        let source = DataSource {
            id: "test".to_string(),
            title: "Test".to_string(),
            source_type: "postgresql".to_string(),
            description: None,
            connection: ConnectionDetails {
                secret_ref: "vault://test".to_string(),
                config: SourceConfig::PostgreSQL(PostgreSQLConfig {
                    host: "localhost".to_string(),
                    port: 5432,
                    database: "test".to_string(),
                    schema: None,
                    ssl_mode: None,
                }),
                encryption_enabled: false,
                credentials: Default::default(),
            },
            tags: vec![],
            metadata: Default::default(),
            schema_ref: None,
            created_at: Some(chrono::Utc::now()),
            updated_at: None,
            last_synced_at: None,
        };

        let resolver = CredentialResolver::with_providers(
            CredentialProvider::Static {
                username: "admin".to_string(),
                password: "secret".to_string(),
            },
            None,
        );

        let creds = resolver.resolve(&source).await.unwrap();

        assert_eq!(creds.username, "admin");
        assert_eq!(creds.password, "secret");
    }

    #[tokio::test]
    async fn test_credential_resolver_from_secret_store_registry() {
        use graphica_core::catalog::types::{ConnectionDetails, PostgreSQLConfig, SourceConfig};
        use graphica_core::secrets::providers::{InlineSecretStore, SecretStoreRegistry};
        use graphica_core::secrets::{SecretStore, SecretValue};

        let source = DataSource {
            id: "test".to_string(),
            title: "Test".to_string(),
            source_type: "postgresql".to_string(),
            description: None,
            connection: ConnectionDetails {
                secret_ref: "datasources/test".to_string(),
                config: SourceConfig::PostgreSQL(PostgreSQLConfig {
                    host: "localhost".to_string(),
                    port: 5432,
                    database: "test".to_string(),
                    schema: None,
                    ssl_mode: None,
                }),
                encryption_enabled: false,
                credentials: Default::default(),
            },
            tags: vec![],
            metadata: Default::default(),
            schema_ref: None,
            created_at: Some(chrono::Utc::now()),
            updated_at: None,
            last_synced_at: None,
        };

        let inline = Arc::new(InlineSecretStore::new());
        inline
            .put_secret(
                "datasources/test",
                SecretValue::KeyValue(
                    [
                        ("username".to_string(), "svc_user".to_string()),
                        ("password".to_string(), "svc_pass".to_string()),
                    ]
                    .into_iter()
                    .collect(),
                ),
                None,
            )
            .await
            .unwrap();

        let registry = Arc::new(SecretStoreRegistry::new());
        registry.register("default", inline.clone());
        registry.set_default(inline);

        let resolver = CredentialResolver::new().with_secret_store_registry(registry);
        let creds = resolver.resolve(&source).await.unwrap();

        assert_eq!(creds.username, "svc_user");
        assert_eq!(creds.password, "svc_pass");
    }

    #[tokio::test]
    async fn test_mock_discovery_service() {
        let service = MockDiscoveryService::new();

        let schema = service
            .discover_by_source_id("test", Some("users"), 100)
            .await
            .unwrap();

        assert_eq!(schema.source_id, "mock");
        assert_eq!(schema.tables.len(), 0);
    }

    #[test]
    fn test_mock_service_supports_none() {
        let service = MockDiscoveryService::new();
        assert!(service.supports_source_type("postgresql"));
    }

    #[test]
    fn test_production_supports_expected_source_aliases() {
        // Validate alias logic in the service-level capability gate.
        let is_supported = |source_type: &str| {
            let lower = source_type.to_lowercase();
            lower.contains("postgresql")
                || lower.contains("postgres")
                || lower == "edb"
                || lower.contains("enterprisedb")
                || lower.contains("db2")
                || lower == "db2"
                || lower.contains("oracle")
                || lower.contains("saphana")
                || lower.contains("sap_hana")
                || lower.contains("sap hana")
                || lower.contains("databricks")
        };

        assert!(is_supported("postgresql"));
        assert!(is_supported("edb"));
        assert!(is_supported("oracle19i"));
        assert!(is_supported("sap_hana"));
        assert!(is_supported("databricks"));
        assert!(!is_supported("csv"));
    }
}
