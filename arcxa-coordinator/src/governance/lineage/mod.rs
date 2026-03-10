//! Lineage Module for Graphica Governance
//!
//! Provides W3C PROV-based lineage tracking with future LE-DAG compatibility.

pub mod rdf_lineage;
pub mod prov;
pub mod ddl_lineage;
pub mod ledag_compat;

pub use rdf_lineage::{
    OntologyDrivenLineage, FieldDefinition, OntologyClass, ShaclShape, DdlStatement,
    RdfLineageService, LineageDescriptor, LineageType,
};

pub use prov::{ProvActivity, ProvEntity, ProvAgent, ProvRelation};

pub use ddl_lineage::{DdlGenerator, DdlDialect, DdlGenerationContext};

pub use ledag_compat::{
    FutureCompatibleLineage, LineageOperation, OperationType,
    LineageStorageAdapter,
};