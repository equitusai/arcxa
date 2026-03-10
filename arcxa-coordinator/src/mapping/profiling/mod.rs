//! # Source Profiling Module
//!
//! Automated discovery and profiling of CSV/Parquet data sources with RDF serialization.
//!
//! ## Architecture
//!
//! This module wraps the existing `IncrementalProfiler` from graphica-core and extends
//! it with RDF serialization using DCAT (Data Catalog) and VoID (Vocabulary of Interlinked
//! Datasets) vocabularies.
//!
//! ## Workflow
//!
//! ```text
//! CSV/Parquet File
//!     ↓
//! AsyncCsvReader (streaming)
//!     ↓
//! IncrementalProfiler (statistical analysis)
//!     ↓
//! DatasetProfile
//!     ↓
//! RDF Serialization (DCAT/VoID)
//!     ↓
//! Governance Brain (RDF Store)
//! ```
//!
//! ## Usage
//!
//! ```ignore
//! use graphica_coordinator::mapping::profiling::{SourceProfiler, ProfileConfig};
//!
//! let profiler = SourceProfiler::new(governance_brain, config);
//! let profile = profiler.profile_csv("data/customers.csv").await?;
//! let dataset_uri = profiler.store_profile(profile).await?;
//! ```

pub mod feature_extraction;
pub mod profiler;
pub mod rdf;
pub mod types;

pub use feature_extraction::SchemaIntelligence;
pub use profiler::SourceProfiler;
pub use types::{ColumnProfile, ProfileConfig, ProfileResult};
