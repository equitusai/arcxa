//! ETL Orchestration Layer
//!
//! Professional orchestration for multi-source CSV-to-database ETL workflows.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                  ETL Orchestration Layer                    │
//! └─────────────────────────────────────────────────────────────┘
//!
//! CSV Sources (Multiple)          Target Database
//! ┌───────────┐                   ┌──────────────┐
//! │customers  │ ──┐               │ PostgreSQL   │
//! │.csv       │   │               │ / DB2        │
//! └───────────┘   │               │ / Oracle     │
//!                 │               └──────────────┘
//! ┌───────────┐   │                      ▲
//! │orders.csv │ ──┤                      │
//! └───────────┘   │                      │
//!                 ▼                      │
//!         ┌───────────────┐              │
//!         │ Unified       │              │
//!         │ Mapping       │              │
//!         │ Session       │              │
//!         └───────┬───────┘              │
//!                 │                      │
//!                 ▼                      │
//!         ┌───────────────┐              │
//!         │ Load          │──────────────┘
//!         │ Orchestrator  │
//!         └───────────────┘
//!                 │
//!                 ▼
//!         ┌───────────────┐
//!         │ Lineage       │
//!         │ Tracking      │
//!         └───────────────┘
//! ```
//!
//! ## Components
//!
//! - **UnifiedMappingSession**: Consolidates multiple CSV mappings to single target schema
//! - **LoadOrchestrator**: Executes the full ETL pipeline with lineage
//! - **ETLLineage**: Field-level lineage tracking (CSV → Ontology → DB)

pub mod lineage;
pub mod load_orchestrator;
pub mod types;
pub mod unified_mapping;

pub use lineage::{EtlLineageTracker, FieldLineage, LineageChain};
pub use load_orchestrator::{LoadOrchestrator, LoadPipeline, LoadStats};
pub use types::*;
pub use unified_mapping::{ConflictResolution, UnifiedMappingCoordinator, UnifiedMappingSession};
