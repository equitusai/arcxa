//! API-level tests for SoS contract lookup by interface pair.

use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use chrono::Utc;
use graphica_coordinator::{
    api::{
        auth::{AuthConfig, Role},
        import_jobs::ImportJobManager,
        rest::build_router,
        setup_token::SetupTokenManager,
        sos_validation::{
            create_router,
            storage::{Contract, SosStorageManager},
            types::{DataContractResponse, SosErrorResponse},
        },
        ApiState,
    },
    storage::LineageStorage,
};
use std::{collections::HashMap, sync::Arc};
use tempfile::TempDir;
use tower::ServiceExt;

fn setup_test_app() -> (TempDir, Arc<SosStorageManager>, axum::Router) {
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let temp_path = temp_dir.path().to_str().expect("temp path should be valid");
    let rocks_path = format!("{temp_path}/lineage_rocks");
    let parquet_path = format!("{temp_path}/lineage_parquet");
    let cold_path = format!("{temp_path}/lineage_cold");
    let sos_path = format!("{temp_path}/sos");

    let lineage_storage =
        LineageStorage::new(&rocks_path, &parquet_path, &cold_path, "localhost:9092")
            .expect("lineage storage should be created");
    let sos_storage_manager =
        Arc::new(SosStorageManager::new(&sos_path).expect("SoS storage manager should be created"));

    let state = Arc::new(ApiState {
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
        auth_config: Arc::new(AuthConfig::disabled()),
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
        sos_storage_manager: Some(sos_storage_manager.clone()),
        discovery_state: None,
        discovery_orchestrator: None,
    });

    let app = create_router().with_state(state);
    (temp_dir, sos_storage_manager, app)
}

fn setup_authenticated_build_router_app() -> (TempDir, Arc<SosStorageManager>, axum::Router, String)
{
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let temp_path = temp_dir.path().to_str().expect("temp path should be valid");
    let rocks_path = format!("{temp_path}/lineage_rocks");
    let parquet_path = format!("{temp_path}/lineage_parquet");
    let cold_path = format!("{temp_path}/lineage_cold");
    let sos_path = format!("{temp_path}/sos");

    let test_secret = [
        0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0x10, 0x32, 0x54, 0x76, 0x98, 0xba, 0xdc,
        0xfe, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef,
        0x11, 0x22,
    ];
    let auth_config = Arc::new(
        AuthConfig::from_secret_bytes(&test_secret).expect("auth config should be created"),
    );
    let token = auth_config
        .generate_token("test_user", Role::Admin)
        .expect("token should be created");

    let lineage_storage =
        LineageStorage::new(&rocks_path, &parquet_path, &cold_path, "localhost:9092")
            .expect("lineage storage should be created");
    let sos_storage_manager =
        Arc::new(SosStorageManager::new(&sos_path).expect("SoS storage manager should be created"));

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
        sos_storage_manager: Some(sos_storage_manager.clone()),
        discovery_state: None,
        discovery_orchestrator: None,
    };

    let app = build_router(state);
    (temp_dir, sos_storage_manager, app, token)
}

fn sample_contract(
    contract_id: &str,
    provider_interface_id: &str,
    consumer_interface_id: &str,
) -> Contract {
    let now = Utc::now();
    Contract {
        contract_id: contract_id.to_string(),
        revision: 1,
        contract_name: format!("Contract {contract_id}"),
        provider_interface_id: provider_interface_id.to_string(),
        consumer_interface_id: consumer_interface_id.to_string(),
        sla_metrics: Vec::new(),
        transformation_rules: HashMap::new(),
        description: Some("Synthetic contract".to_string()),
        tags: vec!["test".to_string()],
        approved: false,
        signed: false,
        lifecycle_state: Some("draft".to_string()),
        approval_status: Some("pending".to_string()),
        approval_requested_by: None,
        approval_requested_at: None,
        approved_by: None,
        approved_at: None,
        rejected_by: None,
        rejected_at: None,
        rejection_reason: None,
        signed_by: None,
        signed_at: None,
        created_by: "system".to_string(),
        updated_by: "system".to_string(),
        superseded_by_revision: None,
        created_at: now.clone(),
        updated_at: now,
    }
}

#[tokio::test]
async fn lookup_contract_returns_contract_for_interface_pair() {
    let (_temp_dir, storage_manager, app) = setup_test_app();
    storage_manager
        .put_contract(&sample_contract("contract-1", "provider-if", "consumer-if"))
        .expect("contract should be stored");

    let response = app
        .oneshot(
            Request::builder()
                .uri("/sos/contracts/lookup?provider_interface_id=provider-if&consumer_interface_id=consumer-if")
                .body(Body::empty())
                .expect("request should be built"),
        )
        .await
        .expect("request should succeed");

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should be readable");
    let contract: DataContractResponse =
        serde_json::from_slice(&body).expect("response should deserialize");

    assert_eq!(contract.contract_id, "contract-1");
    assert_eq!(contract.provider_interface_id, "provider-if");
    assert_eq!(contract.consumer_interface_id, "consumer-if");
}

#[tokio::test]
async fn lookup_contract_returns_not_found_when_pair_is_missing() {
    let (_temp_dir, _storage_manager, app) = setup_test_app();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/sos/contracts/lookup?provider_interface_id=provider-if&consumer_interface_id=consumer-if")
                .body(Body::empty())
                .expect("request should be built"),
        )
        .await
        .expect("request should succeed");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should be readable");
    let error: SosErrorResponse =
        serde_json::from_slice(&body).expect("error response should deserialize");

    assert_eq!(error.error, "CONTRACT_NOT_FOUND");
}

#[tokio::test]
async fn lookup_contract_rejects_empty_provider_interface_id() {
    let (_temp_dir, _storage_manager, app) = setup_test_app();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/sos/contracts/lookup?provider_interface_id=&consumer_interface_id=consumer-if")
                .body(Body::empty())
                .expect("request should be built"),
        )
        .await
        .expect("request should succeed");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should be readable");
    let error: SosErrorResponse =
        serde_json::from_slice(&body).expect("error response should deserialize");

    assert_eq!(error.error, "INVALID_REQUEST");
    assert!(error.message.contains("provider_interface_id"));
}

#[tokio::test]
async fn lookup_contract_via_build_router_with_auth_succeeds() {
    let (_temp_dir, storage_manager, app, token) = setup_authenticated_build_router_app();
    storage_manager
        .put_contract(&sample_contract(
            "contract-auth",
            "provider-if",
            "consumer-if",
        ))
        .expect("contract should be stored");

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/sos/contracts/lookup?provider_interface_id=provider-if&consumer_interface_id=consumer-if")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::empty())
                .expect("request should be built"),
        )
        .await
        .expect("request should succeed");

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should be readable");
    let contract: DataContractResponse =
        serde_json::from_slice(&body).expect("response should deserialize");

    assert_eq!(contract.contract_id, "contract-auth");
}

#[tokio::test]
async fn lookup_contract_via_build_router_requires_auth() {
    let (_temp_dir, _storage_manager, app, _token) = setup_authenticated_build_router_app();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/sos/contracts/lookup?provider_interface_id=provider-if&consumer_interface_id=consumer-if")
                .body(Body::empty())
                .expect("request should be built"),
        )
        .await
        .expect("request should succeed");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
