//! Semantic type detection system
//!
//! This module provides a sophisticated, multi-strategy semantic type detection
//! framework that can identify semantic types (email, phone, SSN, etc.) from
//! column metadata and sample values.
//!
//! ## Architecture
//!
//! The detection system is built on a plugin-like architecture with multiple
//! independent detection strategies:
//!
//! - **Column Name Detector**: Analyzes column names using pattern matching
//! - **Regex Detector**: Matches values against regex patterns
//! - **Statistical Detector**: Uses statistical properties (cardinality, distribution)
//! - **Composite Detector**: Combines multiple strategies with evidence scoring
//!
//! ## Usage Example
//!
//! ```rust,no_run
//! use graphica_core::inference::semantic::*;
//!
//! # async fn example() -> anyhow::Result<()> {
//! // Create detection context
//! let mut context = DetectionContext::new("email_address", "varchar");
//! context.sample_values = vec![
//!     "john@example.com".to_string(),
//!     "jane@test.org".to_string(),
//! ];
//!
//! // Run detection
//! let detector = ColumnNameDetector::new();
//! if let Some(result) = detector.detect(&context).await? {
//!     println!("Detected: {:?} with confidence {}",
//!              result.semantic_type, result.confidence);
//! }
//! # Ok(())
//! # }
//! ```

pub mod column_name;
pub mod registry;
pub mod strategy;
pub mod types;

// Re-exports
pub use column_name::ColumnNameDetector;
pub use registry::{
    DetectionMethod, DetectorMetadata, RegistryStatistics, SemanticDetectionRegistry,
};
pub use strategy::{CompositeStrategy, DetectionStrategy};
pub use types::{
    AggregatedDetection, AggregationMethod, DetectionContext, DetectionEvidence, DetectionResult,
    DetectionStatistics, EvidenceType,
};
