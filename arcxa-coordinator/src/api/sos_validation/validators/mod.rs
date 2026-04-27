//! Validation logic for SoS integration
//!
//! This module implements validation algorithms for:
//! - Interface compatibility (schema, units, coordinate systems)
//! - Data contract compliance (SLA metrics, transformations)
//! - System integration (end-to-end connectivity)
//! - Policy validation (governance rules via SPARQL)
//! - Data validation (JSON Schema compliance)
//!
//! ## Validation Types
//!
//! 1. **Schema Compatibility**
//!    - JSON Schema comparison
//!    - Field mapping validation
//!    - Type compatibility checking
//!
//! 2. **Unit System Compatibility**
//!    - SI ↔ Imperial conversion validation
//!    - Custom unit definitions (via QUDT ontology)
//!    - Precision/rounding requirements
//!
//! 3. **Coordinate System Compatibility**
//!    - WGS84 ↔ ECI J2000 transformations
//!    - Local tangent plane conversions
//!    - Accuracy requirements
//!
//! 4. **SLA Compliance**
//!    - Latency checks
//!    - Reliability/uptime requirements
//!    - Throughput/bandwidth validation
//!
//! 5. **Policy Validation**
//!    - SPARQL query execution
//!    - Governance rule enforcement
//!    - Cross-system constraints

// Implemented validators
pub mod coordinate_validator;
pub mod field_validator;
pub mod policy_validator;
pub mod schema_validator;
pub mod sla_validator;
pub mod unit_validator;

// Export commonly used validators
pub use coordinate_validator::{validate_coordinate_compatibility, CoordinateCompatibilityResult};
pub use field_validator::{validate_data_format, validate_direction, validate_protocol};
pub use policy_validator::{
    evaluate_policy_results, extract_policy_placeholders, map_policy_severity, render_policy_query,
    PolicyEvaluation, PolicyQueryTemplateError,
};
pub use schema_validator::{
    compare_interface_schemas, validate_data_against_schema, SchemaCompatibilityReport,
};
pub use sla_validator::{
    validate_sla_metric_name, validate_sla_metric_value, validate_sla_metrics,
    validate_sla_operator,
};
pub use unit_validator::{validate_unit_compatibility, UnitCompatibilityResult};
