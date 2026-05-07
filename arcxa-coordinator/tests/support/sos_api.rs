#![allow(dead_code, deprecated)]

use axum::{
    body::{to_bytes, Body},
    http::{Method, Request, Response, StatusCode},
    Router,
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
            storage::{Contract, Interface, SosStorageManager, System},
        },
        ApiState,
    },
    governance::{rdf_store::GraphicaRdfStore, RdfStore},
    storage::LineageStorage,
};
use graphica_core::secrets::providers::{InlineSecretStore, SecretStoreRegistry};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use std::{collections::HashMap, sync::Arc};
use tempfile::TempDir;

pub const PROVIDER_SYSTEM_ID: &str = "provider-system";
pub const CONSUMER_SYSTEM_ID: &str = "consumer-system";
pub const PROVIDER_INTERFACE_ID: &str = "provider-if";
pub const CONSUMER_INTERFACE_ID: &str = "consumer-if";
pub const CONTRACT_ID: &str = "provider-consumer-contract";
pub const INTERFACE_PAIR_SUBJECT_KEY: &str = "interface_pair:provider-if:consumer-if";
pub const PROVIDER_INTERFACE_SUBJECT_KEY: &str = "interface:provider-if";
const TEST_AUTH_SECRET: [u8; 32] = [
    0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0x10, 0x32, 0x54, 0x76, 0x98, 0xba, 0xdc, 0xfe,
    0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0x11, 0x22,
];

pub struct SosApiHarness {
    pub _temp_dir: TempDir,
    pub storage_manager: Arc<SosStorageManager>,
    pub rdf_store: Arc<GraphicaRdfStore>,
    pub app: Router,
    pub token: Option<String>,
    pub secret_store_registry: Option<Arc<SecretStoreRegistry>>,
}

pub fn setup_direct_app() -> SosApiHarness {
    let (temp_dir, storage_manager, rdf_store, state) =
        setup_state(Arc::new(AuthConfig::disabled()), None);
    let app = create_router().with_state(Arc::new(state));

    SosApiHarness {
        _temp_dir: temp_dir,
        storage_manager,
        rdf_store,
        app,
        token: None,
        secret_store_registry: None,
    }
}

pub fn setup_authenticated_build_router_app() -> SosApiHarness {
    setup_authenticated_build_router_app_with_secret_store(None)
}

pub fn setup_authenticated_build_router_app_with_inline_secret_store() -> SosApiHarness {
    let registry = Arc::new(SecretStoreRegistry::new());
    let store = Arc::new(InlineSecretStore::new());
    registry.register("default", store.clone());
    registry.set_default(store);
    setup_authenticated_build_router_app_with_secret_store(Some(registry))
}

fn setup_authenticated_build_router_app_with_secret_store(
    secret_store_registry: Option<Arc<SecretStoreRegistry>>,
) -> SosApiHarness {
    let auth_config = Arc::new(
        AuthConfig::from_secret_bytes(&TEST_AUTH_SECRET).expect("auth config should be created"),
    );
    let token = auth_config
        .generate_token("test_user", Role::Admin)
        .expect("token should be created");
    let (temp_dir, storage_manager, rdf_store, state) =
        setup_state(auth_config, secret_store_registry.clone());
    let app = build_router(state);

    SosApiHarness {
        _temp_dir: temp_dir,
        storage_manager,
        rdf_store,
        app,
        token: Some(token),
        secret_store_registry,
    }
}

pub fn issue_test_token(role: Role, subject: &str) -> String {
    AuthConfig::from_secret_bytes(&TEST_AUTH_SECRET)
        .expect("auth config should be created")
        .generate_token(subject, role)
        .expect("token should be created")
}

pub fn seed_minimal_catalog(storage_manager: &SosStorageManager) {
    storage_manager
        .put_system(&sample_system(
            PROVIDER_SYSTEM_ID,
            "Provider System",
            "provider.synthetic",
        ))
        .expect("provider system should be stored");
    storage_manager
        .put_system(&sample_system(
            CONSUMER_SYSTEM_ID,
            "Consumer System",
            "consumer.synthetic",
        ))
        .expect("consumer system should be stored");
    storage_manager
        .put_interface(&sample_interface(
            PROVIDER_INTERFACE_ID,
            PROVIDER_SYSTEM_ID,
            "Provider Telemetry API",
            "Provider",
        ))
        .expect("provider interface should be stored");
    storage_manager
        .put_interface(&sample_interface(
            CONSUMER_INTERFACE_ID,
            CONSUMER_SYSTEM_ID,
            "Consumer Telemetry API",
            "Consumer",
        ))
        .expect("consumer interface should be stored");
    storage_manager
        .put_contract(&sample_contract())
        .expect("contract should be stored");
}

pub fn interface_pair_validation_payload() -> Value {
    json!({
        "type": "interface_compatibility",
        "provider_interface_id": PROVIDER_INTERFACE_ID,
        "consumer_interface_id": CONSUMER_INTERFACE_ID,
    })
}

pub fn valid_data_payload() -> Value {
    json!({
        "sample_id": "sample-1",
        "score": 0.99,
    })
}

pub fn what_if_unit_change_payload() -> Value {
    json!({
        "scenario": "Consumer switches to Imperial units",
        "changes": [{
            "entity_type": "interface",
            "interface_id": CONSUMER_INTERFACE_ID,
            "system_id": CONSUMER_SYSTEM_ID,
            "unit_system": "Imperial",
        }],
    })
}

pub fn json_request(method: Method, uri: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .expect("request should be built")
}

pub fn authed_json_request(method: Method, uri: &str, token: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(body.to_string()))
        .expect("request should be built")
}

pub fn authed_empty_request(method: Method, uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .expect("request should be built")
}

pub fn empty_request(uri: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .body(Body::empty())
        .expect("request should be built")
}

pub async fn assert_json_response<T>(response: Response<Body>, expected: StatusCode) -> T
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

pub async fn assert_status(response: Response<Body>, expected: StatusCode) {
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

pub fn graph_triples(
    rdf_store: &GraphicaRdfStore,
    graph_uri: &str,
) -> Vec<(String, String, String)> {
    let mut triples = rdf_store
        .query(&format!(
            "SELECT ?subject ?predicate ?object WHERE {{ GRAPH <{graph_uri}> {{ ?subject ?predicate ?object }} }}"
        ))
        .expect("graph query should succeed")
        .into_iter()
        .map(|row| {
            let subject = row
                .get("subject")
                .and_then(Value::as_str)
                .expect("subject binding should be a string")
                .to_string();
            let predicate = row
                .get("predicate")
                .and_then(Value::as_str)
                .expect("predicate binding should be a string")
                .to_string();
            let object = row
                .get("object")
                .and_then(Value::as_str)
                .expect("object binding should be a string")
                .to_string();
            (subject, predicate, object)
        })
        .collect::<Vec<_>>();
    triples.sort();
    triples
}

pub fn subject_objects(
    rdf_store: &GraphicaRdfStore,
    graph_uri: &str,
    subject_uri: &str,
    predicate_uri: &str,
) -> Vec<String> {
    graph_triples(rdf_store, graph_uri)
        .into_iter()
        .filter(|(subject, predicate, _)| subject == subject_uri && predicate == predicate_uri)
        .map(|(_, _, object)| object)
        .collect()
}

pub fn subject_exists(rdf_store: &GraphicaRdfStore, graph_uri: &str, subject_uri: &str) -> bool {
    graph_triples(rdf_store, graph_uri)
        .into_iter()
        .any(|(subject, _, _)| subject == subject_uri)
}

fn setup_state(
    auth_config: Arc<AuthConfig>,
    secret_store_registry: Option<Arc<SecretStoreRegistry>>,
) -> (
    TempDir,
    Arc<SosStorageManager>,
    Arc<GraphicaRdfStore>,
    ApiState,
) {
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
    let rdf_store =
        Arc::new(GraphicaRdfStore::new_in_memory().expect("RDF store should be created"));

    let state = ApiState {
        lineage_storage: Arc::new(lineage_storage),
        governance_brain: None,
        rdf_store: Some(rdf_store.clone()),
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
        secret_store_registry,
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
        migration_evidence_gateway: None,
        discovery_orchestrator: None,
    };

    (temp_dir, sos_storage_manager, rdf_store, state)
}

fn sample_system(system_id: &str, system_name: &str, system_type: &str) -> System {
    let now = Utc::now();
    System {
        system_id: system_id.to_string(),
        system_name: system_name.to_string(),
        system_type: system_type.to_string(),
        vendor: "Graphica Test".to_string(),
        version: "1.0".to_string(),
        classification: "UNCLASSIFIED".to_string(),
        description: Some(format!("Synthetic SoS API test system {system_id}")),
        deployment: HashMap::new(),
        capabilities: HashMap::new(),
        tags: vec!["api-test".to_string()],
        active: true,
        created_at: now,
        updated_at: now,
    }
}

fn sample_interface(
    interface_id: &str,
    system_id: &str,
    interface_name: &str,
    direction: &str,
) -> Interface {
    let now = Utc::now();
    Interface {
        interface_id: interface_id.to_string(),
        system_id: system_id.to_string(),
        interface_name: interface_name.to_string(),
        direction: direction.to_string(),
        protocol: "REST".to_string(),
        data_format: "JSON".to_string(),
        schema: sample_schema(),
        coordinate_system: Some("WGS84".to_string()),
        unit_system: Some("SI".to_string()),
        metadata: HashMap::new(),
        created_at: now,
        updated_at: now,
    }
}

fn sample_contract() -> Contract {
    let now = Utc::now();
    Contract {
        contract_id: CONTRACT_ID.to_string(),
        revision: 1,
        contract_name: "Provider Consumer Contract".to_string(),
        provider_interface_id: PROVIDER_INTERFACE_ID.to_string(),
        consumer_interface_id: CONSUMER_INTERFACE_ID.to_string(),
        sla_metrics: Vec::new(),
        transformation_rules: HashMap::new(),
        description: Some("Synthetic API test contract".to_string()),
        tags: vec!["api-test".to_string()],
        approved: true,
        signed: true,
        lifecycle_state: Some("signed".to_string()),
        approval_status: Some("approved".to_string()),
        approval_requested_by: Some("system".to_string()),
        approval_requested_at: Some(now.clone()),
        approved_by: Some("system".to_string()),
        approved_at: Some(now.clone()),
        rejected_by: None,
        rejected_at: None,
        rejection_reason: None,
        signed_by: Some("system".to_string()),
        signed_at: Some(now.clone()),
        created_by: "system".to_string(),
        updated_by: "system".to_string(),
        superseded_by_revision: None,
        created_at: now.clone(),
        updated_at: now,
    }
}

fn sample_schema() -> Value {
    json!({
        "type": "object",
        "required": ["sample_id", "score"],
        "properties": {
            "sample_id": {"type": "string"},
            "score": {"type": "number"}
        },
        "additionalProperties": false,
    })
}
