//! # Unified Semantic Mapping Module
//!
//! Consolidates R2RML and Ontology-DDL into a single semantic mapping architecture
//! supporting both RDF triple generation and SQL DDL generation.
//!
//! ## Architecture
//!
//! ```text
//! CSV/Database Source
//!         ↓
//!     profiling (DCAT)
//!         ↓
//!   ┌─────────────┐
//!   │ semantic/   │
//!   │   core      │  ← Shared ontology mapping
//!   └──────┬──────┘
//!          │
//!    ┌─────┴─────┐
//!    ▼           ▼
//! rdf/        sql/      ← Output adapters
//!    ▼           ▼
//! RDF Store   Database
//! ```
//!
//! ## Phases
//!
//! - **Phase 1** (Current): Foundation - shared types and core logic
//! - **Phase 2**: RDF output adapter (R2RML integration)
//! - **Phase 3**: SQL output adapter (ontology_ddl migration)
//! - **Phase 4**: Hybrid workflows (simultaneous RDF+SQL)
//! - **Phase 5**: Cleanup and deprecation
//!
//! ## Migration Status
//!
//! This module is being built incrementally to consolidate:
//! - `mapping::r2rml` → `semantic::rdf`
//! - `mapping::ontology_ddl` → `semantic::sql`
//!
//! Original modules remain functional during migration.

pub mod bridge; // API backward compatibility
pub mod core; // Shared semantic mapping foundation
pub mod executors;
pub mod rdf; // RDF triple generation (R2RML)
pub mod sql; // SQL DDL generation // Unified execution layer

// Re-export core types for convenience
pub use core::types::*;
