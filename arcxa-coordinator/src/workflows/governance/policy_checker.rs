//! Workflow Governance Policy Checker
//!
//! Validates that workflow executions comply with governance policies before execution.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::governance::{rdf_store::RdfStore, GraphicaRdfStore};

/// Policy check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyCheckResult {
    /// Whether execution is allowed
    pub allowed: bool,
    /// Policy violations found
    pub violations: Vec<PolicyViolation>,
    /// Warnings (non-blocking)
    pub warnings: Vec<String>,
    /// Policies evaluated
    pub policies_checked: Vec<String>,
}

/// Policy violation details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyViolation {
    /// Policy ID that was violated
    pub policy_id: String,
    /// Policy name
    pub policy_name: String,
    /// Severity level
    pub severity: ViolationSeverity,
    /// Description of violation
    pub message: String,
    /// Recommended action
    pub recommendation: Option<String>,
}

/// Violation severity levels
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "UPPERCASE")]
pub enum ViolationSeverity {
    /// Blocks execution
    Critical,
    /// Blocks execution
    High,
    /// Warning only
    Medium,
    /// Informational
    Low,
}

/// Governance policy checker
pub struct GovernancePolicyChecker {
    rdf_store: Arc<GraphicaRdfStore>,
}

impl GovernancePolicyChecker {
    /// Create new policy checker
    pub fn new(rdf_store: Arc<GraphicaRdfStore>) -> Self {
        Self { rdf_store }
    }

    /// Check if workflow execution is allowed for given user and input
    ///
    /// Validates:
    /// - User has permission to execute workflow
    /// - Input data meets classification requirements
    /// - Workflow complies with data handling policies
    /// - No PII/sensitive data policy violations
    pub async fn check_execution_allowed(
        &self,
        workflow_id: &str,
        user_id: Option<&str>,
        input_data: &serde_json::Value,
    ) -> Result<PolicyCheckResult> {
        let mut violations = Vec::new();
        let mut warnings = Vec::new();
        let mut policies_checked = Vec::new();

        // Check 1: User authorization
        if let Some(user) = user_id {
            policies_checked.push("user_authorization".to_string());
            if let Some(violation) = self.check_user_authorization(workflow_id, user).await? {
                violations.push(violation);
            }
        } else {
            warnings.push("No user context provided for authorization check".to_string());
        }

        // Check 2: Data classification
        policies_checked.push("data_classification".to_string());
        if let Some(violation) = self
            .check_data_classification(workflow_id, input_data)
            .await?
        {
            violations.push(violation);
        }

        // Check 3: PII handling
        policies_checked.push("pii_handling".to_string());
        if let Some(violation) = self.check_pii_handling(workflow_id, input_data).await? {
            violations.push(violation);
        }

        // Check 4: Workflow enabled status
        policies_checked.push("workflow_enabled".to_string());
        if let Some(violation) = self.check_workflow_enabled(workflow_id).await? {
            violations.push(violation);
        }

        // Determine if execution allowed (no CRITICAL or HIGH violations)
        let allowed = !violations.iter().any(|v| {
            v.severity == ViolationSeverity::Critical || v.severity == ViolationSeverity::High
        });

        Ok(PolicyCheckResult {
            allowed,
            violations,
            warnings,
            policies_checked,
        })
    }

    /// Check if user is authorized to execute workflow
    async fn check_user_authorization(
        &self,
        workflow_id: &str,
        user_id: &str,
    ) -> Result<Option<PolicyViolation>> {
        // Query RDF for user permissions
        let sparql = format!(
            r#"
PREFIX auth: <http://graphica.io/auth#>
PREFIX wf: <http://graphica.io/workflow#>

ASK {{
    <http://graphica.io/user/{}> auth:canExecute <http://graphica.io/workflow/{}> .
}}
"#,
            user_id, workflow_id
        );

        let results = self.rdf_store.query(&sparql)?;

        // ASK query returns boolean in first result
        let has_permission = results
            .get(0)
            .and_then(|v| v.get("ASK"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if !has_permission {
            return Ok(Some(PolicyViolation {
                policy_id: "AUTH001".to_string(),
                policy_name: "User Authorization".to_string(),
                severity: ViolationSeverity::Critical,
                message: format!(
                    "User '{}' is not authorized to execute workflow '{}'",
                    user_id, workflow_id
                ),
                recommendation: Some(
                    "Request workflow execution permission from administrator".to_string(),
                ),
            }));
        }

        Ok(None)
    }

    /// Check data classification requirements
    async fn check_data_classification(
        &self,
        workflow_id: &str,
        input_data: &serde_json::Value,
    ) -> Result<Option<PolicyViolation>> {
        // Check if workflow handles sensitive data
        let sparql = format!(
            r#"
PREFIX wf: <http://graphica.io/workflow#>
PREFIX data: <http://graphica.io/data#>

SELECT ?classification WHERE {{
    <http://graphica.io/workflow/{}> wf:requiresDataClassification ?classification .
}}
"#,
            workflow_id
        );

        let results = self.rdf_store.query(&sparql)?;

        if !results.is_empty() {
            // Check if input contains fields marked as sensitive
            if self.contains_sensitive_fields(input_data) {
                return Ok(Some(PolicyViolation {
                    policy_id: "DATA001".to_string(),
                    policy_name: "Data Classification".to_string(),
                    severity: ViolationSeverity::High,
                    message: "Input contains sensitive data that requires classification"
                        .to_string(),
                    recommendation: Some(
                        "Ensure data is properly classified before processing".to_string(),
                    ),
                }));
            }
        }

        Ok(None)
    }

    /// Check PII handling policies
    async fn check_pii_handling(
        &self,
        workflow_id: &str,
        input_data: &serde_json::Value,
    ) -> Result<Option<PolicyViolation>> {
        // Detect potential PII fields in input
        let pii_fields = self.detect_pii_fields(input_data);

        if !pii_fields.is_empty() {
            // Check if workflow is certified for PII handling
            let sparql = format!(
                r#"
PREFIX wf: <http://graphica.io/workflow#>

ASK {{
    <http://graphica.io/workflow/{}> wf:piiCertified true .
}}
"#,
                workflow_id
            );

            let results = self.rdf_store.query(&sparql)?;
            let is_certified = results
                .get(0)
                .and_then(|v| v.get("ASK"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            if !is_certified {
                return Ok(Some(PolicyViolation {
                    policy_id: "PII001".to_string(),
                    policy_name: "PII Handling".to_string(),
                    severity: ViolationSeverity::Critical,
                    message: format!(
                        "Workflow is not certified to handle PII. Detected fields: {}",
                        pii_fields.join(", ")
                    ),
                    recommendation: Some(
                        "Remove PII from input or certify workflow for PII handling".to_string(),
                    ),
                }));
            }
        }

        Ok(None)
    }

    /// Check if workflow is enabled
    async fn check_workflow_enabled(&self, workflow_id: &str) -> Result<Option<PolicyViolation>> {
        let sparql = format!(
            r#"
PREFIX wf: <http://graphica.io/workflow#>

SELECT ?enabled WHERE {{
    <http://graphica.io/workflow/{}> wf:enabled ?enabled .
}}
"#,
            workflow_id
        );

        let results = self.rdf_store.query(&sparql)?;

        if let Some(row) = results.first() {
            if let Some(enabled_str) = row.get("enabled").and_then(|v| v.as_str()) {
                if enabled_str == "false" || enabled_str == "\"false\"" {
                    return Ok(Some(PolicyViolation {
                        policy_id: "WF001".to_string(),
                        policy_name: "Workflow Status".to_string(),
                        severity: ViolationSeverity::Critical,
                        message: format!("Workflow '{}' is disabled", workflow_id),
                        recommendation: Some("Enable workflow in admin panel".to_string()),
                    }));
                }
            }
        }

        Ok(None)
    }

    /// Detect if input contains sensitive fields (simple heuristic)
    fn contains_sensitive_fields(&self, data: &serde_json::Value) -> bool {
        let sensitive_keywords = [
            "password",
            "secret",
            "token",
            "key",
            "ssn",
            "credit_card",
            "bank_account",
            "api_key",
            "private_key",
        ];

        if let Some(obj) = data.as_object() {
            for key in obj.keys() {
                let key_lower = key.to_lowercase();
                if sensitive_keywords.iter().any(|k| key_lower.contains(k)) {
                    return true;
                }
            }
        }

        false
    }

    /// Detect PII fields in input (heuristic-based)
    fn detect_pii_fields(&self, data: &serde_json::Value) -> Vec<String> {
        let pii_keywords = [
            "email",
            "phone",
            "address",
            "ssn",
            "name",
            "firstname",
            "lastname",
            "dob",
            "birthdate",
            "passport",
            "license",
            "medical",
            "health",
        ];

        let mut pii_fields = Vec::new();

        if let Some(obj) = data.as_object() {
            for key in obj.keys() {
                let key_lower = key.to_lowercase();
                if pii_keywords.iter().any(|k| {
                    if *k == "name" {
                        // Avoid false positives like "username" while still catching
                        // common PII shapes such as "name", "first_name", "last_name".
                        key_lower == "name"
                            || key_lower.starts_with("name_")
                            || key_lower.ends_with("_name")
                    } else {
                        key_lower.contains(k)
                    }
                }) {
                    pii_fields.push(key.clone());
                }
            }
        }

        pii_fields
    }

    /// Record policy check result in RDF for audit trail
    pub async fn record_policy_check(
        &self,
        execution_id: &str,
        workflow_id: &str,
        result: &PolicyCheckResult,
    ) -> Result<()> {
        use crate::governance::rdf_store::RdfTriple;

        let check_uri = format!("http://graphica.io/policy-check/{}", execution_id);
        let exec_uri = format!("http://graphica.io/execution/{}", execution_id);
        let workflow_uri = format!("http://graphica.io/workflow/{}", workflow_id);

        let mut triples = vec![
            RdfTriple::new(
                &check_uri,
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
                "http://graphica.io/governance#PolicyCheck",
            ),
            RdfTriple::new(
                &check_uri,
                "http://graphica.io/governance#forExecution",
                &exec_uri,
            ),
            RdfTriple::new(
                &check_uri,
                "http://graphica.io/governance#forWorkflow",
                &workflow_uri,
            ),
            RdfTriple::new(
                &check_uri,
                "http://graphica.io/governance#allowed",
                &result.allowed.to_string(),
            ),
            RdfTriple::new(
                &check_uri,
                "http://graphica.io/governance#checkedAt",
                &chrono::Utc::now().to_rfc3339(),
            ),
        ];

        // Record violations
        for (idx, violation) in result.violations.iter().enumerate() {
            let violation_uri = format!("{}/violation/{}", check_uri, idx);
            triples.push(RdfTriple::new(
                &check_uri,
                "http://graphica.io/governance#hasViolation",
                &violation_uri,
            ));
            triples.push(RdfTriple::new(
                &violation_uri,
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
                "http://graphica.io/governance#PolicyViolation",
            ));
            triples.push(RdfTriple::new(
                &violation_uri,
                "http://graphica.io/governance#policyId",
                &violation.policy_id,
            ));
            triples.push(RdfTriple::new(
                &violation_uri,
                "http://graphica.io/governance#severity",
                &format!("{:?}", violation.severity),
            ));
            triples.push(RdfTriple::new(
                &violation_uri,
                "http://graphica.io/governance#message",
                &violation.message,
            ));
        }

        // Convert RdfTriples to tuple format for insertion
        let tuple_triples: Vec<(String, String, String)> =
            triples.into_iter().map(|t| t.to_tuple()).collect();

        self.rdf_store.insert_triples(tuple_triples, None)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_pii_fields() {
        let checker =
            GovernancePolicyChecker::new(Arc::new(GraphicaRdfStore::new_in_memory().unwrap()));

        let data = serde_json::json!({
            "email": "test@example.com",
            "username": "testuser",
            "phone_number": "555-1234",
            "age": 30
        });

        let pii_fields = checker.detect_pii_fields(&data);
        assert!(pii_fields.contains(&"email".to_string()));
        assert!(pii_fields.contains(&"phone_number".to_string()));
        assert!(!pii_fields.contains(&"username".to_string()));
        assert!(!pii_fields.contains(&"age".to_string()));
    }

    #[test]
    fn test_detect_sensitive_fields() {
        let checker =
            GovernancePolicyChecker::new(Arc::new(GraphicaRdfStore::new_in_memory().unwrap()));

        let data = serde_json::json!({
            "api_key": "sk_test_123",
            "username": "testuser",
            "password": "secret123"
        });

        assert!(checker.contains_sensitive_fields(&data));

        let safe_data = serde_json::json!({
            "username": "testuser",
            "age": 30
        });

        assert!(!checker.contains_sensitive_fields(&safe_data));
    }
}
