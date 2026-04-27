use anyhow::{anyhow, Context, Result};
use reqwest::{Client, Method, StatusCode};
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::path::Path;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct SosApiConfig {
    pub base_url: String,
    pub token: String,
    pub timeout: Duration,
}

#[derive(Clone)]
pub struct SosApiClient {
    base_url: String,
    token: String,
    client: Client,
}

impl SosApiClient {
    pub fn new(config: SosApiConfig) -> Result<Self> {
        let base_url = normalize_base_url(&config.base_url)?;
        let client = Client::builder()
            .timeout(config.timeout)
            .build()
            .context("failed to build CLI HTTP client")?;

        Ok(Self {
            base_url,
            token: config.token,
            client,
        })
    }

    pub async fn reconcile(&self, include_ontology_sync: bool) -> Result<Value> {
        self.request_json(
            Method::POST,
            "/sos/reconcile",
            None,
            Some(json!({ "include_ontology_sync": include_ontology_sync })),
        )
        .await
    }

    pub async fn list_systems(&self, request: ListSystemsRequest) -> Result<Value> {
        self.request_json(
            Method::GET,
            "/sos/systems",
            Some(list_systems_query(&request)),
            None,
        )
        .await
    }

    pub async fn list_interfaces(&self) -> Result<Value> {
        self.request_json(Method::GET, "/sos/interfaces", None, None)
            .await
    }

    pub async fn list_contracts(&self) -> Result<Value> {
        self.request_json(Method::GET, "/sos/contracts", None, None)
            .await
    }

    pub async fn list_policies(&self, request: ListPoliciesRequest) -> Result<Value> {
        self.request_json(
            Method::GET,
            "/sos/policies",
            Some(list_policies_query(&request)),
            None,
        )
        .await
    }

    pub async fn validate_interface_pair(
        &self,
        provider_interface_id: &str,
        consumer_interface_id: &str,
        dry_run: bool,
    ) -> Result<Value> {
        let path = if dry_run {
            "/sos/validate/dry-run"
        } else {
            "/sos/validate"
        };

        self.request_json(
            Method::POST,
            path,
            None,
            Some(json!({
                "type": "interface_compatibility",
                "provider_interface_id": provider_interface_id,
                "consumer_interface_id": consumer_interface_id,
            })),
        )
        .await
    }

    pub async fn get_validation_report(&self, report_id: &str) -> Result<Value> {
        let path = format!("/sos/validation-reports/{}", report_id);
        self.request_json(Method::GET, &path, None, None).await
    }

    pub async fn get_validation_history(
        &self,
        subject_key: &str,
        subject_type: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Value> {
        self.request_json(
            Method::GET,
            "/sos/validation-history",
            Some(validation_subject_query(subject_key, subject_type, limit)),
            None,
        )
        .await
    }

    pub async fn get_validation_lineage(
        &self,
        subject_key: &str,
        subject_type: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Value> {
        self.request_json(
            Method::GET,
            "/sos/validation-lineage",
            Some(validation_subject_query(subject_key, subject_type, limit)),
            None,
        )
        .await
    }

    pub async fn get_compatibility_matrix(
        &self,
        evaluation_budget: Option<usize>,
    ) -> Result<Value> {
        self.request_json(
            Method::GET,
            "/sos/compatibility-matrix",
            Some(optional_budget_query([(
                "evaluation_budget",
                evaluation_budget,
            )])),
            None,
        )
        .await
    }

    pub async fn get_dependency_graph(
        &self,
        node_budget: Option<usize>,
        edge_budget: Option<usize>,
    ) -> Result<Value> {
        self.request_json(
            Method::GET,
            "/sos/dependency-graph",
            Some(optional_budget_query([
                ("node_budget", node_budget),
                ("edge_budget", edge_budget),
            ])),
            None,
        )
        .await
    }

    pub async fn run_what_if_analysis(&self, request: WhatIfAnalysisRequest) -> Result<Value> {
        self.request_json(
            Method::POST,
            "/sos/what-if",
            None,
            Some(json!({
                "scenario": request.scenario,
                "changes": request.changes,
                "evaluation_budget": request.evaluation_budget,
            })),
        )
        .await
    }

    pub async fn contract_audit(
        &self,
        contract_id: &str,
        status: Option<&str>,
        offset: usize,
        limit: usize,
    ) -> Result<Value> {
        let contract = self
            .get_contract(contract_id)
            .await
            .with_context(|| format!("failed to load contract '{}'", contract_id))?;
        let approval_requests = self
            .list_contract_approval_requests(
                contract_id,
                StatusPageRequest {
                    status: status.map(str::to_owned),
                    offset,
                    limit,
                },
            )
            .await
            .with_context(|| format!("failed to load approval requests for '{}'", contract_id))?;
        let signatures = self
            .list_contract_signatures(contract_id, limit)
            .await
            .with_context(|| format!("failed to load signatures for '{}'", contract_id))?;

        Ok(json!({
            "contract": contract,
            "approval_requests": approval_requests,
            "signatures": signatures,
        }))
    }

    pub async fn get_contract(&self, contract_id: &str) -> Result<Value> {
        let path = format!("/sos/contracts/{}", contract_id);
        self.request_json(Method::GET, &path, None, None).await
    }

    pub async fn lookup_contract(
        &self,
        provider_interface_id: &str,
        consumer_interface_id: &str,
    ) -> Result<Value> {
        self.request_json(
            Method::GET,
            "/sos/contracts/lookup",
            Some(contract_lookup_query(
                provider_interface_id,
                consumer_interface_id,
            )),
            None,
        )
        .await
    }

    pub async fn list_contract_approval_requests(
        &self,
        contract_id: &str,
        request: StatusPageRequest,
    ) -> Result<Value> {
        let path = format!("/sos/contracts/{}/approval-requests", contract_id);
        self.request_json(Method::GET, &path, Some(status_page_query(&request)), None)
            .await
    }

    pub async fn get_contract_approval_request(
        &self,
        contract_id: &str,
        request_id: &str,
    ) -> Result<Value> {
        let path = format!(
            "/sos/contracts/{}/approval-requests/{}",
            contract_id, request_id
        );
        self.request_json(Method::GET, &path, None, None).await
    }

    pub async fn list_contract_signatures(&self, contract_id: &str, limit: usize) -> Result<Value> {
        let path = format!("/sos/contracts/{}/signatures", contract_id);
        self.request_json(Method::GET, &path, Some(limit_query(limit)), None)
            .await
    }

    pub async fn contract_signing_key_status(&self) -> Result<Value> {
        self.request_json(Method::GET, "/sos/contracts/signing-key", None, None)
            .await
    }

    pub async fn rotate_contract_signing_key(&self, reason: Option<&str>) -> Result<Value> {
        self.request_json(
            Method::POST,
            "/sos/contracts/signing-key/rotate",
            None,
            Some(json!({ "reason": reason })),
        )
        .await
    }

    pub async fn policy_audit(
        &self,
        policy_id: &str,
        status: Option<&str>,
        offset: usize,
        limit: usize,
    ) -> Result<Value> {
        let policy = self
            .get_policy(policy_id)
            .await
            .with_context(|| format!("failed to load policy '{}'", policy_id))?;
        let approval_requests = self
            .list_policy_approval_requests(
                policy_id,
                StatusPageRequest {
                    status: status.map(str::to_owned),
                    offset,
                    limit,
                },
            )
            .await
            .with_context(|| format!("failed to load approval requests for '{}'", policy_id))?;
        let attestations = self
            .list_policy_attestations(policy_id, limit)
            .await
            .with_context(|| format!("failed to load attestations for '{}'", policy_id))?;

        Ok(json!({
            "policy": policy,
            "approval_requests": approval_requests,
            "attestations": attestations,
        }))
    }

    pub async fn get_policy(&self, policy_id: &str) -> Result<Value> {
        let path = format!("/sos/policies/{}", policy_id);
        self.request_json(Method::GET, &path, None, None).await
    }

    pub async fn list_policy_approval_requests(
        &self,
        policy_id: &str,
        request: StatusPageRequest,
    ) -> Result<Value> {
        let path = format!("/sos/policies/{}/approval-requests", policy_id);
        self.request_json(Method::GET, &path, Some(status_page_query(&request)), None)
            .await
    }

    pub async fn get_policy_approval_request(
        &self,
        policy_id: &str,
        request_id: &str,
    ) -> Result<Value> {
        let path = format!(
            "/sos/policies/{}/approval-requests/{}",
            policy_id, request_id
        );
        self.request_json(Method::GET, &path, None, None).await
    }

    pub async fn list_policy_attestations(&self, policy_id: &str, limit: usize) -> Result<Value> {
        let path = format!("/sos/policies/{}/attestations", policy_id);
        self.request_json(Method::GET, &path, Some(limit_query(limit)), None)
            .await
    }

    pub async fn validate_policy(
        &self,
        policy_id: &str,
        request: PolicyValidationRequest,
    ) -> Result<Value> {
        let path = if request.dry_run {
            format!("/sos/policies/{}/validate/dry-run", policy_id)
        } else {
            format!("/sos/policies/{}/validate", policy_id)
        };

        self.request_json(
            Method::POST,
            &path,
            None,
            Some(json!({
                "stage": request.stage,
                "revision": request.revision,
                "context": request.context,
            })),
        )
        .await
    }

    pub async fn policy_signing_key_status(&self) -> Result<Value> {
        self.request_json(Method::GET, "/sos/policies/signing-key", None, None)
            .await
    }

    pub async fn rotate_policy_signing_key(
        &self,
        request: RotatePolicySigningKeyRequest<'_>,
    ) -> Result<Value> {
        self.request_json(
            Method::POST,
            "/sos/policies/signing-key/rotate",
            None,
            Some(json!({
                "reason": request.reason,
                "trust_mode": request.trust_mode,
                "trust_provider": request.trust_provider,
                "external_key_ref": request.external_key_ref,
                "trust_attestation_ref": request.trust_attestation_ref,
            })),
        )
        .await
    }

    async fn request_json(
        &self,
        method: Method,
        path: &str,
        query: Option<Vec<(String, String)>>,
        body: Option<Value>,
    ) -> Result<Value> {
        let url = format!("{}{}", self.base_url, path);
        let mut request = self
            .client
            .request(method, &url)
            .bearer_auth(&self.token)
            .header("Accept", "application/json");

        if let Some(query) = query {
            let filtered: Vec<(String, String)> = query
                .into_iter()
                .filter(|(_, value)| !value.is_empty())
                .collect();
            if !filtered.is_empty() {
                request = request.query(&filtered);
            }
        }

        if let Some(body) = body {
            request = request.json(&body);
        }

        let response = request
            .send()
            .await
            .with_context(|| format!("request to '{}' failed", url))?;
        let status = response.status();
        let text = response
            .text()
            .await
            .with_context(|| format!("failed to read response body from '{}'", url))?;

        if !status.is_success() {
            return Err(build_http_error(status, &url, &text));
        }

        serde_json::from_str(&text)
            .with_context(|| format!("failed to decode JSON response from '{}'", url))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RotatePolicySigningKeyRequest<'a> {
    pub reason: Option<&'a str>,
    pub trust_mode: Option<&'a str>,
    pub trust_provider: Option<&'a str>,
    pub external_key_ref: Option<&'a str>,
    pub trust_attestation_ref: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub struct WhatIfAnalysisRequest {
    pub scenario: String,
    pub changes: Vec<Value>,
    pub evaluation_budget: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct ListSystemsRequest {
    pub system_type: Option<String>,
    pub vendor: Option<String>,
    pub classification: Option<String>,
    pub tags: Option<String>,
    pub active: Option<bool>,
    pub offset: usize,
    pub limit: usize,
}

#[derive(Debug, Clone)]
pub struct ListPoliciesRequest {
    pub target_type: Option<String>,
    pub stage: Option<String>,
    pub active: Option<bool>,
    pub lifecycle_state: Option<String>,
    pub approval_status: Option<String>,
    pub offset: usize,
    pub limit: usize,
}

#[derive(Debug, Clone)]
pub struct StatusPageRequest {
    pub status: Option<String>,
    pub offset: usize,
    pub limit: usize,
}

#[derive(Debug, Clone)]
pub struct PolicyValidationRequest {
    pub stage: Option<String>,
    pub revision: Option<u32>,
    pub context: Map<String, Value>,
    pub dry_run: bool,
}

pub fn load_json_value_array(
    inline_json: Option<&str>,
    file_path: Option<&Path>,
) -> Result<Vec<Value>> {
    match (inline_json, file_path) {
        (Some(_), Some(_)) => Err(anyhow!(
            "provide either inline JSON or a JSON file path, not both"
        )),
        (None, None) => Err(anyhow!(
            "what-if analysis requires either inline JSON changes or a JSON file"
        )),
        (Some(inline_json), None) => parse_json_value_array(
            inline_json.as_bytes(),
            "failed to parse inline what-if changes JSON",
        ),
        (None, Some(file_path)) => {
            let bytes = std::fs::read(file_path)
                .with_context(|| format!("failed to read '{}'", file_path.display()))?;
            parse_json_value_array(&bytes, "failed to parse what-if changes JSON file")
        }
    }
}

pub fn load_optional_json_value_object(
    inline_json: Option<&str>,
    file_path: Option<&Path>,
) -> Result<Map<String, Value>> {
    match (inline_json, file_path) {
        (Some(_), Some(_)) => Err(anyhow!(
            "provide either inline JSON or a JSON file path, not both"
        )),
        (None, None) => Ok(Map::new()),
        (Some(inline_json), None) => {
            parse_json_value_object(inline_json.as_bytes(), "failed to parse inline JSON object")
        }
        (None, Some(file_path)) => {
            let bytes = std::fs::read(file_path)
                .with_context(|| format!("failed to read '{}'", file_path.display()))?;
            parse_json_value_object(&bytes, "failed to parse JSON object file")
        }
    }
}

fn normalize_base_url(base_url: &str) -> Result<String> {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err(anyhow!("Graphica API base URL cannot be empty"));
    }

    if trimmed.ends_with("/api/v1") {
        return Ok(trimmed.to_string());
    }

    if trimmed.contains("/api/") {
        return Ok(trimmed.to_string());
    }

    Ok(format!("{trimmed}/api/v1"))
}

fn validation_subject_query(
    subject_key: &str,
    subject_type: Option<&str>,
    limit: Option<usize>,
) -> Vec<(String, String)> {
    let mut query = vec![("subject_key".to_string(), subject_key.to_string())];
    push_optional_query(&mut query, "subject_type", subject_type);
    if let Some(limit) = limit {
        query.push(("limit".to_string(), limit.to_string()));
    }
    query
}

fn list_systems_query(request: &ListSystemsRequest) -> Vec<(String, String)> {
    let mut query = pagination_query(request.offset, request.limit);
    push_optional_query(&mut query, "system_type", request.system_type.as_deref());
    push_optional_query(&mut query, "vendor", request.vendor.as_deref());
    push_optional_query(
        &mut query,
        "classification",
        request.classification.as_deref(),
    );
    if let Some(tags) = normalize_comma_separated(request.tags.as_deref()) {
        query.push(("tags".to_string(), tags));
    }
    if let Some(active) = request.active {
        query.push(("active".to_string(), active.to_string()));
    }
    query
}

fn list_policies_query(request: &ListPoliciesRequest) -> Vec<(String, String)> {
    let mut query = pagination_query(request.offset, request.limit);
    push_optional_query(&mut query, "target_type", request.target_type.as_deref());
    push_optional_query(&mut query, "stage", request.stage.as_deref());
    if let Some(active) = request.active {
        query.push(("active".to_string(), active.to_string()));
    }
    push_optional_query(
        &mut query,
        "lifecycle_state",
        request.lifecycle_state.as_deref(),
    );
    push_optional_query(
        &mut query,
        "approval_status",
        request.approval_status.as_deref(),
    );
    query
}

fn status_page_query(request: &StatusPageRequest) -> Vec<(String, String)> {
    let mut query = pagination_query(request.offset, request.limit);
    push_optional_query(&mut query, "status", request.status.as_deref());
    query
}

fn contract_lookup_query(
    provider_interface_id: &str,
    consumer_interface_id: &str,
) -> Vec<(String, String)> {
    vec![
        (
            "provider_interface_id".to_string(),
            provider_interface_id.trim().to_string(),
        ),
        (
            "consumer_interface_id".to_string(),
            consumer_interface_id.trim().to_string(),
        ),
    ]
}

fn pagination_query(offset: usize, limit: usize) -> Vec<(String, String)> {
    vec![
        ("offset".to_string(), offset.to_string()),
        ("limit".to_string(), limit.to_string()),
    ]
}

fn limit_query(limit: usize) -> Vec<(String, String)> {
    vec![("limit".to_string(), limit.to_string())]
}

fn optional_budget_query<const N: usize>(
    pairs: [(&str, Option<usize>); N],
) -> Vec<(String, String)> {
    pairs
        .into_iter()
        .filter_map(|(key, value)| value.map(|value| (key.to_string(), value.to_string())))
        .collect()
}

fn push_optional_query(query: &mut Vec<(String, String)>, key: &str, value: Option<&str>) {
    if let Some(value) = trim_optional(value) {
        query.push((key.to_string(), value.to_string()));
    }
}

fn trim_optional(value: Option<&str>) -> Option<&str> {
    value.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then_some(trimmed)
    })
}

fn normalize_comma_separated(value: Option<&str>) -> Option<String> {
    value.and_then(|value| {
        let parts: Vec<&str> = value
            .split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .collect();
        (!parts.is_empty()).then(|| parts.join(","))
    })
}

fn parse_json_value_array(bytes: &[u8], error_context: &str) -> Result<Vec<Value>> {
    let parsed: Value = serde_json::from_slice(bytes).with_context(|| error_context.to_string())?;
    parsed
        .as_array()
        .cloned()
        .ok_or_else(|| anyhow!("what-if changes JSON must be an array of change objects"))
}

fn parse_json_value_object(bytes: &[u8], error_context: &str) -> Result<Map<String, Value>> {
    let parsed: Value = serde_json::from_slice(bytes).with_context(|| error_context.to_string())?;
    parsed
        .as_object()
        .cloned()
        .ok_or_else(|| anyhow!("JSON input must be an object"))
}

fn build_http_error(status: StatusCode, url: &str, body: &str) -> anyhow::Error {
    if let Ok(parsed) = serde_json::from_str::<Value>(body) {
        let message = parsed
            .get("message")
            .and_then(Value::as_str)
            .or_else(|| parsed.get("error").and_then(Value::as_str))
            .unwrap_or("request failed");
        return anyhow!("{} {}: {}", status.as_u16(), url, message);
    }

    anyhow!("{} {}: {}", status.as_u16(), url, body.trim())
}

pub fn render_pretty_json<T: Serialize>(value: &T) -> Result<String> {
    serde_json::to_string_pretty(value).context("failed to serialize CLI output")
}

#[cfg(test)]
mod tests {
    use super::{
        load_json_value_array, load_optional_json_value_object, normalize_base_url,
        normalize_comma_separated, optional_budget_query, status_page_query,
        validation_subject_query, ListPoliciesRequest, ListSystemsRequest, StatusPageRequest,
    };
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn normalize_base_url_appends_api_prefix_when_needed() {
        assert_eq!(
            normalize_base_url("http://localhost:8080").unwrap(),
            "http://localhost:8080/api/v1"
        );
    }

    #[test]
    fn normalize_base_url_keeps_existing_api_path() {
        assert_eq!(
            normalize_base_url("http://localhost:8080/api/v1/").unwrap(),
            "http://localhost:8080/api/v1"
        );
    }

    #[test]
    fn validation_subject_query_omits_blank_subject_type() {
        assert_eq!(
            validation_subject_query("interface:demo", Some("   "), Some(25)),
            vec![
                ("subject_key".to_string(), "interface:demo".to_string()),
                ("limit".to_string(), "25".to_string()),
            ]
        );
    }

    #[test]
    fn optional_budget_query_omits_unset_budgets() {
        assert_eq!(
            optional_budget_query([("evaluation_budget", Some(10)), ("unused", None)]),
            vec![("evaluation_budget".to_string(), "10".to_string())]
        );
    }

    #[test]
    fn load_json_value_array_parses_inline_json() {
        let values = load_json_value_array(Some(r#"[{"id":"system-a"}]"#), None).unwrap();
        assert_eq!(values.len(), 1);
        assert_eq!(values[0]["id"], "system-a");
    }

    #[test]
    fn load_json_value_array_rejects_non_array_json() {
        let error = load_json_value_array(Some(r#"{"id":"system-a"}"#), None).unwrap_err();
        assert!(error
            .to_string()
            .contains("what-if changes JSON must be an array"));
    }

    #[test]
    fn load_json_value_array_reads_json_file() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("arcxa-cli-what-if-{unique}.json"));
        fs::write(&path, r#"[{"kind":"interface"}]"#).unwrap();

        let values = load_json_value_array(None, Some(&path)).unwrap();
        assert_eq!(values.len(), 1);
        assert_eq!(values[0]["kind"], "interface");

        let _ = fs::remove_file(path);
    }

    #[test]
    fn list_systems_query_normalizes_filters() {
        let query = super::list_systems_query(&ListSystemsRequest {
            system_type: Some(" sensor-grid ".to_string()),
            vendor: Some("".to_string()),
            classification: Some(" UNCLASSIFIED ".to_string()),
            tags: Some(" critical, ops , ,edge ".to_string()),
            active: Some(true),
            offset: 5,
            limit: 20,
        });

        assert_eq!(
            query,
            vec![
                ("offset".to_string(), "5".to_string()),
                ("limit".to_string(), "20".to_string()),
                ("system_type".to_string(), "sensor-grid".to_string()),
                ("classification".to_string(), "UNCLASSIFIED".to_string()),
                ("tags".to_string(), "critical,ops,edge".to_string()),
                ("active".to_string(), "true".to_string()),
            ]
        );
    }

    #[test]
    fn list_policies_query_omits_blank_filters() {
        let query = super::list_policies_query(&ListPoliciesRequest {
            target_type: Some(" interface_pair ".to_string()),
            stage: Some(" pre_execution ".to_string()),
            active: Some(false),
            lifecycle_state: Some("   ".to_string()),
            approval_status: Some(" approved ".to_string()),
            offset: 0,
            limit: 10,
        });

        assert_eq!(
            query,
            vec![
                ("offset".to_string(), "0".to_string()),
                ("limit".to_string(), "10".to_string()),
                ("target_type".to_string(), "interface_pair".to_string()),
                ("stage".to_string(), "pre_execution".to_string()),
                ("active".to_string(), "false".to_string()),
                ("approval_status".to_string(), "approved".to_string()),
            ]
        );
    }

    #[test]
    fn status_page_query_omits_blank_status() {
        assert_eq!(
            status_page_query(&StatusPageRequest {
                status: Some("   ".to_string()),
                offset: 2,
                limit: 15,
            }),
            vec![
                ("offset".to_string(), "2".to_string()),
                ("limit".to_string(), "15".to_string()),
            ]
        );
    }

    #[test]
    fn normalize_comma_separated_trims_empty_values() {
        assert_eq!(
            normalize_comma_separated(Some(" alpha, beta ,, gamma ")),
            Some("alpha,beta,gamma".to_string())
        );
        assert_eq!(normalize_comma_separated(Some(" ,  ")), None);
    }

    #[test]
    fn load_optional_json_value_object_defaults_to_empty_map() {
        let values = load_optional_json_value_object(None, None).unwrap();
        assert!(values.is_empty());
    }

    #[test]
    fn load_optional_json_value_object_parses_inline_json() {
        let values =
            load_optional_json_value_object(Some(r#"{"region":"east","budget":3}"#), None).unwrap();
        assert_eq!(values["region"], "east");
        assert_eq!(values["budget"], 3);
    }

    #[test]
    fn load_optional_json_value_object_rejects_non_object_json() {
        let error =
            load_optional_json_value_object(Some(r#"["not","an","object"]"#), None).unwrap_err();
        assert!(error.to_string().contains("must be an object"));
    }
}
