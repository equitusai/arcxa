#![allow(deprecated)]

use arcxa_evidence_ingestion::{
    EvidenceIngestionManager, EvidenceIngestionServiceImpl, GrpcTraceabilityForwarder,
    GrpcVerificationForwarder, PersistedConnectorStore,
};
use arcxa_traceability::{
    EventBusRuntimeMonitor, GraphProjectionConfig, PersistedTraceabilityStore, TraceabilityManager,
    TraceabilityServiceImpl,
};
use arcxa_verification::{VerificationManager, VerificationServiceImpl};
use axum::{
    body::{to_bytes, Body},
    http::{Method, Request, Response, StatusCode},
    Router,
};
use chrono::{Duration, Utc};
use graphica_coordinator::{
    api::{
        auth::{AuthConfig, Role},
        import_jobs::ImportJobManager,
        migration_evidence::{
            EvidencePacketResponse, ExplainValueResponse, MigrationEvidenceErrorResponse,
            MigrationEvidenceGateway, MigrationEvidenceGatewayConfig,
            MigrationEvidenceRebuildResponse, MigrationEvidenceRemoteGatewayConfig,
            MigrationEvidenceRuntimeStatusResponse, ObjectControlsResponse,
            ProgramApprovalsResponse, ProgramExceptionsResponse, RunMigrationConnectorResponse,
            UpsertMigrationConnectorResponse,
        },
        rest::build_router,
        setup_token::SetupTokenManager,
        ApiState,
    },
    storage::LineageStorage,
};
use graphica_core::distributed::proto::migration_evidence::{
    evidence_ingestion_service_server::EvidenceIngestionServiceServer,
    traceability_service_server::TraceabilityServiceServer,
    verification_service_server::VerificationServiceServer,
};
use graphica_core::migration_evidence::{
    ApprovalEvent, ApprovalStatus, ConnectorAuth, ConnectorEndpoint, ConnectorRunRequest,
    ConnectorTransport, ControlStatus, ExceptionRecord, ExceptionSeverity, ExceptionStatus,
    ExecutionEvent, ExecutionStatus, GrpcMigrationEvidenceEventForwarder, MigrationConnector,
    MigrationConnectorRole, MigrationConnectorVendor, MigrationEvidenceArtifactType,
    MigrationEvidenceDeliveryMode, MigrationEvidenceEvent, MigrationObject, MigrationObjectType,
    MigrationProgram, SapEccStagedControlEvidence, SapEccStagedExceptionEvidence,
    SapEccStagedExportBundle, SapEccStagedExportDataFormat, SapEccStagedExportDataSet,
    SapEccStagedExportManifest, SapEccStagedRuleEvidence, SapExtractorFamily, SapExtractorMode,
    SapIdocExtractorBundle, SapIdocExtractorDataFormat, SapIdocExtractorDataSet,
    SapIdocExtractorManifest, SourceFieldRef, TargetFieldRef, TransformationRule,
    TransformationRuleType, VerificationRequest, VerificationSource,
};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};
use tempfile::TempDir;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
use tonic::transport::Server;
use tower::ServiceExt;

const TEST_AUTH_SECRET: [u8; 32] = [
    0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0x10, 0x32, 0x54, 0x76, 0x98, 0xba, 0xdc, 0xfe,
    0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0x11, 0x22,
];

struct Harness {
    _temp_dir: TempDir,
    token: String,
    app: Router,
}

#[tokio::test]
async fn migration_evidence_end_to_end_explains_a_value_over_authenticated_router() {
    let harness = setup_authenticated_app().await;

    let unauthenticated = harness
        .app
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/api/v1/migration-evidence/connectors",
            connector_payload(sample_artifact_connector()),
        ))
        .await
        .expect("request should complete");
    assert_status(unauthenticated, StatusCode::UNAUTHORIZED).await;

    let artifact_create = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            "/api/v1/migration-evidence/connectors",
            &harness.token,
            connector_payload(sample_artifact_connector()),
        ))
        .await
        .expect("artifact connector create should succeed");
    let artifact_connector: UpsertMigrationConnectorResponse =
        assert_json_response(artifact_create, StatusCode::OK).await;
    assert_eq!(artifact_connector.connector.connector_id, "ibm-artifacts");

    let artifact_run = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            "/api/v1/migration-evidence/connectors/ibm-artifacts/runs",
            &harness.token,
            json!(artifact_run_request()),
        ))
        .await
        .expect("artifact connector run should succeed");
    let artifact_run_response: RunMigrationConnectorResponse =
        assert_json_response(artifact_run, StatusCode::OK).await;
    assert_eq!(artifact_run_response.summary.connector_id, "ibm-artifacts");
    assert!(artifact_run_response.summary.ingested_event_count >= 6);
    assert_eq!(
        artifact_run_response.summary.delivery_mode,
        graphica_core::migration_evidence::MigrationEvidenceDeliveryMode::Direct
    );
    assert!(artifact_run_response.summary.traceability_acknowledged);

    let verification_endpoint = spawn_verification_source(json!({"actual_value": 103})).await;
    let verification_create = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            "/api/v1/migration-evidence/connectors",
            &harness.token,
            connector_payload(sample_verification_connector()),
        ))
        .await
        .expect("verification connector create should succeed");
    let verification_connector: UpsertMigrationConnectorResponse =
        assert_json_response(verification_create, StatusCode::OK).await;
    assert_eq!(
        verification_connector.connector.connector_id,
        "sap-verification"
    );

    let verification_run = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            "/api/v1/migration-evidence/connectors/sap-verification/runs",
            &harness.token,
            verification_run_payload(&verification_endpoint),
        ))
        .await
        .expect("verification connector run should succeed");
    let verification_run_response: RunMigrationConnectorResponse =
        assert_json_response(verification_run, StatusCode::OK).await;
    assert_eq!(
        verification_run_response.summary.connector_id,
        "sap-verification"
    );
    assert_eq!(verification_run_response.summary.ingested_event_count, 2);
    assert_eq!(
        verification_run_response.summary.delivery_mode,
        graphica_core::migration_evidence::MigrationEvidenceDeliveryMode::Direct
    );
    assert!(verification_run_response.summary.traceability_acknowledged);

    let explain = harness
        .app
        .clone()
        .oneshot(authed_empty_request(
            Method::GET,
            "/api/v1/migration-evidence/values/explain?program_id=program-rise-1&object_id=object-sales-order&target_field_path=$.amount&target_record_id=SO-1&source_record_id=SO-1",
            &harness.token,
        ))
        .await
        .expect("explain request should succeed");
    let explain_response: ExplainValueResponse =
        assert_json_response(explain, StatusCode::OK).await;

    assert_eq!(
        explain_response.explanation.source_field.field_name,
        "NETWR"
    );
    assert_eq!(
        explain_response.explanation.target_field.field_name,
        "NetAmount"
    );
    assert_eq!(
        explain_response
            .explanation
            .transformation_rule
            .as_ref()
            .map(|rule| rule.rule_id.as_str()),
        Some("rule-net-amount")
    );
    assert_eq!(
        explain_response
            .explanation
            .execution_event
            .as_ref()
            .map(|event| event.tool_name.as_str()),
        Some("ibm_rapid_move")
    );
    assert!(!explain_response.explanation.exceptions.is_empty());
    assert!(!explain_response.explanation.controls.is_empty());
    assert!(!explain_response.explanation.approvals.is_empty());
    assert!(explain_response.explanation.evidence_packet_id.is_some());

    let packet = harness
        .app
        .clone()
        .oneshot(authed_empty_request(
            Method::GET,
            "/api/v1/migration-evidence/objects/object-sales-order/evidence-packet?value_key=$.amount",
            &harness.token,
        ))
        .await
        .expect("evidence packet request should succeed");
    let packet_response: EvidencePacketResponse =
        assert_json_response(packet, StatusCode::OK).await;
    assert_eq!(
        packet_response.packet.packet_id,
        explain_response
            .explanation
            .evidence_packet_id
            .as_deref()
            .expect("explanation should reference an evidence packet")
    );
    let signature = packet_response
        .packet
        .signature
        .as_ref()
        .expect("packet should be signed");
    assert!(arcxa_traceability::verify_evidence_packet_signature(
        &packet_response.packet,
        signature
    ));

    let controls = harness
        .app
        .clone()
        .oneshot(authed_empty_request(
            Method::GET,
            "/api/v1/migration-evidence/objects/object-sales-order/controls",
            &harness.token,
        ))
        .await
        .expect("controls request should succeed");
    let controls_response: ObjectControlsResponse =
        assert_json_response(controls, StatusCode::OK).await;
    assert_eq!(controls_response.controls.len(), 1);
    assert_eq!(controls_response.controls[0].status, ControlStatus::Passed);

    let exceptions = harness
        .app
        .clone()
        .oneshot(authed_empty_request(
            Method::GET,
            "/api/v1/migration-evidence/programs/program-rise-1/exceptions",
            &harness.token,
        ))
        .await
        .expect("exceptions request should succeed");
    let exceptions_response: ProgramExceptionsResponse =
        assert_json_response(exceptions, StatusCode::OK).await;
    assert!(!exceptions_response.exceptions.is_empty());

    let approvals = harness
        .app
        .clone()
        .oneshot(authed_empty_request(
            Method::GET,
            "/api/v1/migration-evidence/programs/program-rise-1/approvals",
            &harness.token,
        ))
        .await
        .expect("approvals request should succeed");
    let approvals_response: ProgramApprovalsResponse =
        assert_json_response(approvals, StatusCode::OK).await;
    assert_eq!(approvals_response.approvals.len(), 1);
    assert_eq!(
        approvals_response.approvals[0].status,
        ApprovalStatus::Approved
    );

    let runtime_status = harness
        .app
        .clone()
        .oneshot(authed_empty_request(
            Method::GET,
            "/api/v1/migration-evidence/runtime/status",
            &harness.token,
        ))
        .await
        .expect("runtime status request should succeed");
    let runtime_status_response: MigrationEvidenceRuntimeStatusResponse =
        assert_json_response(runtime_status, StatusCode::OK).await;
    assert!(runtime_status_response.status.replay_supported);
    assert!(runtime_status_response.status.event_log_available);
    assert_eq!(
        runtime_status_response.status.event_bus.mode,
        graphica_core::migration_evidence::MigrationEvidenceEventBusMode::Direct
    );
    assert_eq!(
        runtime_status_response.status.event_bus.consumer_state,
        graphica_core::migration_evidence::MigrationEvidenceEventConsumerState::Disabled
    );
    assert!(
        !runtime_status_response
            .status
            .event_bus
            .async_delivery_enabled
    );
    assert!(runtime_status_response.status.read_models.event_log_entries >= 8);
    let ingestion_status = runtime_status_response
        .ingestion_status
        .expect("runtime status should include ingestion details");
    assert_eq!(
        ingestion_status.connector_store.backend,
        graphica_core::migration_evidence::ConnectorStoreBackend::RocksDb
    );
    assert_eq!(
        ingestion_status.connector_store.health,
        graphica_core::migration_evidence::ConnectorStoreHealth::Healthy
    );
    assert!(ingestion_status.connector_store.connector_count >= 2);
    assert_eq!(
        ingestion_status.delivery_mode,
        MigrationEvidenceDeliveryMode::Direct
    );

    let rebuild = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            "/api/v1/migration-evidence/runtime/rebuild",
            &harness.token,
            json!({}),
        ))
        .await
        .expect("runtime rebuild request should succeed");
    let rebuild_response: MigrationEvidenceRebuildResponse =
        assert_json_response(rebuild, StatusCode::OK).await;
    assert!(rebuild_response.summary.replayed_event_count >= 8);
}

#[tokio::test]
async fn migration_evidence_error_paths_return_not_found_and_bad_request() {
    let harness = setup_authenticated_app().await;

    let missing_packet = harness
        .app
        .clone()
        .oneshot(authed_empty_request(
            Method::GET,
            "/api/v1/migration-evidence/objects/missing-object/evidence-packet?value_key=$.amount",
            &harness.token,
        ))
        .await
        .expect("missing packet request should complete");
    let missing_payload: MigrationEvidenceErrorResponse =
        assert_json_response(missing_packet, StatusCode::NOT_FOUND).await;
    assert!(missing_payload.error.contains("unknown migration object"));

    let invalid_connector = harness
        .app
        .oneshot(authed_json_request(
            Method::POST,
            "/api/v1/migration-evidence/connectors",
            &harness.token,
            json!({
                "connector_id": "broken",
                "name": "Broken connector",
                "vendor": "ibm_rapid_move",
                "role": "migration_artifact_source",
                "transport": "http_json",
                "program_id": "",
                "endpoint": {
                    "base_url": "",
                    "path": "/artifacts",
                    "method": "GET",
                    "headers": {}
                },
                "auth": { "kind": "none" },
                "enabled": true,
                "metadata": {},
                "created_at": Utc::now(),
                "updated_at": Utc::now()
            }),
        ))
        .await
        .expect("invalid connector request should complete");
    let invalid_payload: MigrationEvidenceErrorResponse =
        assert_json_response(invalid_connector, StatusCode::BAD_REQUEST).await;
    assert!(invalid_payload.error.contains("program_id cannot be empty"));
}

#[tokio::test]
async fn migration_evidence_supports_s4_odata_verification_transport() {
    let harness = setup_authenticated_app().await;

    let mut connector = sample_verification_connector();
    connector.connector_id = "s4-odata-verification".to_string();
    connector.name = "SAP S/4 OData Verification Source".to_string();
    connector.vendor = MigrationConnectorVendor::SapS4;
    connector.transport = ConnectorTransport::SapS4OData;
    connector.endpoint.path = "/sap/opu/odata4/API_SALES_ORDER/A_SalesOrder?$top=1".to_string();

    let create = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            "/api/v1/migration-evidence/connectors",
            &harness.token,
            connector_payload(connector),
        ))
        .await
        .expect("s4 verification connector create should succeed");
    let created: UpsertMigrationConnectorResponse =
        assert_json_response(create, StatusCode::OK).await;
    assert_eq!(created.connector.transport, ConnectorTransport::SapS4OData);

    let verification_endpoint = spawn_verification_source(json!({
        "value": [
            {
                "SalesOrder": "SO-1",
                "NetAmount": "100.00",
                "TransactionCurrency": "USD"
            }
        ]
    }))
    .await;

    let run = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            "/api/v1/migration-evidence/connectors/s4-odata-verification/runs",
            &harness.token,
            json!(ConnectorRunRequest {
                run_label: Some("s4-odata-verify".to_string()),
                manual_events: vec![],
                verification: Some(VerificationRequest {
                    control_name: "sales-order-projection-match".to_string(),
                    program_id: "program-rise-1".to_string(),
                    object_id: "object-sales-order".to_string(),
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
                        "SalesOrder": "SO-1",
                        "NetAmount": 100.0,
                        "TransactionCurrency": "USD"
                    })),
                    tolerance: Some(0.0),
                    metadata: HashMap::from([(
                        "value_key".to_string(),
                        "SO-1::$.projection".to_string()
                    )]),
                    source: VerificationSource {
                        transport: ConnectorTransport::SapS4OData,
                        query: None,
                        endpoint: Some(ConnectorEndpoint {
                            base_url: verification_endpoint,
                            path: "/sap/opu/odata4/API_SALES_ORDER/A_SalesOrder?$top=1".to_string(),
                            method: "GET".to_string(),
                            headers: HashMap::new(),
                        }),
                        auth: ConnectorAuth::default(),
                        connection: HashMap::new(),
                    },
                }),
                request_body: None,
                request_headers: HashMap::new(),
            }),
        ))
        .await
        .expect("s4 odata verification run should succeed");
    let response: RunMigrationConnectorResponse = assert_json_response(run, StatusCode::OK).await;

    assert_eq!(response.summary.ingested_event_count, 2);
    let control_event = response
        .ingested_events
        .iter()
        .find(|event| event.get("artifact_type") == Some(&json!("control_result")))
        .expect("control event should be present");
    let metadata = control_event
        .get("payload")
        .and_then(|payload| payload.get("metadata"))
        .and_then(|metadata| metadata.as_object())
        .expect("control metadata should be present");
    assert_eq!(
        metadata.get("comparison_scope"),
        Some(&json!("record_projection"))
    );
    assert_eq!(metadata.get("verified_field_count"), Some(&json!("3")));
    assert_eq!(
        metadata.get("odata_projection_metadata_validated"),
        Some(&json!("true"))
    );
    assert_eq!(
        metadata.get("odata_entity_set"),
        Some(&json!("A_SalesOrder"))
    );
    assert_eq!(
        metadata.get("odata_requested_fields_json"),
        Some(&json!(
            "[\"NetAmount\",\"SalesOrder\",\"TransactionCurrency\"]"
        ))
    );
}

#[tokio::test]
async fn migration_evidence_supports_sap_ecc_adapter_verification_transport() {
    let harness = setup_authenticated_app().await;

    let mut connector = sample_verification_connector();
    connector.connector_id = "sap-ecc-adapter-verification".to_string();
    connector.name = "SAP ECC Adapter Verification Source".to_string();
    connector.vendor = MigrationConnectorVendor::SapEcc;
    connector.transport = ConnectorTransport::SapEccAdapter;
    connector.endpoint.path = "/adapter/v1/records/VBAK?record_id=500000001".to_string();

    let create = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            "/api/v1/migration-evidence/connectors",
            &harness.token,
            connector_payload(connector),
        ))
        .await
        .expect("sap ecc adapter verification connector create should succeed");
    let created: UpsertMigrationConnectorResponse =
        assert_json_response(create, StatusCode::OK).await;
    assert_eq!(
        created.connector.transport,
        ConnectorTransport::SapEccAdapter
    );

    let verification_endpoint = spawn_ecc_adapter_source(
        json!({
            "record": {
                "VBELN": "500000001",
                "NETWR": "100.00",
                "WAERK": "USD"
            }
        }),
        json!({
            "capabilities": {
                "adapter_version": "0.1.0",
                "system_id": "PRD",
                "client": "100",
                "object_name": "VBAK",
                "key_fields": ["VBELN"],
                "supports_record_projection": true,
                "supports_rowset_projection": true,
                "supports_key_lookup": true,
                "fields": [
                    {"name": "VBELN", "abap_type": "CHAR", "nullable": false},
                    {"name": "NETWR", "abap_type": "CURR", "nullable": true},
                    {"name": "WAERK", "abap_type": "CUKY", "nullable": true}
                ]
            }
        }),
    )
    .await;

    let run = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            "/api/v1/migration-evidence/connectors/sap-ecc-adapter-verification/runs",
            &harness.token,
            json!(ConnectorRunRequest {
                run_label: Some("sap-ecc-adapter-verify".to_string()),
                manual_events: vec![],
                verification: Some(VerificationRequest {
                    control_name: "vbak-projection-match".to_string(),
                    program_id: "program-rise-1".to_string(),
                    object_id: "object-sales-order".to_string(),
                    source_field: SourceFieldRef {
                        system: "SAP ECC".to_string(),
                        object_name: "VBAK".to_string(),
                        field_name: "projection".to_string(),
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
                        "NETWR": 100.0,
                        "WAERK": "USD"
                    })),
                    tolerance: Some(0.0),
                    metadata: HashMap::from([(
                        "value_key".to_string(),
                        "500000001::$.projection".to_string()
                    )]),
                    source: VerificationSource {
                        transport: ConnectorTransport::SapEccAdapter,
                        query: None,
                        endpoint: Some(ConnectorEndpoint {
                            base_url: verification_endpoint,
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
            }),
        ))
        .await
        .expect("sap ecc adapter verification run should succeed");
    let response: RunMigrationConnectorResponse = assert_json_response(run, StatusCode::OK).await;

    assert_eq!(response.summary.ingested_event_count, 2);
    let control_event = response
        .ingested_events
        .iter()
        .find(|event| event.get("artifact_type") == Some(&json!("control_result")))
        .expect("control event should be present");
    let metadata = control_event
        .get("payload")
        .and_then(|payload| payload.get("metadata"))
        .and_then(|metadata| metadata.as_object())
        .expect("control metadata should be present");
    assert_eq!(
        metadata.get("ecc_projection_metadata_validated"),
        Some(&json!("true"))
    );
    assert_eq!(metadata.get("ecc_object_name"), Some(&json!("VBAK")));
    assert_eq!(
        metadata.get("ecc_requested_fields_json"),
        Some(&json!("[\"NETWR\",\"VBELN\",\"WAERK\"]"))
    );
}

#[tokio::test]
async fn migration_evidence_supports_sap_ecc_rfc_bapi_verification_transport() {
    let harness = setup_authenticated_app().await;

    let mut connector = sample_verification_connector();
    connector.connector_id = "sap-ecc-rfc-verification".to_string();
    connector.name = "SAP ECC RFC/BAPI Verification Source".to_string();
    connector.vendor = MigrationConnectorVendor::SapEcc;
    connector.transport = ConnectorTransport::SapEccRfcBapi;
    connector.endpoint.path = "/bridge/v1/read/VBAK?record_id=500000001".to_string();

    let create = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            "/api/v1/migration-evidence/connectors",
            &harness.token,
            connector_payload(connector),
        ))
        .await
        .expect("sap ecc rfc verification connector create should succeed");
    let created: UpsertMigrationConnectorResponse =
        assert_json_response(create, StatusCode::OK).await;
    assert_eq!(
        created.connector.transport,
        ConnectorTransport::SapEccRfcBapi
    );

    let verification_endpoint = spawn_ecc_rfc_source(
        json!({
            "result": {
                "VBELN": "500000001",
                "NETWR": "100.00",
                "WAERK": "USD"
            }
        }),
        json!({
            "capabilities": {
                "bridge_version": "0.2.0",
                "system_id": "PRD",
                "client": "100",
                "function_module": "RFC_READ_TABLE",
                "bapi_name": "BAPI_SALESORDER_GETDETAIL",
                "export_structure": "ORDER_ITEMS_OUT",
                "key_fields": ["VBELN"],
                "supports_record_projection": true,
                "supports_rowset_projection": true,
                "supports_key_lookup": true,
                "supports_cursor_pagination": true,
                "fields": [
                    {"name": "VBELN", "abap_type": "CHAR", "nullable": false},
                    {"name": "NETWR", "abap_type": "CURR", "nullable": true},
                    {"name": "WAERK", "abap_type": "CUKY", "nullable": true}
                ]
            }
        }),
    )
    .await;

    let run = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            "/api/v1/migration-evidence/connectors/sap-ecc-rfc-verification/runs",
            &harness.token,
            json!(ConnectorRunRequest {
                run_label: Some("sap-ecc-rfc-verify".to_string()),
                manual_events: vec![],
                verification: Some(VerificationRequest {
                    control_name: "vbak-rfc-projection-match".to_string(),
                    program_id: "program-rise-1".to_string(),
                    object_id: "object-sales-order".to_string(),
                    source_field: SourceFieldRef {
                        system: "SAP ECC".to_string(),
                        object_name: "VBAK".to_string(),
                        field_name: "projection".to_string(),
                        field_path: "$.projection".to_string(),
                        semantic_type: None,
                        record_id: Some("500000001".to_string()),
                    },
                    target_field: TargetFieldRef {
                        system: "ARCXA ECC RFC".to_string(),
                        object_name: "VBAK".to_string(),
                        field_name: "projection".to_string(),
                        field_path: "$.projection".to_string(),
                        semantic_type: None,
                        record_id: Some("500000001".to_string()),
                    },
                    expected_value: Some(json!({
                        "VBELN": "500000001",
                        "NETWR": 100.0,
                        "WAERK": "USD"
                    })),
                    tolerance: Some(0.0),
                    metadata: HashMap::from([(
                        "value_key".to_string(),
                        "500000001::$.projection".to_string()
                    )]),
                    source: VerificationSource {
                        transport: ConnectorTransport::SapEccRfcBapi,
                        query: None,
                        endpoint: Some(ConnectorEndpoint {
                            base_url: verification_endpoint,
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
            }),
        ))
        .await
        .expect("sap ecc rfc verification run should succeed");
    let response: RunMigrationConnectorResponse = assert_json_response(run, StatusCode::OK).await;

    assert_eq!(response.summary.ingested_event_count, 2);
    let control_event = response
        .ingested_events
        .iter()
        .find(|event| event.get("artifact_type") == Some(&json!("control_result")))
        .expect("control event should be present");
    let metadata = control_event
        .get("payload")
        .and_then(|payload| payload.get("metadata"))
        .and_then(|metadata| metadata.as_object())
        .expect("control metadata should be present");
    assert_eq!(
        metadata.get("ecc_rfc_projection_metadata_validated"),
        Some(&json!("true"))
    );
    assert_eq!(
        metadata.get("ecc_rfc_bapi_name"),
        Some(&json!("BAPI_SALESORDER_GETDETAIL"))
    );
    assert_eq!(
        metadata.get("ecc_rfc_requested_fields_json"),
        Some(&json!("[\"NETWR\",\"VBELN\",\"WAERK\"]"))
    );
}

#[tokio::test]
async fn migration_evidence_supports_sap_ecc_staged_export_transport() {
    let harness = setup_authenticated_app().await;

    let mut connector = sample_artifact_connector();
    connector.connector_id = "sap-ecc-staged-export".to_string();
    connector.name = "SAP ECC Staged Export".to_string();
    connector.vendor = MigrationConnectorVendor::SapEcc;
    connector.transport = ConnectorTransport::SapEccStagedExport;
    connector.endpoint.base_url = String::new();
    connector.endpoint.path = "inline-bundle".to_string();

    let create = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            "/api/v1/migration-evidence/connectors",
            &harness.token,
            connector_payload(connector),
        ))
        .await
        .expect("sap ecc staged export connector create should succeed");
    let created: UpsertMigrationConnectorResponse =
        assert_json_response(create, StatusCode::OK).await;
    assert_eq!(
        created.connector.transport,
        ConnectorTransport::SapEccStagedExport
    );

    let run = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            "/api/v1/migration-evidence/connectors/sap-ecc-staged-export/runs",
            &harness.token,
            json!(ConnectorRunRequest {
                run_label: Some("ecc-wave-1".to_string()),
                manual_events: vec![],
                verification: None,
                request_body: Some(sap_ecc_staged_export_bundle_payload()),
                request_headers: HashMap::new(),
            }),
        ))
        .await
        .expect("sap ecc staged export run should succeed");
    let response: RunMigrationConnectorResponse = assert_json_response(run, StatusCode::OK).await;

    assert!(response.summary.ingested_event_count >= 6);
    let integrity_event = response
        .ingested_events
        .iter()
        .find(|event| {
            event.get("artifact_type") == Some(&json!("control_result"))
                && event
                    .get("payload")
                    .and_then(|payload| payload.get("control_name"))
                    == Some(&json!("sap_ecc_staged_export_integrity"))
        })
        .expect("integrity control event should be present");
    let metadata = integrity_event
        .get("payload")
        .and_then(|payload| payload.get("metadata"))
        .and_then(|metadata| metadata.as_object())
        .expect("integrity metadata should be present");
    assert_eq!(metadata.get("checksum_verified"), Some(&json!("true")));
    assert_eq!(metadata.get("actual_row_count"), Some(&json!("2")));
    assert_eq!(metadata.get("data_format"), Some(&json!("json_rows")));

    let execution_event = response
        .ingested_events
        .iter()
        .find(|event| event.get("artifact_type") == Some(&json!("execution_event")))
        .expect("execution event should be present");
    assert_eq!(
        execution_event
            .get("payload")
            .and_then(|payload| payload.get("tool_name")),
        Some(&json!("sap_ecc_staged_export"))
    );
}

#[tokio::test]
async fn migration_evidence_supports_sap_idoc_extractor_package_transport() {
    let harness = setup_authenticated_app().await;

    let mut connector = sample_artifact_connector();
    connector.connector_id = "sap-idoc-extractor".to_string();
    connector.name = "SAP IDoc Extractor Package".to_string();
    connector.vendor = MigrationConnectorVendor::SapEcc;
    connector.transport = ConnectorTransport::SapIdocExtractorPackage;
    connector.endpoint.base_url = String::new();
    connector.endpoint.path = "inline-bundle".to_string();

    let create = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            "/api/v1/migration-evidence/connectors",
            &harness.token,
            connector_payload(connector),
        ))
        .await
        .expect("sap idoc extractor connector create should succeed");
    let created: UpsertMigrationConnectorResponse =
        assert_json_response(create, StatusCode::OK).await;
    assert_eq!(
        created.connector.transport,
        ConnectorTransport::SapIdocExtractorPackage
    );

    let run = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            "/api/v1/migration-evidence/connectors/sap-idoc-extractor/runs",
            &harness.token,
            json!(ConnectorRunRequest {
                run_label: Some("idoc-wave-1".to_string()),
                manual_events: vec![],
                verification: None,
                request_body: Some(sap_idoc_extractor_bundle_payload()),
                request_headers: HashMap::new(),
            }),
        ))
        .await
        .expect("sap idoc extractor run should succeed");
    let response: RunMigrationConnectorResponse = assert_json_response(run, StatusCode::OK).await;

    assert!(response.summary.ingested_event_count >= 4);
    let integrity_event = response
        .ingested_events
        .iter()
        .find(|event| {
            event.get("artifact_type") == Some(&json!("control_result"))
                && event
                    .get("payload")
                    .and_then(|payload| payload.get("control_name"))
                    == Some(&json!("sap_idoc_extractor_integrity"))
        })
        .expect("integrity control event should be present");
    let metadata = integrity_event
        .get("payload")
        .and_then(|payload| payload.get("metadata"))
        .and_then(|metadata| metadata.as_object())
        .expect("integrity metadata should be present");
    assert_eq!(metadata.get("checksum_verified"), Some(&json!("true")));
    assert_eq!(metadata.get("actual_row_count"), Some(&json!("2")));
    assert_eq!(metadata.get("idoc_type"), Some(&json!("ORDERS05")));

    let execution_event = response
        .ingested_events
        .iter()
        .find(|event| event.get("artifact_type") == Some(&json!("execution_event")))
        .expect("execution event should be present");
    assert_eq!(
        execution_event
            .get("payload")
            .and_then(|payload| payload.get("tool_name")),
        Some(&json!("sap_idoc_extractor_package"))
    );
}

#[tokio::test]
async fn migration_evidence_supports_sap_odp_extractor_package_transport() {
    let harness = setup_authenticated_app().await;

    let mut connector = sample_artifact_connector();
    connector.connector_id = "sap-odp-extractor".to_string();
    connector.name = "SAP ODP Extractor Package".to_string();
    connector.vendor = MigrationConnectorVendor::SapEcc;
    connector.transport = ConnectorTransport::SapOdpExtractorPackage;
    connector.endpoint.base_url = String::new();
    connector.endpoint.path = "inline-bundle".to_string();

    let create = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            "/api/v1/migration-evidence/connectors",
            &harness.token,
            connector_payload(connector),
        ))
        .await
        .expect("sap odp extractor connector create should succeed");
    let created: UpsertMigrationConnectorResponse =
        assert_json_response(create, StatusCode::OK).await;
    assert_eq!(
        created.connector.transport,
        ConnectorTransport::SapOdpExtractorPackage
    );

    let run = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            "/api/v1/migration-evidence/connectors/sap-odp-extractor/runs",
            &harness.token,
            json!(ConnectorRunRequest {
                run_label: Some("odp-wave-1".to_string()),
                manual_events: vec![],
                verification: None,
                request_body: Some(sap_odp_extractor_bundle_payload()),
                request_headers: HashMap::new(),
            }),
        ))
        .await
        .expect("sap odp extractor run should succeed");
    let response: RunMigrationConnectorResponse = assert_json_response(run, StatusCode::OK).await;

    assert!(response.summary.ingested_event_count >= 4);
    let integrity_event = response
        .ingested_events
        .iter()
        .find(|event| {
            event.get("artifact_type") == Some(&json!("control_result"))
                && event
                    .get("payload")
                    .and_then(|payload| payload.get("control_name"))
                    == Some(&json!("sap_odp_extractor_integrity"))
        })
        .expect("ODP integrity control event should be present");
    let metadata = integrity_event
        .get("payload")
        .and_then(|payload| payload.get("metadata"))
        .and_then(|metadata| metadata.as_object())
        .expect("ODP integrity metadata should be present");
    assert_eq!(metadata.get("checksum_verified"), Some(&json!("true")));
    assert_eq!(metadata.get("actual_row_count"), Some(&json!("2")));
    assert_eq!(metadata.get("extractor_family"), Some(&json!("odp")));
    assert_eq!(metadata.get("queue_name"), Some(&json!("ODQ_QUEUE_001")));

    let execution_event = response
        .ingested_events
        .iter()
        .find(|event| event.get("artifact_type") == Some(&json!("execution_event")))
        .expect("ODP execution event should be present");
    assert_eq!(
        execution_event
            .get("payload")
            .and_then(|payload| payload.get("tool_name")),
        Some(&json!("sap_odp_extractor_package"))
    );
}

#[tokio::test]
async fn migration_evidence_remote_gateway_routes_verification_through_split_services() {
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let remote = spawn_remote_migration_evidence_stack(temp_dir.path()).await;
    let gateway = MigrationEvidenceGateway::new(MigrationEvidenceGatewayConfig {
        connector_state_path: temp_dir.path().join("unused/connectors/state.json"),
        connector_rocksdb_path: Some(temp_dir.path().join("unused/connectors/rocksdb")),
        traceability_state_path: temp_dir.path().join("unused/traceability/state.json"),
        traceability_rocksdb_path: Some(temp_dir.path().join("unused/traceability/rocksdb")),
        signing_key_seed: [7u8; 32],
        shard_endpoint: None,
        event_bus: None,
        remote_services: Some(MigrationEvidenceRemoteGatewayConfig {
            evidence_ingestion_endpoint: remote.ingestion_endpoint.clone(),
            traceability_endpoint: remote.traceability_endpoint.clone(),
        }),
        secret_store_registry: None,
    })
    .await
    .expect("remote migration evidence gateway should be created");

    gateway
        .upsert_connector(sample_verification_connector())
        .await
        .expect("verification connector should be registered through remote gateway");

    let verification_endpoint = spawn_verification_source(json!({"actual_value": 10})).await;
    let (summary, events) = gateway
        .run_connector(
            "sap-verification",
            ConnectorRunRequest {
                run_label: Some("remote-verification".to_string()),
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
                            base_url: verification_endpoint,
                            path: "/verify".to_string(),
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
        .expect("remote verification connector run should succeed");

    assert_eq!(summary.ingested_event_count, 2);
    assert!(summary.traceability_acknowledged);
    assert_eq!(events.len(), 2);

    let controls = gateway
        .controls_for_object("object-1")
        .await
        .expect("remote traceability should return controls");
    assert_eq!(controls.len(), 1);
    assert_eq!(controls[0].status, ControlStatus::Passed);

    let runtime = gateway
        .runtime_status()
        .await
        .expect("remote traceability should return runtime status");
    assert!(runtime.read_models.event_log_entries >= 2);
    assert_eq!(
        runtime.event_bus.mode,
        graphica_core::migration_evidence::MigrationEvidenceEventBusMode::Direct
    );
    let ingestion_runtime = gateway
        .ingestion_runtime_status()
        .await
        .expect("remote ingestion should return runtime status");
    assert_eq!(
        ingestion_runtime.connector_store.backend,
        graphica_core::migration_evidence::ConnectorStoreBackend::RocksDb
    );
    assert_eq!(
        ingestion_runtime.delivery_mode,
        MigrationEvidenceDeliveryMode::Direct
    );
}

async fn setup_authenticated_app() -> Harness {
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let temp_path = temp_dir.path().to_str().expect("temp path should be valid");
    let rocks_path = format!("{temp_path}/lineage_rocks");
    let parquet_path = format!("{temp_path}/lineage_parquet");
    let cold_path = format!("{temp_path}/lineage_cold");
    let connector_state_path = temp_dir
        .path()
        .join("migration-evidence/connectors/state.json");
    let traceability_state_path = temp_dir
        .path()
        .join("migration-evidence/traceability/state.json");
    let traceability_rocksdb_path = temp_dir
        .path()
        .join("migration-evidence/traceability/rocksdb");

    let auth_config = Arc::new(
        AuthConfig::from_secret_bytes(&TEST_AUTH_SECRET).expect("auth config should be created"),
    );
    let token = auth_config
        .generate_token("migration-evidence-tester", Role::Admin)
        .expect("token should be created");

    let gateway = Arc::new(
        MigrationEvidenceGateway::new(MigrationEvidenceGatewayConfig {
            connector_state_path,
            connector_rocksdb_path: Some(
                temp_dir
                    .path()
                    .join("migration-evidence/connectors/rocksdb"),
            ),
            traceability_state_path,
            traceability_rocksdb_path: Some(traceability_rocksdb_path),
            signing_key_seed: [9u8; 32],
            shard_endpoint: None,
            event_bus: None,
            remote_services: None,
            secret_store_registry: None,
        })
        .await
        .expect("migration evidence gateway should be created"),
    );

    let lineage_storage =
        LineageStorage::new(&rocks_path, &parquet_path, &cold_path, "localhost:9092")
            .expect("lineage storage should be created");

    let state = ApiState {
        lineage_storage: Arc::new(lineage_storage),
        governance_brain: None,
        rdf_store: None,
        shard_registry: None,
        query_executor: None,
        workflow_engine: None,
        model_registry: None,
        model_cache: None,
        rule_executor: None,
        circuit_breakers: None,
        auth_config,
        user_service: None,
        setup_token_manager: Arc::new(SetupTokenManager::new()),
        audit_logger: None,
        datasource_catalog: None,
        datasource_catalog_impl: None,
        persisted_ontology_registry: None,
        ontology_registry: None,
        rdf_storage: None,
        connector_registry: None,
        resolved_entity_cache: None,
        metrics_registry: None,
        import_job_manager: Arc::new(ImportJobManager::new()),
        mapping_engine: None,
        secret_store_registry: None,
        loader_job_manager: None,
        unified_mapping_coordinator: None,
        binding_service: None,
        schedule_store: None,
        workflow_store: None,
        execution_store: None,
        stream_executor: None,
        file_library: None,
        transformer_registry: None,
        kafka_producer: None,
        http_client: None,
        lineage_generator: None,
        metrics: None,
        replay_coordinator: None,
        row_lineage_store: None,
        column_lineage_store: None,
        schema_evolution_store: None,
        manual_mapping_store: None,
        db2_pool: None,
        checkpoint_persistence: None,
        dlq_reader: None,
        dlq_reprocessor: None,
        dlq_stats_calculator: None,
        schema_version_store: None,
        policy_checker: None,
        execution_sync: None,
        approval_store: None,
        gdpr_coordinator: None,
        export_executor: None,
        progress_store: None,
        cancellation_manager: None,
        sos_storage_manager: None,
        migration_evidence_gateway: Some(gateway),
        discovery_state: None,
        discovery_orchestrator: None,
    };

    let app = build_router(state);

    Harness {
        _temp_dir: temp_dir,
        token,
        app,
    }
}

struct RemoteMigrationEvidenceStack {
    ingestion_endpoint: String,
    traceability_endpoint: String,
    #[allow(dead_code)]
    verification_endpoint: String,
}

async fn spawn_remote_migration_evidence_stack(
    root: &std::path::Path,
) -> RemoteMigrationEvidenceStack {
    let traceability_store = PersistedTraceabilityStore::open_rocksdb(
        root.join("remote/traceability/rocksdb"),
        Some(root.join("remote/traceability/state.json")),
    )
    .await
    .expect("traceability store should open");
    let traceability_manager = TraceabilityManager::new(
        traceability_store,
        ed25519_dalek::SigningKey::from_bytes(&[8u8; 32]),
        GraphProjectionConfig {
            shard_endpoint: None,
        },
        EventBusRuntimeMonitor::direct(),
    );
    let traceability_endpoint = spawn_grpc_service(move |incoming| {
        let manager = traceability_manager.clone();
        async move {
            Server::builder()
                .add_service(TraceabilityServiceServer::new(
                    TraceabilityServiceImpl::new(manager),
                ))
                .serve_with_incoming(incoming)
                .await
                .expect("traceability service should run");
        }
    })
    .await;

    let verification_manager = VerificationManager::new(Arc::new(
        GrpcMigrationEvidenceEventForwarder::new(traceability_endpoint.clone()),
    ));
    let verification_endpoint = spawn_grpc_service(move |incoming| {
        let manager = verification_manager.clone();
        async move {
            Server::builder()
                .add_service(VerificationServiceServer::new(
                    VerificationServiceImpl::new(manager),
                ))
                .serve_with_incoming(incoming)
                .await
                .expect("verification service should run");
        }
    })
    .await;

    let connector_store = PersistedConnectorStore::open_rocksdb(
        root.join("remote/connectors/rocksdb"),
        Some(root.join("remote/connectors/state.json")),
    )
    .await
    .expect("connector store should open");
    let ingestion_manager = EvidenceIngestionManager::new(
        connector_store,
        Arc::new(GrpcTraceabilityForwarder::new(
            traceability_endpoint.clone(),
        )),
        Arc::new(GrpcVerificationForwarder::new(
            verification_endpoint.clone(),
        )),
        MigrationEvidenceDeliveryMode::Direct,
    );
    let ingestion_endpoint = spawn_grpc_service(move |incoming| {
        let manager = ingestion_manager.clone();
        async move {
            Server::builder()
                .add_service(EvidenceIngestionServiceServer::new(
                    EvidenceIngestionServiceImpl::new(manager),
                ))
                .serve_with_incoming(incoming)
                .await
                .expect("evidence ingestion service should run");
        }
    })
    .await;

    RemoteMigrationEvidenceStack {
        ingestion_endpoint,
        traceability_endpoint,
        verification_endpoint,
    }
}

async fn spawn_grpc_service<F, Fut>(serve: F) -> String
where
    F: FnOnce(
            std::pin::Pin<
                Box<dyn futures::Stream<Item = Result<TcpStream, std::io::Error>> + Send + 'static>,
            >,
        ) -> Fut
        + Send
        + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("grpc listener should bind");
    let addr = listener
        .local_addr()
        .expect("grpc listener should expose an address");
    let incoming = Box::pin(async_stream::stream! {
        loop {
            match listener.accept().await {
                Ok((stream, _addr)) => yield Ok(stream),
                Err(error) => {
                    yield Err(error);
                    break;
                }
            }
        }
    });
    tokio::spawn(serve(incoming));
    format!("http://{}", addr)
}

fn sample_artifact_connector() -> MigrationConnector {
    MigrationConnector {
        connector_id: "ibm-artifacts".to_string(),
        name: "IBM Rapid Move Artifact Ingestion".to_string(),
        vendor: MigrationConnectorVendor::IbmRapidMove,
        role: MigrationConnectorRole::MigrationArtifactSource,
        transport: ConnectorTransport::HttpJson,
        program_id: "program-rise-1".to_string(),
        endpoint: ConnectorEndpoint {
            base_url: "https://ibm.example.test".to_string(),
            path: "/artifacts".to_string(),
            method: "POST".to_string(),
            headers: HashMap::new(),
        },
        auth: ConnectorAuth::default(),
        schedule: None,
        enabled: true,
        metadata: HashMap::from([
            ("engagement_type".to_string(), "rise_migration".to_string()),
            ("system_of_record".to_string(), "ibm_rapid_move".to_string()),
        ]),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

fn sample_verification_connector() -> MigrationConnector {
    MigrationConnector {
        connector_id: "sap-verification".to_string(),
        name: "SAP Verification Source".to_string(),
        vendor: MigrationConnectorVendor::SapHana,
        role: MigrationConnectorRole::VerificationSource,
        transport: ConnectorTransport::HttpJson,
        program_id: "program-rise-1".to_string(),
        endpoint: ConnectorEndpoint {
            base_url: "https://sap.example.test".to_string(),
            path: "/verify".to_string(),
            method: "GET".to_string(),
            headers: HashMap::new(),
        },
        auth: ConnectorAuth::default(),
        schedule: None,
        enabled: true,
        metadata: HashMap::from([("verification_scope".to_string(), "post-load".to_string())]),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

fn artifact_run_request() -> ConnectorRunRequest {
    let now = Utc::now();
    let value_key = Some("SO-1::$.amount".to_string());
    let program = MigrationProgram {
        program_id: "program-rise-1".to_string(),
        name: "RISE Wave 1".to_string(),
        customer_name: Some("Contoso Manufacturing".to_string()),
        source_landscape: Some("SAP ECC".to_string()),
        target_landscape: Some("SAP S/4HANA".to_string()),
        tags: vec!["ibm-rise".to_string(), "hana".to_string()],
        metadata: HashMap::new(),
        created_at: now,
        updated_at: now,
    };
    let object = MigrationObject {
        object_id: "object-sales-order".to_string(),
        program_id: "program-rise-1".to_string(),
        object_type: MigrationObjectType::BusinessObject,
        name: "SalesOrder".to_string(),
        description: Some("Migrated sales order total".to_string()),
        source_record_id: Some("SO-1".to_string()),
        target_record_id: Some("SO-1".to_string()),
        tags: vec!["critical-finance".to_string()],
        metadata: HashMap::from([("business_owner".to_string(), "order-to-cash".to_string())]),
    };
    let rule = TransformationRule {
        rule_id: "rule-net-amount".to_string(),
        rule_type: TransformationRuleType::Mapping,
        name: "Normalize net amount".to_string(),
        description: Some("Map ECC net value into the S/4HANA sales-order API".to_string()),
        source_fields: vec![SourceFieldRef {
            system: "SAP ECC".to_string(),
            object_name: "VBAK".to_string(),
            field_name: "NETWR".to_string(),
            field_path: "$.amount".to_string(),
            semantic_type: Some("currency_amount".to_string()),
            record_id: Some("SO-1".to_string()),
        }],
        target_fields: vec![TargetFieldRef {
            system: "SAP S/4HANA".to_string(),
            object_name: "A_SalesOrder".to_string(),
            field_name: "NetAmount".to_string(),
            field_path: "$.amount".to_string(),
            semantic_type: Some("currency_amount".to_string()),
            record_id: Some("SO-1".to_string()),
        }],
        expression: Some("NETWR * 1.0".to_string()),
        filter_predicate: None,
        default_value: None,
        aggregation: None,
        metadata: HashMap::from([("tool".to_string(), "IBM Rapid Move".to_string())]),
    };
    let execution = ExecutionEvent {
        execution_id: "exec-migrate-1".to_string(),
        program_id: "program-rise-1".to_string(),
        object_id: "object-sales-order".to_string(),
        connector_run_id: "ibm-run-1".to_string(),
        tool_name: "ibm_rapid_move".to_string(),
        tool_run_id: "rm-run-1".to_string(),
        stage: "load".to_string(),
        status: ExecutionStatus::Succeeded,
        happened_at: now - Duration::minutes(10),
        source_snapshot_ref: Some("ecc://wave1/vbak/SO-1".to_string()),
        target_snapshot_ref: Some("s4://wave1/a_salesorder/SO-1".to_string()),
        records_examined: Some(1),
        records_affected: Some(1),
        metadata: HashMap::new(),
    };
    let exception = ExceptionRecord {
        exception_id: "exception-1".to_string(),
        program_id: "program-rise-1".to_string(),
        object_id: "object-sales-order".to_string(),
        severity: ExceptionSeverity::Warning,
        status: ExceptionStatus::Accepted,
        category: "manual_adjustment".to_string(),
        message: "Cutover team accepted a minor rounding difference during dress rehearsal"
            .to_string(),
        source_value: Some(json!(100)),
        target_value: Some(json!(101)),
        remediation: Some("Documented for sign-off packet".to_string()),
        detected_at: now - Duration::minutes(8),
        resolved_at: Some(now - Duration::minutes(6)),
        metadata: HashMap::new(),
    };
    let approval = ApprovalEvent {
        approval_id: "approval-1".to_string(),
        program_id: "program-rise-1".to_string(),
        object_id: "object-sales-order".to_string(),
        approver_role: "data_owner".to_string(),
        approver_id: "owner-42".to_string(),
        status: ApprovalStatus::Approved,
        comment: Some("Approved after dress rehearsal evidence review".to_string()),
        approved_at: now - Duration::minutes(5),
        evidence_refs: vec!["urn:ibm:rapid-move:evidence:packet-1".to_string()],
        attestation_ref: Some("urn:arcxa:approval:attestation:1".to_string()),
        metadata: HashMap::new(),
    };

    ConnectorRunRequest {
        run_label: Some("ibm-rise-wave-1".to_string()),
        manual_events: vec![
            MigrationEvidenceEvent::new(
                "ibm-artifacts",
                "manual-run",
                MigrationConnectorVendor::IbmRapidMove,
                "program-rise-1",
                "object-sales-order",
                MigrationEvidenceArtifactType::Program,
                None,
                serde_json::to_value(program).expect("program should serialize"),
            ),
            MigrationEvidenceEvent::new(
                "ibm-artifacts",
                "manual-run",
                MigrationConnectorVendor::IbmRapidMove,
                "program-rise-1",
                "object-sales-order",
                MigrationEvidenceArtifactType::Object,
                None,
                serde_json::to_value(object).expect("object should serialize"),
            ),
            MigrationEvidenceEvent::new(
                "ibm-artifacts",
                "manual-run",
                MigrationConnectorVendor::IbmRapidMove,
                "program-rise-1",
                "object-sales-order",
                MigrationEvidenceArtifactType::TransformationRule,
                value_key.clone(),
                serde_json::to_value(rule).expect("rule should serialize"),
            ),
            MigrationEvidenceEvent::new(
                "ibm-artifacts",
                "manual-run",
                MigrationConnectorVendor::IbmRapidMove,
                "program-rise-1",
                "object-sales-order",
                MigrationEvidenceArtifactType::ExecutionEvent,
                value_key.clone(),
                serde_json::to_value(execution).expect("execution should serialize"),
            ),
            MigrationEvidenceEvent::new(
                "ibm-artifacts",
                "manual-run",
                MigrationConnectorVendor::IbmRapidMove,
                "program-rise-1",
                "object-sales-order",
                MigrationEvidenceArtifactType::ExceptionRecord,
                value_key.clone(),
                serde_json::to_value(exception).expect("exception should serialize"),
            ),
            MigrationEvidenceEvent::new(
                "ibm-artifacts",
                "manual-run",
                MigrationConnectorVendor::IbmRapidMove,
                "program-rise-1",
                "object-sales-order",
                MigrationEvidenceArtifactType::ApprovalEvent,
                None,
                serde_json::to_value(approval).expect("approval should serialize"),
            ),
        ],
        verification: None,
        request_body: None,
        request_headers: HashMap::new(),
    }
}

fn verification_run_payload(verification_endpoint: &str) -> Value {
    json!(ConnectorRunRequest {
        run_label: Some("sap-verification".to_string()),
        manual_events: vec![],
        verification: Some(VerificationRequest {
            control_name: "net-amount-reconciliation".to_string(),
            program_id: "program-rise-1".to_string(),
            object_id: "object-sales-order".to_string(),
            source_field: SourceFieldRef {
                system: "SAP ECC".to_string(),
                object_name: "VBAK".to_string(),
                field_name: "NETWR".to_string(),
                field_path: "$.amount".to_string(),
                semantic_type: Some("currency_amount".to_string()),
                record_id: Some("SO-1".to_string()),
            },
            target_field: TargetFieldRef {
                system: "SAP S/4HANA".to_string(),
                object_name: "A_SalesOrder".to_string(),
                field_name: "NetAmount".to_string(),
                field_path: "$.amount".to_string(),
                semantic_type: Some("currency_amount".to_string()),
                record_id: Some("SO-1".to_string()),
            },
            expected_value: Some(json!(100)),
            tolerance: Some(5.0),
            metadata: HashMap::from([("value_key".to_string(), "SO-1::$.amount".to_string())]),
            source: VerificationSource {
                transport: ConnectorTransport::HttpJson,
                query: None,
                endpoint: Some(ConnectorEndpoint {
                    base_url: verification_endpoint.to_string(),
                    path: "/verify".to_string(),
                    method: "GET".to_string(),
                    headers: HashMap::new(),
                }),
                auth: ConnectorAuth::default(),
                connection: HashMap::new(),
            },
        }),
        request_body: None,
        request_headers: HashMap::new(),
    })
}

fn sap_ecc_staged_export_bundle_payload() -> Value {
    let rows = json!([
        {"VBELN": "500000001", "NETWR": 125.5, "WAERK": "USD"},
        {"VBELN": "500000002", "NETWR": 130.0, "WAERK": "USD"}
    ]);
    let rows_sha = sha256_hex(&serde_json::to_vec(&rows).expect("rows should serialize"));

    serde_json::to_value(SapEccStagedExportBundle {
        manifest: SapEccStagedExportManifest {
            schema_version: "1.0".to_string(),
            export_id: "ecc-export-1".to_string(),
            program_id: "program-rise-1".to_string(),
            object_id: "object-sales-order".to_string(),
            object_name: "VBAK".to_string(),
            source_system_id: "ECC-PRD".to_string(),
            source_client: "100".to_string(),
            extracted_at: Utc::now(),
            key_fields: vec!["VBELN".to_string()],
            data_set: Some(SapEccStagedExportDataSet {
                format: SapEccStagedExportDataFormat::JsonRows,
                path: None,
                inline_payload: Some(rows),
                expected_row_count: Some(2),
                sha256: Some(rows_sha),
                metadata: HashMap::new(),
            }),
            metadata: HashMap::from([("cutover_wave".to_string(), "wave-1".to_string())]),
        },
        program: None,
        object: None,
        transformation_rules: vec![SapEccStagedRuleEvidence {
            value_key: Some("500000001::$.NetAmount".to_string()),
            rule: TransformationRule {
                rule_id: "rule-netwr".to_string(),
                rule_type: TransformationRuleType::Mapping,
                name: "NETWR to NetAmount".to_string(),
                description: None,
                source_fields: vec![SourceFieldRef {
                    system: "SAP ECC".to_string(),
                    object_name: "VBAK".to_string(),
                    field_name: "NETWR".to_string(),
                    field_path: "$.NETWR".to_string(),
                    semantic_type: Some("currency_amount".to_string()),
                    record_id: Some("500000001".to_string()),
                }],
                target_fields: vec![TargetFieldRef {
                    system: "SAP S/4HANA".to_string(),
                    object_name: "A_SalesOrder".to_string(),
                    field_name: "NetAmount".to_string(),
                    field_path: "$.NetAmount".to_string(),
                    semantic_type: Some("currency_amount".to_string()),
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
            value_key: Some("500000001::$.NetAmount".to_string()),
            exception: ExceptionRecord {
                exception_id: "exception-1".to_string(),
                program_id: "program-rise-1".to_string(),
                object_id: "object-sales-order".to_string(),
                severity: ExceptionSeverity::Warning,
                status: ExceptionStatus::Accepted,
                category: "rounding".to_string(),
                message: "Rounded during target harmonization".to_string(),
                source_value: None,
                target_value: None,
                remediation: None,
                detected_at: Utc::now(),
                resolved_at: None,
                metadata: HashMap::new(),
            },
        }],
        controls: vec![SapEccStagedControlEvidence {
            value_key: Some("500000001::$.NetAmount".to_string()),
            control: graphica_core::migration_evidence::ControlResult {
                control_id: "control-1".to_string(),
                program_id: "program-rise-1".to_string(),
                object_id: "object-sales-order".to_string(),
                control_name: "sample_record_reconciled".to_string(),
                control_type: "field_reconciliation".to_string(),
                status: ControlStatus::Passed,
                summary: "Sample record reconciled".to_string(),
                expected_value: Some(json!(125.5)),
                actual_value: Some(json!(125.5)),
                tolerance: Some(0.0),
                executed_at: Utc::now(),
                evidence_refs: vec![],
                metadata: HashMap::new(),
            },
        }],
        approvals: vec![],
    })
    .expect("bundle should serialize")
}

fn sap_idoc_extractor_bundle_payload() -> Value {
    let docs = json!([
        {"DOCNUM": "000000000000001", "SEGMENT": "E1EDK01", "BELNR": "900000001"},
        {"DOCNUM": "000000000000002", "SEGMENT": "E1EDK01", "BELNR": "900000002"}
    ]);
    let docs_sha = sha256_hex(docs.to_string().as_bytes());

    serde_json::to_value(SapIdocExtractorBundle {
        manifest: SapIdocExtractorManifest {
            schema_version: "1.0".to_string(),
            package_id: "idoc-package-1".to_string(),
            program_id: "program-rise-1".to_string(),
            object_id: "object-sales-order".to_string(),
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
                inline_payload: Some(docs.to_string()),
                expected_row_count: Some(2),
                sha256: Some(docs_sha),
            }),
        },
        executions: vec![],
        exceptions: vec![],
        controls: vec![],
        approvals: vec![],
    })
    .expect("IDoc bundle should serialize")
}

fn sap_odp_extractor_bundle_payload() -> Value {
    let rows = json!([
        {"VBELN": "500000001", "NETWR": "125.50", "WAERK": "USD"},
        {"VBELN": "500000002", "NETWR": "130.00", "WAERK": "USD"}
    ]);
    let rows_sha = sha256_hex(rows.to_string().as_bytes());

    serde_json::to_value(SapIdocExtractorBundle {
        manifest: SapIdocExtractorManifest {
            schema_version: "1.0".to_string(),
            package_id: "odp-package-1".to_string(),
            program_id: "program-rise-1".to_string(),
            object_id: "object-open-orders".to_string(),
            object_name: "2LIS_11_VAHDR".to_string(),
            source_system_id: "ECC-PRD".to_string(),
            source_client: "100".to_string(),
            extractor_family: SapExtractorFamily::Odp,
            extractor_name: "odq-sales-order-header".to_string(),
            extractor_run_id: "run-odp-1".to_string(),
            extracted_at: Utc::now(),
            extractor_object: Some("2LIS_11_VAHDR".to_string()),
            extractor_context: Some("SAPI".to_string()),
            extraction_mode: Some(SapExtractorMode::Delta),
            delta_token: Some("delta-token-1".to_string()),
            subscriber_name: Some("ARCXA_DEMO".to_string()),
            queue_name: Some("ODQ_QUEUE_001".to_string()),
            idoc_type: None,
            message_type: None,
            segment_counts: BTreeMap::new(),
            data_set: Some(SapIdocExtractorDataSet {
                format: SapIdocExtractorDataFormat::JsonDocuments,
                path: None,
                inline_payload: Some(rows.to_string()),
                expected_row_count: Some(2),
                sha256: Some(rows_sha),
            }),
        },
        executions: vec![],
        exceptions: vec![],
        controls: vec![],
        approvals: vec![],
    })
    .expect("ODP bundle should serialize")
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};

    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        out.push_str(&format!("{:02x}", byte));
    }
    out
}

async fn spawn_verification_source(payload: Value) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("local addr should resolve");
    let body = payload.to_string();
    let metadata_body = r#"<?xml version="1.0" encoding="utf-8"?>
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
</edmx:Edmx>"#
        .to_string();
    tokio::spawn(async move {
        for _ in 0..4 {
            if let Ok((mut socket, _)) = listener.accept().await {
                let mut buffer = vec![0u8; 4096];
                let bytes_read = socket.read(&mut buffer).await.unwrap_or(0);
                let request = String::from_utf8_lossy(&buffer[..bytes_read]);
                let (content_type, response_body) = if request.contains("$metadata") {
                    ("application/xml", metadata_body.clone())
                } else {
                    ("application/json", body.clone())
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: {}\r\ncontent-length: {}\r\n\r\n{}",
                    content_type,
                    response_body.len(),
                    response_body
                );
                let _ = socket.write_all(response.as_bytes()).await;
            } else {
                break;
            }
        }
    });
    format!("http://{}", addr)
}

async fn spawn_ecc_adapter_source(payload: Value, capabilities: Value) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("local addr should resolve");
    let body = payload.to_string();
    let capabilities_body = capabilities.to_string();
    tokio::spawn(async move {
        for _ in 0..4 {
            if let Ok((mut socket, _)) = listener.accept().await {
                let mut buffer = vec![0u8; 4096];
                let bytes_read = socket.read(&mut buffer).await.unwrap_or(0);
                let request = String::from_utf8_lossy(&buffer[..bytes_read]);
                let response_body = if request.contains("/capabilities") {
                    capabilities_body.clone()
                } else {
                    body.clone()
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                    response_body.len(),
                    response_body
                );
                let _ = socket.write_all(response.as_bytes()).await;
            } else {
                break;
            }
        }
    });
    format!("http://{}", addr)
}

async fn spawn_ecc_rfc_source(payload: Value, capabilities: Value) -> String {
    spawn_ecc_adapter_source(payload, capabilities).await
}

fn connector_payload(connector: MigrationConnector) -> Value {
    serde_json::to_value(connector).expect("connector should serialize")
}

fn json_request(method: Method, uri: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .expect("request should build")
}

fn authed_json_request(method: Method, uri: &str, token: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(body.to_string()))
        .expect("request should build")
}

fn authed_empty_request(method: Method, uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .expect("request should build")
}

async fn assert_json_response<T>(response: Response<Body>, expected: StatusCode) -> T
where
    T: DeserializeOwned,
{
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should be readable");
    assert_eq!(
        status,
        expected,
        "unexpected response body: {}",
        String::from_utf8_lossy(&body)
    );
    serde_json::from_slice(&body).expect("response should deserialize")
}

async fn assert_status(response: Response<Body>, expected: StatusCode) {
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should be readable");
    assert_eq!(
        status,
        expected,
        "unexpected response body: {}",
        String::from_utf8_lossy(&body)
    );
}
