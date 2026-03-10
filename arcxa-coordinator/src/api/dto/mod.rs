//! Data Transfer Objects (DTOs)
//!
//! Request and response types for the REST API, organized by domain.

// Common DTOs (errors, health checks)
pub mod audit;
pub mod common;
pub mod connectors;
pub mod datasets; // Dataset import DTOs
pub mod entities;
pub mod fusion;
pub mod lineage;
pub mod models;
pub mod quality;
pub mod rdf;
pub mod sparql;
pub mod temporal;
pub mod wal;

// Re-export common types for convenience
pub use common::{
    ApiError, ApiErrorResponse, ComponentHealth, HealthResponse, ReadinessResponse,
    StorageHealthResponse,
};

// Re-export entity types
pub use entities::{
    AttributeDatapoint, AttributeTimeseriesResponse, DatasetColumnDto, DatasetLineage,
    DatasetListQuery, DatasetListResponse, DatasetResponse, DatasetSummary, DerivedAttribute,
    EntityAttributesResponse, EntityListResponse, EntityResponse, EntitySummary,
};

// Re-export dataset import types
pub use datasets::{
    ColumnDefinition, DatasourceImportRequest, ImportDatasetResponse, ImportError,
    ImportErrorResponse, ImportLineage, ImportMetadata, ImportStatus, ImportStatusResponse,
    ImportSummary, IncrementalImportConfig, ListImportsQuery, ListImportsResponse,
    SchemaDefinition, StorageInfo,
};

// Re-export fusion types
pub use fusion::{
    FusionCandidate, FusionCandidateListResponse, FusionCandidateQuery, FusionResolveRequest,
    FusionResolveResponse, ProposeFusionRequest, ProposeFusionResponse, ReverseFusionRequest,
    ReverseFusionResponse, ReviewCandidateRequest, ReviewCandidateResponse,
};

// Re-export lineage types
pub use lineage::{
    AsOfQuery, BackwardAnalysisQuery, EntityLineageResponse, ForwardImpactQuery,
    LineageQueryRequest, LineageResponse, ModelImpactQuery, ModelImpactResponse, ModelLineageGraph,
    WriteLineageResponse,
};

// Re-export model types
pub use models::{
    FeatureSchemaDto, ModelEndpointDto, ModelResponse, ModelSummaryResponseDto, Prediction,
    PredictionsResponse, RecordPredictionsRequest, RegisterModelRequest,
    RegisterOrchestrationModelRequest,
};

// Re-export quality types
pub use quality::{
    CreateRuleRequest, LoadRuleRequest, RuleResponse, ScorecardQuery, ScorecardResponse,
    ViolationListResponse, ViolationQuery,
};

// Re-export SPARQL types
pub use sparql::{SparqlQuery, SparqlResultRow, SparqlResults};

// Re-export temporal types
pub use temporal::{CheckpointRequest, CheckpointResponse, TemporalSummaryResponse};

// Re-export WAL types
pub use wal::{WalOperation, WalOperationsResponse, WalReplayResponse, WalStatusResponse};

// Re-export audit types
pub use audit::AuditQueryRequest;

// Re-export connector types
pub use connectors::{
    ConfigFieldResponse, ConnectorCapabilitiesResponse, ConnectorListResponse,
    ConnectorMetadataResponse, ConnectorOperationResponse, ConnectorStatisticsResponse,
    CredentialFieldResponse,
};

// Re-export RDF types
pub use rdf::{RdfAutoSaveStatsResponse, RdfSaveResponse};

// DTO modules will be added incrementally during refactoring
// pub mod models;
// pub mod entities;
// pub mod fusion;
// pub mod sparql;
// pub mod temporal;
// pub mod wal;
// pub mod auth;
// pub mod audit;
// pub mod connectors;
