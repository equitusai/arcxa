//! Workflow Engine - Condition evaluation, routing, and action execution
//!
//! The engine layer is stateless and focuses on pure workflow execution logic.

mod batch_executor;
mod data_loader;
mod evaluator;
mod executor;
mod kafka_source;
mod preflight_validator;
mod production_executor;
mod router;
mod stream_executor;
mod transaction_coordinator;

pub mod transformers;

pub use batch_executor::BatchJobExecutor;
pub use data_loader::{DataLoader, LoadConfig, LoadStats};
pub use evaluator::ConditionEvaluator;
pub use executor::{ActionExecutor, ExecutionContext};
pub use kafka_source::{KafkaRecord, KafkaSource};
pub use preflight_validator::{
    PreflightResult, PreflightValidator, ResourceRequirements, ValidationCheck, ValidationError,
    ValidationWarning,
};
pub use production_executor::ProductionWorkflowExecutor;
pub use router::{RouteMatch, RouteStats, WorkflowRouter};
pub use stream_executor::{StreamExecutor, StreamHandle, StreamStats};
pub use transaction_coordinator::{
    TransactionCoordinator, TransactionInfo, TransactionState, TransactionSummary,
};
