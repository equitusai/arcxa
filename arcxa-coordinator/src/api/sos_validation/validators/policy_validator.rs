//! SPARQL-backed policy validation helpers for SoS validation.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyEvaluation {
    pub passed: bool,
    pub severity: String,
    pub violation_count: usize,
    pub details: Option<Value>,
}

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum PolicyQueryTemplateError {
    #[error("Malformed policy query template: unclosed placeholder")]
    UnclosedPlaceholder,
    #[error("Malformed policy query template: placeholder names cannot be empty")]
    EmptyPlaceholder,
    #[error("Malformed policy query template: invalid placeholder name '{0}'")]
    InvalidPlaceholderName(String),
    #[error("Policy query references missing template variable: {0}")]
    MissingPlaceholder(String),
    #[error(
        "Policy query placeholder '{placeholder}' must be an absolute URI string, got '{value}'"
    )]
    InvalidUri { placeholder: String, value: String },
    #[error("Policy query placeholder '{0}' cannot render null values")]
    NullValue(String),
    #[error("Policy query placeholder '{placeholder}' cannot render values of type {value_type}")]
    UnsupportedValueType {
        placeholder: String,
        value_type: &'static str,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PlaceholderToken {
    start: usize,
    end: usize,
    name: String,
}

pub fn extract_policy_placeholders(query: &str) -> Result<Vec<String>, PolicyQueryTemplateError> {
    let mut placeholders: Vec<_> = parse_policy_template(query)?
        .into_iter()
        .map(|token| token.name)
        .collect();
    placeholders.sort();
    placeholders.dedup();
    Ok(placeholders)
}

pub fn render_policy_query(
    query: &str,
    context: &HashMap<String, Value>,
) -> Result<String, PolicyQueryTemplateError> {
    let tokens = parse_policy_template(query)?;
    if tokens.is_empty() {
        return Ok(query.to_string());
    }

    let mut rendered = String::with_capacity(query.len());
    let mut cursor = 0;

    for token in tokens {
        rendered.push_str(&query[cursor..token.start]);
        let value = context
            .get(&token.name)
            .ok_or_else(|| PolicyQueryTemplateError::MissingPlaceholder(token.name.clone()))?;
        rendered.push_str(&render_placeholder_value(&token.name, value)?);
        cursor = token.end;
    }

    rendered.push_str(&query[cursor..]);
    Ok(rendered)
}

pub fn evaluate_policy_results(
    query: &str,
    results: &[Value],
    context: &HashMap<String, Value>,
) -> PolicyEvaluation {
    let severity = policy_severity_from_context(context);
    let query_upper = query.to_ascii_uppercase();

    if query_upper.contains("ASK") {
        let passed = results
            .first()
            .and_then(|row| row.get("ASK"))
            .and_then(Value::as_bool)
            .unwrap_or(false);

        return PolicyEvaluation {
            passed,
            severity,
            violation_count: usize::from(!passed),
            details: None,
        };
    }

    let violation_count = results.len();
    PolicyEvaluation {
        passed: violation_count == 0,
        severity,
        violation_count,
        details: (violation_count > 0).then(|| Value::Array(results.to_vec())),
    }
}

pub fn map_policy_severity(raw: &str) -> String {
    match raw.to_ascii_lowercase().as_str() {
        "critical" | "high" => "error".to_string(),
        "medium" => "warning".to_string(),
        "low" => "info".to_string(),
        "error" | "warning" | "info" => raw.to_ascii_lowercase(),
        _ => "error".to_string(),
    }
}

fn policy_severity_from_context(context: &HashMap<String, Value>) -> String {
    context
        .get("severity")
        .and_then(Value::as_str)
        .map(map_policy_severity)
        .unwrap_or_else(|| "error".to_string())
}

fn parse_policy_template(query: &str) -> Result<Vec<PlaceholderToken>, PolicyQueryTemplateError> {
    let mut tokens = Vec::new();
    let mut cursor = 0;

    while let Some(start_offset) = query[cursor..].find("{{") {
        let start = cursor + start_offset;
        let placeholder_start = start + 2;
        let Some(end_offset) = query[placeholder_start..].find("}}") else {
            return Err(PolicyQueryTemplateError::UnclosedPlaceholder);
        };
        let end = placeholder_start + end_offset;
        let raw_name = query[placeholder_start..end].trim();

        if raw_name.is_empty() {
            return Err(PolicyQueryTemplateError::EmptyPlaceholder);
        }

        if !is_valid_placeholder_name(raw_name) {
            return Err(PolicyQueryTemplateError::InvalidPlaceholderName(
                raw_name.to_string(),
            ));
        }

        tokens.push(PlaceholderToken {
            start,
            end: end + 2,
            name: raw_name.to_string(),
        });
        cursor = end + 2;
    }

    Ok(tokens)
}

fn is_valid_placeholder_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(ch) if ch.is_ascii_alphabetic() || ch == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn render_placeholder_value(
    placeholder: &str,
    value: &Value,
) -> Result<String, PolicyQueryTemplateError> {
    if placeholder.ends_with("_uri") {
        let Value::String(uri) = value else {
            return Err(PolicyQueryTemplateError::UnsupportedValueType {
                placeholder: placeholder.to_string(),
                value_type: json_value_type(value),
            });
        };
        return render_uri_placeholder(placeholder, uri);
    }

    match value {
        Value::String(string) => render_string_literal(string),
        Value::Number(number) => Ok(number.to_string()),
        Value::Bool(boolean) => Ok(boolean.to_string()),
        Value::Array(_) | Value::Object(_) => {
            let json_payload =
                serde_json::to_string(value).expect("serializing JSON payload should not fail");
            render_string_literal(&json_payload)
        }
        Value::Null => Err(PolicyQueryTemplateError::NullValue(placeholder.to_string())),
    }
}

fn render_uri_placeholder(
    placeholder: &str,
    value: &str,
) -> Result<String, PolicyQueryTemplateError> {
    if is_absolute_uri(value) {
        Ok(value.to_string())
    } else {
        Err(PolicyQueryTemplateError::InvalidUri {
            placeholder: placeholder.to_string(),
            value: value.to_string(),
        })
    }
}

fn render_string_literal(value: &str) -> Result<String, PolicyQueryTemplateError> {
    Ok(serde_json::to_string(value).expect("serializing string literal should not fail"))
}

fn is_absolute_uri(value: &str) -> bool {
    if value.is_empty()
        || value.chars().any(|ch| ch.is_whitespace())
        || value
            .chars()
            .any(|ch| matches!(ch, '<' | '>' | '"' | '\\' | '{' | '}' | '|' | '^' | '`'))
    {
        return false;
    }

    let Some((scheme, _rest)) = value.split_once(':') else {
        return false;
    };

    let mut chars = scheme.chars();
    matches!(chars.next(), Some(ch) if ch.is_ascii_alphabetic())
        && chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '-' | '.'))
}

fn json_value_type(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn ask_policy_false_is_failure() {
        let context = HashMap::new();
        let evaluation =
            evaluate_policy_results("ASK { ?s ?p ?o }", &[json!({"ASK": false})], &context);
        assert!(!evaluation.passed);
        assert_eq!(evaluation.violation_count, 1);
    }

    #[test]
    fn select_policy_with_rows_is_failure() {
        let context = HashMap::from([("severity".to_string(), json!("Medium"))]);
        let evaluation = evaluate_policy_results(
            "SELECT ?s WHERE { ?s ?p ?o }",
            &[json!({"s": "http://example.com/s"})],
            &context,
        );
        assert!(!evaluation.passed);
        assert_eq!(evaluation.severity, "warning");
    }

    #[test]
    fn render_policy_query_quotes_and_escapes_string_literals() {
        let context = HashMap::from([(
            "policy_name".to_string(),
            json!("name with \"quotes\"\nand newline"),
        )]);

        let rendered =
            render_policy_query("FILTER (?name = {{policy_name}})", &context).expect("render");

        assert_eq!(
            rendered,
            "FILTER (?name = \"name with \\\"quotes\\\"\\nand newline\")"
        );
    }

    #[test]
    fn render_policy_query_preserves_legacy_uri_placeholder_shape() {
        let context = HashMap::from([(
            "provider_interface_uri".to_string(),
            json!("http://graphica.io/sos/interface/provider-if"),
        )]);

        let rendered = render_policy_query("ASK { <{{provider_interface_uri}}> ?p ?o }", &context)
            .expect("render");

        assert_eq!(
            rendered,
            "ASK { <http://graphica.io/sos/interface/provider-if> ?p ?o }"
        );
    }

    #[test]
    fn render_policy_query_renders_numbers_booleans_and_json_payloads() {
        let context = HashMap::from([
            ("source_interface_count".to_string(), json!(3)),
            ("active".to_string(), json!(true)),
            (
                "payload".to_string(),
                json!({
                    "sample_id": "row-1",
                    "score": 0.93
                }),
            ),
        ]);

        let rendered = render_policy_query(
            "FILTER (?count >= {{source_interface_count}} && ?enabled = {{active}} && ?payload = {{payload}})",
            &context,
        )
        .expect("render");

        assert_eq!(
            rendered,
            "FILTER (?count >= 3 && ?enabled = true && ?payload = \"{\\\"sample_id\\\":\\\"row-1\\\",\\\"score\\\":0.93}\")"
        );
    }

    #[test]
    fn render_policy_query_rejects_missing_placeholders() {
        let error = render_policy_query("ASK { {{missing_value}} ?p ?o }", &HashMap::new())
            .expect_err("missing placeholder should fail");

        assert_eq!(
            error,
            PolicyQueryTemplateError::MissingPlaceholder("missing_value".to_string())
        );
    }

    #[test]
    fn render_policy_query_rejects_malformed_templates() {
        let error = render_policy_query("ASK { {{provider_interface_uri} ?p ?o }", &HashMap::new())
            .expect_err("unclosed placeholder should fail");
        assert_eq!(error, PolicyQueryTemplateError::UnclosedPlaceholder);

        let error = render_policy_query("ASK { {{ }} ?p ?o }", &HashMap::new())
            .expect_err("empty placeholder should fail");
        assert_eq!(error, PolicyQueryTemplateError::EmptyPlaceholder);
    }

    #[test]
    fn extract_policy_placeholders_deduplicates_and_validates_names() {
        let placeholders = extract_policy_placeholders(
            "ASK { {{policy_id}} {{policy_id}} {{provider_interface_uri}} }",
        )
        .expect("placeholder extraction should succeed");
        assert_eq!(
            placeholders,
            vec![
                "policy_id".to_string(),
                "provider_interface_uri".to_string()
            ]
        );

        let error = extract_policy_placeholders("ASK { {{bad-placeholder}} }")
            .expect_err("invalid placeholder names should fail");
        assert_eq!(
            error,
            PolicyQueryTemplateError::InvalidPlaceholderName("bad-placeholder".to_string())
        );
    }

    #[test]
    fn render_policy_query_rejects_invalid_uri_values() {
        let context = HashMap::from([("interface_uri".to_string(), json!("not a uri"))]);
        let error = render_policy_query("ASK { <{{interface_uri}}> ?p ?o }", &context)
            .expect_err("invalid URI placeholder should fail");

        assert_eq!(
            error,
            PolicyQueryTemplateError::InvalidUri {
                placeholder: "interface_uri".to_string(),
                value: "not a uri".to_string(),
            }
        );
    }

    #[test]
    fn render_policy_query_rejects_null_values() {
        let context = HashMap::from([("payload".to_string(), Value::Null)]);
        let error = render_policy_query("ASK { ?s ?p {{payload}} }", &context)
            .expect_err("null placeholders should fail");

        assert_eq!(
            error,
            PolicyQueryTemplateError::NullValue("payload".to_string())
        );
    }
}
