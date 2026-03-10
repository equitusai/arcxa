//! Data Source Catalog
//!
//! Provides data source registration, discovery, and metadata management.
//! Integrates with the RDF graph for governance and lineage tracking.

pub mod api_types;
pub mod client;
pub mod connector;
pub mod connector_v2; // Phase 2: V2 connector with profiling and streaming
pub mod connectors;
pub mod ontology;
pub mod ontology_extensions; // Phase 1: Semantic type ontology extensions
pub mod ontology_registry; // Phase 1: Custom ontology management
pub mod postgres_tls;
pub mod schema_to_rdf;
pub mod secrets;
pub mod types; // Phase 1: Schema to RDF conversion

pub use api_types::*;
pub use client::{CatalogResult, DataSourceCatalog, UsageStatistics};
pub use connector::{ConnectorCapabilities, Credentials, DataSourceConnector, ValidationResult};
pub use connector_v2::{
    CompressionFormat, CsvExportOptions, DataSourceConnectorV2, DataSourceConnectorV2Adapter,
    DataStream, ExportConfig, ExportFormat, ParquetExportOptions, RowBatch,
};
pub use connectors::ConnectorRegistry;
pub use ontology::{namespaces, DataSourceType, CATALOG_ONTOLOGY};
pub use ontology_extensions::{
    cardinality_class_to_uri, semantic_type_to_uri, EXTENDED_CATALOG_ONTOLOGY,
};
pub use ontology_registry::{
    OntologyMetadata, OntologyRegistry, RegisteredOntology, ValidationStatus,
};
pub use schema_to_rdf::{RdfNode, RdfTriple, SchemaRdfConverter};
pub use secrets::{EnvSecretProvider, InMemorySecretProvider, SecretProvider};
pub use types::*;
