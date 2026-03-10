//! Graphica Core Library
//!
//! Shared domain types, business logic, and proto definitions.
//! This crate has NO storage dependencies (no RocksDB, no Oxigraph).
//!
//! ## Architecture
//!
//! This is the foundation layer shared by:
//! - `graphica-shard`: RDF shard server (Oxigraph + RocksDB backend)
//! - `graphica-coordinator`: Main application (temporal indexes + APIs)
//!
//! ## Key Principles
//!
//! 1. **Storage-agnostic**: All storage implementations in separate crates
//! 2. **Pure domain logic**: Business rules, types, conversions
//! 3. **Proto definitions**: Shared gRPC interfaces
//! 4. **No dep conflicts**: Enables monorepo to avoid RocksDB version conflicts

// Error types
pub mod errors;

// Core domain types
pub mod core;

// Data profiling (HyperLogLog, sketches)
pub mod profiling;

// Checkpointing abstractions
pub mod checkpointing;

// Data ingestion (Kafka, dataflow)
pub mod ingestion;

// Observability (tracing, metrics)
pub mod observability;

// Orchestration (rule DAG, ML integration)
pub mod orchestration;

// Reliability (circuit breakers, retries)
pub mod reliability;

// Security (SQL injection prevention, input validation)
pub mod security;

// Distributed system types and proto
pub mod distributed;

// Data source catalog
pub mod catalog;

// Schema inference (multi-tier metadata discovery)
pub mod inference;

// Declarative workflow support (GitOps workflow-as-code)
pub mod workflows;

// Unified schema representation (universal type system)
pub mod schema;

// Secret management
pub mod secrets;

// Machine Learning (embeddings, semantic classification)
pub mod ml;

// GDPR compliance (data erasure, consent, data portability)
pub mod gdpr;

// OpenLineage integration (lineage event interchange format)
pub mod openlineage;

// Re-export commonly used types
pub use errors::GraphicaError;

// Re-export inference types for convenience
pub use inference::{
    orchestrator::SchemaInferenceOrchestrator,
    types::{InferenceJob, InferenceTier, SchemaMetadata},
};

// Re-export unified schema types for convenience
pub use schema::{
    ConversionResult, FieldProfile, SourceType, TypeConverter, UnifiedField, UnifiedSchema,
    UniversalDataType,
};

// Re-export ML types for convenience
// TODO: Restore when ML dependencies are stable
// pub use ml::{EmbeddingService, Embedding, EmbeddingError};
