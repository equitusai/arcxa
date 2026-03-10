// Declarative workflow support for GitOps workflow-as-code
//
// This module provides shared types and traits for declarative workflows.
// Actual parsing and deployment logic lives in graphica-coordinator.

pub mod errors;
pub mod schema;
pub mod testing;
pub mod validation;
pub mod validators;

pub use errors::*;
pub use schema::*;
pub use testing::*;
pub use validation::*;
pub use validators::*;
