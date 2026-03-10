//! Metrics module
//!
//! Organized by subsystem for clear separation of concerns:
//! - `registry`: Central metrics registry and initialization
//! - `api`: HTTP API request/response metrics
//! - `rdf`: RDF store and SPARQL query metrics
//! - `shard`: Distributed shard coordination metrics
//! - `system`: System health and resource metrics
//! - `error`: Error tracking and categorization
//! - `loader`: ETL loader operations and performance metrics
//! - `workflow`: Workflow execution and action metrics

pub mod api;
pub mod error;
pub mod loader;
pub mod rdf;
pub mod registry;
pub mod shard;
pub mod system;
pub mod workflow;

pub use api::ApiMetrics;
pub use error::ErrorMetrics;
pub use loader::LoaderMetrics;
pub use rdf::RdfMetrics;
pub use registry::MetricsRegistry;
pub use shard::ShardMetrics;
pub use system::SystemMetrics;
pub use workflow::WorkflowMetrics;
