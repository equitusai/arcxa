//! OpenAPI documentation for Systems-of-Systems (SoS) Validation API
//!
//! This module aggregates all SoS validation endpoints into a single OpenAPI specification.

use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    paths(
        // System Management
        crate::api::sos_validation::handlers::register_system,
        crate::api::sos_validation::handlers::list_systems,
        crate::api::sos_validation::handlers::get_system,
        crate::api::sos_validation::handlers::update_system,
        crate::api::sos_validation::handlers::delete_system,
        crate::api::sos_validation::handlers::list_system_interfaces,
        // Interface Definition
        crate::api::sos_validation::handlers::register_interface,
        crate::api::sos_validation::handlers::list_interfaces,
        crate::api::sos_validation::handlers::get_interface,
        crate::api::sos_validation::handlers::update_interface,
        crate::api::sos_validation::handlers::delete_interface,
        crate::api::sos_validation::handlers::validate_interface_schema,
        // Data Contracts
        crate::api::sos_validation::handlers::create_contract,
        crate::api::sos_validation::handlers::list_contracts,
        crate::api::sos_validation::handlers::get_contract,
        crate::api::sos_validation::handlers::update_contract,
        crate::api::sos_validation::handlers::delete_contract,
        crate::api::sos_validation::handlers::approve_contract,
        crate::api::sos_validation::handlers::sign_contract,
        // Validation
        crate::api::sos_validation::handlers::validate,
        crate::api::sos_validation::handlers::validate_dry_run,
        // Analytics
        crate::api::sos_validation::handlers::get_compatibility_matrix,
        crate::api::sos_validation::handlers::get_dependency_graph,
        crate::api::sos_validation::handlers::what_if_analysis,
    ),
    components(
        schemas(
            // System Management types
            crate::api::sos_validation::types::RegisterSystemRequest,
            crate::api::sos_validation::types::UpdateSystemRequest,
            crate::api::sos_validation::types::ListSystemsQuery,
            crate::api::sos_validation::types::SystemResponse,
            crate::api::sos_validation::types::ListSystemsResponse,
            // Interface Definition types
            crate::api::sos_validation::types::SystemInterface,
            crate::api::sos_validation::types::RegisterInterfaceRequest,
            crate::api::sos_validation::types::UpdateInterfaceRequest,
            crate::api::sos_validation::types::InterfaceResponse,
            // Data Contract types
            crate::api::sos_validation::types::SlaMetric,
            crate::api::sos_validation::types::CreateDataContractRequest,
            crate::api::sos_validation::types::UpdateDataContractRequest,
            crate::api::sos_validation::types::DataContractResponse,
            // Validation types
            crate::api::sos_validation::types::ValidateRequest,
            crate::api::sos_validation::types::CheckResult,
            crate::api::sos_validation::types::ValidationResponse,
            // Analytics types
            crate::api::sos_validation::types::CompatibilityScore,
            crate::api::sos_validation::types::CompatibilityDetail,
            crate::api::sos_validation::types::CompatibilityMatrixResponse,
            crate::api::sos_validation::types::WhatIfRequest,
            crate::api::sos_validation::types::WhatIfResponse,
            // Error types
            crate::api::sos_validation::types::SosErrorResponse,
        )
    ),
    tags(
        (name = "SoS - System Management", description = "Register and manage systems in the SoS catalog with capabilities and deployment information"),
        (name = "SoS - Interface Definition", description = "Define system interfaces with schemas, coordinate systems, and unit specifications"),
        (name = "SoS - Data Contracts", description = "Create and manage data contracts between interfaces with SLA requirements"),
        (name = "SoS - Validation", description = "Execute cross-system validation for compatibility, policies, and SLA compliance"),
        (name = "SoS - Analytics", description = "Analyze compatibility matrices, dependency graphs, and what-if scenarios"),
    ),
    info(
        title = "ARCXA Systems-of-Systems Validation API",
        version = "1.0.0",
        description = "REST API for validating compatibility and integration between systems in a Systems-of-Systems architecture. Supports system registration, interface definition, data contracts, cross-system validation (schemas, units, coordinate systems, SLAs), and analytics.",
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
pub struct SosValidationApiDoc;
