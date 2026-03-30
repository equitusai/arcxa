//! Workflow Domain Types
//!
//! Core business logic types for the workflow routing system.

mod action;
mod approval;
mod batch_job;
mod cdc;
mod condition;
mod data_source;
mod data_source_reader;
mod execution;
mod execution_mode;
mod route;
mod schedule;
mod scheduler;
mod workflow;

pub use action::{Action, ActionId, ActionResult, ActionStatus};
pub use approval::{ApprovalRequest, ApprovalStatus};
pub use batch_job::{
    BatchJob, BatchJobConfig, BatchJobProgress, BatchJobStatus, ResourceLimits, TransactionMode,
    TransactionSummaryInfo, WorkflowExecutionRef, WorkflowExecutionStatus,
};
pub use cdc::{CdcOperation, CdcSource, DebeziumEvent};
pub use condition::Condition;
pub use data_source::{DataSource, DatabaseConnectionConfig, DatabaseType};
pub use data_source_reader::{
    create_reader, CsvFileReader, DataRow, DataSourceReader, DataStream, DatabaseQueryReader,
    S3ObjectReader, SourceMetadata,
};
pub use execution::{
    ExecutionFilters, ExecutionLog, ExecutionRuntimeMetricsSummary, ExecutionStatus, LogLevel,
    PersistedStepResult, WorkflowExecution,
};
pub use execution_mode::{
    AutoScalingConfig, ExecutionMode, MicroBatchConfig, ResourceEstimate, StateBackendConfig,
    StreamingConfig, WatermarkStrategy,
};
pub use route::{Route, RouteId};
pub use schedule::{validate_cron_expression, validate_timezone, WorkflowSchedule};
pub use scheduler::{calculate_next_execution, update_next_run};
pub use workflow::{Workflow, WorkflowId, WorkflowResourceLimits, WorkflowSummary};
