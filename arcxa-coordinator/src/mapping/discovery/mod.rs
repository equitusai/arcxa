//! # Intelligent Schema Discovery System
//!
//! Production-ready schema discovery replacing hardcoded placeholders with:
//! - Real data source introspection via INFORMATION_SCHEMA
//! - Intelligent type inference using pattern recognition
//! - Semantic matching via transformer embeddings
//! - Aggressive caching for sub-second performance
//!
//! ## Architecture
//!
//! ```text
//! ┌──────────────────┐
//! │ MappingEngine    │
//! │ (analyze_schema) │
//! └────────┬─────────┘
//!          │
//!          ▼
//! ┌────────────────────────┐
//! │ DiscoveryOrchestrator  │  ← Main entry point
//! │ - Cache management     │
//! │ - Extractor selection  │
//! └────────┬───────────────┘
//!          │
//!          ├───────────┬──────────────┬─────────────┐
//!          ▼           ▼              ▼             ▼
//!    ┌─────────┐  ┌────────┐   ┌─────────┐  ┌────────┐
//!    │   PG    │  │  Snow  │   │ Oracle  │  │  CSV   │
//!    │Extractor│  │Extractor│   │Extractor│  │Extract │
//!    └─────────┘  └────────┘   └─────────┘  └────────┘
//!          │
//!          ▼
//! ┌──────────────────────┐
//! │ TypeInferenceEngine  │  ← Pattern recognition
//! │ - Email detection    │
//! │ - Phone detection    │
//! │ - Name heuristics    │
//! └──────────────────────┘
//!          │
//!          ▼
//! ┌──────────────────────┐
//! │ SemanticMatcher      │  ← Transformer embeddings
//! │ (existing)           │
//! └──────────────────────┘
//! ```
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
//!     cache_ttl_secs: 3600, // 1 hour
//! };
//!
//! let schema = orchestrator.discover_schema(
//!     &source,
//!     &credentials,
//!     config,
//! ).await?;
//!
//! // Schema now contains:
//! // - Real tables from INFORMATION_SCHEMA
//! // - Columns with inferred semantic types
//! // - Sample values for profiling
//! // - Pattern detection results
//! ```
//!
//! ## Performance
//!
//! Target latencies:
//! - Schema metadata: <100ms (cache: 2ms)
//! - Sample extraction: <500ms (cache: 10ms)
//! - Type inference: <10ms per column
//! - **End-to-end: <1s per table** (cache: <50ms)
//!
//! ## Implementation Status
//!
//! **Phase 1: Foundation** ✅ COMPLETE
//! - ✅ Type system
//! - ✅ SchemaExtractor trait
//! - ✅ DiscoveryCache with RocksDB
//! - ✅ DiscoveryOrchestrator (skeleton)
//! - ✅ TypeInferenceEngine
//!
//! **Current Status**
//! - ✅ PostgreSQLExtractor
//! - ✅ DB2Extractor
//! - ✅ OracleExtractor
//! - ✅ SAPHANAExtractor
//! - ✅ DatabricksExtractor (connector-backed extraction)
//! - ✅ CSV extractor
//!
//! **Next**
//! - ⏳ Snowflake, S3/Parquet deep extraction
//! - ⏳ Performance optimization
//! - ⏳ Advanced caching strategies

pub mod cache;
pub mod cache_warming;
pub mod ddl_bridge;
pub mod extractors;
pub mod inference;
pub mod orchestrator;
pub mod service;
pub mod state;
pub mod types;

pub use cache::DiscoveryCache;
pub use cache_warming::{CacheWarmingCatalog, CacheWarmingCoordinator, DataSourceEvent};
pub use extractors::{
    DB2Extractor, DatabricksExtractor, ExtractorRegistry, OracleExtractor, PostgreSQLExtractor,
    SAPHANAExtractor, SchemaExtractor,
};
pub use inference::TypeInferenceEngine;
pub use orchestrator::DiscoveryOrchestrator;
pub use service::{
    CredentialProvider, CredentialResolver, DiscoveryService, MockDiscoveryService,
    ProductionDiscoveryService,
};
pub use state::{
    DiscoveryProgress, DiscoveryResult, DiscoveryStateManager, DiscoveryStats, DiscoveryStatus,
};
pub use types::*;
