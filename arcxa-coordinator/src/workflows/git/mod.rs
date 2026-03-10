//! Git integration for workflow management
//!
//! Provides Git hooks, file watching, and workflow repository management.

pub mod helpers;
pub mod hooks;
pub mod watcher;

pub use helpers::*;
pub use hooks::*;
pub use watcher::*;
