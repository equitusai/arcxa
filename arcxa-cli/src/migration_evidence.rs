use anyhow::{anyhow, Context, Result};
use reqwest::{Client, Method, StatusCode};
use serde_json::Value;
use std::path::Path;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct MigrationEvidenceApiConfig {
    pub base_url: String,
    pub token: String,
    pub timeout: Duration,
}

#[derive(Clone)]
pub struct MigrationEvidenceApiClient {
    base_url: String,
    token: String,
    client: Client,
}

impl MigrationEvidenceApiClient {
    pub fn new(config: MigrationEvidenceApiConfig) -> Result<Self> {
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

    pub async fn upsert_connector(&self, connector: Value) -> Result<Value> {
        self.request_json(
            Method::POST,
            "/migration-evidence/connectors",
            None,
            Some(connector),
        )
        .await
    }

    pub async fn run_connector(&self, connector_id: &str, request: Value) -> Result<Value> {
        let path = format!("/migration-evidence/connectors/{connector_id}/runs");
        self.request_json(Method::POST, &path, None, Some(request))
            .await
    }

    pub async fn explain_value(&self, request: ExplainValueRequest<'_>) -> Result<Value> {
        self.request_json(
            Method::GET,
            "/migration-evidence/values/explain",
            Some(explain_value_query(request)),
            None,
        )
        .await
    }

    pub async fn evidence_packet(&self, object_id: &str, value_key: Option<&str>) -> Result<Value> {
        let path = format!("/migration-evidence/objects/{object_id}/evidence-packet");
        self.request_json(
            Method::GET,
            &path,
            Some(optional_value_key_query(value_key)),
            None,
        )
        .await
    }

    pub async fn object_controls(&self, object_id: &str) -> Result<Value> {
        let path = format!("/migration-evidence/objects/{object_id}/controls");
        self.request_json(Method::GET, &path, None, None).await
    }

    pub async fn program_exceptions(&self, program_id: &str) -> Result<Value> {
        let path = format!("/migration-evidence/programs/{program_id}/exceptions");
        self.request_json(Method::GET, &path, None, None).await
    }

    pub async fn program_approvals(&self, program_id: &str) -> Result<Value> {
        let path = format!("/migration-evidence/programs/{program_id}/approvals");
        self.request_json(Method::GET, &path, None, None).await
    }

    pub async fn runtime_status(&self) -> Result<Value> {
        self.request_json(
            Method::GET,
            "/migration-evidence/runtime/status",
            None,
            None,
        )
        .await
    }

    pub async fn rebuild_read_models(&self) -> Result<Value> {
        self.request_json(
            Method::POST,
            "/migration-evidence/runtime/rebuild",
            None,
            Some(serde_json::json!({})),
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
pub struct ExplainValueRequest<'a> {
    pub program_id: &'a str,
    pub object_id: &'a str,
    pub target_field_path: &'a str,
    pub target_record_id: Option<&'a str>,
    pub source_record_id: Option<&'a str>,
}

pub fn load_required_json_value(
    inline_json: Option<&str>,
    file_path: Option<&Path>,
    input_name: &str,
) -> Result<Value> {
    match (inline_json, file_path) {
        (Some(_), Some(_)) => Err(anyhow!(
            "provide either inline JSON or a JSON file path for {}, not both",
            input_name
        )),
        (None, None) => Err(anyhow!(
            "{} requires either inline JSON or a JSON file path",
            input_name
        )),
        (Some(inline_json), None) => parse_json_value(
            inline_json.as_bytes(),
            &format!("failed to parse inline JSON for {input_name}"),
        ),
        (None, Some(file_path)) => {
            let bytes = std::fs::read(file_path)
                .with_context(|| format!("failed to read '{}'", file_path.display()))?;
            parse_json_value(
                &bytes,
                &format!("failed to parse JSON file for {input_name}"),
            )
        }
    }
}

fn parse_json_value(bytes: &[u8], error_context: &str) -> Result<Value> {
    let parsed: Value = serde_json::from_slice(bytes).with_context(|| error_context.to_string())?;
    if !parsed.is_object() {
        return Err(anyhow!("migration-evidence JSON input must be an object"));
    }
    Ok(parsed)
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

fn explain_value_query(request: ExplainValueRequest<'_>) -> Vec<(String, String)> {
    let mut query = vec![
        (
            "program_id".to_string(),
            request.program_id.trim().to_string(),
        ),
        (
            "object_id".to_string(),
            request.object_id.trim().to_string(),
        ),
        (
            "target_field_path".to_string(),
            request.target_field_path.trim().to_string(),
        ),
    ];
    push_optional_query(&mut query, "target_record_id", request.target_record_id);
    push_optional_query(&mut query, "source_record_id", request.source_record_id);
    query
}

fn optional_value_key_query(value_key: Option<&str>) -> Vec<(String, String)> {
    let mut query = Vec::new();
    push_optional_query(&mut query, "value_key", value_key);
    query
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

#[cfg(test)]
mod tests {
    use super::{
        explain_value_query, load_required_json_value, normalize_base_url, ExplainValueRequest,
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
    fn explain_value_query_omits_blank_optional_record_ids() {
        assert_eq!(
            explain_value_query(ExplainValueRequest {
                program_id: "program-1",
                object_id: "object-1",
                target_field_path: "$.amount",
                target_record_id: Some("   "),
                source_record_id: Some("SO-1"),
            }),
            vec![
                ("program_id".to_string(), "program-1".to_string()),
                ("object_id".to_string(), "object-1".to_string()),
                ("target_field_path".to_string(), "$.amount".to_string()),
                ("source_record_id".to_string(), "SO-1".to_string()),
            ]
        );
    }

    #[test]
    fn load_required_json_value_parses_inline_object() {
        let parsed = load_required_json_value(
            Some(r#"{"connector_id":"ibm-artifacts"}"#),
            None,
            "connector",
        )
        .unwrap();
        assert_eq!(parsed["connector_id"], "ibm-artifacts");
    }

    #[test]
    fn load_required_json_value_rejects_non_object_payloads() {
        let error = load_required_json_value(Some(r#"[1,2,3]"#), None, "connector").unwrap_err();
        assert!(error.to_string().contains("must be an object"));
    }

    #[test]
    fn load_required_json_value_parses_json_file() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("migration-evidence-{unique}.json"));
        fs::write(&path, r#"{"run_label":"demo"}"#).unwrap();

        let parsed = load_required_json_value(None, Some(&path), "run request").unwrap();
        assert_eq!(parsed["run_label"], "demo");

        let _ = fs::remove_file(path);
    }
}
