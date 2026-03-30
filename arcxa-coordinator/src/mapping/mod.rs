//! # Advanced Field Mapping Engine
//!
//! Intelligent field-to-ontology mapping system using hybrid AI approaches.
//!
//! ## Architecture
//!
//! The mapping engine combines multiple matcher types:
//!
//! - **Statistical Matcher** (Phase 1): TF-IDF + N-grams for lexical similarity
//! - **Semantic Matcher** (Phase 2): Transformer embeddings for semantic understanding
//! - **GNN Matcher** (Phase 3): Graph structure for schema-aware matching
//! - **Symbolic Matcher** (Phase 4): SPARQL reasoning using ontology axioms
//!
//! ## Usage
//!
//! ```rust,ignore
//! use graphica_coordinator::mapping::{MappingEngine, types::*};
//!
//! // Analyze a source schema
//! let request = AnalyzeSchemaRequest {
//!     source_id: "pg_source".to_string(),
//!     table_name: "customers".to_string(),
//!     fields: vec![
//!         SchemaFieldInput {
//!             name: "cust_email".to_string(),
//!             data_type: "VARCHAR".to_string(),
//!             nullable: false,
//!             sample_values: Some(vec!["john@example.com".to_string()]),
//!             description: None,
//!         }
//!     ],
//!     sample_size: Some(100),
//! };
//!
//! let response = engine.analyze_schema(request).await?;
//!
//! // Get mapping candidates
//! let candidates = engine.get_candidates(field_id, 10, 0.5, None).await?;
//! ```

pub mod bindings; // Versioned ontology->physical bindings for goal SQL planning
pub mod data_source; // Unified data source abstraction (CSV, DB, Parquet, Streaming)
pub mod execution; // External execution ports for DB2/Oracle adapter workflows
pub mod lineage; // Extended lineage tracking for CSV-to-DB pipeline
pub mod loader; // Database loaders for bulk data loading
pub mod manual; // Manual field mapping with persistence and auto-suggestion
pub mod multi_source; // Multi-source schema consolidation for CSV-to-DB workflows
pub mod ontology_registry; // Phase 4: Ontology term management (registry, parsing, defaults)
pub mod similarity; // Shared similarity functions (n-grams, edit distance, cosine)
pub mod statistical;
pub mod storage;
pub mod types;
pub mod uri_utils; // Shared URI utilities (extract_local_name, extract_namespace)
pub mod vendor_loader; // Vendor ontology bulk loader for ERP migrations

// Phase 2: Semantic Matcher (requires ONNX model files)
// NOTE: This module is referenced in field_mapping code but doesn't exist - pre-existing issue
// TODO: Either implement this module or remove references to it
// Temporarily commented to fix build during consolidation work
// pub mod semantic;

// Phase 1+: Intelligent Schema Discovery (replaces hardcoded demo fields)
pub mod discovery;

// ============================================================================
// Phase 1: RDF-Driven ETL Components (NEW)
// ============================================================================

/// Stage 1: Source discovery and profiling with DCAT/VoID RDF serialization
/// Includes feature extraction and pattern detection
pub mod profiling;

pub mod planner;
/// Unified Semantic Mapping (Phase 1: R2RML + Ontology DDL Consolidation)
///
/// Consolidates `r2rml` and `ontology_ddl` into a single semantic mapping architecture.
/// Supports both RDF triple generation and SQL DDL generation from shared ontology mappings.
///
/// Status: Phase 1 foundation - shared types and core logic
pub mod semantic_mapping;

/// Stage 2: Semantic mapping (R2RML) - Sprint 1.2
pub mod r2rml;

/// Stage 3: DDL generation (SHACL to SQL) - Sprint 1.3
pub mod ddl;

/// Stage 4: Ontology-driven DDL (Phase 2 - GAP-002)
///
/// Semantic ontology mapping for DDL generation, ensuring cross-source consistency.
///
/// Provides:
/// - Field→ontology mapping using existing MappingEngine
/// - Ontology→SHACL constraint rules (schema:email → PropertyShape)

/// Stage 4b: Field Mapping Engine (Phase 2 - Task 2.1)
///
/// Consolidates ontology mapping logic across all systems:
/// - MappingEngine (statistical + semantic)
/// - OntologyDdlOrchestrator (pattern + registry + heuristics)
/// - graphica-core FieldMapper (lexical)
///
/// Provides:
/// - Single source of truth for all ontology mapping
/// - Pluggable strategy pattern (Pattern, Semantic, Statistical, Lexical, Registry, Heuristic)
/// - Consistent confidence scoring across all strategies
/// - Integration with graphica-model service for transformer embeddings
pub mod field_mapping;
/// - SHACL→DDL generation via existing convert_shape_to_table()
/// - RDF triple generation for semantic lineage
/// - Cross-source DDL consistency
///
/// Status: Phase 2.1 implemented (types + constraint registry)
/// TODO: Implement remaining phases (mapping_resolver, shacl_generator, rdf_lineage)
pub mod ontology_ddl;

pub use types::*;

use anyhow::{Context, Result};
use serde_json::Value as JsonValue;
use std::{fs::File, path::Path, sync::Arc};
use tracing::{info, warn};

use crate::governance::rdf_store::GraphicaRdfStore;
use graphica_core::catalog::OntologyRegistry;
use profiling::feature_extraction::SchemaIntelligence;
use statistical::StatisticalMatcher;
use storage::MappingStorage;
// PRE-EXISTING ISSUE: semantic module doesn't exist
// use semantic::SemanticMatcherClient;
use discovery::{DiscoveryOrchestrator, DiscoveryService};
use ontology_registry::RegistryClient;

/// Main mapping engine coordinator
pub struct MappingEngine {
    /// Statistical matcher (TF-IDF + N-grams)
    statistical: Arc<StatisticalMatcher>,

    /// Schema intelligence (feature extraction + profiling)
    intelligence: Arc<SchemaIntelligence>,

    /// Storage layer for mappings and indexes
    pub storage: Arc<MappingStorage>,

    /// RDF store for ontology queries
    rdf_store: Arc<GraphicaRdfStore>,

    /// Semantic matcher (Phase 2: Transformer embeddings via gRPC)
    /// Optional - coordinator runs without it if model service unavailable
    // PRE-EXISTING ISSUE: semantic module doesn't exist
    // semantic_matcher: Option<Arc<SemanticMatcherClient>>,

    /// Intelligent schema discovery service (Phase 1+)
    /// Trait-based service layer for catalog + discovery integration
    /// Replaces hardcoded demo fields with real-time introspection
    discovery_service: Option<Arc<dyn DiscoveryService>>,

    /// Direct discovery orchestrator (fallback when no catalog available)
    pub discovery: Arc<DiscoveryOrchestrator>,

    /// Ontology registry for custom domain ontologies
    ontology_registry: Option<Arc<parking_lot::RwLock<OntologyRegistry>>>,

    /// Ontology client for querying ontology terms
    ontology_client: RegistryClient,
}

impl MappingEngine {
    /// Create a new mapping engine
    pub async fn new(
        rocksdb_path: &str,
        rdf_store: Arc<GraphicaRdfStore>,
        // semantic_config: Option<semantic::ModelServiceConfig>, // PRE-EXISTING ISSUE
    ) -> Result<Self> {
        info!("🧠 Initializing Advanced Field Mapping Engine...");

        // Initialize storage
        let storage = Arc::new(MappingStorage::new(rocksdb_path)?);
        info!("  ✓ Storage initialized");

        // Initialize statistical matcher
        let statistical = Arc::new(StatisticalMatcher::new(storage.clone())?);
        info!("  ✓ Statistical matcher ready (TF-IDF + N-grams)");

        // Initialize schema intelligence
        let intelligence = Arc::new(SchemaIntelligence::new());
        info!("  ✓ Schema intelligence ready");

        // Initialize intelligent schema discovery (Phase 1+)
        let cache_path = format!("{}/discovery_cache", rocksdb_path);
        let mut orchestrator = DiscoveryOrchestrator::new(&cache_path)?;

        // Register extractors for supported data sources
        orchestrator.register_extractor(
            "postgresql".to_string(),
            discovery::PostgreSQLExtractor::new(),
        );
        orchestrator.register_extractor("edb".to_string(), discovery::PostgreSQLExtractor::new());
        orchestrator.register_extractor("db2".to_string(), discovery::DB2Extractor::new());
        orchestrator.register_extractor("oracle".to_string(), discovery::OracleExtractor::new());
        orchestrator.register_extractor("saphana".to_string(), discovery::SAPHANAExtractor::new());
        orchestrator.register_extractor(
            "databricks".to_string(),
            discovery::DatabricksExtractor::new(),
        );

        let discovery = Arc::new(orchestrator);
        info!("  ✓ Schema discovery ready (PostgreSQL/EDB, DB2, Oracle, SAP HANA, Databricks)");

        // PRE-EXISTING ISSUE: Semantic module doesn't exist - disabled for now
        // Initialize semantic matcher (Phase 2) - Optional
        // let semantic_matcher = if let Some(config) = semantic_config {
        //     let cache_dir = format!("{}/semantic_cache", rocksdb_path);
        //     match SemanticMatcherClient::new(config, &cache_dir).await {
        //         Ok(client) => {
        //             if client.is_available().await {
        //                 info!("  ✓ Semantic matcher ready (Transformer embeddings via gRPC)");
        //                 Some(Arc::new(client))
        //             } else {
        //                 warn!("  ⚠ Semantic matcher initialized but model service unavailable");
        //                 warn!("  ⚠ Will use cache-only mode for semantic matching");
        //                 Some(Arc::new(client))
        //             }
        //         }
        //         Err(e) => {
        //             warn!("  ⚠ Failed to initialize semantic matcher: {}", e);
        //             warn!("  ⚠ Mapping engine will use statistical matching only");
        //             None
        //         }
        //     }
        // } else {
        //     info!("  ℹ Semantic matcher not configured (Phase 1 mode)");
        //     None
        // };
        let semantic_matcher: Option<Arc<()>> = None; // Disabled - semantic module not implemented

        let phase_status = if semantic_matcher.is_some() {
            "Phase 1+2: Statistical + Semantic + Discovery"
        } else {
            "Phase 1: Statistical + Discovery"
        };
        info!("✅ Mapping engine initialized ({})", phase_status);

        // Initialize ontology client (will use defaults until registry is wired)
        let ontology_client = RegistryClient::new(None);

        Ok(Self {
            statistical,
            intelligence,
            storage,
            rdf_store,
            // PRE-EXISTING ISSUE: semantic_matcher field commented out
            // semantic_matcher,
            discovery_service: None, // Set via with_discovery_service()
            discovery,
            ontology_registry: None, // Set via with_ontology_registry()
            ontology_client,
        })
    }

    /// Enable ontology registry for custom domain ontologies
    pub fn with_ontology_registry(
        &mut self,
        ontology_registry: Arc<parking_lot::RwLock<OntologyRegistry>>,
    ) {
        info!("🔌 Wiring ontology registry into MappingEngine");
        self.ontology_registry = Some(ontology_registry.clone());

        // Update ontology client with the registry
        self.ontology_client = RegistryClient::new(Some(ontology_registry));
        info!("   ✓ Ontology client configured with registry");
    }

    /// Get a reference to the ontology client for cache invalidation setup
    pub fn ontology_client(&self) -> &RegistryClient {
        &self.ontology_client
    }

    /// Get the semantic matcher client (if available)
    // PRE-EXISTING ISSUE: semantic module doesn't exist
    // pub fn semantic_matcher(&self) -> Option<&Arc<SemanticMatcherClient>> {
    //     self.semantic_matcher.as_ref()
    // }

    /// Enable intelligent discovery with catalog integration
    ///
    /// This method wires the MappingEngine to use a DiscoveryService,
    /// enabling automatic schema discovery from real data sources.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let catalog = Arc::new(InMemoryDataSourceCatalog::new(registry));
    /// let discovery_service = Arc::new(ProductionDiscoveryService::new(
    ///     catalog.clone(),
    ///     mapping_engine.discovery.clone(),
    /// ));
    ///
    /// mapping_engine.with_discovery_service(discovery_service);
    /// ```
    pub fn with_discovery_service(&mut self, discovery_service: Arc<dyn DiscoveryService>) {
        info!("🔌 Wiring discovery service into MappingEngine");
        self.discovery_service = Some(discovery_service);
    }

    /// Check if intelligent discovery is enabled
    pub fn has_discovery_service(&self) -> bool {
        self.discovery_service.is_some()
    }

    /// Analyze a source schema and extract features
    pub async fn analyze_schema(
        &self,
        request: AnalyzeSchemaRequest,
    ) -> Result<AnalyzeSchemaResponse> {
        let start = std::time::Instant::now();

        info!(
            "Analyzing schema: source={}, table={}",
            request.source_id, request.table_name
        );

        // Convert input fields to SchemaFields
        let mut fields = Vec::new();
        for (idx, field_input) in request.fields.iter().enumerate() {
            let field_id = format!("{}_{}_{}", request.source_id, request.table_name, idx);

            let mut field = SchemaField {
                id: field_id.clone(),
                name: field_input.name.clone(),
                normalized_name: normalize_name(&field_input.name),
                data_type: field_input.data_type.clone(),
                nullable: field_input.nullable,
                sample_values: field_input.sample_values.clone().unwrap_or_default(),
                source_id: request.source_id.clone(),
                table_name: request.table_name.clone(),
                description: field_input.description.clone(),
                features: None,
            };

            // Extract features using schema intelligence
            let features = self.intelligence.extract_features(&field).await?;
            field.features = Some(features);

            // Store field in storage
            self.storage.store_field(&field)?;

            fields.push(field);
        }

        let processing_time_ms = start.elapsed().as_millis() as u64;

        info!(
            "✓ Analyzed {} fields in {}ms",
            fields.len(),
            processing_time_ms
        );

        Ok(AnalyzeSchemaResponse {
            fields,
            processing_time_ms,
        })
    }

    /// Get mapping candidates for a field
    pub async fn get_candidates(
        &self,
        field_id: &str,
        top_k: usize,
        min_confidence: f64,
        ontology_namespaces: Option<&[String]>,
    ) -> Result<GetCandidatesResponse> {
        let start = std::time::Instant::now();

        info!("Finding candidates for field: {}", field_id);

        // Retrieve field from storage
        let field = self
            .storage
            .get_field(field_id)?
            .ok_or_else(|| anyhow::anyhow!("Field not found: {}", field_id))?;

        // Get ontology terms (filtered by namespaces if provided)
        let ontology_terms = if let Some(namespaces) = ontology_namespaces {
            if !namespaces.is_empty() {
                info!(
                    "Filtering ontology terms by {} namespaces",
                    namespaces.len()
                );
                self.ontology_client.get_terms_by_namespaces(namespaces)?
            } else {
                self.get_ontology_terms().await?
            }
        } else {
            self.get_ontology_terms().await?
        };

        // Use statistical matcher to find candidates
        let mut candidates = self
            .statistical
            .find_candidates(&field, &ontology_terms, top_k)?;

        // Phase 2: Enhance with semantic matching (if available)
        // PRE-EXISTING ISSUE: semantic_matcher field doesn't exist
        // if let Some(semantic_matcher) = &self.semantic_matcher {
        //     info!("  Enhancing with semantic matching...");
        //     ... semantic enhancement code ...
        // } else {
        info!("  ℹ Semantic matcher not available, using statistical matching only");
        // }

        // Filter by minimum confidence (after blending)
        candidates.retain(|c| c.confidence >= min_confidence);

        // Limit to top_k
        candidates.truncate(top_k);

        let processing_time_ms = start.elapsed().as_millis() as u64;

        info!(
            "✓ Found {} candidates (>= {:.2} confidence) in {}ms",
            candidates.len(),
            min_confidence,
            processing_time_ms
        );

        Ok(GetCandidatesResponse {
            field_id: field_id.to_string(),
            candidates,
            processing_time_ms,
        })
    }

    /// Record user feedback on a mapping
    pub async fn record_feedback(&self, feedback: MappingFeedback) -> Result<()> {
        info!(
            "Recording feedback for field {} by user {}",
            feedback.field_id, feedback.user_id
        );

        // Store feedback in RDF store as training data
        self.storage.store_feedback(&feedback)?;

        // Update statistical matcher indexes with new ground truth
        if let Some(term_uri) = &feedback.selected_term_uri {
            let field = self
                .storage
                .get_field(&feedback.field_id)?
                .ok_or_else(|| anyhow::anyhow!("Field not found"))?;

            self.statistical.update_with_feedback(&field, term_uri)?;
        }

        info!("✓ Feedback recorded and indexes updated");

        Ok(())
    }

    /// Check if semantic matcher is available
    pub fn is_semantic_available(&self) -> bool {
        // PRE-EXISTING ISSUE: semantic_matcher field doesn't exist
        // self.semantic_matcher.is_some()
        false
    }

    /// Get phase status string
    pub fn get_phase_status(&self) -> &'static str {
        // PRE-EXISTING ISSUE: semantic_matcher field doesn't exist
        // if self.semantic_matcher.is_some() {
        //     "Phase 1+2: Statistical + Semantic"
        // } else {
        "Phase 1: Statistical only"
        // }
    }

    /// Find mapping candidates for an ad-hoc field (not stored in database)
    ///
    /// This method is useful for generating suggestions for fields that haven't been
    /// persisted yet, such as in manual mapping suggestion APIs.
    ///
    /// # Arguments
    /// * `field` - The schema field to find candidates for (can be synthetic/temporary)
    /// * `top_k` - Maximum number of candidates to return
    ///
    /// # Returns
    /// Vector of mapping candidates sorted by confidence (descending)
    pub async fn find_candidates_for_field(
        &self,
        field: &SchemaField,
        top_k: usize,
    ) -> Result<Vec<MappingCandidate>> {
        // Get ontology terms from registry
        let ontology_terms = self.get_ontology_terms().await?;

        if ontology_terms.is_empty() {
            return Ok(vec![]);
        }

        // Use statistical matcher to find candidates
        self.statistical
            .find_candidates(field, &ontology_terms, top_k)
    }

    /// Get ontology terms from ontology registry
    ///
    /// **Phase 4 Implementation**: Queries registered ontologies dynamically
    ///
    /// This method delegates to the RegistryClient which:
    /// 1. Queries all active ontologies from the registry
    /// 2. Parses Turtle content to extract classes and properties
    /// 3. Converts to OntologyTerm format for matching
    /// 4. Falls back to schema.org terms if no registry available
    async fn get_ontology_terms(&self) -> Result<Vec<OntologyTerm>> {
        self.ontology_client.get_ontology_terms()
    }

    // ========================================================================
    // Mapping Session Workflow - Phase 1 Implementation
    // ========================================================================

    /// Analyze a data source for mapping and create a session
    pub async fn analyze_for_mapping(
        &self,
        source_id: &str,
        request: AnalyzeForMappingRequest,
    ) -> Result<AnalyzeForMappingResponse> {
        let start = std::time::Instant::now();
        let allow_demo_fallback = std::env::var("ALLOW_DEMO_MAPPING_FALLBACK")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        let created_by = request.user_id.clone();
        let requested_tables = request.tables.clone();
        let config = Self::build_mapping_session_config(&request);

        info!(
            "Starting mapping analysis for source: {}, user: {}",
            source_id, created_by
        );

        // Get tables to analyze (query data source when not provided)
        let tables_to_analyze = if let Some(tables) = requested_tables {
            tables
        } else if let Some(discovery_service) = &self.discovery_service {
            info!("No tables specified, discovering tables from datasource");
            let discovered = discovery_service
                .discover_by_source_id(source_id, None, config.sample_size)
                .await
                .with_context(|| {
                    format!("Failed to discover tables for datasource {}", source_id)
                })?;

            let tables = discovered
                .tables
                .into_iter()
                .map(|t| t.name)
                .collect::<Vec<_>>();

            if tables.is_empty() {
                anyhow::bail!("No tables discovered for datasource {}", source_id);
            }

            tables
        } else if allow_demo_fallback {
            warn!(
                "⚠️  Discovery service not configured; using demo fallback tables (ALLOW_DEMO_MAPPING_FALLBACK enabled)"
            );
            vec![]
        } else {
            anyhow::bail!(
                "Discovery service not configured; tables must be provided for datasource {}",
                source_id
            );
        };

        let mut table_inputs = Vec::new();

        // Analyze each table
        for table_name in &tables_to_analyze {
            info!("  Analyzing table: {}", table_name);

            // Use intelligent discovery if service is available
            let discovered_fields = if let Some(discovery_service) = &self.discovery_service {
                info!("  🔍 Using intelligent schema discovery");

                match discovery_service
                    .discover_by_source_id(source_id, Some(table_name), config.sample_size)
                    .await
                {
                    Ok(discovered) => {
                        // Convert discovered schema to SchemaFieldInput
                        let mut fields = Vec::new();
                        for table in discovered.tables {
                            for column in table.columns {
                                fields.push(SchemaFieldInput {
                                    name: column.name.clone(),
                                    data_type: column.data_type.clone(),
                                    nullable: column.nullable,
                                    sample_values: Some(column.sample_values.clone()),
                                    description: column.semantic_type.map(|st| {
                                        format!(
                                            "Inferred: {} (confidence: {:.2})",
                                            st, column.confidence
                                        )
                                    }),
                                });
                            }
                        }

                        info!("    ✓ Discovered {} fields intelligently", fields.len());
                        fields
                    }
                    Err(e) => {
                        if allow_demo_fallback {
                            warn!(
                                "    ⚠ Discovery failed ({}), falling back to demo data (ALLOW_DEMO_MAPPING_FALLBACK enabled)",
                                e
                            );
                            self.get_demo_fields_for_table(source_id, table_name)
                        } else {
                            return Err(anyhow::anyhow!(
                                "Discovery failed for datasource {} table {}: {}",
                                source_id,
                                table_name,
                                e
                            ));
                        }
                    }
                }
            } else if allow_demo_fallback {
                warn!(
                    "  ⚙️  Discovery service not configured, using demo data (ALLOW_DEMO_MAPPING_FALLBACK enabled)"
                );
                self.get_demo_fields_for_table(source_id, table_name)
            } else {
                return Err(anyhow::anyhow!(
                    "Discovery service not configured for datasource {} (table: {})",
                    source_id,
                    table_name
                ));
            };

            table_inputs.push((table_name.clone(), discovered_fields));
        }

        self.create_mapping_session_from_field_inputs(
            source_id,
            &created_by,
            config,
            table_inputs,
            start,
        )
        .await
    }

    /// Analyze a managed Parquet-backed dataset and create a source mapping session.
    pub async fn analyze_dataset_for_mapping(
        &self,
        dataset_id: &str,
        parquet_path: &str,
        request: AnalyzeForMappingRequest,
    ) -> Result<AnalyzeForMappingResponse> {
        let start = std::time::Instant::now();
        let config = Self::build_mapping_session_config(&request);
        let created_by = request.user_id.clone();
        let table_name = request
            .tables
            .clone()
            .and_then(|tables| tables.into_iter().find(|name| !name.trim().is_empty()))
            .unwrap_or_else(|| derive_parquet_table_name(dataset_id, parquet_path));

        info!(
            "Starting mapping analysis for dataset: {}, user: {}, path: {}",
            dataset_id, created_by, parquet_path
        );

        let discovered_fields =
            discover_fields_from_parquet_path(parquet_path, config.sample_size)?;

        self.create_mapping_session_from_field_inputs(
            dataset_id,
            &created_by,
            config,
            vec![(table_name, discovered_fields)],
            start,
        )
        .await
    }

    fn build_mapping_session_config(request: &AnalyzeForMappingRequest) -> MappingSessionConfig {
        MappingSessionConfig {
            sample_size: request.sample_size.unwrap_or(1000),
            auto_approve_threshold: request.auto_approve_threshold.unwrap_or(0.95),
            min_confidence: request.min_confidence.unwrap_or(0.5),
            max_candidates: request.max_candidates.unwrap_or(10),
            ontology_namespaces: request.ontology_namespaces.clone(),
        }
    }

    async fn create_mapping_session_from_field_inputs(
        &self,
        source_id: &str,
        created_by: &str,
        config: MappingSessionConfig,
        table_inputs: Vec<(String, Vec<SchemaFieldInput>)>,
        start: std::time::Instant,
    ) -> Result<AnalyzeForMappingResponse> {
        let session_id = format!(
            "session_{}",
            uuid::Uuid::new_v4().to_string().replace('-', "")
        );
        let mut table_mappings = Vec::new();

        for (table_name, discovered_fields) in table_inputs {
            let mut field_mappings = Vec::new();

            for field_input in discovered_fields {
                let field_id = format!("{}_{}_{}", source_id, table_name, field_input.name);
                let analyze_req = AnalyzeSchemaRequest {
                    source_id: source_id.to_string(),
                    table_name: table_name.clone(),
                    fields: vec![field_input.clone()],
                    sample_size: Some(config.sample_size),
                };

                let analyzed = self.analyze_schema(analyze_req).await?;
                let analyzed_field = &analyzed.fields[0];
                let candidates_response = self
                    .get_candidates(
                        &analyzed_field.id,
                        config.max_candidates,
                        config.min_confidence,
                        config.ontology_namespaces.as_deref(),
                    )
                    .await?;

                let (approval_status, selected_mapping) =
                    if let Some(top_candidate) = candidates_response.candidates.first() {
                        if top_candidate.confidence >= config.auto_approve_threshold {
                            (
                                FieldApprovalStatus::AutoApproved,
                                Some(SelectedMapping {
                                    ontology_term_uri: top_candidate.ontology_term_uri.clone(),
                                    confidence: top_candidate.confidence,
                                    was_top_candidate: true,
                                    transformation: top_candidate.transformation.clone(),
                                }),
                            )
                        } else {
                            (FieldApprovalStatus::Pending, None)
                        }
                    } else {
                        (FieldApprovalStatus::Pending, None)
                    };

                field_mappings.push(FieldMappingState {
                    field_id,
                    field_name: field_input.name.clone(),
                    data_type: field_input.data_type.clone(),
                    sample_values: field_input.sample_values.clone().unwrap_or_default(),
                    candidates: candidates_response.candidates,
                    selected_mapping,
                    approval_status,
                    reviewed_by: None,
                    reviewed_at: None,
                    notes: None,
                });
            }

            table_mappings.push(TableMapping {
                table_name,
                field_mappings,
                metadata: None,
            });
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let mut session = MappingSession {
            session_id: session_id.clone(),
            source_id: source_id.to_string(),
            status: MappingSessionStatus::Draft,
            tables: table_mappings,
            created_by: created_by.to_string(),
            created_at: now,
            reviewed_by: None,
            reviewed_at: None,
            applied_at: None,
            config,
            summary: MappingSessionSummary::default(),
        };

        session.summary = storage::MappingStorage::compute_summary(&session);
        self.storage.store_session(&session)?;

        if session.summary.pending_review > 0 {
            self.storage
                .update_session_status(&session.session_id, MappingSessionStatus::PendingReview)?;
            session.status = MappingSessionStatus::PendingReview;
        }

        let processing_time_ms = start.elapsed().as_millis() as u64;

        info!(
            "✓ Mapping analysis complete: {} fields, {} auto-approved, {} pending review ({}ms)",
            session.summary.total_fields,
            session.summary.auto_approved,
            session.summary.pending_review,
            processing_time_ms
        );

        Ok(AnalyzeForMappingResponse {
            session_id,
            summary: session.summary,
            status: session.status,
            processing_time_ms,
        })
    }

    /// Discover fields from a data source using intelligent schema discovery
    ///
    /// This method uses the DiscoveryOrchestrator to perform real-time schema introspection
    /// instead of returning hardcoded demo data.
    ///
    /// # Arguments
    /// * `source` - The data source to discover schema from
    /// * `credentials` - Authentication credentials for the data source
    /// * `table_name` - Optional table name filter
    /// * `sample_size` - Number of rows to sample for type inference
    ///
    /// # Returns
    /// Vector of SchemaFieldInput with intelligent type inference and sample values
    async fn discover_fields_from_source(
        &self,
        source: &graphica_core::catalog::types::DataSource,
        credentials: &graphica_core::catalog::connector::Credentials,
        table_name: Option<&str>,
        sample_size: usize,
    ) -> Result<Vec<SchemaFieldInput>> {
        use discovery::DiscoveryConfig;

        info!(
            "🔍 Discovering schema for source: {}, table: {:?}",
            source.id, table_name
        );

        let config = DiscoveryConfig {
            schema_filter: None, // Let extractor determine default schema
            table_filter: table_name.map(|t| t.to_string()),
            sample_size,
            cache_ttl_secs: 3600, // 1 hour cache
        };

        let discovered = self
            .discovery
            .discover_schema(source, credentials, config)
            .await?;

        // Convert DiscoveredSchema to Vec<SchemaFieldInput>
        let mut fields = Vec::new();
        for table in discovered.tables {
            for column in table.columns {
                fields.push(SchemaFieldInput {
                    name: column.name.clone(),
                    data_type: column.data_type.clone(),
                    nullable: column.nullable,
                    sample_values: Some(column.sample_values.clone()),
                    description: column.semantic_type.map(|st| {
                        format!(
                            "Inferred semantic type: {} (confidence: {:.2})",
                            st, column.confidence
                        )
                    }),
                });
            }
        }

        info!("  ✓ Discovered {} fields", fields.len());

        Ok(fields)
    }

    /// Get demo fields for a table (FALLBACK - Use discover_fields_from_source() instead)
    ///
    /// **DEPRECATED**: This method returns hardcoded demo data and should only be used
    /// as a fallback when the actual data source is not available in the catalog.
    ///
    /// **TODO**: Replace all calls to this method with discover_fields_from_source()
    /// by adding a DataSourceCatalog reference to MappingEngine and looking up the
    /// source configuration.
    fn get_demo_fields_for_table(
        &self,
        _source_id: &str,
        table_name: &str,
    ) -> Vec<SchemaFieldInput> {
        // This is a FALLBACK for demo purposes
        // In a real implementation, this would:
        // 1. Look up the source in the DataSourceCatalog
        // 2. Get credentials from the catalog
        // 3. Call discover_fields_from_source() with the actual source
        warn!(
            "⚠️  Using fallback demo data for table '{}' - actual discovery not yet wired",
            table_name
        );
        warn!("💡 TODO: Add DataSourceCatalog to MappingEngine to enable real discovery");

        match table_name {
            "customers" => vec![
                SchemaFieldInput {
                    name: "customer_email".to_string(),
                    data_type: "VARCHAR".to_string(),
                    nullable: false,
                    sample_values: Some(vec!["john@example.com".to_string()]),
                    description: None,
                },
                SchemaFieldInput {
                    name: "customer_id".to_string(),
                    data_type: "INTEGER".to_string(),
                    nullable: false,
                    sample_values: Some(vec!["12345".to_string()]),
                    description: None,
                },
                SchemaFieldInput {
                    name: "full_name".to_string(),
                    data_type: "VARCHAR".to_string(),
                    nullable: false,
                    sample_values: Some(vec!["John Doe".to_string()]),
                    description: None,
                },
            ],
            "users" => vec![
                SchemaFieldInput {
                    name: "email".to_string(),
                    data_type: "VARCHAR".to_string(),
                    nullable: false,
                    sample_values: Some(vec!["user@example.com".to_string()]),
                    description: None,
                },
                SchemaFieldInput {
                    name: "user_id".to_string(),
                    data_type: "INTEGER".to_string(),
                    nullable: false,
                    sample_values: Some(vec!["1001".to_string()]),
                    description: None,
                },
                SchemaFieldInput {
                    name: "username".to_string(),
                    data_type: "VARCHAR".to_string(),
                    nullable: false,
                    sample_values: Some(vec!["jdoe123".to_string()]),
                    description: None,
                },
            ],
            "products" => vec![
                SchemaFieldInput {
                    name: "product_id".to_string(),
                    data_type: "INTEGER".to_string(),
                    nullable: false,
                    sample_values: Some(vec!["5001".to_string()]),
                    description: None,
                },
                SchemaFieldInput {
                    name: "product_name".to_string(),
                    data_type: "VARCHAR".to_string(),
                    nullable: false,
                    sample_values: Some(vec!["Widget Pro".to_string()]),
                    description: None,
                },
                SchemaFieldInput {
                    name: "price".to_string(),
                    data_type: "DECIMAL".to_string(),
                    nullable: false,
                    sample_values: Some(vec!["29.99".to_string()]),
                    description: None,
                },
            ],
            "orders" => vec![
                SchemaFieldInput {
                    name: "order_id".to_string(),
                    data_type: "INTEGER".to_string(),
                    nullable: false,
                    sample_values: Some(vec!["9001".to_string()]),
                    description: None,
                },
                SchemaFieldInput {
                    name: "customer_id".to_string(),
                    data_type: "INTEGER".to_string(),
                    nullable: false,
                    sample_values: Some(vec!["1001".to_string()]),
                    description: None,
                },
                SchemaFieldInput {
                    name: "order_date".to_string(),
                    data_type: "DATE".to_string(),
                    nullable: false,
                    sample_values: Some(vec!["2024-01-15".to_string()]),
                    description: None,
                },
            ],
            _ => {
                // Generic fields for unknown tables
                warn!(
                    "  ⚠ No demo data for table '{}', returning generic fields",
                    table_name
                );
                warn!("  💡 TODO: Integrate with datasource catalog to query actual schema");

                vec![
                    SchemaFieldInput {
                        name: format!("{}_id", table_name.trim_end_matches('s')),
                        data_type: "INTEGER".to_string(),
                        nullable: false,
                        sample_values: Some(vec!["1".to_string()]),
                        description: Some(format!("ID for {} table", table_name)),
                    },
                    SchemaFieldInput {
                        name: format!("{}_name", table_name.trim_end_matches('s')),
                        data_type: "VARCHAR".to_string(),
                        nullable: true,
                        sample_values: Some(vec!["Example Name".to_string()]),
                        description: Some(format!("Name field for {}", table_name)),
                    },
                ]
            }
        }
    }

    // ========================================================================
    // Data Import - Phase 2 Implementation
    // ========================================================================

    /// Execute data import using approved mappings
    pub async fn execute_import(
        &self,
        session_id: &str,
        request: ImportDataRequest,
    ) -> Result<ImportDataResponse> {
        let start = std::time::Instant::now();

        info!(
            "Starting data import for session: {}, user: {}",
            session_id, request.user_id
        );

        // Load session
        let session = self
            .storage
            .get_session(session_id)?
            .ok_or_else(|| anyhow::anyhow!("Session not found: {}", session_id))?;

        // Validate session is in Active status
        if session.status != MappingSessionStatus::Active {
            return Err(anyhow::anyhow!(
                "Session must be in Active status for import, currently {:?}",
                session.status
            ));
        }

        // Generate import ID
        let import_id = format!(
            "import_{}",
            uuid::Uuid::new_v4().to_string().replace('-', "")
        );

        // Determine target graph
        let target_graph = request
            .target_graph
            .unwrap_or_else(|| format!("http://graphica.io/graph/entities/{}", session.source_id));

        // Initialize statistics
        let mut stats = ImportStatistics {
            rows_processed: 0,
            entities_created: 0,
            triples_stored: 0,
            tables_imported: 0,
            fields_mapped: 0,
            errors: vec![],
        };

        // Collect all triples to store
        let mut all_triples = Vec::new();

        // Ontology namespaces
        let gph_ns = "http://graphica.io/ontology#";
        let prov_ns = "http://www.w3.org/ns/prov#";
        let rdf_ns = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
        let xsd_ns = "http://www.w3.org/2001/XMLSchema#";

        // Filter tables if specified
        let tables_to_import: Vec<_> = if let Some(table_filter) = &request.tables {
            session
                .tables
                .iter()
                .filter(|t| table_filter.contains(&t.table_name))
                .collect()
        } else {
            session.tables.iter().collect()
        };

        // Import each table
        for table in &tables_to_import {
            info!("  Importing table: {}", table.table_name);

            // Get demo data for this table (in production, query actual data source)
            let demo_rows = self.get_demo_data_for_table(&session.source_id, &table.table_name);

            // Apply row limit if specified
            let rows_to_import: Vec<_> = if let Some(limit) = request.limit {
                demo_rows.into_iter().take(limit).collect()
            } else {
                demo_rows
            };

            // Count mapped fields in this table
            let mapped_fields: Vec<_> = table
                .field_mappings
                .iter()
                .filter(|f| f.selected_mapping.is_some())
                .collect();

            if mapped_fields.is_empty() {
                warn!("  No mapped fields in table {}, skipping", table.table_name);
                continue;
            }

            stats.fields_mapped += mapped_fields.len();

            // Process each row
            for (row_idx, row) in rows_to_import.iter().enumerate() {
                let entity_id = format!("{}_{}_{}", session.source_id, table.table_name, row_idx);
                let entity_uri = format!("{}entity/{}", gph_ns, entity_id);

                // Entity type triple
                all_triples.push((
                    entity_uri.clone(),
                    format!("{}type", rdf_ns),
                    format!("{}Entity", gph_ns),
                ));

                // Entity ID triple
                all_triples.push((
                    entity_uri.clone(),
                    format!("{}entityId", gph_ns),
                    format!("\"{}\"^^{}string", entity_id, xsd_ns),
                ));

                // Entity type (source table)
                all_triples.push((
                    entity_uri.clone(),
                    format!("{}entityType", gph_ns),
                    format!("\"{}\"^^{}string", table.table_name, xsd_ns),
                ));

                // Provenance: link to mapping session
                all_triples.push((
                    entity_uri.clone(),
                    format!("{}wasDerivedFrom", prov_ns),
                    format!("<{}mapping/session/{}>", gph_ns, session_id),
                ));

                // Generate triples for each mapped field
                for field_mapping in &mapped_fields {
                    if let Some(selected) = &field_mapping.selected_mapping {
                        // Get value from row
                        if let Some(value) = row.get(&field_mapping.field_name) {
                            // Create triple: entity -> ontology term -> value
                            let value_literal = format!("\"{}\"", value.replace('"', "\\\""));

                            all_triples.push((
                                entity_uri.clone(),
                                format!("<{}>", selected.ontology_term_uri),
                                value_literal,
                            ));
                        }
                    }
                }

                stats.rows_processed += 1;
                stats.entities_created += 1;
            }

            stats.tables_imported += 1;
        }

        stats.triples_stored = all_triples.len();

        // Store all triples in target graph
        use crate::governance::rdf_store::{NamedGraph, RdfStore};
        let graph = NamedGraph::new(target_graph.clone());

        info!(
            "Storing {} triples in graph: {}",
            all_triples.len(),
            target_graph
        );

        self.rdf_store
            .insert_triples(all_triples, Some(&graph))
            .context("Failed to store entity triples")?;

        // Ensure at least 1ms is reported even for very fast operations
        let processing_time_ms = (start.elapsed().as_millis() as u64).max(1);

        info!(
            "✓ Import complete: {} entities, {} triples, {} tables ({}ms)",
            stats.entities_created, stats.triples_stored, stats.tables_imported, processing_time_ms
        );

        Ok(ImportDataResponse {
            import_id,
            session_id: session_id.to_string(),
            status: ImportStatus::Completed,
            stats,
            processing_time_ms,
            target_graph,
        })
    }

    /// Get demo data for a table (placeholder for actual data source query)
    fn get_demo_data_for_table(
        &self,
        _source_id: &str,
        table_name: &str,
    ) -> Vec<std::collections::HashMap<String, String>> {
        // This is a placeholder for demo purposes
        // In a real implementation, this would query the actual data source
        use std::collections::HashMap;

        match table_name {
            "customers" => vec![
                {
                    let mut row = HashMap::new();
                    row.insert("customer_id".to_string(), "1001".to_string());
                    row.insert(
                        "customer_email".to_string(),
                        "alice@example.com".to_string(),
                    );
                    row.insert("full_name".to_string(), "Alice Smith".to_string());
                    row
                },
                {
                    let mut row = HashMap::new();
                    row.insert("customer_id".to_string(), "1002".to_string());
                    row.insert("customer_email".to_string(), "bob@example.com".to_string());
                    row.insert("full_name".to_string(), "Bob Johnson".to_string());
                    row
                },
                {
                    let mut row = HashMap::new();
                    row.insert("customer_id".to_string(), "1003".to_string());
                    row.insert(
                        "customer_email".to_string(),
                        "charlie@example.com".to_string(),
                    );
                    row.insert("full_name".to_string(), "Charlie Brown".to_string());
                    row
                },
                {
                    let mut row = HashMap::new();
                    row.insert("customer_id".to_string(), "1004".to_string());
                    row.insert(
                        "customer_email".to_string(),
                        "diana@example.com".to_string(),
                    );
                    row.insert("full_name".to_string(), "Diana Prince".to_string());
                    row
                },
                {
                    let mut row = HashMap::new();
                    row.insert("customer_id".to_string(), "1005".to_string());
                    row.insert(
                        "customer_email".to_string(),
                        "edward@example.com".to_string(),
                    );
                    row.insert("full_name".to_string(), "Edward Norton".to_string());
                    row
                },
            ],
            _ => vec![],
        }
    }
}

fn derive_parquet_table_name(source_id: &str, parquet_path: &str) -> String {
    let candidate = Path::new(parquet_path)
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(source_id);

    let normalized: String = candidate
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect();

    let normalized = normalized.trim_matches('_').to_string();
    if normalized.is_empty() {
        source_id.replace('-', "_")
    } else {
        normalized
    }
}

fn discover_fields_from_parquet_path(
    parquet_path: &str,
    sample_size: usize,
) -> Result<Vec<SchemaFieldInput>> {
    let mut file = File::open(parquet_path)
        .with_context(|| format!("Failed to open managed Parquet file {}", parquet_path))?;
    let metadata = arrow2::io::parquet::read::read_metadata(&mut file)
        .with_context(|| format!("Failed to read Parquet metadata from {}", parquet_path))?;
    let schema = arrow2::io::parquet::read::infer_schema(&metadata)
        .with_context(|| format!("Failed to infer Parquet schema for {}", parquet_path))?;
    let sample_rows =
        crate::workflows::dataset_input::read_parquet_rows(parquet_path, Some(sample_size))?;

    let mut fields = Vec::with_capacity(schema.fields.len());

    for field in &schema.fields {
        let mut sample_values = Vec::new();
        let mut saw_null = false;

        for row in &sample_rows {
            let value = row
                .as_object()
                .and_then(|object| object.get(&field.name))
                .unwrap_or(&JsonValue::Null);

            if value.is_null() {
                saw_null = true;
                continue;
            }

            if sample_values.len() < 10 {
                sample_values.push(json_value_to_sample_string(value));
            }
        }

        fields.push(SchemaFieldInput {
            name: field.name.clone(),
            data_type: parquet_data_type_to_schema_type(field.data_type()),
            nullable: field.is_nullable || saw_null,
            sample_values: Some(sample_values),
            description: None,
        });
    }

    Ok(fields)
}

fn parquet_data_type_to_schema_type(data_type: &arrow2::datatypes::DataType) -> String {
    use arrow2::datatypes::DataType;

    match data_type {
        DataType::Utf8 | DataType::LargeUtf8 => "VARCHAR",
        DataType::Int8
        | DataType::Int16
        | DataType::Int32
        | DataType::UInt8
        | DataType::UInt16
        | DataType::UInt32 => "INTEGER",
        DataType::Int64 | DataType::UInt64 => "BIGINT",
        DataType::Float32 => "FLOAT",
        DataType::Float64 => "DOUBLE",
        DataType::Boolean => "BOOLEAN",
        DataType::Date32 | DataType::Date64 => "DATE",
        DataType::Timestamp(_, _) => "TIMESTAMP",
        DataType::Time32(_) | DataType::Time64(_) => "TIME",
        DataType::Decimal(_, _) => "DECIMAL",
        DataType::Binary | DataType::LargeBinary | DataType::FixedSizeBinary(_) => "BINARY",
        DataType::List(_)
        | DataType::LargeList(_)
        | DataType::Struct(_)
        | DataType::Map(_, _)
        | DataType::Dictionary(_, _, _)
        | DataType::Union(_, _, _) => "JSON",
        _ => "VARCHAR",
    }
    .to_string()
}

fn json_value_to_sample_string(value: &JsonValue) -> String {
    match value {
        JsonValue::String(text) => text.clone(),
        _ => value.to_string(),
    }
}

/// Normalize a field name (lowercase, remove special chars)
fn normalize_name(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphica_core::catalog::api_types::QueryResult;
    use serde_json::json;

    #[test]
    fn test_normalize_name() {
        assert_eq!(normalize_name("customer_email"), "customeremail");
        assert_eq!(normalize_name("Customer-Email"), "customeremail");
        assert_eq!(normalize_name("CUST_EMAIL_ADDR"), "custemailaddr");
    }

    #[test]
    fn test_derive_parquet_table_name_sanitizes_file_stem() {
        let table_name =
            derive_parquet_table_name("ds_import_123", "/tmp/Customer Feed 2026-03.parquet");

        assert_eq!(table_name, "Customer_Feed_2026_03");
    }

    #[test]
    fn test_discover_fields_from_parquet_path_reads_schema_and_samples() {
        let parquet_path = std::env::temp_dir().join(format!(
            "mapping_discovery_{}.parquet",
            uuid::Uuid::new_v4()
        ));
        let parquet_path_str = parquet_path.to_string_lossy().to_string();
        let query_result = QueryResult {
            rows: vec![
                json!({"customer_id": "C001", "lifetime_value": 123.45, "active": true}),
                json!({"customer_id": "C002", "lifetime_value": 456.78, "active": false}),
            ],
            row_count: 2,
            execution_time_ms: 1,
            truncated: false,
            columns: None,
        };

        crate::api::handlers::datasets::write_query_result_to_parquet(
            &query_result,
            &parquet_path_str,
        )
        .expect("parquet write should succeed");

        let fields =
            discover_fields_from_parquet_path(&parquet_path_str, 10).expect("schema discovery");

        let customer_id = fields
            .iter()
            .find(|field| field.name == "customer_id")
            .expect("customer_id field");
        let active = fields
            .iter()
            .find(|field| field.name == "active")
            .expect("active field");

        assert_eq!(customer_id.data_type, "VARCHAR");
        assert!(customer_id
            .sample_values
            .clone()
            .unwrap_or_default()
            .contains(&"C001".to_string()));
        assert_eq!(active.data_type, "BOOLEAN");

        let _ = std::fs::remove_file(parquet_path);
    }
}
