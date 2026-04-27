//! HTTP/router coverage for incremental SoS RDF projection on mutation paths.

#[path = "support/sos_api.rs"]
mod sos_api;

use axum::http::{Method, StatusCode};
use graphica_coordinator::api::sos_validation::types::{
    DataContractResponse, InterfaceResponse, SystemResponse, ValidationResponse,
};
use serde_json::json;
use sos_api::{
    assert_json_response, assert_status, authed_empty_request, authed_json_request,
    interface_pair_validation_payload, seed_minimal_catalog, setup_authenticated_build_router_app,
    subject_exists, subject_objects,
};
use tower::ServiceExt;

const CATALOG_GRAPH: &str = "http://graphica.io/graph/sos-catalog";
const VALIDATION_GRAPH: &str = "http://graphica.io/graph/sos-validations";
const SOS_NS: &str = "http://graphica.io/sos#";

fn system_uri(system_id: &str) -> String {
    format!("http://graphica.io/sos/system/{system_id}")
}

fn interface_uri(interface_id: &str) -> String {
    format!("http://graphica.io/sos/interface/{interface_id}")
}

fn contract_uri(contract_id: &str) -> String {
    format!("http://graphica.io/sos/contract/{contract_id}")
}

fn validation_activity_uri(validation_id: &str) -> String {
    format!("http://graphica.io/sos/validation/{validation_id}")
}

fn validation_report_uri(report_id: &str) -> String {
    format!("http://graphica.io/sos/validation-report/{report_id}")
}

async fn register_system(
    harness: &sos_api::SosApiHarness,
    token: &str,
    system_id: &str,
    system_name: &str,
) -> SystemResponse {
    let response = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            "/api/v1/sos/systems",
            token,
            json!({
                "system_id": system_id,
                "system_name": system_name,
                "system_type": "synthetic.system",
                "vendor": "Graphica Test",
                "version": "1.0.0",
                "classification": "UNCLASSIFIED",
                "description": format!("Synthetic system {system_id}"),
                "deployment": {},
                "capabilities": {},
                "tags": ["api-test"],
            }),
        ))
        .await
        .expect("system registration request should succeed");

    assert_json_response(response, StatusCode::OK).await
}

async fn register_interface(
    harness: &sos_api::SosApiHarness,
    token: &str,
    system_id: &str,
    interface_id: &str,
    interface_name: &str,
    direction: &str,
) -> InterfaceResponse {
    let response = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            "/api/v1/sos/interfaces",
            token,
            json!({
                "system_id": system_id,
                "interface_id": interface_id,
                "interface_name": interface_name,
                "direction": direction,
                "protocol": "REST",
                "data_format": "JSON",
                "schema": {
                    "type": "object",
                    "required": ["sample_id", "score"],
                    "properties": {
                        "sample_id": {"type": "string"},
                        "score": {"type": "number"}
                    },
                    "additionalProperties": false
                },
                "coordinate_system": "WGS84",
                "unit_system": "SI",
                "metadata": {},
            }),
        ))
        .await
        .expect("interface registration request should succeed");

    assert_json_response(response, StatusCode::OK).await
}

async fn create_contract(
    harness: &sos_api::SosApiHarness,
    token: &str,
    contract_id: &str,
    provider_interface_id: &str,
    consumer_interface_id: &str,
) -> DataContractResponse {
    let response = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            "/api/v1/sos/contracts",
            token,
            json!({
                "contract_id": contract_id,
                "contract_name": format!("Contract {contract_id}"),
                "provider_interface_id": provider_interface_id,
                "consumer_interface_id": consumer_interface_id,
                "sla_metrics": [{
                    "name": "latency_ms",
                    "value": 100.0,
                    "operator": "<=",
                    "unit": "ms"
                }],
                "transformation_rules": {},
                "description": "Synthetic contract",
                "tags": ["api-test"],
            }),
        ))
        .await
        .expect("contract creation request should succeed");

    assert_json_response(response, StatusCode::OK).await
}

#[tokio::test]
async fn system_mutations_project_catalog_subjects_incrementally_via_build_router() {
    let harness = setup_authenticated_build_router_app();
    let token = harness
        .token
        .as_deref()
        .expect("authenticated harness should expose a token");

    let created = register_system(&harness, token, "http-system", "HTTP System").await;
    assert_eq!(created.system_id, "http-system");

    let system_subject = system_uri("http-system");
    let system_name_predicate = format!("{SOS_NS}systemName");
    assert_eq!(
        subject_objects(
            &harness.rdf_store,
            CATALOG_GRAPH,
            &system_subject,
            &system_name_predicate,
        ),
        vec!["\"HTTP System\"".to_string()]
    );

    let update_response = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::PUT,
            "/api/v1/sos/systems/http-system",
            token,
            json!({
                "system_name": "HTTP System Renamed",
                "tags": ["api-test", "renamed"]
            }),
        ))
        .await
        .expect("system update request should succeed");
    let updated: SystemResponse = assert_json_response(update_response, StatusCode::OK).await;

    assert_eq!(updated.system_name, "HTTP System Renamed");
    assert_eq!(
        subject_objects(
            &harness.rdf_store,
            CATALOG_GRAPH,
            &system_subject,
            &system_name_predicate,
        ),
        vec!["\"HTTP System Renamed\"".to_string()]
    );

    let delete_response = harness
        .app
        .clone()
        .oneshot(authed_empty_request(
            Method::DELETE,
            "/api/v1/sos/systems/http-system",
            token,
        ))
        .await
        .expect("system delete request should succeed");
    assert_status(delete_response, StatusCode::NO_CONTENT).await;

    assert!(
        !subject_exists(&harness.rdf_store, CATALOG_GRAPH, &system_subject),
        "system subject should be removed from the catalog graph after delete"
    );
}

#[tokio::test]
async fn interface_and_contract_mutations_project_catalog_subjects_incrementally_via_build_router()
{
    let harness = setup_authenticated_build_router_app();
    let token = harness
        .token
        .as_deref()
        .expect("authenticated harness should expose a token");

    register_system(&harness, token, "provider-http", "HTTP Provider").await;
    register_system(&harness, token, "consumer-http", "HTTP Consumer").await;

    let provider = register_interface(
        &harness,
        token,
        "provider-http",
        "provider-http-if",
        "HTTP Provider Interface",
        "Provider",
    )
    .await;
    let consumer = register_interface(
        &harness,
        token,
        "consumer-http",
        "consumer-http-if",
        "HTTP Consumer Interface",
        "Consumer",
    )
    .await;

    assert_eq!(provider.interface.interface_id, "provider-http-if");
    assert_eq!(consumer.interface.interface_id, "consumer-http-if");

    let provider_subject = interface_uri("provider-http-if");
    let interface_name_predicate = format!("{SOS_NS}interfaceName");
    let unit_system_predicate = format!("{SOS_NS}unitSystem");

    assert_eq!(
        subject_objects(
            &harness.rdf_store,
            CATALOG_GRAPH,
            &provider_subject,
            &interface_name_predicate,
        ),
        vec!["\"HTTP Provider Interface\"".to_string()]
    );
    assert_eq!(
        subject_objects(
            &harness.rdf_store,
            CATALOG_GRAPH,
            &provider_subject,
            &unit_system_predicate,
        ),
        vec!["\"SI\"".to_string()]
    );

    let update_interface_response = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::PUT,
            "/api/v1/sos/interfaces/provider-http-if",
            token,
            json!({
                "interface_name": "HTTP Provider Interface v2",
                "unit_system": "Imperial"
            }),
        ))
        .await
        .expect("interface update request should succeed");
    let updated_interface: InterfaceResponse =
        assert_json_response(update_interface_response, StatusCode::OK).await;

    assert_eq!(
        updated_interface.interface.interface_name,
        "HTTP Provider Interface v2"
    );
    assert_eq!(
        subject_objects(
            &harness.rdf_store,
            CATALOG_GRAPH,
            &provider_subject,
            &interface_name_predicate,
        ),
        vec!["\"HTTP Provider Interface v2\"".to_string()]
    );
    assert_eq!(
        subject_objects(
            &harness.rdf_store,
            CATALOG_GRAPH,
            &provider_subject,
            &unit_system_predicate,
        ),
        vec!["\"Imperial\"".to_string()]
    );

    let created_contract = create_contract(
        &harness,
        token,
        "http-contract",
        "provider-http-if",
        "consumer-http-if",
    )
    .await;
    assert_eq!(created_contract.contract_id, "http-contract");

    let contract_subject = contract_uri("http-contract");
    let contract_name_predicate = format!("{SOS_NS}contractName");
    assert_eq!(
        subject_objects(
            &harness.rdf_store,
            CATALOG_GRAPH,
            &contract_subject,
            &contract_name_predicate,
        ),
        vec!["\"Contract http-contract\"".to_string()]
    );

    let update_contract_response = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::PUT,
            "/api/v1/sos/contracts/http-contract",
            token,
            json!({
                "contract_name": "Contract http-contract v2"
            }),
        ))
        .await
        .expect("contract update request should succeed");
    let updated_contract: DataContractResponse =
        assert_json_response(update_contract_response, StatusCode::OK).await;

    assert_eq!(updated_contract.contract_name, "Contract http-contract v2");
    assert_eq!(
        subject_objects(
            &harness.rdf_store,
            CATALOG_GRAPH,
            &contract_subject,
            &contract_name_predicate,
        ),
        vec!["\"Contract http-contract v2\"".to_string()]
    );

    let delete_contract_response = harness
        .app
        .clone()
        .oneshot(authed_empty_request(
            Method::DELETE,
            "/api/v1/sos/contracts/http-contract",
            token,
        ))
        .await
        .expect("contract delete request should succeed");
    assert_status(delete_contract_response, StatusCode::NO_CONTENT).await;

    assert!(
        !subject_exists(&harness.rdf_store, CATALOG_GRAPH, &contract_subject),
        "contract subject should be removed from the catalog graph after delete"
    );

    let delete_interface_response = harness
        .app
        .clone()
        .oneshot(authed_empty_request(
            Method::DELETE,
            "/api/v1/sos/interfaces/provider-http-if",
            token,
        ))
        .await
        .expect("interface delete request should succeed");
    assert_status(delete_interface_response, StatusCode::NO_CONTENT).await;

    assert!(
        !subject_exists(&harness.rdf_store, CATALOG_GRAPH, &provider_subject),
        "interface subject should be removed from the catalog graph after delete"
    );
}

#[tokio::test]
async fn persisted_validation_projects_validation_graph_via_build_router() {
    let harness = setup_authenticated_build_router_app();
    let token = harness
        .token
        .as_deref()
        .expect("authenticated harness should expose a token");
    seed_minimal_catalog(&harness.storage_manager);

    let response = harness
        .app
        .clone()
        .oneshot(authed_json_request(
            Method::POST,
            "/api/v1/sos/validate",
            token,
            interface_pair_validation_payload(),
        ))
        .await
        .expect("validation request should succeed");
    let validation: ValidationResponse = assert_json_response(response, StatusCode::OK).await;

    let report_id = validation
        .report_id
        .clone()
        .expect("persisted validation should return a report id");

    assert!(
        subject_exists(
            &harness.rdf_store,
            VALIDATION_GRAPH,
            &validation_activity_uri(&validation.validation_id),
        ),
        "persisted validation should project a validation activity subject"
    );
    assert!(
        subject_exists(
            &harness.rdf_store,
            VALIDATION_GRAPH,
            &validation_report_uri(&report_id),
        ),
        "persisted validation should project a validation report subject"
    );
}
