//! Ontology-to-physical binding lifecycle module.
//!
//! Provides versioned binding persistence and retrieval for goal-driven SQL planning.

pub mod service;
pub mod store;
pub mod types;

pub use service::BindingService;
pub use store::BindingStore;
pub use types::{
    BindingCoverageDiff, BindingProvenance, BindingStatus, OntologyPhysicalBinding,
    UpsertBindingRequest,
};
