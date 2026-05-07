use crate::store::PersistedConnectorStore;
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use graphica_core::distributed::proto::migration_evidence::{
    evidence_ingestion_service_server::EvidenceIngestionService,
    verification_service_client::VerificationServiceClient, ConnectorReply, GetConnectorRequest,
    HealthRequest, HealthResponse, RunConnectorRequest, RunConnectorResponse,
    RunVerificationAndEmitRequest, RuntimeStatusReply, RuntimeStatusRequest,
    UpsertConnectorRequest,
};
use graphica_core::migration_evidence::{
    discover_sap_ecc_adapter_capabilities, discover_sap_ecc_rfc_bapi_capabilities,
    discover_sap_s4_odata_capabilities, field_types_by_name, infer_sap_s4_odata_metadata_path,
    resolve_connector_auth, rfc_field_types_by_name, ConnectorAuthKind,
    ConnectorAuthResolutionMetadata, ConnectorRunRequest as DomainConnectorRunRequest,
    ConnectorRunSummary, ConnectorTransport, ControlResult, EvidenceIngestionRuntimeStatus,
    ExecutionEvent, ExecutionStatus, MigrationConnector, MigrationConnectorRole,
    MigrationEvidenceArtifactType, MigrationEvidenceDeliveryMode, MigrationEvidenceEvent,
    MigrationEvidenceEventForwarder, MigrationObject, MigrationObjectType, MigrationProgram,
    SapEccAdapterCapabilities, SapEccRfcBapiCapabilities, SapEccRfcBapiProfile,
    SapEccStagedApprovalEvidence, SapEccStagedControlEvidence, SapEccStagedExceptionEvidence,
    SapEccStagedExecutionEvidence, SapEccStagedExportBundle, SapEccStagedExportDataFormat,
    SapEccStagedExportDataSet, SapEccStagedExportManifest, SapEccStagedRuleEvidence,
    SapExtractorFamily, SapIdocExtractorApprovalEvidence, SapIdocExtractorBundle,
    SapIdocExtractorControlEvidence, SapIdocExtractorDataFormat, SapIdocExtractorDataSet,
    SapIdocExtractorExceptionEvidence, SapIdocExtractorExecutionEvidence, SapIdocExtractorManifest,
    SapS4ODataCapabilities, SapS4ODataVersion, VerificationDispatchRequest,
    VerificationDispatchResult, VerificationRequest,
};
use graphica_core::secrets::providers::SecretStoreRegistry;
use reqwest::Method;
use serde::de::DeserializeOwned;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::time::Instant;
use tonic::{Request, Response, Status};

#[async_trait]
pub trait VerificationProvider: Send + Sync {
    async fn run_verification_and_emit(
        &self,
        request: VerificationDispatchRequest,
    ) -> Result<VerificationDispatchResult>;
}

#[derive(Clone)]
pub struct GrpcVerificationForwarder {
    endpoint: String,
}

impl GrpcVerificationForwarder {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
        }
    }
}

#[async_trait]
impl VerificationProvider for GrpcVerificationForwarder {
    async fn run_verification_and_emit(
        &self,
        request: VerificationDispatchRequest,
    ) -> Result<VerificationDispatchResult> {
        let mut client = VerificationServiceClient::connect(self.endpoint.clone()).await?;
        let response = client
            .run_verification_and_emit(RunVerificationAndEmitRequest {
                verification_dispatch_request_json: serde_json::to_string(&request)?,
            })
            .await?
            .into_inner();
        serde_json::from_str(&response.verification_dispatch_result_json)
            .context("failed to deserialize verification dispatch response")
    }
}

#[derive(Clone)]
pub struct EvidenceIngestionManager {
    store: PersistedConnectorStore,
    traceability: std::sync::Arc<dyn MigrationEvidenceEventForwarder>,
    verification: std::sync::Arc<dyn VerificationProvider>,
    delivery_mode: MigrationEvidenceDeliveryMode,
    secret_store_registry: Option<std::sync::Arc<SecretStoreRegistry>>,
}

impl EvidenceIngestionManager {
    pub fn new(
        store: PersistedConnectorStore,
        traceability: std::sync::Arc<dyn MigrationEvidenceEventForwarder>,
        verification: std::sync::Arc<dyn VerificationProvider>,
        delivery_mode: MigrationEvidenceDeliveryMode,
    ) -> Self {
        Self {
            store,
            traceability,
            verification,
            delivery_mode,
            secret_store_registry: None,
        }
    }

    pub fn with_secret_store_registry(
        mut self,
        registry: std::sync::Arc<SecretStoreRegistry>,
    ) -> Self {
        self.secret_store_registry = Some(registry);
        self
    }

    pub async fn upsert_connector(
        &self,
        mut connector: MigrationConnector,
    ) -> Result<MigrationConnector> {
        validate_connector(&connector)?;
        connector.updated_at = Utc::now();
        self.store.upsert(connector).await
    }

    pub async fn get_connector(&self, connector_id: &str) -> Result<MigrationConnector> {
        self.store
            .get(connector_id)
            .await
            .ok_or_else(|| anyhow!("unknown connector '{}'", connector_id))
    }

    pub async fn run_connector(
        &self,
        connector_id: &str,
        request: DomainConnectorRunRequest,
    ) -> Result<(ConnectorRunSummary, Vec<MigrationEvidenceEvent>)> {
        let mut connector = self.get_connector(connector_id).await?;
        if !connector.enabled {
            return Err(anyhow!(
                "connector '{}' is disabled",
                connector.connector_id
            ));
        }
        if let Some(discovered) = discover_connector_capabilities(
            &connector,
            &request,
            self.secret_store_registry.clone(),
        )
        .await?
        {
            connector = self.store.upsert(discovered).await?;
        }
        let started_at = Utc::now();
        let run_id = format!("run-{}", uuid::Uuid::new_v4());

        let (mut events, dispatch_override) =
            if connector.role == MigrationConnectorRole::VerificationSource {
                let mut verification = request.verification.ok_or_else(|| {
                    anyhow!("verification connector runs require a verification payload")
                })?;
                enrich_verification_source_from_connector(&connector, &mut verification);
                let verification_dispatch = self
                    .verification
                    .run_verification_and_emit(VerificationDispatchRequest {
                        connector_id: connector.connector_id.clone(),
                        run_id: run_id.clone(),
                        vendor: connector.vendor.clone(),
                        verification,
                    })
                    .await?;
                (
                    verification_dispatch.emitted_events,
                    Some(verification_dispatch.dispatch_summary),
                )
            } else if !request.manual_events.is_empty() {
                (request.manual_events, None)
            } else {
                (
                    fetch_connector_events(
                        &connector,
                        &run_id,
                        request.request_body,
                        request.request_headers,
                        self.secret_store_registry.clone(),
                    )
                    .await?,
                    None,
                )
            };

        normalize_events(&connector, &run_id, &mut events)?;
        let dispatch = if let Some(dispatch) = dispatch_override {
            dispatch
        } else {
            self.traceability.ingest_events(events.clone()).await?
        };
        let completed_at = Utc::now();

        Ok((
            ConnectorRunSummary {
                run_id,
                connector_id: connector.connector_id.clone(),
                ingested_event_count: dispatch.accepted_event_count,
                delivery_mode: dispatch.delivery_mode,
                traceability_acknowledged: dispatch.traceability_acknowledged,
                touched_program_ids: dispatch.touched_program_ids,
                touched_object_ids: dispatch.touched_object_ids,
                started_at,
                completed_at,
            },
            events,
        ))
    }

    pub async fn runtime_status(&self) -> Result<EvidenceIngestionRuntimeStatus> {
        Ok(EvidenceIngestionRuntimeStatus {
            connector_store: self.store.runtime_status().await,
            delivery_mode: self.delivery_mode.clone(),
            verification_service_configured: true,
            updated_at: Utc::now(),
        })
    }
}

fn validate_connector(connector: &MigrationConnector) -> Result<()> {
    if connector.connector_id.is_empty() {
        return Err(anyhow!("connector_id cannot be empty"));
    }
    if connector.program_id.is_empty() {
        return Err(anyhow!("program_id cannot be empty"));
    }
    if matches!(
        connector.role,
        MigrationConnectorRole::MigrationArtifactSource
    ) && connector.endpoint.base_url.is_empty()
        && !matches!(
            connector.transport,
            ConnectorTransport::SapEccStagedExport
                | ConnectorTransport::SapIdocExtractorPackage
                | ConnectorTransport::SapOdpExtractorPackage
                | ConnectorTransport::ManualDrop
        )
    {
        return Err(anyhow!("artifact source connectors require base_url"));
    }
    Ok(())
}

async fn discover_connector_capabilities(
    connector: &MigrationConnector,
    request: &DomainConnectorRunRequest,
    secret_store_registry: Option<std::sync::Arc<SecretStoreRegistry>>,
) -> Result<Option<MigrationConnector>> {
    let endpoint = request
        .verification
        .as_ref()
        .and_then(|verification| verification.source.endpoint.as_ref())
        .unwrap_or(&connector.endpoint);
    let auth = request
        .verification
        .as_ref()
        .map(|verification| &verification.source.auth)
        .filter(|auth| auth.kind != ConnectorAuthKind::None)
        .unwrap_or(&connector.auth);
    let resolved_auth = resolve_connector_auth(auth, secret_store_registry).await?;
    let extra_headers = request
        .verification
        .as_ref()
        .map(|verification| build_odata_headers_from_connection(&verification.source.connection))
        .unwrap_or_default();

    match connector.transport {
        ConnectorTransport::SapS4OData => {
            if connector
                .metadata
                .get("odata_metadata_discovered_at")
                .is_some()
                && connector
                    .metadata
                    .get("force_odata_metadata_refresh")
                    .map(String::as_str)
                    != Some("true")
            {
                return Ok(None);
            }

            let document = fetch_s4_metadata_document(
                endpoint,
                &resolved_auth.auth,
                &connector.metadata,
                extra_headers,
            )
            .await?;
            let capabilities = discover_sap_s4_odata_capabilities(&document, &endpoint.path)
                .context("failed to derive SAP S/4HANA OData connector capabilities")?;

            let mut enriched = connector.clone();
            apply_s4_capabilities_metadata(&mut enriched, endpoint, &capabilities)?;
            apply_auth_resolution_metadata(
                &mut enriched.metadata,
                "auth_",
                &resolved_auth.metadata,
            );
            enriched.updated_at = Utc::now();
            Ok(Some(enriched))
        }
        ConnectorTransport::SapEccAdapter => {
            if connector
                .metadata
                .get("ecc_capabilities_discovered_at")
                .is_some()
                && connector
                    .metadata
                    .get("force_ecc_capabilities_refresh")
                    .map(String::as_str)
                    != Some("true")
            {
                return Ok(None);
            }

            let payload =
                fetch_ecc_capabilities_document(endpoint, &resolved_auth.auth, &connector.metadata)
                    .await?;
            let capabilities = discover_sap_ecc_adapter_capabilities(&payload)
                .context("failed to derive SAP ECC adapter connector capabilities")?;
            let health = fetch_optional_json_document(
                build_optional_health_url(
                    endpoint,
                    &connector.metadata,
                    capabilities.health_path.as_deref(),
                    "ecc_health_path",
                    "/adapter/v1/health",
                ),
                &resolved_auth.auth,
                &endpoint.headers,
            )
            .await;

            let mut enriched = connector.clone();
            apply_ecc_capabilities_metadata(&mut enriched, endpoint, &capabilities)?;
            apply_connector_health_metadata(&mut enriched, "ecc", health)?;
            apply_auth_resolution_metadata(
                &mut enriched.metadata,
                "auth_",
                &resolved_auth.metadata,
            );
            enriched.updated_at = Utc::now();
            Ok(Some(enriched))
        }
        ConnectorTransport::SapEccRfcBapi => {
            if connector
                .metadata
                .get("ecc_rfc_capabilities_discovered_at")
                .is_some()
                && connector
                    .metadata
                    .get("force_ecc_rfc_capabilities_refresh")
                    .map(String::as_str)
                    != Some("true")
            {
                return Ok(None);
            }

            let payload = fetch_ecc_rfc_capabilities_document(
                endpoint,
                &resolved_auth.auth,
                &connector.metadata,
            )
            .await?;
            let capabilities = discover_sap_ecc_rfc_bapi_capabilities(&payload)
                .context("failed to derive SAP ECC RFC/BAPI bridge connector capabilities")?;
            let health = fetch_optional_json_document(
                build_optional_health_url(
                    endpoint,
                    &connector.metadata,
                    capabilities.health_path.as_deref(),
                    "ecc_rfc_health_path",
                    "/bridge/v1/health",
                ),
                &resolved_auth.auth,
                &endpoint.headers,
            )
            .await;

            let mut enriched = connector.clone();
            apply_ecc_rfc_capabilities_metadata(&mut enriched, endpoint, &capabilities)?;
            apply_connector_health_metadata(&mut enriched, "ecc_rfc", health)?;
            apply_auth_resolution_metadata(
                &mut enriched.metadata,
                "auth_",
                &resolved_auth.metadata,
            );
            enriched.updated_at = Utc::now();
            Ok(Some(enriched))
        }
        _ => Ok(None),
    }
}

async fn fetch_connector_events(
    connector: &MigrationConnector,
    run_id: &str,
    request_body: Option<Value>,
    request_headers: HashMap<String, String>,
    secret_store_registry: Option<std::sync::Arc<SecretStoreRegistry>>,
) -> Result<Vec<MigrationEvidenceEvent>> {
    if matches!(connector.transport, ConnectorTransport::SapEccStagedExport) {
        return fetch_sap_ecc_staged_export_events(connector, run_id, request_body).await;
    }
    if matches!(
        connector.transport,
        ConnectorTransport::SapIdocExtractorPackage | ConnectorTransport::SapOdpExtractorPackage
    ) {
        return fetch_sap_idoc_extractor_events(connector, run_id, request_body).await;
    }

    let resolved_auth = resolve_connector_auth(&connector.auth, secret_store_registry).await?;
    let client = reqwest::Client::new();
    let method = Method::from_bytes(connector.endpoint.method.as_bytes())
        .context("invalid connector HTTP method")?;
    let url = format!(
        "{}{}",
        connector.endpoint.base_url.trim_end_matches('/'),
        connector.endpoint.path
    );
    let mut request = client.request(method, &url);
    for (key, value) in &connector.endpoint.headers {
        request = request.header(key, value);
    }
    for (key, value) in request_headers {
        request = request.header(key, value);
    }
    request = apply_auth(request, &resolved_auth.auth);
    if let Some(body) = request_body {
        request = request.json(&body);
    }
    let response = request.send().await?.error_for_status()?;
    let value: Value = response.json().await?;
    let event_values = if let Some(items) = value.get("events").and_then(Value::as_array) {
        items.clone()
    } else if let Some(items) = value.as_array() {
        items.clone()
    } else {
        return Err(anyhow!("connector response must be an array of canonical events or an object with an 'events' array"));
    };

    let mut events = Vec::new();
    for item in event_values {
        let mut event: MigrationEvidenceEvent = serde_json::from_value(item)
            .context("failed to deserialize canonical migration evidence event")?;
        event.run_id = run_id.to_string();
        events.push(event);
    }
    Ok(events)
}

async fn fetch_sap_ecc_staged_export_events(
    connector: &MigrationConnector,
    run_id: &str,
    request_body: Option<Value>,
) -> Result<Vec<MigrationEvidenceEvent>> {
    let (bundle, bundle_dir) = load_sap_ecc_staged_export_bundle(connector, request_body).await?;
    build_sap_ecc_staged_export_events(connector, run_id, bundle, bundle_dir.as_deref()).await
}

async fn load_sap_ecc_staged_export_bundle(
    connector: &MigrationConnector,
    request_body: Option<Value>,
) -> Result<(SapEccStagedExportBundle, Option<PathBuf>)> {
    if let Some(body) = request_body {
        let bundle: SapEccStagedExportBundle = serde_json::from_value(body)
            .context("failed to deserialize SAP ECC staged export bundle")?;
        return Ok((bundle, None));
    }

    let manifest_path = resolve_staged_export_manifest_path(&connector.endpoint)?;
    let manifest_bytes = tokio::fs::read(&manifest_path).await.with_context(|| {
        format!(
            "failed to read staged export manifest '{}'",
            manifest_path.display()
        )
    })?;
    let bundle: SapEccStagedExportBundle =
        serde_json::from_slice(&manifest_bytes).with_context(|| {
            format!(
                "failed to parse staged export manifest '{}'",
                manifest_path.display()
            )
        })?;
    let bundle_dir = manifest_path.parent().map(Path::to_path_buf);
    Ok((bundle, bundle_dir))
}

fn resolve_staged_export_manifest_path(
    endpoint: &graphica_core::migration_evidence::ConnectorEndpoint,
) -> Result<PathBuf> {
    if endpoint.path.is_empty() {
        return Err(anyhow!(
            "sap_ecc_staged_export connectors require either inline request_body or endpoint.path"
        ));
    }

    if endpoint.base_url.is_empty() {
        return Ok(PathBuf::from(&endpoint.path));
    }

    if let Some(base) = endpoint.base_url.strip_prefix("file://") {
        return Ok(PathBuf::from(base).join(endpoint.path.trim_start_matches('/')));
    }

    Err(anyhow!(
        "sap_ecc_staged_export only supports inline request bodies or file:// endpoints"
    ))
}

async fn build_sap_ecc_staged_export_events(
    connector: &MigrationConnector,
    run_id: &str,
    bundle: SapEccStagedExportBundle,
    bundle_dir: Option<&Path>,
) -> Result<Vec<MigrationEvidenceEvent>> {
    validate_staged_export_manifest(&bundle.manifest)?;
    let data_set = bundle
        .manifest
        .data_set
        .clone()
        .ok_or_else(|| anyhow!("sap_ecc_staged_export manifest requires data_set"))?;

    let data_assessment = load_staged_export_data_set(&data_set, bundle_dir).await?;

    let program = bundle
        .program
        .unwrap_or_else(|| default_staged_export_program(&bundle.manifest));
    let object = bundle
        .object
        .unwrap_or_else(|| default_staged_export_object(&bundle.manifest));

    let mut events = vec![
        MigrationEvidenceEvent::new(
            &connector.connector_id,
            run_id,
            connector.vendor.clone(),
            program.program_id.clone(),
            object.object_id.clone(),
            MigrationEvidenceArtifactType::Program,
            None,
            serde_json::to_value(&program)?,
        ),
        MigrationEvidenceEvent::new(
            &connector.connector_id,
            run_id,
            connector.vendor.clone(),
            object.program_id.clone(),
            object.object_id.clone(),
            MigrationEvidenceArtifactType::Object,
            None,
            serde_json::to_value(&object)?,
        ),
    ];

    let mut executions = bundle.executions;
    if executions.is_empty() {
        executions.push(SapEccStagedExecutionEvidence {
            value_key: None,
            execution: default_staged_export_execution(
                &bundle.manifest,
                run_id,
                connector,
                &data_assessment,
            ),
        });
    }

    let mut controls = bundle.controls;
    controls.insert(
        0,
        SapEccStagedControlEvidence {
            value_key: None,
            control: integrity_control_from_manifest(&bundle.manifest, &object, &data_assessment),
        },
    );

    push_staged_rule_events(
        &mut events,
        connector,
        run_id,
        &object,
        bundle.transformation_rules,
    )?;
    push_staged_execution_events(&mut events, connector, run_id, &object, executions)?;
    push_staged_exception_events(&mut events, connector, run_id, &object, bundle.exceptions)?;
    push_staged_control_events(&mut events, connector, run_id, &object, controls)?;
    push_staged_approval_events(&mut events, connector, run_id, &object, bundle.approvals)?;

    Ok(events)
}

fn validate_staged_export_manifest(manifest: &SapEccStagedExportManifest) -> Result<()> {
    if manifest.schema_version.is_empty() {
        return Err(anyhow!(
            "sap_ecc_staged_export manifest requires schema_version"
        ));
    }
    if manifest.export_id.is_empty() {
        return Err(anyhow!("sap_ecc_staged_export manifest requires export_id"));
    }
    if manifest.program_id.is_empty()
        || manifest.object_id.is_empty()
        || manifest.object_name.is_empty()
    {
        return Err(anyhow!(
            "sap_ecc_staged_export manifest requires program_id, object_id, and object_name"
        ));
    }
    if manifest.source_system_id.is_empty() || manifest.source_client.is_empty() {
        return Err(anyhow!(
            "sap_ecc_staged_export manifest requires source_system_id and source_client"
        ));
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct StagedExportDataAssessment {
    row_count: u64,
    sha256: String,
    source_ref: String,
    format: SapEccStagedExportDataFormat,
}

async fn load_staged_export_data_set(
    data_set: &SapEccStagedExportDataSet,
    bundle_dir: Option<&Path>,
) -> Result<StagedExportDataAssessment> {
    let (bytes, source_ref) = load_staged_export_data_bytes(data_set, bundle_dir).await?;
    let row_count = count_staged_export_rows(data_set, &bytes)?;
    if let Some(expected) = data_set.expected_row_count {
        if row_count != expected {
            return Err(anyhow!(
                "sap_ecc_staged_export row count mismatch: expected {}, found {}",
                expected,
                row_count
            ));
        }
    }

    let sha256 = sha256_hex(&bytes);
    if let Some(expected_sha) = data_set.sha256.as_deref() {
        if !expected_sha.eq_ignore_ascii_case(&sha256) {
            return Err(anyhow!(
                "sap_ecc_staged_export checksum mismatch for {}",
                source_ref
            ));
        }
    }

    Ok(StagedExportDataAssessment {
        row_count,
        sha256,
        source_ref,
        format: data_set.format.clone(),
    })
}

async fn load_staged_export_data_bytes(
    data_set: &SapEccStagedExportDataSet,
    bundle_dir: Option<&Path>,
) -> Result<(Vec<u8>, String)> {
    if let Some(inline) = &data_set.inline_payload {
        return Ok((
            serialize_inline_data_set(data_set, inline)?,
            "inline_payload".to_string(),
        ));
    }

    let Some(path) = data_set.path.as_deref() else {
        return Err(anyhow!(
            "sap_ecc_staged_export data_set requires either inline_payload or path"
        ));
    };
    let resolved = resolve_relative_bundle_path(bundle_dir, path);
    let bytes = tokio::fs::read(&resolved).await.with_context(|| {
        format!(
            "failed to read staged export data file '{}'",
            resolved.display()
        )
    })?;
    Ok((bytes, resolved.display().to_string()))
}

fn serialize_inline_data_set(
    data_set: &SapEccStagedExportDataSet,
    payload: &Value,
) -> Result<Vec<u8>> {
    match data_set.format {
        SapEccStagedExportDataFormat::JsonRows => serde_json::to_vec(payload)
            .context("failed to serialize staged export inline JSON payload"),
        SapEccStagedExportDataFormat::Csv | SapEccStagedExportDataFormat::Tsv => payload
            .as_str()
            .map(|text| text.as_bytes().to_vec())
            .ok_or_else(|| anyhow!("inline CSV/TSV staged export payload must be a string")),
    }
}

fn resolve_relative_bundle_path(bundle_dir: Option<&Path>, path: &str) -> PathBuf {
    let candidate = PathBuf::from(path);
    if candidate.is_absolute() {
        candidate
    } else if let Some(root) = bundle_dir {
        root.join(candidate)
    } else {
        candidate
    }
}

fn count_staged_export_rows(data_set: &SapEccStagedExportDataSet, bytes: &[u8]) -> Result<u64> {
    match data_set.format {
        SapEccStagedExportDataFormat::JsonRows => {
            let payload: Value = serde_json::from_slice(bytes)
                .context("failed to parse staged export JSON payload")?;
            match payload {
                Value::Array(rows) => Ok(rows.len() as u64),
                Value::Object(mut object) => object
                    .remove("rows")
                    .and_then(|rows| rows.as_array().map(|items| items.len() as u64))
                    .ok_or_else(|| {
                        anyhow!("staged export JSON payload must be an array or object with 'rows'")
                    }),
                _ => Err(anyhow!(
                    "staged export JSON payload must be an array or object with 'rows'"
                )),
            }
        }
        SapEccStagedExportDataFormat::Csv => count_delimited_rows(bytes, b','),
        SapEccStagedExportDataFormat::Tsv => count_delimited_rows(bytes, b'\t'),
    }
}

fn count_delimited_rows(bytes: &[u8], delimiter: u8) -> Result<u64> {
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(true)
        .from_reader(bytes);
    let mut count = 0u64;
    for record in reader.records() {
        record.context("failed to parse staged export delimited data row")?;
        count += 1;
    }
    Ok(count)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        output.push_str(&format!("{:02x}", byte));
    }
    output
}

fn default_staged_export_program(manifest: &SapEccStagedExportManifest) -> MigrationProgram {
    let mut metadata = manifest.metadata.clone();
    metadata.insert(
        "source_system_id".to_string(),
        manifest.source_system_id.clone(),
    );
    metadata.insert("source_client".to_string(), manifest.source_client.clone());
    metadata.insert("export_id".to_string(), manifest.export_id.clone());
    MigrationProgram {
        program_id: manifest.program_id.clone(),
        name: format!("ECC Export {}", manifest.program_id),
        customer_name: None,
        source_landscape: Some(format!("SAP ECC {}", manifest.source_system_id)),
        target_landscape: None,
        tags: vec![
            "sap".to_string(),
            "ecc".to_string(),
            "staged_export".to_string(),
        ],
        metadata,
        created_at: manifest.extracted_at,
        updated_at: manifest.extracted_at,
    }
}

fn default_staged_export_object(manifest: &SapEccStagedExportManifest) -> MigrationObject {
    let mut metadata = manifest.metadata.clone();
    metadata.insert(
        "source_system_id".to_string(),
        manifest.source_system_id.clone(),
    );
    metadata.insert("source_client".to_string(), manifest.source_client.clone());
    metadata.insert("export_id".to_string(), manifest.export_id.clone());
    MigrationObject {
        object_id: manifest.object_id.clone(),
        program_id: manifest.program_id.clone(),
        object_type: MigrationObjectType::BusinessObject,
        name: manifest.object_name.clone(),
        description: Some(format!(
            "ECC staged export for {} from client {}",
            manifest.object_name, manifest.source_client
        )),
        source_record_id: None,
        target_record_id: None,
        tags: vec![
            "sap".to_string(),
            "ecc".to_string(),
            "staged_export".to_string(),
        ],
        metadata,
    }
}

fn default_staged_export_execution(
    manifest: &SapEccStagedExportManifest,
    run_id: &str,
    connector: &MigrationConnector,
    assessment: &StagedExportDataAssessment,
) -> ExecutionEvent {
    let mut metadata = manifest.metadata.clone();
    metadata.insert(
        "source_system_id".to_string(),
        manifest.source_system_id.clone(),
    );
    metadata.insert("source_client".to_string(), manifest.source_client.clone());
    metadata.insert("export_id".to_string(), manifest.export_id.clone());
    metadata.insert(
        "connector_transport".to_string(),
        "sap_ecc_staged_export".to_string(),
    );
    metadata.insert("data_source_ref".to_string(), assessment.source_ref.clone());
    metadata.insert("data_sha256".to_string(), assessment.sha256.clone());
    metadata.insert(
        "data_format".to_string(),
        match assessment.format {
            SapEccStagedExportDataFormat::JsonRows => "json_rows",
            SapEccStagedExportDataFormat::Csv => "csv",
            SapEccStagedExportDataFormat::Tsv => "tsv",
        }
        .to_string(),
    );
    ExecutionEvent {
        execution_id: format!("ecc-export-{}", manifest.export_id),
        program_id: manifest.program_id.clone(),
        object_id: manifest.object_id.clone(),
        connector_run_id: run_id.to_string(),
        tool_name: "sap_ecc_staged_export".to_string(),
        tool_run_id: connector.connector_id.clone(),
        stage: "ecc_staged_export_ingest".to_string(),
        status: ExecutionStatus::Succeeded,
        happened_at: manifest.extracted_at,
        source_snapshot_ref: Some(format!("sha256:{}", assessment.sha256)),
        target_snapshot_ref: None,
        records_examined: Some(assessment.row_count),
        records_affected: Some(assessment.row_count),
        metadata,
    }
}

fn integrity_control_from_manifest(
    manifest: &SapEccStagedExportManifest,
    object: &MigrationObject,
    assessment: &StagedExportDataAssessment,
) -> ControlResult {
    let mut metadata = manifest.metadata.clone();
    metadata.insert(
        "source_system_id".to_string(),
        manifest.source_system_id.clone(),
    );
    metadata.insert("source_client".to_string(), manifest.source_client.clone());
    metadata.insert("export_id".to_string(), manifest.export_id.clone());
    metadata.insert("data_source_ref".to_string(), assessment.source_ref.clone());
    metadata.insert("data_sha256".to_string(), assessment.sha256.clone());
    metadata.insert(
        "actual_row_count".to_string(),
        assessment.row_count.to_string(),
    );
    metadata.insert("checksum_verified".to_string(), "true".to_string());
    metadata.insert(
        "data_format".to_string(),
        match assessment.format {
            SapEccStagedExportDataFormat::JsonRows => "json_rows",
            SapEccStagedExportDataFormat::Csv => "csv",
            SapEccStagedExportDataFormat::Tsv => "tsv",
        }
        .to_string(),
    );
    if let Some(expected) = manifest
        .data_set
        .as_ref()
        .and_then(|set| set.expected_row_count)
    {
        metadata.insert("expected_row_count".to_string(), expected.to_string());
    }

    ControlResult {
        control_id: format!("control-{}-integrity", manifest.export_id),
        program_id: manifest.program_id.clone(),
        object_id: object.object_id.clone(),
        control_name: "sap_ecc_staged_export_integrity".to_string(),
        control_type: "staged_export_integrity".to_string(),
        status: graphica_core::migration_evidence::ControlStatus::Passed,
        summary: format!(
            "Verified staged export integrity for {} rows from {} client {}",
            assessment.row_count, manifest.source_system_id, manifest.source_client
        ),
        expected_value: manifest
            .data_set
            .as_ref()
            .and_then(|set| set.expected_row_count)
            .map(Value::from),
        actual_value: Some(Value::from(assessment.row_count)),
        tolerance: None,
        executed_at: manifest.extracted_at,
        evidence_refs: vec![format!("sha256:{}", assessment.sha256)],
        metadata,
    }
}

fn push_staged_rule_events(
    events: &mut Vec<MigrationEvidenceEvent>,
    connector: &MigrationConnector,
    run_id: &str,
    object: &MigrationObject,
    rules: Vec<SapEccStagedRuleEvidence>,
) -> Result<()> {
    for evidence in rules {
        push_event(
            events,
            connector,
            run_id,
            &object.program_id,
            &object.object_id,
            MigrationEvidenceArtifactType::TransformationRule,
            evidence.value_key,
            serde_json::to_value(evidence.rule)?,
        );
    }
    Ok(())
}

fn push_staged_execution_events(
    events: &mut Vec<MigrationEvidenceEvent>,
    connector: &MigrationConnector,
    run_id: &str,
    object: &MigrationObject,
    executions: Vec<SapEccStagedExecutionEvidence>,
) -> Result<()> {
    for evidence in executions {
        push_event(
            events,
            connector,
            run_id,
            &object.program_id,
            &object.object_id,
            MigrationEvidenceArtifactType::ExecutionEvent,
            evidence.value_key,
            serde_json::to_value(evidence.execution)?,
        );
    }
    Ok(())
}

fn push_staged_exception_events(
    events: &mut Vec<MigrationEvidenceEvent>,
    connector: &MigrationConnector,
    run_id: &str,
    object: &MigrationObject,
    exceptions: Vec<SapEccStagedExceptionEvidence>,
) -> Result<()> {
    for evidence in exceptions {
        push_event(
            events,
            connector,
            run_id,
            &object.program_id,
            &object.object_id,
            MigrationEvidenceArtifactType::ExceptionRecord,
            evidence.value_key,
            serde_json::to_value(evidence.exception)?,
        );
    }
    Ok(())
}

fn push_staged_control_events(
    events: &mut Vec<MigrationEvidenceEvent>,
    connector: &MigrationConnector,
    run_id: &str,
    object: &MigrationObject,
    controls: Vec<SapEccStagedControlEvidence>,
) -> Result<()> {
    for evidence in controls {
        push_event(
            events,
            connector,
            run_id,
            &object.program_id,
            &object.object_id,
            MigrationEvidenceArtifactType::ControlResult,
            evidence.value_key,
            serde_json::to_value(evidence.control)?,
        );
    }
    Ok(())
}

fn push_staged_approval_events(
    events: &mut Vec<MigrationEvidenceEvent>,
    connector: &MigrationConnector,
    run_id: &str,
    object: &MigrationObject,
    approvals: Vec<SapEccStagedApprovalEvidence>,
) -> Result<()> {
    for evidence in approvals {
        push_event(
            events,
            connector,
            run_id,
            &object.program_id,
            &object.object_id,
            MigrationEvidenceArtifactType::ApprovalEvent,
            evidence.value_key,
            serde_json::to_value(evidence.approval)?,
        );
    }
    Ok(())
}

fn push_event(
    events: &mut Vec<MigrationEvidenceEvent>,
    connector: &MigrationConnector,
    run_id: &str,
    program_id: &str,
    object_id: &str,
    artifact_type: MigrationEvidenceArtifactType,
    value_key: Option<String>,
    payload: Value,
) {
    events.push(MigrationEvidenceEvent::new(
        &connector.connector_id,
        run_id,
        connector.vendor.clone(),
        program_id,
        object_id,
        artifact_type,
        value_key,
        payload,
    ));
}

async fn fetch_sap_idoc_extractor_events(
    connector: &MigrationConnector,
    run_id: &str,
    request_body: Option<Value>,
) -> Result<Vec<MigrationEvidenceEvent>> {
    let (bundle, bundle_dir) = load_sap_idoc_extractor_bundle(connector, request_body).await?;
    build_sap_idoc_extractor_events(connector, run_id, bundle, bundle_dir.as_deref()).await
}

fn effective_extractor_family(
    connector: &MigrationConnector,
    manifest: &SapIdocExtractorManifest,
) -> SapExtractorFamily {
    match connector.transport {
        ConnectorTransport::SapOdpExtractorPackage => SapExtractorFamily::Odp,
        ConnectorTransport::SapIdocExtractorPackage => manifest.extractor_family.clone(),
        _ => manifest.extractor_family.clone(),
    }
}

fn extractor_transport_name(
    connector: &MigrationConnector,
    family: &SapExtractorFamily,
) -> &'static str {
    match connector.transport {
        ConnectorTransport::SapOdpExtractorPackage => "sap_odp_extractor_package",
        ConnectorTransport::SapIdocExtractorPackage => match family {
            SapExtractorFamily::Odp => "sap_odp_extractor_package",
            _ => "sap_idoc_extractor_package",
        },
        _ => match family {
            SapExtractorFamily::Odp => "sap_odp_extractor_package",
            _ => "sap_idoc_extractor_package",
        },
    }
}

fn extractor_stage_name(family: &SapExtractorFamily) -> &'static str {
    match family {
        SapExtractorFamily::Idoc => "idoc_extractor_ingest",
        SapExtractorFamily::Odp => "odp_extractor_ingest",
        SapExtractorFamily::Generic => "extractor_package_ingest",
    }
}

fn extractor_integrity_control_name(family: &SapExtractorFamily) -> &'static str {
    match family {
        SapExtractorFamily::Idoc => "sap_idoc_extractor_integrity",
        SapExtractorFamily::Odp => "sap_odp_extractor_integrity",
        SapExtractorFamily::Generic => "sap_extractor_package_integrity",
    }
}

fn extractor_integrity_control_type(family: &SapExtractorFamily) -> &'static str {
    match family {
        SapExtractorFamily::Idoc => "idoc_extractor_integrity",
        SapExtractorFamily::Odp => "odp_extractor_integrity",
        SapExtractorFamily::Generic => "extractor_package_integrity",
    }
}

fn extractor_family_tag(family: &SapExtractorFamily) -> &'static str {
    match family {
        SapExtractorFamily::Idoc => "idoc_extractor",
        SapExtractorFamily::Odp => "odp_extractor",
        SapExtractorFamily::Generic => "extractor_package",
    }
}

fn extractor_family_label(family: &SapExtractorFamily) -> &'static str {
    match family {
        SapExtractorFamily::Idoc => "IDoc / extractor",
        SapExtractorFamily::Odp => "ODP extractor",
        SapExtractorFamily::Generic => "extractor",
    }
}

async fn load_sap_idoc_extractor_bundle(
    connector: &MigrationConnector,
    request_body: Option<Value>,
) -> Result<(SapIdocExtractorBundle, Option<PathBuf>)> {
    if let Some(body) = request_body {
        let bundle: SapIdocExtractorBundle = serde_json::from_value(body)
            .context("failed to deserialize SAP extractor package bundle")?;
        return Ok((bundle, None));
    }

    let manifest_path = resolve_idoc_extractor_manifest_path(&connector.endpoint)?;
    let manifest_bytes = tokio::fs::read(&manifest_path).await.with_context(|| {
        format!(
            "failed to read SAP extractor manifest '{}'",
            manifest_path.display()
        )
    })?;
    let bundle: SapIdocExtractorBundle =
        serde_json::from_slice(&manifest_bytes).with_context(|| {
            format!(
                "failed to parse SAP extractor manifest '{}'",
                manifest_path.display()
            )
        })?;
    let bundle_dir = manifest_path.parent().map(Path::to_path_buf);
    Ok((bundle, bundle_dir))
}

fn resolve_idoc_extractor_manifest_path(
    endpoint: &graphica_core::migration_evidence::ConnectorEndpoint,
) -> Result<PathBuf> {
    if endpoint.path.is_empty() {
        return Err(anyhow!(
            "SAP extractor package connectors require either inline request_body or endpoint.path"
        ));
    }

    if endpoint.base_url.is_empty() {
        return Ok(PathBuf::from(&endpoint.path));
    }

    if let Some(base) = endpoint.base_url.strip_prefix("file://") {
        return Ok(PathBuf::from(base).join(endpoint.path.trim_start_matches('/')));
    }

    Err(anyhow!(
        "SAP extractor package transports only support inline request bodies or file:// endpoints"
    ))
}

async fn build_sap_idoc_extractor_events(
    connector: &MigrationConnector,
    run_id: &str,
    bundle: SapIdocExtractorBundle,
    bundle_dir: Option<&Path>,
) -> Result<Vec<MigrationEvidenceEvent>> {
    let family = effective_extractor_family(connector, &bundle.manifest);
    validate_idoc_extractor_manifest(&bundle.manifest, &connector.transport, &family)?;
    let data_set = bundle
        .manifest
        .data_set
        .clone()
        .ok_or_else(|| anyhow!("SAP extractor package manifest requires data_set"))?;

    let data_assessment = load_idoc_extractor_data_set(&data_set, bundle_dir).await?;

    let program = default_idoc_extractor_program(&bundle.manifest, &family);
    let object = default_idoc_extractor_object(&bundle.manifest, &family);

    let mut events = vec![
        MigrationEvidenceEvent::new(
            &connector.connector_id,
            run_id,
            connector.vendor.clone(),
            program.program_id.clone(),
            object.object_id.clone(),
            MigrationEvidenceArtifactType::Program,
            None,
            serde_json::to_value(&program)?,
        ),
        MigrationEvidenceEvent::new(
            &connector.connector_id,
            run_id,
            connector.vendor.clone(),
            object.program_id.clone(),
            object.object_id.clone(),
            MigrationEvidenceArtifactType::Object,
            None,
            serde_json::to_value(&object)?,
        ),
    ];

    let mut executions = bundle.executions;
    if executions.is_empty() {
        executions.push(SapIdocExtractorExecutionEvidence {
            value_key: None,
            execution: default_idoc_extractor_execution(
                &bundle.manifest,
                run_id,
                connector,
                &data_assessment,
                &family,
            ),
        });
    }

    let mut controls = bundle.controls;
    controls.insert(
        0,
        SapIdocExtractorControlEvidence {
            value_key: None,
            control: idoc_integrity_control_from_manifest(
                &bundle.manifest,
                &object,
                &data_assessment,
                &family,
            ),
        },
    );

    push_idoc_execution_events(&mut events, connector, run_id, &object, executions)?;
    push_idoc_exception_events(&mut events, connector, run_id, &object, bundle.exceptions)?;
    push_idoc_control_events(&mut events, connector, run_id, &object, controls)?;
    push_idoc_approval_events(&mut events, connector, run_id, &object, bundle.approvals)?;

    Ok(events)
}

fn validate_idoc_extractor_manifest(
    manifest: &SapIdocExtractorManifest,
    transport: &ConnectorTransport,
    family: &SapExtractorFamily,
) -> Result<()> {
    if manifest.schema_version.is_empty() {
        return Err(anyhow!(
            "SAP extractor package manifest requires schema_version"
        ));
    }
    if manifest.package_id.is_empty() {
        return Err(anyhow!(
            "SAP extractor package manifest requires package_id"
        ));
    }
    if manifest.program_id.is_empty()
        || manifest.object_id.is_empty()
        || manifest.object_name.is_empty()
    {
        return Err(anyhow!(
            "SAP extractor package manifest requires program_id, object_id, and object_name"
        ));
    }
    if manifest.source_system_id.is_empty() || manifest.source_client.is_empty() {
        return Err(anyhow!(
            "SAP extractor package manifest requires source_system_id and source_client"
        ));
    }
    if manifest.extractor_name.is_empty() || manifest.extractor_run_id.is_empty() {
        return Err(anyhow!(
            "SAP extractor package manifest requires extractor_name and extractor_run_id"
        ));
    }
    if matches!(transport, ConnectorTransport::SapOdpExtractorPackage)
        && !matches!(family, SapExtractorFamily::Odp)
    {
        return Err(anyhow!(
            "sap_odp_extractor_package manifests must declare extractor_family='odp'"
        ));
    }
    if matches!(family, SapExtractorFamily::Odp)
        && (manifest
            .extractor_context
            .as_deref()
            .unwrap_or_default()
            .is_empty()
            || manifest
                .extractor_object
                .as_deref()
                .unwrap_or_default()
                .is_empty())
    {
        return Err(anyhow!(
            "ODP extractor packages require extractor_context and extractor_object"
        ));
    }
    match family {
        SapExtractorFamily::Idoc => {
            if manifest.idoc_type.as_deref().unwrap_or_default().is_empty()
                && manifest
                    .message_type
                    .as_deref()
                    .unwrap_or_default()
                    .is_empty()
            {
                return Err(anyhow!(
                    "IDoc extractor packages require idoc_type or message_type"
                ));
            }
        }
        SapExtractorFamily::Odp => {
            let has_subscriber = manifest
                .subscriber_name
                .as_deref()
                .map(|value| !value.trim().is_empty())
                .unwrap_or(false);
            let has_queue = manifest
                .queue_name
                .as_deref()
                .map(|value| !value.trim().is_empty())
                .unwrap_or(false);
            if has_subscriber != has_queue {
                return Err(anyhow!(
                    "ODP extractor packages require subscriber_name and queue_name together"
                ));
            }
            if matches!(
                manifest.extraction_mode,
                Some(graphica_core::migration_evidence::SapExtractorMode::Delta)
            ) && manifest
                .delta_token
                .as_deref()
                .unwrap_or_default()
                .is_empty()
                && !(has_subscriber && has_queue)
            {
                return Err(anyhow!(
                    "ODP delta extractor packages require delta_token or subscriber/queue context"
                ));
            }
        }
        SapExtractorFamily::Generic => {
            if manifest
                .extractor_object
                .as_deref()
                .unwrap_or_default()
                .is_empty()
                && manifest
                    .extractor_context
                    .as_deref()
                    .unwrap_or_default()
                    .is_empty()
            {
                return Err(anyhow!(
                    "generic extractor packages require extractor_object or extractor_context"
                ));
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct IdocExtractorDataAssessment {
    row_count: u64,
    sha256: String,
    source_ref: String,
    format: SapIdocExtractorDataFormat,
}

async fn load_idoc_extractor_data_set(
    data_set: &SapIdocExtractorDataSet,
    bundle_dir: Option<&Path>,
) -> Result<IdocExtractorDataAssessment> {
    let (bytes, source_ref) = load_idoc_extractor_data_bytes(data_set, bundle_dir).await?;
    let row_count = count_idoc_extractor_rows(data_set, &bytes)?;
    if let Some(expected) = data_set.expected_row_count {
        if row_count != expected {
            return Err(anyhow!(
                "SAP extractor package row count mismatch: expected {}, found {}",
                expected,
                row_count
            ));
        }
    }

    let sha256 = sha256_hex(&bytes);
    if let Some(expected_sha) = data_set.sha256.as_deref() {
        if !expected_sha.eq_ignore_ascii_case(&sha256) {
            return Err(anyhow!(
                "SAP extractor package checksum mismatch for {}",
                source_ref
            ));
        }
    }

    Ok(IdocExtractorDataAssessment {
        row_count,
        sha256,
        source_ref,
        format: data_set.format.clone(),
    })
}

async fn load_idoc_extractor_data_bytes(
    data_set: &SapIdocExtractorDataSet,
    bundle_dir: Option<&Path>,
) -> Result<(Vec<u8>, String)> {
    if let Some(inline) = &data_set.inline_payload {
        return Ok((
            serialize_inline_idoc_data_set(data_set, inline)?,
            "inline_payload".to_string(),
        ));
    }

    let Some(path) = data_set.path.as_deref() else {
        return Err(anyhow!(
            "SAP extractor package data_set requires either inline_payload or path"
        ));
    };
    let resolved = resolve_relative_bundle_path(bundle_dir, path);
    let bytes = tokio::fs::read(&resolved).await.with_context(|| {
        format!(
            "failed to read IDoc extractor data file '{}'",
            resolved.display()
        )
    })?;
    Ok((bytes, resolved.display().to_string()))
}

fn serialize_inline_idoc_data_set(
    data_set: &SapIdocExtractorDataSet,
    payload: &str,
) -> Result<Vec<u8>> {
    match data_set.format {
        SapIdocExtractorDataFormat::JsonDocuments => Ok(payload.as_bytes().to_vec()),
        SapIdocExtractorDataFormat::Csv | SapIdocExtractorDataFormat::Tsv => {
            Ok(payload.as_bytes().to_vec())
        }
    }
}

fn count_idoc_extractor_rows(data_set: &SapIdocExtractorDataSet, bytes: &[u8]) -> Result<u64> {
    match data_set.format {
        SapIdocExtractorDataFormat::JsonDocuments => {
            let payload: Value = serde_json::from_slice(bytes)
                .context("failed to parse IDoc extractor JSON payload")?;
            match payload {
                Value::Array(rows) => Ok(rows.len() as u64),
                Value::Object(mut object) => object
                    .remove("documents")
                    .or_else(|| object.remove("rows"))
                    .and_then(|rows| rows.as_array().map(|items| items.len() as u64))
                    .ok_or_else(|| anyhow!("IDoc extractor JSON payload must be an array or object with 'documents'")),
                _ => Err(anyhow!(
                    "IDoc extractor JSON payload must be an array or object with 'documents'"
                )),
            }
        }
        SapIdocExtractorDataFormat::Csv => count_delimited_rows(bytes, b','),
        SapIdocExtractorDataFormat::Tsv => count_delimited_rows(bytes, b'\t'),
    }
}

fn default_idoc_extractor_program(
    manifest: &SapIdocExtractorManifest,
    family: &SapExtractorFamily,
) -> MigrationProgram {
    let mut metadata = BTreeMap::new();
    metadata.insert(
        "source_system_id".to_string(),
        manifest.source_system_id.clone(),
    );
    metadata.insert("source_client".to_string(), manifest.source_client.clone());
    metadata.insert("package_id".to_string(), manifest.package_id.clone());
    metadata.insert(
        "extractor_name".to_string(),
        manifest.extractor_name.clone(),
    );
    metadata.insert(
        "extractor_family".to_string(),
        serde_json::to_value(family)
            .ok()
            .and_then(|value| value.as_str().map(str::to_string))
            .unwrap_or_else(|| "generic".to_string()),
    );
    if let Some(extractor_object) = manifest.extractor_object.as_deref() {
        metadata.insert("extractor_object".to_string(), extractor_object.to_string());
    }
    if let Some(extractor_context) = manifest.extractor_context.as_deref() {
        metadata.insert(
            "extractor_context".to_string(),
            extractor_context.to_string(),
        );
    }
    if let Some(extraction_mode) = manifest.extraction_mode.as_ref() {
        metadata.insert(
            "extraction_mode".to_string(),
            serde_json::to_value(extraction_mode)
                .ok()
                .and_then(|value| value.as_str().map(str::to_string))
                .unwrap_or_else(|| "full".to_string()),
        );
    }
    if let Some(delta_token) = manifest.delta_token.as_deref() {
        metadata.insert("delta_token".to_string(), delta_token.to_string());
    }
    if let Some(subscriber_name) = manifest.subscriber_name.as_deref() {
        metadata.insert("subscriber_name".to_string(), subscriber_name.to_string());
    }
    if let Some(queue_name) = manifest.queue_name.as_deref() {
        metadata.insert("queue_name".to_string(), queue_name.to_string());
    }
    if let Some(idoc_type) = manifest.idoc_type.as_deref() {
        metadata.insert("idoc_type".to_string(), idoc_type.to_string());
    }
    if let Some(message_type) = manifest.message_type.as_deref() {
        metadata.insert("message_type".to_string(), message_type.to_string());
    }
    MigrationProgram {
        program_id: manifest.program_id.clone(),
        name: format!("{} {}", extractor_family_label(family), manifest.program_id),
        customer_name: None,
        source_landscape: Some(format!("SAP ECC {}", manifest.source_system_id)),
        target_landscape: None,
        tags: vec![
            "sap".to_string(),
            "ecc".to_string(),
            extractor_family_tag(family).to_string(),
        ],
        metadata: metadata.into_iter().collect(),
        created_at: manifest.extracted_at,
        updated_at: manifest.extracted_at,
    }
}

fn default_idoc_extractor_object(
    manifest: &SapIdocExtractorManifest,
    family: &SapExtractorFamily,
) -> MigrationObject {
    let mut metadata = BTreeMap::new();
    metadata.insert(
        "source_system_id".to_string(),
        manifest.source_system_id.clone(),
    );
    metadata.insert("source_client".to_string(), manifest.source_client.clone());
    metadata.insert("package_id".to_string(), manifest.package_id.clone());
    metadata.insert(
        "extractor_name".to_string(),
        manifest.extractor_name.clone(),
    );
    metadata.insert(
        "extractor_family".to_string(),
        serde_json::to_value(family)
            .ok()
            .and_then(|value| value.as_str().map(str::to_string))
            .unwrap_or_else(|| "generic".to_string()),
    );
    metadata.insert(
        "segment_counts_json".to_string(),
        serde_json::to_string(&manifest.segment_counts).unwrap_or_else(|_| "{}".to_string()),
    );
    if let Some(extractor_object) = manifest.extractor_object.as_deref() {
        metadata.insert("extractor_object".to_string(), extractor_object.to_string());
    }
    if let Some(extractor_context) = manifest.extractor_context.as_deref() {
        metadata.insert(
            "extractor_context".to_string(),
            extractor_context.to_string(),
        );
    }
    if let Some(subscriber_name) = manifest.subscriber_name.as_deref() {
        metadata.insert("subscriber_name".to_string(), subscriber_name.to_string());
    }
    if let Some(queue_name) = manifest.queue_name.as_deref() {
        metadata.insert("queue_name".to_string(), queue_name.to_string());
    }
    if let Some(idoc_type) = manifest.idoc_type.as_deref() {
        metadata.insert("idoc_type".to_string(), idoc_type.to_string());
    }
    if let Some(message_type) = manifest.message_type.as_deref() {
        metadata.insert("message_type".to_string(), message_type.to_string());
    }
    MigrationObject {
        object_id: manifest.object_id.clone(),
        program_id: manifest.program_id.clone(),
        object_type: MigrationObjectType::Interface,
        name: manifest.object_name.clone(),
        description: Some(format!(
            "SAP {} package for {} from client {}",
            extractor_family_label(family),
            manifest.object_name,
            manifest.source_client
        )),
        source_record_id: None,
        target_record_id: None,
        tags: vec![
            "sap".to_string(),
            "ecc".to_string(),
            extractor_family_tag(family).to_string(),
        ],
        metadata: metadata.into_iter().collect(),
    }
}

fn default_idoc_extractor_execution(
    manifest: &SapIdocExtractorManifest,
    run_id: &str,
    connector: &MigrationConnector,
    assessment: &IdocExtractorDataAssessment,
    family: &SapExtractorFamily,
) -> ExecutionEvent {
    let mut metadata = HashMap::new();
    metadata.insert(
        "source_system_id".to_string(),
        manifest.source_system_id.clone(),
    );
    metadata.insert("source_client".to_string(), manifest.source_client.clone());
    metadata.insert("package_id".to_string(), manifest.package_id.clone());
    metadata.insert(
        "extractor_name".to_string(),
        manifest.extractor_name.clone(),
    );
    metadata.insert(
        "extractor_run_id".to_string(),
        manifest.extractor_run_id.clone(),
    );
    metadata.insert(
        "connector_transport".to_string(),
        extractor_transport_name(connector, family).to_string(),
    );
    metadata.insert(
        "extractor_family".to_string(),
        serde_json::to_value(family)
            .ok()
            .and_then(|value| value.as_str().map(str::to_string))
            .unwrap_or_else(|| "generic".to_string()),
    );
    metadata.insert("data_source_ref".to_string(), assessment.source_ref.clone());
    metadata.insert("data_sha256".to_string(), assessment.sha256.clone());
    metadata.insert(
        "data_format".to_string(),
        match assessment.format {
            SapIdocExtractorDataFormat::JsonDocuments => "json_documents",
            SapIdocExtractorDataFormat::Csv => "csv",
            SapIdocExtractorDataFormat::Tsv => "tsv",
        }
        .to_string(),
    );
    if let Some(idoc_type) = manifest.idoc_type.as_deref() {
        metadata.insert("idoc_type".to_string(), idoc_type.to_string());
    }
    if let Some(extractor_object) = manifest.extractor_object.as_deref() {
        metadata.insert("extractor_object".to_string(), extractor_object.to_string());
    }
    if let Some(extractor_context) = manifest.extractor_context.as_deref() {
        metadata.insert(
            "extractor_context".to_string(),
            extractor_context.to_string(),
        );
    }
    if let Some(extraction_mode) = manifest.extraction_mode.as_ref() {
        metadata.insert(
            "extraction_mode".to_string(),
            serde_json::to_value(extraction_mode)
                .ok()
                .and_then(|value| value.as_str().map(str::to_string))
                .unwrap_or_else(|| "full".to_string()),
        );
    }
    if let Some(delta_token) = manifest.delta_token.as_deref() {
        metadata.insert("delta_token".to_string(), delta_token.to_string());
    }
    if let Some(subscriber_name) = manifest.subscriber_name.as_deref() {
        metadata.insert("subscriber_name".to_string(), subscriber_name.to_string());
    }
    if let Some(queue_name) = manifest.queue_name.as_deref() {
        metadata.insert("queue_name".to_string(), queue_name.to_string());
    }
    if let Some(message_type) = manifest.message_type.as_deref() {
        metadata.insert("message_type".to_string(), message_type.to_string());
    }
    ExecutionEvent {
        execution_id: format!("extractor-package-{}", manifest.package_id),
        program_id: manifest.program_id.clone(),
        object_id: manifest.object_id.clone(),
        connector_run_id: run_id.to_string(),
        tool_name: extractor_transport_name(connector, family).to_string(),
        tool_run_id: connector.connector_id.clone(),
        stage: extractor_stage_name(family).to_string(),
        status: ExecutionStatus::Succeeded,
        happened_at: manifest.extracted_at,
        source_snapshot_ref: Some(format!("sha256:{}", assessment.sha256)),
        target_snapshot_ref: None,
        records_examined: Some(assessment.row_count),
        records_affected: Some(assessment.row_count),
        metadata,
    }
}

fn idoc_integrity_control_from_manifest(
    manifest: &SapIdocExtractorManifest,
    object: &MigrationObject,
    assessment: &IdocExtractorDataAssessment,
    family: &SapExtractorFamily,
) -> ControlResult {
    let mut metadata = HashMap::new();
    metadata.insert(
        "source_system_id".to_string(),
        manifest.source_system_id.clone(),
    );
    metadata.insert("source_client".to_string(), manifest.source_client.clone());
    metadata.insert("package_id".to_string(), manifest.package_id.clone());
    metadata.insert(
        "extractor_name".to_string(),
        manifest.extractor_name.clone(),
    );
    metadata.insert(
        "extractor_run_id".to_string(),
        manifest.extractor_run_id.clone(),
    );
    metadata.insert(
        "extractor_family".to_string(),
        serde_json::to_value(family)
            .ok()
            .and_then(|value| value.as_str().map(str::to_string))
            .unwrap_or_else(|| "generic".to_string()),
    );
    metadata.insert("data_source_ref".to_string(), assessment.source_ref.clone());
    metadata.insert("data_sha256".to_string(), assessment.sha256.clone());
    metadata.insert(
        "actual_row_count".to_string(),
        assessment.row_count.to_string(),
    );
    metadata.insert("checksum_verified".to_string(), "true".to_string());
    metadata.insert(
        "data_format".to_string(),
        match assessment.format {
            SapIdocExtractorDataFormat::JsonDocuments => "json_documents",
            SapIdocExtractorDataFormat::Csv => "csv",
            SapIdocExtractorDataFormat::Tsv => "tsv",
        }
        .to_string(),
    );
    if let Some(idoc_type) = manifest.idoc_type.as_deref() {
        metadata.insert("idoc_type".to_string(), idoc_type.to_string());
    }
    if let Some(message_type) = manifest.message_type.as_deref() {
        metadata.insert("message_type".to_string(), message_type.to_string());
    }
    if let Some(extractor_object) = manifest.extractor_object.as_deref() {
        metadata.insert("extractor_object".to_string(), extractor_object.to_string());
    }
    if let Some(extractor_context) = manifest.extractor_context.as_deref() {
        metadata.insert(
            "extractor_context".to_string(),
            extractor_context.to_string(),
        );
    }
    if let Some(extraction_mode) = manifest.extraction_mode.as_ref() {
        metadata.insert(
            "extraction_mode".to_string(),
            serde_json::to_value(extraction_mode)
                .ok()
                .and_then(|value| value.as_str().map(str::to_string))
                .unwrap_or_else(|| "full".to_string()),
        );
    }
    if let Some(delta_token) = manifest.delta_token.as_deref() {
        metadata.insert("delta_token".to_string(), delta_token.to_string());
    }
    if let Some(subscriber_name) = manifest.subscriber_name.as_deref() {
        metadata.insert("subscriber_name".to_string(), subscriber_name.to_string());
    }
    if let Some(queue_name) = manifest.queue_name.as_deref() {
        metadata.insert("queue_name".to_string(), queue_name.to_string());
    }
    if let Some(expected) = manifest
        .data_set
        .as_ref()
        .and_then(|set| set.expected_row_count)
    {
        metadata.insert("expected_row_count".to_string(), expected.to_string());
    }

    ControlResult {
        control_id: format!("control-{}-integrity", manifest.package_id),
        program_id: manifest.program_id.clone(),
        object_id: object.object_id.clone(),
        control_name: extractor_integrity_control_name(family).to_string(),
        control_type: extractor_integrity_control_type(family).to_string(),
        status: graphica_core::migration_evidence::ControlStatus::Passed,
        summary: format!(
            "Verified SAP {} package integrity for {} rows from {} client {}",
            extractor_family_label(family),
            assessment.row_count,
            manifest.source_system_id,
            manifest.source_client
        ),
        expected_value: manifest
            .data_set
            .as_ref()
            .and_then(|set| set.expected_row_count)
            .map(Value::from),
        actual_value: Some(Value::from(assessment.row_count)),
        tolerance: None,
        executed_at: manifest.extracted_at,
        evidence_refs: vec![format!("sha256:{}", assessment.sha256)],
        metadata,
    }
}

fn push_idoc_execution_events(
    events: &mut Vec<MigrationEvidenceEvent>,
    connector: &MigrationConnector,
    run_id: &str,
    object: &MigrationObject,
    executions: Vec<SapIdocExtractorExecutionEvidence>,
) -> Result<()> {
    for evidence in executions {
        push_event(
            events,
            connector,
            run_id,
            &object.program_id,
            &object.object_id,
            MigrationEvidenceArtifactType::ExecutionEvent,
            evidence.value_key,
            serde_json::to_value(evidence.execution)?,
        );
    }
    Ok(())
}

fn push_idoc_exception_events(
    events: &mut Vec<MigrationEvidenceEvent>,
    connector: &MigrationConnector,
    run_id: &str,
    object: &MigrationObject,
    exceptions: Vec<SapIdocExtractorExceptionEvidence>,
) -> Result<()> {
    for evidence in exceptions {
        push_event(
            events,
            connector,
            run_id,
            &object.program_id,
            &object.object_id,
            MigrationEvidenceArtifactType::ExceptionRecord,
            evidence.value_key,
            serde_json::to_value(evidence.exception)?,
        );
    }
    Ok(())
}

fn push_idoc_control_events(
    events: &mut Vec<MigrationEvidenceEvent>,
    connector: &MigrationConnector,
    run_id: &str,
    object: &MigrationObject,
    controls: Vec<SapIdocExtractorControlEvidence>,
) -> Result<()> {
    for evidence in controls {
        push_event(
            events,
            connector,
            run_id,
            &object.program_id,
            &object.object_id,
            MigrationEvidenceArtifactType::ControlResult,
            evidence.value_key,
            serde_json::to_value(evidence.control)?,
        );
    }
    Ok(())
}

fn push_idoc_approval_events(
    events: &mut Vec<MigrationEvidenceEvent>,
    connector: &MigrationConnector,
    run_id: &str,
    object: &MigrationObject,
    approvals: Vec<SapIdocExtractorApprovalEvidence>,
) -> Result<()> {
    for evidence in approvals {
        push_event(
            events,
            connector,
            run_id,
            &object.program_id,
            &object.object_id,
            MigrationEvidenceArtifactType::ApprovalEvent,
            evidence.value_key,
            serde_json::to_value(evidence.approval)?,
        );
    }
    Ok(())
}

async fn fetch_ecc_rfc_capabilities_document(
    endpoint: &graphica_core::migration_evidence::ConnectorEndpoint,
    auth: &graphica_core::migration_evidence::ConnectorAuth,
    connector_metadata: &HashMap<String, String>,
) -> Result<Value> {
    let capabilities_path = connector_metadata
        .get("ecc_rfc_capabilities_path")
        .cloned()
        .unwrap_or_else(|| "/bridge/v1/capabilities".to_string());
    let capabilities_url = connector_metadata
        .get("ecc_rfc_capabilities_url")
        .cloned()
        .unwrap_or_else(|| {
            format!(
                "{}{}",
                endpoint.base_url.trim_end_matches('/'),
                capabilities_path
            )
        });
    let client = reqwest::Client::new();
    let mut request = client.get(&capabilities_url);
    request = request.header("accept", "application/json");
    for (key, value) in &endpoint.headers {
        request = request.header(key, value);
    }
    request = apply_auth(request, auth);
    let response = request.send().await?.error_for_status()?;
    response
        .json()
        .await
        .context("failed to read SAP ECC RFC/BAPI bridge capabilities response")
}

fn build_optional_health_url(
    endpoint: &graphica_core::migration_evidence::ConnectorEndpoint,
    connector_metadata: &HashMap<String, String>,
    discovered_health_path: Option<&str>,
    metadata_key: &str,
    default_path: &str,
) -> Option<String> {
    let path = connector_metadata
        .get(metadata_key)
        .cloned()
        .or_else(|| discovered_health_path.map(str::to_string))
        .unwrap_or_else(|| default_path.to_string());
    if path.trim().is_empty() {
        return None;
    }
    Some(format!(
        "{}{}",
        endpoint.base_url.trim_end_matches('/'),
        path
    ))
}

async fn fetch_optional_json_document(
    url: Option<String>,
    auth: &graphica_core::migration_evidence::ConnectorAuth,
    headers: &HashMap<String, String>,
) -> Option<Value> {
    let url = url?;
    let client = reqwest::Client::new();
    let mut request = client.get(&url);
    request = request.header("accept", "application/json");
    for (key, value) in headers {
        request = request.header(key, value);
    }
    request = apply_auth(request, auth);
    let response = request.send().await.ok()?.error_for_status().ok()?;
    response.json().await.ok()
}

fn apply_connector_health_metadata(
    connector: &mut MigrationConnector,
    prefix: &str,
    health: Option<Value>,
) -> Result<()> {
    connector.metadata.insert(
        format!("{prefix}_health_checked_at"),
        Utc::now().to_rfc3339(),
    );
    match health {
        Some(Value::Object(object)) => {
            let status = object
                .get("status")
                .or_else(|| object.get("state"))
                .and_then(Value::as_str)
                .unwrap_or("ready");
            let ready = object
                .get("ready")
                .or_else(|| object.get("healthy"))
                .and_then(Value::as_bool)
                .unwrap_or(status == "ready" || status == "ok");
            connector
                .metadata
                .insert(format!("{prefix}_health_status"), status.to_string());
            connector
                .metadata
                .insert(format!("{prefix}_health_ready"), ready.to_string());
            if let Some(version) = object.get("version").and_then(Value::as_str) {
                connector
                    .metadata
                    .insert(format!("{prefix}_health_version"), version.to_string());
            }
            if let Some(build) = object.get("build").and_then(Value::as_str) {
                connector
                    .metadata
                    .insert(format!("{prefix}_health_build"), build.to_string());
            }
            connector.metadata.insert(
                format!("{prefix}_health_payload_json"),
                serde_json::to_string(&Value::Object(object))?,
            );
        }
        Some(other) => {
            connector.metadata.insert(
                format!("{prefix}_health_status"),
                "unexpected_payload".to_string(),
            );
            connector
                .metadata
                .insert(format!("{prefix}_health_ready"), "false".to_string());
            connector.metadata.insert(
                format!("{prefix}_health_payload_json"),
                serde_json::to_string(&other)?,
            );
        }
        None => {
            connector
                .metadata
                .insert(format!("{prefix}_health_status"), "unavailable".to_string());
            connector
                .metadata
                .insert(format!("{prefix}_health_ready"), "false".to_string());
        }
    }
    Ok(())
}

async fn fetch_s4_metadata_document(
    endpoint: &graphica_core::migration_evidence::ConnectorEndpoint,
    auth: &graphica_core::migration_evidence::ConnectorAuth,
    connector_metadata: &HashMap<String, String>,
    extra_headers: HashMap<String, String>,
) -> Result<String> {
    let metadata_path = connector_metadata
        .get("odata_metadata_path")
        .cloned()
        .unwrap_or_else(|| infer_sap_s4_odata_metadata_path(&endpoint.path));
    let metadata_url = connector_metadata
        .get("odata_metadata_url")
        .cloned()
        .unwrap_or_else(|| {
            format!(
                "{}{}",
                endpoint.base_url.trim_end_matches('/'),
                metadata_path
            )
        });
    let client = reqwest::Client::new();
    let mut request = client.get(&metadata_url);
    request = request.header("accept", "application/xml, text/xml");
    for (key, value) in &endpoint.headers {
        request = request.header(key, value);
    }
    for (key, value) in extra_headers {
        request = request.header(key, value);
    }
    request = apply_auth(request, auth);
    let response = request.send().await?.error_for_status()?;
    response
        .text()
        .await
        .context("failed to read SAP S/4HANA OData metadata document")
}

async fn fetch_ecc_capabilities_document(
    endpoint: &graphica_core::migration_evidence::ConnectorEndpoint,
    auth: &graphica_core::migration_evidence::ConnectorAuth,
    connector_metadata: &HashMap<String, String>,
) -> Result<Value> {
    let capabilities_path = connector_metadata
        .get("ecc_capabilities_path")
        .cloned()
        .unwrap_or_else(|| "/adapter/v1/capabilities".to_string());
    let capabilities_url = connector_metadata
        .get("ecc_capabilities_url")
        .cloned()
        .unwrap_or_else(|| {
            format!(
                "{}{}",
                endpoint.base_url.trim_end_matches('/'),
                capabilities_path
            )
        });
    let client = reqwest::Client::new();
    let mut request = client.get(&capabilities_url);
    request = request.header("accept", "application/json");
    for (key, value) in &endpoint.headers {
        request = request.header(key, value);
    }
    request = apply_auth(request, auth);
    let response = request.send().await?.error_for_status()?;
    response
        .json()
        .await
        .context("failed to read SAP ECC adapter capabilities response")
}

fn build_odata_headers_from_connection(
    connection: &HashMap<String, String>,
) -> HashMap<String, String> {
    let mut headers = HashMap::new();
    if let Some(client) = connection.get("odata_client") {
        headers.insert("sap-client".to_string(), client.clone());
    }
    if let Some(language) = connection.get("odata_language") {
        headers.insert("accept-language".to_string(), language.clone());
    }
    headers
}

fn apply_s4_capabilities_metadata(
    connector: &mut MigrationConnector,
    endpoint: &graphica_core::migration_evidence::ConnectorEndpoint,
    capabilities: &SapS4ODataCapabilities,
) -> Result<()> {
    connector.metadata.insert(
        "odata_service_root_path".to_string(),
        capabilities.service_root_path.clone(),
    );
    connector.metadata.insert(
        "odata_metadata_path".to_string(),
        capabilities.metadata_path.clone(),
    );
    connector.metadata.insert(
        "odata_metadata_url".to_string(),
        format!(
            "{}{}",
            endpoint.base_url.trim_end_matches('/'),
            capabilities.metadata_path
        ),
    );
    connector.metadata.insert(
        "odata_metadata_version".to_string(),
        match capabilities.version {
            SapS4ODataVersion::V2 => "v2",
            SapS4ODataVersion::V4 => "v4",
            SapS4ODataVersion::Unknown => "unknown",
        }
        .to_string(),
    );
    if let Some(entity_set) = &capabilities.entity_set {
        connector
            .metadata
            .insert("odata_entity_set".to_string(), entity_set.clone());
    }
    if let Some(entity_type) = &capabilities.entity_type {
        connector
            .metadata
            .insert("odata_entity_type".to_string(), entity_type.clone());
    }
    connector.metadata.insert(
        "odata_key_fields_json".to_string(),
        serde_json::to_string(&capabilities.key_fields)?,
    );
    connector.metadata.insert(
        "odata_property_types_json".to_string(),
        serde_json::to_string(
            &capabilities
                .properties
                .iter()
                .map(|property| (property.name.clone(), property.edm_type.clone()))
                .collect::<BTreeMap<_, _>>(),
        )?,
    );
    connector.metadata.insert(
        "odata_property_count".to_string(),
        capabilities.properties.len().to_string(),
    );
    connector.metadata.insert(
        "odata_supports_record_projection".to_string(),
        capabilities.supports_record_projection.to_string(),
    );
    connector.metadata.insert(
        "odata_supports_rowset_projection".to_string(),
        capabilities.supports_rowset_projection.to_string(),
    );
    connector.metadata.insert(
        "odata_metadata_discovered_at".to_string(),
        Utc::now().to_rfc3339(),
    );
    Ok(())
}

fn apply_ecc_capabilities_metadata(
    connector: &mut MigrationConnector,
    endpoint: &graphica_core::migration_evidence::ConnectorEndpoint,
    capabilities: &SapEccAdapterCapabilities,
) -> Result<()> {
    let capabilities_path = connector
        .metadata
        .get("ecc_capabilities_path")
        .cloned()
        .unwrap_or_else(|| "/adapter/v1/capabilities".to_string());
    connector.metadata.insert(
        "ecc_capabilities_path".to_string(),
        capabilities_path.clone(),
    );
    connector.metadata.insert(
        "ecc_capabilities_url".to_string(),
        format!(
            "{}{}",
            endpoint.base_url.trim_end_matches('/'),
            capabilities_path
        ),
    );
    if let Some(health_path) = capabilities.health_path.as_deref() {
        connector
            .metadata
            .insert("ecc_health_path".to_string(), health_path.to_string());
        connector.metadata.insert(
            "ecc_health_url".to_string(),
            format!("{}{}", endpoint.base_url.trim_end_matches('/'), health_path),
        );
    }
    if let Some(version) = capabilities.adapter_version.as_deref() {
        connector
            .metadata
            .insert("ecc_adapter_version".to_string(), version.to_string());
    }
    if let Some(system_id) = capabilities.system_id.as_deref() {
        connector
            .metadata
            .insert("ecc_system_id".to_string(), system_id.to_string());
    }
    if let Some(client) = capabilities.client.as_deref() {
        connector
            .metadata
            .insert("ecc_client".to_string(), client.to_string());
    }
    if let Some(object_name) = capabilities.object_name.as_deref() {
        connector
            .metadata
            .insert("ecc_object_name".to_string(), object_name.to_string());
    }
    connector.metadata.insert(
        "ecc_key_fields_json".to_string(),
        serde_json::to_string(&capabilities.key_fields)?,
    );
    connector.metadata.insert(
        "ecc_required_parameters_json".to_string(),
        serde_json::to_string(&capabilities.required_parameters)?,
    );
    connector.metadata.insert(
        "ecc_field_types_json".to_string(),
        serde_json::to_string(&field_types_by_name(&capabilities.fields))?,
    );
    connector.metadata.insert(
        "ecc_supported_auth_modes_json".to_string(),
        serde_json::to_string(&capabilities.supported_auth_modes)?,
    );
    connector.metadata.insert(
        "ecc_supported_session_modes_json".to_string(),
        serde_json::to_string(&capabilities.supported_session_modes)?,
    );
    if let Some(session_id_path) = capabilities.session_id_path.as_deref() {
        connector.metadata.insert(
            "ecc_session_id_path".to_string(),
            session_id_path.to_string(),
        );
    }
    if let Some(session_id_parameter_name) = capabilities.session_id_parameter_name.as_deref() {
        connector.metadata.insert(
            "ecc_session_id_parameter_name".to_string(),
            session_id_parameter_name.to_string(),
        );
    }
    if let Some(close_session_path) = capabilities.close_session_path.as_deref() {
        connector.metadata.insert(
            "ecc_close_session_path".to_string(),
            close_session_path.to_string(),
        );
    }
    if let Some(close_session_method) = capabilities.close_session_method.as_deref() {
        connector.metadata.insert(
            "ecc_close_session_method".to_string(),
            close_session_method.to_string(),
        );
    }
    connector.metadata.insert(
        "ecc_requires_explicit_session_close".to_string(),
        capabilities.requires_explicit_session_close.to_string(),
    );
    if let Some(session_ttl_seconds) = capabilities.session_ttl_seconds {
        connector.metadata.insert(
            "ecc_session_ttl_seconds".to_string(),
            session_ttl_seconds.to_string(),
        );
    }
    if let Some(max_page_size) = capabilities.max_page_size {
        connector
            .metadata
            .insert("ecc_max_page_size".to_string(), max_page_size.to_string());
    }
    if let Some(page_size_parameter_name) = capabilities.page_size_parameter_name.as_deref() {
        connector.metadata.insert(
            "ecc_page_size_parameter_name".to_string(),
            page_size_parameter_name.to_string(),
        );
    }
    if let Some(language_parameter_name) = capabilities.language_parameter_name.as_deref() {
        connector.metadata.insert(
            "ecc_language_parameter_name".to_string(),
            language_parameter_name.to_string(),
        );
    }
    connector.metadata.insert(
        "ecc_field_count".to_string(),
        capabilities.fields.len().to_string(),
    );
    connector.metadata.insert(
        "ecc_supports_record_projection".to_string(),
        capabilities.supports_record_projection.to_string(),
    );
    connector.metadata.insert(
        "ecc_supports_rowset_projection".to_string(),
        capabilities.supports_rowset_projection.to_string(),
    );
    connector.metadata.insert(
        "ecc_supports_key_lookup".to_string(),
        capabilities.supports_key_lookup.to_string(),
    );
    connector.metadata.insert(
        "ecc_capabilities_discovered_at".to_string(),
        Utc::now().to_rfc3339(),
    );
    Ok(())
}

fn apply_ecc_rfc_capabilities_metadata(
    connector: &mut MigrationConnector,
    endpoint: &graphica_core::migration_evidence::ConnectorEndpoint,
    capabilities: &SapEccRfcBapiCapabilities,
) -> Result<()> {
    let capabilities_path = connector
        .metadata
        .get("ecc_rfc_capabilities_path")
        .cloned()
        .unwrap_or_else(|| "/bridge/v1/capabilities".to_string());
    connector.metadata.insert(
        "ecc_rfc_capabilities_path".to_string(),
        capabilities_path.clone(),
    );
    connector.metadata.insert(
        "ecc_rfc_capabilities_url".to_string(),
        format!(
            "{}{}",
            endpoint.base_url.trim_end_matches('/'),
            capabilities_path
        ),
    );
    if let Some(health_path) = capabilities.health_path.as_deref() {
        connector
            .metadata
            .insert("ecc_rfc_health_path".to_string(), health_path.to_string());
        connector.metadata.insert(
            "ecc_rfc_health_url".to_string(),
            format!("{}{}", endpoint.base_url.trim_end_matches('/'), health_path),
        );
    }
    connector.metadata.insert(
        "ecc_rfc_profile".to_string(),
        match capabilities.profile {
            SapEccRfcBapiProfile::BapiRecordLookup => "bapi_record_lookup",
            SapEccRfcBapiProfile::FunctionModuleExport => "function_module_export",
            SapEccRfcBapiProfile::TableReadRowset => "table_read_rowset",
            SapEccRfcBapiProfile::QueryBridge => "query_bridge",
        }
        .to_string(),
    );
    if let Some(version) = capabilities.bridge_version.as_deref() {
        connector
            .metadata
            .insert("ecc_rfc_bridge_version".to_string(), version.to_string());
    }
    if let Some(system_id) = capabilities.system_id.as_deref() {
        connector
            .metadata
            .insert("ecc_rfc_system_id".to_string(), system_id.to_string());
    }
    if let Some(client) = capabilities.client.as_deref() {
        connector
            .metadata
            .insert("ecc_rfc_client".to_string(), client.to_string());
    }
    if let Some(function_module) = capabilities.function_module.as_deref() {
        connector.metadata.insert(
            "ecc_rfc_function_module".to_string(),
            function_module.to_string(),
        );
    }
    if let Some(bapi_name) = capabilities.bapi_name.as_deref() {
        connector
            .metadata
            .insert("ecc_rfc_bapi_name".to_string(), bapi_name.to_string());
    }
    if let Some(export_structure) = capabilities.export_structure.as_deref() {
        connector.metadata.insert(
            "ecc_rfc_export_structure".to_string(),
            export_structure.to_string(),
        );
    }
    connector.metadata.insert(
        "ecc_rfc_key_fields_json".to_string(),
        serde_json::to_string(&capabilities.key_fields)?,
    );
    connector.metadata.insert(
        "ecc_rfc_required_parameters_json".to_string(),
        serde_json::to_string(&capabilities.required_parameters)?,
    );
    connector.metadata.insert(
        "ecc_rfc_supported_auth_modes_json".to_string(),
        serde_json::to_string(&capabilities.supported_auth_modes)?,
    );
    connector.metadata.insert(
        "ecc_rfc_supported_session_modes_json".to_string(),
        serde_json::to_string(&capabilities.supported_session_modes)?,
    );
    if let Some(session_id_path) = capabilities.session_id_path.as_deref() {
        connector.metadata.insert(
            "ecc_rfc_session_id_path".to_string(),
            session_id_path.to_string(),
        );
    }
    if let Some(session_id_parameter_name) = capabilities.session_id_parameter_name.as_deref() {
        connector.metadata.insert(
            "ecc_rfc_session_id_parameter_name".to_string(),
            session_id_parameter_name.to_string(),
        );
    }
    if let Some(close_session_path) = capabilities.close_session_path.as_deref() {
        connector.metadata.insert(
            "ecc_rfc_close_session_path".to_string(),
            close_session_path.to_string(),
        );
    }
    if let Some(close_session_method) = capabilities.close_session_method.as_deref() {
        connector.metadata.insert(
            "ecc_rfc_close_session_method".to_string(),
            close_session_method.to_string(),
        );
    }
    connector.metadata.insert(
        "ecc_rfc_requires_explicit_session_close".to_string(),
        capabilities.requires_explicit_session_close.to_string(),
    );
    if let Some(session_ttl_seconds) = capabilities.session_ttl_seconds {
        connector.metadata.insert(
            "ecc_rfc_session_ttl_seconds".to_string(),
            session_ttl_seconds.to_string(),
        );
    }
    connector.metadata.insert(
        "ecc_rfc_field_types_json".to_string(),
        serde_json::to_string(&rfc_field_types_by_name(&capabilities.fields))?,
    );
    if let Some(max_page_size) = capabilities.max_page_size {
        connector.metadata.insert(
            "ecc_rfc_max_page_size".to_string(),
            max_page_size.to_string(),
        );
    }
    if let Some(page_size_parameter_name) = capabilities.page_size_parameter_name.as_deref() {
        connector.metadata.insert(
            "ecc_rfc_page_size_parameter_name".to_string(),
            page_size_parameter_name.to_string(),
        );
    }
    if let Some(language_parameter_name) = capabilities.language_parameter_name.as_deref() {
        connector.metadata.insert(
            "ecc_rfc_language_parameter_name".to_string(),
            language_parameter_name.to_string(),
        );
    }
    if let Some(cursor_parameter_name) = capabilities.cursor_parameter_name.as_deref() {
        connector.metadata.insert(
            "ecc_rfc_cursor_parameter_name".to_string(),
            cursor_parameter_name.to_string(),
        );
    }
    if let Some(next_cursor_path) = capabilities.next_cursor_path.as_deref() {
        connector.metadata.insert(
            "ecc_rfc_next_cursor_path".to_string(),
            next_cursor_path.to_string(),
        );
    }
    connector.metadata.insert(
        "ecc_rfc_field_count".to_string(),
        capabilities.fields.len().to_string(),
    );
    connector.metadata.insert(
        "ecc_rfc_supports_record_projection".to_string(),
        capabilities.supports_record_projection.to_string(),
    );
    connector.metadata.insert(
        "ecc_rfc_supports_rowset_projection".to_string(),
        capabilities.supports_rowset_projection.to_string(),
    );
    connector.metadata.insert(
        "ecc_rfc_supports_key_lookup".to_string(),
        capabilities.supports_key_lookup.to_string(),
    );
    connector.metadata.insert(
        "ecc_rfc_supports_cursor_pagination".to_string(),
        capabilities.supports_cursor_pagination.to_string(),
    );
    connector.metadata.insert(
        "ecc_rfc_capabilities_discovered_at".to_string(),
        Utc::now().to_rfc3339(),
    );
    Ok(())
}

fn apply_auth(
    mut request: reqwest::RequestBuilder,
    auth: &graphica_core::migration_evidence::ConnectorAuth,
) -> reqwest::RequestBuilder {
    match auth.kind {
        ConnectorAuthKind::Bearer => {
            if let Some(token) = auth.token.as_deref() {
                request = request.bearer_auth(token);
            }
        }
        ConnectorAuthKind::ApiKey => {
            if let (Some(header), Some(key)) =
                (auth.header_name.as_deref(), auth.api_key.as_deref())
            {
                request = request.header(header, key);
            }
        }
        ConnectorAuthKind::Basic => {
            request = request.basic_auth(
                auth.username.clone().unwrap_or_default(),
                auth.password.clone(),
            );
        }
        ConnectorAuthKind::None => {}
    }
    request
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

fn enrich_verification_source_from_connector(
    connector: &MigrationConnector,
    verification: &mut VerificationRequest,
) {
    if verification.source.endpoint.is_none() {
        verification.source.endpoint = Some(connector.endpoint.clone());
    }
    if verification.source.auth.kind == ConnectorAuthKind::None
        && verification.source.auth.secret_ref.is_none()
    {
        verification.source.auth = connector.auth.clone();
    }
    let prefix = match connector.transport {
        ConnectorTransport::SapS4OData => Some("odata_"),
        ConnectorTransport::SapEccAdapter => Some("ecc_"),
        ConnectorTransport::SapEccRfcBapi => Some("ecc_rfc_"),
        _ => None,
    };
    let Some(prefix) = prefix else {
        return;
    };

    for (key, value) in &connector.metadata {
        if key.starts_with(prefix) {
            verification
                .source
                .connection
                .entry(key.clone())
                .or_insert_with(|| value.clone());
        }
    }
}

fn normalize_events(
    connector: &MigrationConnector,
    run_id: &str,
    events: &mut [MigrationEvidenceEvent],
) -> Result<()> {
    for event in events {
        event.connector_id = connector.connector_id.clone();
        event.run_id = run_id.to_string();
        event.vendor = connector.vendor.clone();
        if event.program_id.is_empty() {
            event.program_id = connector.program_id.clone();
        }
        if event.program_id.is_empty() {
            return Err(anyhow!("migration evidence event is missing program_id"));
        }
        if event.object_id.is_empty()
            && !matches!(event.artifact_type, MigrationEvidenceArtifactType::Program)
        {
            return Err(anyhow!("migration evidence event is missing object_id"));
        }
        event.captured_at = Utc::now();
    }
    Ok(())
}

#[derive(Clone)]
pub struct EvidenceIngestionServiceImpl {
    manager: EvidenceIngestionManager,
    started_at: Instant,
}

impl EvidenceIngestionServiceImpl {
    pub fn new(manager: EvidenceIngestionManager) -> Self {
        Self {
            manager,
            started_at: Instant::now(),
        }
    }
}

#[tonic::async_trait]
impl EvidenceIngestionService for EvidenceIngestionServiceImpl {
    async fn upsert_connector(
        &self,
        request: Request<UpsertConnectorRequest>,
    ) -> Result<Response<ConnectorReply>, Status> {
        let connector: MigrationConnector =
            deserialize(&request.into_inner().connector_json).map_err(internal_status)?;
        let stored = self
            .manager
            .upsert_connector(connector)
            .await
            .map_err(internal_status)?;
        Ok(Response::new(ConnectorReply {
            connector_json: serialize(&stored).map_err(internal_status)?,
        }))
    }

    async fn get_connector(
        &self,
        request: Request<GetConnectorRequest>,
    ) -> Result<Response<ConnectorReply>, Status> {
        let connector = self
            .manager
            .get_connector(&request.into_inner().connector_id)
            .await
            .map_err(internal_status)?;
        Ok(Response::new(ConnectorReply {
            connector_json: serialize(&connector).map_err(internal_status)?,
        }))
    }

    async fn run_connector(
        &self,
        request: Request<RunConnectorRequest>,
    ) -> Result<Response<RunConnectorResponse>, Status> {
        let req = request.into_inner();
        let run_request: DomainConnectorRunRequest =
            deserialize(&req.run_request_json).map_err(internal_status)?;
        let (summary, events) = self
            .manager
            .run_connector(&req.connector_id, run_request)
            .await
            .map_err(internal_status)?;
        Ok(Response::new(RunConnectorResponse {
            run_summary_json: serialize(&summary).map_err(internal_status)?,
            event_json: events
                .iter()
                .map(|event| serialize(event))
                .collect::<Result<Vec<_>, _>>()
                .map_err(internal_status)?,
        }))
    }

    async fn get_runtime_status(
        &self,
        _request: Request<RuntimeStatusRequest>,
    ) -> Result<Response<RuntimeStatusReply>, Status> {
        let status = self
            .manager
            .runtime_status()
            .await
            .map_err(internal_status)?;
        Ok(Response::new(RuntimeStatusReply {
            runtime_status_json: serialize(&status).map_err(internal_status)?,
        }))
    }

    async fn health(
        &self,
        _request: Request<HealthRequest>,
    ) -> Result<Response<HealthResponse>, Status> {
        Ok(Response::new(HealthResponse {
            healthy: true,
            service_name: "arcxa-evidence-ingestion".to_string(),
            uptime_seconds: self.started_at.elapsed().as_secs() as i64,
        }))
    }
}

fn serialize<T: serde::Serialize>(value: &T) -> Result<String> {
    serde_json::to_string(value).context("failed to serialize response")
}

fn deserialize<T: DeserializeOwned>(value: &str) -> Result<T> {
    serde_json::from_str(value).context("failed to deserialize payload")
}

fn internal_status(error: impl std::fmt::Display) -> Status {
    Status::internal(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphica_core::migration_evidence::{
        verification_result_to_events, ConnectorAuth, ConnectorAuthKind, ConnectorEndpoint,
        ConnectorTransport, ControlResult, ControlStatus, ExceptionRecord, ExceptionSeverity,
        ExceptionStatus, ExecutionEvent, MigrationConnector, MigrationConnectorVendor,
        MigrationEvidenceArtifactType, MigrationEvidenceDeliveryMode,
        MigrationEvidenceDispatchSummary, MigrationEvidenceEventForwarder,
        SapEccStagedControlEvidence, SapEccStagedExceptionEvidence, SapEccStagedExportBundle,
        SapEccStagedExportDataFormat, SapEccStagedExportDataSet, SapEccStagedExportManifest,
        SapEccStagedRuleEvidence, SapIdocExtractorBundle, SapIdocExtractorDataFormat,
        SapIdocExtractorDataSet, SapIdocExtractorManifest, SourceFieldRef, TargetFieldRef,
        TransformationRule, TransformationRuleType, VerificationDispatchRequest,
        VerificationDispatchResult, VerificationRequest, VerificationResult, VerificationSource,
    };
    use graphica_core::secrets::providers::{InlineSecretStore, SecretStoreRegistry};
    use graphica_core::secrets::{put_secret_by_ref, SecretValue};
    use std::sync::{Arc, Mutex};
    use tempfile::tempdir;
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

    #[derive(Clone, Default)]
    struct StubVerification;

    #[async_trait]
    impl VerificationProvider for StubVerification {
        async fn run_verification_and_emit(
            &self,
            request: VerificationDispatchRequest,
        ) -> Result<VerificationDispatchResult> {
            let verification = request.verification;
            let verification_result = VerificationResult {
                execution_event: ExecutionEvent {
                    execution_id: "exec-1".to_string(),
                    program_id: verification.program_id.clone(),
                    object_id: verification.object_id.clone(),
                    connector_run_id: "run-1".to_string(),
                    tool_name: "sap_hana_verification".to_string(),
                    tool_run_id: "tool-run-1".to_string(),
                    stage: "verification".to_string(),
                    status: graphica_core::migration_evidence::ExecutionStatus::Succeeded,
                    happened_at: Utc::now(),
                    source_snapshot_ref: None,
                    target_snapshot_ref: None,
                    records_examined: Some(1),
                    records_affected: Some(1),
                    metadata: verification.metadata.clone(),
                },
                control_result: ControlResult {
                    control_id: "control-1".to_string(),
                    program_id: verification.program_id.clone(),
                    object_id: verification.object_id.clone(),
                    control_name: verification.control_name.clone(),
                    control_type: "verification".to_string(),
                    status: ControlStatus::Passed,
                    summary: "matched".to_string(),
                    expected_value: verification.expected_value.clone(),
                    actual_value: verification.expected_value.clone(),
                    tolerance: verification.tolerance,
                    executed_at: Utc::now(),
                    evidence_refs: vec![],
                    metadata: HashMap::from([(
                        "value_key".to_string(),
                        "SO-1::$.amount".to_string(),
                    )]),
                },
                exception_record: None,
            };
            let emitted_events = verification_result_to_events(
                request.connector_id,
                request.run_id,
                request.vendor,
                verification_result.clone(),
            )?;
            Ok(VerificationDispatchResult {
                verification_result,
                dispatch_summary: MigrationEvidenceDispatchSummary::from_events(
                    &emitted_events,
                    MigrationEvidenceDeliveryMode::Direct,
                    true,
                ),
                emitted_events,
            })
        }
    }

    #[derive(Clone, Default)]
    struct CapturingVerification {
        captured: Arc<Mutex<Vec<VerificationDispatchRequest>>>,
    }

    #[async_trait]
    impl VerificationProvider for CapturingVerification {
        async fn run_verification_and_emit(
            &self,
            request: VerificationDispatchRequest,
        ) -> Result<VerificationDispatchResult> {
            self.captured.lock().unwrap().push(request.clone());
            let verification_result = VerificationResult {
                execution_event: ExecutionEvent {
                    execution_id: "exec-capture".to_string(),
                    program_id: request.verification.program_id.clone(),
                    object_id: request.verification.object_id.clone(),
                    connector_run_id: request.run_id.clone(),
                    tool_name: "sap_s4_odata_verification".to_string(),
                    tool_run_id: "tool-run-capture".to_string(),
                    stage: "verification".to_string(),
                    status: graphica_core::migration_evidence::ExecutionStatus::Succeeded,
                    happened_at: Utc::now(),
                    source_snapshot_ref: None,
                    target_snapshot_ref: None,
                    records_examined: Some(1),
                    records_affected: Some(1),
                    metadata: HashMap::new(),
                },
                control_result: ControlResult {
                    control_id: "control-capture".to_string(),
                    program_id: request.verification.program_id.clone(),
                    object_id: request.verification.object_id.clone(),
                    control_name: request.verification.control_name.clone(),
                    control_type: "verification".to_string(),
                    status: ControlStatus::Passed,
                    summary: "matched".to_string(),
                    expected_value: request.verification.expected_value.clone(),
                    actual_value: request.verification.expected_value.clone(),
                    tolerance: request.verification.tolerance,
                    executed_at: Utc::now(),
                    evidence_refs: vec![],
                    metadata: HashMap::new(),
                },
                exception_record: None,
            };
            let emitted_events = verification_result_to_events(
                request.connector_id,
                request.run_id,
                request.vendor,
                verification_result.clone(),
            )?;
            Ok(VerificationDispatchResult {
                verification_result,
                dispatch_summary: MigrationEvidenceDispatchSummary::from_events(
                    &emitted_events,
                    MigrationEvidenceDeliveryMode::Direct,
                    true,
                ),
                emitted_events,
            })
        }
    }

    #[tokio::test]
    async fn verification_connector_emits_execution_and_control_events() {
        let temp = tempdir().unwrap();
        let store = PersistedConnectorStore::open(temp.path().join("connectors.json"))
            .await
            .unwrap();
        let forwarder = CapturingForwarder::default();
        let manager = EvidenceIngestionManager::new(
            store,
            Arc::new(forwarder.clone()),
            Arc::new(StubVerification),
            MigrationEvidenceDeliveryMode::Direct,
        );
        manager
            .upsert_connector(MigrationConnector {
                connector_id: "connector-1".to_string(),
                name: "sap verification".to_string(),
                vendor: MigrationConnectorVendor::SapHana,
                role: MigrationConnectorRole::VerificationSource,
                transport: ConnectorTransport::SapHanaSql,
                program_id: "program-1".to_string(),
                endpoint: ConnectorEndpoint {
                    base_url: "https://example.test".to_string(),
                    path: "/unused".to_string(),
                    method: "POST".to_string(),
                    headers: HashMap::new(),
                },
                auth: ConnectorAuth::default(),
                schedule: None,
                enabled: true,
                metadata: HashMap::new(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
            })
            .await
            .unwrap();

        let (summary, events) = manager
            .run_connector(
                "connector-1",
                DomainConnectorRunRequest {
                    run_label: None,
                    manual_events: vec![],
                    verification: Some(VerificationRequest {
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
                        expected_value: Some(Value::from(10)),
                        tolerance: Some(0.0),
                        metadata: HashMap::new(),
                        source: VerificationSource {
                            transport: ConnectorTransport::SapHanaSql,
                            query: Some("SELECT 10".to_string()),
                            endpoint: None,
                            auth: ConnectorAuth::default(),
                            connection: HashMap::new(),
                        },
                    }),
                    request_body: None,
                    request_headers: HashMap::new(),
                },
            )
            .await
            .unwrap();

        assert_eq!(summary.ingested_event_count, 2);
        assert_eq!(summary.delivery_mode, MigrationEvidenceDeliveryMode::Direct);
        assert!(summary.traceability_acknowledged);
        assert_eq!(events.len(), 2);
        // Verification connectors now emit through the verification service path,
        // so ingestion should not re-forward the already-dispatched events.
        assert_eq!(forwarder.captured.lock().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn s4_odata_connector_discovers_metadata_and_persists_capabilities() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            if let Ok((mut socket, _)) = listener.accept().await {
                let mut buffer = vec![0u8; 2048];
                let _ = socket.read(&mut buffer).await;
                let request = String::from_utf8_lossy(&buffer);
                assert!(request.contains("/sap/opu/odata4/API_SALES_ORDER/$metadata"));
                let body = r#"<?xml version="1.0" encoding="utf-8"?>
<edmx:Edmx Version="4.0" xmlns:edmx="http://docs.oasis-open.org/odata/ns/edmx">
  <edmx:DataServices>
    <Schema Namespace="API_SALES_ORDER" xmlns="http://docs.oasis-open.org/odata/ns/edm">
      <EntityType Name="A_SalesOrderType">
        <Key><PropertyRef Name="SalesOrder"/></Key>
        <Property Name="SalesOrder" Type="Edm.String" Nullable="false"/>
        <Property Name="NetAmount" Type="Edm.Decimal" Nullable="true"/>
        <Property Name="TransactionCurrency" Type="Edm.String" Nullable="true"/>
      </EntityType>
      <EntityContainer Name="Container">
        <EntitySet Name="A_SalesOrder" EntityType="API_SALES_ORDER.A_SalesOrderType"/>
      </EntityContainer>
    </Schema>
  </edmx:DataServices>
</edmx:Edmx>"#;
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/xml\r\ncontent-length: {}\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = socket.write_all(response.as_bytes()).await;
            }
        });

        let temp = tempdir().unwrap();
        let store = PersistedConnectorStore::open(temp.path().join("connectors.json"))
            .await
            .unwrap();
        let manager = EvidenceIngestionManager::new(
            store,
            Arc::new(CapturingForwarder::default()),
            Arc::new(StubVerification),
            MigrationEvidenceDeliveryMode::Direct,
        );
        manager
            .upsert_connector(MigrationConnector {
                connector_id: "s4-connector".to_string(),
                name: "sap s4 verification".to_string(),
                vendor: MigrationConnectorVendor::SapS4,
                role: MigrationConnectorRole::VerificationSource,
                transport: ConnectorTransport::SapS4OData,
                program_id: "program-1".to_string(),
                endpoint: ConnectorEndpoint {
                    base_url: format!("http://{}", addr),
                    path: "/sap/opu/odata4/API_SALES_ORDER/A_SalesOrder?$top=1".to_string(),
                    method: "GET".to_string(),
                    headers: HashMap::new(),
                },
                auth: ConnectorAuth::default(),
                schedule: None,
                enabled: true,
                metadata: HashMap::new(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
            })
            .await
            .unwrap();

        manager
            .run_connector(
                "s4-connector",
                DomainConnectorRunRequest {
                    run_label: None,
                    manual_events: vec![],
                    verification: Some(VerificationRequest {
                        control_name: "projection-match".to_string(),
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
                            field_path: "$.NetAmount".to_string(),
                            semantic_type: None,
                            record_id: Some("SO-1".to_string()),
                        },
                        expected_value: Some(Value::from(10)),
                        tolerance: Some(0.0),
                        metadata: HashMap::new(),
                        source: VerificationSource {
                            transport: ConnectorTransport::SapS4OData,
                            query: None,
                            endpoint: Some(ConnectorEndpoint {
                                base_url: format!("http://{}", addr),
                                path: "/sap/opu/odata4/API_SALES_ORDER/A_SalesOrder?$top=1"
                                    .to_string(),
                                method: "GET".to_string(),
                                headers: HashMap::new(),
                            }),
                            auth: ConnectorAuth::default(),
                            connection: HashMap::from([(
                                "odata_client".to_string(),
                                "100".to_string(),
                            )]),
                        },
                    }),
                    request_body: None,
                    request_headers: HashMap::new(),
                },
            )
            .await
            .unwrap();

        let stored = manager.get_connector("s4-connector").await.unwrap();
        assert_eq!(
            stored.metadata.get("odata_metadata_version"),
            Some(&"v4".to_string())
        );
        assert_eq!(
            stored.metadata.get("odata_entity_set"),
            Some(&"A_SalesOrder".to_string())
        );
        assert_eq!(
            stored.metadata.get("odata_entity_type"),
            Some(&"API_SALES_ORDER.A_SalesOrderType".to_string())
        );
        assert_eq!(
            stored.metadata.get("odata_key_fields_json"),
            Some(&"[\"SalesOrder\"]".to_string())
        );
        assert_eq!(
            stored.metadata.get("odata_supports_record_projection"),
            Some(&"true".to_string())
        );
    }

    #[tokio::test]
    async fn s4_odata_connector_passes_discovered_metadata_into_verification_source() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            if let Ok((mut socket, _)) = listener.accept().await {
                let mut buffer = vec![0u8; 2048];
                let _ = socket.read(&mut buffer).await;
                let body = r#"<?xml version="1.0" encoding="utf-8"?>
<edmx:Edmx Version="4.0" xmlns:edmx="http://docs.oasis-open.org/odata/ns/edmx">
  <edmx:DataServices>
    <Schema Namespace="API_SALES_ORDER" xmlns="http://docs.oasis-open.org/odata/ns/edm">
      <EntityType Name="A_SalesOrderType">
        <Key><PropertyRef Name="SalesOrder"/></Key>
        <Property Name="SalesOrder" Type="Edm.String" Nullable="false"/>
        <Property Name="NetAmount" Type="Edm.Decimal" Nullable="true"/>
      </EntityType>
      <EntityContainer Name="Container">
        <EntitySet Name="A_SalesOrder" EntityType="API_SALES_ORDER.A_SalesOrderType"/>
      </EntityContainer>
    </Schema>
  </edmx:DataServices>
</edmx:Edmx>"#;
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/xml\r\ncontent-length: {}\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = socket.write_all(response.as_bytes()).await;
            }
        });

        let temp = tempdir().unwrap();
        let store = PersistedConnectorStore::open(temp.path().join("connectors.json"))
            .await
            .unwrap();
        let verification = CapturingVerification::default();
        let manager = EvidenceIngestionManager::new(
            store,
            Arc::new(CapturingForwarder::default()),
            Arc::new(verification.clone()),
            MigrationEvidenceDeliveryMode::Direct,
        );
        manager
            .upsert_connector(MigrationConnector {
                connector_id: "s4-connector".to_string(),
                name: "sap s4 verification".to_string(),
                vendor: MigrationConnectorVendor::SapS4,
                role: MigrationConnectorRole::VerificationSource,
                transport: ConnectorTransport::SapS4OData,
                program_id: "program-1".to_string(),
                endpoint: ConnectorEndpoint {
                    base_url: format!("http://{}", addr),
                    path: "/sap/opu/odata4/API_SALES_ORDER/A_SalesOrder?$top=1".to_string(),
                    method: "GET".to_string(),
                    headers: HashMap::new(),
                },
                auth: ConnectorAuth::default(),
                schedule: None,
                enabled: true,
                metadata: HashMap::new(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
            })
            .await
            .unwrap();

        manager
            .run_connector(
                "s4-connector",
                DomainConnectorRunRequest {
                    run_label: None,
                    manual_events: vec![],
                    verification: Some(VerificationRequest {
                        control_name: "projection-match".to_string(),
                        program_id: "program-1".to_string(),
                        object_id: "object-1".to_string(),
                        source_field: SourceFieldRef {
                            system: "ECC".to_string(),
                            object_name: "VBAK".to_string(),
                            field_name: "NETWR".to_string(),
                            field_path: "$.NetAmount".to_string(),
                            semantic_type: None,
                            record_id: Some("SO-1".to_string()),
                        },
                        target_field: TargetFieldRef {
                            system: "S4".to_string(),
                            object_name: "A_SalesOrder".to_string(),
                            field_name: "NetAmount".to_string(),
                            field_path: "$.NetAmount".to_string(),
                            semantic_type: None,
                            record_id: Some("SO-1".to_string()),
                        },
                        expected_value: Some(Value::from(10)),
                        tolerance: Some(0.0),
                        metadata: HashMap::new(),
                        source: VerificationSource {
                            transport: ConnectorTransport::SapS4OData,
                            query: None,
                            endpoint: Some(ConnectorEndpoint {
                                base_url: format!("http://{}", addr),
                                path: "/sap/opu/odata4/API_SALES_ORDER/A_SalesOrder?$top=1"
                                    .to_string(),
                                method: "GET".to_string(),
                                headers: HashMap::new(),
                            }),
                            auth: ConnectorAuth::default(),
                            connection: HashMap::new(),
                        },
                    }),
                    request_body: None,
                    request_headers: HashMap::new(),
                },
            )
            .await
            .unwrap();

        let captured = verification.captured.lock().unwrap();
        let forwarded = captured
            .first()
            .expect("verification request should be captured");
        assert_eq!(
            forwarded
                .verification
                .source
                .connection
                .get("odata_property_types_json"),
            Some(&r#"{"NetAmount":"Edm.Decimal","SalesOrder":"Edm.String"}"#.to_string())
        );
        assert_eq!(
            forwarded
                .verification
                .source
                .connection
                .get("odata_metadata_version"),
            Some(&"v4".to_string())
        );
    }

    #[tokio::test]
    async fn sap_ecc_adapter_connector_discovers_capabilities_and_forwards_them() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            for _ in 0..2 {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let mut buffer = vec![0u8; 2048];
                let _ = socket.read(&mut buffer).await;
                let request = String::from_utf8_lossy(&buffer);
                assert!(
                    request.contains("Authorization: Bearer adapter-secret-token")
                        || request.contains("authorization: Bearer adapter-secret-token")
                );
                let (expected_path, body) = if request.contains("/adapter/v1/capabilities") {
                    (
                        "/adapter/v1/capabilities",
                        r#"{
  "capabilities": {
    "adapter_version": "0.1.0",
    "system_id": "PRD",
    "client": "100",
    "object_name": "VBAK",
    "key_fields": ["VBELN"],
    "required_parameters": ["record_id"],
    "supported_auth_modes": ["destination"],
    "supported_session_modes": ["stateful"],
    "health_path": "/adapter/v1/health",
    "session_id_path": "$.session.id",
    "session_id_parameter_name": "sessionId",
    "close_session_path": "/adapter/v1/session/close",
    "close_session_method": "post",
    "requires_explicit_session_close": true,
    "session_ttl_seconds": 900,
    "max_page_size": 200,
    "page_size_parameter_name": "pageSize",
    "language_parameter_name": "spras",
    "supports_record_projection": true,
    "supports_rowset_projection": true,
    "supports_key_lookup": true,
    "fields": [
      {"name": "VBELN", "abap_type": "CHAR", "nullable": false},
      {"name": "NETWR", "abap_type": "CURR", "nullable": true}
    ]
  }
}"#,
                    )
                } else {
                    (
                        "/adapter/v1/health",
                        r#"{"status":"ok","ready":true,"version":"0.1.0","build":"ecc-adapter-build-1"}"#,
                    )
                };
                assert!(request.contains(expected_path));
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = socket.write_all(response.as_bytes()).await;
            }
        });

        let temp = tempdir().unwrap();
        let store = PersistedConnectorStore::open(temp.path().join("connectors.json"))
            .await
            .unwrap();
        let verification = CapturingVerification::default();
        let registry = Arc::new(SecretStoreRegistry::new());
        let secret_store = Arc::new(InlineSecretStore::new());
        registry.register("default", secret_store.clone());
        registry.set_default(secret_store.clone());
        let stored_secret_version = put_secret_by_ref(
            secret_store.as_ref(),
            "vault://migration/ecc/adapter-token",
            SecretValue::String("adapter-secret-token".to_string()),
            None,
        )
        .await
        .unwrap();
        let manager = EvidenceIngestionManager::new(
            store,
            Arc::new(CapturingForwarder::default()),
            Arc::new(verification.clone()),
            MigrationEvidenceDeliveryMode::Direct,
        )
        .with_secret_store_registry(registry);
        manager
            .upsert_connector(MigrationConnector {
                connector_id: "ecc-connector".to_string(),
                name: "sap ecc adapter verification".to_string(),
                vendor: MigrationConnectorVendor::SapEcc,
                role: MigrationConnectorRole::VerificationSource,
                transport: ConnectorTransport::SapEccAdapter,
                program_id: "program-1".to_string(),
                endpoint: ConnectorEndpoint {
                    base_url: format!("http://{}", addr),
                    path: "/adapter/v1/records/VBAK?record_id=500000001".to_string(),
                    method: "GET".to_string(),
                    headers: HashMap::new(),
                },
                auth: ConnectorAuth {
                    kind: ConnectorAuthKind::Bearer,
                    secret_ref: Some("vault://migration/ecc/adapter-token".to_string()),
                    token: None,
                    api_key: None,
                    header_name: None,
                    username: None,
                    password: None,
                },
                schedule: None,
                enabled: true,
                metadata: HashMap::new(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
            })
            .await
            .unwrap();

        manager
            .run_connector(
                "ecc-connector",
                DomainConnectorRunRequest {
                    run_label: None,
                    manual_events: vec![],
                    verification: Some(VerificationRequest {
                        control_name: "projection-match".to_string(),
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
                        expected_value: Some(Value::from(10)),
                        tolerance: Some(0.0),
                        metadata: HashMap::new(),
                        source: VerificationSource {
                            transport: ConnectorTransport::SapEccAdapter,
                            query: None,
                            endpoint: Some(ConnectorEndpoint {
                                base_url: format!("http://{}", addr),
                                path: "/adapter/v1/records/VBAK?record_id=500000001".to_string(),
                                method: "GET".to_string(),
                                headers: HashMap::new(),
                            }),
                            auth: ConnectorAuth::default(),
                            connection: HashMap::new(),
                        },
                    }),
                    request_body: None,
                    request_headers: HashMap::new(),
                },
            )
            .await
            .unwrap();

        let stored = manager.get_connector("ecc-connector").await.unwrap();
        assert_eq!(
            stored.metadata.get("ecc_adapter_version"),
            Some(&"0.1.0".to_string())
        );
        assert_eq!(
            stored.metadata.get("ecc_object_name"),
            Some(&"VBAK".to_string())
        );
        assert_eq!(
            stored.metadata.get("ecc_key_fields_json"),
            Some(&"[\"VBELN\"]".to_string())
        );
        assert_eq!(
            stored.metadata.get("ecc_supported_auth_modes_json"),
            Some(&r#"["destination"]"#.to_string())
        );
        assert_eq!(
            stored.metadata.get("ecc_supported_session_modes_json"),
            Some(&r#"["stateful"]"#.to_string())
        );
        assert_eq!(
            stored.metadata.get("ecc_session_id_path"),
            Some(&"$.session.id".to_string())
        );
        assert_eq!(
            stored.metadata.get("ecc_session_id_parameter_name"),
            Some(&"sessionId".to_string())
        );
        assert_eq!(
            stored.metadata.get("ecc_close_session_path"),
            Some(&"/adapter/v1/session/close".to_string())
        );
        assert_eq!(
            stored.metadata.get("ecc_requires_explicit_session_close"),
            Some(&"true".to_string())
        );
        assert_eq!(
            stored.metadata.get("ecc_session_ttl_seconds"),
            Some(&"900".to_string())
        );
        assert_eq!(
            stored.metadata.get("ecc_required_parameters_json"),
            Some(&r#"["record_id"]"#.to_string())
        );
        assert_eq!(
            stored.metadata.get("ecc_max_page_size"),
            Some(&"200".to_string())
        );
        assert_eq!(
            stored.metadata.get("ecc_page_size_parameter_name"),
            Some(&"pageSize".to_string())
        );
        assert_eq!(
            stored.metadata.get("ecc_language_parameter_name"),
            Some(&"spras".to_string())
        );
        assert_eq!(
            stored.metadata.get("ecc_health_status"),
            Some(&"ok".to_string())
        );
        assert_eq!(
            stored.metadata.get("ecc_health_ready"),
            Some(&"true".to_string())
        );
        assert_eq!(
            stored.metadata.get("auth_secret_ref"),
            Some(&"vault://migration/ecc/adapter-token".to_string())
        );
        assert_eq!(
            stored.metadata.get("auth_secret_store"),
            Some(&"inline".to_string())
        );
        assert_eq!(
            stored.metadata.get("auth_secret_version"),
            Some(&stored_secret_version)
        );

        let captured = verification.captured.lock().unwrap();
        let forwarded = captured
            .first()
            .expect("verification request should be captured");
        assert_eq!(
            forwarded
                .verification
                .source
                .connection
                .get("ecc_field_types_json"),
            Some(&r#"{"NETWR":"CURR","VBELN":"CHAR"}"#.to_string())
        );
        assert_eq!(
            forwarded
                .verification
                .source
                .connection
                .get("ecc_object_name"),
            Some(&"VBAK".to_string())
        );
        assert_eq!(
            forwarded
                .verification
                .source
                .connection
                .get("ecc_supported_auth_modes_json"),
            Some(&r#"["destination"]"#.to_string())
        );
        assert_eq!(
            forwarded
                .verification
                .source
                .connection
                .get("ecc_language_parameter_name"),
            Some(&"spras".to_string())
        );
    }

    #[tokio::test]
    async fn sap_ecc_staged_export_inline_bundle_emits_canonical_events() {
        let temp = tempdir().unwrap();
        let store = PersistedConnectorStore::open(temp.path().join("connectors.json"))
            .await
            .unwrap();
        let forwarder = CapturingForwarder::default();
        let manager = EvidenceIngestionManager::new(
            store,
            Arc::new(forwarder.clone()),
            Arc::new(StubVerification),
            MigrationEvidenceDeliveryMode::Direct,
        );

        manager
            .upsert_connector(MigrationConnector {
                connector_id: "ecc-stage-inline".to_string(),
                name: "ecc staged export inline".to_string(),
                vendor: MigrationConnectorVendor::SapEcc,
                role: MigrationConnectorRole::MigrationArtifactSource,
                transport: ConnectorTransport::SapEccStagedExport,
                program_id: "program-ecc-1".to_string(),
                endpoint: ConnectorEndpoint {
                    base_url: String::new(),
                    path: "inline-bundle".to_string(),
                    method: "POST".to_string(),
                    headers: HashMap::new(),
                },
                auth: ConnectorAuth::default(),
                schedule: None,
                enabled: true,
                metadata: HashMap::new(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
            })
            .await
            .unwrap();

        let inline_rows = serde_json::json!([
            {"VBELN": "500000001", "NETWR": 125.5, "WAERK": "USD"},
            {"VBELN": "500000002", "NETWR": 130.0, "WAERK": "USD"}
        ]);
        let inline_sha = sha256_hex(&serde_json::to_vec(&inline_rows).unwrap());
        let value_key = Some("500000001::$.NETWR".to_string());

        let bundle = SapEccStagedExportBundle {
            manifest: SapEccStagedExportManifest {
                schema_version: "1.0".to_string(),
                export_id: "ecc-export-1".to_string(),
                program_id: "program-ecc-1".to_string(),
                object_id: "object-vbak".to_string(),
                object_name: "VBAK".to_string(),
                source_system_id: "ECC-PRD".to_string(),
                source_client: "100".to_string(),
                extracted_at: Utc::now(),
                key_fields: vec!["VBELN".to_string()],
                data_set: Some(SapEccStagedExportDataSet {
                    format: SapEccStagedExportDataFormat::JsonRows,
                    path: None,
                    inline_payload: Some(inline_rows),
                    expected_row_count: Some(2),
                    sha256: Some(inline_sha),
                    metadata: HashMap::new(),
                }),
                metadata: HashMap::from([("cutover_wave".to_string(), "wave-1".to_string())]),
            },
            program: None,
            object: None,
            transformation_rules: vec![SapEccStagedRuleEvidence {
                value_key: value_key.clone(),
                rule: TransformationRule {
                    rule_id: "rule-netwr".to_string(),
                    rule_type: TransformationRuleType::Mapping,
                    name: "Map NETWR to NetAmount".to_string(),
                    description: None,
                    source_fields: vec![SourceFieldRef {
                        system: "SAP ECC".to_string(),
                        object_name: "VBAK".to_string(),
                        field_name: "NETWR".to_string(),
                        field_path: "$.NETWR".to_string(),
                        semantic_type: None,
                        record_id: Some("500000001".to_string()),
                    }],
                    target_fields: vec![TargetFieldRef {
                        system: "SAP S/4HANA".to_string(),
                        object_name: "SalesOrder".to_string(),
                        field_name: "NetAmount".to_string(),
                        field_path: "$.NetAmount".to_string(),
                        semantic_type: None,
                        record_id: Some("500000001".to_string()),
                    }],
                    expression: Some("NETWR".to_string()),
                    filter_predicate: None,
                    default_value: None,
                    aggregation: None,
                    metadata: HashMap::new(),
                },
            }],
            executions: vec![],
            exceptions: vec![SapEccStagedExceptionEvidence {
                value_key: value_key.clone(),
                exception: ExceptionRecord {
                    exception_id: "exception-currency-1".to_string(),
                    program_id: "program-ecc-1".to_string(),
                    object_id: "object-vbak".to_string(),
                    severity: ExceptionSeverity::Warning,
                    status: ExceptionStatus::Accepted,
                    category: "currency_rounding".to_string(),
                    message: "Rounded to target currency precision".to_string(),
                    source_value: None,
                    target_value: None,
                    remediation: None,
                    detected_at: Utc::now(),
                    resolved_at: None,
                    metadata: HashMap::new(),
                },
            }],
            controls: vec![SapEccStagedControlEvidence {
                value_key: value_key.clone(),
                control: ControlResult {
                    control_id: "control-netwr-1".to_string(),
                    program_id: "program-ecc-1".to_string(),
                    object_id: "object-vbak".to_string(),
                    control_name: "net_amount_reconciled".to_string(),
                    control_type: "field_reconciliation".to_string(),
                    status: ControlStatus::Passed,
                    summary: "Net amount reconciled for sample record".to_string(),
                    expected_value: Some(Value::from(125.5)),
                    actual_value: Some(Value::from(125.5)),
                    tolerance: Some(0.0),
                    executed_at: Utc::now(),
                    evidence_refs: vec![],
                    metadata: HashMap::new(),
                },
            }],
            approvals: vec![],
        };

        let (summary, events) = manager
            .run_connector(
                "ecc-stage-inline",
                DomainConnectorRunRequest {
                    run_label: Some("ecc-wave-1".to_string()),
                    manual_events: vec![],
                    verification: None,
                    request_body: Some(serde_json::to_value(bundle).unwrap()),
                    request_headers: HashMap::new(),
                },
            )
            .await
            .unwrap();

        assert_eq!(summary.ingested_event_count, events.len());
        assert!(events.iter().any(|event| {
            event.artifact_type == MigrationEvidenceArtifactType::ExecutionEvent
                && event.payload.get("tool_name")
                    == Some(&Value::String("sap_ecc_staged_export".to_string()))
        }));
        let integrity = events
            .iter()
            .find(|event| {
                event.artifact_type == MigrationEvidenceArtifactType::ControlResult
                    && event.payload.get("control_name")
                        == Some(&Value::String(
                            "sap_ecc_staged_export_integrity".to_string(),
                        ))
            })
            .expect("integrity control should be emitted");
        assert_eq!(
            integrity
                .payload
                .get("metadata")
                .and_then(|metadata| metadata.get("checksum_verified"))
                .and_then(Value::as_str),
            Some("true")
        );
        assert_eq!(
            integrity
                .payload
                .get("metadata")
                .and_then(|metadata| metadata.get("actual_row_count"))
                .and_then(Value::as_str),
            Some("2")
        );

        let captured = forwarder.captured.lock().unwrap();
        assert_eq!(captured.len(), events.len());
    }

    #[tokio::test]
    async fn sap_ecc_staged_export_loads_manifest_file_and_verifies_csv_dataset() {
        let temp = tempdir().unwrap();
        let csv_path = temp.path().join("vbak.csv");
        let manifest_path = temp.path().join("manifest.json");
        let csv_body = "VBELN,NETWR,WAERK\n500000001,125.50,USD\n500000002,130.00,USD\n";
        tokio::fs::write(&csv_path, csv_body).await.unwrap();
        let csv_sha = sha256_hex(csv_body.as_bytes());

        let bundle = SapEccStagedExportBundle {
            manifest: SapEccStagedExportManifest {
                schema_version: "1.0".to_string(),
                export_id: "ecc-export-file-1".to_string(),
                program_id: "program-ecc-2".to_string(),
                object_id: "object-vbak-file".to_string(),
                object_name: "VBAK".to_string(),
                source_system_id: "ECC-QA".to_string(),
                source_client: "200".to_string(),
                extracted_at: Utc::now(),
                key_fields: vec!["VBELN".to_string()],
                data_set: Some(SapEccStagedExportDataSet {
                    format: SapEccStagedExportDataFormat::Csv,
                    path: Some("vbak.csv".to_string()),
                    inline_payload: None,
                    expected_row_count: Some(2),
                    sha256: Some(csv_sha),
                    metadata: HashMap::new(),
                }),
                metadata: HashMap::new(),
            },
            program: None,
            object: None,
            transformation_rules: vec![],
            executions: vec![],
            exceptions: vec![],
            controls: vec![],
            approvals: vec![],
        };
        tokio::fs::write(&manifest_path, serde_json::to_vec_pretty(&bundle).unwrap())
            .await
            .unwrap();

        let store = PersistedConnectorStore::open(temp.path().join("connectors.json"))
            .await
            .unwrap();
        let manager = EvidenceIngestionManager::new(
            store,
            Arc::new(CapturingForwarder::default()),
            Arc::new(StubVerification),
            MigrationEvidenceDeliveryMode::Direct,
        );

        manager
            .upsert_connector(MigrationConnector {
                connector_id: "ecc-stage-file".to_string(),
                name: "ecc staged export file".to_string(),
                vendor: MigrationConnectorVendor::SapEcc,
                role: MigrationConnectorRole::MigrationArtifactSource,
                transport: ConnectorTransport::SapEccStagedExport,
                program_id: "program-ecc-2".to_string(),
                endpoint: ConnectorEndpoint {
                    base_url: String::new(),
                    path: manifest_path.display().to_string(),
                    method: "POST".to_string(),
                    headers: HashMap::new(),
                },
                auth: ConnectorAuth::default(),
                schedule: None,
                enabled: true,
                metadata: HashMap::new(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
            })
            .await
            .unwrap();

        let (_summary, events) = manager
            .run_connector(
                "ecc-stage-file",
                DomainConnectorRunRequest {
                    run_label: None,
                    manual_events: vec![],
                    verification: None,
                    request_body: None,
                    request_headers: HashMap::new(),
                },
            )
            .await
            .unwrap();

        let integrity = events
            .iter()
            .find(|event| {
                event.artifact_type == MigrationEvidenceArtifactType::ControlResult
                    && event.payload.get("control_name")
                        == Some(&Value::String(
                            "sap_ecc_staged_export_integrity".to_string(),
                        ))
            })
            .expect("integrity control should be emitted");
        assert_eq!(
            integrity
                .payload
                .get("metadata")
                .and_then(|metadata| metadata.get("data_format"))
                .and_then(Value::as_str),
            Some("csv")
        );
        assert_eq!(
            integrity
                .payload
                .get("metadata")
                .and_then(|metadata| metadata.get("actual_row_count"))
                .and_then(Value::as_str),
            Some("2")
        );
    }

    #[test]
    fn dispatch_summary_deduplicates_program_and_object_ids() {
        let events = vec![
            MigrationEvidenceEvent::new(
                "connector-1",
                "run-1",
                MigrationConnectorVendor::Generic,
                "program-1",
                "object-1",
                MigrationEvidenceArtifactType::Program,
                None,
                serde_json::json!({}),
            ),
            MigrationEvidenceEvent::new(
                "connector-1",
                "run-1",
                MigrationConnectorVendor::Generic,
                "program-1",
                "object-1",
                MigrationEvidenceArtifactType::Object,
                None,
                serde_json::json!({}),
            ),
            MigrationEvidenceEvent::new(
                "connector-1",
                "run-1",
                MigrationConnectorVendor::Generic,
                "program-2",
                "object-2",
                MigrationEvidenceArtifactType::Object,
                None,
                serde_json::json!({}),
            ),
        ];

        let dispatch = MigrationEvidenceDispatchSummary::from_events(
            &events,
            MigrationEvidenceDeliveryMode::Direct,
            true,
        );

        assert_eq!(
            dispatch.touched_program_ids,
            vec!["program-1".to_string(), "program-2".to_string()]
        );
        assert_eq!(
            dispatch.touched_object_ids,
            vec!["object-1".to_string(), "object-2".to_string()]
        );
    }

    #[tokio::test]
    async fn sap_ecc_rfc_bapi_connector_discovers_capabilities_and_forwards_them() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            for _ in 0..2 {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let mut buffer = vec![0u8; 2048];
                let _ = socket.read(&mut buffer).await;
                let request = String::from_utf8_lossy(&buffer);
                let (expected_path, body) = if request.contains("/bridge/v1/capabilities") {
                    (
                        "/bridge/v1/capabilities",
                        r#"{
  "capabilities": {
    "profile": "function_module_export",
    "bridge_version": "0.2.0",
    "system_id": "PRD",
    "client": "100",
    "function_module": "RFC_READ_TABLE",
    "bapi_name": "BAPI_SALESORDER_GETDETAIL",
    "export_structure": "ORDER_ITEMS_OUT",
    "key_fields": ["VBELN"],
    "required_parameters": ["record_id"],
    "supported_auth_modes": ["destination"],
    "supported_session_modes": ["stateful"],
    "health_path": "/bridge/v1/health",
    "session_id_path": "$.session.id",
    "session_id_parameter_name": "sessionId",
    "close_session_path": "/bridge/v1/session/close",
    "close_session_method": "post",
    "requires_explicit_session_close": true,
    "session_ttl_seconds": 600,
    "max_page_size": 500,
    "page_size_parameter_name": "pageSize",
    "language_parameter_name": "LANGU",
    "cursor_parameter_name": "cursorToken",
    "next_cursor_path": "$.pagination.token",
    "supports_record_projection": true,
    "supports_rowset_projection": true,
    "supports_key_lookup": true,
    "supports_cursor_pagination": true,
    "fields": [
      {"name": "VBELN", "abap_type": "CHAR", "nullable": false},
      {"name": "NETWR", "abap_type": "CURR", "nullable": true}
    ]
  }
}"#,
                    )
                } else {
                    (
                        "/bridge/v1/health",
                        r#"{"status":"ok","ready":true,"version":"0.2.0","build":"ecc-rfc-bridge-build-2"}"#,
                    )
                };
                assert!(request.contains(expected_path));
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = socket.write_all(response.as_bytes()).await;
            }
        });

        let temp = tempdir().unwrap();
        let store = PersistedConnectorStore::open(temp.path().join("connectors.json"))
            .await
            .unwrap();
        let verification = CapturingVerification::default();
        let manager = EvidenceIngestionManager::new(
            store,
            Arc::new(CapturingForwarder::default()),
            Arc::new(verification.clone()),
            MigrationEvidenceDeliveryMode::Direct,
        );
        manager
            .upsert_connector(MigrationConnector {
                connector_id: "ecc-rfc-connector".to_string(),
                name: "sap ecc rfc verification".to_string(),
                vendor: MigrationConnectorVendor::SapEcc,
                role: MigrationConnectorRole::VerificationSource,
                transport: ConnectorTransport::SapEccRfcBapi,
                program_id: "program-1".to_string(),
                endpoint: ConnectorEndpoint {
                    base_url: format!("http://{}", addr),
                    path: "/bridge/v1/read/VBAK?record_id=500000001".to_string(),
                    method: "GET".to_string(),
                    headers: HashMap::new(),
                },
                auth: ConnectorAuth::default(),
                schedule: None,
                enabled: true,
                metadata: HashMap::new(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
            })
            .await
            .unwrap();

        manager
            .run_connector(
                "ecc-rfc-connector",
                DomainConnectorRunRequest {
                    run_label: None,
                    manual_events: vec![],
                    verification: Some(VerificationRequest {
                        control_name: "projection-match".to_string(),
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
                        expected_value: Some(Value::from(10)),
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
                            connection: HashMap::new(),
                        },
                    }),
                    request_body: None,
                    request_headers: HashMap::new(),
                },
            )
            .await
            .unwrap();

        let stored = manager.get_connector("ecc-rfc-connector").await.unwrap();
        assert_eq!(
            stored.metadata.get("ecc_rfc_bridge_version"),
            Some(&"0.2.0".to_string())
        );
        assert_eq!(
            stored.metadata.get("ecc_rfc_bapi_name"),
            Some(&"BAPI_SALESORDER_GETDETAIL".to_string())
        );
        assert_eq!(
            stored.metadata.get("ecc_rfc_function_module"),
            Some(&"RFC_READ_TABLE".to_string())
        );
        assert_eq!(
            stored.metadata.get("ecc_rfc_profile"),
            Some(&"function_module_export".to_string())
        );
        assert_eq!(
            stored.metadata.get("ecc_rfc_required_parameters_json"),
            Some(&r#"["record_id"]"#.to_string())
        );
        assert_eq!(
            stored.metadata.get("ecc_rfc_cursor_parameter_name"),
            Some(&"cursorToken".to_string())
        );
        assert_eq!(
            stored.metadata.get("ecc_rfc_supported_auth_modes_json"),
            Some(&r#"["destination"]"#.to_string())
        );
        assert_eq!(
            stored.metadata.get("ecc_rfc_supported_session_modes_json"),
            Some(&r#"["stateful"]"#.to_string())
        );
        assert_eq!(
            stored.metadata.get("ecc_rfc_session_id_path"),
            Some(&"$.session.id".to_string())
        );
        assert_eq!(
            stored.metadata.get("ecc_rfc_close_session_path"),
            Some(&"/bridge/v1/session/close".to_string())
        );
        assert_eq!(
            stored
                .metadata
                .get("ecc_rfc_requires_explicit_session_close"),
            Some(&"true".to_string())
        );
        assert_eq!(
            stored.metadata.get("ecc_rfc_session_ttl_seconds"),
            Some(&"600".to_string())
        );
        assert_eq!(
            stored.metadata.get("ecc_rfc_max_page_size"),
            Some(&"500".to_string())
        );
        assert_eq!(
            stored.metadata.get("ecc_rfc_language_parameter_name"),
            Some(&"LANGU".to_string())
        );
        assert_eq!(
            stored.metadata.get("ecc_rfc_health_status"),
            Some(&"ok".to_string())
        );
        assert_eq!(
            stored.metadata.get("ecc_rfc_health_ready"),
            Some(&"true".to_string())
        );

        let captured = verification.captured.lock().unwrap();
        let forwarded = captured
            .first()
            .expect("verification request should be captured");
        assert_eq!(
            forwarded
                .verification
                .source
                .connection
                .get("ecc_rfc_field_types_json"),
            Some(&r#"{"NETWR":"CURR","VBELN":"CHAR"}"#.to_string())
        );
        assert_eq!(
            forwarded
                .verification
                .source
                .connection
                .get("ecc_rfc_bapi_name"),
            Some(&"BAPI_SALESORDER_GETDETAIL".to_string())
        );
        assert_eq!(
            forwarded
                .verification
                .source
                .connection
                .get("ecc_rfc_profile"),
            Some(&"function_module_export".to_string())
        );
        assert_eq!(
            forwarded
                .verification
                .source
                .connection
                .get("ecc_rfc_supported_auth_modes_json"),
            Some(&r#"["destination"]"#.to_string())
        );
        assert_eq!(
            forwarded
                .verification
                .source
                .connection
                .get("ecc_rfc_language_parameter_name"),
            Some(&"LANGU".to_string())
        );
    }

    #[tokio::test]
    async fn sap_idoc_extractor_package_inline_bundle_emits_canonical_events() {
        let temp = tempdir().unwrap();
        let store = PersistedConnectorStore::open(temp.path().join("connectors.json"))
            .await
            .unwrap();
        let forwarder = CapturingForwarder::default();
        let manager = EvidenceIngestionManager::new(
            store,
            Arc::new(forwarder.clone()),
            Arc::new(StubVerification),
            MigrationEvidenceDeliveryMode::Direct,
        );

        manager
            .upsert_connector(MigrationConnector {
                connector_id: "idoc-inline".to_string(),
                name: "sap idoc extractor inline".to_string(),
                vendor: MigrationConnectorVendor::SapEcc,
                role: MigrationConnectorRole::MigrationArtifactSource,
                transport: ConnectorTransport::SapIdocExtractorPackage,
                program_id: "program-idoc-1".to_string(),
                endpoint: ConnectorEndpoint {
                    base_url: String::new(),
                    path: "inline-bundle".to_string(),
                    method: "POST".to_string(),
                    headers: HashMap::new(),
                },
                auth: ConnectorAuth::default(),
                schedule: None,
                enabled: true,
                metadata: HashMap::new(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
            })
            .await
            .unwrap();

        let inline_docs = serde_json::json!([
            {"DOCNUM": "000000000000001", "SEGMENT": "E1EDK01", "BELNR": "900000001"},
            {"DOCNUM": "000000000000002", "SEGMENT": "E1EDK01", "BELNR": "900000002"}
        ]);
        let inline_sha = sha256_hex(&serde_json::to_vec(&inline_docs).unwrap());
        let bundle = SapIdocExtractorBundle {
            manifest: SapIdocExtractorManifest {
                schema_version: "1.0".to_string(),
                package_id: "idoc-package-1".to_string(),
                program_id: "program-idoc-1".to_string(),
                object_id: "object-idoc-orders".to_string(),
                object_name: "ORDERS05".to_string(),
                source_system_id: "ECC-PRD".to_string(),
                source_client: "100".to_string(),
                extractor_family: SapExtractorFamily::Idoc,
                extractor_name: "control-m-extractor".to_string(),
                extractor_run_id: "run-1".to_string(),
                extracted_at: Utc::now(),
                extractor_object: None,
                extractor_context: None,
                extraction_mode: None,
                delta_token: None,
                subscriber_name: None,
                queue_name: None,
                idoc_type: Some("ORDERS05".to_string()),
                message_type: Some("ORDERS".to_string()),
                segment_counts: [("E1EDK01".to_string(), 2u64)].into_iter().collect(),
                data_set: Some(SapIdocExtractorDataSet {
                    format: SapIdocExtractorDataFormat::JsonDocuments,
                    path: None,
                    inline_payload: Some(inline_docs.to_string()),
                    expected_row_count: Some(2),
                    sha256: Some(inline_sha),
                }),
            },
            executions: vec![],
            exceptions: vec![],
            controls: vec![],
            approvals: vec![],
        };

        let (summary, events) = manager
            .run_connector(
                "idoc-inline",
                DomainConnectorRunRequest {
                    run_label: Some("idoc-wave-1".to_string()),
                    manual_events: vec![],
                    verification: None,
                    request_body: Some(serde_json::to_value(bundle).unwrap()),
                    request_headers: HashMap::new(),
                },
            )
            .await
            .unwrap();

        assert_eq!(summary.ingested_event_count, events.len());
        let integrity = events
            .iter()
            .find(|event| {
                event.artifact_type == MigrationEvidenceArtifactType::ControlResult
                    && event.payload.get("control_name")
                        == Some(&Value::String("sap_idoc_extractor_integrity".to_string()))
            })
            .expect("integrity control should be emitted");
        assert_eq!(
            integrity
                .payload
                .get("metadata")
                .and_then(|metadata| metadata.get("checksum_verified"))
                .and_then(Value::as_str),
            Some("true")
        );
        assert_eq!(
            integrity
                .payload
                .get("metadata")
                .and_then(|metadata| metadata.get("idoc_type"))
                .and_then(Value::as_str),
            Some("ORDERS05")
        );
        assert!(forwarder.captured.lock().unwrap().len() >= 3);
    }

    #[tokio::test]
    async fn sap_odp_extractor_package_inline_bundle_emits_canonical_events() {
        let temp = tempdir().unwrap();
        let store = PersistedConnectorStore::open(temp.path().join("connectors.json"))
            .await
            .unwrap();
        let forwarder = CapturingForwarder::default();
        let manager = EvidenceIngestionManager::new(
            store,
            Arc::new(forwarder.clone()),
            Arc::new(StubVerification),
            MigrationEvidenceDeliveryMode::Direct,
        );

        manager
            .upsert_connector(MigrationConnector {
                connector_id: "odp-inline".to_string(),
                name: "sap odp extractor inline".to_string(),
                vendor: MigrationConnectorVendor::SapEcc,
                role: MigrationConnectorRole::MigrationArtifactSource,
                transport: ConnectorTransport::SapOdpExtractorPackage,
                program_id: "program-odp-1".to_string(),
                endpoint: ConnectorEndpoint {
                    base_url: String::new(),
                    path: "inline-bundle".to_string(),
                    method: "POST".to_string(),
                    headers: HashMap::new(),
                },
                auth: ConnectorAuth::default(),
                schedule: None,
                enabled: true,
                metadata: HashMap::new(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
            })
            .await
            .unwrap();

        let inline_rows = serde_json::json!([
            {"VBELN": "500000001", "NETWR": "125.50", "WAERK": "USD"},
            {"VBELN": "500000002", "NETWR": "130.00", "WAERK": "USD"}
        ]);
        let inline_sha = sha256_hex(&serde_json::to_vec(&inline_rows).unwrap());
        let bundle = SapIdocExtractorBundle {
            manifest: SapIdocExtractorManifest {
                schema_version: "1.0".to_string(),
                package_id: "odp-package-1".to_string(),
                program_id: "program-odp-1".to_string(),
                object_id: "object-odp-vbak".to_string(),
                object_name: "VBAK".to_string(),
                source_system_id: "ECC-PRD".to_string(),
                source_client: "100".to_string(),
                extractor_family: SapExtractorFamily::Odp,
                extractor_name: "odq-customer-open-items".to_string(),
                extractor_run_id: "run-odp-1".to_string(),
                extracted_at: Utc::now(),
                extractor_object: Some("2LIS_11_VAHDR".to_string()),
                extractor_context: Some("SAPI".to_string()),
                extraction_mode: Some(graphica_core::migration_evidence::SapExtractorMode::Delta),
                delta_token: Some("delta-token-1".to_string()),
                subscriber_name: Some("ARCXA_DEMO".to_string()),
                queue_name: Some("ODQ_QUEUE_001".to_string()),
                idoc_type: None,
                message_type: None,
                segment_counts: BTreeMap::new(),
                data_set: Some(SapIdocExtractorDataSet {
                    format: SapIdocExtractorDataFormat::JsonDocuments,
                    path: None,
                    inline_payload: Some(inline_rows.to_string()),
                    expected_row_count: Some(2),
                    sha256: Some(inline_sha),
                }),
            },
            executions: vec![],
            exceptions: vec![],
            controls: vec![],
            approvals: vec![],
        };

        let (summary, events) = manager
            .run_connector(
                "odp-inline",
                DomainConnectorRunRequest {
                    run_label: Some("odp-wave-1".to_string()),
                    manual_events: vec![],
                    verification: None,
                    request_body: Some(serde_json::to_value(bundle).unwrap()),
                    request_headers: HashMap::new(),
                },
            )
            .await
            .unwrap();

        assert_eq!(summary.ingested_event_count, events.len());
        let integrity = events
            .iter()
            .find(|event| {
                event.artifact_type == MigrationEvidenceArtifactType::ControlResult
                    && event.payload.get("control_name")
                        == Some(&Value::String("sap_odp_extractor_integrity".to_string()))
            })
            .expect("ODP integrity control should be emitted");
        assert_eq!(
            integrity
                .payload
                .get("metadata")
                .and_then(|metadata| metadata.get("extractor_family"))
                .and_then(Value::as_str),
            Some("odp")
        );
        assert_eq!(
            integrity
                .payload
                .get("metadata")
                .and_then(|metadata| metadata.get("queue_name"))
                .and_then(Value::as_str),
            Some("ODQ_QUEUE_001")
        );

        let execution = events
            .iter()
            .find(|event| event.artifact_type == MigrationEvidenceArtifactType::ExecutionEvent)
            .expect("ODP execution event should exist");
        assert_eq!(
            execution.payload.get("tool_name"),
            Some(&Value::String("sap_odp_extractor_package".to_string()))
        );
    }

    #[test]
    fn odp_delta_extractor_requires_delta_token_or_subscriber_queue_context() {
        let manifest = SapIdocExtractorManifest {
            schema_version: "1.0".to_string(),
            package_id: "odp-package-invalid".to_string(),
            program_id: "program-odp-1".to_string(),
            object_id: "object-odp".to_string(),
            object_name: "VBAK".to_string(),
            source_system_id: "ECC-PRD".to_string(),
            source_client: "100".to_string(),
            extractor_family: SapExtractorFamily::Odp,
            extractor_name: "odq-vbak".to_string(),
            extractor_run_id: "run-odp-invalid".to_string(),
            extracted_at: Utc::now(),
            extractor_object: Some("2LIS_11_VAHDR".to_string()),
            extractor_context: Some("SAPI".to_string()),
            extraction_mode: Some(graphica_core::migration_evidence::SapExtractorMode::Delta),
            delta_token: None,
            subscriber_name: Some("ONLY_SUBSCRIBER".to_string()),
            queue_name: None,
            idoc_type: None,
            message_type: None,
            segment_counts: BTreeMap::new(),
            data_set: Some(SapIdocExtractorDataSet {
                format: SapIdocExtractorDataFormat::JsonDocuments,
                inline_payload: Some("[]".to_string()),
                path: None,
                expected_row_count: Some(0),
                sha256: Some(sha256_hex(b"[]")),
            }),
        };

        let error = validate_idoc_extractor_manifest(
            &manifest,
            &ConnectorTransport::SapOdpExtractorPackage,
            &SapExtractorFamily::Odp,
        )
        .expect_err("ODP delta manifests should fail without complete delta context");
        assert!(error
            .to_string()
            .contains("subscriber_name and queue_name together"));
    }

    #[test]
    fn idoc_extractor_requires_message_identity() {
        let manifest = SapIdocExtractorManifest {
            schema_version: "1.0".to_string(),
            package_id: "idoc-package-invalid".to_string(),
            program_id: "program-idoc-1".to_string(),
            object_id: "object-idoc".to_string(),
            object_name: "generic-idoc".to_string(),
            source_system_id: "ECC-PRD".to_string(),
            source_client: "100".to_string(),
            extractor_family: SapExtractorFamily::Idoc,
            extractor_name: "idoc-export".to_string(),
            extractor_run_id: "run-idoc-invalid".to_string(),
            extracted_at: Utc::now(),
            extractor_object: None,
            extractor_context: None,
            extraction_mode: Some(graphica_core::migration_evidence::SapExtractorMode::Snapshot),
            delta_token: None,
            subscriber_name: None,
            queue_name: None,
            idoc_type: None,
            message_type: None,
            segment_counts: BTreeMap::new(),
            data_set: Some(SapIdocExtractorDataSet {
                format: SapIdocExtractorDataFormat::JsonDocuments,
                inline_payload: Some("[]".to_string()),
                path: None,
                expected_row_count: Some(0),
                sha256: Some(sha256_hex(b"[]")),
            }),
        };

        let error = validate_idoc_extractor_manifest(
            &manifest,
            &ConnectorTransport::SapIdocExtractorPackage,
            &SapExtractorFamily::Idoc,
        )
        .expect_err("IDoc manifests should require a message identity");
        assert!(error
            .to_string()
            .contains("require idoc_type or message_type"));
    }
}
