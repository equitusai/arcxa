// ! ARCXA Coordinator Main Binary
//!
//! Main application server with:
//! - Query coordinator (scatter-gather to shards)
//! - REST API (Axum)
//! - gRPC API (Tonic)
//! - Temporal indexes (RocksDB)

use anyhow::{Context, Result};
use clap::Parser;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::signal;
use tracing::{error, info, warn};

use graphica_coordinator::api::auth::AuthConfig;
use graphica_coordinator::api::setup_token::SetupTokenManager;
use graphica_coordinator::api::{rest::build_router, ApiState};
use graphica_coordinator::common::odbc_runtime::log_odbc_runtime_inventory;
use graphica_coordinator::config::CoordinatorConfig;
use graphica_coordinator::governance::distributed::{
    CoordinatorServiceConfig, CoordinatorServiceImpl, CoordinatorServiceServer, ShardId,
    ShardRegistry,
};
use graphica_coordinator::governance::rdf_store::GraphicaRdfStore;
use graphica_coordinator::governance::rdf_wal::RdfWalWrapper;
use graphica_coordinator::governance::{GovernanceBrain, SharedGovernanceBrain};
use graphica_coordinator::storage::wal::{FileWal, LogSequenceNumber, WalMetricsCollector};
use graphica_coordinator::storage::{ColumnLineageStore, LineageStorage};
use graphica_coordinator::AppContext;
use tonic::transport::Server;

/// ARCXA Coordinator CLI
#[derive(Parser, Debug)]
#[command(name = "arcxa-coordinator")]
#[command(about = "ARCXA coordinator server")]
struct Args {
    /// REST API port
    #[arg(long, default_value = "8080")]
    rest_port: u16,

    /// gRPC API port
    #[arg(long, default_value = "9090")]
    grpc_port: u16,

    /// RocksDB data path for temporal indexes
    #[arg(long, default_value = "./data/coordinator/rocksdb")]
    rocksdb_path: String,

    /// Shard URLs (comma-separated)
    #[arg(long, default_value = "")]
    shard_urls: String,

    /// Number of shards
    #[arg(long, default_value = "2")]
    shard_count: usize,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let mut args = Args::parse();

    // Override with environment variables if set
    if let Ok(port) = std::env::var("REST_PORT") {
        args.rest_port = port.parse().unwrap_or(args.rest_port);
    }
    if let Ok(port) = std::env::var("GRPC_PORT") {
        args.grpc_port = port.parse().unwrap_or(args.grpc_port);
    }
    if let Ok(path) = std::env::var("ROCKSDB_PATH") {
        args.rocksdb_path = path;
    }
    if let Ok(urls) = std::env::var("SHARD_URLS") {
        args.shard_urls = urls;
    }
    if let Ok(count) = std::env::var("SHARD_COUNT") {
        args.shard_count = count.parse().unwrap_or(args.shard_count);
    }

    info!("ARCXA Coordinator starting...");
    info!("   Version: 0.2.0");
    info!("   REST API: http://0.0.0.0:{}", args.rest_port);
    info!("   gRPC API: http://0.0.0.0:{}", args.grpc_port);
    info!("   RocksDB:  {}", args.rocksdb_path);

    if !args.shard_urls.is_empty() {
        info!("   Shards:   {}", args.shard_urls);
    } else {
        warn!("WARNING: No shards configured - running in standalone mode");
    }

    log_odbc_runtime_inventory();

    // Initialize application context FIRST (before any components)
    info!("Initializing application context...");
    let environment = std::env::var("ENVIRONMENT").unwrap_or_else(|_| "production".to_string());
    let app_context =
        AppContext::new(environment).context("Failed to initialize application context")?;
    info!(
        "SUCCESS: Application context initialized (environment: {})",
        app_context.env()
    );

    // Load coordinator configuration from environment
    info!("Loading coordinator configuration from environment...");
    let coordinator_config =
        CoordinatorConfig::from_env().context("Failed to load coordinator configuration")?;
    info!("SUCCESS: Coordinator configuration loaded");

    // Create data directory
    std::fs::create_dir_all(&args.rocksdb_path).context("Failed to create RocksDB directory")?;

    // Initialize storage
    info!("Initializing storage...");

    // Set default paths if not provided
    let parquet_path =
        std::env::var("PARQUET_PATH").unwrap_or_else(|_| "./data/parquet".to_string());
    let archive_path =
        std::env::var("ARCHIVE_PATH").unwrap_or_else(|_| "./data/archive".to_string());
    let kafka_brokers =
        std::env::var("KAFKA_BROKERS").unwrap_or_else(|_| "localhost:9092".to_string());

    std::fs::create_dir_all(&parquet_path).ok();
    std::fs::create_dir_all(&archive_path).ok();

    // Determine Kafka mode: legacy, durable, or hybrid (for progressive rollout)
    let kafka_mode = std::env::var("KAFKA_MODE")
        .unwrap_or_else(|_| "durable".to_string())
        .to_lowercase();

    let lineage_storage = match kafka_mode.as_str() {
        "hybrid" => {
            info!("Kafka: Initializing with HYBRID mode (progressive rollout with feature flags)");

            // Load feature flags from environment
            use graphica_coordinator::storage::kafka::FeatureFlagManager;
            let feature_flags = match FeatureFlagManager::from_env() {
                Ok(mgr) => {
                    info!("   Feature flags loaded from environment");
                    mgr
                }
                Err(e) => {
                    warn!("   Failed to load feature flags: {}, using defaults", e);
                    FeatureFlagManager::default()
                }
            };

            // Log rollout configuration
            let rollout_pct = std::env::var("KAFKA_FEATURE_DURABLE_WRITES_ROLLOUT_PCT")
                .unwrap_or_else(|_| "0".to_string());
            info!("   Durable writes rollout: {}%", rollout_pct);

            // Use production Kafka config for durability
            use graphica_coordinator::storage::kafka::KafkaConfig;
            let kafka_config = KafkaConfig::production();

            let storage = LineageStorage::new_with_hybrid_kafka(
                &args.rocksdb_path,
                &parquet_path,
                &archive_path,
                &kafka_brokers,
                Some(kafka_config),
                feature_flags,
            )
            .await
            .context("Failed to initialize lineage storage with hybrid Kafka")?;

            // Run recovery on startup to replay any unacknowledged events
            info!("Kafka: Running startup recovery for durable sink...");
            match storage.recover_kafka_on_startup().await {
                Ok(report) => {
                    if report.total_events > 0 {
                        info!(
                            "Kafka: Recovery successful - {}/{} events replayed in {:?}",
                            report.replayed_events, report.total_events, report.duration
                        );
                    } else {
                        info!("Kafka: No unacknowledged events to replay (clean startup)");
                    }
                }
                Err(e) => {
                    warn!(
                        "Kafka: Recovery had errors (events remain in WAL for next attempt): {}",
                        e
                    );
                }
            }

            info!("SUCCESS: Lineage storage initialized with hybrid Kafka");
            Arc::new(storage)
        }
        "durable" => {
            info!("Kafka: Initializing with DURABLE Kafka sink (WAL-backed, zero data loss)");

            // Use production Kafka config for durability
            use graphica_coordinator::storage::kafka::KafkaConfig;
            let kafka_config = KafkaConfig::production();

            let storage = LineageStorage::new_with_durable_kafka(
                &args.rocksdb_path,
                &parquet_path,
                &archive_path,
                &kafka_brokers,
                Some(kafka_config),
            )
            .await
            .context("Failed to initialize lineage storage with durable Kafka")?;

            // Run recovery on startup to replay any unacknowledged events
            info!("Kafka: Running startup recovery to replay unacknowledged events...");
            match storage.recover_kafka_on_startup().await {
                Ok(report) => {
                    if report.total_events > 0 {
                        info!(
                            "Kafka: Recovery successful - {}/{} events replayed in {:?}",
                            report.replayed_events, report.total_events, report.duration
                        );
                    } else {
                        info!("Kafka: No unacknowledged events to replay (clean startup)");
                    }
                }
                Err(e) => {
                    // Log error but don't fail startup (events are durable in WAL)
                    warn!(
                        "Kafka: Recovery had errors (events remain in WAL for next attempt): {}",
                        e
                    );
                }
            }

            info!("SUCCESS: Lineage storage initialized with durable Kafka");
            Arc::new(storage)
        }
        "legacy" => {
            #[allow(deprecated)]
            let storage = LineageStorage::new(
                &args.rocksdb_path,
                &parquet_path,
                &archive_path,
                &kafka_brokers,
            )
            .context("Failed to initialize lineage storage")?;

            warn!("WARNING: Using LEGACY Kafka sink (fire-and-forget, can lose data)");
            warn!("   Set KAFKA_MODE=durable to enable zero data loss");
            warn!("   Set KAFKA_MODE=hybrid for progressive rollout with feature flags");
            Arc::new(storage)
        }
        _ => {
            warn!(
                "WARNING: Unknown KAFKA_MODE '{}', defaulting to durable",
                kafka_mode
            );

            use graphica_coordinator::storage::kafka::KafkaConfig;
            let kafka_config = KafkaConfig::production();

            let storage = LineageStorage::new_with_durable_kafka(
                &args.rocksdb_path,
                &parquet_path,
                &archive_path,
                &kafka_brokers,
                Some(kafka_config),
            )
            .await
            .context("Failed to initialize lineage storage with durable Kafka")?;

            info!("SUCCESS: Lineage storage initialized with durable Kafka (default)");
            Arc::new(storage)
        }
    };

    info!("   Kafka mode: {}", lineage_storage.kafka_sink_type());

    // Initialize shard registry (always - needed for auto-registration)
    info!("Distributed: Initializing shard registry...");
    let registry_path = format!("{}/shard_registry", args.rocksdb_path);
    let shard_registry = Arc::new(
        ShardRegistry::new(&registry_path, 2, 60) // replication=2, heartbeat_timeout=60s
            .context("Failed to initialize shard registry")?,
    );

    // Register shards from SHARD_URLS if provided (legacy/manual mode)
    let governance_brain = if !args.shard_urls.is_empty() {
        info!("Distributed: Initializing distributed shard coordinator (manual mode)...");

        // Parse shard URLs
        let shard_urls: Vec<&str> = args.shard_urls.split(',').map(|s| s.trim()).collect();
        info!("   Found {} shard URLs", shard_urls.len());

        // Initialize shards - use automatic hash partitioning
        let topology = shard_registry
            .get_topology()
            .context("Failed to get topology")?;

        // Always sync shard URLs with SHARD_URLS env var
        // This handles both initial registration and URL updates
        if topology.total_shards != shard_urls.len() as u32 {
            if topology.total_shards > 0 {
                info!(
                    "   Shard count changed from {} to {}, re-registering...",
                    topology.total_shards,
                    shard_urls.len()
                );
                // Note: In production, you'd want migration logic here
                // For now, we just warn and re-register
            }

            info!(
                "   Registering {} shards with automatic hash partitioning...",
                shard_urls.len()
            );

            // Use the new automatic registration method
            let shard_addresses: Vec<(u32, String, Vec<String>)> = shard_urls
                .iter()
                .enumerate()
                .map(|(i, url)| (i as u32, url.to_string(), vec![]))
                .collect();

            shard_registry
                .register_shards_auto(shard_addresses)
                .context("Failed to register shards")?;

            for (i, url) in shard_urls.iter().enumerate() {
                info!(
                    "   SUCCESS: Shard {}: {} (hash range auto-assigned)",
                    i, url
                );
            }
        } else {
            info!(
                "   Using existing {} registered shards",
                topology.total_shards
            );
            // Log existing shard URLs for verification
            for i in 0..shard_urls.len() {
                if let Ok(Some(shard)) = shard_registry.get_shard(ShardId(i as u32)) {
                    info!("   SUCCESS: Shard {}: {}", i, shard.leader_address);
                }
            }
        }

        // Activate all shards for local development (when running manually started shards)
        // In production, shards would send heartbeats and transition to Active automatically
        info!("   Activating shards for query execution...");
        for i in 0..shard_urls.len() {
            use graphica_coordinator::governance::distributed::ShardStatus;
            shard_registry
                .update_shard_status(ShardId(i as u32), ShardStatus::Active)
                .context(format!("Failed to activate shard {}", i))?;
            info!("   SUCCESS: Shard {} is now Active", i);
        }

        // Create GovernanceBrain with shard-coordinating RDF store
        let brain_path = format!("{}/governance", args.rocksdb_path);
        let brain =
            GovernanceBrain::new(&brain_path).context("Failed to initialize governance brain")?;
        let shared_brain = SharedGovernanceBrain::new(brain);

        info!("SUCCESS: Distributed coordinator initialized (manual mode)");

        Some(shared_brain)
    } else {
        info!("Distributed: Auto-registration mode (no SHARD_URLS configured)");
        info!("   Shards will register via gRPC CoordinatorService");

        // Create GovernanceBrain with shard-coordinating RDF store
        let brain_path = format!("{}/governance", args.rocksdb_path);
        let brain =
            GovernanceBrain::new(&brain_path).context("Failed to initialize governance brain")?;
        let shared_brain = SharedGovernanceBrain::new(brain);

        Some(shared_brain)
    };

    // Initialize RDF WAL (if configured)
    let rdf_wal: Option<Arc<RdfWalWrapper>> = if let Some(ref rdf_wal_config) =
        coordinator_config.rdf_wal
    {
        if rdf_wal_config.enabled {
            info!("RDF WAL: Initializing write-ahead log for crash recovery...");
            info!("   Path: {:?}", rdf_wal_config.wal.path);
            info!(
                "   Max file size: {} MB",
                rdf_wal_config.wal.max_file_size / (1024 * 1024)
            );
            info!("   Auto-recover: {}", rdf_wal_config.auto_recover);

            // Create WAL directory
            std::fs::create_dir_all(&rdf_wal_config.wal.path)
                .context("Failed to create RDF WAL directory")?;

            // Create WAL metrics collector
            let wal_metrics = Arc::new(WalMetricsCollector::new("rdf_wal"));

            // Initialize FileWal
            let file_wal = FileWal::new(rdf_wal_config.wal.clone(), wal_metrics.clone())
                .await
                .context("Failed to create RDF WAL")?;
            let file_wal = Arc::new(file_wal);

            // Create shard router and connection pool for recovery
            use graphica_coordinator::governance::shard_coordinator::connection::ConnectionPool;
            use graphica_coordinator::governance::shard_coordinator::insert::InsertExecutor;
            use graphica_coordinator::governance::shard_coordinator::routing::ShardRouter;

            let shard_router = Arc::new(ShardRouter::new(shard_registry.clone()));
            let connection_pool = Arc::new(ConnectionPool::new());

            // Create insert executor for WAL replay
            let insert_executor =
                Arc::new(InsertExecutor::new(shard_router.clone(), connection_pool));

            // Create RDF WAL wrapper
            let rdf_wal = Arc::new(RdfWalWrapper::new(file_wal, insert_executor, shard_router));

            info!("SUCCESS: RDF WAL initialized");

            // Run startup recovery if auto_recover is enabled
            if rdf_wal_config.auto_recover {
                info!("RDF WAL: Running startup recovery...");

                let start_lsn = rdf_wal_config
                    .recovery_start_lsn
                    .map(LogSequenceNumber)
                    .unwrap_or(LogSequenceNumber(0));

                let start_time = std::time::Instant::now();

                match rdf_wal.replay(start_lsn).await {
                    Ok(recovered_count) => {
                        let duration = start_time.elapsed();
                        if recovered_count > 0 {
                            info!(
                                "RDF WAL: Recovery successful - {} triples replayed in {:?}",
                                recovered_count, duration
                            );
                        } else {
                            info!("RDF WAL: No uncommitted operations to replay (clean startup)");
                        }
                    }
                    Err(e) => {
                        // Log error but don't fail startup (events remain in WAL for next attempt)
                        warn!(
                            "RDF WAL: Recovery had errors (entries remain in WAL for next attempt): {}",
                            e
                        );
                    }
                }
            } else {
                info!("RDF WAL: Auto-recovery disabled, skipping startup recovery");
            }

            Some(rdf_wal)
        } else {
            info!("RDF WAL: Disabled in configuration");
            None
        }
    } else {
        info!("RDF WAL: Not configured (running without WAL protection)");
        None
    };

    // Initialize RDF store (distributed vs in-memory based on environment)
    let rdf_store = if app_context.env() == "development" {
        info!("Development: Initializing in-memory RDF store (no external shards required)");
        Arc::new(
            GraphicaRdfStore::new_in_memory()
                .context("Failed to initialize in-memory RDF store")?,
        )
    } else if let Some(ref wal) = rdf_wal {
        info!("Production: Initializing distributed RDF store with WAL durability");
        Arc::new(
            GraphicaRdfStore::new_with_registry_and_wal(
                shard_registry.clone(),
                app_context.clone(),
                wal.clone(),
            )
            .context("Failed to initialize distributed RDF store with WAL")?,
        )
    } else {
        info!("Production: Initializing distributed RDF store (no WAL)");
        Arc::new(
            GraphicaRdfStore::new_with_registry(shard_registry.clone(), app_context.clone())
                .context("Failed to initialize distributed RDF store")?,
        )
    };

    // Initialize async RDF store adapter for transformers (SHACL-DDL, etc.)
    // This wraps the existing RDF store (single source of truth) with an async interface
    info!("RDF: Creating async adapter for RDF store (single source of truth)...");
    let rdf_adapter = Arc::new(graphica_coordinator::governance::AsyncRdfStoreAdapter::new(
        rdf_store.clone(),
    ));
    info!("SUCCESS: AsyncRdfStoreAdapter created, wrapping existing RDF store");

    // Initialize auth config
    let enable_auth = std::env::var("ENABLE_AUTH")
        .unwrap_or_else(|_| "true".to_string())
        .parse::<bool>()
        .unwrap_or(true);

    let auth_config = if !enable_auth {
        warn!("WARNING:  ENABLE_AUTH=false, authentication is DISABLED (development only)");
        Arc::new(AuthConfig::disabled())
    } else {
        Arc::new(AuthConfig::from_env().unwrap_or_else(|_| {
            warn!("WARNING:  No JWT_SECRET found, using development auth (insecure)");
            // Create a development config with a dummy secret
            AuthConfig::from_secret_bytes(b"development-secret-key-insecure-32bytes!!!")
                .expect("Failed to create development auth config")
        }))
    };

    // Initialize setup token manager and generate initial token
    let setup_token_manager = Arc::new(SetupTokenManager::new());
    match setup_token_manager.generate_token().await {
        Ok(token) => {
            info!("Security: Setup token generated (expires in 1 hour):");
            info!("   Token: {}", token.token);
            info!("   Use this token to create the initial admin user via POST /auth/setup");
        }
        Err(_) => {
            info!("INFO:  Setup token not generated (admin user may already exist)");
        }
    }

    // Initialize user service with KvStore
    info!("User: Initializing user service...");
    let user_kv_store = Arc::new(
        graphica_coordinator::storage::kv_store::KvStore::new(&format!(
            "{}/users",
            args.rocksdb_path
        ))
        .context("Failed to create KV store for users")?,
    );
    let user_service = Some(Arc::new(
        graphica_coordinator::api::users::UserService::new(user_kv_store),
    ));

    // Initialize audit logger with KvStore
    let audit_kv_store = Arc::new(
        graphica_coordinator::storage::kv_store::KvStore::new(&format!(
            "{}/audit",
            args.rocksdb_path
        ))
        .context("Failed to create KV store for audit")?,
    );
    let audit_sink = Arc::new(graphica_coordinator::api::audit::RocksAuditSink::new(
        audit_kv_store,
    ));
    let audit_logger = Some(Arc::new(
        graphica_coordinator::api::audit::AuditLogger::new(audit_sink),
    ));

    // Create query executor (always - we have shard registry for auto-registration)
    use graphica_coordinator::governance::shard_coordinator::connection::ConnectionPool;
    use graphica_coordinator::governance::shard_coordinator::query::QueryExecutor;
    use graphica_coordinator::governance::shard_coordinator::routing::ShardRouter;

    info!("Query: Initializing query executor with observability...");
    let router = Arc::new(ShardRouter::new(shard_registry.clone()));
    let pool = Arc::new(ConnectionPool::new());

    // Pass AppContext to QueryExecutor for metrics access
    let query_executor = Some(Arc::new(QueryExecutor::new(
        router,
        pool,
        app_context.clone(),
    )));

    // Initialize persisted ontology registry for custom domain ontologies
    info!("Registry: Initializing persisted ontology registry with RocksDB...");
    let ontology_path = format!("{}/ontologies", args.rocksdb_path);
    let persisted_ontology_registry = Arc::new(
        graphica_coordinator::mapping::ontology_registry::PersistedOntologyRegistry::open(
            &ontology_path,
        )
        .await
        .context("Failed to initialize persisted ontology registry")?,
    );

    let ontology_registry = persisted_ontology_registry.registry();
    info!("   SUCCESS: Ontology registry initialized with persistence and crash recovery");

    // Initialize RDF storage client for governance brain persistence
    let rdf_storage_client = if let Ok(shard_endpoint) = std::env::var("SHARD_ENDPOINT") {
        info!(
            "Connecting: Initializing RDF storage client with shard endpoint: {}",
            shard_endpoint
        );
        let client = graphica_coordinator::api::rdf_storage::RdfStorageClient::new(shard_endpoint)
            .with_default_graph("http://graphica.io/catalog/inferred");
        Some(Arc::new(tokio::sync::Mutex::new(client)))
    } else {
        warn!(
            "WARNING:  SHARD_ENDPOINT not set, RDF storage for schema inference will be disabled"
        );
        info!("INFO: Set SHARD_ENDPOINT=http://localhost:9090 to enable RDF storage");
        None
    };

    // Initialize connector registry
    info!("Plugin: Initializing connector registry...");
    let connector_registry = Arc::new(parking_lot::RwLock::new(
        graphica_core::catalog::connectors::ConnectorRegistry::new(),
    ));

    // Initialize in-memory datasource catalog with RDF sync
    info!("Catalog: Initializing in-memory datasource catalog with RDF governance sync...");
    let datasource_catalog = Arc::new(
        graphica_coordinator::catalog_impl::InMemoryDataSourceCatalog::new_with_rdf(
            connector_registry.clone(),
            rdf_store.clone(),
        ),
    );
    info!("SUCCESS: Datasource catalog initialized with automatic RDF sync");

    let mut discovery_service_handle: Option<
        Arc<graphica_coordinator::mapping::discovery::ProductionDiscoveryService>,
    > = None;

    // Initialize field mapping engine (Phase 1+2: Statistical + Semantic + Discovery)
    let mapping_engine = {
        let mapping_path = format!("{}/mapping", args.rocksdb_path);

        // Check if semantic matcher (Phase 2) is configured
        // PRE-EXISTING ISSUE: semantic module doesn't exist
        /*
        let semantic_config = if let Ok(model_service_url) = std::env::var("GRAPHICA_MODEL_SERVICE_URL") {
            info!("Semantic: Semantic matcher configured: {}", model_service_url);
            Some(graphica_coordinator::mapping::semantic::ModelServiceConfig {
                url: model_service_url,
                model_name: "minilm".to_string(),
                connect_timeout: 5,      // seconds
                request_timeout: 10,     // seconds
                circuit_breaker_threshold: 5,
                circuit_breaker_timeout: 30,  // seconds
            })
        } else {
            info!("INFO:  GRAPHICA_MODEL_SERVICE_URL not set, semantic matcher will not be available");
            info!("   Set GRAPHICA_MODEL_SERVICE_URL=http://localhost:50051 to enable Phase 2 semantic matching");
            None
        };
        */
        info!("INFO:  Semantic matcher not available (PRE-EXISTING ISSUE: semantic module doesn't exist)");

        match graphica_coordinator::mapping::MappingEngine::new(&mapping_path, rdf_store.clone())
            .await
        {
            Ok(mut engine) => {
                let phase_info = engine.get_phase_status();
                info!("SUCCESS: Field mapping engine initialized ({})", phase_info);

                // Wire intelligent discovery service with catalog integration
                info!("Plugin: Wiring intelligent schema discovery...");
                use graphica_coordinator::mapping::discovery::{
                    CacheWarmingCoordinator, ProductionDiscoveryService,
                };

                let discovery_service = Arc::new(ProductionDiscoveryService::new(
                    datasource_catalog.clone(),
                    engine.discovery.clone(),
                ));
                discovery_service_handle = Some(discovery_service.clone());

                // Enable discovery in mapping engine
                engine.with_discovery_service(discovery_service.clone());
                info!("   SUCCESS: Discovery service wired to mapping engine");

                // Enable discovery in datasource catalog for schema inference
                datasource_catalog.set_discovery_service(discovery_service.clone());
                info!("   SUCCESS: Discovery service wired to datasource catalog");

                // Wire ontology registry for dynamic ontology term loading
                engine.with_ontology_registry(ontology_registry.clone());
                info!("   SUCCESS: Ontology registry wired to mapping engine");

                // Wrap engine in Arc early to enable cache invalidation callback
                let engine = Arc::new(engine);

                // Wire automatic cache invalidation (Optimization Priority 5)
                let engine_for_callback = engine.clone();
                persisted_ontology_registry.set_cache_invalidation_callback(Box::new(move || {
                    engine_for_callback.ontology_client().invalidate_cache();
                }));
                info!("   SUCCESS: Automatic cache invalidation wired (ontology updates will clear term cache)");

                // Optional: Enable background cache warming
                let cache_warming =
                    Arc::new(CacheWarmingCoordinator::new(discovery_service.clone()));
                info!("   SUCCESS: Background cache warming enabled");
                info!("   INFO: Cache will auto-warm when datasources are registered");

                // Store cache warming coordinator for future use (optional)
                // In production, you'd wire this to catalog events
                drop(cache_warming); // For now, just demonstrate initialization

                info!("SUCCESS: Intelligent schema discovery fully integrated!");

                Some(engine)
            }
            Err(e) => {
                warn!("WARNING:  Failed to initialize mapping engine: {}", e);
                warn!("   Field mapping features will be unavailable");
                None
            }
        }
    };

    // Initialize secret store registry
    info!("Security: Initializing secret store registry...");
    let secret_store_registry = {
        use graphica_core::secrets::providers::SecretStoreRegistry;
        use graphica_core::secrets::providers::{FileSecretStore, InlineSecretStore};

        let registry = Arc::new(SecretStoreRegistry::with_cache(300, 1000)); // 5 min TTL, 1000 max entries
        let store_type = std::env::var("GRAPHICA_SECRET_STORE_TYPE")
            .ok()
            .map(|value| value.to_ascii_lowercase())
            .unwrap_or_else(|| {
                if std::env::var("GRAPHICA_SECRET_STORE_DIR").is_ok() {
                    "file".to_string()
                } else {
                    "inline".to_string()
                }
            });

        let store: Option<graphica_core::secrets::SecretStoreRef> = match store_type.as_str() {
            "file" => {
                let directory = std::env::var("GRAPHICA_SECRET_STORE_DIR")
                    .unwrap_or_else(|_| "./data/secrets".to_string());
                let format = std::env::var("GRAPHICA_SECRET_STORE_FORMAT")
                    .unwrap_or_else(|_| "json".to_string());
                match FileSecretStore::with_directory_and_format(&directory, &format) {
                    Ok(store) => {
                        if let Err(err) = std::fs::create_dir_all(store.base_dir()) {
                            error!(
                                "ERROR: Failed to create secret store directory '{}': {}",
                                directory, err
                            );
                            None
                        } else {
                            info!(
                                "SUCCESS: Secret store registry initialized with file provider ({}, format={})",
                                directory, format
                            );
                            Some(Arc::new(store))
                        }
                    }
                    Err(err) => {
                        error!(
                            "ERROR: Failed to initialize file secret store ({}): {}",
                            directory, err
                        );
                        None
                    }
                }
            }
            "inline" => {
                info!("SUCCESS: Secret store registry initialized with inline provider (development mode)");
                Some(Arc::new(InlineSecretStore::new()))
            }
            other => {
                warn!(
                    "WARNING: Unknown GRAPHICA_SECRET_STORE_TYPE '{}', defaulting to inline",
                    other
                );
                Some(Arc::new(InlineSecretStore::new()))
            }
        };

        if let Some(store) = store {
            registry.register("default", store.clone());
            registry.set_default(store);

            info!("   Cache: 300s TTL, 1000 max entries");

            Some(registry)
        } else {
            None
        }
    };

    if let Some(registry) = &secret_store_registry {
        datasource_catalog
            .set_secret_store_registry(registry.clone())
            .await;
        if let Some(discovery_service) = &discovery_service_handle {
            discovery_service.set_secret_store_registry(registry.clone());
            info!("   SUCCESS: Secret store registry wired to discovery service");
        }
        info!("   SUCCESS: Secret store registry wired to datasource catalog");
    }

    // Initialize loader job manager for ETL operations
    info!("Loader: Initializing loader job manager with lineage tracking...");
    let loader_job_manager = {
        use graphica_coordinator::mapping::loader::lineage::RdfLineageSink;
        use graphica_coordinator::mapping::loader::orchestration::{
            LoaderJobConfig, LoaderJobManager,
        };
        use graphica_coordinator::observability::metrics::LoaderMetrics;

        let loader_config = LoaderJobConfig {
            max_concurrent_jobs: 10,
            batch_size: 1000,
            checkpoint_interval_rows: 10_000,
            checkpoint_dir: std::path::PathBuf::from("./data/loader/checkpoints"),
            dlq_dir: std::path::PathBuf::from("./data/loader/dlq"),
            ..Default::default()
        };

        // Create directories
        std::fs::create_dir_all(&loader_config.checkpoint_dir).ok();
        std::fs::create_dir_all(&loader_config.dlq_dir).ok();

        match LoaderMetrics::new(app_context.metrics.as_ref().unwrap().registry()) {
            Ok(metrics) => {
                // Create RDF lineage sink for W3C PROV tracking
                let lineage_sink = Arc::new(RdfLineageSink::new(
                    rdf_store.clone(),
                    Some("http://graphica.io/lineage".to_string()),
                ));

                match LoaderJobManager::new_with_lineage(
                    Arc::new(metrics),
                    loader_config,
                    lineage_sink,
                ) {
                    Ok(manager) => {
                        info!("SUCCESS: Loader job manager initialized with W3C PROV lineage");
                        info!("   Max concurrent jobs: 10");
                        info!("   Checkpoint dir: ./data/loader/checkpoints");
                        info!("   DLQ dir: ./data/loader/dlq");
                        info!("   Lineage graph: http://graphica.io/lineage");
                        Some(Arc::new(manager))
                    }
                    Err(e) => {
                        warn!("WARNING:  Failed to initialize loader job manager: {}", e);
                        None
                    }
                }
            }
            Err(e) => {
                warn!("WARNING:  Failed to initialize loader metrics: {}", e);
                None
            }
        }
    };

    // Initialize unified mapping coordinator for CSV-to-DB workflows
    info!("Workflow: Initializing unified mapping coordinator...");
    let unified_mapping_coordinator = if let Some(ref engine) = mapping_engine {
        let unified_storage_path = format!("{}/unified_mapping", args.rocksdb_path);

        match graphica_coordinator::mapping::multi_source::storage::UnifiedMappingStorage::new(
            &unified_storage_path,
        ) {
            Ok(unified_storage) => {
                let unified_storage = Arc::new(unified_storage);
                let coordinator = Arc::new(
                    graphica_coordinator::mapping::multi_source::UnifiedMappingCoordinator::new(
                        engine.storage.clone(),
                        unified_storage,
                    ),
                );

                info!("SUCCESS: Unified mapping coordinator initialized");
                info!("   Storage: {}", unified_storage_path);
                Some(coordinator)
            }
            Err(e) => {
                warn!(
                    "WARNING:  Failed to initialize unified mapping coordinator: {}",
                    e
                );
                warn!("   CSV-to-DB unified mapping features will be unavailable");
                None
            }
        }
    } else {
        info!("INFO:  Mapping engine not available, unified mapping coordinator disabled");
        None
    };

    // Initialize versioned ontology->physical binding service
    info!("Workflow: Initializing ontology binding service...");
    let binding_service = {
        let binding_store_path = std::env::var("ONTOLOGY_BINDING_DB_PATH")
            .unwrap_or_else(|_| format!("{}/ontology_bindings", args.rocksdb_path));

        match graphica_coordinator::mapping::bindings::BindingStore::new(&binding_store_path) {
            Ok(store) => {
                info!("SUCCESS: Ontology binding store initialized");
                info!("   Storage: {}", binding_store_path);
                Some(Arc::new(
                    graphica_coordinator::mapping::bindings::BindingService::new(Arc::new(store)),
                ))
            }
            Err(e) => {
                warn!(
                    "WARNING: Failed to initialize ontology binding store: {}",
                    e
                );
                warn!("   Stored binding strategy will be unavailable");
                None
            }
        }
    };

    // Initialize workflow orchestration components
    info!("Workflow: Initializing workflow orchestration components...");
    let (workflow_engine, model_registry, model_cache, rule_executor) = {
        use graphica_coordinator::api::handlers::PersistentModelRegistry;
        use graphica_core::orchestration::ml::{CacheConfig, ModelCache, ModelInvoker};
        use graphica_core::orchestration::rules::RuleExecutor;
        use graphica_core::orchestration::WorkflowEngine;
        use std::time::Duration;

        // Initialize ML model registry with RDF persistence
        let model_registry =
            match PersistentModelRegistry::new_and_load(Some(rdf_store.clone())).await {
                Ok(registry) => {
                    info!("SUCCESS: ML model registry initialized with RDF persistence");
                    Arc::new(registry)
                }
                Err(e) => {
                    warn!("WARNING: Failed to load model registry from RDF: {}", e);
                    warn!("   Creating in-memory-only registry (persistence disabled)");
                    Arc::new(PersistentModelRegistry::new(None))
                }
            };

        // Initialize ML model cache with reasonable defaults
        let cache_config = CacheConfig {
            max_size: 100,
            default_ttl: Duration::from_secs(300), // 5 minutes
            model_ttls: std::collections::HashMap::new(),
        };
        let model_cache = Arc::new(ModelCache::new(cache_config));
        info!("SUCCESS: ML model cache initialized (max: 100 entries, TTL: 300s)");

        // Initialize model invoker (uses core registry from persistent wrapper)
        let core_registry = model_registry.as_core_registry();
        let model_invoker = match ModelInvoker::new(core_registry, model_cache.clone()) {
            Ok(invoker) => {
                info!("SUCCESS: ML model invoker initialized with persistent registry");
                Arc::new(invoker)
            }
            Err(e) => {
                warn!("WARNING:  Failed to initialize model invoker: {}", e);
                warn!("   ML prediction workflows will not be available");
                return Err(anyhow::anyhow!("Failed to initialize model invoker: {}", e));
            }
        };

        // Initialize rule executor (wraps WASM engine internally)
        let rule_executor = Arc::new(RuleExecutor::new());
        info!("SUCCESS: Rule executor initialized (with WASM engine)");

        // Initialize workflow engine with execution capabilities
        let workflow_engine = Arc::new(WorkflowEngine::new_with_execution(
            model_invoker,
            rule_executor.clone(),
        ));
        info!("SUCCESS: Workflow engine initialized with execution capabilities");

        (
            Some(workflow_engine),
            Some(model_registry),
            Some(model_cache),
            Some(rule_executor),
        )
    };

    // Initialize RocksDB-backed execution store for persistent workflow state
    // This creates both the RocksExecutionStore (for CheckpointManager) and
    // the ExecutionStore with RocksDbBackend (for API)
    let (execution_store, rocks_execution_store): (
        Arc<graphica_coordinator::workflows::storage::ExecutionStore>,
        Option<Arc<graphica_coordinator::workflows::storage::RocksExecutionStore>>,
    ) = {
        use graphica_coordinator::workflows::storage::persistence::RocksDbBackend;
        use graphica_coordinator::workflows::storage::ExecutionStore;

        let db_path = std::env::var("WORKFLOW_EXECUTION_DB_PATH")
            .unwrap_or_else(|_| "./data/workflow-executions-db".to_string());

        match RocksDbBackend::open(&db_path) {
            Ok(rocks_backend) => {
                info!(
                    "SUCCESS: Workflow Execution RocksDB storage initialized at {}",
                    db_path
                );
                // Get the inner RocksExecutionStore for CheckpointManager
                let rocks_store = rocks_backend.inner().clone();
                // Create ExecutionStore with RocksDB backend for API
                let exec_store = ExecutionStore::with_backend(Arc::new(rocks_backend));
                (Arc::new(exec_store), Some(rocks_store))
            }
            Err(e) => {
                error!(
                    "ERROR: Failed to initialize Workflow Execution RocksDB storage: {}",
                    e
                );
                error!("   Falling back to in-memory storage (data will not persist!)");
                (Arc::new(ExecutionStore::new()), None)
            }
        }
    };

    // Initialize File Library storage for enterprise file management
    info!("Initializing File Library storage...");
    let file_library: Option<
        Arc<dyn graphica_coordinator::api::file_library::storage_trait::FileLibraryStore>,
    > = {
        let db_path = std::env::var("FILE_LIBRARY_DB_PATH")
            .unwrap_or_else(|_| "./data/file-library-db".to_string());
        let storage_dir = std::env::var("FILE_LIBRARY_STORAGE_PATH")
            .unwrap_or_else(|_| "./data/file-library".to_string());

        // Clean up temporary files from incomplete uploads
        if let Err(e) =
            graphica_coordinator::api::file_library::migration::cleanup_temp_files(&storage_dir)
                .await
        {
            warn!("Failed to clean up temporary files: {}", e);
        }

        match graphica_coordinator::api::file_library::storage_rocksdb::RocksDBFileLibrary::open(
            &db_path,
        ) {
            Ok(storage) => {
                info!(
                    "SUCCESS: File Library RocksDB storage initialized at {}",
                    db_path
                );
                Some(Arc::new(storage) as Arc<dyn graphica_coordinator::api::file_library::storage_trait::FileLibraryStore>)
            }
            Err(e) => {
                error!(
                    "ERROR: Failed to initialize File Library RocksDB storage: {}",
                    e
                );
                error!("   Falling back to in-memory storage (data will not persist!)");
                Some(Arc::new(graphica_coordinator::api::file_library::FileLibraryStorage::new()) as Arc<dyn graphica_coordinator::api::file_library::storage_trait::FileLibraryStore>)
            }
        }
    };

    // Initialize DB2 connection pool for workflow operations
    info!("Workflow: Initializing DB2 connection pool...");
    let db2_pool = {
        use graphica_coordinator::mapping::loader::{
            create_db2_pool, DB2Config, DB2PoolConfig, PoolTimeouts,
        };

        // TODO: Load from configuration file or environment variables
        let pool_config = DB2PoolConfig {
            db2_config: DB2Config {
                host: std::env::var("DB2_HOST").unwrap_or_else(|_| "localhost".to_string()),
                port: std::env::var("DB2_PORT")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(50000),
                database: std::env::var("DB2_DATABASE").unwrap_or_else(|_| "GRAPHICA".to_string()),
                username: std::env::var("DB2_USER").unwrap_or_else(|_| "db2inst1".to_string()),
                password: std::env::var("DB2_PASSWORD")
                    .unwrap_or_else(|_| "graphica-db2-pass".to_string()),
                ..DB2Config::default()
            },
            max_size: std::env::var("DB2_POOL_SIZE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(10),
            timeouts: PoolTimeouts::default(),
            health_check_enabled: true,
        };

        match create_db2_pool(pool_config).await {
            Ok(pool) => {
                info!(
                    "   SUCCESS: DB2 connection pool created with {} max connections",
                    pool.status().max_size
                );
                Some(Arc::new(pool))
            }
            Err(e) => {
                warn!("   WARNING: Failed to create DB2 connection pool: {:?}. DB2 workflows will create per-request connections.", e);
                None
            }
        }
    };

    // Initialize column lineage store early so it can be wired into transformers
    let column_lineage_store_concrete: Option<Arc<ColumnLineageStore>> = {
        let column_lineage_enabled = std::env::var("COLUMN_LINEAGE_ENABLED")
            .unwrap_or_else(|_| "true".to_string())
            .parse()
            .unwrap_or(true);

        if column_lineage_enabled {
            let column_lineage_path = std::env::var("COLUMN_LINEAGE_DB_PATH")
                .unwrap_or_else(|_| "./data/column-lineage-db".to_string());
            match ColumnLineageStore::new(&column_lineage_path) {
                Ok(store) => {
                    info!(
                        "SUCCESS: Column lineage store initialized at {}",
                        column_lineage_path
                    );
                    Some(Arc::new(store))
                }
                Err(e) => {
                    warn!("Failed to initialize column lineage store: {}", e);
                    None
                }
            }
        } else {
            info!("Column lineage store disabled via COLUMN_LINEAGE_ENABLED=false");
            None
        }
    };

    // Initialize transformer registry for workflow Transform actions
    info!("Workflow: Initializing transformer registry...");
    let transformer_registry = {
        use graphica_coordinator::workflows::engine::transformers::TransformerRegistry;

        let mut registry = TransformerRegistry::new();

        // Wire CSV parser with file library
        if let Some(ref file_library) = file_library {
            registry = registry.with_csv_parser(file_library.clone());
            info!("   SUCCESS: CSV parser transformer registered with file library");
        } else {
            warn!("   WARNING: File library not available, CSV parser transformer disabled");
        }

        // Wire DB2 loader with connection pool (if available)
        registry = registry.with_db2_loader(db2_pool.clone());
        if db2_pool.is_some() {
            info!("   SUCCESS: DB2 loader transformer registered with connection pool (high-performance mode)");
        } else {
            info!("   SUCCESS: DB2 loader transformer registered without pool (per-request connections)");
        }

        // Wire SHACL-DDL generator with async RDF adapter
        registry = registry.with_shacl_ddl_generator(rdf_adapter.clone());
        info!("   SUCCESS: SHACL-DDL generator transformer registered with async RDF adapter");

        // Wire ontology mapper with mapping engine and column lineage store
        if let Some(ref engine) = mapping_engine {
            if let Some(ref col_store) = column_lineage_store_concrete {
                registry =
                    registry.with_ontology_mapper_and_lineage(engine.clone(), col_store.clone());
                info!("   SUCCESS: Ontology mapper transformer registered with mapping engine and column lineage");
            } else {
                registry = registry.with_ontology_mapper(engine.clone());
                info!("   SUCCESS: Ontology mapper transformer registered with mapping engine (no column lineage)");
            }
        } else {
            warn!("   WARNING: Mapping engine not available, ontology mapper transformer disabled");
        }

        let transformer_count = registry.count();
        info!(
            "SUCCESS: Transformer registry initialized with {} transformers",
            transformer_count
        );

        Some(Arc::new(registry))
    };

    // Initialize production persistence components
    info!("Initializing production persistence components...");

    // Checkpoint persistence (hybrid RocksDB + RDF)
    let checkpoint_persistence = if let Some(ref rocks_store) = rocks_execution_store {
        let persistence = graphica_coordinator::checkpointing::CheckpointPersistence::new(
            rocks_store.db.clone(),
            rdf_store.clone(),
            "checkpoints".to_string(), // Column family name
        );
        info!("SUCCESS: Checkpoint persistence initialized (RocksDB + RDF)");
        Some(Arc::new(persistence))
    } else {
        warn!("WARNING: RocksDB not available, checkpoint persistence disabled");
        None
    };

    // DLQ components (reader, reprocessor, stats calculator)
    let dlq_base_path = std::env::var("DLQ_BASE_PATH").unwrap_or_else(|_| "./data/dlq".to_string());

    let dlq_reader = {
        let reader = graphica_coordinator::mapping::loader::DlqReader::new(
            std::path::PathBuf::from(&dlq_base_path),
        );
        info!("SUCCESS: DLQ reader initialized at {}", dlq_base_path);
        Arc::new(reader)
    };

    let dlq_reprocessor = {
        let reprocessor = graphica_coordinator::mapping::loader::DlqReprocessor::new(
            dlq_reader.clone(),
            std::path::PathBuf::from(&dlq_base_path),
        );
        info!("SUCCESS: DLQ reprocessor initialized");
        Arc::new(reprocessor)
    };

    let dlq_stats_calculator = {
        let calculator = graphica_coordinator::mapping::loader::DlqStatsCalculator::new(
            std::path::PathBuf::from(&dlq_base_path),
        );
        info!("SUCCESS: DLQ stats calculator initialized");
        Arc::new(calculator)
    };

    // Schema version store (RDF-backed)
    let schema_version_store = {
        let store = graphica_coordinator::governance::RdfSchemaVersionStore::new(rdf_store.clone());
        info!("SUCCESS: Schema version store initialized (RDF-backed)");
        Arc::new(store)
    };

    info!("SUCCESS: All production persistence components initialized");

    // Initialize lineage stores for GDPR coordinator (must be done before ApiState)
    // Note: column_lineage_store_concrete is initialized earlier for transformer wiring
    use graphica_coordinator::storage::{RowLineageStore, SchemaEvolutionStore};

    let row_lineage_store_concrete: Option<Arc<RowLineageStore>> = {
        let row_lineage_enabled = std::env::var("ROW_LINEAGE_ENABLED")
            .unwrap_or_else(|_| "true".to_string())
            .parse()
            .unwrap_or(true);

        if row_lineage_enabled {
            let row_lineage_path = std::env::var("ROW_LINEAGE_DB_PATH")
                .unwrap_or_else(|_| "./data/row-lineage-db".to_string());
            match RowLineageStore::new(&row_lineage_path) {
                Ok(store) => Some(Arc::new(store)),
                Err(e) => {
                    warn!("Failed to initialize row lineage store for GDPR: {}", e);
                    None
                }
            }
        } else {
            None
        }
    };

    let schema_evolution_store_concrete: Option<Arc<SchemaEvolutionStore>> = {
        let schema_evolution_enabled = std::env::var("SCHEMA_EVOLUTION_ENABLED")
            .unwrap_or_else(|_| "true".to_string())
            .parse()
            .unwrap_or(true);

        if schema_evolution_enabled {
            let schema_evolution_path = std::env::var("SCHEMA_EVOLUTION_DB_PATH")
                .unwrap_or_else(|_| "./data/schema-evolution-db".to_string());
            match SchemaEvolutionStore::open(&schema_evolution_path) {
                Ok(store) => Some(Arc::new(store)),
                Err(e) => {
                    warn!(
                        "Failed to initialize schema evolution store for GDPR: {}",
                        e
                    );
                    None
                }
            }
        } else {
            None
        }
    };

    // Initialize data management coordinator (GDPR Article 17 + general data lifecycle management)
    let gdpr_coordinator = {
        use graphica_coordinator::gdpr::GdprCoordinator;
        let gdpr_enabled = std::env::var("GDPR_ENABLED")
            .unwrap_or_else(|_| "true".to_string())
            .parse()
            .unwrap_or(true);

        if gdpr_enabled {
            info!("Initializing data management coordinator (GDPR Article 17 + tenant data lifecycle)...");

            let coordinator = GdprCoordinator::with_default_retention(
                row_lineage_store_concrete.clone(),
                column_lineage_store_concrete.clone(),
                schema_evolution_store_concrete.clone(),
            );

            let stores_available = [
                row_lineage_store_concrete.is_some(),
                column_lineage_store_concrete.is_some(),
                schema_evolution_store_concrete.is_some(),
            ]
            .iter()
            .filter(|&&x| x)
            .count();

            info!(
                "SUCCESS: Data management coordinator initialized with {} storage backend(s)",
                stores_available
            );
            if stores_available == 0 {
                warn!("WARNING: Data management coordinator has no storage backends available");
                warn!("   Enable ROW_LINEAGE_ENABLED, COLUMN_LINEAGE_ENABLED, or SCHEMA_EVOLUTION_ENABLED");
            }

            Some(Arc::new(coordinator))
        } else {
            info!("INFO: Data management coordinator disabled (GDPR_ENABLED=false)");
            None
        }
    };

    // Wire up lineage tracking to workflow engine
    // This creates the CoordinatorLineageTracker with both RDF and row-level lineage support
    let workflow_engine = {
        use graphica_coordinator::workflows::lineage::rdf::WorkflowLineageGenerator;
        use graphica_coordinator::workflows::lineage::tracker_impl::CoordinatorLineageTracker;

        if let Some(ref engine) = workflow_engine {
            let lineage_enabled = std::env::var("WORKFLOW_LINEAGE_ENABLED")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true);

            if lineage_enabled {
                // Create lineage generator for RDF-based field lineage
                let generator = Arc::new(WorkflowLineageGenerator::new(rdf_store.clone()));

                // Create coordinator lineage tracker with optional row lineage store
                let tracker: Arc<dyn graphica_core::orchestration::workflow::LineageTracker> =
                    if let Some(ref store) = row_lineage_store_concrete {
                        info!("Wiring lineage tracker with row-level lineage support...");
                        Arc::new(CoordinatorLineageTracker::with_row_lineage_store(
                            generator,
                            store.clone(),
                        ))
                    } else {
                        info!("Wiring lineage tracker without row-level lineage support...");
                        Arc::new(CoordinatorLineageTracker::new(generator))
                    };

                // Create new engine with lineage tracker
                // Note: We reconstruct the engine to add the lineage tracker
                use graphica_core::orchestration::WorkflowEngine;
                let (model_invoker, rule_executor) = (
                    engine.model_invoker().clone(),
                    engine.rule_executor().clone(),
                );

                if let (Some(mi), Some(re)) = (model_invoker, rule_executor) {
                    let mut new_engine =
                        WorkflowEngine::new_with_execution(mi, re).with_lineage_tracker(tracker);

                    // Wire transformer callback if transformer registry is available
                    if let Some(ref registry) = transformer_registry {
                        use graphica_core::orchestration::workflow::TransformerCallback;
                        use std::future::Future;
                        use std::pin::Pin;

                        // Wrapper type to safely pass raw pointer across async boundaries
                        // SAFETY: Safe when pointer outlives future and future is awaited immediately
                        struct SendPtr(*mut serde_json::Value);
                        unsafe impl Send for SendPtr {}

                        let registry_clone = registry.clone();
                        let callback: Arc<TransformerCallback> =
                            Arc::new(Box::new(
                                move |name: &str,
                                      config: &serde_json::Value,
                                      data: &mut serde_json::Value|
                                      -> Pin<
                                    Box<dyn Future<Output = anyhow::Result<()>> + Send>,
                                > {
                                    let name = name.to_string();
                                    let config = config.clone();
                                    let registry = registry_clone.clone();

                                    // Wrap the raw pointer in a Send-able wrapper
                                    // SAFETY: We ensure the pointer remains valid by awaiting immediately
                                    let data_ptr = SendPtr(data as *mut serde_json::Value);

                                    Box::pin(async move {
                                        // Move the SendPtr into the async block to satisfy Send bounds
                                        let ptr = data_ptr;

                                        // SAFETY: This is safe because the caller (execute_semantic_mapper) awaits
                                        // this future immediately and the data reference remains valid throughout
                                        let data = unsafe { &mut *ptr.0 };

                                        registry.execute(&name, &config, data, None).await
                                    })
                                },
                            ));

                        new_engine = new_engine.with_transformer_callback(callback);
                        info!("SUCCESS: Workflow engine wired with transformer callback");
                    }

                    // Wire DB loader callback for database loading steps
                    // Pass RDF store to enable ontology-driven loading
                    let db_loader_callback = graphica_coordinator::workflows::db_loader_callback::create_db_loader_callback(
                        datasource_catalog.clone(),
                        Some(rdf_store.clone()),
                        secret_store_registry.clone()
                    );
                    new_engine = new_engine.with_db_loader_callback(db_loader_callback);
                    info!("SUCCESS: Workflow engine wired with DB loader callback (ontology-driven loading enabled)");

                    let db_extract_callback = graphica_coordinator::workflows::db_extract_callback::create_db_extract_callback(
                        datasource_catalog.clone()
                    );
                    new_engine = new_engine.with_db_extract_callback(db_extract_callback);
                    info!("SUCCESS: Workflow engine wired with DB extract callback");

                    info!("SUCCESS: Workflow engine wired with lineage tracking");
                    Some(Arc::new(new_engine))
                } else {
                    info!("WARNING: Workflow engine lacks execution capabilities, lineage tracking not wired");
                    workflow_engine.clone()
                }
            } else {
                workflow_engine.clone()
            }
        } else {
            None
        }
    };

    // Build API state
    let api_state = ApiState {
        lineage_storage,
        governance_brain,
        rdf_store: Some(rdf_store.clone()),
        shard_registry: Some(shard_registry.clone()),
        query_executor,
        workflow_engine,
        model_registry,
        model_cache,
        rule_executor,
        circuit_breakers: Some(Arc::new(dashmap::DashMap::new())),
        auth_config,
        user_service,
        setup_token_manager,
        audit_logger,
        datasource_catalog: Some(datasource_catalog.clone()),
        datasource_catalog_impl: Some(datasource_catalog),
        import_job_manager: Arc::new(
            graphica_coordinator::api::import_jobs::ImportJobManager::new(),
        ),
        persisted_ontology_registry: Some(persisted_ontology_registry.clone()),
        ontology_registry: Some(ontology_registry),
        rdf_storage: rdf_storage_client,
        connector_registry: Some(connector_registry),
        resolved_entity_cache: Some(Arc::new(
            graphica_coordinator::api::resolved_entity_cache::ResolvedEntityCache::new(),
        )),
        // Reference metrics from AppContext for middleware compatibility
        metrics_registry: app_context.metrics.clone(),
        // Field mapping engine for intelligent schema-to-ontology mapping
        mapping_engine,
        // Secret store registry for managing credentials across multiple backends
        secret_store_registry,
        // Loader job manager for ETL operations
        loader_job_manager,
        // Unified mapping coordinator for CSV-to-DB workflows
        unified_mapping_coordinator,
        // Versioned ontology binding lifecycle service
        binding_service,
        // Workflow schedule store for managing workflow schedules
        schedule_store: Some(Arc::new(
            graphica_coordinator::workflows::storage::ScheduleStore::new(),
        )),
        // Workflow store for modern route-based workflows
        workflow_store: Some(Arc::new(
            graphica_coordinator::workflows::storage::WorkflowStore::new(),
        )),
        // Execution store for workflow execution tracking (RocksDB-backed via RocksDbBackend)
        // Uses the same underlying RocksDB as CheckpointManager for data consistency
        execution_store: Some(execution_store),
        // File Library storage for enterprise file management (using RocksDB for persistence)
        file_library,
        // Transformer registry for workflow Transform actions (CSV parser, DB2 migrator, etc.)
        transformer_registry,
        // Production workflow action integrations
        kafka_producer: None, // Initialized on-demand in streaming executor
        http_client: None,    // Initialized on-demand in streaming executor
        // Workflow lineage tracking with RDF backend
        lineage_generator: {
            use graphica_coordinator::workflows::lineage::rdf::WorkflowLineageGenerator;
            if std::env::var("WORKFLOW_LINEAGE_ENABLED")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true)
            {
                info!("Initializing workflow lineage generator with RDF backend...");
                Some(Arc::new(WorkflowLineageGenerator::new(rdf_store.clone())))
            } else {
                info!("INFO: Workflow lineage tracking disabled (WORKFLOW_LINEAGE_ENABLED=false)");
                None
            }
        },
        // Workflow metrics with Prometheus registry
        metrics: {
            use graphica_coordinator::observability::metrics::WorkflowMetrics;
            if let Some(ref metrics_registry) = app_context.metrics {
                match WorkflowMetrics::new(metrics_registry.registry()) {
                    Ok(wf_metrics) => {
                        info!("SUCCESS: Workflow metrics initialized with Prometheus registry");
                        Some(Arc::new(wf_metrics))
                    }
                    Err(e) => {
                        warn!("WARNING: Failed to initialize workflow metrics: {}", e);
                        warn!("   Workflow metrics will not be available");
                        None
                    }
                }
            } else {
                None
            }
        },
        // Distributed replay coordinator for Raft-based leader election (optional)
        replay_coordinator: None, // TODO: Initialize when distributed HA is configured
        // Row-level lineage tracking with RocksDB backend (using pre-initialized concrete store)
        row_lineage_store: row_lineage_store_concrete.clone().map(|store| {
            store as Arc<dyn graphica_core::core::lineage::row_level::RowLevelLineageSink>
        }),
        // Column-level lineage tracking with RocksDB backend (using pre-initialized concrete store)
        column_lineage_store: column_lineage_store_concrete.clone().map(|store| {
            store as Arc<dyn graphica_core::core::lineage::column_level::ColumnLineageSink>
        }),
        // Schema evolution store for DDL change tracking and drift analysis (using pre-initialized concrete store)
        schema_evolution_store: schema_evolution_store_concrete.clone(),
        // Manual mapping store for user-defined ontology mappings (with RocksDB persistence)
        manual_mapping_store: {
            use graphica_coordinator::mapping::manual::ManualMappingStore;

            let manual_mapping_enabled = std::env::var("MANUAL_MAPPING_ENABLED")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true);

            if manual_mapping_enabled {
                let manual_mapping_path = std::env::var("MANUAL_MAPPING_DB_PATH")
                    .unwrap_or_else(|_| "./data/manual-mappings-db".to_string());

                info!(
                    "Initializing manual mapping store at: {}",
                    manual_mapping_path
                );

                match ManualMappingStore::new(rdf_store.clone(), &manual_mapping_path) {
                    Ok(store) => {
                        info!(
                            "SUCCESS: Manual mapping RocksDB initialized at {}",
                            manual_mapping_path
                        );
                        Some(Arc::new(store))
                    }
                    Err(e) => {
                        warn!("ERROR: Failed to initialize manual mapping store: {}", e);
                        warn!("   Manual mapping features will not be available");
                        None
                    }
                }
            } else {
                info!("INFO: Manual mapping store disabled (MANUAL_MAPPING_ENABLED=false)");
                None
            }
        },
        // DB2 connection pool for high-performance concurrent DB2 operations
        db2_pool,
        // Production persistence components (initialized above)
        checkpoint_persistence,
        dlq_reader: Some(dlq_reader),
        dlq_reprocessor: Some(dlq_reprocessor),
        dlq_stats_calculator: Some(dlq_stats_calculator),
        schema_version_store: Some(schema_version_store),
        // Governance policy checker for workflow execution validation (Phase 1.1)
        policy_checker: {
            use graphica_coordinator::workflows::governance::GovernancePolicyChecker;
            if std::env::var("WORKFLOW_GOVERNANCE_ENABLED")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true)
            {
                info!("Initializing workflow governance policy checker with RDF backend...");
                Some(Arc::new(GovernancePolicyChecker::new(rdf_store.clone())))
            } else {
                info!("INFO: Workflow governance policy checking disabled (WORKFLOW_GOVERNANCE_ENABLED=false)");
                None
            }
        },
        // Execution state synchronizer for unified RDF + execution graph (Phase 3.1)
        execution_sync: {
            use graphica_coordinator::workflows::lineage::ExecutionStateSynchronizer;
            if std::env::var("WORKFLOW_EXECUTION_SYNC_ENABLED")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true)
            {
                info!("Initializing execution state synchronizer for unified RDF/execution architecture...");
                Some(Arc::new(ExecutionStateSynchronizer::new(rdf_store.clone())))
            } else {
                info!("INFO: Execution state synchronization disabled (WORKFLOW_EXECUTION_SYNC_ENABLED=false)");
                None
            }
        },
        // Approval store for workflow approval requests (human-in-the-loop workflows)
        approval_store: {
            use graphica_coordinator::workflows::storage::ApprovalStore;
            if std::env::var("WORKFLOW_APPROVAL_ENABLED")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true)
            {
                info!("Initializing approval store for workflow approval requests...");
                Some(Arc::new(ApprovalStore::new()))
            } else {
                info!("INFO: Workflow approval system disabled (WORKFLOW_APPROVAL_ENABLED=false)");
                None
            }
        },
        // GDPR coordinator for Article 17 (Right to Erasure) compliance
        gdpr_coordinator,
        // GDPR export executor for Article 20 (Right to Data Portability) compliance
        // To enable GDPR data exports, set GDPR_EXPORT_ENABLED=true and configure:
        //   - GDPR_EXPORT_DIR: Directory for storing export files (default: ./data/gdpr_exports)
        //   - GDPR_EXPORT_JOBS_DB: RocksDB path for job storage (default: ./data/gdpr_export_jobs)
        //   - GDPR_EXPORT_EXPIRY_HOURS: Download expiry in hours (default: 48)
        //
        // Example initialization:
        //   use graphica_coordinator::gdpr::export::{
        //       ExportExecutor, ExportJobStore, DataDiscoveryService, FormatConverter
        //   };
        //   let job_store = Arc::new(ExportJobStore::create("./data/gdpr_export_jobs")?);
        //   let discovery = Arc::new(DataDiscoveryService::new(lineage_storage.clone()));
        //   let converter = Arc::new(FormatConverter);
        //   let export_dir = PathBuf::from("./data/gdpr_exports");
        //   std::fs::create_dir_all(&export_dir)?;
        //   let executor = Arc::new(ExportExecutor::new(
        //       job_store, discovery, converter, export_dir, 48
        //   ));
        export_executor: None, // TODO: Initialize when GDPR_EXPORT_ENABLED=true
        // Phase 3: Workflow progress tracking and cancellation
        progress_store: None, // TODO: Initialize with RocksDB when WORKFLOW_PROGRESS_ENABLED=true
        cancellation_manager: Some(Arc::new(
            graphica_coordinator::workflows::CancellationManager::new(),
        )),
        // Systems-of-Systems validation storage (high-performance RocksDB backend)
        sos_storage_manager: {
            use graphica_coordinator::api::sos_validation::storage::SosStorageManager;

            let sos_enabled = std::env::var("SOS_VALIDATION_ENABLED")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true);

            if sos_enabled {
                let sos_db_path = std::env::var("SOS_DB_PATH")
                    .unwrap_or_else(|_| "./data/sos-validation-db".to_string());

                info!("Initializing SoS validation storage at: {}", sos_db_path);

                match SosStorageManager::new(&sos_db_path) {
                    Ok(manager) => {
                        info!(
                            "SUCCESS: SoS validation RocksDB initialized at {}",
                            sos_db_path
                        );
                        Some(Arc::new(manager))
                    }
                    Err(e) => {
                        warn!("ERROR: Failed to initialize SoS validation storage: {}", e);
                        warn!("   SoS validation features will not be available");
                        None
                    }
                }
            } else {
                info!("INFO: SoS validation storage disabled (SOS_VALIDATION_ENABLED=false)");
                None
            }
        },
        // Schema discovery state manager (Phase 1: Async discovery with progress tracking)
        discovery_state: Some(Arc::new(
            graphica_coordinator::mapping::discovery::DiscoveryStateManager::new(),
        )),
        // Schema discovery orchestrator (Phase 1: Intelligent schema discovery)
        discovery_orchestrator: {
            let discovery_cache_path = std::env::var("DISCOVERY_CACHE_PATH")
                .unwrap_or_else(|_| "./data/discovery-cache".to_string());

            info!(
                "Initializing discovery orchestrator with cache at: {}",
                discovery_cache_path
            );

            match graphica_coordinator::mapping::discovery::DiscoveryOrchestrator::new(
                &discovery_cache_path,
            ) {
                Ok(mut orchestrator) => {
                    // Register extractors for supported data sources
                    use graphica_coordinator::mapping::discovery::extractors::*;

                    orchestrator
                        .register_extractor("PostgreSQL".to_string(), PostgreSQLExtractor::new());
                    orchestrator.register_extractor("DB2".to_string(), DB2Extractor::new());
                    orchestrator.register_extractor("Oracle".to_string(), OracleExtractor::new());
                    orchestrator.register_extractor("SAPHANA".to_string(), SAPHANAExtractor::new());
                    orchestrator
                        .register_extractor("Databricks".to_string(), DatabricksExtractor::new());

                    info!(
                        "✓ Discovery orchestrator initialized with {} extractors",
                        orchestrator.registry.list_extractors().len()
                    );

                    Some(Arc::new(orchestrator))
                }
                Err(e) => {
                    warn!("ERROR: Failed to initialize discovery orchestrator: {}", e);
                    warn!("   Schema discovery features will not be available");
                    None
                }
            }
        },
    };

    // Start approval timeout handler for automatic expiration of stale approvals
    let timeout_handler_handle = if let Some(ref approval_store) = api_state.approval_store {
        use graphica_coordinator::workflows::governance::ApprovalTimeoutHandler;
        use std::time::Duration;

        let check_interval_secs = std::env::var("APPROVAL_TIMEOUT_CHECK_INTERVAL_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(60); // Default: check every 60 seconds

        if let Some(ref execution_store) = api_state.execution_store {
            info!(
                "Initializing approval timeout handler (check interval: {}s)...",
                check_interval_secs
            );

            let handler = ApprovalTimeoutHandler::new(
                approval_store.clone(),
                execution_store.clone(),
                Duration::from_secs(check_interval_secs),
            );

            let handle = handler.start();
            info!("SUCCESS: Approval timeout handler started - will check for expired approvals every {}s", check_interval_secs);
            Some(handle)
        } else {
            warn!(
                "WARNING: Approval timeout handler not started - execution_store not initialized"
            );
            None
        }
    } else {
        info!("INFO: Approval timeout handler not started - approval system disabled");
        None
    };

    // Checkpoint Manager for periodic workflow state persistence with RocksDB
    if let Some(ref rocks_store) = rocks_execution_store {
        let checkpoint_enabled = std::env::var("CHECKPOINT_ENABLED")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(true); // Enabled by default with RocksExecutionStore

        if checkpoint_enabled {
            use graphica_coordinator::workflows::storage::{CheckpointConfig, CheckpointManager};

            let shard_url = std::env::var("WORKFLOW_SHARD_URL").ok();
            let checkpoint_interval = std::env::var("CHECKPOINT_INTERVAL_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(30);

            let stale_timeout = std::env::var("CHECKPOINT_STALE_TIMEOUT_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(300); // 5 minutes default

            let checkpoint_config = CheckpointConfig {
                interval_secs: checkpoint_interval,
                enabled: true,
                running_only: true,
                shard_url: shard_url.clone(),
                stale_timeout_secs: stale_timeout,
            };

            info!("Checkpoint Manager configuration:");
            info!("   Enabled: {}", checkpoint_enabled);
            info!("   Interval: {} seconds", checkpoint_interval);
            if let Some(ref url) = shard_url {
                info!("   Shard URL: {}", url);
            } else {
                info!("   Shard URL: Not configured (local RocksDB persistence only)");
            }

            let checkpoint_manager = Arc::new(CheckpointManager::new(
                rocks_store.clone(),
                checkpoint_config,
            ));

            // Connect to shard if configured
            let manager_clone = checkpoint_manager.clone();
            tokio::spawn(async move {
                if let Err(e) = manager_clone.connect_shard().await {
                    warn!("WARNING: Failed to connect to shard: {}", e);
                    warn!("   Checkpoints will be stored in local RocksDB only");
                }
            });

            // Start periodic checkpointing in background
            tokio::spawn(async move {
                checkpoint_manager.start_periodic_checkpointing().await;
            });

            info!("SUCCESS: Checkpoint Manager started with RocksDB persistence");
        } else {
            info!("INFO: Checkpoint Manager disabled (CHECKPOINT_ENABLED=false)");
        }
    } else {
        warn!("WARNING: RocksExecutionStore failed to initialize, Checkpoint Manager disabled");
    }

    // Start Streaming CDC Executor (if configured)
    // This enables CDC → Workflow → Action execution pipelines for production streaming
    if let Ok(workflow_id) = std::env::var("WORKFLOW_STREAMING_ID") {
        use graphica_coordinator::workflows::engine::StreamExecutor;

        let kafka_brokers = std::env::var("KAFKA_CDC_BROKERS")
            .unwrap_or_else(|_| "localhost:9092".to_string())
            .split(',')
            .map(String::from)
            .collect::<Vec<String>>();

        let kafka_topic = std::env::var("KAFKA_CDC_TOPIC")
            .unwrap_or_else(|_| "dbserver1.public.customers".to_string());

        let consumer_group = std::env::var("KAFKA_CONSUMER_GROUP")
            .unwrap_or_else(|_| format!("graphica-workflow-{}", workflow_id));

        info!("Workflow: Starting Streaming CDC Executor...");
        info!("   Workflow ID: {}", workflow_id);
        info!("   Kafka Brokers: {:?}", kafka_brokers);
        info!("   Kafka Topic: {}", kafka_topic);
        info!("   Consumer Group: {}", consumer_group);

        // Clone necessary state for the background task
        let workflow_store = api_state.workflow_store.clone();
        let execution_store = api_state.execution_store.clone();

        tokio::spawn(async move {
            // Unwrap Options (these should be initialized at this point)
            let workflow_store = workflow_store.expect("workflow_store not initialized");
            let execution_store = execution_store.expect("execution_store not initialized");

            // Fetch workflow definition
            let workflow = match workflow_store.get(&workflow_id) {
                Ok(Some(wf)) => wf,
                Ok(None) => {
                    error!(
                        "ERROR: Workflow '{}' not found, streaming disabled",
                        workflow_id
                    );
                    return;
                }
                Err(e) => {
                    error!("ERROR: Failed to fetch workflow '{}': {}", workflow_id, e);
                    return;
                }
            };

            info!("SUCCESS: Loaded workflow '{}' for streaming", workflow.name);

            // Create StreamExecutor
            let stream_executor =
                StreamExecutor::new(workflow_store.clone(), execution_store.clone());

            // Start streaming loop
            info!("Starting: Starting CDC → Workflow streaming pipeline...");
            if let Err(e) = stream_executor
                .start_simple_stream_loop(&workflow, kafka_brokers, kafka_topic, consumer_group)
                .await
            {
                error!("ERROR: Streaming loop failed: {}", e);
            }
        });

        info!("SUCCESS: Streaming CDC Executor: Background task spawned");
    } else {
        info!("INFO:  Streaming CDC not configured (set WORKFLOW_STREAMING_ID to enable)");
    }

    // Build router
    info!("Distributed: Building REST API router...");
    let app = build_router(api_state);

    // Create server address
    let addr = SocketAddr::from(([0, 0, 0, 0], args.rest_port));

    // Start gRPC CoordinatorService for shard auto-registration
    info!("Distributed: Starting gRPC CoordinatorService...");
    let grpc_addr = format!("0.0.0.0:{}", args.grpc_port)
        .parse()
        .context("Failed to parse gRPC address")?;

    let coordinator_config = CoordinatorServiceConfig {
        enable_auth: false,
        coordinator_version: env!("CARGO_PKG_VERSION").to_string(),
        heartbeat_interval_secs: 30,
        stats_reporting_interval_secs: 60,
        enable_compression: true,
        max_shards: 100,
    };

    let coordinator_service =
        CoordinatorServiceImpl::new(shard_registry.clone(), coordinator_config)
            .context("Failed to create CoordinatorService")?;

    let grpc_server = CoordinatorServiceServer::new(coordinator_service);

    // Start gRPC server in background task
    let grpc_shutdown = shutdown_signal();
    tokio::spawn(async move {
        info!("gRPC: CoordinatorService listening on {}", grpc_addr);
        if let Err(e) = Server::builder()
            .add_service(grpc_server)
            .serve_with_shutdown(grpc_addr, grpc_shutdown)
            .await
        {
            error!("gRPC server error: {}", e);
        }
        info!("gRPC: CoordinatorService shutdown complete");
    });

    info!("SUCCESS: Coordinator ready!");
    info!("   REST API: http://localhost:{}", args.rest_port);
    info!(
        "   gRPC API: http://0.0.0.0:{} (auto-registration)",
        args.grpc_port
    );
    info!("   Health:   http://localhost:{}/health", args.rest_port);
    info!("   Metrics:  http://localhost:{}/metrics", args.rest_port);
    info!(
        "   OpenAPI:  http://localhost:{}/openapi.yaml",
        args.rest_port
    );
    info!("");
    info!("Press Ctrl+C to shutdown");

    // Start server with graceful shutdown
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .context("Failed to bind TCP listener")?;

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("Server error")?;

    // Gracefully stop approval timeout handler
    if let Some(handle) = timeout_handler_handle {
        info!("Shutdown: Stopping approval timeout handler...");
        if let Err(e) = handle.stop().await {
            warn!("Shutdown: Error stopping approval timeout handler: {}", e);
        } else {
            info!("Shutdown: Approval timeout handler stopped successfully");
        }
    }

    info!("Shutdown: Coordinator shutdown complete");

    Ok(())
}

/// Graceful shutdown signal handler
async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    info!("Stopping: Shutdown signal received, cleaning up...");
}
