//! OpenAPI documentation for Lineage API
//!
//! This module aggregates all lineage endpoints into a single OpenAPI specification.

use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    paths(
        // Row-level lineage endpoints
        crate::api::lineage::row_handlers::search_row_keys,
        crate::api::lineage::row_handlers::get_row_lineage,
        crate::api::lineage::row_handlers::get_row_journey,
        crate::api::lineage::row_handlers::get_batch_lineage,
        crate::api::lineage::row_handlers::get_job_stats,
        crate::api::lineage::row_handlers::get_filtered_rows,
        // Workflow lineage endpoints
        crate::api::lineage::handlers::get_run_lineage,
        crate::api::lineage::handlers::get_record_lineage,
        crate::api::lineage::handlers::get_lineage_graph,
        crate::api::lineage::handlers::get_model_impact,
        crate::api::lineage::handlers::query_lineage_by_time_range,
        // Column-level lineage endpoints
        crate::api::lineage::column_handlers::get_column_lineage,
        crate::api::lineage::column_handlers::get_column_graph,
        crate::api::lineage::column_handlers::analyze_column_impact,
        crate::api::lineage::column_handlers::get_derived_columns,
        // Schema evolution endpoints
        crate::api::lineage::schema_handlers::record_schema_change,
        crate::api::lineage::schema_handlers::get_datasource_schema_changes,
        crate::api::lineage::schema_handlers::get_table_schema_changes,
        crate::api::lineage::schema_handlers::save_schema_version,
        crate::api::lineage::schema_handlers::get_latest_schema_version,
        crate::api::lineage::schema_handlers::analyze_schema_drift,
        crate::api::lineage::schema_handlers::analyze_migration_impact,
    ),
    components(
        schemas(
            // Row-level lineage types
            crate::api::lineage::types::RowKeySearchQuery,
            crate::api::lineage::types::RowKeySearchMatch,
            crate::api::lineage::types::RowKeySearchResponse,
            crate::api::lineage::types::RowLineageResponse,
            crate::api::lineage::types::BatchLineageResponse,
            crate::api::lineage::types::FilteredRowsQuery,
            crate::api::lineage::types::FilteredRowsResponse,
            crate::api::lineage::types::FilteredRow,
            // Workflow lineage types
            crate::api::lineage::types::RunLineageResponse,
            crate::api::lineage::types::LineageRecordResponse,
            crate::api::lineage::types::LineageGraphResponse,
            crate::api::lineage::types::LineageStatistics,
            crate::api::lineage::types::ModelImpactResponse,
            crate::api::lineage::types::AffectedRecordDto,
            crate::api::lineage::types::TimeRangeLineageQuery,
            crate::api::lineage::types::TimeRangeLineageResponse,
            crate::api::lineage::types::DataRefDto,
            crate::api::lineage::types::CdcPositionDto,
            crate::api::lineage::types::TransformDto,
            crate::api::lineage::types::ModelDto,
            crate::api::lineage::types::ModelMetricsDto,
            // Types re-exported from graphica-core (with ToSchema derives)
            crate::api::lineage::types::RowLineageEvent,
            crate::api::lineage::types::RowJourney,
            crate::api::lineage::types::JobStatistics,
            crate::api::lineage::types::JourneyStep,
            crate::api::lineage::types::ProcessingOutcome,
            crate::api::lineage::types::RowTransformation,
            crate::api::lineage::types::QualityViolation,
            crate::api::lineage::types::RowId,
            crate::api::lineage::types::SourceType,
            crate::api::lineage::types::DatabaseType,
            crate::api::lineage::types::RowPosition,
            // Column-level lineage types
            graphica_core::core::lineage::column_level::ColumnRef,
            graphica_core::core::lineage::column_level::ColumnLineageEvent,
            graphica_core::core::lineage::column_level::ColumnLineageGraph,
            graphica_core::core::lineage::column_level::ColumnLineageStatistics,
            graphica_core::core::lineage::column_level::ColumnImpactAnalysis,
            graphica_core::core::lineage::column_level::TransformationType,
            // Schema evolution types
            graphica_core::core::lineage::schema_evolution::SchemaChangeEvent,
            graphica_core::core::lineage::schema_evolution::SchemaChangeType,
            graphica_core::core::lineage::schema_evolution::SchemaElement,
            graphica_core::core::lineage::schema_evolution::SchemaElementType,
            graphica_core::core::lineage::schema_evolution::SchemaVersion,
            graphica_core::core::lineage::schema_evolution::TableSchema,
            graphica_core::core::lineage::schema_evolution::ColumnSchema,
            graphica_core::core::lineage::schema_evolution::ForeignKeySchema,
            graphica_core::core::lineage::schema_evolution::IndexSchema,
            graphica_core::core::lineage::schema_evolution::SchemaDriftAnalysis,
            graphica_core::core::lineage::schema_evolution::DriftSeverity,
            graphica_core::core::lineage::schema_evolution::MigrationImpactAnalysis,
            graphica_core::core::lineage::schema_evolution::RiskLevel,
        )
    ),
    tags(
        (name = "Row-Level Lineage", description = "Fine-grained row-level lineage tracking for ETL operations"),
        (name = "Workflow Lineage", description = "Workflow run lineage and provenance tracking"),
        (name = "Column-Level Lineage", description = "Column-to-column lineage tracking and dependency analysis"),
        (name = "Schema Evolution", description = "Schema change tracking, drift analysis, and migration impact assessment"),
    ),
    info(
        title = "ARCXA Lineage API",
        version = "1.0.0",
        description = "REST API for querying W3C PROV-compliant lineage data",
        contact(
            name = "ARCXA Team",
            email = "avinam@equitus.us"
        )
    ),
    servers(
        (url = "http://localhost:8080", description = "Development server"),
        (url = "https://api.graphica.io", description = "Production server")
    )
)]
pub struct LineageApiDoc;
