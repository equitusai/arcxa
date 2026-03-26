//! Canonical workflow runtime substrate.
//!
//! This module is the landing zone for performance-sensitive workflow execution
//! code. It introduces batch-oriented runtime primitives without forcing an
//! immediate rewrite of the legacy JSON-row execution paths.

pub mod frame;
pub mod lineage;
pub mod metrics;
pub mod operators;
pub mod planner;
pub mod spill;
