//! OpenLineage Integration
//!
//! This module implements the OpenLineage 1.0.0 specification for lineage event interchange.
//!
//! OpenLineage is an open standard for lineage metadata collection and analysis.
//! It provides a common framework for capturing lineage information from various
//! data processing systems in a standardized format.
//!
//! ## Key Concepts
//!
//! - **Events**: The core unit of lineage information. Events track the execution of jobs.
//! - **Runs**: A specific execution instance of a job, identified by a unique run ID.
//! - **Jobs**: Named, repeatable data transformation processes.
//! - **Datasets**: Data sources and sinks consumed/produced by jobs.
//! - **Facets**: Extension mechanism for adding rich metadata to any of the above.
//!
//! ## Example
//!
//! ```
//! use graphica_core::openlineage::{
//!     OpenLineageEvent, EventType, Dataset,
//!     facets::{SchemaDatasetFacet, SchemaField},
//! };
//!
//! // Create an event for a completed job
//! let mut event = OpenLineageEvent::new(
//!     EventType::Complete,
//!     "550e8400-e29b-41d4-a716-446655440000".to_string(),
//!     "my-scheduler".to_string(),
//!     "etl.process_orders".to_string(),
//!     "https://github.com/my-org/my-pipeline".to_string(),
//! );
//!
//! // Add input dataset with schema
//! let schema_fields = vec![
//!     SchemaField::new("order_id".to_string(), "INTEGER".to_string()),
//!     SchemaField::new("customer_id".to_string(), "INTEGER".to_string()),
//!     SchemaField::new("total".to_string(), "DECIMAL".to_string()),
//! ];
//!
//! let schema_facet = SchemaDatasetFacet::new(
//!     "graphica".to_string(),
//!     schema_fields,
//! );
//!
//! let input = Dataset::new(
//!     "postgres://prod".to_string(),
//!     "public.orders".to_string(),
//! ).with_facet("schema".to_string(), serde_json::to_value(schema_facet).unwrap());
//!
//! event = event.with_input(input);
//!
//! // Serialize to JSON for transmission
//! let json = serde_json::to_string_pretty(&event).unwrap();
//! println!("{}", json);
//! ```
//!
//! ## Specification Compliance
//!
//! This implementation follows the OpenLineage 1.0.0 specification:
//! - Event schema: https://openlineage.io/spec/1-0-0/OpenLineage.json
//! - Standard facets: https://openlineage.io/spec/facets/
//!
//! ## Integration with Graphica
//!
//! Graphica's internal lineage model can be converted to OpenLineage events,
//! allowing lineage data to be exported to OpenLineage-compatible systems like:
//! - Marquez
//! - Egeria
//! - DataHub
//! - Atlan
//!
//! See the `converter` module for conversion utilities.

pub mod client;
pub mod converter;
pub mod event;
pub mod facets;

// Re-export main types for convenience
pub use client::{OpenLineageClient, OpenLineageClientConfig};
pub use converter::LineageConverter;
pub use event::{Dataset, EventType, Job, OpenLineageEvent, Run};
pub use facets::Facet;
