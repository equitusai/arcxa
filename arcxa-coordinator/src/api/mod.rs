//! # API Module
//!
//! REST and gRPC API implementations.

pub mod audit;
pub mod auth;
pub mod cluster_admin;
pub mod datasources;
pub mod ddl; // DDL generation API (Stage 3: SHACL to SQL)
pub mod field_lineage; // Phase 1: Field-level lineage and golden records
pub mod file_library; // File Library API (Enterprise CSV/TSV/Excel file management)
pub mod gdpr; // GDPR compliance API (Article 17: Right to Erasure)
pub mod governance; // Governance API: SPARQL queries and RDF store management
pub mod grpc;
pub mod import_jobs; // Background import job management
pub mod kafka_raft; // Kafka Raft coordination API (distributed replay leader election)
pub mod lineage; // Lineage query API (Sprint 1.9: W3C PROV query endpoints)
pub mod loader; // ETL loader API (CSV to DB2/PostgreSQL)
pub mod ontology; // Phase 1: Custom ontology management API
pub mod ontology_ddl; // Ontology-driven DDL API (GAP-002 Phase 3: Semantic DDL generation)
pub mod openapi;
pub mod openlineage;
pub mod profiling; // Source profiling API (Stage 1: Dataset discovery with DCAT/VoID)
pub mod r2rml; // R2RML mapping API (Stage 2: Semantic mapping)
pub mod rate_limit;
pub mod rdf_storage; // RDF storage client for governance brain
pub mod resolved_entity_cache; // Phase 2: Streaming golden record cache
pub mod rest;
pub mod schema_api; // Phase 2: Cross-source mapping and type conversion API
mod schema_api_impl; // Helper functions for schema API
pub mod secrets; // Secret management API (credentials, API keys, etc.)
pub mod setup_token;
pub mod sos_validation; // Systems-of-Systems validation API: cross-system compatibility and integration
pub mod unified_mapping; // Unified mapping API (CSV-to-DB consolidation with PostgreSQL, DB2, Oracle support)
pub mod users;
pub mod workflow; // OpenLineage API (lineage event interchange format)

// REST API sub-modules (refactored from rest.rs)
pub mod dto; // Data Transfer Objects
pub mod handlers; // Request handlers
pub mod validators; // Validation logic

use std::sync::Arc;

// Proto types are imported from graphica-core
pub use graphica_core::distributed::proto;

/// API state with real storage backends
#[derive(Clone)]
pub struct ApiState {
    pub lineage_storage: Arc<crate::storage::LineageStorage>,
    pub governance_brain: Option<crate::governance::SharedGovernanceBrain>,
    pub rdf_store: Option<Arc<crate::governance::rdf_store::GraphicaRdfStore>>,
    // Distributed shard registry (optional - for horizontal scaling)
    pub shard_registry: Option<Arc<crate::governance::distributed::ShardRegistry>>,
    // Query executor for distributed SPARQL queries (needed for workflow input adapters)
    pub query_executor: Option<Arc<crate::governance::shard_coordinator::query::QueryExecutor>>,
    // Workflow orchestration components (using graphica_core modules)
    pub workflow_engine: Option<Arc<graphica_core::orchestration::WorkflowEngine>>,
    pub model_registry: Option<Arc<crate::api::handlers::PersistentModelRegistry>>,
    pub model_cache: Option<Arc<graphica_core::orchestration::ml::ModelCache>>,
    pub rule_executor: Option<Arc<graphica_core::orchestration::rules::RuleExecutor>>,
    // Circuit breakers (using graphica_core)
    pub circuit_breakers:
        Option<Arc<dashmap::DashMap<String, Arc<graphica_core::reliability::CircuitBreaker>>>>,
    // Authentication configuration
    pub auth_config: Arc<crate::api::auth::AuthConfig>,
    // User management service
    pub user_service: Option<Arc<crate::api::users::UserService>>,
    // Setup token manager for secure admin initialization
    pub setup_token_manager: Arc<crate::api::setup_token::SetupTokenManager>,
    // Audit logging for security events
    pub audit_logger: Option<Arc<crate::api::audit::AuditLogger>>,
    // Data source catalog for external data source configuration
    pub datasource_catalog: Option<Arc<dyn graphica_core::catalog::DataSourceCatalog>>,
    // Concrete catalog implementation (for admin operations like RDF sync)
    pub datasource_catalog_impl: Option<Arc<crate::catalog_impl::InMemoryDataSourceCatalog>>,
    // Persisted ontology registry for custom domain ontologies (with RocksDB persistence)
    pub persisted_ontology_registry:
        Option<Arc<crate::mapping::ontology_registry::PersistedOntologyRegistry>>,
    // Legacy in-memory registry accessor (for mapping engine compatibility)
    pub ontology_registry:
        Option<Arc<parking_lot::RwLock<graphica_core::catalog::OntologyRegistry>>>,
    // RDF storage client for persisting schema inference to governance brain
    pub rdf_storage: Option<Arc<tokio::sync::Mutex<rdf_storage::RdfStorageClient>>>,
    // Connector registry for datasource plugin metadata
    pub connector_registry:
        Option<Arc<parking_lot::RwLock<graphica_core::catalog::connectors::ConnectorRegistry>>>,
    // Streaming golden record cache (Phase 2)
    pub resolved_entity_cache: Option<Arc<resolved_entity_cache::ResolvedEntityCache>>,
    // Metrics registry for observability
    pub metrics_registry: Option<Arc<crate::observability::MetricsRegistry>>,
    // Background import job manager
    pub import_job_manager: Arc<import_jobs::ImportJobManager>,
    // Field mapping engine for intelligent schema-to-ontology mapping
    pub mapping_engine: Option<Arc<crate::mapping::MappingEngine>>,
    // Secret store registry for managing credentials across multiple backends
    pub secret_store_registry: Option<Arc<graphica_core::secrets::providers::SecretStoreRegistry>>,
    // Loader job manager for ETL operations
    pub loader_job_manager: Option<Arc<crate::mapping::loader::orchestration::LoaderJobManager>>,
    // Unified mapping coordinator for CSV-to-DB consolidation
    pub unified_mapping_coordinator:
        Option<Arc<crate::mapping::multi_source::UnifiedMappingCoordinator>>,
    // Versioned ontology->physical binding service for goal-driven SQL planning
    pub binding_service: Option<Arc<crate::mapping::bindings::BindingService>>,
    // Workflow schedule store for managing workflow schedules
    pub schedule_store: Option<Arc<crate::workflows::storage::ScheduleStore>>,
    // Workflow store for modern route-based workflows
    pub workflow_store: Option<Arc<crate::workflows::storage::WorkflowStore>>,
    // Execution store for workflow execution tracking
    pub execution_store: Option<Arc<crate::workflows::storage::ExecutionStore>>,
    // Shared stream executor for route-based streaming workflows
    pub stream_executor: Option<Arc<crate::workflows::engine::StreamExecutor>>,
    // File Library storage for enterprise file management (supports multiple backends via trait)
    pub file_library: Option<Arc<dyn file_library::storage_trait::FileLibraryStore>>,
    // Transformer registry for workflow Transform actions (CSV parser, DB2 migrator, deduplicator, etc.)
    pub transformer_registry:
        Option<Arc<crate::workflows::engine::transformers::TransformerRegistry>>,
    // Phase 3: Production workflow action integrations
    pub kafka_producer: Option<Arc<crate::workflows::integration::KafkaProducer>>,
    pub http_client: Option<Arc<crate::workflows::integration::HttpClient>>,
    pub lineage_generator: Option<Arc<crate::workflows::lineage::WorkflowLineageGenerator>>,
    pub metrics: Option<Arc<crate::observability::metrics::WorkflowMetrics>>,
    // Distributed replay coordinator for Raft-based leader election in HA deployments
    pub replay_coordinator: Option<Arc<crate::storage::kafka::DistributedReplayCoordinator>>,
    // Row-level lineage store for fine-grained ETL tracking
    pub row_lineage_store:
        Option<Arc<dyn graphica_core::core::lineage::row_level::RowLevelLineageSink>>,
    // Column-level lineage store for column-to-column dependency tracking
    pub column_lineage_store:
        Option<Arc<dyn graphica_core::core::lineage::column_level::ColumnLineageSink>>,
    // Schema evolution store for DDL change tracking and drift analysis
    pub schema_evolution_store:
        Option<Arc<crate::storage::schema_evolution_store::SchemaEvolutionStore>>,
    // Manual field mapping store for user-defined ontology mappings (with learning feedback loop)
    pub manual_mapping_store: Option<Arc<crate::mapping::manual::ManualMappingStore>>,
    // DB2 connection pool for high-performance concurrent DB2 workflow operations
    pub db2_pool: Option<Arc<crate::mapping::loader::DB2Pool>>,
    // Checkpoint persistence for ETL operation recovery (production-ready)
    pub checkpoint_persistence: Option<Arc<crate::checkpointing::CheckpointPersistence>>,
    // DLQ reader for retrieving failed rows with pagination/filtering
    pub dlq_reader: Option<Arc<crate::mapping::loader::DlqReader>>,
    // DLQ reprocessor for retrying failed rows with lineage tracking
    pub dlq_reprocessor: Option<Arc<crate::mapping::loader::DlqReprocessor>>,
    // DLQ statistics calculator for real-time error metrics
    pub dlq_stats_calculator: Option<Arc<crate::mapping::loader::DlqStatsCalculator>>,
    // RDF-backed schema version store for DDL evolution tracking
    pub schema_version_store: Option<Arc<crate::governance::RdfSchemaVersionStore>>,
    // Governance policy checker for workflow execution validation (Phase 1.1)
    pub policy_checker: Option<Arc<crate::workflows::governance::GovernancePolicyChecker>>,
    // Execution state synchronizer for unified RDF + execution graph (Phase 3.1)
    pub execution_sync: Option<Arc<crate::workflows::lineage::ExecutionStateSynchronizer>>,
    // Approval store for workflow approval requests (Phase 3.1)
    pub approval_store: Option<Arc<crate::workflows::storage::ApprovalStore>>,
    // GDPR coordinator for Article 17 (Right to Erasure) compliance
    pub gdpr_coordinator: Option<Arc<crate::gdpr::GdprCoordinator>>,
    // GDPR export executor for Article 20 (Right to Data Portability) compliance
    pub export_executor: Option<Arc<crate::gdpr::export::ExportExecutor>>,
    // Workflow progress tracking store (Phase 3)
    pub progress_store: Option<Arc<crate::workflows::storage::ProgressStore>>,
    // Workflow cancellation manager (Phase 3)
    pub cancellation_manager: Option<Arc<crate::workflows::CancellationManager>>,
    // Systems-of-Systems validation storage (high-performance RocksDB backend)
    pub sos_storage_manager: Option<Arc<crate::api::sos_validation::storage::SosStorageManager>>,
    // Schema discovery state manager (Phase 1: Async discovery with progress tracking)
    pub discovery_state: Option<Arc<crate::mapping::discovery::DiscoveryStateManager>>,
    // Schema discovery orchestrator (Phase 1: Intelligent schema discovery)
    pub discovery_orchestrator: Option<Arc<crate::mapping::discovery::DiscoveryOrchestrator>>,
}
