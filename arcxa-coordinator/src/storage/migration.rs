//! # Storage Migration Module
//!
//! Re-exports from the standalone graphica-migrations crate.
//!
//! This module was extracted to reduce build dependencies for CLI tools.

// Re-export all public items from graphica-migrations
pub use graphica_migrations::{
    get_migration_status, migrate_column_family, migrate_database, set_migration_status,
    MigrationStatus,
};
