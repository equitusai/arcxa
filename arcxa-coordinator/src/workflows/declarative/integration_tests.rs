//! Integration tests for declarative workflow system
//!
//! Tests the full pipeline: YAML → Schema → Domain → Schema → YAML

use super::{builder::WorkflowBuilder, parser::DeclarativeParser, serializer::WorkflowSerializer};
use graphica_core::workflows::*;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

#[cfg(test)]
mod tests {
    use super::*;

    /// Test roundtrip: YAML → WorkflowSchema → Workflow → WorkflowSchema → YAML
    #[test]
    fn test_roundtrip_simple_workflow() {
        let yaml = r#"
apiVersion: graphica.io/v1
kind: Workflow
metadata:
  name: test-workflow
  version: "1.0.0"
  description: Test workflow for roundtrip
  tags:
    - test
    - integration
spec:
  execution:
    mode: batch
    timeout: 3600
    retries: 3
    retryDelay: 300
  routes:
    - name: default
      description: Default route
      priority: 0
      condition:
        type: Always
      actions:
        - type: Log
          level: info
          message: Processing record
  defaultRoute: default
"#;

        // Parse YAML
        let schema1 =
            DeclarativeParser::parse_yaml(yaml, "test.yaml").expect("Failed to parse YAML");

        // Build domain workflow
        let workflow = WorkflowBuilder::build(&schema1).expect("Failed to build workflow");

        // Serialize back to schema
        let schema2 =
            WorkflowSerializer::to_schema(&workflow).expect("Failed to serialize workflow");

        // Verify key properties match
        assert_eq!(schema1.metadata.name, schema2.metadata.name);
        assert_eq!(schema1.metadata.description, schema2.metadata.description);
        assert_eq!(schema1.metadata.tags, schema2.metadata.tags);
        assert_eq!(schema1.spec.routes.len(), schema2.spec.routes.len());
        assert_eq!(schema1.spec.routes[0].name, schema2.spec.routes[0].name);
    }

    /// Test roundtrip with complex nested conditions
    #[test]
    fn test_roundtrip_complex_conditions() {
        let yaml = r#"
apiVersion: graphica.io/v1
kind: Workflow
metadata:
  name: complex-routing
  description: Workflow with complex conditions
spec:
  execution:
    mode: batch
  routes:
    - name: enterprise-route
      priority: 100
      condition:
        type: And
        conditions:
          - type: Equals
            field: customer_type
            value: "enterprise"
          - type: GreaterThan
            field: annual_revenue
            value: 1000000
          - type: Or
            conditions:
              - type: Equals
                field: region
                value: "US"
              - type: Equals
                field: region
                value: "EU"
      actions:
        - type: Log
          level: info
          message: Enterprise customer detected
    - name: default
      priority: 0
      condition:
        type: Always
      actions:
        - type: Log
          level: info
          message: Standard processing
"#;

        // Parse YAML
        let schema1 =
            DeclarativeParser::parse_yaml(yaml, "test.yaml").expect("Failed to parse YAML");

        // Build domain workflow
        let workflow = WorkflowBuilder::build(&schema1).expect("Failed to build workflow");

        // Verify workflow structure
        assert_eq!(workflow.routes.len(), 2);
        assert_eq!(workflow.routes[0].name, "enterprise-route");
        assert_eq!(workflow.routes[0].priority, 100);

        // Serialize back to schema
        let schema2 =
            WorkflowSerializer::to_schema(&workflow).expect("Failed to serialize workflow");

        // Verify roundtrip
        assert_eq!(schema1.spec.routes.len(), schema2.spec.routes.len());
        assert_eq!(
            schema1.spec.routes[0].priority,
            schema2.spec.routes[0].priority
        );
    }

    /// Test roundtrip with all supported action types
    #[test]
    fn test_roundtrip_all_action_types() {
        let yaml = r#"
apiVersion: graphica.io/v1
kind: Workflow
metadata:
  name: action-test
spec:
  execution:
    mode: batch
  routes:
    - name: multi-action
      condition:
        type: Always
      actions:
        - type: Log
          level: info
          message: Step 1
        - type: Transform
          transformer: uppercase
          config:
            field: name
"#;

        // Parse YAML
        let schema1 =
            DeclarativeParser::parse_yaml(yaml, "test.yaml").expect("Failed to parse YAML");

        // Build domain workflow
        let workflow = WorkflowBuilder::build(&schema1).expect("Failed to build workflow");

        // Verify actions
        assert_eq!(workflow.routes[0].actions.len(), 2);

        // Serialize back
        let schema2 =
            WorkflowSerializer::to_schema(&workflow).expect("Failed to serialize workflow");

        // Verify action count preserved
        assert_eq!(
            schema1.spec.routes[0].actions.len(),
            schema2.spec.routes[0].actions.len()
        );
    }

    /// Test file-based roundtrip (write → read → write)
    #[test]
    fn test_file_roundtrip() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let file1 = temp_dir.path().join("workflow1.yaml");
        let file2 = temp_dir.path().join("workflow2.yaml");

        let yaml = r#"
apiVersion: graphica.io/v1
kind: Workflow
metadata:
  name: file-test
  description: File roundtrip test
spec:
  execution:
    mode: batch
  routes:
    - name: default
      condition:
        type: Always
      actions:
        - type: Log
          level: info
          message: test
"#;

        // Write original YAML
        fs::write(&file1, yaml).expect("Failed to write file1");

        // Parse from file
        let schema1 = DeclarativeParser::parse_file(&file1).expect("Failed to parse file1");

        // Build workflow
        let workflow = WorkflowBuilder::build(&schema1).expect("Failed to build workflow");

        // Serialize to YAML
        let yaml2 = WorkflowSerializer::to_yaml(&workflow).expect("Failed to serialize to YAML");

        // Write to second file
        fs::write(&file2, &yaml2).expect("Failed to write file2");

        // Parse second file
        let schema2 = DeclarativeParser::parse_file(&file2).expect("Failed to parse file2");

        // Verify schemas match
        assert_eq!(schema1.metadata.name, schema2.metadata.name);
        assert_eq!(schema1.metadata.description, schema2.metadata.description);
        assert_eq!(schema1.spec.routes.len(), schema2.spec.routes.len());
    }

    /// Test JSON roundtrip
    #[test]
    fn test_json_roundtrip() {
        let json = r#"{
  "apiVersion": "graphica.io/v1",
  "kind": "Workflow",
  "metadata": {
    "name": "json-test",
    "tags": ["test"]
  },
  "spec": {
    "execution": {
      "mode": "batch",
      "timeout": 3600,
      "retries": 0,
      "retryDelay": 300
    },
    "routes": [
      {
        "name": "default",
        "priority": 0,
        "condition": {
          "type": "Always"
        },
        "actions": [
          {
            "type": "Log",
            "level": "info",
            "message": "test"
          }
        ]
      }
    ]
  }
}"#;

        // Parse JSON
        let schema1 =
            DeclarativeParser::parse_json(json, "test.json").expect("Failed to parse JSON");

        // Build workflow
        let workflow = WorkflowBuilder::build(&schema1).expect("Failed to build workflow");

        // Serialize back to JSON
        let json2 = WorkflowSerializer::to_json(&workflow).expect("Failed to serialize to JSON");

        // Parse again
        let schema2 =
            DeclarativeParser::parse_json(&json2, "test2.json").expect("Failed to parse JSON2");

        // Verify
        assert_eq!(schema1.metadata.name, schema2.metadata.name);
        assert_eq!(schema1.spec.routes.len(), schema2.spec.routes.len());
    }

    /// Test that validation errors are preserved across serialization
    #[test]
    fn test_validation_preserved() {
        let yaml = r#"
apiVersion: graphica.io/v1
kind: Workflow
metadata:
  name: validation-test
spec:
  execution:
    mode: batch
  routes:
    - name: route1
      condition:
        type: Equals
        field: status
        value: "active"
      actions:
        - type: Log
          level: info
          message: Active
    - name: route2
      condition:
        type: NotEquals
        field: status
        value: "inactive"
      actions:
        - type: Log
          level: warn
          message: Not inactive
  defaultRoute: route1
"#;

        // Parse and validate
        let schema1 =
            DeclarativeParser::parse_yaml(yaml, "test.yaml").expect("Failed to parse YAML");
        DeclarativeParser::validate(&schema1).expect("Validation failed");

        // Build workflow
        let workflow = WorkflowBuilder::build(&schema1).expect("Failed to build workflow");

        // Verify default route is set correctly
        assert_eq!(workflow.default_route, Some("route1".to_string()));

        // Serialize back
        let schema2 =
            WorkflowSerializer::to_schema(&workflow).expect("Failed to serialize workflow");

        // Validate again
        DeclarativeParser::validate(&schema2).expect("Validation failed after roundtrip");

        // Verify default route preserved
        assert_eq!(schema2.spec.default_route, Some("route1".to_string()));
    }

    /// Test regex pattern preservation
    #[test]
    fn test_regex_pattern_roundtrip() {
        let yaml = r#"
apiVersion: graphica.io/v1
kind: Workflow
metadata:
  name: regex-test
spec:
  execution:
    mode: batch
  routes:
    - name: email-validation
      condition:
        type: Regex
        field: email
        pattern: "^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\\.[a-zA-Z]{2,}$"
      actions:
        - type: Log
          level: info
          message: Valid email
"#;

        // Parse YAML
        let schema1 =
            DeclarativeParser::parse_yaml(yaml, "test.yaml").expect("Failed to parse YAML");

        // Build workflow
        let workflow = WorkflowBuilder::build(&schema1).expect("Failed to build workflow");

        // Serialize back
        let schema2 =
            WorkflowSerializer::to_schema(&workflow).expect("Failed to serialize workflow");

        // Verify regex pattern preserved
        if let ConditionSpec::Regex { pattern, .. } = &schema2.spec.routes[0].condition {
            assert!(pattern.contains("@"));
            assert!(pattern.contains("+"));
        } else {
            panic!("Expected Regex condition");
        }
    }

    /// Test multiple routes with different priorities
    #[test]
    fn test_priority_preservation() {
        let yaml = r#"
apiVersion: graphica.io/v1
kind: Workflow
metadata:
  name: priority-test
spec:
  execution:
    mode: batch
  routes:
    - name: high
      priority: 100
      condition:
        type: Equals
        field: priority
        value: "high"
      actions:
        - type: Log
          level: warn
          message: High priority
    - name: medium
      priority: 50
      condition:
        type: Equals
        field: priority
        value: "medium"
      actions:
        - type: Log
          level: info
          message: Medium priority
    - name: low
      priority: 10
      condition:
        type: Always
      actions:
        - type: Log
          level: debug
          message: Low priority
"#;

        // Parse YAML
        let schema1 =
            DeclarativeParser::parse_yaml(yaml, "test.yaml").expect("Failed to parse YAML");

        // Build workflow
        let workflow = WorkflowBuilder::build(&schema1).expect("Failed to build workflow");

        // Verify priorities
        assert_eq!(workflow.routes[0].priority, 100);
        assert_eq!(workflow.routes[1].priority, 50);
        assert_eq!(workflow.routes[2].priority, 10);

        // Serialize back
        let schema2 =
            WorkflowSerializer::to_schema(&workflow).expect("Failed to serialize workflow");

        // Verify priorities preserved
        assert_eq!(schema2.spec.routes[0].priority, 100);
        assert_eq!(schema2.spec.routes[1].priority, 50);
        assert_eq!(schema2.spec.routes[2].priority, 10);
    }

    /// Test NOT condition roundtrip
    #[test]
    fn test_not_condition_roundtrip() {
        let yaml = r#"
apiVersion: graphica.io/v1
kind: Workflow
metadata:
  name: not-test
spec:
  execution:
    mode: batch
  routes:
    - name: not-null
      condition:
        type: Not
        condition:
          type: IsNull
          field: customer_id
      actions:
        - type: Log
          level: info
          message: Customer ID exists
"#;

        // Parse YAML
        let schema1 =
            DeclarativeParser::parse_yaml(yaml, "test.yaml").expect("Failed to parse YAML");

        // Build workflow
        let workflow = WorkflowBuilder::build(&schema1).expect("Failed to build workflow");

        // Serialize back
        let schema2 =
            WorkflowSerializer::to_schema(&workflow).expect("Failed to serialize workflow");

        // Verify NOT condition preserved
        if let ConditionSpec::Not { condition } = &schema2.spec.routes[0].condition {
            assert!(matches!(**condition, ConditionSpec::IsNull { .. }));
        } else {
            panic!("Expected Not condition");
        }
    }

    /// Test deeply nested conditions
    #[test]
    fn test_deeply_nested_conditions() {
        let yaml = r#"
apiVersion: graphica.io/v1
kind: Workflow
metadata:
  name: nested-test
spec:
  execution:
    mode: batch
  routes:
    - name: complex
      condition:
        type: And
        conditions:
          - type: Or
            conditions:
              - type: Equals
                field: region
                value: "US"
              - type: Equals
                field: region
                value: "EU"
          - type: Not
            condition:
              type: IsNull
              field: customer_id
          - type: GreaterThan
            field: amount
            value: 100
      actions:
        - type: Log
          level: info
          message: Complex condition matched
"#;

        // Parse YAML
        let schema1 =
            DeclarativeParser::parse_yaml(yaml, "test.yaml").expect("Failed to parse YAML");

        // Build workflow
        let workflow = WorkflowBuilder::build(&schema1).expect("Failed to build workflow");

        // Serialize back
        let schema2 =
            WorkflowSerializer::to_schema(&workflow).expect("Failed to serialize workflow");

        // Verify structure preserved (basic check)
        if let ConditionSpec::And { conditions } = &schema2.spec.routes[0].condition {
            assert_eq!(conditions.len(), 3);
        } else {
            panic!("Expected And condition");
        }
    }
}
