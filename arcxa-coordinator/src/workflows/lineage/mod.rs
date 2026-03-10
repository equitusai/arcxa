//! Workflow Lineage Tracking
//!
//! RDF-based lineage tracking for workflow executions, transformations, and ML predictions.
//! This module implements field-level provenance tracking with W3C PROV ontology extensions.
//!
//! ## Status: Phase 3.3 - In Progress
//!
//! This module contains the RDF-first lineage tracking implementation.
//!
//! ## Completed ✅
//! - RdfTriple type defined in governance module
//! - insert_batch() method implemented
//! - Shard integration verified
//! - WorkflowLineageGenerator implemented
//! - CoordinatorLineageTracker bridges to workflow executor
//!
//! ## Current Work
//! - Adding workflow execution RDF generation methods
//! - End-to-end integration testing
//!
//! See docs/PHASE3_PROGRESS_SESSION2.md for latest progress.

pub mod execution_sync;
pub mod rdf;
pub mod tracker_impl; // Phase 3.1: Unified RDF + execution graph architecture

pub use execution_sync::ExecutionStateSynchronizer;
pub use rdf::{FieldModification, WorkflowLineageGenerator};
pub use tracker_impl::CoordinatorLineageTracker;
