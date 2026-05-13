use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use graphica_core::catalog::api_types::QueryResult;
use graphica_core::catalog::connector::{Credentials, DataSourceConnector};
#[cfg(feature = "odbc")]
use graphica_core::catalog::connectors::saphana::SAPHANAConnector;
use graphica_core::catalog::types::{ConnectionDetails, DataSource, SAPHANAConfig, SourceConfig};
use graphica_core::distributed::proto::migration_evidence::{
    verification_service_server::VerificationService, HealthRequest, HealthResponse,
    RunVerificationAndEmitRequest, RunVerificationAndEmitResponse, RunVerificationRequest,
    RunVerificationResponse,
};
use graphica_core::migration_evidence::{
    derive_sap_ecc_projection_fields, derive_sap_ecc_rfc_bapi_projection_fields,
    derive_sap_s4_odata_projection_fields, extract_json_path_value,
    extract_sap_ecc_adapter_next_path, extract_sap_ecc_rfc_bapi_next_cursor_from_path,
    extract_sap_s4_odata_next_link, merge_sap_ecc_adapter_page_payloads,
    merge_sap_ecc_rfc_bapi_page_payloads, merge_sap_s4_odata_page_payloads, resolve_connector_auth,
    resolve_sap_ecc_adapter_value, resolve_sap_ecc_rfc_bapi_value, resolve_sap_s4_odata_value,
    verification_result_to_events, ConnectorAuth, ConnectorAuthResolutionMetadata,
    ConnectorTransport, ControlResult, ControlStatus, ExceptionRecord, ExceptionSeverity,
    ExceptionStatus, ExecutionEvent, ExecutionStatus, MigrationEvidenceEventForwarder,
    SapEccAdapterCapabilities, SapEccAdapterField, SapEccBackendAuthMode, SapEccProjectionFields,
    SapEccRfcBapiCapabilities, SapEccRfcBapiField, SapEccRfcBapiProfile,
    SapEccRfcBapiProjectionFields, SapEccSessionMode, SapS4ODataCapabilities,
    SapS4ODataProjectionFields, SapS4ODataProperty, SapS4ODataVersion, VerificationDispatchRequest,
    VerificationDispatchResult, VerificationRequest, VerificationResult,
};
use graphica_core::secrets::providers::SecretStoreRegistry;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;
use tonic::{Request, Response, Status};

#[derive(Clone)]
pub struct VerificationManager {
    event_forwarder: Arc<dyn MigrationEvidenceEventForwarder>,
    secret_store_registry: Option<Arc<SecretStoreRegistry>>,
    session_cache: Arc<Mutex<HashMap<String, CachedBridgeSession>>>,
}

#[derive(Debug, Clone)]
struct CachedBridgeSession {
    session_id: String,
    expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Default)]
struct BridgeSessionOutcome {
    session_id_present: bool,
    session_reused: bool,
    session_closed: bool,
    session_ttl_seconds: Option<u64>,
}

impl VerificationManager {
    pub fn new(event_forwarder: Arc<dyn MigrationEvidenceEventForwarder>) -> Self {
        Self {
            event_forwarder,
            secret_store_registry: None,
            session_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn with_secret_store_registry(mut self, registry: Arc<SecretStoreRegistry>) -> Self {
        self.secret_store_registry = Some(registry);
        self
    }

    async fn resolve_connector_auth(
        &self,
        auth: &ConnectorAuth,
    ) -> Result<graphica_core::migration_evidence::ResolvedConnectorAuth> {
        resolve_connector_auth(auth, self.secret_store_registry.clone()).await
    }

    pub async fn run_verification(
        &self,
        request: VerificationRequest,
    ) -> Result<VerificationResult> {
        let resolved_auth = self.resolve_connector_auth(&request.source.auth).await?;
        let mut s4_odata_context = None;
        let mut ecc_adapter_context = None;
        let mut ecc_rfc_context = None;
        let actual_value = match request.source.transport {
            ConnectorTransport::HttpJson => {
                let endpoint = request
                    .source
                    .endpoint
                    .clone()
                    .ok_or_else(|| anyhow!("http_json verification requires endpoint"))?;
                self.fetch_http_json_actual_value(&endpoint, &resolved_auth.auth)
                    .await?
            }
            ConnectorTransport::SapS4OData => {
                let endpoint = request
                    .source
                    .endpoint
                    .clone()
                    .ok_or_else(|| anyhow!("sap_s4_odata verification requires endpoint"))?;
                self.fetch_s4_odata_actual_value(
                    &endpoint,
                    &resolved_auth.auth,
                    &request.source.connection,
                    &request.target_field.field_path,
                    request.expected_value.as_ref(),
                )
                .await
                .map(|outcome| {
                    s4_odata_context = Some(outcome.clone());
                    outcome.actual_value
                })?
            }
            ConnectorTransport::SapHanaSql => {
                let query = request
                    .source
                    .query
                    .clone()
                    .ok_or_else(|| anyhow!("sap_hana_sql verification requires query"))?;
                self.fetch_hana_actual_value(
                    &request.source.connection,
                    &resolved_auth.auth,
                    &query,
                )
                .await?
            }
            ConnectorTransport::SapEccAdapter => {
                let endpoint = request
                    .source
                    .endpoint
                    .clone()
                    .ok_or_else(|| anyhow!("sap_ecc_adapter verification requires endpoint"))?;
                self.fetch_ecc_adapter_actual_value(
                    &endpoint,
                    &resolved_auth.auth,
                    &request.source.connection,
                    &request.target_field.field_path,
                    request.expected_value.as_ref(),
                )
                .await
                .map(|outcome| {
                    ecc_adapter_context = Some(outcome.clone());
                    outcome.actual_value
                })?
            }
            ConnectorTransport::SapEccRfcBapi => {
                let endpoint =
                    request.source.endpoint.clone().ok_or_else(|| {
                        anyhow!("sap_ecc_rfc_bapi verification requires endpoint")
                    })?;
                self.fetch_ecc_rfc_bapi_actual_value(
                    &endpoint,
                    &resolved_auth.auth,
                    &request.source.connection,
                    &request.target_field.field_path,
                    request.expected_value.as_ref(),
                )
                .await
                .map(|outcome| {
                    ecc_rfc_context = Some(outcome.clone());
                    outcome.actual_value
                })?
            }
            ConnectorTransport::SapEccStagedExport => {
                return Err(anyhow!(
                    "sap_ecc_staged_export is an ingestion transport, not a live verification transport"
                ));
            }
            ConnectorTransport::SapIdocExtractorPackage => {
                return Err(anyhow!(
                    "sap_idoc_extractor_package is an ingestion transport, not a live verification transport"
                ));
            }
            ConnectorTransport::SapOdpExtractorPackage => {
                return Err(anyhow!(
                    "sap_odp_extractor_package is an ingestion transport, not a live verification transport"
                ));
            }
            ConnectorTransport::ManualDrop => {
                return Err(anyhow!(
                    "manual_drop transport is not supported for live verification"
                ));
            }
        };

        let mut assessment = assess_values(
            request.expected_value.as_ref(),
            Some(&actual_value),
            request.tolerance,
        );
        let mut control_metadata = merge_control_metadata(&request.metadata, &assessment);
        apply_auth_resolution_metadata(
            &mut control_metadata,
            "source_auth_",
            &resolved_auth.metadata,
        );
        let mut execution_metadata = request.metadata.clone();
        apply_auth_resolution_metadata(
            &mut execution_metadata,
            "source_auth_",
            &resolved_auth.metadata,
        );
        let mut summary_override = None;
        if let Some(context) = s4_odata_context.as_ref() {
            apply_s4_odata_verification_metadata(&mut control_metadata, context);
            if !context.projection.missing_fields.is_empty() {
                assessment.status = ControlStatus::Failed;
                summary_override = Some(build_s4_projection_validation_summary(context));
            } else if context.pagination_truncated {
                assessment.status = merge_status(assessment.status.clone(), ControlStatus::Warning);
                summary_override = Some(build_s4_pagination_warning_summary(context));
            }
        }
        if let Some(context) = ecc_adapter_context.as_ref() {
            apply_ecc_adapter_verification_metadata(&mut control_metadata, context);
            if !context.projection.missing_fields.is_empty() {
                assessment.status = ControlStatus::Failed;
                summary_override = Some(build_ecc_projection_validation_summary(context));
            } else if context.pagination_truncated {
                assessment.status = merge_status(assessment.status.clone(), ControlStatus::Warning);
                summary_override = Some(build_ecc_pagination_warning_summary(context));
            }
        }
        if let Some(context) = ecc_rfc_context.as_ref() {
            apply_ecc_rfc_verification_metadata(&mut control_metadata, context);
            if !context.projection.missing_fields.is_empty() {
                assessment.status = ControlStatus::Failed;
                summary_override = Some(build_ecc_rfc_projection_validation_summary(context));
            } else if context.pagination_truncated {
                assessment.status = merge_status(assessment.status.clone(), ControlStatus::Warning);
                summary_override = Some(build_ecc_rfc_pagination_warning_summary(context));
            }
        }

        let status = assessment.status.clone();
        control_metadata = merge_control_metadata(&control_metadata, &assessment);
        let execution_id = uuid::Uuid::new_v4().to_string();
        let execution_event = ExecutionEvent {
            execution_id: execution_id.clone(),
            program_id: request.program_id.clone(),
            object_id: request.object_id.clone(),
            connector_run_id: format!("verification-{}", execution_id),
            tool_name: match request.source.transport {
                ConnectorTransport::HttpJson => "sap_verification_api".to_string(),
                ConnectorTransport::SapEccAdapter => "sap_ecc_adapter_verification".to_string(),
                ConnectorTransport::SapEccRfcBapi => "sap_ecc_rfc_bapi_verification".to_string(),
                ConnectorTransport::SapEccStagedExport => {
                    "sap_ecc_staged_export_ingest".to_string()
                }
                ConnectorTransport::SapIdocExtractorPackage => {
                    "sap_idoc_extractor_ingest".to_string()
                }
                ConnectorTransport::SapOdpExtractorPackage => {
                    "sap_odp_extractor_ingest".to_string()
                }
                ConnectorTransport::SapS4OData => "sap_s4_odata_verification".to_string(),
                ConnectorTransport::SapHanaSql => "sap_hana_verification".to_string(),
                ConnectorTransport::ManualDrop => "verification".to_string(),
            },
            tool_run_id: execution_id.clone(),
            stage: "verification".to_string(),
            status: match status {
                ControlStatus::Passed => ExecutionStatus::Succeeded,
                ControlStatus::Warning => ExecutionStatus::Partial,
                ControlStatus::Failed => ExecutionStatus::Failed,
                ControlStatus::NotRun => ExecutionStatus::Partial,
            },
            happened_at: Utc::now(),
            source_snapshot_ref: None,
            target_snapshot_ref: None,
            records_examined: Some(1),
            records_affected: Some(1),
            metadata: execution_metadata,
        };

        let control_result = ControlResult {
            control_id: uuid::Uuid::new_v4().to_string(),
            program_id: request.program_id.clone(),
            object_id: request.object_id.clone(),
            control_name: request.control_name.clone(),
            control_type: "verification".to_string(),
            status: status.clone(),
            summary: summary_override.unwrap_or_else(|| {
                build_summary(
                    &assessment,
                    request.expected_value.as_ref(),
                    &actual_value,
                    request.tolerance,
                )
            }),
            expected_value: request.expected_value.clone(),
            actual_value: Some(actual_value.clone()),
            tolerance: request.tolerance,
            executed_at: Utc::now(),
            evidence_refs: vec![],
            metadata: control_metadata.clone(),
        };

        let exception_record = if matches!(status, ControlStatus::Failed | ControlStatus::Warning) {
            Some(ExceptionRecord {
                exception_id: uuid::Uuid::new_v4().to_string(),
                program_id: request.program_id.clone(),
                object_id: request.object_id.clone(),
                severity: if status == ControlStatus::Failed {
                    ExceptionSeverity::Error
                } else {
                    ExceptionSeverity::Warning
                },
                status: ExceptionStatus::Open,
                category: "verification_delta".to_string(),
                message: control_result.summary.clone(),
                source_value: request.expected_value.clone(),
                target_value: Some(actual_value),
                remediation: None,
                detected_at: Utc::now(),
                resolved_at: None,
                metadata: control_metadata.clone(),
            })
        } else {
            None
        };

        Ok(VerificationResult {
            execution_event,
            control_result,
            exception_record,
        })
    }

    pub async fn run_verification_and_emit(
        &self,
        request: VerificationDispatchRequest,
    ) -> Result<VerificationDispatchResult> {
        let verification_result = self.run_verification(request.verification).await?;
        let emitted_events = verification_result_to_events(
            request.connector_id,
            request.run_id,
            request.vendor,
            verification_result.clone(),
        )
        .context(
            "failed to translate verification result into canonical migration evidence events",
        )?;
        let dispatch_summary = self
            .event_forwarder
            .ingest_events(emitted_events.clone())
            .await?;

        Ok(VerificationDispatchResult {
            verification_result,
            emitted_events,
            dispatch_summary,
        })
    }

    async fn fetch_http_json_actual_value(
        &self,
        endpoint: &graphica_core::migration_evidence::ConnectorEndpoint,
        auth: &graphica_core::migration_evidence::ConnectorAuth,
    ) -> Result<Value> {
        let response = self.request_json(endpoint, auth, HashMap::new()).await?;
        if response.get("actual_value").is_some() {
            Ok(response.get("actual_value").cloned().unwrap_or(Value::Null))
        } else {
            Ok(response)
        }
    }

    async fn fetch_s4_odata_actual_value(
        &self,
        endpoint: &graphica_core::migration_evidence::ConnectorEndpoint,
        auth: &graphica_core::migration_evidence::ConnectorAuth,
        connection: &HashMap<String, String>,
        target_field_path: &str,
        expected_value: Option<&Value>,
    ) -> Result<SapS4ODataVerificationOutcome> {
        let mut extra_headers = HashMap::from([
            ("accept".to_string(), "application/json".to_string()),
            ("x-requested-with".to_string(), "XMLHttpRequest".to_string()),
        ]);
        if let Some(client) = connection.get("odata_client") {
            extra_headers.insert("sap-client".to_string(), client.clone());
        }
        if let Some(language) = connection.get("odata_language") {
            extra_headers.insert("accept-language".to_string(), language.clone());
        }

        let capabilities = parse_s4_odata_capabilities(connection);
        let projection = derive_sap_s4_odata_projection_fields(
            capabilities.as_ref(),
            &endpoint.path,
            target_field_path,
            expected_value,
            connection,
        );
        let pagination = parse_s4_odata_pagination_config(connection);
        let response = self
            .request_s4_odata_payload(endpoint, auth, extra_headers, &pagination)
            .await?;
        let explicit_path = connection
            .get("odata_value_path")
            .cloned()
            .or_else(|| connection.get("value_path").cloned());
        let fallback_path = if target_field_path.starts_with("$.") {
            Some(target_field_path)
        } else {
            None
        };
        let mut preferred_paths = Vec::new();
        if let Some(path) = explicit_path.as_deref() {
            preferred_paths.push(path);
        }
        if let Some(path) = fallback_path {
            if preferred_paths.iter().all(|candidate| *candidate != path) {
                preferred_paths.push(path);
            }
        }
        let actual_value = resolve_sap_s4_odata_value(response.payload, &preferred_paths)?;
        Ok(SapS4ODataVerificationOutcome {
            actual_value,
            projection,
            metadata_capabilities: capabilities,
            page_count: response.page_count,
            paginated: response.page_count > 1,
            pagination_truncated: response.truncated,
            row_count: count_s4_odata_rows(&response.normalized_payload),
            next_link_remaining: response.next_link_remaining,
        })
    }

    async fn fetch_ecc_adapter_actual_value(
        &self,
        endpoint: &graphica_core::migration_evidence::ConnectorEndpoint,
        auth: &graphica_core::migration_evidence::ConnectorAuth,
        connection: &HashMap<String, String>,
        target_field_path: &str,
        expected_value: Option<&Value>,
    ) -> Result<SapEccAdapterVerificationOutcome> {
        let mut extra_headers = HashMap::from([
            ("accept".to_string(), "application/json".to_string()),
            ("x-requested-with".to_string(), "XMLHttpRequest".to_string()),
        ]);
        if let Some(client) = connection.get("ecc_client") {
            extra_headers.insert("x-sap-client".to_string(), client.clone());
        }
        if let Some(language) = connection.get("ecc_language") {
            extra_headers.insert("accept-language".to_string(), language.clone());
        }
        if let Some(system_id) = connection.get("ecc_system_id") {
            extra_headers.insert("x-sap-system-id".to_string(), system_id.clone());
        }

        let capabilities = parse_ecc_adapter_capabilities(connection);
        let request_config = parse_ecc_adapter_request_config(connection)?;
        validate_ecc_adapter_runtime_requirements(capabilities.as_ref(), &request_config)?;
        let projection = derive_sap_ecc_projection_fields(
            capabilities.as_ref(),
            target_field_path,
            expected_value,
            connection,
        );
        let pagination = parse_ecc_adapter_pagination_config(connection);
        let session_config =
            parse_ecc_adapter_session_config(connection, capabilities.as_ref(), &request_config);
        let missing_required_parameters = find_missing_required_adapter_parameters(
            endpoint,
            &request_config.request_parameters,
            capabilities.as_ref(),
        )?;
        if !missing_required_parameters.is_empty() {
            return Err(anyhow!(
                "sap_ecc_adapter verification is missing required request parameter(s): {}",
                missing_required_parameters.join(", ")
            ));
        }
        let mut effective_request_parameters = request_config.request_parameters.clone();
        if let Some(language) = request_config.language.as_deref() {
            let parameter_name = capabilities
                .as_ref()
                .and_then(|caps| caps.language_parameter_name.as_deref())
                .or_else(|| {
                    connection
                        .get("ecc_language_parameter_name")
                        .map(String::as_str)
                })
                .unwrap_or("language");
            effective_request_parameters
                .entry(parameter_name.to_string())
                .or_insert_with(|| language.to_string());
        }
        if let Some(page_size) = request_config.page_size {
            effective_request_parameters
                .entry(pagination.page_size_parameter_name.clone())
                .or_insert_with(|| page_size.to_string());
        }
        let response = self
            .request_ecc_adapter_payload(
                endpoint,
                auth,
                extra_headers,
                &effective_request_parameters,
                &pagination,
                &session_config,
            )
            .await?;
        let explicit_path = connection
            .get("ecc_value_path")
            .cloned()
            .or_else(|| connection.get("value_path").cloned());
        let fallback_path = if target_field_path.starts_with("$.") {
            Some(target_field_path)
        } else {
            None
        };
        let mut preferred_paths = Vec::new();
        if let Some(path) = explicit_path.as_deref() {
            preferred_paths.push(path);
        }
        if let Some(path) = fallback_path {
            if preferred_paths.iter().all(|candidate| *candidate != path) {
                preferred_paths.push(path);
            }
        }
        let actual_value = resolve_sap_ecc_adapter_value(response.payload, &preferred_paths)?;
        Ok(SapEccAdapterVerificationOutcome {
            actual_value,
            projection,
            metadata_capabilities: capabilities,
            request_parameters: effective_request_parameters,
            missing_required_parameters,
            session_mode: request_config.session_mode,
            backend_auth_mode: request_config.backend_auth_mode,
            language: request_config.language,
            page_size: request_config.page_size,
            session_id_present: response.session.session_id_present,
            session_reused: response.session.session_reused,
            session_closed: response.session.session_closed,
            session_ttl_seconds: response.session.session_ttl_seconds,
            page_count: response.page_count,
            paginated: response.page_count > 1,
            pagination_truncated: response.truncated,
            row_count: count_s4_odata_rows(&response.normalized_payload),
            next_link_remaining: response.next_path_remaining,
        })
    }

    async fn fetch_ecc_rfc_bapi_actual_value(
        &self,
        endpoint: &graphica_core::migration_evidence::ConnectorEndpoint,
        auth: &graphica_core::migration_evidence::ConnectorAuth,
        connection: &HashMap<String, String>,
        target_field_path: &str,
        expected_value: Option<&Value>,
    ) -> Result<SapEccRfcBapiVerificationOutcome> {
        let mut extra_headers = HashMap::from([
            ("accept".to_string(), "application/json".to_string()),
            ("x-requested-with".to_string(), "XMLHttpRequest".to_string()),
        ]);
        if let Some(client) = connection.get("ecc_rfc_client") {
            extra_headers.insert("x-sap-client".to_string(), client.clone());
        }
        if let Some(language) = connection.get("ecc_rfc_language") {
            extra_headers.insert("accept-language".to_string(), language.clone());
        }
        if let Some(system_id) = connection.get("ecc_rfc_system_id") {
            extra_headers.insert("x-sap-system-id".to_string(), system_id.clone());
        }

        let capabilities = parse_ecc_rfc_bapi_capabilities(connection);
        let request_config = parse_ecc_rfc_request_config(connection)?;
        validate_ecc_rfc_runtime_requirements(capabilities.as_ref(), &request_config)?;
        let projection = derive_sap_ecc_rfc_bapi_projection_fields(
            capabilities.as_ref(),
            target_field_path,
            expected_value,
            connection,
        );
        let pagination = parse_ecc_rfc_bapi_pagination_config(connection);
        let session_config =
            parse_ecc_rfc_session_config(connection, capabilities.as_ref(), &request_config);
        let mut request_parameters = request_config.request_parameters.clone();
        if let Some(language) = request_config.language.as_deref() {
            let parameter_name = capabilities
                .as_ref()
                .and_then(|caps| caps.language_parameter_name.as_deref())
                .or_else(|| {
                    connection
                        .get("ecc_rfc_language_parameter_name")
                        .map(String::as_str)
                })
                .unwrap_or("language");
            request_parameters
                .entry(parameter_name.to_string())
                .or_insert_with(|| language.to_string());
        }
        if let Some(page_size) = request_config.page_size {
            request_parameters
                .entry(pagination.page_size_parameter_name.clone())
                .or_insert_with(|| page_size.to_string());
        }
        let missing_required_parameters = find_missing_required_rfc_parameters(
            endpoint,
            &request_parameters,
            capabilities.as_ref(),
        )?;
        if !missing_required_parameters.is_empty() {
            return Err(anyhow!(
                "sap_ecc_rfc_bapi verification is missing required request parameter(s): {}",
                missing_required_parameters.join(", ")
            ));
        }
        let response = self
            .request_ecc_rfc_bapi_payload(
                endpoint,
                auth,
                extra_headers,
                &request_parameters,
                &pagination,
                &session_config,
            )
            .await?;
        let explicit_path = connection
            .get("ecc_rfc_value_path")
            .cloned()
            .or_else(|| connection.get("value_path").cloned());
        let fallback_path = if target_field_path.starts_with("$.") {
            Some(target_field_path)
        } else {
            None
        };
        let mut preferred_paths = Vec::new();
        if let Some(path) = explicit_path.as_deref() {
            preferred_paths.push(path);
        }
        if let Some(path) = fallback_path {
            if preferred_paths.iter().all(|candidate| *candidate != path) {
                preferred_paths.push(path);
            }
        }
        let actual_value = resolve_sap_ecc_rfc_bapi_value(response.payload, &preferred_paths)?;
        Ok(SapEccRfcBapiVerificationOutcome {
            actual_value,
            projection,
            profile: capabilities.as_ref().map(|caps| caps.profile.clone()),
            request_parameters,
            missing_required_parameters,
            session_mode: request_config.session_mode,
            backend_auth_mode: request_config.backend_auth_mode,
            language: request_config.language,
            page_size: request_config.page_size,
            session_id_present: response.session.session_id_present,
            session_reused: response.session.session_reused,
            session_closed: response.session.session_closed,
            session_ttl_seconds: response.session.session_ttl_seconds,
            metadata_capabilities: capabilities,
            page_count: response.page_count,
            paginated: response.page_count > 1,
            pagination_truncated: response.truncated,
            row_count: count_s4_odata_rows(&response.normalized_payload),
            next_cursor_remaining: response.next_cursor_remaining,
        })
    }

    async fn request_json(
        &self,
        endpoint: &graphica_core::migration_evidence::ConnectorEndpoint,
        auth: &graphica_core::migration_evidence::ConnectorAuth,
        extra_headers: HashMap<String, String>,
    ) -> Result<Value> {
        let method = reqwest::Method::from_bytes(endpoint.method.as_bytes())
            .context("invalid HTTP method")?;
        let url = format!(
            "{}{}",
            endpoint.base_url.trim_end_matches('/'),
            endpoint.path
        );
        self.request_json_url(&url, method, &endpoint.headers, auth, extra_headers)
            .await
    }

    async fn fetch_hana_actual_value(
        &self,
        connection: &HashMap<String, String>,
        auth: &graphica_core::migration_evidence::ConnectorAuth,
        query: &str,
    ) -> Result<Value> {
        let source = build_hana_source(connection)?;
        let connector = SAPHANAConnector::new();
        let credentials = Credentials {
            username: auth
                .username
                .clone()
                .unwrap_or_else(|| connection.get("username").cloned().unwrap_or_default()),
            password: auth
                .password
                .clone()
                .unwrap_or_else(|| connection.get("password").cloned().unwrap_or_default()),
            additional: [
                "odbc_driver",
                "odbc_dsn",
                "odbc_connection_string",
                "odbc_options",
            ]
            .into_iter()
            .filter_map(|key| {
                connection
                    .get(key)
                    .cloned()
                    .map(|value| (key.to_string(), value))
            })
            .collect(),
        };
        let result = connector
            .execute_query(&source, credentials, query, HashMap::new(), Some(1), 30)
            .await?;
        first_query_value(result)
    }
}

#[derive(Debug, Clone)]
struct SapS4ODataVerificationOutcome {
    actual_value: Value,
    projection: SapS4ODataProjectionFields,
    metadata_capabilities: Option<SapS4ODataCapabilities>,
    page_count: usize,
    paginated: bool,
    pagination_truncated: bool,
    row_count: Option<usize>,
    next_link_remaining: Option<String>,
}

#[derive(Debug, Clone)]
struct SapEccAdapterVerificationOutcome {
    actual_value: Value,
    projection: SapEccProjectionFields,
    metadata_capabilities: Option<SapEccAdapterCapabilities>,
    request_parameters: HashMap<String, String>,
    missing_required_parameters: Vec<String>,
    session_mode: Option<SapEccSessionMode>,
    backend_auth_mode: Option<SapEccBackendAuthMode>,
    language: Option<String>,
    page_size: Option<usize>,
    session_id_present: bool,
    session_reused: bool,
    session_closed: bool,
    session_ttl_seconds: Option<u64>,
    page_count: usize,
    paginated: bool,
    pagination_truncated: bool,
    row_count: Option<usize>,
    next_link_remaining: Option<String>,
}

#[derive(Debug, Clone)]
struct SapEccRfcBapiVerificationOutcome {
    actual_value: Value,
    projection: SapEccRfcBapiProjectionFields,
    metadata_capabilities: Option<SapEccRfcBapiCapabilities>,
    profile: Option<SapEccRfcBapiProfile>,
    request_parameters: HashMap<String, String>,
    missing_required_parameters: Vec<String>,
    session_mode: Option<SapEccSessionMode>,
    backend_auth_mode: Option<SapEccBackendAuthMode>,
    language: Option<String>,
    page_size: Option<usize>,
    session_id_present: bool,
    session_reused: bool,
    session_closed: bool,
    session_ttl_seconds: Option<u64>,
    page_count: usize,
    paginated: bool,
    pagination_truncated: bool,
    row_count: Option<usize>,
    next_cursor_remaining: Option<String>,
}

#[derive(Debug, Clone)]
struct SapS4ODataPageFetchResult {
    payload: Value,
    normalized_payload: Value,
    page_count: usize,
    truncated: bool,
    next_link_remaining: Option<String>,
}

#[derive(Debug, Clone)]
struct SapEccAdapterPageFetchResult {
    payload: Value,
    normalized_payload: Value,
    page_count: usize,
    truncated: bool,
    next_path_remaining: Option<String>,
    session: BridgeSessionOutcome,
}

#[derive(Debug, Clone)]
struct SapEccRfcBapiPageFetchResult {
    payload: Value,
    normalized_payload: Value,
    page_count: usize,
    truncated: bool,
    next_cursor_remaining: Option<String>,
    session: BridgeSessionOutcome,
}

#[derive(Debug, Clone, Copy)]
struct SapS4ODataPaginationConfig {
    follow_next_link: bool,
    max_pages: usize,
}

#[derive(Debug, Clone)]
struct SapEccAdapterPaginationConfig {
    follow_next_path: bool,
    max_pages: usize,
    page_size_parameter_name: String,
}

#[derive(Debug, Clone)]
struct SapEccRfcBapiPaginationConfig {
    follow_next_cursor: bool,
    max_pages: usize,
    page_size_parameter_name: String,
    cursor_parameter_name: String,
    next_cursor_path: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct SapEccAdapterSessionConfig {
    session_mode: Option<SapEccSessionMode>,
    session_id_path: Option<String>,
    session_id_parameter_name: String,
    close_session_path: Option<String>,
    close_session_method: String,
    requires_explicit_session_close: bool,
    session_ttl_seconds: Option<u64>,
}

#[derive(Debug, Clone, Default)]
struct SapEccRfcSessionConfig {
    session_mode: Option<SapEccSessionMode>,
    session_id_path: Option<String>,
    session_id_parameter_name: String,
    close_session_path: Option<String>,
    close_session_method: String,
    requires_explicit_session_close: bool,
    session_ttl_seconds: Option<u64>,
}

#[derive(Debug, Clone, Default)]
struct SapEccAdapterRequestConfig {
    request_parameters: HashMap<String, String>,
    session_mode: Option<SapEccSessionMode>,
    backend_auth_mode: Option<SapEccBackendAuthMode>,
    language: Option<String>,
    page_size: Option<usize>,
}

#[derive(Debug, Clone, Default)]
struct SapEccRfcBapiRequestConfig {
    request_parameters: HashMap<String, String>,
    session_mode: Option<SapEccSessionMode>,
    backend_auth_mode: Option<SapEccBackendAuthMode>,
    language: Option<String>,
    page_size: Option<usize>,
}

impl VerificationManager {
    async fn request_json_url(
        &self,
        url: &str,
        method: reqwest::Method,
        endpoint_headers: &HashMap<String, String>,
        auth: &graphica_core::migration_evidence::ConnectorAuth,
        extra_headers: HashMap<String, String>,
    ) -> Result<Value> {
        let client = reqwest::Client::new();
        let mut request = client.request(method, url);
        for (key, value) in endpoint_headers {
            request = request.header(key, value);
        }
        for (key, value) in extra_headers {
            request = request.header(key, value);
        }
        match auth.kind {
            graphica_core::migration_evidence::ConnectorAuthKind::Bearer => {
                if let Some(token) = auth.token.as_deref() {
                    request = request.bearer_auth(token);
                }
            }
            graphica_core::migration_evidence::ConnectorAuthKind::ApiKey => {
                if let (Some(header), Some(api_key)) =
                    (auth.header_name.as_deref(), auth.api_key.as_deref())
                {
                    request = request.header(header, api_key);
                }
            }
            graphica_core::migration_evidence::ConnectorAuthKind::Basic => {
                request = request.basic_auth(
                    auth.username.clone().unwrap_or_default(),
                    auth.password.clone(),
                );
            }
            graphica_core::migration_evidence::ConnectorAuthKind::None => {}
        }

        let response = request.send().await?.error_for_status()?;
        let value: Value = response.json().await?;
        Ok(value)
    }

    async fn lookup_cached_session(&self, cache_key: &str) -> Option<CachedBridgeSession> {
        let now = Utc::now();
        let mut cache = self.session_cache.lock().await;
        if let Some(entry) = cache.get(cache_key).cloned() {
            if entry
                .expires_at
                .map(|expires_at| expires_at > now)
                .unwrap_or(true)
            {
                return Some(entry);
            }
            cache.remove(cache_key);
        }
        None
    }

    async fn store_cached_session(
        &self,
        cache_key: &str,
        session_id: &str,
        ttl_seconds: Option<u64>,
    ) {
        let expires_at = ttl_seconds.and_then(|ttl| {
            chrono::Duration::from_std(std::time::Duration::from_secs(ttl))
                .ok()
                .map(|duration| Utc::now() + duration)
        });
        self.session_cache.lock().await.insert(
            cache_key.to_string(),
            CachedBridgeSession {
                session_id: session_id.to_string(),
                expires_at,
            },
        );
    }

    async fn close_bridge_session(
        &self,
        base_url: &str,
        auth: &graphica_core::migration_evidence::ConnectorAuth,
        endpoint_headers: &HashMap<String, String>,
        extra_headers: HashMap<String, String>,
        method_name: &str,
        close_path: &str,
        session_parameter_name: &str,
        session_id: &str,
    ) -> Result<()> {
        let method = reqwest::Method::from_bytes(method_name.as_bytes())
            .with_context(|| format!("invalid bridge close-session HTTP method '{method_name}'"))?;
        let url = append_query_parameters(
            &resolve_s4_odata_next_link(base_url, close_path)?,
            &HashMap::from([(session_parameter_name.to_string(), session_id.to_string())]),
        )?;
        let client = reqwest::Client::new();
        let mut request = client.request(method, &url);
        for (key, value) in endpoint_headers {
            request = request.header(key, value);
        }
        for (key, value) in extra_headers {
            request = request.header(key, value);
        }
        match auth.kind {
            graphica_core::migration_evidence::ConnectorAuthKind::Bearer => {
                if let Some(token) = auth.token.as_deref() {
                    request = request.bearer_auth(token);
                }
            }
            graphica_core::migration_evidence::ConnectorAuthKind::ApiKey => {
                if let (Some(header), Some(api_key)) =
                    (auth.header_name.as_deref(), auth.api_key.as_deref())
                {
                    request = request.header(header, api_key);
                }
            }
            graphica_core::migration_evidence::ConnectorAuthKind::Basic => {
                request = request.basic_auth(
                    auth.username.clone().unwrap_or_default(),
                    auth.password.clone(),
                );
            }
            graphica_core::migration_evidence::ConnectorAuthKind::None => {}
        }
        request.send().await?.error_for_status()?;
        Ok(())
    }

    async fn request_s4_odata_payload(
        &self,
        endpoint: &graphica_core::migration_evidence::ConnectorEndpoint,
        auth: &graphica_core::migration_evidence::ConnectorAuth,
        extra_headers: HashMap<String, String>,
        pagination: &SapS4ODataPaginationConfig,
    ) -> Result<SapS4ODataPageFetchResult> {
        let method = reqwest::Method::from_bytes(endpoint.method.as_bytes())
            .context("invalid SAP S/4 OData HTTP method")?;
        let initial_url = format!(
            "{}{}",
            endpoint.base_url.trim_end_matches('/'),
            endpoint.path
        );
        let mut raw_payload = self
            .request_json_url(
                &initial_url,
                method.clone(),
                &endpoint.headers,
                auth,
                extra_headers.clone(),
            )
            .await?;
        let mut merged_payload = raw_payload.clone();
        let mut page_count = 1usize;
        let mut next_link_remaining = extract_sap_s4_odata_next_link(&raw_payload);
        let mut truncated = false;

        while pagination.follow_next_link {
            let Some(next_link) = next_link_remaining.clone() else {
                break;
            };
            if page_count >= pagination.max_pages {
                truncated = true;
                break;
            }

            let next_url = resolve_s4_odata_next_link(&initial_url, &next_link)?;
            raw_payload = self
                .request_json_url(
                    &next_url,
                    method.clone(),
                    &endpoint.headers,
                    auth,
                    extra_headers.clone(),
                )
                .await?;
            merged_payload = merge_sap_s4_odata_page_payloads(merged_payload, raw_payload.clone());
            next_link_remaining = extract_sap_s4_odata_next_link(&raw_payload);
            page_count += 1;
        }

        let normalized_payload = graphica_core::migration_evidence::normalize_sap_s4_odata_payload(
            merged_payload.clone(),
        );
        Ok(SapS4ODataPageFetchResult {
            payload: merged_payload,
            normalized_payload,
            page_count,
            truncated,
            next_link_remaining,
        })
    }

    async fn request_ecc_adapter_payload(
        &self,
        endpoint: &graphica_core::migration_evidence::ConnectorEndpoint,
        auth: &graphica_core::migration_evidence::ConnectorAuth,
        extra_headers: HashMap<String, String>,
        request_parameters: &HashMap<String, String>,
        pagination: &SapEccAdapterPaginationConfig,
        session: &SapEccAdapterSessionConfig,
    ) -> Result<SapEccAdapterPageFetchResult> {
        let method = reqwest::Method::from_bytes(endpoint.method.as_bytes())
            .context("invalid SAP ECC adapter HTTP method")?;
        let base_url = format!(
            "{}{}",
            endpoint.base_url.trim_end_matches('/'),
            endpoint.path
        );
        let cache_key = bridge_session_cache_key(&base_url, &session.session_id_parameter_name);
        let cached_session = if matches!(session.session_mode, Some(SapEccSessionMode::Cached)) {
            self.lookup_cached_session(&cache_key).await
        } else {
            None
        };
        let mut initial_parameters = request_parameters.clone();
        let mut session_outcome = BridgeSessionOutcome {
            session_reused: cached_session.is_some(),
            session_ttl_seconds: session.session_ttl_seconds,
            ..BridgeSessionOutcome::default()
        };
        if let Some(cached) = cached_session.as_ref() {
            initial_parameters
                .entry(session.session_id_parameter_name.clone())
                .or_insert_with(|| cached.session_id.clone());
        }
        let initial_url = append_query_parameters(&base_url, &initial_parameters)?;
        let mut raw_payload = self
            .request_json_url(
                &initial_url,
                method.clone(),
                &endpoint.headers,
                auth,
                extra_headers.clone(),
            )
            .await?;
        let mut active_session_id =
            extract_bridge_session_id(&raw_payload, session.session_id_path.as_deref());
        session_outcome.session_id_present = active_session_id.is_some();
        let mut merged_payload = raw_payload.clone();
        let mut page_count = 1usize;
        let mut next_path_remaining = extract_sap_ecc_adapter_next_path(&raw_payload);
        let mut truncated = false;

        while pagination.follow_next_path {
            let Some(next_path) = next_path_remaining.clone() else {
                break;
            };
            if page_count >= pagination.max_pages {
                truncated = true;
                break;
            }

            let mut next_url = resolve_s4_odata_next_link(&initial_url, &next_path)?;
            if let Some(session_id) = active_session_id.as_deref() {
                next_url = append_query_parameters(
                    &next_url,
                    &HashMap::from([(
                        session.session_id_parameter_name.clone(),
                        session_id.to_string(),
                    )]),
                )?;
            }
            raw_payload = self
                .request_json_url(
                    &next_url,
                    method.clone(),
                    &endpoint.headers,
                    auth,
                    extra_headers.clone(),
                )
                .await?;
            merged_payload =
                merge_sap_ecc_adapter_page_payloads(merged_payload, raw_payload.clone());
            if let Some(session_id) =
                extract_bridge_session_id(&raw_payload, session.session_id_path.as_deref())
            {
                active_session_id = Some(session_id);
                session_outcome.session_id_present = true;
            }
            next_path_remaining = extract_sap_ecc_adapter_next_path(&raw_payload);
            page_count += 1;
        }

        if let Some(session_id) = active_session_id.as_deref() {
            match session.session_mode {
                Some(SapEccSessionMode::Cached) => {
                    self.store_cached_session(&cache_key, session_id, session.session_ttl_seconds)
                        .await;
                }
                Some(SapEccSessionMode::Stateful)
                    if session.requires_explicit_session_close
                        && session.close_session_path.is_some() =>
                {
                    self.close_bridge_session(
                        &base_url,
                        auth,
                        &endpoint.headers,
                        extra_headers.clone(),
                        session.close_session_method.as_str(),
                        session.close_session_path.as_deref().unwrap_or_default(),
                        &session.session_id_parameter_name,
                        session_id,
                    )
                    .await?;
                    session_outcome.session_closed = true;
                }
                _ => {}
            }
        }

        let normalized_payload =
            graphica_core::migration_evidence::normalize_sap_ecc_adapter_payload(
                merged_payload.clone(),
            );
        Ok(SapEccAdapterPageFetchResult {
            payload: merged_payload,
            normalized_payload,
            page_count,
            truncated,
            next_path_remaining,
            session: session_outcome,
        })
    }

    async fn request_ecc_rfc_bapi_payload(
        &self,
        endpoint: &graphica_core::migration_evidence::ConnectorEndpoint,
        auth: &graphica_core::migration_evidence::ConnectorAuth,
        extra_headers: HashMap<String, String>,
        request_parameters: &HashMap<String, String>,
        pagination: &SapEccRfcBapiPaginationConfig,
        session: &SapEccRfcSessionConfig,
    ) -> Result<SapEccRfcBapiPageFetchResult> {
        let method = reqwest::Method::from_bytes(endpoint.method.as_bytes())
            .context("invalid SAP ECC RFC/BAPI bridge HTTP method")?;
        let base_url = format!(
            "{}{}",
            endpoint.base_url.trim_end_matches('/'),
            endpoint.path
        );
        let cache_key = bridge_session_cache_key(&base_url, &session.session_id_parameter_name);
        let cached_session = if matches!(session.session_mode, Some(SapEccSessionMode::Cached)) {
            self.lookup_cached_session(&cache_key).await
        } else {
            None
        };
        let mut initial_parameters = request_parameters.clone();
        let mut session_outcome = BridgeSessionOutcome {
            session_reused: cached_session.is_some(),
            session_ttl_seconds: session.session_ttl_seconds,
            ..BridgeSessionOutcome::default()
        };
        if let Some(cached) = cached_session.as_ref() {
            initial_parameters
                .entry(session.session_id_parameter_name.clone())
                .or_insert_with(|| cached.session_id.clone());
        }
        let initial_url = append_query_parameters(&base_url, &initial_parameters)?;
        let mut raw_payload = self
            .request_json_url(
                &initial_url,
                method.clone(),
                &endpoint.headers,
                auth,
                extra_headers.clone(),
            )
            .await?;
        let mut active_session_id =
            extract_bridge_session_id(&raw_payload, session.session_id_path.as_deref());
        session_outcome.session_id_present = active_session_id.is_some();
        let mut merged_payload = raw_payload.clone();
        let mut page_count = 1usize;
        let mut next_cursor_remaining = extract_sap_ecc_rfc_bapi_next_cursor_from_path(
            &raw_payload,
            pagination.next_cursor_path.as_deref(),
        );
        let mut truncated = false;

        while pagination.follow_next_cursor {
            let Some(next_cursor) = next_cursor_remaining.clone() else {
                break;
            };
            if page_count >= pagination.max_pages {
                truncated = true;
                break;
            }

            let mut next_url = append_cursor_query(
                &initial_url,
                &pagination.cursor_parameter_name,
                &next_cursor,
            )?;
            if let Some(session_id) = active_session_id.as_deref() {
                next_url = append_query_parameters(
                    &next_url,
                    &HashMap::from([(
                        session.session_id_parameter_name.clone(),
                        session_id.to_string(),
                    )]),
                )?;
            }
            raw_payload = self
                .request_json_url(
                    &next_url,
                    method.clone(),
                    &endpoint.headers,
                    auth,
                    extra_headers.clone(),
                )
                .await?;
            merged_payload =
                merge_sap_ecc_rfc_bapi_page_payloads(merged_payload, raw_payload.clone());
            if let Some(session_id) =
                extract_bridge_session_id(&raw_payload, session.session_id_path.as_deref())
            {
                active_session_id = Some(session_id);
                session_outcome.session_id_present = true;
            }
            next_cursor_remaining = extract_sap_ecc_rfc_bapi_next_cursor_from_path(
                &raw_payload,
                pagination.next_cursor_path.as_deref(),
            );
            page_count += 1;
        }

        if let Some(session_id) = active_session_id.as_deref() {
            match session.session_mode {
                Some(SapEccSessionMode::Cached) => {
                    self.store_cached_session(&cache_key, session_id, session.session_ttl_seconds)
                        .await;
                }
                Some(SapEccSessionMode::Stateful)
                    if session.requires_explicit_session_close
                        && session.close_session_path.is_some() =>
                {
                    self.close_bridge_session(
                        &base_url,
                        auth,
                        &endpoint.headers,
                        extra_headers.clone(),
                        session.close_session_method.as_str(),
                        session.close_session_path.as_deref().unwrap_or_default(),
                        &session.session_id_parameter_name,
                        session_id,
                    )
                    .await?;
                    session_outcome.session_closed = true;
                }
                _ => {}
            }
        }

        let normalized_payload =
            graphica_core::migration_evidence::normalize_sap_ecc_rfc_bapi_payload(
                merged_payload.clone(),
            );
        Ok(SapEccRfcBapiPageFetchResult {
            payload: merged_payload,
            normalized_payload,
            page_count,
            truncated,
            next_cursor_remaining,
            session: session_outcome,
        })
    }
}

fn parse_s4_odata_capabilities(
    connection: &HashMap<String, String>,
) -> Option<SapS4ODataCapabilities> {
    let property_types = connection
        .get("odata_property_types_json")
        .and_then(|raw| serde_json::from_str::<HashMap<String, String>>(raw).ok())?;
    let mut properties = property_types
        .into_iter()
        .map(|(name, edm_type)| SapS4ODataProperty {
            name,
            edm_type,
            nullable: true,
        })
        .collect::<Vec<_>>();
    properties.sort_by(|left, right| left.name.cmp(&right.name));

    Some(SapS4ODataCapabilities {
        service_root_path: connection
            .get("odata_service_root_path")
            .cloned()
            .unwrap_or_default(),
        metadata_path: connection
            .get("odata_metadata_path")
            .cloned()
            .unwrap_or_default(),
        version: match connection.get("odata_metadata_version").map(String::as_str) {
            Some("v2") => SapS4ODataVersion::V2,
            Some("v4") => SapS4ODataVersion::V4,
            _ => SapS4ODataVersion::Unknown,
        },
        entity_set: connection.get("odata_entity_set").cloned(),
        entity_type: connection.get("odata_entity_type").cloned(),
        key_fields: connection
            .get("odata_key_fields_json")
            .and_then(|raw| serde_json::from_str::<Vec<String>>(raw).ok())
            .unwrap_or_default(),
        supports_record_projection: connection
            .get("odata_supports_record_projection")
            .map(|value| value == "true")
            .unwrap_or(!properties.is_empty()),
        supports_rowset_projection: connection
            .get("odata_supports_rowset_projection")
            .map(|value| value == "true")
            .unwrap_or(true),
        properties,
    })
}

fn parse_s4_odata_pagination_config(
    connection: &HashMap<String, String>,
) -> SapS4ODataPaginationConfig {
    let follow_next_link = connection
        .get("odata_follow_next_link")
        .map(|value| value != "false")
        .unwrap_or(true);
    let max_pages = connection
        .get("odata_max_pages")
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(25);

    SapS4ODataPaginationConfig {
        follow_next_link,
        max_pages,
    }
}

fn parse_ecc_adapter_capabilities(
    connection: &HashMap<String, String>,
) -> Option<SapEccAdapterCapabilities> {
    let field_types = connection
        .get("ecc_field_types_json")
        .and_then(|raw| serde_json::from_str::<HashMap<String, String>>(raw).ok())?;
    let mut fields = field_types
        .into_iter()
        .map(|(name, abap_type)| SapEccAdapterField {
            name,
            abap_type,
            nullable: true,
        })
        .collect::<Vec<_>>();
    fields.sort_by(|left, right| left.name.cmp(&right.name));

    Some(SapEccAdapterCapabilities {
        adapter_version: connection.get("ecc_adapter_version").cloned(),
        system_id: connection.get("ecc_system_id").cloned(),
        client: connection.get("ecc_client").cloned(),
        object_name: connection.get("ecc_object_name").cloned(),
        key_fields: connection
            .get("ecc_key_fields_json")
            .and_then(|raw| serde_json::from_str::<Vec<String>>(raw).ok())
            .unwrap_or_default(),
        required_parameters: connection
            .get("ecc_required_parameters_json")
            .and_then(|raw| serde_json::from_str::<Vec<String>>(raw).ok())
            .unwrap_or_default(),
        supported_auth_modes: connection
            .get("ecc_supported_auth_modes_json")
            .and_then(|raw| serde_json::from_str::<Vec<SapEccBackendAuthMode>>(raw).ok())
            .unwrap_or_default(),
        supported_session_modes: connection
            .get("ecc_supported_session_modes_json")
            .and_then(|raw| serde_json::from_str::<Vec<SapEccSessionMode>>(raw).ok())
            .unwrap_or_default(),
        health_path: connection.get("ecc_health_path").cloned(),
        session_id_path: connection.get("ecc_session_id_path").cloned(),
        session_id_parameter_name: connection.get("ecc_session_id_parameter_name").cloned(),
        close_session_path: connection.get("ecc_close_session_path").cloned(),
        close_session_method: connection.get("ecc_close_session_method").cloned(),
        requires_explicit_session_close: connection
            .get("ecc_requires_explicit_session_close")
            .map(|value| value == "true")
            .unwrap_or(false),
        session_ttl_seconds: connection
            .get("ecc_session_ttl_seconds")
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|value| *value > 0),
        max_page_size: connection
            .get("ecc_max_page_size")
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|value| *value > 0),
        page_size_parameter_name: connection.get("ecc_page_size_parameter_name").cloned(),
        language_parameter_name: connection.get("ecc_language_parameter_name").cloned(),
        supports_record_projection: connection
            .get("ecc_supports_record_projection")
            .map(|value| value == "true")
            .unwrap_or(!fields.is_empty()),
        supports_rowset_projection: connection
            .get("ecc_supports_rowset_projection")
            .map(|value| value == "true")
            .unwrap_or(true),
        supports_key_lookup: connection
            .get("ecc_supports_key_lookup")
            .map(|value| value == "true")
            .unwrap_or(true),
        fields,
    })
}

fn parse_ecc_adapter_pagination_config(
    connection: &HashMap<String, String>,
) -> SapEccAdapterPaginationConfig {
    let follow_next_path = connection
        .get("ecc_follow_next_path")
        .map(|value| value != "false")
        .unwrap_or(true);
    let max_pages = connection
        .get("ecc_max_pages")
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(25);
    let page_size_parameter_name = connection
        .get("ecc_page_size_parameter_name")
        .cloned()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "page_size".to_string());

    SapEccAdapterPaginationConfig {
        follow_next_path,
        max_pages,
        page_size_parameter_name,
    }
}

fn parse_ecc_rfc_bapi_capabilities(
    connection: &HashMap<String, String>,
) -> Option<SapEccRfcBapiCapabilities> {
    let field_types = connection
        .get("ecc_rfc_field_types_json")
        .and_then(|raw| serde_json::from_str::<HashMap<String, String>>(raw).ok())?;
    let mut fields = field_types
        .into_iter()
        .map(|(name, abap_type)| SapEccRfcBapiField {
            name,
            abap_type,
            nullable: true,
        })
        .collect::<Vec<_>>();
    fields.sort_by(|left, right| left.name.cmp(&right.name));

    Some(SapEccRfcBapiCapabilities {
        profile: connection
            .get("ecc_rfc_profile")
            .map(|value| parse_rfc_profile_value(value))
            .unwrap_or_else(|| {
                infer_rfc_profile_from_connection(
                    connection.get("ecc_rfc_bapi_name").map(String::as_str),
                    connection
                        .get("ecc_rfc_function_module")
                        .map(String::as_str),
                    connection
                        .get("ecc_rfc_export_structure")
                        .map(String::as_str),
                )
            }),
        bridge_version: connection.get("ecc_rfc_bridge_version").cloned(),
        system_id: connection.get("ecc_rfc_system_id").cloned(),
        client: connection.get("ecc_rfc_client").cloned(),
        function_module: connection.get("ecc_rfc_function_module").cloned(),
        bapi_name: connection.get("ecc_rfc_bapi_name").cloned(),
        export_structure: connection.get("ecc_rfc_export_structure").cloned(),
        key_fields: connection
            .get("ecc_rfc_key_fields_json")
            .and_then(|raw| serde_json::from_str::<Vec<String>>(raw).ok())
            .unwrap_or_default(),
        required_parameters: connection
            .get("ecc_rfc_required_parameters_json")
            .and_then(|raw| serde_json::from_str::<Vec<String>>(raw).ok())
            .unwrap_or_default(),
        supported_auth_modes: connection
            .get("ecc_rfc_supported_auth_modes_json")
            .and_then(|raw| serde_json::from_str::<Vec<SapEccBackendAuthMode>>(raw).ok())
            .unwrap_or_default(),
        supported_session_modes: connection
            .get("ecc_rfc_supported_session_modes_json")
            .and_then(|raw| serde_json::from_str::<Vec<SapEccSessionMode>>(raw).ok())
            .unwrap_or_default(),
        health_path: connection.get("ecc_rfc_health_path").cloned(),
        session_id_path: connection.get("ecc_rfc_session_id_path").cloned(),
        session_id_parameter_name: connection.get("ecc_rfc_session_id_parameter_name").cloned(),
        close_session_path: connection.get("ecc_rfc_close_session_path").cloned(),
        close_session_method: connection.get("ecc_rfc_close_session_method").cloned(),
        requires_explicit_session_close: connection
            .get("ecc_rfc_requires_explicit_session_close")
            .map(|value| value == "true")
            .unwrap_or(false),
        session_ttl_seconds: connection
            .get("ecc_rfc_session_ttl_seconds")
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|value| *value > 0),
        max_page_size: connection
            .get("ecc_rfc_max_page_size")
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|value| *value > 0),
        page_size_parameter_name: connection.get("ecc_rfc_page_size_parameter_name").cloned(),
        language_parameter_name: connection.get("ecc_rfc_language_parameter_name").cloned(),
        cursor_parameter_name: connection.get("ecc_rfc_cursor_parameter_name").cloned(),
        next_cursor_path: connection.get("ecc_rfc_next_cursor_path").cloned(),
        supports_record_projection: connection
            .get("ecc_rfc_supports_record_projection")
            .map(|value| value == "true")
            .unwrap_or(!fields.is_empty()),
        supports_rowset_projection: connection
            .get("ecc_rfc_supports_rowset_projection")
            .map(|value| value == "true")
            .unwrap_or(true),
        supports_key_lookup: connection
            .get("ecc_rfc_supports_key_lookup")
            .map(|value| value == "true")
            .unwrap_or(true),
        supports_cursor_pagination: connection
            .get("ecc_rfc_supports_cursor_pagination")
            .map(|value| value == "true")
            .unwrap_or(true),
        fields,
    })
}

fn parse_ecc_rfc_bapi_pagination_config(
    connection: &HashMap<String, String>,
) -> SapEccRfcBapiPaginationConfig {
    let follow_next_cursor = connection
        .get("ecc_rfc_follow_next_cursor")
        .map(|value| value != "false")
        .unwrap_or(true);
    let max_pages = connection
        .get("ecc_rfc_max_pages")
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(25);
    let page_size_parameter_name = connection
        .get("ecc_rfc_page_size_parameter_name")
        .cloned()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "page_size".to_string());
    let cursor_parameter_name = connection
        .get("ecc_rfc_cursor_parameter_name")
        .cloned()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "cursor".to_string());
    let next_cursor_path = connection
        .get("ecc_rfc_next_cursor_path")
        .cloned()
        .filter(|value| !value.trim().is_empty());

    SapEccRfcBapiPaginationConfig {
        follow_next_cursor,
        max_pages,
        page_size_parameter_name,
        cursor_parameter_name,
        next_cursor_path,
    }
}

fn resolve_s4_odata_next_link(initial_url: &str, next_link: &str) -> Result<String> {
    if let Ok(url) = reqwest::Url::parse(next_link) {
        return Ok(url.to_string());
    }

    let base = reqwest::Url::parse(initial_url)
        .with_context(|| format!("invalid SAP S/4 OData base URL '{initial_url}'"))?;
    let joined = base
        .join(next_link)
        .with_context(|| format!("invalid SAP S/4 OData next link '{next_link}'"))?;
    Ok(joined.to_string())
}

fn append_query_parameters(
    initial_url: &str,
    parameters: &HashMap<String, String>,
) -> Result<String> {
    let mut url = reqwest::Url::parse(initial_url)
        .with_context(|| format!("invalid SAP ECC RFC/BAPI bridge base URL '{initial_url}'"))?;
    if !parameters.is_empty() {
        let existing = url
            .query_pairs()
            .map(|(key, _)| key.to_string())
            .collect::<std::collections::HashSet<_>>();
        let mut query_pairs = url.query_pairs_mut();
        for (key, value) in parameters {
            if !existing.contains(key) {
                query_pairs.append_pair(key, value);
            }
        }
    }
    Ok(url.to_string())
}

fn append_cursor_query(
    initial_url: &str,
    cursor_parameter_name: &str,
    cursor: &str,
) -> Result<String> {
    let mut url = reqwest::Url::parse(initial_url)
        .with_context(|| format!("invalid SAP ECC RFC/BAPI bridge base URL '{initial_url}'"))?;
    url.query_pairs_mut()
        .append_pair(cursor_parameter_name, cursor);
    Ok(url.to_string())
}

fn bridge_session_cache_key(base_url: &str, session_parameter_name: &str) -> String {
    format!("{base_url}::{session_parameter_name}")
}

fn extract_bridge_session_id(payload: &Value, session_id_path: Option<&str>) -> Option<String> {
    let path = session_id_path?;
    extract_json_path_value(payload, path)
        .and_then(|value| match value {
            Value::String(value) => Some(value),
            Value::Number(value) => Some(value.to_string()),
            Value::Bool(value) => Some(value.to_string()),
            _ => None,
        })
        .filter(|value| !value.trim().is_empty())
}

fn apply_auth_resolution_metadata(
    metadata: &mut HashMap<String, String>,
    prefix: &str,
    resolution: &ConnectorAuthResolutionMetadata,
) {
    if let Some(secret_ref) = resolution.secret_ref.as_deref() {
        metadata.insert(format!("{prefix}secret_ref"), secret_ref.to_string());
    }
    if let Some(secret_store) = resolution.secret_store.as_deref() {
        metadata.insert(format!("{prefix}secret_store"), secret_store.to_string());
    }
    if let Some(secret_version) = resolution.secret_version.as_deref() {
        metadata.insert(
            format!("{prefix}secret_version"),
            secret_version.to_string(),
        );
    }
    if let Some(interval_days) = resolution.rotation_interval_days {
        metadata.insert(
            format!("{prefix}rotation_interval_days"),
            interval_days.to_string(),
        );
    }
    if let Some(next_rotation) = resolution.next_rotation {
        metadata.insert(format!("{prefix}next_rotation"), next_rotation.to_rfc3339());
    }
    if let Some(last_rotated) = resolution.last_rotated {
        metadata.insert(format!("{prefix}last_rotated"), last_rotated.to_rfc3339());
    }
}

fn parse_rfc_profile_value(value: &str) -> SapEccRfcBapiProfile {
    match value {
        "bapi_record_lookup" | "bapi_lookup" | "bapi" => SapEccRfcBapiProfile::BapiRecordLookup,
        "function_module_export" | "function_module" | "rfc_function" => {
            SapEccRfcBapiProfile::FunctionModuleExport
        }
        "table_read_rowset" | "table_read" | "rowset" => SapEccRfcBapiProfile::TableReadRowset,
        _ => SapEccRfcBapiProfile::QueryBridge,
    }
}

fn infer_rfc_profile_from_connection(
    bapi_name: Option<&str>,
    function_module: Option<&str>,
    export_structure: Option<&str>,
) -> SapEccRfcBapiProfile {
    if bapi_name.is_some() {
        SapEccRfcBapiProfile::BapiRecordLookup
    } else if function_module.is_some() && export_structure.is_some() {
        SapEccRfcBapiProfile::FunctionModuleExport
    } else if export_structure.is_some() {
        SapEccRfcBapiProfile::TableReadRowset
    } else {
        SapEccRfcBapiProfile::QueryBridge
    }
}

fn parse_request_parameters_json(
    connection: &HashMap<String, String>,
    key: &str,
) -> Result<HashMap<String, String>> {
    if let Some(raw) = connection.get(key) {
        let value: Value =
            serde_json::from_str(raw).with_context(|| format!("{key} must be valid JSON"))?;
        let object = value
            .as_object()
            .ok_or_else(|| anyhow!("{key} must be a JSON object"))?;
        let mut parameters = HashMap::new();
        for (key, value) in object {
            let rendered = match value {
                Value::Null => continue,
                Value::String(value) => value.clone(),
                Value::Number(value) => value.to_string(),
                Value::Bool(value) => value.to_string(),
                _ => {
                    return Err(anyhow!(
                        "{key} values must be strings, numbers, booleans, or null"
                    ))
                }
            };
            parameters.insert(key.clone(), rendered);
        }
        return Ok(parameters);
    }

    Ok(HashMap::new())
}

fn parse_ecc_adapter_request_config(
    connection: &HashMap<String, String>,
) -> Result<SapEccAdapterRequestConfig> {
    Ok(SapEccAdapterRequestConfig {
        request_parameters: parse_request_parameters_json(connection, "ecc_request_params_json")?,
        session_mode: connection
            .get("ecc_session_mode")
            .map(|value| parse_session_mode_value(value))
            .transpose()?,
        backend_auth_mode: connection
            .get("ecc_backend_auth_mode")
            .map(|value| parse_backend_auth_mode_value(value))
            .transpose()?,
        language: connection.get("ecc_language").cloned(),
        page_size: connection
            .get("ecc_page_size")
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|value| *value > 0),
    })
}

fn parse_ecc_rfc_request_config(
    connection: &HashMap<String, String>,
) -> Result<SapEccRfcBapiRequestConfig> {
    Ok(SapEccRfcBapiRequestConfig {
        request_parameters: parse_request_parameters_json(
            connection,
            "ecc_rfc_request_params_json",
        )?,
        session_mode: connection
            .get("ecc_rfc_session_mode")
            .map(|value| parse_session_mode_value(value))
            .transpose()?,
        backend_auth_mode: connection
            .get("ecc_rfc_backend_auth_mode")
            .map(|value| parse_backend_auth_mode_value(value))
            .transpose()?,
        language: connection.get("ecc_rfc_language").cloned(),
        page_size: connection
            .get("ecc_rfc_page_size")
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|value| *value > 0),
    })
}

fn parse_ecc_adapter_session_config(
    connection: &HashMap<String, String>,
    capabilities: Option<&SapEccAdapterCapabilities>,
    request: &SapEccAdapterRequestConfig,
) -> SapEccAdapterSessionConfig {
    SapEccAdapterSessionConfig {
        session_mode: request.session_mode.clone(),
        session_id_path: capabilities
            .and_then(|caps| caps.session_id_path.clone())
            .or_else(|| connection.get("ecc_session_id_path").cloned()),
        session_id_parameter_name: capabilities
            .and_then(|caps| caps.session_id_parameter_name.clone())
            .or_else(|| connection.get("ecc_session_id_parameter_name").cloned())
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "session_id".to_string()),
        close_session_path: capabilities
            .and_then(|caps| caps.close_session_path.clone())
            .or_else(|| connection.get("ecc_close_session_path").cloned()),
        close_session_method: capabilities
            .and_then(|caps| caps.close_session_method.clone())
            .or_else(|| connection.get("ecc_close_session_method").cloned())
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "POST".to_string()),
        requires_explicit_session_close: capabilities
            .map(|caps| caps.requires_explicit_session_close)
            .unwrap_or_else(|| {
                connection
                    .get("ecc_requires_explicit_session_close")
                    .map(|value| value == "true")
                    .unwrap_or(false)
            }),
        session_ttl_seconds: capabilities
            .and_then(|caps| caps.session_ttl_seconds)
            .or_else(|| {
                connection
                    .get("ecc_session_ttl_seconds")
                    .and_then(|value| value.parse::<u64>().ok())
                    .filter(|value| *value > 0)
            }),
    }
}

fn parse_ecc_rfc_session_config(
    connection: &HashMap<String, String>,
    capabilities: Option<&SapEccRfcBapiCapabilities>,
    request: &SapEccRfcBapiRequestConfig,
) -> SapEccRfcSessionConfig {
    SapEccRfcSessionConfig {
        session_mode: request.session_mode.clone(),
        session_id_path: capabilities
            .and_then(|caps| caps.session_id_path.clone())
            .or_else(|| connection.get("ecc_rfc_session_id_path").cloned()),
        session_id_parameter_name: capabilities
            .and_then(|caps| caps.session_id_parameter_name.clone())
            .or_else(|| connection.get("ecc_rfc_session_id_parameter_name").cloned())
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "session_id".to_string()),
        close_session_path: capabilities
            .and_then(|caps| caps.close_session_path.clone())
            .or_else(|| connection.get("ecc_rfc_close_session_path").cloned()),
        close_session_method: capabilities
            .and_then(|caps| caps.close_session_method.clone())
            .or_else(|| connection.get("ecc_rfc_close_session_method").cloned())
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "POST".to_string()),
        requires_explicit_session_close: capabilities
            .map(|caps| caps.requires_explicit_session_close)
            .unwrap_or_else(|| {
                connection
                    .get("ecc_rfc_requires_explicit_session_close")
                    .map(|value| value == "true")
                    .unwrap_or(false)
            }),
        session_ttl_seconds: capabilities
            .and_then(|caps| caps.session_ttl_seconds)
            .or_else(|| {
                connection
                    .get("ecc_rfc_session_ttl_seconds")
                    .and_then(|value| value.parse::<u64>().ok())
                    .filter(|value| *value > 0)
            }),
    }
}

fn parse_backend_auth_mode_value(value: &str) -> Result<SapEccBackendAuthMode> {
    serde_json::from_str::<SapEccBackendAuthMode>(&format!("\"{value}\""))
        .with_context(|| format!("unsupported SAP ECC backend auth mode '{value}'"))
}

fn parse_session_mode_value(value: &str) -> Result<SapEccSessionMode> {
    serde_json::from_str::<SapEccSessionMode>(&format!("\"{value}\""))
        .with_context(|| format!("unsupported SAP ECC session mode '{value}'"))
}

fn validate_ecc_adapter_runtime_requirements(
    capabilities: Option<&SapEccAdapterCapabilities>,
    request: &SapEccAdapterRequestConfig,
) -> Result<()> {
    let Some(capabilities) = capabilities else {
        return Ok(());
    };
    if let Some(session_mode) = request.session_mode.as_ref() {
        if !capabilities.supported_session_modes.is_empty()
            && !capabilities.supported_session_modes.contains(session_mode)
        {
            return Err(anyhow!(
                "sap_ecc_adapter verification requested unsupported session_mode '{:?}'",
                session_mode
            ));
        }
    }
    if let Some(auth_mode) = request.backend_auth_mode.as_ref() {
        if !capabilities.supported_auth_modes.is_empty()
            && !capabilities.supported_auth_modes.contains(auth_mode)
        {
            return Err(anyhow!(
                "sap_ecc_adapter verification requested unsupported backend_auth_mode '{:?}'",
                auth_mode
            ));
        }
    }
    if let Some(page_size) = request.page_size {
        if let Some(max_page_size) = capabilities.max_page_size {
            if page_size > max_page_size {
                return Err(anyhow!(
                    "sap_ecc_adapter verification requested page_size {} above capability limit {}",
                    page_size,
                    max_page_size
                ));
            }
        }
    }
    Ok(())
}

fn validate_ecc_rfc_runtime_requirements(
    capabilities: Option<&SapEccRfcBapiCapabilities>,
    request: &SapEccRfcBapiRequestConfig,
) -> Result<()> {
    let Some(capabilities) = capabilities else {
        return Ok(());
    };
    if let Some(session_mode) = request.session_mode.as_ref() {
        if !capabilities.supported_session_modes.is_empty()
            && !capabilities.supported_session_modes.contains(session_mode)
        {
            return Err(anyhow!(
                "sap_ecc_rfc_bapi verification requested unsupported session_mode '{:?}'",
                session_mode
            ));
        }
    }
    if let Some(auth_mode) = request.backend_auth_mode.as_ref() {
        if !capabilities.supported_auth_modes.is_empty()
            && !capabilities.supported_auth_modes.contains(auth_mode)
        {
            return Err(anyhow!(
                "sap_ecc_rfc_bapi verification requested unsupported backend_auth_mode '{:?}'",
                auth_mode
            ));
        }
    }
    if let Some(page_size) = request.page_size {
        if let Some(max_page_size) = capabilities.max_page_size {
            if page_size > max_page_size {
                return Err(anyhow!(
                    "sap_ecc_rfc_bapi verification requested page_size {} above capability limit {}",
                    page_size,
                    max_page_size
                ));
            }
        }
    }
    Ok(())
}

fn find_missing_required_adapter_parameters(
    endpoint: &graphica_core::migration_evidence::ConnectorEndpoint,
    request_parameters: &HashMap<String, String>,
    capabilities: Option<&SapEccAdapterCapabilities>,
) -> Result<Vec<String>> {
    let Some(capabilities) = capabilities else {
        return Ok(Vec::new());
    };
    if capabilities.required_parameters.is_empty() {
        return Ok(Vec::new());
    }

    let base_url = format!(
        "{}{}",
        endpoint.base_url.trim_end_matches('/'),
        endpoint.path
    );
    let url = reqwest::Url::parse(&base_url)
        .with_context(|| format!("invalid SAP ECC adapter URL '{}'", base_url))?;
    let query_pairs = url
        .query_pairs()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect::<HashMap<_, _>>();

    let mut missing = capabilities
        .required_parameters
        .iter()
        .filter(|parameter| {
            !query_pairs.contains_key(parameter.as_str())
                && !request_parameters.contains_key(parameter.as_str())
        })
        .cloned()
        .collect::<Vec<_>>();
    missing.sort();
    missing.dedup();
    Ok(missing)
}

fn find_missing_required_rfc_parameters(
    endpoint: &graphica_core::migration_evidence::ConnectorEndpoint,
    request_parameters: &HashMap<String, String>,
    capabilities: Option<&SapEccRfcBapiCapabilities>,
) -> Result<Vec<String>> {
    let Some(capabilities) = capabilities else {
        return Ok(Vec::new());
    };
    if capabilities.required_parameters.is_empty() {
        return Ok(Vec::new());
    }

    let base_url = format!(
        "{}{}",
        endpoint.base_url.trim_end_matches('/'),
        endpoint.path
    );
    let url = reqwest::Url::parse(&base_url)
        .with_context(|| format!("invalid SAP ECC RFC/BAPI bridge URL '{}'", base_url))?;
    let query_pairs = url
        .query_pairs()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect::<HashMap<_, _>>();

    let mut missing = capabilities
        .required_parameters
        .iter()
        .filter(|parameter| {
            !query_pairs.contains_key(parameter.as_str())
                && !request_parameters.contains_key(parameter.as_str())
        })
        .cloned()
        .collect::<Vec<_>>();
    missing.sort();
    missing.dedup();
    Ok(missing)
}

fn count_s4_odata_rows(value: &Value) -> Option<usize> {
    match value {
        Value::Array(items) => Some(items.len()),
        Value::Object(_) => Some(1),
        _ => None,
    }
}

fn apply_s4_odata_verification_metadata(
    metadata: &mut HashMap<String, String>,
    context: &SapS4ODataVerificationOutcome,
) {
    metadata.insert(
        "odata_page_count".to_string(),
        context.page_count.to_string(),
    );
    metadata.insert("odata_paginated".to_string(), context.paginated.to_string());
    metadata.insert(
        "odata_pagination_truncated".to_string(),
        context.pagination_truncated.to_string(),
    );
    if let Some(row_count) = context.row_count {
        metadata.insert("odata_row_count".to_string(), row_count.to_string());
    }
    if let Some(next_link) = context.next_link_remaining.as_deref() {
        metadata.insert(
            "odata_next_link_remaining".to_string(),
            next_link.to_string(),
        );
    }
    if !context.projection.requested_fields.is_empty() {
        metadata.insert(
            "odata_requested_fields_json".to_string(),
            serde_json::to_string(&context.projection.requested_fields)
                .unwrap_or_else(|_| "[]".to_string()),
        );
    }
    if !context.projection.select_fields.is_empty() {
        metadata.insert(
            "odata_select_fields_json".to_string(),
            serde_json::to_string(&context.projection.select_fields)
                .unwrap_or_else(|_| "[]".to_string()),
        );
    }
    if !context.projection.metadata_fields.is_empty() {
        metadata.insert(
            "odata_metadata_fields_json".to_string(),
            serde_json::to_string(&context.projection.metadata_fields)
                .unwrap_or_else(|_| "[]".to_string()),
        );
    }
    if !context.projection.missing_fields.is_empty() {
        metadata.insert(
            "odata_missing_projection_fields_json".to_string(),
            serde_json::to_string(&context.projection.missing_fields)
                .unwrap_or_else(|_| "[]".to_string()),
        );
    }
    metadata.insert(
        "odata_projection_metadata_validated".to_string(),
        context.metadata_capabilities.is_some().to_string(),
    );
    if let Some(capabilities) = context.metadata_capabilities.as_ref() {
        if let Some(entity_set) = capabilities.entity_set.as_deref() {
            metadata.insert("odata_entity_set".to_string(), entity_set.to_string());
        }
        if let Some(entity_type) = capabilities.entity_type.as_deref() {
            metadata.insert("odata_entity_type".to_string(), entity_type.to_string());
        }
        if !capabilities.key_fields.is_empty() {
            metadata.insert(
                "odata_key_fields_json".to_string(),
                serde_json::to_string(&capabilities.key_fields)
                    .unwrap_or_else(|_| "[]".to_string()),
            );
        }
    }
}

fn apply_ecc_adapter_verification_metadata(
    metadata: &mut HashMap<String, String>,
    context: &SapEccAdapterVerificationOutcome,
) {
    metadata.insert("ecc_page_count".to_string(), context.page_count.to_string());
    metadata.insert("ecc_paginated".to_string(), context.paginated.to_string());
    metadata.insert(
        "ecc_pagination_truncated".to_string(),
        context.pagination_truncated.to_string(),
    );
    if let Some(row_count) = context.row_count {
        metadata.insert("ecc_row_count".to_string(), row_count.to_string());
    }
    if let Some(next_path) = context.next_link_remaining.as_deref() {
        metadata.insert("ecc_next_path_remaining".to_string(), next_path.to_string());
    }
    if !context.projection.requested_fields.is_empty() {
        metadata.insert(
            "ecc_requested_fields_json".to_string(),
            serde_json::to_string(&context.projection.requested_fields)
                .unwrap_or_else(|_| "[]".to_string()),
        );
    }
    if !context.projection.declared_fields.is_empty() {
        metadata.insert(
            "ecc_declared_fields_json".to_string(),
            serde_json::to_string(&context.projection.declared_fields)
                .unwrap_or_else(|_| "[]".to_string()),
        );
    }
    if !context.projection.missing_fields.is_empty() {
        metadata.insert(
            "ecc_missing_projection_fields_json".to_string(),
            serde_json::to_string(&context.projection.missing_fields)
                .unwrap_or_else(|_| "[]".to_string()),
        );
    }
    if !context.request_parameters.is_empty() {
        metadata.insert(
            "ecc_request_parameters_json".to_string(),
            serde_json::to_string(&context.request_parameters).unwrap_or_else(|_| "{}".to_string()),
        );
    }
    if !context.missing_required_parameters.is_empty() {
        metadata.insert(
            "ecc_missing_required_parameters_json".to_string(),
            serde_json::to_string(&context.missing_required_parameters)
                .unwrap_or_else(|_| "[]".to_string()),
        );
    }
    if let Some(session_mode) = context.session_mode.as_ref() {
        metadata.insert(
            "ecc_session_mode".to_string(),
            serde_json::to_string(session_mode)
                .unwrap_or_else(|_| "\"unknown\"".to_string())
                .trim_matches('"')
                .to_string(),
        );
    }
    if let Some(backend_auth_mode) = context.backend_auth_mode.as_ref() {
        metadata.insert(
            "ecc_backend_auth_mode".to_string(),
            serde_json::to_string(backend_auth_mode)
                .unwrap_or_else(|_| "\"unknown\"".to_string())
                .trim_matches('"')
                .to_string(),
        );
    }
    if let Some(language) = context.language.as_deref() {
        metadata.insert("ecc_language".to_string(), language.to_string());
    }
    if let Some(page_size) = context.page_size {
        metadata.insert("ecc_page_size".to_string(), page_size.to_string());
    }
    metadata.insert(
        "ecc_session_id_present".to_string(),
        context.session_id_present.to_string(),
    );
    metadata.insert(
        "ecc_session_reused".to_string(),
        context.session_reused.to_string(),
    );
    metadata.insert(
        "ecc_session_closed".to_string(),
        context.session_closed.to_string(),
    );
    if let Some(session_ttl_seconds) = context.session_ttl_seconds {
        metadata.insert(
            "ecc_session_ttl_seconds".to_string(),
            session_ttl_seconds.to_string(),
        );
    }
    metadata.insert(
        "ecc_projection_metadata_validated".to_string(),
        context.metadata_capabilities.is_some().to_string(),
    );
    if let Some(capabilities) = context.metadata_capabilities.as_ref() {
        if let Some(object_name) = capabilities.object_name.as_deref() {
            metadata.insert("ecc_object_name".to_string(), object_name.to_string());
        }
        if !capabilities.key_fields.is_empty() {
            metadata.insert(
                "ecc_key_fields_json".to_string(),
                serde_json::to_string(&capabilities.key_fields)
                    .unwrap_or_else(|_| "[]".to_string()),
            );
        }
        if !capabilities.required_parameters.is_empty() {
            metadata.insert(
                "ecc_required_parameters_json".to_string(),
                serde_json::to_string(&capabilities.required_parameters)
                    .unwrap_or_else(|_| "[]".to_string()),
            );
        }
    }
}

fn apply_ecc_rfc_verification_metadata(
    metadata: &mut HashMap<String, String>,
    context: &SapEccRfcBapiVerificationOutcome,
) {
    metadata.insert(
        "ecc_rfc_page_count".to_string(),
        context.page_count.to_string(),
    );
    metadata.insert(
        "ecc_rfc_paginated".to_string(),
        context.paginated.to_string(),
    );
    metadata.insert(
        "ecc_rfc_pagination_truncated".to_string(),
        context.pagination_truncated.to_string(),
    );
    if let Some(row_count) = context.row_count {
        metadata.insert("ecc_rfc_row_count".to_string(), row_count.to_string());
    }
    if let Some(next_cursor) = context.next_cursor_remaining.as_deref() {
        metadata.insert(
            "ecc_rfc_next_cursor_remaining".to_string(),
            next_cursor.to_string(),
        );
    }
    if !context.projection.requested_fields.is_empty() {
        metadata.insert(
            "ecc_rfc_requested_fields_json".to_string(),
            serde_json::to_string(&context.projection.requested_fields)
                .unwrap_or_else(|_| "[]".to_string()),
        );
    }
    if !context.projection.declared_fields.is_empty() {
        metadata.insert(
            "ecc_rfc_declared_fields_json".to_string(),
            serde_json::to_string(&context.projection.declared_fields)
                .unwrap_or_else(|_| "[]".to_string()),
        );
    }
    if !context.projection.missing_fields.is_empty() {
        metadata.insert(
            "ecc_rfc_missing_projection_fields_json".to_string(),
            serde_json::to_string(&context.projection.missing_fields)
                .unwrap_or_else(|_| "[]".to_string()),
        );
    }
    if !context.request_parameters.is_empty() {
        metadata.insert(
            "ecc_rfc_request_parameters_json".to_string(),
            serde_json::to_string(&context.request_parameters).unwrap_or_else(|_| "{}".to_string()),
        );
    }
    if !context.missing_required_parameters.is_empty() {
        metadata.insert(
            "ecc_rfc_missing_required_parameters_json".to_string(),
            serde_json::to_string(&context.missing_required_parameters)
                .unwrap_or_else(|_| "[]".to_string()),
        );
    }
    metadata.insert(
        "ecc_rfc_projection_metadata_validated".to_string(),
        context.metadata_capabilities.is_some().to_string(),
    );
    if let Some(profile) = context.profile.as_ref() {
        metadata.insert(
            "ecc_rfc_profile".to_string(),
            match profile {
                SapEccRfcBapiProfile::BapiRecordLookup => "bapi_record_lookup",
                SapEccRfcBapiProfile::FunctionModuleExport => "function_module_export",
                SapEccRfcBapiProfile::TableReadRowset => "table_read_rowset",
                SapEccRfcBapiProfile::QueryBridge => "query_bridge",
            }
            .to_string(),
        );
    }
    if let Some(session_mode) = context.session_mode.as_ref() {
        metadata.insert(
            "ecc_rfc_session_mode".to_string(),
            serde_json::to_string(session_mode)
                .unwrap_or_else(|_| "\"unknown\"".to_string())
                .trim_matches('"')
                .to_string(),
        );
    }
    if let Some(backend_auth_mode) = context.backend_auth_mode.as_ref() {
        metadata.insert(
            "ecc_rfc_backend_auth_mode".to_string(),
            serde_json::to_string(backend_auth_mode)
                .unwrap_or_else(|_| "\"unknown\"".to_string())
                .trim_matches('"')
                .to_string(),
        );
    }
    if let Some(language) = context.language.as_deref() {
        metadata.insert("ecc_rfc_language".to_string(), language.to_string());
    }
    if let Some(page_size) = context.page_size {
        metadata.insert("ecc_rfc_page_size".to_string(), page_size.to_string());
    }
    metadata.insert(
        "ecc_rfc_session_id_present".to_string(),
        context.session_id_present.to_string(),
    );
    metadata.insert(
        "ecc_rfc_session_reused".to_string(),
        context.session_reused.to_string(),
    );
    metadata.insert(
        "ecc_rfc_session_closed".to_string(),
        context.session_closed.to_string(),
    );
    if let Some(session_ttl_seconds) = context.session_ttl_seconds {
        metadata.insert(
            "ecc_rfc_session_ttl_seconds".to_string(),
            session_ttl_seconds.to_string(),
        );
    }
    if let Some(capabilities) = context.metadata_capabilities.as_ref() {
        if let Some(function_module) = capabilities.function_module.as_deref() {
            metadata.insert(
                "ecc_rfc_function_module".to_string(),
                function_module.to_string(),
            );
        }
        if let Some(bapi_name) = capabilities.bapi_name.as_deref() {
            metadata.insert("ecc_rfc_bapi_name".to_string(), bapi_name.to_string());
        }
        if let Some(export_structure) = capabilities.export_structure.as_deref() {
            metadata.insert(
                "ecc_rfc_export_structure".to_string(),
                export_structure.to_string(),
            );
        }
        if !capabilities.key_fields.is_empty() {
            metadata.insert(
                "ecc_rfc_key_fields_json".to_string(),
                serde_json::to_string(&capabilities.key_fields)
                    .unwrap_or_else(|_| "[]".to_string()),
            );
        }
        if !capabilities.required_parameters.is_empty() {
            metadata.insert(
                "ecc_rfc_required_parameters_json".to_string(),
                serde_json::to_string(&capabilities.required_parameters)
                    .unwrap_or_else(|_| "[]".to_string()),
            );
        }
        if !capabilities.supported_session_modes.is_empty() {
            metadata.insert(
                "ecc_rfc_supported_session_modes_json".to_string(),
                serde_json::to_string(&capabilities.supported_session_modes)
                    .unwrap_or_else(|_| "[]".to_string()),
            );
        }
        if !capabilities.supported_auth_modes.is_empty() {
            metadata.insert(
                "ecc_rfc_supported_auth_modes_json".to_string(),
                serde_json::to_string(&capabilities.supported_auth_modes)
                    .unwrap_or_else(|_| "[]".to_string()),
            );
        }
    }
}

fn build_s4_projection_validation_summary(context: &SapS4ODataVerificationOutcome) -> String {
    let entity_label = context
        .metadata_capabilities
        .as_ref()
        .and_then(|caps| caps.entity_set.as_deref())
        .unwrap_or("unknown entity set");
    format!(
        "Verification failed because SAP S/4HANA OData metadata for {} does not expose requested field(s): {}",
        entity_label,
        context.projection.missing_fields.join(", ")
    )
}

fn build_s4_pagination_warning_summary(context: &SapS4ODataVerificationOutcome) -> String {
    let row_fragment = context
        .row_count
        .map(|rows| format!(" across {} fetched row(s)", rows))
        .unwrap_or_default();
    format!(
        "Verification matched the fetched SAP S/4HANA OData projection{} but pagination was truncated after {} page(s)",
        row_fragment,
        context.page_count
    )
}

fn build_ecc_projection_validation_summary(context: &SapEccAdapterVerificationOutcome) -> String {
    let object_label = context
        .metadata_capabilities
        .as_ref()
        .and_then(|caps| caps.object_name.as_deref())
        .unwrap_or("unknown ECC object");
    format!(
        "Verification failed because SAP ECC adapter metadata for {} does not expose requested field(s): {}",
        object_label,
        context.projection.missing_fields.join(", ")
    )
}

fn build_ecc_pagination_warning_summary(context: &SapEccAdapterVerificationOutcome) -> String {
    let row_fragment = context
        .row_count
        .map(|rows| format!(" across {} fetched row(s)", rows))
        .unwrap_or_default();
    format!(
        "Verification matched the fetched SAP ECC adapter projection{} but pagination was truncated after {} page(s)",
        row_fragment,
        context.page_count
    )
}

fn build_ecc_rfc_projection_validation_summary(
    context: &SapEccRfcBapiVerificationOutcome,
) -> String {
    let bridge_label = context
        .metadata_capabilities
        .as_ref()
        .and_then(|caps| {
            caps.bapi_name
                .as_deref()
                .or(caps.function_module.as_deref())
        })
        .unwrap_or("unknown ECC RFC/BAPI bridge");
    format!(
        "Verification failed because SAP ECC RFC/BAPI bridge metadata for {} does not expose requested field(s): {}",
        bridge_label,
        context.projection.missing_fields.join(", ")
    )
}

fn build_ecc_rfc_pagination_warning_summary(context: &SapEccRfcBapiVerificationOutcome) -> String {
    let row_fragment = context
        .row_count
        .map(|rows| format!(" across {} fetched row(s)", rows))
        .unwrap_or_default();
    format!(
        "Verification matched the fetched SAP ECC RFC/BAPI bridge projection{} but cursor pagination was truncated after {} page(s)",
        row_fragment,
        context.page_count
    )
}

fn build_hana_source(connection: &HashMap<String, String>) -> Result<DataSource> {
    let host = connection
        .get("host")
        .cloned()
        .ok_or_else(|| anyhow!("sap_hana verification requires host"))?;
    let port = connection
        .get("port")
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(30015);
    let database = connection
        .get("database")
        .cloned()
        .ok_or_else(|| anyhow!("sap_hana verification requires database"))?;
    let schema = connection.get("schema").cloned();
    let metadata = [
        "odbc_driver",
        "odbc_dsn",
        "odbc_connection_string",
        "odbc_options",
    ]
    .into_iter()
    .filter_map(|key| {
        connection
            .get(key)
            .cloned()
            .map(|value| (key.to_string(), value))
    })
    .collect();

    Ok(DataSource {
        id: connection
            .get("datasource_id")
            .cloned()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        title: connection
            .get("title")
            .cloned()
            .unwrap_or_else(|| "SAP HANA verification".to_string()),
        description: Some("ARCXA migration evidence verification datasource".to_string()),
        source_type: "SAPHANA".to_string(),
        connection: ConnectionDetails {
            secret_ref: String::new(),
            config: SourceConfig::SAPHANA(SAPHANAConfig {
                host,
                port,
                database,
                schema,
                instance_number: connection.get("instance_number").cloned(),
            }),
            encryption_enabled: false,
            credentials: HashMap::new(),
        },
        schema_ref: None,
        created_at: Some(Utc::now()),
        updated_at: Some(Utc::now()),
        last_synced_at: None,
        tags: vec!["migration-evidence".to_string(), "verification".to_string()],
        metadata,
    })
}

fn first_query_value(result: QueryResult) -> Result<Value> {
    let row = result
        .rows
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("verification query returned no rows"))?;
    if let Some(object) = row.as_object() {
        if object.len() == 1 {
            if let Some(value) = object.values().next() {
                Ok(value.clone())
            } else {
                Err(anyhow!("verification query returned no columns"))
            }
        } else {
            Ok(Value::Object(object.clone()))
        }
    } else {
        Err(anyhow!("verification query returned a non-object row"))
    }
}

#[derive(Debug, Clone)]
struct VerificationAssessment {
    status: ControlStatus,
    comparison_mode: &'static str,
    comparison_scope: &'static str,
    expected_kind: &'static str,
    actual_kind: &'static str,
    numeric_delta: Option<f64>,
    tolerance_applied: bool,
    verified_field_count: usize,
    mismatch_field_count: usize,
    missing_actual_fields: Vec<String>,
    unexpected_actual_fields: Vec<String>,
    aggregate_keys: Vec<String>,
    mismatch_examples: Vec<String>,
}

fn assess_values(
    expected: Option<&Value>,
    actual: Option<&Value>,
    tolerance: Option<f64>,
) -> VerificationAssessment {
    match (expected, actual) {
        (Some(Value::Object(expected)), Some(Value::Object(actual))) => {
            return assess_object_values(expected, actual, tolerance);
        }
        (Some(Value::Array(expected)), Some(Value::Array(actual))) => {
            return assess_array_values(expected, actual, tolerance);
        }
        _ => {}
    }

    match (
        expected.and_then(coerce_comparable_value),
        actual.and_then(coerce_comparable_value),
    ) {
        (Some(ComparableValue::Number(expected)), Some(ComparableValue::Number(actual))) => {
            let delta = (expected - actual).abs();
            match tolerance {
                Some(limit) if delta <= limit => VerificationAssessment {
                    status: ControlStatus::Passed,
                    comparison_mode: "numeric_tolerance",
                    comparison_scope: "scalar",
                    expected_kind: "number",
                    actual_kind: "number",
                    numeric_delta: Some(delta),
                    tolerance_applied: true,
                    verified_field_count: 0,
                    mismatch_field_count: 0,
                    missing_actual_fields: vec![],
                    unexpected_actual_fields: vec![],
                    aggregate_keys: vec![],
                    mismatch_examples: vec![],
                },
                Some(_) => VerificationAssessment {
                    status: ControlStatus::Failed,
                    comparison_mode: "numeric_tolerance",
                    comparison_scope: "scalar",
                    expected_kind: "number",
                    actual_kind: "number",
                    numeric_delta: Some(delta),
                    tolerance_applied: true,
                    verified_field_count: 0,
                    mismatch_field_count: 1,
                    missing_actual_fields: vec![],
                    unexpected_actual_fields: vec![],
                    aggregate_keys: vec![],
                    mismatch_examples: vec![format!("delta={delta}")],
                },
                None if delta == 0.0 => VerificationAssessment {
                    status: ControlStatus::Passed,
                    comparison_mode: "numeric_exact",
                    comparison_scope: "scalar",
                    expected_kind: "number",
                    actual_kind: "number",
                    numeric_delta: Some(delta),
                    tolerance_applied: false,
                    verified_field_count: 0,
                    mismatch_field_count: 0,
                    missing_actual_fields: vec![],
                    unexpected_actual_fields: vec![],
                    aggregate_keys: vec![],
                    mismatch_examples: vec![],
                },
                None => VerificationAssessment {
                    status: ControlStatus::Warning,
                    comparison_mode: "numeric_exact",
                    comparison_scope: "scalar",
                    expected_kind: "number",
                    actual_kind: "number",
                    numeric_delta: Some(delta),
                    tolerance_applied: false,
                    verified_field_count: 0,
                    mismatch_field_count: 1,
                    missing_actual_fields: vec![],
                    unexpected_actual_fields: vec![],
                    aggregate_keys: vec![],
                    mismatch_examples: vec![format!("delta={delta}")],
                },
            }
        }
        (Some(ComparableValue::Boolean(expected)), Some(ComparableValue::Boolean(actual))) => {
            VerificationAssessment {
                status: if expected == actual {
                    ControlStatus::Passed
                } else if tolerance.is_some() {
                    ControlStatus::Failed
                } else {
                    ControlStatus::Warning
                },
                comparison_mode: "boolean_exact",
                comparison_scope: "scalar",
                expected_kind: "boolean",
                actual_kind: "boolean",
                numeric_delta: None,
                tolerance_applied: false,
                verified_field_count: 0,
                mismatch_field_count: usize::from(expected != actual),
                missing_actual_fields: vec![],
                unexpected_actual_fields: vec![],
                aggregate_keys: vec![],
                mismatch_examples: if expected != actual {
                    vec![format!("expected={expected} actual={actual}")]
                } else {
                    vec![]
                },
            }
        }
        (Some(ComparableValue::Timestamp(expected)), Some(ComparableValue::Timestamp(actual))) => {
            VerificationAssessment {
                status: if expected == actual {
                    ControlStatus::Passed
                } else if tolerance.is_some() {
                    ControlStatus::Failed
                } else {
                    ControlStatus::Warning
                },
                comparison_mode: "timestamp_exact",
                comparison_scope: "scalar",
                expected_kind: "timestamp",
                actual_kind: "timestamp",
                numeric_delta: None,
                tolerance_applied: false,
                verified_field_count: 0,
                mismatch_field_count: usize::from(expected != actual),
                missing_actual_fields: vec![],
                unexpected_actual_fields: vec![],
                aggregate_keys: vec![],
                mismatch_examples: if expected != actual {
                    vec!["timestamp_mismatch".to_string()]
                } else {
                    vec![]
                },
            }
        }
        (Some(ComparableValue::Date(expected)), Some(ComparableValue::Date(actual))) => {
            VerificationAssessment {
                status: if expected == actual {
                    ControlStatus::Passed
                } else if tolerance.is_some() {
                    ControlStatus::Failed
                } else {
                    ControlStatus::Warning
                },
                comparison_mode: "date_exact",
                comparison_scope: "scalar",
                expected_kind: "date",
                actual_kind: "date",
                numeric_delta: None,
                tolerance_applied: false,
                verified_field_count: 0,
                mismatch_field_count: usize::from(expected != actual),
                missing_actual_fields: vec![],
                unexpected_actual_fields: vec![],
                aggregate_keys: vec![],
                mismatch_examples: if expected != actual {
                    vec!["date_mismatch".to_string()]
                } else {
                    vec![]
                },
            }
        }
        (Some(ComparableValue::Time(expected)), Some(ComparableValue::Time(actual))) => {
            VerificationAssessment {
                status: if expected == actual {
                    ControlStatus::Passed
                } else if tolerance.is_some() {
                    ControlStatus::Failed
                } else {
                    ControlStatus::Warning
                },
                comparison_mode: "time_exact",
                comparison_scope: "scalar",
                expected_kind: "time",
                actual_kind: "time",
                numeric_delta: None,
                tolerance_applied: false,
                verified_field_count: 0,
                mismatch_field_count: usize::from(expected != actual),
                missing_actual_fields: vec![],
                unexpected_actual_fields: vec![],
                aggregate_keys: vec![],
                mismatch_examples: if expected != actual {
                    vec!["time_mismatch".to_string()]
                } else {
                    vec![]
                },
            }
        }
        (Some(ComparableValue::String(expected)), Some(ComparableValue::String(actual))) => {
            VerificationAssessment {
                status: if expected == actual {
                    ControlStatus::Passed
                } else if tolerance.is_some() {
                    ControlStatus::Failed
                } else {
                    ControlStatus::Warning
                },
                comparison_mode: "string_exact",
                comparison_scope: "scalar",
                expected_kind: "string",
                actual_kind: "string",
                numeric_delta: None,
                tolerance_applied: false,
                verified_field_count: 0,
                mismatch_field_count: usize::from(expected != actual),
                missing_actual_fields: vec![],
                unexpected_actual_fields: vec![],
                aggregate_keys: vec![],
                mismatch_examples: if expected != actual {
                    vec!["string_mismatch".to_string()]
                } else {
                    vec![]
                },
            }
        }
        (Some(expected), Some(actual)) => VerificationAssessment {
            status: if tolerance.is_some() {
                ControlStatus::Failed
            } else {
                ControlStatus::Warning
            },
            comparison_mode: "typed_mismatch",
            comparison_scope: "scalar",
            expected_kind: expected.kind_name(),
            actual_kind: actual.kind_name(),
            numeric_delta: None,
            tolerance_applied: false,
            verified_field_count: 0,
            mismatch_field_count: 1,
            missing_actual_fields: vec![],
            unexpected_actual_fields: vec![],
            aggregate_keys: vec![],
            mismatch_examples: vec![format!(
                "expected_type={} actual_type={}",
                expected.kind_name(),
                actual.kind_name()
            )],
        },
        (_, Some(actual)) => VerificationAssessment {
            status: ControlStatus::Passed,
            comparison_mode: "presence_only",
            comparison_scope: "scalar",
            expected_kind: "missing",
            actual_kind: actual.kind_name(),
            numeric_delta: None,
            tolerance_applied: false,
            verified_field_count: 0,
            mismatch_field_count: 0,
            missing_actual_fields: vec![],
            unexpected_actual_fields: vec![],
            aggregate_keys: vec![],
            mismatch_examples: vec![],
        },
        _ => VerificationAssessment {
            status: ControlStatus::NotRun,
            comparison_mode: "not_run",
            comparison_scope: "scalar",
            expected_kind: expected
                .and_then(coerce_comparable_value)
                .map(|value| value.kind_name())
                .unwrap_or("missing"),
            actual_kind: "missing",
            numeric_delta: None,
            tolerance_applied: false,
            verified_field_count: 0,
            mismatch_field_count: 0,
            missing_actual_fields: vec![],
            unexpected_actual_fields: vec![],
            aggregate_keys: vec![],
            mismatch_examples: vec![],
        },
    }
}

fn assess_object_values(
    expected: &Map<String, Value>,
    actual: &Map<String, Value>,
    tolerance: Option<f64>,
) -> VerificationAssessment {
    let mut status = ControlStatus::Passed;
    let mut verified_field_count = 0usize;
    let mut mismatch_field_count = 0usize;
    let mut missing_actual_fields = Vec::new();
    let mut unexpected_actual_fields = actual
        .keys()
        .filter(|key| !expected.contains_key(*key))
        .cloned()
        .collect::<Vec<_>>();
    unexpected_actual_fields.sort();
    let mut mismatch_examples = Vec::new();

    for (field, expected_value) in expected {
        match actual.get(field) {
            Some(actual_value) => {
                verified_field_count += 1;
                let field_assessment =
                    assess_values(Some(expected_value), Some(actual_value), tolerance);
                if field_assessment.status != ControlStatus::Passed {
                    mismatch_field_count += 1;
                    if mismatch_examples.len() < 5 {
                        mismatch_examples
                            .push(format!("{}:{}", field, field_assessment.comparison_mode));
                    }
                }
                status = merge_status(status, field_assessment.status);
            }
            None => {
                mismatch_field_count += 1;
                missing_actual_fields.push(field.clone());
                if mismatch_examples.len() < 5 {
                    mismatch_examples.push(format!("{field}:missing"));
                }
                status = ControlStatus::Failed;
            }
        }
    }

    missing_actual_fields.sort();

    let scope = if expected.keys().any(|key| looks_like_aggregate_key(key)) {
        "aggregate_projection"
    } else {
        "record_projection"
    };

    let aggregate_keys = if scope == "aggregate_projection" {
        expected
            .keys()
            .filter(|key| looks_like_aggregate_key(key))
            .cloned()
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    VerificationAssessment {
        status,
        comparison_mode: "object_fieldwise",
        comparison_scope: scope,
        expected_kind: "object",
        actual_kind: "object",
        numeric_delta: None,
        tolerance_applied: tolerance.is_some(),
        verified_field_count,
        mismatch_field_count,
        missing_actual_fields,
        unexpected_actual_fields,
        aggregate_keys,
        mismatch_examples,
    }
}

fn assess_array_values(
    expected: &[Value],
    actual: &[Value],
    tolerance: Option<f64>,
) -> VerificationAssessment {
    let mut status = ControlStatus::Passed;
    let mut mismatch_field_count = 0usize;
    let mut mismatch_examples = Vec::new();
    let verified_field_count = expected.len().min(actual.len());

    if expected.len() != actual.len() {
        mismatch_field_count += expected.len().max(actual.len()) - verified_field_count;
        status = if tolerance.is_some() {
            ControlStatus::Failed
        } else {
            ControlStatus::Warning
        };
        mismatch_examples.push(format!("length:{}!={}", expected.len(), actual.len()));
    }

    for (idx, (expected_value, actual_value)) in expected.iter().zip(actual.iter()).enumerate() {
        let element_assessment = assess_values(Some(expected_value), Some(actual_value), tolerance);
        if element_assessment.status != ControlStatus::Passed {
            mismatch_field_count += 1;
            if mismatch_examples.len() < 5 {
                mismatch_examples.push(format!("[{idx}]:{}", element_assessment.comparison_mode));
            }
        }
        status = merge_status(status, element_assessment.status);
    }

    VerificationAssessment {
        status,
        comparison_mode: "array_positionwise",
        comparison_scope: "rowset",
        expected_kind: "array",
        actual_kind: "array",
        numeric_delta: None,
        tolerance_applied: tolerance.is_some(),
        verified_field_count,
        mismatch_field_count,
        missing_actual_fields: vec![],
        unexpected_actual_fields: vec![],
        aggregate_keys: vec![],
        mismatch_examples,
    }
}

fn merge_status(current: ControlStatus, next: ControlStatus) -> ControlStatus {
    use ControlStatus::*;
    match (current, next) {
        (Failed, _) | (_, Failed) => Failed,
        (Warning, _) | (_, Warning) => Warning,
        (NotRun, _) | (_, NotRun) => NotRun,
        _ => Passed,
    }
}

fn looks_like_aggregate_key(key: &str) -> bool {
    let lowered = key.to_ascii_lowercase();
    lowered.contains("count")
        || lowered.contains("sum")
        || lowered.contains("avg")
        || lowered.contains("min")
        || lowered.contains("max")
        || lowered.contains("total")
}

fn build_summary(
    assessment: &VerificationAssessment,
    expected: Option<&Value>,
    actual: &Value,
    tolerance: Option<f64>,
) -> String {
    if assessment.comparison_scope == "record_projection"
        || assessment.comparison_scope == "aggregate_projection"
    {
        let shape = if assessment.comparison_scope == "aggregate_projection" {
            "aggregate projection"
        } else {
            "record projection"
        };
        let mismatch_fragment = if assessment.mismatch_field_count > 0 {
            format!(
                " with {} mismatched field(s): {}",
                assessment.mismatch_field_count,
                assessment.mismatch_examples.join(", ")
            )
        } else {
            String::new()
        };
        return match assessment.status {
            ControlStatus::Passed => format!(
                "Verification passed via {} comparison across {} field(s)",
                shape, assessment.verified_field_count
            ),
            ControlStatus::Warning => format!(
                "Verification produced a {} delta across {} field(s){}",
                shape, assessment.verified_field_count, mismatch_fragment
            ),
            ControlStatus::Failed => format!(
                "Verification failed for {} across {} field(s){}",
                shape, assessment.verified_field_count, mismatch_fragment
            ),
            ControlStatus::NotRun => "Verification did not run".to_string(),
        };
    }

    match assessment.status {
        ControlStatus::Passed => format!(
            "Verification passed via {} comparison: actual value {} matches expectation {:?}",
            assessment.comparison_mode, actual, expected
        ),
        ControlStatus::Warning => format!(
            "Verification produced a {} delta: actual value {} differs from expectation {:?} with tolerance {:?}",
            assessment.comparison_mode, actual, expected, tolerance
        ),
        ControlStatus::Failed => format!(
            "Verification failed via {} comparison: actual value {} differs from expectation {:?} beyond tolerance {:?}",
            assessment.comparison_mode, actual, expected, tolerance
        ),
        ControlStatus::NotRun => "Verification did not run".to_string(),
    }
}

fn merge_control_metadata(
    metadata: &HashMap<String, String>,
    assessment: &VerificationAssessment,
) -> HashMap<String, String> {
    let mut merged = metadata.clone();
    merged.insert(
        "comparison_mode".to_string(),
        assessment.comparison_mode.to_string(),
    );
    merged.insert(
        "comparison_scope".to_string(),
        assessment.comparison_scope.to_string(),
    );
    merged.insert(
        "expected_value_type".to_string(),
        assessment.expected_kind.to_string(),
    );
    merged.insert(
        "actual_value_type".to_string(),
        assessment.actual_kind.to_string(),
    );
    merged.insert(
        "tolerance_applied".to_string(),
        assessment.tolerance_applied.to_string(),
    );
    merged.insert(
        "verified_field_count".to_string(),
        assessment.verified_field_count.to_string(),
    );
    merged.insert(
        "mismatch_field_count".to_string(),
        assessment.mismatch_field_count.to_string(),
    );
    if let Some(delta) = assessment.numeric_delta {
        merged.insert("numeric_delta".to_string(), delta.to_string());
    }
    if !assessment.missing_actual_fields.is_empty() {
        merged.insert(
            "missing_actual_fields".to_string(),
            serde_json::to_string(&assessment.missing_actual_fields)
                .unwrap_or_else(|_| "[]".to_string()),
        );
    }
    if !assessment.unexpected_actual_fields.is_empty() {
        merged.insert(
            "unexpected_actual_fields".to_string(),
            serde_json::to_string(&assessment.unexpected_actual_fields)
                .unwrap_or_else(|_| "[]".to_string()),
        );
    }
    if !assessment.aggregate_keys.is_empty() {
        merged.insert(
            "aggregate_keys".to_string(),
            serde_json::to_string(&assessment.aggregate_keys).unwrap_or_else(|_| "[]".to_string()),
        );
    }
    if !assessment.mismatch_examples.is_empty() {
        merged.insert(
            "mismatch_examples".to_string(),
            serde_json::to_string(&assessment.mismatch_examples)
                .unwrap_or_else(|_| "[]".to_string()),
        );
    }
    merged
}

#[derive(Debug, Clone, PartialEq)]
enum ComparableValue {
    Number(f64),
    Boolean(bool),
    Timestamp(DateTime<FixedOffset>),
    Date(NaiveDate),
    Time(NaiveTime),
    String(String),
}

impl ComparableValue {
    fn kind_name(&self) -> &'static str {
        match self {
            ComparableValue::Number(_) => "number",
            ComparableValue::Boolean(_) => "boolean",
            ComparableValue::Timestamp(_) => "timestamp",
            ComparableValue::Date(_) => "date",
            ComparableValue::Time(_) => "time",
            ComparableValue::String(_) => "string",
        }
    }
}

fn coerce_comparable_value(value: &Value) -> Option<ComparableValue> {
    match value {
        Value::Null => None,
        Value::Bool(flag) => Some(ComparableValue::Boolean(*flag)),
        Value::Number(number) => number.as_f64().map(ComparableValue::Number),
        Value::String(text) => coerce_string_value(text),
        _ => Some(ComparableValue::String(value.to_string())),
    }
}

fn coerce_string_value(text: &str) -> Option<ComparableValue> {
    if let Some(flag) = parse_bool(text) {
        return Some(ComparableValue::Boolean(flag));
    }

    if let Ok(timestamp) = DateTime::parse_from_rfc3339(text) {
        return Some(ComparableValue::Timestamp(timestamp));
    }

    if let Ok(timestamp) = NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S%.f") {
        return FixedOffset::east_opt(0)
            .and_then(|offset| offset.from_local_datetime(&timestamp).single())
            .map(ComparableValue::Timestamp);
    }

    if let Ok(date) = NaiveDate::parse_from_str(text, "%Y-%m-%d") {
        return Some(ComparableValue::Date(date));
    }

    if let Ok(time) = NaiveTime::parse_from_str(text, "%H:%M:%S%.f") {
        return Some(ComparableValue::Time(time));
    }

    if let Ok(number) = text.parse::<f64>() {
        return Some(ComparableValue::Number(number));
    }

    Some(ComparableValue::String(text.to_string()))
}

fn parse_bool(text: &str) -> Option<bool> {
    match text.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "t" | "yes" | "y" => Some(true),
        "0" | "false" | "f" | "no" | "n" => Some(false),
        _ => None,
    }
}

#[derive(Clone)]
pub struct VerificationServiceImpl {
    manager: VerificationManager,
    started_at: Instant,
}

impl VerificationServiceImpl {
    pub fn new(manager: VerificationManager) -> Self {
        Self {
            manager,
            started_at: Instant::now(),
        }
    }
}

#[tonic::async_trait]
impl VerificationService for VerificationServiceImpl {
    async fn run_verification(
        &self,
        request: Request<RunVerificationRequest>,
    ) -> Result<Response<RunVerificationResponse>, Status> {
        let verification_request: VerificationRequest =
            deserialize(&request.into_inner().verification_request_json)
                .map_err(internal_status)?;
        let result = self
            .manager
            .run_verification(verification_request)
            .await
            .map_err(internal_status)?;
        Ok(Response::new(RunVerificationResponse {
            verification_result_json: serialize(&result).map_err(internal_status)?,
        }))
    }

    async fn run_verification_and_emit(
        &self,
        request: Request<RunVerificationAndEmitRequest>,
    ) -> Result<Response<RunVerificationAndEmitResponse>, Status> {
        let dispatch_request: VerificationDispatchRequest =
            deserialize(&request.into_inner().verification_dispatch_request_json)
                .map_err(internal_status)?;
        let result = self
            .manager
            .run_verification_and_emit(dispatch_request)
            .await
            .map_err(internal_status)?;
        Ok(Response::new(RunVerificationAndEmitResponse {
            verification_dispatch_result_json: serialize(&result).map_err(internal_status)?,
        }))
    }

    async fn health(
        &self,
        _request: Request<HealthRequest>,
    ) -> Result<Response<HealthResponse>, Status> {
        Ok(Response::new(HealthResponse {
            healthy: true,
            service_name: "arcxa-verification".to_string(),
            uptime_seconds: self.started_at.elapsed().as_secs() as i64,
        }))
    }
}

fn serialize<T: serde::Serialize>(value: &T) -> Result<String> {
    serde_json::to_string(value).context("failed to serialize verification response")
}

fn deserialize<T: DeserializeOwned>(value: &str) -> Result<T> {
    serde_json::from_str(value).context("failed to deserialize verification request")
}

fn internal_status(error: impl std::fmt::Display) -> Status {
    Status::internal(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use graphica_core::migration_evidence::{
        ConnectorAuth, ConnectorAuthKind, ConnectorEndpoint, MigrationEvidenceDeliveryMode,
        MigrationEvidenceDispatchSummary, MigrationEvidenceEvent, MigrationEvidenceEventForwarder,
        SourceFieldRef, TargetFieldRef, VerificationDispatchRequest, VerificationSource,
    };
    use graphica_core::secrets::providers::{InlineSecretStore, SecretStoreRegistry};
    use graphica_core::secrets::{put_secret_by_ref, SecretValue};
    use serde_json::json;
    use std::sync::{Arc, Mutex};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[derive(Clone, Default)]
    struct CapturingForwarder {
        captured: Arc<Mutex<Vec<MigrationEvidenceEvent>>>,
    }

    #[async_trait]
    impl MigrationEvidenceEventForwarder for CapturingForwarder {
        async fn ingest_events(
            &self,
            events: Vec<MigrationEvidenceEvent>,
        ) -> Result<MigrationEvidenceDispatchSummary> {
            self.captured.lock().unwrap().extend(events.clone());
            Ok(MigrationEvidenceDispatchSummary::from_events(
                &events,
                MigrationEvidenceDeliveryMode::Direct,
                true,
            ))
        }
    }

    async fn spawn_s4_odata_server(routes: Vec<(&'static str, &'static str)>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            for _ in 0..routes.len() {
                if let Ok((mut socket, _)) = listener.accept().await {
                    let mut buffer = vec![0u8; 4096];
                    let bytes = socket.read(&mut buffer).await.unwrap_or(0);
                    let request = String::from_utf8_lossy(&buffer[..bytes]);
                    let path = request
                        .lines()
                        .next()
                        .and_then(|line| line.split_whitespace().nth(1))
                        .unwrap_or("/");
                    let body = routes
                        .iter()
                        .find_map(|(route, body)| {
                            if path.starts_with(route) {
                                Some(*body)
                            } else {
                                None
                            }
                        })
                        .unwrap_or("{\"error\":\"not found\"}");
                    let status = if body.contains("\"error\":\"not found\"") {
                        "404 Not Found"
                    } else {
                        "200 OK"
                    };
                    let response = format!(
                        "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = socket.write_all(response.as_bytes()).await;
                }
            }
        });
        format!("http://{}", addr)
    }

    async fn spawn_ecc_adapter_server(routes: Vec<(&'static str, &'static str)>) -> String {
        spawn_s4_odata_server(routes).await
    }

    async fn spawn_ecc_rfc_server(routes: Vec<(&'static str, &'static str)>) -> String {
        spawn_s4_odata_server(routes).await
    }

    #[tokio::test]
    async fn http_verification_returns_warning_when_delta_has_no_tolerance() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            if let Ok((mut socket, _)) = listener.accept().await {
                let mut buffer = vec![0u8; 1024];
                let _ = socket.read(&mut buffer).await;
                let response = b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 19\r\n\r\n{\"actual_value\":11}";
                let _ = socket.write_all(response).await;
            }
        });

        let manager = VerificationManager::new(Arc::new(CapturingForwarder::default()));
        let result = manager
            .run_verification(VerificationRequest {
                control_name: "amount-match".to_string(),
                program_id: "program-1".to_string(),
                object_id: "object-1".to_string(),
                source_field: SourceFieldRef {
                    system: "ECC".to_string(),
                    object_name: "VBAK".to_string(),
                    field_name: "NETWR".to_string(),
                    field_path: "$.amount".to_string(),
                    semantic_type: None,
                    record_id: Some("SO-1".to_string()),
                },
                target_field: TargetFieldRef {
                    system: "S4".to_string(),
                    object_name: "A_SalesOrder".to_string(),
                    field_name: "NetAmount".to_string(),
                    field_path: "$.amount".to_string(),
                    semantic_type: None,
                    record_id: Some("SO-1".to_string()),
                },
                expected_value: Some(json!(10)),
                tolerance: None,
                metadata: HashMap::new(),
                source: VerificationSource {
                    transport: ConnectorTransport::HttpJson,
                    query: None,
                    endpoint: Some(ConnectorEndpoint {
                        base_url: format!("http://{}", addr),
                        path: "/verify".to_string(),
                        method: "GET".to_string(),
                        headers: HashMap::new(),
                    }),
                    auth: ConnectorAuth::default(),
                    connection: HashMap::new(),
                },
            })
            .await
            .unwrap();

        assert_eq!(result.control_result.status, ControlStatus::Warning);
        assert!(result.exception_record.is_some());
    }

    #[tokio::test]
    async fn verification_and_emit_returns_canonical_events_and_dispatch_summary() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            if let Ok((mut socket, _)) = listener.accept().await {
                let mut buffer = vec![0u8; 1024];
                let _ = socket.read(&mut buffer).await;
                let response = b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 19\r\n\r\n{\"actual_value\":10}";
                let _ = socket.write_all(response).await;
            }
        });

        let forwarder = CapturingForwarder::default();
        let manager = VerificationManager::new(Arc::new(forwarder.clone()));

        let dispatch = manager
            .run_verification_and_emit(VerificationDispatchRequest {
                connector_id: "verification-connector".to_string(),
                run_id: "run-1".to_string(),
                vendor: graphica_core::migration_evidence::MigrationConnectorVendor::SapHana,
                verification: VerificationRequest {
                    control_name: "amount-match".to_string(),
                    program_id: "program-1".to_string(),
                    object_id: "object-1".to_string(),
                    source_field: SourceFieldRef {
                        system: "ECC".to_string(),
                        object_name: "VBAK".to_string(),
                        field_name: "NETWR".to_string(),
                        field_path: "$.amount".to_string(),
                        semantic_type: None,
                        record_id: Some("SO-1".to_string()),
                    },
                    target_field: TargetFieldRef {
                        system: "S4".to_string(),
                        object_name: "A_SalesOrder".to_string(),
                        field_name: "NetAmount".to_string(),
                        field_path: "$.amount".to_string(),
                        semantic_type: None,
                        record_id: Some("SO-1".to_string()),
                    },
                    expected_value: Some(json!(10)),
                    tolerance: Some(0.0),
                    metadata: HashMap::from([(
                        "value_key".to_string(),
                        "SO-1::$.amount".to_string(),
                    )]),
                    source: VerificationSource {
                        transport: ConnectorTransport::HttpJson,
                        query: None,
                        endpoint: Some(ConnectorEndpoint {
                            base_url: format!("http://{}", addr),
                            path: "/verify".to_string(),
                            method: "GET".to_string(),
                            headers: HashMap::new(),
                        }),
                        auth: ConnectorAuth::default(),
                        connection: HashMap::new(),
                    },
                },
            })
            .await
            .unwrap();

        assert_eq!(dispatch.emitted_events.len(), 2);
        assert_eq!(dispatch.dispatch_summary.accepted_event_count, 2);
        assert_eq!(
            dispatch.dispatch_summary.delivery_mode,
            MigrationEvidenceDeliveryMode::Direct
        );
        assert!(dispatch.dispatch_summary.traceability_acknowledged);
        assert_eq!(forwarder.captured.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn s4_odata_verification_extracts_value_from_odata_v4_collection() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            if let Ok((mut socket, _)) = listener.accept().await {
                let mut buffer = vec![0u8; 2048];
                let _ = socket.read(&mut buffer).await;
                let request = String::from_utf8_lossy(&buffer);
                assert!(
                    request.contains("Accept: application/json")
                        || request.contains("accept: application/json")
                );
                let response = b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 67\r\n\r\n{\"value\":[{\"SalesOrder\":\"500000001\",\"NetAmount\":\"100.00\"}]}";
                let _ = socket.write_all(response).await;
            }
        });

        let manager = VerificationManager::new(Arc::new(CapturingForwarder::default()));
        let result = manager
            .run_verification(VerificationRequest {
                control_name: "net-amount-match".to_string(),
                program_id: "program-1".to_string(),
                object_id: "object-1".to_string(),
                source_field: SourceFieldRef {
                    system: "SAP ECC".to_string(),
                    object_name: "VBAK".to_string(),
                    field_name: "NETWR".to_string(),
                    field_path: "$.NETWR".to_string(),
                    semantic_type: Some("currency_amount".to_string()),
                    record_id: Some("SO-1".to_string()),
                },
                target_field: TargetFieldRef {
                    system: "SAP S/4HANA".to_string(),
                    object_name: "A_SalesOrder".to_string(),
                    field_name: "NetAmount".to_string(),
                    field_path: "$.NetAmount".to_string(),
                    semantic_type: Some("currency_amount".to_string()),
                    record_id: Some("SO-1".to_string()),
                },
                expected_value: Some(json!(100.0)),
                tolerance: Some(0.0),
                metadata: HashMap::new(),
                source: VerificationSource {
                    transport: ConnectorTransport::SapS4OData,
                    query: None,
                    endpoint: Some(ConnectorEndpoint {
                        base_url: format!("http://{}", addr),
                        path: "/sap/opu/odata4/API_SALES_ORDER/A_SalesOrder?$top=1".to_string(),
                        method: "GET".to_string(),
                        headers: HashMap::new(),
                    }),
                    auth: ConnectorAuth::default(),
                    connection: HashMap::from([("odata_client".to_string(), "100".to_string())]),
                },
            })
            .await
            .unwrap();

        assert_eq!(result.control_result.status, ControlStatus::Passed);
        assert_eq!(result.control_result.actual_value, Some(json!("100.00")));
        assert_eq!(
            result.control_result.metadata.get("comparison_mode"),
            Some(&"numeric_tolerance".to_string())
        );
    }

    #[tokio::test]
    async fn s4_odata_verification_supports_record_projection_comparisons() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            if let Ok((mut socket, _)) = listener.accept().await {
                let mut buffer = vec![0u8; 2048];
                let _ = socket.read(&mut buffer).await;
                let response = b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 94\r\n\r\n{\"d\":{\"results\":[{\"SalesOrder\":\"500000001\",\"NetAmount\":\"100.00\",\"TransactionCurrency\":\"USD\"}]}}";
                let _ = socket.write_all(response).await;
            }
        });

        let manager = VerificationManager::new(Arc::new(CapturingForwarder::default()));
        let result = manager
            .run_verification(VerificationRequest {
                control_name: "sales-order-projection-match".to_string(),
                program_id: "program-1".to_string(),
                object_id: "object-1".to_string(),
                source_field: SourceFieldRef {
                    system: "SAP ECC".to_string(),
                    object_name: "VBAK".to_string(),
                    field_name: "DOCUMENT".to_string(),
                    field_path: "$.projection".to_string(),
                    semantic_type: None,
                    record_id: Some("SO-1".to_string()),
                },
                target_field: TargetFieldRef {
                    system: "SAP S/4HANA".to_string(),
                    object_name: "A_SalesOrder".to_string(),
                    field_name: "projection".to_string(),
                    field_path: "$.projection".to_string(),
                    semantic_type: None,
                    record_id: Some("SO-1".to_string()),
                },
                expected_value: Some(json!({
                    "SalesOrder": "500000001",
                    "NetAmount": 100.0,
                    "TransactionCurrency": "USD"
                })),
                tolerance: Some(0.0),
                metadata: HashMap::new(),
                source: VerificationSource {
                    transport: ConnectorTransport::SapS4OData,
                    query: None,
                    endpoint: Some(ConnectorEndpoint {
                        base_url: format!("http://{}", addr),
                        path: "/sap/opu/odata/sap/API_SALES_ORDER_SRV/A_SalesOrder".to_string(),
                        method: "GET".to_string(),
                        headers: HashMap::new(),
                    }),
                    auth: ConnectorAuth::default(),
                    connection: HashMap::new(),
                },
            })
            .await
            .unwrap();

        assert_eq!(result.control_result.status, ControlStatus::Passed);
        assert_eq!(
            result.control_result.metadata.get("comparison_scope"),
            Some(&"record_projection".to_string())
        );
        assert_eq!(
            result.control_result.metadata.get("verified_field_count"),
            Some(&"3".to_string())
        );
    }

    #[tokio::test]
    async fn s4_odata_verification_fails_when_metadata_omits_requested_projection_field() {
        let endpoint = spawn_s4_odata_server(vec![(
            "/sap/opu/odata4/API_SALES_ORDER/A_SalesOrder",
            r#"{"value":[{"SalesOrder":"500000001","NetAmount":"100.00","TransactionCurrency":"USD","UnknownField":"shadow"}]}"#,
        )])
        .await;

        let manager = VerificationManager::new(Arc::new(CapturingForwarder::default()));
        let result = manager
            .run_verification(VerificationRequest {
                control_name: "sales-order-projection-match".to_string(),
                program_id: "program-1".to_string(),
                object_id: "object-1".to_string(),
                source_field: SourceFieldRef {
                    system: "SAP ECC".to_string(),
                    object_name: "VBAK".to_string(),
                    field_name: "DOCUMENT".to_string(),
                    field_path: "$.projection".to_string(),
                    semantic_type: None,
                    record_id: Some("SO-1".to_string()),
                },
                target_field: TargetFieldRef {
                    system: "SAP S/4HANA".to_string(),
                    object_name: "A_SalesOrder".to_string(),
                    field_name: "projection".to_string(),
                    field_path: "$.projection".to_string(),
                    semantic_type: None,
                    record_id: Some("SO-1".to_string()),
                },
                expected_value: Some(json!({
                    "SalesOrder": "500000001",
                    "NetAmount": 100.0,
                    "UnknownField": "shadow"
                })),
                tolerance: Some(0.0),
                metadata: HashMap::new(),
                source: VerificationSource {
                    transport: ConnectorTransport::SapS4OData,
                    query: None,
                    endpoint: Some(ConnectorEndpoint {
                        base_url: endpoint,
                        path: "/sap/opu/odata4/API_SALES_ORDER/A_SalesOrder?$select=SalesOrder,NetAmount,UnknownField".to_string(),
                        method: "GET".to_string(),
                        headers: HashMap::new(),
                    }),
                    auth: ConnectorAuth::default(),
                    connection: HashMap::from([
                        ("odata_metadata_version".to_string(), "v4".to_string()),
                        ("odata_entity_set".to_string(), "A_SalesOrder".to_string()),
                        (
                            "odata_entity_type".to_string(),
                            "API_SALES_ORDER.A_SalesOrderType".to_string(),
                        ),
                        (
                            "odata_key_fields_json".to_string(),
                            r#"["SalesOrder"]"#.to_string(),
                        ),
                        (
                            "odata_property_types_json".to_string(),
                            r#"{"SalesOrder":"Edm.String","NetAmount":"Edm.Decimal","TransactionCurrency":"Edm.String"}"#.to_string(),
                        ),
                    ]),
                },
            })
            .await
            .unwrap();

        assert_eq!(result.control_result.status, ControlStatus::Failed);
        assert!(result
            .control_result
            .summary
            .contains("does not expose requested field(s): UnknownField"));
        assert_eq!(
            result
                .control_result
                .metadata
                .get("odata_missing_projection_fields_json"),
            Some(&r#"["UnknownField"]"#.to_string())
        );
        assert_eq!(
            result
                .control_result
                .metadata
                .get("odata_projection_metadata_validated"),
            Some(&"true".to_string())
        );
    }

    #[tokio::test]
    async fn s4_odata_verification_follows_next_link_for_rowset_projection() {
        let endpoint = spawn_s4_odata_server(vec![
            (
                "/sap/opu/odata4/API_SALES_ORDER/A_SalesOrder?$top=1",
                r#"{"value":[{"SalesOrder":"500000001","NetAmount":"100.00"}],"@odata.nextLink":"/sap/opu/odata4/API_SALES_ORDER/A_SalesOrder?$skiptoken=page2"}"#,
            ),
            (
                "/sap/opu/odata4/API_SALES_ORDER/A_SalesOrder?$skiptoken=page2",
                r#"{"value":[{"SalesOrder":"500000002","NetAmount":"250.50"}]}"#,
            ),
        ])
        .await;

        let manager = VerificationManager::new(Arc::new(CapturingForwarder::default()));
        let result = manager
            .run_verification(VerificationRequest {
                control_name: "sales-order-rowset-match".to_string(),
                program_id: "program-1".to_string(),
                object_id: "object-1".to_string(),
                source_field: SourceFieldRef {
                    system: "SAP ECC".to_string(),
                    object_name: "VBAK".to_string(),
                    field_name: "ROWSET".to_string(),
                    field_path: "$".to_string(),
                    semantic_type: None,
                    record_id: None,
                },
                target_field: TargetFieldRef {
                    system: "SAP S/4HANA".to_string(),
                    object_name: "A_SalesOrder".to_string(),
                    field_name: "rowset".to_string(),
                    field_path: "$".to_string(),
                    semantic_type: None,
                    record_id: None,
                },
                expected_value: Some(json!([
                    {"SalesOrder": "500000001", "NetAmount": 100.0},
                    {"SalesOrder": "500000002", "NetAmount": 250.5}
                ])),
                tolerance: Some(0.0),
                metadata: HashMap::new(),
                source: VerificationSource {
                    transport: ConnectorTransport::SapS4OData,
                    query: None,
                    endpoint: Some(ConnectorEndpoint {
                        base_url: endpoint,
                        path: "/sap/opu/odata4/API_SALES_ORDER/A_SalesOrder?$top=1".to_string(),
                        method: "GET".to_string(),
                        headers: HashMap::new(),
                    }),
                    auth: ConnectorAuth::default(),
                    connection: HashMap::from([(
                        "odata_follow_next_link".to_string(),
                        "true".to_string(),
                    )]),
                },
            })
            .await
            .unwrap();

        assert_eq!(result.control_result.status, ControlStatus::Passed);
        assert_eq!(
            result.control_result.metadata.get("comparison_scope"),
            Some(&"rowset".to_string())
        );
        assert_eq!(
            result.control_result.metadata.get("odata_page_count"),
            Some(&"2".to_string())
        );
        assert_eq!(
            result.control_result.metadata.get("odata_row_count"),
            Some(&"2".to_string())
        );
    }

    #[tokio::test]
    async fn ecc_adapter_verification_fails_when_metadata_omits_requested_projection_field() {
        let endpoint = spawn_ecc_adapter_server(vec![(
            "/adapter/v1/records/VBAK",
            r#"{"record":{"VBELN":"500000001","NETWR":"100.00","UNDECLARED":"shadow"}}"#,
        )])
        .await;

        let manager = VerificationManager::new(Arc::new(CapturingForwarder::default()));
        let result = manager
            .run_verification(VerificationRequest {
                control_name: "vbak-projection-match".to_string(),
                program_id: "program-1".to_string(),
                object_id: "object-1".to_string(),
                source_field: SourceFieldRef {
                    system: "SAP ECC".to_string(),
                    object_name: "VBAK".to_string(),
                    field_name: "DOCUMENT".to_string(),
                    field_path: "$.projection".to_string(),
                    semantic_type: None,
                    record_id: Some("500000001".to_string()),
                },
                target_field: TargetFieldRef {
                    system: "ARCXA ECC Adapter".to_string(),
                    object_name: "VBAK".to_string(),
                    field_name: "projection".to_string(),
                    field_path: "$.projection".to_string(),
                    semantic_type: None,
                    record_id: Some("500000001".to_string()),
                },
                expected_value: Some(json!({
                    "VBELN": "500000001",
                    "UNDECLARED": "shadow"
                })),
                tolerance: Some(0.0),
                metadata: HashMap::new(),
                source: VerificationSource {
                    transport: ConnectorTransport::SapEccAdapter,
                    query: None,
                    endpoint: Some(ConnectorEndpoint {
                        base_url: endpoint,
                        path: "/adapter/v1/records/VBAK?record_id=500000001".to_string(),
                        method: "GET".to_string(),
                        headers: HashMap::new(),
                    }),
                    auth: ConnectorAuth::default(),
                    connection: HashMap::from([
                        ("ecc_adapter_version".to_string(), "0.1.0".to_string()),
                        ("ecc_system_id".to_string(), "PRD".to_string()),
                        ("ecc_client".to_string(), "100".to_string()),
                        ("ecc_object_name".to_string(), "VBAK".to_string()),
                        (
                            "ecc_key_fields_json".to_string(),
                            r#"["VBELN"]"#.to_string(),
                        ),
                        (
                            "ecc_field_types_json".to_string(),
                            r#"{"NETWR":"CURR","VBELN":"CHAR"}"#.to_string(),
                        ),
                    ]),
                },
            })
            .await
            .unwrap();

        assert_eq!(result.control_result.status, ControlStatus::Failed);
        assert!(result
            .control_result
            .summary
            .contains("does not expose requested field(s): UNDECLARED"));
        assert_eq!(
            result
                .control_result
                .metadata
                .get("ecc_missing_projection_fields_json"),
            Some(&r#"["UNDECLARED"]"#.to_string())
        );
    }

    #[tokio::test]
    async fn ecc_adapter_verification_follows_next_path_for_rowset_projection() {
        let endpoint = spawn_ecc_adapter_server(vec![
            (
                "/adapter/v1/records/VBAP?page=1",
                r#"{"rows":[{"VBELN":"500000001","POSNR":"000010"},{"VBELN":"500000001","POSNR":"000020"}],"pagination":{"next_path":"/adapter/v1/records/VBAP?page=2"}}"#,
            ),
            (
                "/adapter/v1/records/VBAP?page=2",
                r#"{"rows":[{"VBELN":"500000002","POSNR":"000010"}]}"#,
            ),
        ])
        .await;

        let manager = VerificationManager::new(Arc::new(CapturingForwarder::default()));
        let result = manager
            .run_verification(VerificationRequest {
                control_name: "vbap-rowset-match".to_string(),
                program_id: "program-1".to_string(),
                object_id: "object-1".to_string(),
                source_field: SourceFieldRef {
                    system: "SAP ECC".to_string(),
                    object_name: "VBAP".to_string(),
                    field_name: "ROWSET".to_string(),
                    field_path: "$".to_string(),
                    semantic_type: None,
                    record_id: None,
                },
                target_field: TargetFieldRef {
                    system: "ARCXA ECC Adapter".to_string(),
                    object_name: "VBAP".to_string(),
                    field_name: "rowset".to_string(),
                    field_path: "$".to_string(),
                    semantic_type: None,
                    record_id: None,
                },
                expected_value: Some(json!([
                    {"VBELN": "500000001", "POSNR": "000010"},
                    {"VBELN": "500000001", "POSNR": "000020"},
                    {"VBELN": "500000002", "POSNR": "000010"}
                ])),
                tolerance: Some(0.0),
                metadata: HashMap::new(),
                source: VerificationSource {
                    transport: ConnectorTransport::SapEccAdapter,
                    query: None,
                    endpoint: Some(ConnectorEndpoint {
                        base_url: endpoint,
                        path: "/adapter/v1/records/VBAP?page=1".to_string(),
                        method: "GET".to_string(),
                        headers: HashMap::new(),
                    }),
                    auth: ConnectorAuth::default(),
                    connection: HashMap::from([(
                        "ecc_follow_next_path".to_string(),
                        "true".to_string(),
                    )]),
                },
            })
            .await
            .unwrap();

        assert_eq!(result.control_result.status, ControlStatus::Passed);
        assert_eq!(
            result.control_result.metadata.get("comparison_scope"),
            Some(&"rowset".to_string())
        );
        assert_eq!(
            result.control_result.metadata.get("ecc_page_count"),
            Some(&"2".to_string())
        );
        assert_eq!(
            result.control_result.metadata.get("ecc_row_count"),
            Some(&"3".to_string())
        );
    }

    #[tokio::test]
    async fn ecc_adapter_verification_rejects_page_size_above_capability_limit() {
        let endpoint = spawn_ecc_adapter_server(vec![]).await;

        let manager = VerificationManager::new(Arc::new(CapturingForwarder::default()));
        let error = manager
            .run_verification(VerificationRequest {
                control_name: "ecc-page-size-limit".to_string(),
                program_id: "program-1".to_string(),
                object_id: "object-1".to_string(),
                source_field: SourceFieldRef {
                    system: "SAP ECC".to_string(),
                    object_name: "VBAK".to_string(),
                    field_name: "NETWR".to_string(),
                    field_path: "$.NETWR".to_string(),
                    semantic_type: None,
                    record_id: Some("500000001".to_string()),
                },
                target_field: TargetFieldRef {
                    system: "ARCXA ECC Adapter".to_string(),
                    object_name: "VBAK".to_string(),
                    field_name: "NETWR".to_string(),
                    field_path: "$.NETWR".to_string(),
                    semantic_type: None,
                    record_id: Some("500000001".to_string()),
                },
                expected_value: Some(json!(100.0)),
                tolerance: Some(0.0),
                metadata: HashMap::new(),
                source: VerificationSource {
                    transport: ConnectorTransport::SapEccAdapter,
                    query: None,
                    endpoint: Some(ConnectorEndpoint {
                        base_url: endpoint,
                        path: "/adapter/v1/records/VBAK?record_id=500000001".to_string(),
                        method: "GET".to_string(),
                        headers: HashMap::new(),
                    }),
                    auth: ConnectorAuth::default(),
                    connection: HashMap::from([
                        (
                            "ecc_field_types_json".to_string(),
                            "{\"VBELN\":\"CHAR\",\"NETWR\":\"CURR\"}".to_string(),
                        ),
                        ("ecc_max_page_size".to_string(), "200".to_string()),
                        ("ecc_page_size".to_string(), "500".to_string()),
                    ]),
                },
            })
            .await
            .expect_err("page-size limit should fail before live adapter call");

        assert!(error
            .to_string()
            .contains("requested page_size 500 above capability limit 200"));
    }

    #[tokio::test]
    async fn ecc_adapter_verification_reuses_cached_session_and_rotated_secret() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let requests_handle = requests.clone();
        tokio::spawn(async move {
            for _ in 0..2 {
                if let Ok((mut socket, _)) = listener.accept().await {
                    let mut buffer = vec![0u8; 4096];
                    let bytes = socket.read(&mut buffer).await.unwrap_or(0);
                    let request = String::from_utf8_lossy(&buffer[..bytes]).to_string();
                    requests_handle.lock().unwrap().push(request);
                    let response_body = r#"{"record":{"VBELN":"500000001","NETWR":"100.00"},"session":{"id":"sess-1"}}"#;
                    let response = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                        response_body.len(),
                        response_body
                    );
                    let _ = socket.write_all(response.as_bytes()).await;
                }
            }
        });

        let registry = Arc::new(SecretStoreRegistry::new());
        let store = Arc::new(InlineSecretStore::new());
        registry.register("default", store.clone());
        registry.set_default(store.clone());
        let first_version = put_secret_by_ref(
            store.as_ref(),
            "vault://migration/ecc/adapter-token",
            SecretValue::String("token-one".to_string()),
            None,
        )
        .await
        .unwrap();

        let manager = VerificationManager::new(Arc::new(CapturingForwarder::default()))
            .with_secret_store_registry(registry.clone());
        let build_request = |base_url: String| VerificationRequest {
            control_name: "ecc-secret-rotation".to_string(),
            program_id: "program-1".to_string(),
            object_id: "object-1".to_string(),
            source_field: SourceFieldRef {
                system: "SAP ECC".to_string(),
                object_name: "VBAK".to_string(),
                field_name: "NETWR".to_string(),
                field_path: "$.NETWR".to_string(),
                semantic_type: None,
                record_id: Some("500000001".to_string()),
            },
            target_field: TargetFieldRef {
                system: "ARCXA ECC Adapter".to_string(),
                object_name: "VBAK".to_string(),
                field_name: "NETWR".to_string(),
                field_path: "$.NETWR".to_string(),
                semantic_type: None,
                record_id: Some("500000001".to_string()),
            },
            expected_value: Some(json!(100.0)),
            tolerance: Some(0.0),
            metadata: HashMap::new(),
            source: VerificationSource {
                transport: ConnectorTransport::SapEccAdapter,
                query: None,
                endpoint: Some(ConnectorEndpoint {
                    base_url,
                    path: "/adapter/v1/records/VBAK?record_id=500000001".to_string(),
                    method: "GET".to_string(),
                    headers: HashMap::new(),
                }),
                auth: ConnectorAuth {
                    kind: ConnectorAuthKind::Bearer,
                    secret_ref: Some("vault://migration/ecc/adapter-token".to_string()),
                    token: None,
                    api_key: None,
                    header_name: None,
                    username: None,
                    password: None,
                },
                connection: HashMap::from([
                    (
                        "ecc_field_types_json".to_string(),
                        "{\"VBELN\":\"CHAR\",\"NETWR\":\"CURR\"}".to_string(),
                    ),
                    ("ecc_session_mode".to_string(), "cached".to_string()),
                    (
                        "ecc_session_id_path".to_string(),
                        "$.session.id".to_string(),
                    ),
                    (
                        "ecc_session_id_parameter_name".to_string(),
                        "sessionId".to_string(),
                    ),
                    ("ecc_session_ttl_seconds".to_string(), "300".to_string()),
                ]),
            },
        };

        let first = manager
            .run_verification(build_request(format!("http://{}", addr)))
            .await
            .unwrap();

        let second_version = put_secret_by_ref(
            store.as_ref(),
            "vault://migration/ecc/adapter-token",
            SecretValue::String("token-two".to_string()),
            None,
        )
        .await
        .unwrap();

        let second = manager
            .run_verification(build_request(format!("http://{}", addr)))
            .await
            .unwrap();

        assert_eq!(
            first
                .control_result
                .metadata
                .get("source_auth_secret_version"),
            Some(&first_version)
        );
        assert_eq!(
            second
                .control_result
                .metadata
                .get("source_auth_secret_version"),
            Some(&second_version)
        );
        assert_eq!(
            second.control_result.metadata.get("ecc_session_reused"),
            Some(&"true".to_string())
        );

        let captured = requests.lock().unwrap();
        assert_eq!(captured.len(), 2);
        assert!(
            captured[0].contains("Authorization: Bearer token-one")
                || captured[0].contains("authorization: Bearer token-one")
        );
        assert!(
            captured[1].contains("Authorization: Bearer token-two")
                || captured[1].contains("authorization: Bearer token-two")
        );
        assert!(!captured[0].contains("sessionId=sess-1"));
        assert!(captured[1].contains("sessionId=sess-1"));
    }

    #[tokio::test]
    async fn ecc_rfc_bapi_verification_fails_when_metadata_omits_requested_projection_field() {
        let endpoint = spawn_ecc_rfc_server(vec![(
            "/bridge/v1/read/VBAK?record_id=500000001",
            r#"{"result":{"VBELN":"500000001","NETWR":"100.00","WAERK":"USD"}}"#,
        )])
        .await;

        let manager = VerificationManager::new(Arc::new(CapturingForwarder::default()));
        let result = manager
            .run_verification(VerificationRequest {
                control_name: "rfc-projection-match".to_string(),
                program_id: "program-1".to_string(),
                object_id: "object-1".to_string(),
                source_field: SourceFieldRef {
                    system: "SAP ECC".to_string(),
                    object_name: "VBAK".to_string(),
                    field_name: "projection".to_string(),
                    field_path: "$.projection".to_string(),
                    semantic_type: None,
                    record_id: Some("500000001".to_string()),
                },
                target_field: TargetFieldRef {
                    system: "SAP ECC RFC".to_string(),
                    object_name: "VBAK".to_string(),
                    field_name: "projection".to_string(),
                    field_path: "$.projection".to_string(),
                    semantic_type: None,
                    record_id: Some("500000001".to_string()),
                },
                expected_value: Some(json!({
                    "VBELN": "500000001",
                    "NETWR": 100.0,
                    "UNDECLARED": "x"
                })),
                tolerance: Some(0.0),
                metadata: HashMap::new(),
                source: VerificationSource {
                    transport: ConnectorTransport::SapEccRfcBapi,
                    query: None,
                    endpoint: Some(ConnectorEndpoint {
                        base_url: endpoint,
                        path: "/bridge/v1/read/VBAK?record_id=500000001".to_string(),
                        method: "GET".to_string(),
                        headers: HashMap::new(),
                    }),
                    auth: ConnectorAuth::default(),
                    connection: HashMap::from([
                        (
                            "ecc_rfc_field_types_json".to_string(),
                            "{\"VBELN\":\"CHAR\",\"NETWR\":\"CURR\",\"WAERK\":\"CUKY\"}"
                                .to_string(),
                        ),
                        (
                            "ecc_rfc_profile".to_string(),
                            "bapi_record_lookup".to_string(),
                        ),
                        (
                            "ecc_rfc_key_fields_json".to_string(),
                            "[\"VBELN\"]".to_string(),
                        ),
                        (
                            "ecc_rfc_required_parameters_json".to_string(),
                            "[\"record_id\"]".to_string(),
                        ),
                        (
                            "ecc_rfc_bapi_name".to_string(),
                            "BAPI_SALESORDER_GETDETAIL".to_string(),
                        ),
                    ]),
                },
            })
            .await
            .unwrap();

        assert_eq!(result.control_result.status, ControlStatus::Failed);
        assert!(result
            .control_result
            .summary
            .contains("does not expose requested field(s): UNDECLARED"));
        assert_eq!(
            result
                .control_result
                .metadata
                .get("ecc_rfc_missing_projection_fields_json"),
            Some(&r#"["UNDECLARED"]"#.to_string())
        );
        assert_eq!(
            result.control_result.metadata.get("ecc_rfc_profile"),
            Some(&"bapi_record_lookup".to_string())
        );
    }

    #[tokio::test]
    async fn ecc_rfc_bapi_verification_follows_next_cursor_for_rowset_projection() {
        let endpoint = spawn_ecc_rfc_server(vec![
            (
                "/bridge/v1/read/VBAP?record_id=500000001&cursorToken=cursor-2",
                r#"{"rows":[{"VBELN":"500000002","POSNR":"000010"}]}"#,
            ),
            (
                "/bridge/v1/read/VBAP?record_id=500000001",
                r#"{"rows":[{"VBELN":"500000001","POSNR":"000010"},{"VBELN":"500000001","POSNR":"000020"}],"pagination":{"token":"cursor-2"}}"#,
            ),
        ])
        .await;

        let manager = VerificationManager::new(Arc::new(CapturingForwarder::default()));
        let result = manager
            .run_verification(VerificationRequest {
                control_name: "vbap-rfc-rowset-match".to_string(),
                program_id: "program-1".to_string(),
                object_id: "object-1".to_string(),
                source_field: SourceFieldRef {
                    system: "SAP ECC".to_string(),
                    object_name: "VBAP".to_string(),
                    field_name: "ROWSET".to_string(),
                    field_path: "$".to_string(),
                    semantic_type: None,
                    record_id: None,
                },
                target_field: TargetFieldRef {
                    system: "SAP ECC RFC".to_string(),
                    object_name: "VBAP".to_string(),
                    field_name: "rowset".to_string(),
                    field_path: "$".to_string(),
                    semantic_type: None,
                    record_id: None,
                },
                expected_value: Some(json!([
                    {"VBELN": "500000001", "POSNR": "000010"},
                    {"VBELN": "500000001", "POSNR": "000020"},
                    {"VBELN": "500000002", "POSNR": "000010"}
                ])),
                tolerance: Some(0.0),
                metadata: HashMap::new(),
                source: VerificationSource {
                    transport: ConnectorTransport::SapEccRfcBapi,
                    query: None,
                    endpoint: Some(ConnectorEndpoint {
                        base_url: endpoint,
                        path: "/bridge/v1/read/VBAP?record_id=500000001".to_string(),
                        method: "GET".to_string(),
                        headers: HashMap::new(),
                    }),
                    auth: ConnectorAuth::default(),
                    connection: HashMap::from([
                        ("ecc_rfc_follow_next_cursor".to_string(), "true".to_string()),
                        (
                            "ecc_rfc_cursor_parameter_name".to_string(),
                            "cursorToken".to_string(),
                        ),
                        (
                            "ecc_rfc_next_cursor_path".to_string(),
                            "$.pagination.token".to_string(),
                        ),
                        (
                            "ecc_rfc_request_params_json".to_string(),
                            r#"{"record_id":"500000001"}"#.to_string(),
                        ),
                    ]),
                },
            })
            .await
            .unwrap();

        assert_eq!(result.control_result.status, ControlStatus::Passed);
        assert_eq!(
            result.control_result.metadata.get("comparison_scope"),
            Some(&"rowset".to_string())
        );
        assert_eq!(
            result.control_result.metadata.get("ecc_rfc_page_count"),
            Some(&"2".to_string())
        );
        assert_eq!(
            result.control_result.metadata.get("ecc_rfc_row_count"),
            Some(&"3".to_string())
        );
        assert_eq!(
            result
                .control_result
                .metadata
                .get("ecc_rfc_request_parameters_json"),
            Some(&r#"{"record_id":"500000001"}"#.to_string())
        );
    }

    #[tokio::test]
    async fn ecc_rfc_bapi_verification_rejects_missing_required_request_parameters() {
        let endpoint = spawn_ecc_rfc_server(vec![]).await;

        let manager = VerificationManager::new(Arc::new(CapturingForwarder::default()));
        let error = manager
            .run_verification(VerificationRequest {
                control_name: "rfc-required-params".to_string(),
                program_id: "program-1".to_string(),
                object_id: "object-1".to_string(),
                source_field: SourceFieldRef {
                    system: "SAP ECC".to_string(),
                    object_name: "VBAK".to_string(),
                    field_name: "NETWR".to_string(),
                    field_path: "$.NETWR".to_string(),
                    semantic_type: None,
                    record_id: Some("500000001".to_string()),
                },
                target_field: TargetFieldRef {
                    system: "SAP ECC RFC".to_string(),
                    object_name: "VBAK".to_string(),
                    field_name: "NETWR".to_string(),
                    field_path: "$.NETWR".to_string(),
                    semantic_type: None,
                    record_id: Some("500000001".to_string()),
                },
                expected_value: Some(json!(100.0)),
                tolerance: Some(0.0),
                metadata: HashMap::new(),
                source: VerificationSource {
                    transport: ConnectorTransport::SapEccRfcBapi,
                    query: None,
                    endpoint: Some(ConnectorEndpoint {
                        base_url: endpoint,
                        path: "/bridge/v1/read/VBAK".to_string(),
                        method: "GET".to_string(),
                        headers: HashMap::new(),
                    }),
                    auth: ConnectorAuth::default(),
                    connection: HashMap::from([
                        (
                            "ecc_rfc_required_parameters_json".to_string(),
                            r#"["record_id"]"#.to_string(),
                        ),
                        ("ecc_rfc_field_types_json".to_string(), "{}".to_string()),
                    ]),
                },
            })
            .await
            .expect_err("missing required request parameters should fail early");

        assert!(error
            .to_string()
            .contains("missing required request parameter(s): record_id"));
    }

    #[tokio::test]
    async fn ecc_rfc_bapi_verification_rejects_unsupported_session_mode() {
        let endpoint = spawn_ecc_rfc_server(vec![]).await;

        let manager = VerificationManager::new(Arc::new(CapturingForwarder::default()));
        let error = manager
            .run_verification(VerificationRequest {
                control_name: "rfc-session-mode".to_string(),
                program_id: "program-1".to_string(),
                object_id: "object-1".to_string(),
                source_field: SourceFieldRef {
                    system: "SAP ECC".to_string(),
                    object_name: "VBAK".to_string(),
                    field_name: "NETWR".to_string(),
                    field_path: "$.NETWR".to_string(),
                    semantic_type: None,
                    record_id: Some("500000001".to_string()),
                },
                target_field: TargetFieldRef {
                    system: "SAP ECC RFC".to_string(),
                    object_name: "VBAK".to_string(),
                    field_name: "NETWR".to_string(),
                    field_path: "$.NETWR".to_string(),
                    semantic_type: None,
                    record_id: Some("500000001".to_string()),
                },
                expected_value: Some(json!(100.0)),
                tolerance: Some(0.0),
                metadata: HashMap::new(),
                source: VerificationSource {
                    transport: ConnectorTransport::SapEccRfcBapi,
                    query: None,
                    endpoint: Some(ConnectorEndpoint {
                        base_url: endpoint,
                        path: "/bridge/v1/read/VBAK?record_id=500000001".to_string(),
                        method: "GET".to_string(),
                        headers: HashMap::new(),
                    }),
                    auth: ConnectorAuth::default(),
                    connection: HashMap::from([
                        (
                            "ecc_rfc_field_types_json".to_string(),
                            "{\"VBELN\":\"CHAR\",\"NETWR\":\"CURR\"}".to_string(),
                        ),
                        (
                            "ecc_rfc_supported_session_modes_json".to_string(),
                            "[\"stateless\"]".to_string(),
                        ),
                        ("ecc_rfc_session_mode".to_string(), "stateful".to_string()),
                    ]),
                },
            })
            .await
            .expect_err("unsupported session mode should fail before live RFC call");

        assert!(error
            .to_string()
            .contains("requested unsupported session_mode"));
    }

    #[tokio::test]
    async fn ecc_rfc_bapi_verification_closes_required_stateful_session() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let requests_handle = requests.clone();
        tokio::spawn(async move {
            for _ in 0..2 {
                if let Ok((mut socket, _)) = listener.accept().await {
                    let mut buffer = vec![0u8; 4096];
                    let bytes = socket.read(&mut buffer).await.unwrap_or(0);
                    let request = String::from_utf8_lossy(&buffer[..bytes]).to_string();
                    requests_handle.lock().unwrap().push(request.clone());
                    let (status, body) = if request.contains("/bridge/v1/session/close") {
                        ("200 OK", "{\"closed\":true}")
                    } else {
                        (
                            "200 OK",
                            r#"{"result":{"VBELN":"500000001","NETWR":"100.00"},"session":{"id":"bridge-session-1"}}"#,
                        )
                    };
                    let response = format!(
                        "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = socket.write_all(response.as_bytes()).await;
                }
            }
        });

        let manager = VerificationManager::new(Arc::new(CapturingForwarder::default()));
        let result = manager
            .run_verification(VerificationRequest {
                control_name: "rfc-session-close".to_string(),
                program_id: "program-1".to_string(),
                object_id: "object-1".to_string(),
                source_field: SourceFieldRef {
                    system: "SAP ECC".to_string(),
                    object_name: "VBAK".to_string(),
                    field_name: "NETWR".to_string(),
                    field_path: "$.NETWR".to_string(),
                    semantic_type: None,
                    record_id: Some("500000001".to_string()),
                },
                target_field: TargetFieldRef {
                    system: "SAP ECC RFC".to_string(),
                    object_name: "VBAK".to_string(),
                    field_name: "NETWR".to_string(),
                    field_path: "$.NETWR".to_string(),
                    semantic_type: None,
                    record_id: Some("500000001".to_string()),
                },
                expected_value: Some(json!(100.0)),
                tolerance: Some(0.0),
                metadata: HashMap::new(),
                source: VerificationSource {
                    transport: ConnectorTransport::SapEccRfcBapi,
                    query: None,
                    endpoint: Some(ConnectorEndpoint {
                        base_url: format!("http://{}", addr),
                        path: "/bridge/v1/read/VBAK?record_id=500000001".to_string(),
                        method: "GET".to_string(),
                        headers: HashMap::new(),
                    }),
                    auth: ConnectorAuth::default(),
                    connection: HashMap::from([
                        (
                            "ecc_rfc_field_types_json".to_string(),
                            "{\"VBELN\":\"CHAR\",\"NETWR\":\"CURR\"}".to_string(),
                        ),
                        (
                            "ecc_rfc_required_parameters_json".to_string(),
                            "[\"record_id\"]".to_string(),
                        ),
                        (
                            "ecc_rfc_request_params_json".to_string(),
                            r#"{"record_id":"500000001"}"#.to_string(),
                        ),
                        ("ecc_rfc_session_mode".to_string(), "stateful".to_string()),
                        (
                            "ecc_rfc_session_id_path".to_string(),
                            "$.session.id".to_string(),
                        ),
                        (
                            "ecc_rfc_session_id_parameter_name".to_string(),
                            "sessionId".to_string(),
                        ),
                        (
                            "ecc_rfc_close_session_path".to_string(),
                            "/bridge/v1/session/close".to_string(),
                        ),
                        (
                            "ecc_rfc_close_session_method".to_string(),
                            "POST".to_string(),
                        ),
                        (
                            "ecc_rfc_requires_explicit_session_close".to_string(),
                            "true".to_string(),
                        ),
                    ]),
                },
            })
            .await
            .unwrap();

        assert_eq!(
            result.control_result.metadata.get("ecc_rfc_session_closed"),
            Some(&"true".to_string())
        );
        let captured = requests.lock().unwrap();
        assert_eq!(captured.len(), 2);
        assert!(captured[0].contains("/bridge/v1/read/VBAK?record_id=500000001"));
        assert!(captured[1].contains("/bridge/v1/session/close?sessionId=bridge-session-1"));
    }

    #[test]
    fn assessment_coerces_numeric_strings_into_numeric_controls() {
        let assessment = assess_values(Some(&json!("10.50")), Some(&json!(10.5)), Some(0.0));

        assert_eq!(assessment.status, ControlStatus::Passed);
        assert_eq!(assessment.comparison_mode, "numeric_tolerance");
        assert_eq!(assessment.expected_kind, "number");
        assert_eq!(assessment.actual_kind, "number");
        assert_eq!(assessment.numeric_delta, Some(0.0));
    }

    #[test]
    fn assessment_understands_timestamp_strings() {
        let assessment = assess_values(
            Some(&json!("2026-05-01T12:30:00Z")),
            Some(&json!("2026-05-01 12:30:00")),
            None,
        );

        assert_eq!(assessment.status, ControlStatus::Passed);
        assert_eq!(assessment.comparison_mode, "timestamp_exact");
        assert_eq!(assessment.expected_kind, "timestamp");
        assert_eq!(assessment.actual_kind, "timestamp");
    }

    #[test]
    fn merged_control_metadata_includes_typed_comparison_fields() {
        let assessment = assess_values(Some(&json!(100)), Some(&json!(104)), Some(2.0));
        let metadata = merge_control_metadata(&HashMap::new(), &assessment);

        assert_eq!(
            metadata.get("comparison_mode"),
            Some(&"numeric_tolerance".to_string())
        );
        assert_eq!(
            metadata.get("comparison_scope"),
            Some(&"scalar".to_string())
        );
        assert_eq!(
            metadata.get("expected_value_type"),
            Some(&"number".to_string())
        );
        assert_eq!(
            metadata.get("actual_value_type"),
            Some(&"number".to_string())
        );
        assert_eq!(metadata.get("tolerance_applied"), Some(&"true".to_string()));
        assert_eq!(metadata.get("numeric_delta"), Some(&"4".to_string()));
    }

    #[test]
    fn multi_column_query_result_returns_object_for_record_projection_checks() {
        let result = QueryResult {
            rows: vec![json!({
                "DOC_ID": "4900001234",
                "AMOUNT": 100.5,
                "CURRENCY": "USD"
            })],
            row_count: 1,
            execution_time_ms: 12,
            truncated: false,
            columns: None,
        };

        let actual = first_query_value(result).unwrap();

        assert_eq!(
            actual,
            json!({
                "DOC_ID": "4900001234",
                "AMOUNT": 100.5,
                "CURRENCY": "USD"
            })
        );
    }

    #[test]
    fn assessment_understands_multi_column_record_projection() {
        let assessment = assess_values(
            Some(&json!({
                "DOC_ID": "4900001234",
                "AMOUNT": 100.0,
                "CURRENCY": "USD"
            })),
            Some(&json!({
                "DOC_ID": "4900001234",
                "AMOUNT": 100.0,
                "CURRENCY": "USD",
                "LAST_CHANGED_AT": "2026-05-01T12:00:00Z"
            })),
            Some(0.0),
        );

        assert_eq!(assessment.status, ControlStatus::Passed);
        assert_eq!(assessment.comparison_mode, "object_fieldwise");
        assert_eq!(assessment.comparison_scope, "record_projection");
        assert_eq!(assessment.verified_field_count, 3);
        assert_eq!(assessment.mismatch_field_count, 0);
        assert_eq!(
            assessment.unexpected_actual_fields,
            vec!["LAST_CHANGED_AT".to_string()]
        );
    }

    #[test]
    fn assessment_understands_aggregate_projection_with_numeric_tolerance() {
        let assessment = assess_values(
            Some(&json!({
                "ROW_COUNT": 10,
                "SUM_AMOUNT": 250.0
            })),
            Some(&json!({
                "ROW_COUNT": 10,
                "SUM_AMOUNT": 250.4
            })),
            Some(0.5),
        );

        assert_eq!(assessment.status, ControlStatus::Passed);
        assert_eq!(assessment.comparison_mode, "object_fieldwise");
        assert_eq!(assessment.comparison_scope, "aggregate_projection");
        assert_eq!(
            assessment.aggregate_keys,
            vec!["ROW_COUNT".to_string(), "SUM_AMOUNT".to_string()]
        );
        assert_eq!(assessment.mismatch_field_count, 0);
    }

    #[test]
    fn assessment_fails_when_expected_projection_field_is_missing() {
        let assessment = assess_values(
            Some(&json!({
                "DOC_ID": "4900001234",
                "AMOUNT": 100.0
            })),
            Some(&json!({
                "DOC_ID": "4900001234"
            })),
            None,
        );

        assert_eq!(assessment.status, ControlStatus::Failed);
        assert_eq!(assessment.missing_actual_fields, vec!["AMOUNT".to_string()]);
        assert_eq!(assessment.mismatch_field_count, 1);
    }
}
