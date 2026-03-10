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
pub mod field_validator;
pub mod sla_validator;

// Export commonly used validators
pub use field_validator::{validate_data_format, validate_direction, validate_protocol};
pub use sla_validator::{
    validate_sla_metric_name, validate_sla_metric_value, validate_sla_metrics,
    validate_sla_operator,
};

// TODO: Implement additional validators
// - schema_validator.rs - JSON Schema validation (Phase 2)
// - unit_validator.rs - Unit system compatibility (Phase 4)
// - coordinate_validator.rs - Coordinate system transformations (Phase 4)
// - policy_validator.rs - Governance policy checks (Phase 5)
