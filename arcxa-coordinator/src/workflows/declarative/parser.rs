//! Declarative workflow parser
//!
//! Provides functionality to parse YAML/JSON workflow definitions with comprehensive
//! error handling and validation.

use super::errors::ParseError;
use graphica_core::workflows::WorkflowSchema;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

/// Supported API version for workflow schemas
pub const SUPPORTED_API_VERSION: &str = "graphica.io/v1";

/// Supported workflow kind
pub const WORKFLOW_KIND: &str = "Workflow";

/// Parser for declarative workflow files
pub struct DeclarativeParser;

impl DeclarativeParser {
    /// Parse a workflow file (auto-detects format from extension)
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the workflow file (.yaml, .yml, or .json)
    ///
    /// # Returns
    ///
    /// * `Ok(WorkflowSchema)` - Successfully parsed workflow
    /// * `Err(ParseError)` - Parsing failed
    ///
    /// # Example
    ///
    /// ```ignore
    /// let schema = DeclarativeParser::parse_file("workflow.yaml")?;
    /// ```
    pub fn parse_file(path: impl AsRef<Path>) -> Result<WorkflowSchema, ParseError> {
        let path = path.as_ref();
        info!("Parsing workflow file: {}", path.display());

        // Check file exists
        if !path.exists() {
            return Err(ParseError::FileNotFound(path.to_path_buf()));
        }

        // Detect format from extension
        match path.extension().and_then(|s| s.to_str()) {
            Some("yaml") | Some("yml") => Self::parse_yaml_file(path),
            Some("json") => Self::parse_json_file(path),
            Some(ext) => Err(ParseError::UnsupportedFormat(ext.to_string())),
            None => Err(ParseError::UnsupportedFormat("no extension".to_string())),
        }
    }

    /// Parse a YAML workflow file
    pub fn parse_yaml_file(path: impl AsRef<Path>) -> Result<WorkflowSchema, ParseError> {
        let path = path.as_ref();
        debug!("Parsing YAML file: {}", path.display());

        // Read file contents
        let contents = fs::read_to_string(path).map_err(|e| ParseError::IoError {
            path: path.to_path_buf(),
            source: e,
        })?;

        // Check if empty
        if contents.trim().is_empty() {
            return Err(ParseError::EmptyFile(path.to_path_buf()));
        }

        // Parse YAML
        Self::parse_yaml(&contents, path)
    }

    /// Parse a JSON workflow file
    pub fn parse_json_file(path: impl AsRef<Path>) -> Result<WorkflowSchema, ParseError> {
        let path = path.as_ref();
        debug!("Parsing JSON file: {}", path.display());

        // Read file contents
        let contents = fs::read_to_string(path).map_err(|e| ParseError::IoError {
            path: path.to_path_buf(),
            source: e,
        })?;

        // Check if empty
        if contents.trim().is_empty() {
            return Err(ParseError::EmptyFile(path.to_path_buf()));
        }

        // Parse JSON
        Self::parse_json(&contents, path)
    }

    /// Parse YAML content from a string
    pub fn parse_yaml(content: &str, path: impl AsRef<Path>) -> Result<WorkflowSchema, ParseError> {
        let path = path.as_ref();

        // Deserialize YAML
        let schema: WorkflowSchema =
            serde_yaml::from_str(content).map_err(|e| ParseError::YamlError {
                path: path.to_path_buf(),
                source: e,
            })?;

        // Validate API version and kind
        Self::validate_metadata(&schema, path)?;

        info!(
            "Successfully parsed YAML workflow: {}",
            schema.metadata.name
        );

        Ok(schema)
    }

    /// Parse JSON content from a string
    pub fn parse_json(content: &str, path: impl AsRef<Path>) -> Result<WorkflowSchema, ParseError> {
        let path = path.as_ref();

        // Deserialize JSON
        let schema: WorkflowSchema =
            serde_json::from_str(content).map_err(|e| ParseError::JsonError {
                path: path.to_path_buf(),
                source: e,
            })?;

        // Validate API version and kind
        Self::validate_metadata(&schema, path)?;

        info!(
            "Successfully parsed JSON workflow: {}",
            schema.metadata.name
        );

        Ok(schema)
    }

    /// Parse multiple workflow files from a directory
    ///
    /// Recursively searches for .yaml, .yml, and .json files.
    ///
    /// # Arguments
    ///
    /// * `dir` - Directory path to search
    /// * `recursive` - Whether to search subdirectories
    ///
    /// # Returns
    ///
    /// Vector of successfully parsed workflows and vector of errors
    pub fn parse_directory(
        dir: impl AsRef<Path>,
        recursive: bool,
    ) -> (Vec<WorkflowSchema>, Vec<(PathBuf, ParseError)>) {
        let dir = dir.as_ref();
        let mut workflows = Vec::new();
        let mut errors = Vec::new();

        info!(
            "Parsing workflow directory: {} (recursive: {})",
            dir.display(),
            recursive
        );

        // Read directory entries
        let entries = match fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(e) => {
                errors.push((
                    dir.to_path_buf(),
                    ParseError::IoError {
                        path: dir.to_path_buf(),
                        source: e,
                    },
                ));
                return (workflows, errors);
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();

            if path.is_dir() && recursive {
                // Recursively parse subdirectory
                let (mut sub_workflows, mut sub_errors) = Self::parse_directory(&path, true);
                workflows.append(&mut sub_workflows);
                errors.append(&mut sub_errors);
            } else if path.is_file() {
                // Check if it's a workflow file
                if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                    if matches!(ext, "yaml" | "yml" | "json") {
                        match Self::parse_file(&path) {
                            Ok(schema) => workflows.push(schema),
                            Err(e) => errors.push((path, e)),
                        }
                    }
                }
            }
        }

        info!(
            "Parsed {} workflows from directory ({}  errors)",
            workflows.len(),
            errors.len()
        );

        (workflows, errors)
    }

    /// Validate workflow metadata (API version and kind)
    fn validate_metadata(
        schema: &WorkflowSchema,
        path: impl AsRef<Path>,
    ) -> Result<(), ParseError> {
        let path = path.as_ref();

        // Check API version
        if schema.api_version != SUPPORTED_API_VERSION {
            warn!(
                "Unsupported API version in {}: {} (expected: {})",
                path.display(),
                schema.api_version,
                SUPPORTED_API_VERSION
            );
            return Err(ParseError::InvalidApiVersion {
                found: schema.api_version.clone(),
                expected: SUPPORTED_API_VERSION.to_string(),
            });
        }

        // Check kind
        if schema.kind != WORKFLOW_KIND {
            warn!(
                "Invalid kind in {}: {} (expected: {})",
                path.display(),
                schema.kind,
                WORKFLOW_KIND
            );
            return Err(ParseError::InvalidKind {
                found: schema.kind.clone(),
                expected: WORKFLOW_KIND.to_string(),
            });
        }

        // Basic schema validation
        if schema.metadata.name.trim().is_empty() {
            return Err(ParseError::SchemaValidation(
                "Workflow name cannot be empty".to_string(),
            ));
        }

        if schema.spec.routes.is_empty() {
            return Err(ParseError::SchemaValidation(
                "Workflow must have at least one route".to_string(),
            ));
        }

        Ok(())
    }

    /// Validate a workflow schema after parsing
    ///
    /// This performs more thorough validation than the basic checks in parse.
    pub fn validate(schema: &WorkflowSchema) -> Result<(), ParseError> {
        // Validate metadata
        if schema.metadata.name.trim().is_empty() {
            return Err(ParseError::SchemaValidation(
                "Workflow name cannot be empty".to_string(),
            ));
        }

        // Validate routes
        if schema.spec.routes.is_empty() {
            return Err(ParseError::SchemaValidation(
                "Workflow must have at least one route".to_string(),
            ));
        }

        // Check for duplicate route names
        let mut route_names = std::collections::HashSet::new();
        for route in &schema.spec.routes {
            if !route_names.insert(&route.name) {
                return Err(ParseError::SchemaValidation(format!(
                    "Duplicate route name: {}",
                    route.name
                )));
            }
        }

        // Validate default route exists
        if let Some(ref default_route) = schema.spec.default_route {
            if !schema.spec.routes.iter().any(|r| &r.name == default_route) {
                return Err(ParseError::SchemaValidation(format!(
                    "Default route '{}' not found in routes",
                    default_route
                )));
            }
        }

        // Validate schedule if present
        if let Some(ref schedule) = schema.spec.schedule {
            // Basic cron validation (just check not empty)
            if schedule.cron.trim().is_empty() {
                return Err(ParseError::SchemaValidation(
                    "Cron expression cannot be empty".to_string(),
                ));
            }
        }

        // Validate quality threshold if present
        if let Some(ref monitoring) = schema.spec.monitoring {
            if let Some(threshold) = monitoring.quality_threshold {
                if !(0.0..=1.0).contains(&threshold) {
                    return Err(ParseError::SchemaValidation(format!(
                        "Quality threshold must be between 0.0 and 1.0, got {}",
                        threshold
                    )));
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphica_core::workflows::*;
    use std::collections::HashMap;
    use tempfile::NamedTempFile;

    fn create_test_schema() -> WorkflowSchema {
        WorkflowSchema {
            api_version: SUPPORTED_API_VERSION.to_string(),
            kind: WORKFLOW_KIND.to_string(),
            metadata: WorkflowMetadata {
                name: "test-workflow".to_string(),
                version: Some("1.0.0".to_string()),
                description: Some("Test workflow".to_string()),
                owner: Some("test-team".to_string()),
                tags: vec!["test".to_string()],
                annotations: HashMap::new(),
            },
            spec: WorkflowSpec {
                schedule: None,
                execution: ExecutionSpec::default(),
                routes: vec![RouteSpec {
                    name: "default".to_string(),
                    description: None,
                    priority: 0,
                    condition: ConditionSpec::Always,
                    actions: vec![ActionSpec::Log {
                        level: "info".to_string(),
                        message: "test".to_string(),
                    }],
                }],
                default_route: Some("default".to_string()),
                monitoring: None,
                resources: None,
            },
        }
    }

    #[test]
    fn test_parse_yaml_string() {
        let yaml = r#"
apiVersion: graphica.io/v1
kind: Workflow
metadata:
  name: test-workflow
  tags:
    - test
spec:
  routes:
    - name: default
      priority: 0
      condition:
        type: Always
      actions:
        - type: Log
          level: info
          message: test
"#;

        let result = DeclarativeParser::parse_yaml(yaml, "test.yaml");
        assert!(result.is_ok());

        let schema = result.unwrap();
        assert_eq!(schema.metadata.name, "test-workflow");
        assert_eq!(schema.spec.routes.len(), 1);
    }

    #[test]
    fn test_parse_json_string() {
        let json = r#"{
  "apiVersion": "graphica.io/v1",
  "kind": "Workflow",
  "metadata": {
    "name": "test-workflow",
    "tags": ["test"]
  },
  "spec": {
    "routes": [
      {
        "name": "default",
        "priority": 0,
        "condition": {"type": "Always"},
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

        let result = DeclarativeParser::parse_json(json, "test.json");
        assert!(result.is_ok());

        let schema = result.unwrap();
        assert_eq!(schema.metadata.name, "test-workflow");
    }

    #[test]
    fn test_parse_yaml_file() {
        let schema = create_test_schema();
        let yaml = serde_yaml::to_string(&schema).unwrap();

        let mut temp_file = NamedTempFile::new().unwrap();
        use std::io::Write;
        temp_file.write_all(yaml.as_bytes()).unwrap();
        temp_file.flush().unwrap();

        let result = DeclarativeParser::parse_yaml_file(temp_file.path());
        assert!(result.is_ok());

        let parsed = result.unwrap();
        assert_eq!(parsed.metadata.name, schema.metadata.name);
    }

    #[test]
    fn test_parse_json_file() {
        let schema = create_test_schema();
        let json = serde_json::to_string_pretty(&schema).unwrap();

        let mut temp_file = NamedTempFile::new().unwrap();
        use std::io::Write;
        temp_file.write_all(json.as_bytes()).unwrap();
        temp_file.flush().unwrap();

        let result = DeclarativeParser::parse_json_file(temp_file.path());
        assert!(result.is_ok());

        let parsed = result.unwrap();
        assert_eq!(parsed.metadata.name, schema.metadata.name);
    }

    #[test]
    fn test_parse_file_not_found() {
        let result = DeclarativeParser::parse_file("nonexistent.yaml");
        assert!(matches!(result, Err(ParseError::FileNotFound(_))));
    }

    #[test]
    fn test_parse_empty_file() {
        let mut temp_file = NamedTempFile::new().unwrap();
        use std::io::Write;
        temp_file.write_all(b"").unwrap();
        temp_file.flush().unwrap();

        let result = DeclarativeParser::parse_yaml_file(temp_file.path());
        assert!(matches!(result, Err(ParseError::EmptyFile(_))));
    }

    #[test]
    fn test_parse_invalid_api_version() {
        let yaml = r#"
apiVersion: invalid/v1
kind: Workflow
metadata:
  name: test
spec:
  routes:
    - name: default
      condition: {type: Always}
      actions: []
"#;

        let result = DeclarativeParser::parse_yaml(yaml, "test.yaml");
        assert!(matches!(result, Err(ParseError::InvalidApiVersion { .. })));
    }

    #[test]
    fn test_parse_invalid_kind() {
        let yaml = r#"
apiVersion: graphica.io/v1
kind: InvalidKind
metadata:
  name: test
spec:
  routes:
    - name: default
      condition: {type: Always}
      actions: []
"#;

        let result = DeclarativeParser::parse_yaml(yaml, "test.yaml");
        assert!(matches!(result, Err(ParseError::InvalidKind { .. })));
    }

    #[test]
    fn test_validate_empty_name() {
        let mut schema = create_test_schema();
        schema.metadata.name = "".to_string();

        let result = DeclarativeParser::validate(&schema);
        assert!(matches!(result, Err(ParseError::SchemaValidation(_))));
    }

    #[test]
    fn test_validate_no_routes() {
        let mut schema = create_test_schema();
        schema.spec.routes.clear();

        let result = DeclarativeParser::validate(&schema);
        assert!(matches!(result, Err(ParseError::SchemaValidation(_))));
    }

    #[test]
    fn test_validate_duplicate_route_names() {
        let mut schema = create_test_schema();
        schema.spec.routes.push(RouteSpec {
            name: "default".to_string(), // Duplicate
            description: None,
            priority: 1,
            condition: ConditionSpec::Always,
            actions: vec![],
        });

        let result = DeclarativeParser::validate(&schema);
        assert!(matches!(result, Err(ParseError::SchemaValidation(_))));
    }

    #[test]
    fn test_validate_invalid_default_route() {
        let mut schema = create_test_schema();
        schema.spec.default_route = Some("nonexistent".to_string());

        let result = DeclarativeParser::validate(&schema);
        assert!(matches!(result, Err(ParseError::SchemaValidation(_))));
    }

    #[test]
    fn test_validate_invalid_quality_threshold() {
        let mut schema = create_test_schema();
        schema.spec.monitoring = Some(MonitoringSpec {
            sla_minutes: None,
            quality_threshold: Some(1.5), // Invalid: > 1.0
            alerts: vec![],
        });

        let result = DeclarativeParser::validate(&schema);
        assert!(matches!(result, Err(ParseError::SchemaValidation(_))));
    }

    #[test]
    fn test_parse_directory() {
        use std::fs;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let schema = create_test_schema();

        // Create workflow file
        let workflow_path = temp_dir.path().join("workflow.yaml");
        let yaml = serde_yaml::to_string(&schema).unwrap();
        fs::write(&workflow_path, yaml).unwrap();

        // Parse directory
        let (workflows, errors) = DeclarativeParser::parse_directory(temp_dir.path(), false);

        assert_eq!(workflows.len(), 1);
        assert_eq!(errors.len(), 0);
        assert_eq!(workflows[0].metadata.name, "test-workflow");
    }

    #[test]
    fn test_parse_directory_recursive() {
        use std::fs;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let sub_dir = temp_dir.path().join("subdir");
        fs::create_dir(&sub_dir).unwrap();

        let schema = create_test_schema();

        // Create workflow in root
        let workflow1_path = temp_dir.path().join("workflow1.yaml");
        fs::write(&workflow1_path, serde_yaml::to_string(&schema).unwrap()).unwrap();

        // Create workflow in subdirectory
        let mut schema2 = schema.clone();
        schema2.metadata.name = "workflow2".to_string();
        let workflow2_path = sub_dir.join("workflow2.yaml");
        fs::write(&workflow2_path, serde_yaml::to_string(&schema2).unwrap()).unwrap();

        // Parse directory recursively
        let (workflows, errors) = DeclarativeParser::parse_directory(temp_dir.path(), true);

        assert_eq!(workflows.len(), 2);
        assert_eq!(errors.len(), 0);
    }
}
