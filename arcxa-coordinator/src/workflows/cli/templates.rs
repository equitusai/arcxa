//! Workflow templates for quick scaffolding

use anyhow::{Context, Result};

/// Get template content by name
pub fn get_template(template_name: &str) -> Result<&'static str> {
    match template_name {
        "basic" => Ok(BASIC_TEMPLATE),
        "advanced" => Ok(ADVANCED_TEMPLATE),
        "ml-enrichment" => Ok(ML_ENRICHMENT_TEMPLATE),
        "data-quality" => Ok(DATA_QUALITY_TEMPLATE),
        _ => anyhow::bail!("Unknown template: {}", template_name),
    }
}

/// List available templates
pub fn list_templates() -> Vec<(&'static str, &'static str)> {
    vec![
        ("basic", "Simple workflow with single route"),
        ("advanced", "Multi-route workflow with conditions"),
        ("ml-enrichment", "ML model enrichment workflow"),
        ("data-quality", "Data quality validation workflow"),
    ]
}

/// Basic workflow template
const BASIC_TEMPLATE: &str = r#"apiVersion: graphica.io/v1
kind: Workflow
metadata:
  name: {{name}}
  version: 1.0.0
  description: Basic workflow template
  owner: data-team
  tags:
    - template
    - basic

spec:
  # Execution configuration
  execution:
    mode: streaming
    timeout: 300
    retries: 3

  # Default route
  default_route: process

  # Routes
  routes:
    - name: process
      priority: 100
      condition:
        Always: true

      actions:
        - type: Log
          level: info
          message: "Processing record"

        - type: Transform
          transformer: identity
          output_schema:
            - "*"

  # Monitoring
  monitoring:
    enabled: true
    quality_threshold: 0.95
    alerts:
      - type: email
        recipients:
          - data-team@example.com
        on_failure: true
"#;

/// Advanced workflow template with multiple routes
const ADVANCED_TEMPLATE: &str = r#"apiVersion: graphica.io/v1
kind: Workflow
metadata:
  name: {{name}}
  version: 1.0.0
  description: Advanced multi-route workflow
  owner: data-team
  tags:
    - template
    - advanced
    - multi-route

spec:
  # Schedule (optional)
  schedule:
    cron: "0 */6 * * *"
    timezone: UTC

  # Execution configuration
  execution:
    mode: streaming
    timeout: 600
    retries: 5

  # Default route for unmatched records
  default_route: fallback

  # Multiple routes with conditions
  routes:
    # High-priority records
    - name: high-priority
      priority: 100
      condition:
        Field:
          field: priority
          operator: Equals
          value: "high"

      actions:
        - type: Log
          level: info
          message: "Processing high-priority record"

        - type: Enrich
          reference_data: premium_customers
          join_key: customer_id

        - type: Validate
          rule_id: high_priority_rules

    # Standard processing
    - name: standard
      priority: 50
      condition:
        Field:
          field: priority
          operator: In
          value: ["medium", "low"]

      actions:
        - type: Log
          level: info
          message: "Processing standard record"

        - type: Transform
          transformer: standardize
          output_schema:
            - customer_id
            - amount
            - timestamp

    # Fallback route
    - name: fallback
      priority: 0
      condition:
        Always: true

      actions:
        - type: Log
          level: warn
          message: "Record matched no specific route"

  # Resource limits
  resources:
    memory_mb: 512
    cpu_cores: 2.0

  # Monitoring
  monitoring:
    enabled: true
    quality_threshold: 0.95
    sla:
      latency_p99_ms: 1000
      throughput_per_sec: 10000
    alerts:
      - type: email
        recipients:
          - data-team@example.com
        on_failure: true
      - type: slack
        webhook: https://hooks.slack.com/services/YOUR/WEBHOOK/URL
        on_quality_drop: true
"#;

/// ML enrichment workflow template
const ML_ENRICHMENT_TEMPLATE: &str = r#"apiVersion: graphica.io/v1
kind: Workflow
metadata:
  name: {{name}}
  version: 1.0.0
  description: ML model enrichment workflow
  owner: ml-team
  tags:
    - template
    - ml
    - enrichment

spec:
  execution:
    mode: streaming
    timeout: 300
    retries: 3

  default_route: enrich

  routes:
    - name: enrich
      priority: 100
      condition:
        Always: true

      actions:
        # Validate input data
        - type: Validate
          rule_id: input_schema_check

        # Enrich with reference data
        - type: Enrich
          reference_data: customer_profiles
          join_key: customer_id

        # Call ML model
        - type: CallModel
          model_id: customer_segmentation_v2
          input_fields:
            - age
            - income
            - purchase_history
          output_fields:
            - segment
            - propensity_score

        # Log enrichment
        - type: Log
          level: info
          message: "Enriched with ML predictions"

        # Transform output
        - type: Transform
          transformer: ml_output_formatter
          output_schema:
            - customer_id
            - segment
            - propensity_score
            - enriched_at

  # Resource limits (ML models need more memory)
  resources:
    memory_mb: 2048
    cpu_cores: 4.0

  # Monitoring
  monitoring:
    enabled: true
    quality_threshold: 0.90
    model_drift_detection: true
    alerts:
      - type: email
        recipients:
          - ml-team@example.com
        on_failure: true
      - type: pagerduty
        integration_key: YOUR_KEY_HERE
        on_model_drift: true
"#;

/// Data quality validation workflow template
const DATA_QUALITY_TEMPLATE: &str = r#"apiVersion: graphica.io/v1
kind: Workflow
metadata:
  name: {{name}}
  version: 1.0.0
  description: Data quality validation workflow
  owner: dq-team
  tags:
    - template
    - data-quality
    - validation

spec:
  execution:
    mode: streaming
    timeout: 300
    retries: 3

  default_route: quarantine

  routes:
    # Valid records
    - name: valid
      priority: 100
      condition:
        And:
          - Field:
              field: email
              operator: Regex
              value: "^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\\.[a-zA-Z]{2,}$"
          - Field:
              field: age
              operator: GreaterThan
              value: "0"
          - Field:
              field: customer_id
              operator: NotNull
              value: null

      actions:
        - type: Log
          level: info
          message: "Record passed validation"

        - type: Validate
          rule_id: advanced_quality_rules

        - type: Route
          destination: downstream_kafka_topic

    # Invalid records - quarantine
    - name: quarantine
      priority: 0
      condition:
        Always: true

      actions:
        - type: Log
          level: error
          message: "Record failed validation - quarantined"

        - type: Route
          destination: quarantine_topic

  # Monitoring with strict quality thresholds
  monitoring:
    enabled: true
    quality_threshold: 0.98
    sla:
      latency_p99_ms: 500
      throughput_per_sec: 50000
    alerts:
      - type: email
        recipients:
          - dq-team@example.com
        on_quality_drop: true
      - type: slack
        webhook: https://hooks.slack.com/services/YOUR/WEBHOOK/URL
        on_failure: true
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_basic_template() {
        let template = get_template("basic").unwrap();
        assert!(template.contains("{{name}}"));
        assert!(template.contains("apiVersion: graphica.io/v1"));
    }

    #[test]
    fn test_get_advanced_template() {
        let template = get_template("advanced").unwrap();
        assert!(template.contains("high-priority"));
        assert!(template.contains("standard"));
        assert!(template.contains("fallback"));
    }

    #[test]
    fn test_get_ml_template() {
        let template = get_template("ml-enrichment").unwrap();
        assert!(template.contains("CallModel"));
        assert!(template.contains("model_id"));
    }

    #[test]
    fn test_get_dq_template() {
        let template = get_template("data-quality").unwrap();
        assert!(template.contains("quarantine"));
        assert!(template.contains("Validate"));
    }

    #[test]
    fn test_unknown_template() {
        let result = get_template("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_list_templates() {
        let templates = list_templates();
        assert_eq!(templates.len(), 4);
        assert_eq!(templates[0].0, "basic");
        assert_eq!(templates[1].0, "advanced");
    }

    #[test]
    fn test_template_variable_replacement() {
        let template = get_template("basic").unwrap();
        let result = template.replace("{{name}}", "my-workflow");
        assert!(result.contains("name: my-workflow"));
        assert!(!result.contains("{{name}}"));
    }
}
