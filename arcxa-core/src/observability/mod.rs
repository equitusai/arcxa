//! # Observability Module
//!
//! Distributed tracing, correlation IDs, and enhanced metrics for production observability.

pub mod correlation;
pub mod tracing;
// pub mod alerts; // Moved to coordinator (uses prometheus)

pub use self::tracing::{instrument_pipeline_stage, TracingContext};
pub use correlation::{CorrelationId, CorrelationIdGenerator};
// pub use alerts::{AlertRule, AlertManager, AlertCondition, AlertSeverity};
