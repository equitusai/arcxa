//! Security utilities for Graphica
//!
//! This module provides security-related functionality including:
//! - SQL injection prevention
//! - Input validation
//! - Identifier sanitization

pub mod sql_validation;

pub use sql_validation::{
    quote_identifier, validate_fk_action, validate_identifier, validate_qualified_identifier,
    validate_sql_type, DatabaseType,
};
