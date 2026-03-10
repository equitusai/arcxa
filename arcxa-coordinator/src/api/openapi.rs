//! OpenAPI Module
//!
//! **Note**: ARCXA uses a modular API documentation architecture.
//! Each API module has its own dedicated Swagger UI for focused documentation.
//!
//! All Swagger UIs are properly versioned under `/api/v1`:
//!
//! - File Library: `http://localhost:8080/api/v1/file-library/swagger-ui`
//! - R2RML: `http://localhost:8080/api/v1/r2rml/swagger-ui`
//! - Loader: `http://localhost:8080/api/v1/loader/swagger-ui`
//! - DDL: `http://localhost:8080/api/v1/ddl/swagger-ui`
//! - Ontology: `http://localhost:8080/api/v1/ontology/swagger-ui`
//! - Workflow: `http://localhost:8080/api/v1/workflows/swagger-ui`
//! - Unified Mapping: `http://localhost:8080/api/v1/mapping/swagger-ui`
//! - Field Lineage: `http://localhost:8080/api/v1/field-lineage/swagger-ui`
//! - Row Lineage: `http://localhost:8080/api/v1/lineage/swagger-ui`
//! - Data Sources: `http://localhost:8080/api/v1/datasources/swagger-ui`
//! - Governance: `http://localhost:8080/api/v1/governance/swagger-ui`
//!
//! All API endpoints and their documentation are versioned together for consistency.

/// Generate OpenAPI index document (dynamically generated, not static)
///
/// This is a minimal OpenAPI 3.0 document that serves as an index to the modular
/// Swagger UIs. Each module has its own complete OpenAPI spec.
pub fn generate_openapi_index() -> String {
    r#"openapi: 3.0.0
info:
  title: ARCXA Data Governance Platform API
  version: 1.0.0
  description: |
    ARCXA uses a modular API documentation architecture.
    Each API module has its own dedicated Swagger UI under /api/v1:

    - File Library: http://localhost:8080/api/v1/file-library/swagger-ui
    - R2RML: http://localhost:8080/api/v1/r2rml/swagger-ui
    - Loader: http://localhost:8080/api/v1/loader/swagger-ui
    - DDL: http://localhost:8080/api/v1/ddl/swagger-ui
    - Ontology: http://localhost:8080/api/v1/ontology/swagger-ui
    - Workflow: http://localhost:8080/api/v1/workflows/swagger-ui
    - Unified Mapping: http://localhost:8080/api/v1/mapping/swagger-ui
    - Field Lineage: http://localhost:8080/api/v1/field-lineage/swagger-ui
    - Row Lineage: http://localhost:8080/api/v1/lineage/swagger-ui
    - Data Sources: http://localhost:8080/api/v1/datasources/swagger-ui
    - Governance: http://localhost:8080/api/v1/governance/swagger-ui

    All API endpoints are properly versioned under /api/v1.
    For complete API documentation, please visit the module-specific Swagger UIs above.

    Each module's OpenAPI spec is available at:
    - /api/v1/{module}/api-docs/openapi.json
  contact:
    name: ARCXA Team
    email: avinam@equitus.us
paths: {}
"#
    .to_string()
}
