//! Integration with Graphica subsystems for Systems-of-Systems validation.

pub mod ontology_sync;
pub mod service;
pub mod startup;

pub use ontology_sync::{ensure_interface_ontology_assets, reconcile_sos_ontology_assets};
pub use service::{
    create_sos_validation_callback, SosValidationService, SosValidationServiceError,
    ValidationExecutionOptions,
};
pub use startup::{perform_startup_recovery, SosStartupRecoveryOutcome};
