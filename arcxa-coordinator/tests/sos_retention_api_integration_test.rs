//! HTTP/router coverage for SoS validation report retention.

#[path = "support/sos_api.rs"]
mod sos_api;

use axum::http::{Method, StatusCode};
use graphica_coordinator::api::sos_validation::types::{
    ValidationHistoryResponse, ValidationResponse,
};
use sos_api::{
    assert_json_response, assert_status, authed_empty_request, authed_json_request,
    interface_pair_validation_payload, seed_minimal_catalog, setup_authenticated_build_router_app,
    subject_exists, INTERFACE_PAIR_SUBJECT_KEY,
};
use tokio::time::{sleep, Duration};
use tower::ServiceExt;

const VALIDATION_GRAPH: &str = "http://graphica.io/graph/sos-validations";

fn validation_report_uri(report_id: &str) -> String {
    format!("http://graphica.io/sos/validation-report/{report_id}")
}

struct EnvGuard {
    vars: Vec<(&'static str, Option<String>)>,
}

impl EnvGuard {
    fn set(pairs: &[(&'static str, &'static str)]) -> Self {
        let vars = pairs
            .iter()
            .map(|(key, value)| {
                let previous = std::env::var(key).ok();
                std::env::set_var(key, value);
                (*key, previous)
            })
            .collect();

        Self { vars }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (key, previous) in self.vars.drain(..) {
            if let Some(value) = previous {
                std::env::set_var(key, value);
            } else {
                std::env::remove_var(key);
            }
        }
    }
}

#[tokio::test]
async fn build_router_applies_env_configured_validation_report_retention() {
    let _guard = EnvGuard::set(&[
        ("SOS_VALIDATION_REPORT_PRUNING_ENABLED", "true"),
        ("SOS_VALIDATION_REPORT_RETENTION_PER_SUBJECT", "2"),
        ("SOS_VALIDATION_REPORT_RETENTION_DAYS", "0"),
    ]);

    let harness = setup_authenticated_build_router_app();
    let token = harness
        .token
        .as_deref()
        .expect("authenticated harness should expose a token");
    seed_minimal_catalog(&harness.storage_manager);

    let mut report_ids = Vec::new();
    for _ in 0..3 {
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
        report_ids.push(
            validation
                .report_id
                .expect("persisted validation should return a report id"),
        );
        sleep(Duration::from_millis(5)).await;
    }

    let history_response = harness
        .app
        .clone()
        .oneshot(authed_empty_request(
            Method::GET,
            &format!(
                "/api/v1/sos/validation-history?subject_key={INTERFACE_PAIR_SUBJECT_KEY}&subject_type=interface_pair&limit=10"
            ),
            token,
        ))
        .await
        .expect("history request should succeed");
    let history: ValidationHistoryResponse =
        assert_json_response(history_response, StatusCode::OK).await;

    assert_eq!(history.reports.len(), 2);
    assert_eq!(history.reports[0].report_id, report_ids[2]);
    assert_eq!(history.reports[1].report_id, report_ids[1]);
    assert_eq!(
        history.reports[0].previous_report_id.as_deref(),
        Some(report_ids[1].as_str())
    );

    let pruned_report_response = harness
        .app
        .clone()
        .oneshot(authed_empty_request(
            Method::GET,
            &format!("/api/v1/sos/validation-reports/{}", report_ids[0]),
            token,
        ))
        .await
        .expect("pruned report lookup should complete");
    assert_status(pruned_report_response, StatusCode::NOT_FOUND).await;

    assert!(
        !subject_exists(
            &harness.rdf_store,
            VALIDATION_GRAPH,
            &validation_report_uri(&report_ids[0]),
        ),
        "pruned report subject should be removed from the validation graph"
    );
}
