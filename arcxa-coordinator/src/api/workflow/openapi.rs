//! OpenAPI documentation for Workflow Orchestration API
//!
//! This module aggregates all workflow management, execution, and scheduling endpoints
//! into a single OpenAPI specification.

use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    paths(
        // Workflow CRUD operations
        crate::api::workflow::handlers::register_workflow,
        crate::api::workflow::handlers::list_workflows,
        crate::api::workflow::handlers::get_workflow,
        crate::api::workflow::handlers::get_workflow_details,
        crate::api::workflow::handlers::update_workflow,
        crate::api::workflow::handlers::delete_workflow,
        // Workflow execution
        crate::api::workflow::handlers::execute_workflow,
        // Workflow validation & testing
        crate::api::workflow::handlers::validate_workflow_definition,
        crate::api::workflow::handlers::test_workflow_step,
        crate::api::workflow::handlers::dry_run_workflow,
        // Workflow scheduling (plural endpoints)
        crate::api::workflow::handlers::create_schedule,
        crate::api::workflow::handlers::list_schedules,
        crate::api::workflow::handlers::get_schedule,
        crate::api::workflow::handlers::update_schedule,
        crate::api::workflow::handlers::delete_schedule,
        // Execution history
        crate::api::workflow::handlers::list_workflow_executions,
        // Phase 3: Progress monitoring and cancellation
        crate::api::workflow::handlers::get_execution_progress,
        crate::api::workflow::handlers::list_workflow_execution_progress,
        crate::api::workflow::handlers::get_active_executions,
        crate::api::workflow::handlers::cancel_execution,
    ),
    components(
        schemas(
            // Registration types
            crate::api::workflow::types::RegisterWorkflowRequest,
            crate::api::workflow::types::RegisterWorkflowResponse,
            crate::api::workflow::types::WorkflowDetailsResponse,
            // Execution types
            crate::api::workflow::types::ExecuteWorkflowRequest,
            crate::api::workflow::types::ExecuteWorkflowResponse,
            crate::api::workflow::types::ExecutionResultDto,
            crate::api::workflow::types::StepResultDto,
            crate::api::workflow::types::WorkflowInputWrapper,
            crate::api::workflow::types::ExecutionContextParams,
            // Query types
            crate::api::workflow::types::WorkflowSummaryDto,
            // Testing/Validation types
            crate::api::workflow::types::TestWorkflowStepRequest,
            crate::api::workflow::types::TestWorkflowStepResponse,
            crate::api::workflow::types::DryRunWorkflowRequest,
            crate::api::workflow::types::DryRunWorkflowResponse,
            crate::api::workflow::types::StepExecutionResult,
            // Scheduling types
            crate::api::workflow::types::ScheduleWorkflowRequest,
            crate::api::workflow::types::UpdateScheduleRequest,
            crate::api::workflow::types::ScheduleWorkflowResponse,
            crate::api::workflow::types::WorkflowScheduleInfo,
            // Execution history types
            crate::api::workflow::types::WorkflowExecutionSummary,
            // Phase 3: Progress monitoring types
            graphica_core::orchestration::workflow::progress::WorkflowProgress,
            graphica_core::orchestration::workflow::progress::StepProgress,
            graphica_core::orchestration::workflow::progress::ExecutionStatus,
            // External types from graphica_core
            graphica_core::orchestration::workflow::WorkflowDefinition,
            graphica_core::orchestration::workflow::WorkflowStep,
            graphica_core::orchestration::workflow::WorkflowInput,
            // Re-exported workflow definition types
            crate::api::workflow::types::FallbackStrategy,
            crate::api::workflow::types::StepType,
            crate::api::workflow::types::StepConfig,
            // Step config variants
            crate::api::workflow::types::MLPredictionConfig,
            crate::api::workflow::types::HeuristicConfig,
            crate::api::workflow::types::WasmRuleConfig,
            crate::api::workflow::types::ConfidenceGateConfig,
            crate::api::workflow::types::WeightedVoteConfig,
            crate::api::workflow::types::ConfidenceAggregateConfig,
            crate::api::workflow::types::CsvSourceConfig,
            crate::api::workflow::types::DbExtractConfig,
            crate::api::workflow::types::FieldTransformerConfig,
            crate::api::workflow::types::DbLoaderConfig,
            crate::api::workflow::types::RdfLoaderConfig,
            crate::api::workflow::types::DataValidatorConfig,
            crate::api::workflow::types::DataJoinerConfig,
            crate::api::workflow::types::SemanticMapperConfig,
            crate::api::workflow::types::DeduplicatorConfig,
            crate::api::workflow::types::AggregatorConfig,
            crate::api::workflow::types::CsvExporterConfig,
            // Nested types
            crate::api::workflow::types::FeatureMapping,
            crate::api::workflow::types::PredictionSpec,
            crate::api::workflow::types::FieldTransformation,
            crate::api::workflow::types::TransformOperation,
            crate::api::workflow::types::LoadMode,
            crate::api::workflow::types::ValidationRule,
            crate::api::workflow::types::RuleType,
            crate::api::workflow::types::Severity,
            crate::api::workflow::types::JoinType,
            crate::api::workflow::types::MappingMode,
            crate::api::workflow::types::DedupMethod,
            crate::api::workflow::types::FuzzyAlgorithm,
            crate::api::workflow::types::KeepStrategy,
            crate::api::workflow::types::Aggregation,
            crate::api::workflow::types::AggFunction,
        )
    ),
    tags(
        (name = "Workflow Orchestration", description = "Workflow lifecycle management, graph-native SPARQL execution, scheduling with cron expressions and IANA timezone support"),
    ),
    info(
        title = "ARCXA Workflow Orchestration API",
        version = "1.0.0",
        description = "REST API for workflow management with SPARQL-first data targeting, scheduled execution, and RDF/PROV-based lineage tracking",
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
pub struct WorkflowApiDoc;
