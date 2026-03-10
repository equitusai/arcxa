//! Format Readers Module
//!
//! This module contains implementations of the FormatReader trait for various
//! file formats (CSV, JSON, Parquet, etc.).

pub mod csv;

// Re-export commonly used types
pub use csv::{CsvOptions, CsvReader};
