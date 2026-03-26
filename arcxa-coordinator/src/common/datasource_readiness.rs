//! Shared datasource readiness evaluation used by API and workflow validation.

use graphica_core::catalog::{
    api_types::{DataSourceCapabilities, DataSourceStatus},
    DataSourceResponse,
};

pub const DATASOURCE_NOT_READY_CODE: &str = "DATASOURCE_NOT_READY";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatasourceOperation {
    SchemaInference,
    Query,
    Discovery,
    WorkflowRead,
    WorkflowWrite,
}

impl DatasourceOperation {
    pub fn description(self) -> &'static str {
        match self {
            Self::SchemaInference => "schema inference",
            Self::Query => "query execution",
            Self::Discovery => "schema discovery",
            Self::WorkflowRead => "workflow extraction",
            Self::WorkflowWrite => "workflow loading",
        }
    }

    pub fn is_supported(self, capabilities: &DataSourceCapabilities) -> bool {
        match self {
            Self::SchemaInference | Self::Discovery => capabilities.can_infer_schema,
            Self::Query => capabilities.can_query,
            Self::WorkflowRead => capabilities.can_read_workflow,
            Self::WorkflowWrite => capabilities.can_write_workflow,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatasourceReadinessFailure {
    pub code: &'static str,
    pub message: String,
}

pub fn evaluate_datasource_readiness(
    response: &DataSourceResponse,
    operation: DatasourceOperation,
) -> Result<(), DatasourceReadinessFailure> {
    let capabilities = response
        .capabilities
        .clone()
        .unwrap_or_else(default_capabilities);

    if response.status != DataSourceStatus::Active {
        return Err(DatasourceReadinessFailure {
            code: DATASOURCE_NOT_READY_CODE,
            message: build_status_not_ready_message(response, operation),
        });
    }

    if !operation.is_supported(&capabilities) {
        return Err(DatasourceReadinessFailure {
            code: DATASOURCE_NOT_READY_CODE,
            message: build_capability_not_ready_message(response, operation),
        });
    }

    Ok(())
}

fn default_capabilities() -> DataSourceCapabilities {
    DataSourceCapabilities {
        can_test: false,
        can_infer_schema: false,
        can_query: false,
        can_read_workflow: false,
        can_write_workflow: false,
        supports_parameters: false,
        supports_tls: false,
        supports_incremental: false,
        supports_cancellation: false,
    }
}

fn build_status_not_ready_message(
    response: &DataSourceResponse,
    operation: DatasourceOperation,
) -> String {
    let datasource_id = response.source.id.as_str();
    let action = operation.description();

    match response.status {
        DataSourceStatus::Unverified => format!(
            "Datasource '{}' is not ready for {} because it has not been verified yet. Test the connection successfully before retrying.",
            datasource_id, action
        ),
        DataSourceStatus::Testing => format!(
            "Datasource '{}' is not ready for {} because a connection test is still in progress.",
            datasource_id, action
        ),
        DataSourceStatus::Disabled => format!(
            "Datasource '{}' is not ready for {} because it is disabled.",
            datasource_id, action
        ),
        DataSourceStatus::Error => {
            let last_error = response
                .last_test_result
                .as_ref()
                .and_then(|result| result.error.as_deref())
                .map(str::trim)
                .filter(|error| !error.is_empty());

            if let Some(last_error) = last_error {
                format!(
                    "Datasource '{}' is not ready for {} because the last connection test failed: {}",
                    datasource_id, action, last_error
                )
            } else {
                format!(
                    "Datasource '{}' is not ready for {} because the last connection test failed. Fix the connection issue and retest before retrying.",
                    datasource_id, action
                )
            }
        }
        DataSourceStatus::Active => build_capability_not_ready_message(response, operation),
    }
}

fn build_capability_not_ready_message(
    response: &DataSourceResponse,
    operation: DatasourceOperation,
) -> String {
    let datasource_id = response.source.id.as_str();
    let action = operation.description();

    if response.source.source_type.eq_ignore_ascii_case("Oracle") {
        format!(
            "Datasource '{}' is not operationally ready for {} on this deployment. Verify Oracle ODBC driver or DSN configuration, then retest the connection.",
            datasource_id, action
        )
    } else {
        format!(
            "Datasource '{}' is not operationally ready for {}. Re-test the connection and verify connector support before retrying.",
            datasource_id, action
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use graphica_core::catalog::{
        types::OracleConfig, ConnectionDetails, ConnectionTestResult, DataSource, SourceConfig,
    };
    use std::collections::HashMap;

    fn test_response(
        status: DataSourceStatus,
        capabilities: DataSourceCapabilities,
    ) -> DataSourceResponse {
        DataSourceResponse {
            source: DataSource {
                id: "urn:graphica:datasource:test".to_string(),
                title: "Test".to_string(),
                description: None,
                source_type: "Oracle".to_string(),
                connection: ConnectionDetails {
                    secret_ref: "vault://test".to_string(),
                    config: SourceConfig::Oracle(OracleConfig {
                        host: "localhost".to_string(),
                        port: 1521,
                        service_name: Some("ORCL".to_string()),
                        sid: None,
                        schema: None,
                    }),
                    encryption_enabled: false,
                    credentials: HashMap::new(),
                },
                schema_ref: None,
                tags: Vec::new(),
                metadata: HashMap::new(),
                created_at: None,
                updated_at: None,
                last_synced_at: None,
            },
            status,
            last_test_result: None,
            capabilities: Some(capabilities),
        }
    }

    #[test]
    fn unverified_datasource_is_not_ready() {
        let result = evaluate_datasource_readiness(
            &test_response(
                DataSourceStatus::Unverified,
                DataSourceCapabilities {
                    can_test: true,
                    can_infer_schema: false,
                    can_query: false,
                    can_read_workflow: false,
                    can_write_workflow: false,
                    supports_parameters: true,
                    supports_tls: false,
                    supports_incremental: true,
                    supports_cancellation: false,
                },
            ),
            DatasourceOperation::SchemaInference,
        )
        .unwrap_err();

        assert_eq!(result.code, DATASOURCE_NOT_READY_CODE);
        assert!(result.message.contains("has not been verified"));
    }

    #[test]
    fn failed_test_error_is_included_in_message() {
        let mut response = test_response(
            DataSourceStatus::Error,
            DataSourceCapabilities {
                can_test: true,
                can_infer_schema: false,
                can_query: false,
                can_read_workflow: false,
                can_write_workflow: false,
                supports_parameters: true,
                supports_tls: false,
                supports_incremental: true,
                supports_cancellation: false,
            },
        );
        response.last_test_result = Some(ConnectionTestResult {
            success: false,
            duration_ms: 0,
            error: Some("missing driver".to_string()),
            metadata: HashMap::new(),
            tested_at: Utc::now(),
        });

        let result =
            evaluate_datasource_readiness(&response, DatasourceOperation::Discovery).unwrap_err();

        assert!(result.message.contains("missing driver"));
    }

    #[test]
    fn active_datasource_requires_operation_capability() {
        let result = evaluate_datasource_readiness(
            &test_response(
                DataSourceStatus::Active,
                DataSourceCapabilities {
                    can_test: true,
                    can_infer_schema: false,
                    can_query: false,
                    can_read_workflow: false,
                    can_write_workflow: false,
                    supports_parameters: true,
                    supports_tls: false,
                    supports_incremental: true,
                    supports_cancellation: false,
                },
            ),
            DatasourceOperation::WorkflowRead,
        )
        .unwrap_err();

        assert!(result.message.contains("operationally ready"));
    }
}
