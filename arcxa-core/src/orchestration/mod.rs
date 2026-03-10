//! # Rule Orchestration Layer
//!
//! Provides cluster-wide rule management and workflow orchestration for complex
//! governance operations combining ML models, heuristics, and confidence-based
//! decision making.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────┐
//! │  Rule Orchestration Service             │
//! ├─────────────────────────────────────────┤
//! │  - Workflow Engine (DAG execution)      │
//! │  - ML Model Registry & Invoker          │
//! │  - Confidence Aggregation               │
//! │  - Cluster Coordination (Raft)          │
//! └────────┬────────────────────────────────┘
//!          │
//!    ┌─────┴─────┬──────────┬────────┐
//!    ▼           ▼          ▼        ▼
//! ┌─────┐   ┌────────┐  ┌──────┐ ┌──────┐
//! │WASM │   │ML Model│  │  RDF │ │ WAL  │
//! │Rules│   │ Invoke │  │Store │ │      │
//! └─────┘   └────────┘  └──────┘ └──────┘
//! ```
//!
//! ## Components
//!
//! - **workflow**: Workflow definition, DAG execution, orchestration
//! - **ml**: ML model registry, invocation, caching
//! - **rules**: Rule versioning, WASM wrapper, heuristics
//! - **confidence**: Confidence aggregation algorithms
//! - **cluster**: Raft consensus, rule distribution (Phase 2)
//! - **experiments**: A/B testing framework (Phase 2)
//! - **api**: REST/gRPC endpoints for workflow management

pub mod api;
pub mod confidence;
pub mod field_lineage;
pub mod ml;
pub mod persistence;
pub mod rules;
pub mod workflow;

// Phase 2 components (to be implemented)
// pub mod cluster;
// pub mod experiments;

// Re-export core types
pub use workflow::{
    ExecutionContext, Workflow, WorkflowDefinition, WorkflowEngine, WorkflowExecutor,
    WorkflowResult, WorkflowStep,
};

pub use ml::{ModelCache, ModelInvoker, ModelMetadata, ModelRegistry, ModelRequest, ModelResponse};

pub use confidence::{AggregationMethod, ConfidenceAggregator, ConfidenceScore};

pub use field_lineage::{
    ConflictSeverity, FieldConflict, FieldResolution, FieldResolver, FieldValue, SourceValue,
    StrategyType, VotingEngine, VotingStrategy,
};
