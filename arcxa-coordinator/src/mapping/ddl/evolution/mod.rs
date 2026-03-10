//! Schema Evolution Module
//!
//! Generate idempotent schema migrations by comparing SHACL shapes.

pub mod diff;
pub mod migration;
pub mod versioning;

pub use diff::{SchemaDiff, SchemaDiffEngine};
pub use migration::{MigrationGenerator, MigrationPlan, MigrationStep};
pub use versioning::{
    record_schema_version, InMemorySchemaVersionStore, SchemaHistory, SchemaVersion,
    SchemaVersionStore,
};
