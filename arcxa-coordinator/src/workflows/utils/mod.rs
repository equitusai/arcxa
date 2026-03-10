//! Utility modules for workflow execution and management.
//!
//! This module contains cross-cutting utilities used throughout the workflow system.

pub mod string_pool;

// Re-export commonly used types for convenience
pub use string_pool::{
    arc_str, arc_str_owned, intern, intern_owned, intern_slice, intern_vec, to_string, ActionType,
    Atom, ExecutionId, FieldName, LargeString, RouteId, StepId, WorkflowId,
};
