//! Concrete validator implementations for workflow schemas
//!
//! This module provides production-ready validators that implement the
//! Validator trait to perform comprehensive workflow validation.

use super::errors::{ValidationError, ValidationWarning};
use super::schema::*;
use super::validation::{ValidationResult, Validator};
use std::collections::{HashMap, HashSet};

/// Schema structure validator
///
/// Validates basic workflow structure, required fields, and data types.
/// This is the first validator that should run.
pub struct SchemaValidator;

impl Validator for SchemaValidator {
    fn validate(&self, schema: &WorkflowSchema) -> ValidationResult {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        // Validate API version
        if schema.api_version != "graphica.io/v1" {
            errors.push(ValidationError::InvalidApiVersion(
                schema.api_version.clone(),
                "graphica.io/v1".to_string(),
            ));
        }

        // Validate kind
        if schema.kind != "Workflow" {
            errors.push(ValidationError::InvalidKind(schema.kind.clone()));
        }

        // Validate metadata
        if schema.metadata.name.is_empty() {
            errors.push(ValidationError::EmptyWorkflowName);
        }

        if schema.metadata.name.len() > 253 {
            errors.push(ValidationError::WorkflowNameTooLong {
                length: schema.metadata.name.len(),
            });
        }

        // Validate version format (if present)
        if let Some(ref version) = schema.metadata.version {
            if !Self::is_valid_version(version) {
                warnings.push(ValidationWarning::InvalidVersionFormat {
                    version: version.clone(),
                });
            }
        }

        // Validate routes exist
        if schema.spec.routes.is_empty() {
            errors.push(ValidationError::NoRoutes);
        }

        // Validate route names
        let mut route_names = HashSet::new();
        for route in &schema.spec.routes {
            if route.name.is_empty() {
                errors.push(ValidationError::EmptyRouteName);
            }

            if !route_names.insert(&route.name) {
                errors.push(ValidationError::DuplicateRouteName(route.name.clone()));
            }

            // Validate priority range
            if route.priority < 0 {
                warnings.push(ValidationWarning::NegativePriority {
                    route: route.name.clone(),
                    priority: route.priority,
                });
            }

            if route.priority > 10000 {
                warnings.push(ValidationWarning::ExtremelyHighPriority {
                    route: route.name.clone(),
                    priority: route.priority,
                });
            }

            // Validate actions
            if route.actions.is_empty() {
                warnings.push(ValidationWarning::RouteWithoutActions {
                    route: route.name.clone(),
                });
            }
        }

        // Validate default route exists
        if let Some(ref default_route) = schema.spec.default_route {
            if !route_names.contains(default_route) {
                errors.push(ValidationError::DefaultRouteNotFound(default_route.clone()));
            }
        } else if schema.spec.routes.len() > 1 {
            warnings.push(ValidationWarning::NoDefaultRoute);
        }

        // Validate execution spec
        if schema.spec.execution.timeout == 0 {
            errors.push(ValidationError::InvalidTimeout(0));
        }

        // Retries are u32 so can't be negative, no validation needed
        // But warn if excessive
        if schema.spec.execution.retries > 10 {
            warnings.push(ValidationWarning::ExcessiveRetries {
                retries: schema.spec.execution.retries as i32,
            });
        }

        // Validate monitoring thresholds (if present)
        if let Some(ref monitoring) = schema.spec.monitoring {
            if let Some(threshold) = monitoring.quality_threshold {
                if threshold < 0.0 || threshold > 1.0 {
                    errors.push(ValidationError::InvalidQualityThreshold(threshold));
                }
            }
        }

        ValidationResult {
            valid: errors.is_empty(),
            errors,
            warnings,
        }
    }

    fn name(&self) -> &str {
        "SchemaValidator"
    }
}

impl SchemaValidator {
    /// Check if version follows semver pattern (basic check)
    fn is_valid_version(version: &str) -> bool {
        let parts: Vec<&str> = version.split('.').collect();
        if parts.len() != 3 {
            return false;
        }
        parts.iter().all(|p| p.parse::<u32>().is_ok())
    }
}

/// Semantic logic validator
///
/// Validates condition logic, detects unreachable routes, and identifies
/// logical contradictions in condition expressions.
pub struct SemanticValidator;

impl Validator for SemanticValidator {
    fn validate(&self, schema: &WorkflowSchema) -> ValidationResult {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        // Check for Always conditions that block other routes
        let mut has_always = false;
        for (i, route) in schema.spec.routes.iter().enumerate() {
            if matches!(route.condition, ConditionSpec::Always) {
                if has_always {
                    warnings.push(ValidationWarning::MultipleAlwaysConditions {
                        route: route.name.clone(),
                    });
                }

                // Check if there are lower-priority routes after this one
                let higher_or_equal_priority = route.priority;
                for other in &schema.spec.routes[i + 1..] {
                    if other.priority <= higher_or_equal_priority {
                        warnings.push(ValidationWarning::UnreachableRoute {
                            route: other.name.clone(),
                        });
                    }
                }

                has_always = true;
            }

            // Validate condition structure
            Self::validate_condition(&route.condition, &route.name, &mut errors, &mut warnings);
        }

        // Check for contradictory conditions
        Self::check_contradictions(&schema.spec.routes, &mut warnings);

        ValidationResult {
            valid: errors.is_empty(),
            errors,
            warnings,
        }
    }

    fn name(&self) -> &str {
        "SemanticValidator"
    }
}

impl SemanticValidator {
    /// Recursively validate condition structure
    fn validate_condition(
        condition: &ConditionSpec,
        route_name: &str,
        errors: &mut Vec<ValidationError>,
        warnings: &mut Vec<ValidationWarning>,
    ) {
        match condition {
            ConditionSpec::Equals { field, .. }
            | ConditionSpec::NotEquals { field, .. }
            | ConditionSpec::GreaterThan { field, .. }
            | ConditionSpec::LessThan { field, .. }
            | ConditionSpec::Contains { field, .. }
            | ConditionSpec::IsNull { field } => {
                if field.is_empty() {
                    errors.push(ValidationError::EmptyFieldName {
                        route: route_name.to_string(),
                    });
                }
            }

            ConditionSpec::Regex { field, pattern } => {
                if field.is_empty() {
                    errors.push(ValidationError::EmptyFieldName {
                        route: route_name.to_string(),
                    });
                }
                // Validate regex pattern
                if let Err(_e) = regex::Regex::new(pattern) {
                    errors.push(ValidationError::InvalidRegexPattern(pattern.clone()));
                }
            }

            ConditionSpec::And { conditions } | ConditionSpec::Or { conditions } => {
                if conditions.is_empty() {
                    errors.push(ValidationError::EmptyLogicalOperator {
                        route: route_name.to_string(),
                        operator: if matches!(condition, ConditionSpec::And { .. }) {
                            "AND".to_string()
                        } else {
                            "OR".to_string()
                        },
                    });
                }

                if conditions.len() == 1 {
                    warnings.push(ValidationWarning::SingleConditionInLogicalOperator {
                        route: route_name.to_string(),
                    });
                }

                // Recursively validate nested conditions
                for cond in conditions {
                    Self::validate_condition(cond, route_name, errors, warnings);
                }
            }

            ConditionSpec::Not { condition } => {
                // Check for double negation
                if matches!(**condition, ConditionSpec::Not { .. }) {
                    warnings.push(ValidationWarning::DoubleNegation {
                        route: route_name.to_string(),
                    });
                }

                Self::validate_condition(condition, route_name, errors, warnings);
            }

            ConditionSpec::Always => {
                // Already handled in main validation
            }
        }
    }

    /// Check for contradictory conditions between routes
    fn check_contradictions(routes: &[RouteSpec], warnings: &mut Vec<ValidationWarning>) {
        // Simple contradiction detection: field = value AND field != value
        for route in routes {
            let equalities = Self::extract_equalities(&route.condition);
            for (field, values) in equalities {
                if values.len() > 1 {
                    warnings.push(ValidationWarning::ContradictoryConditions {
                        route: route.name.clone(),
                        field,
                    });
                }
            }
        }
    }

    /// Extract field equality conditions
    fn extract_equalities(condition: &ConditionSpec) -> HashMap<String, Vec<serde_json::Value>> {
        let mut equalities: HashMap<String, Vec<serde_json::Value>> = HashMap::new();

        match condition {
            ConditionSpec::Equals { field, value } => {
                equalities
                    .entry(field.clone())
                    .or_default()
                    .push(value.clone());
            }
            ConditionSpec::And { conditions } => {
                for cond in conditions {
                    let nested = Self::extract_equalities(cond);
                    for (field, values) in nested {
                        equalities.entry(field).or_default().extend(values);
                    }
                }
            }
            _ => {}
        }

        equalities
    }
}

/// Action dependency validator
///
/// Validates that actions have required dependencies and prerequisites.
pub struct DependencyValidator;

impl Validator for DependencyValidator {
    fn validate(&self, schema: &WorkflowSchema) -> ValidationResult {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        for route in &schema.spec.routes {
            for (i, action) in route.actions.iter().enumerate() {
                match action {
                    ActionSpec::Enrich {
                        reference_data,
                        join_key,
                    } => {
                        if reference_data.is_empty() {
                            errors.push(ValidationError::EmptyReferenceData {
                                route: route.name.clone(),
                            });
                        }
                        if join_key.is_empty() {
                            errors.push(ValidationError::EmptyJoinKey {
                                route: route.name.clone(),
                            });
                        }
                    }

                    ActionSpec::Validate { rule_id } => {
                        if rule_id.is_empty() {
                            errors.push(ValidationError::EmptyRuleId {
                                route: route.name.clone(),
                            });
                        }
                    }

                    ActionSpec::SendToKafka { topic, .. } => {
                        if topic.is_empty() {
                            errors.push(ValidationError::EmptyKafkaTopic {
                                route: route.name.clone(),
                            });
                        }
                    }

                    ActionSpec::SendToHttp { url, method, .. } => {
                        if url.is_empty() {
                            errors.push(ValidationError::EmptyHttpUrl {
                                route: route.name.clone(),
                            });
                        }

                        // Basic URL validation
                        if !url.starts_with("http://") && !url.starts_with("https://") {
                            warnings.push(ValidationWarning::InvalidHttpUrl {
                                route: route.name.clone(),
                                url: url.clone(),
                            });
                        }

                        // Validate HTTP method
                        if !Self::is_valid_http_method(method) {
                            errors.push(ValidationError::InvalidHttpMethod {
                                route: route.name.clone(),
                                method: method.clone(),
                            });
                        }
                    }

                    ActionSpec::ExecuteCode { language, code } => {
                        if code.is_empty() {
                            errors.push(ValidationError::EmptyCode {
                                route: route.name.clone(),
                            });
                        }

                        if !Self::is_supported_language(language) {
                            warnings.push(ValidationWarning::UnsupportedLanguage {
                                route: route.name.clone(),
                                language: language.clone(),
                            });
                        }
                    }

                    ActionSpec::CallModel { model_id, .. } => {
                        if model_id.is_empty() {
                            errors.push(ValidationError::EmptyModelId {
                                route: route.name.clone(),
                            });
                        }
                    }

                    ActionSpec::Log { level, message } => {
                        if message.is_empty() {
                            warnings.push(ValidationWarning::EmptyLogMessage {
                                route: route.name.clone(),
                            });
                        }

                        if !Self::is_valid_log_level(level) {
                            warnings.push(ValidationWarning::InvalidLogLevel {
                                route: route.name.clone(),
                                level: level.clone(),
                            });
                        }
                    }

                    ActionSpec::Transform { transformer, .. } => {
                        if transformer.is_empty() {
                            errors.push(ValidationError::EmptyTransformer {
                                route: route.name.clone(),
                            });
                        }
                    }
                }

                // Check for potentially expensive operations at the end
                if i < route.actions.len() - 1 {
                    if matches!(
                        action,
                        ActionSpec::CallModel { .. } | ActionSpec::SendToHttp { .. }
                    ) {
                        warnings.push(ValidationWarning::ExpensiveOperationNotLast {
                            route: route.name.clone(),
                        });
                    }
                }
            }
        }

        ValidationResult {
            valid: errors.is_empty(),
            errors,
            warnings,
        }
    }

    fn name(&self) -> &str {
        "DependencyValidator"
    }
}

impl DependencyValidator {
    fn is_valid_http_method(method: &str) -> bool {
        matches!(
            method.to_uppercase().as_str(),
            "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS"
        )
    }

    fn is_supported_language(language: &str) -> bool {
        matches!(
            language.to_lowercase().as_str(),
            "javascript" | "python" | "wasm"
        )
    }

    fn is_valid_log_level(level: &str) -> bool {
        matches!(
            level.to_lowercase().as_str(),
            "trace" | "debug" | "info" | "warn" | "error"
        )
    }
}

/// Resource constraint validator
///
/// Validates resource limits, quotas, and constraints.
pub struct ResourceValidator;

impl Validator for ResourceValidator {
    fn validate(&self, schema: &WorkflowSchema) -> ValidationResult {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        // Validate execution timeouts
        if schema.spec.execution.timeout > 86400 {
            // 24 hours
            warnings.push(ValidationWarning::VeryLongTimeout {
                timeout: schema.spec.execution.timeout as i64,
            });
        }

        if schema.spec.execution.retries > 10 {
            warnings.push(ValidationWarning::ExcessiveRetries {
                retries: schema.spec.execution.retries as i32,
            });
        }

        // Validate resource spec if present
        if let Some(ref resources) = schema.spec.resources {
            // Memory validation
            if let Some(memory_mb) = resources.memory_mb {
                if memory_mb == 0 {
                    errors.push(ValidationError::InvalidMemoryLimit { limit: 0 });
                }

                if memory_mb > 32768 {
                    // 32GB
                    warnings.push(ValidationWarning::ExcessiveMemoryRequest {
                        memory_mb: memory_mb as i32,
                    });
                }
            }

            // CPU validation
            if let Some(cpu_cores) = resources.cpu_cores {
                if cpu_cores == 0 {
                    errors.push(ValidationError::InvalidCpuLimit { limit: 0.0 });
                }

                if cpu_cores > 32 {
                    warnings.push(ValidationWarning::ExcessiveCpuRequest {
                        cpu_cores: cpu_cores as f64,
                    });
                }
            }
        }

        // Validate schedule cron if present
        if let Some(ref schedule) = schema.spec.schedule {
            if schedule.cron.is_empty() {
                errors.push(ValidationError::EmptyCronExpression);
            }
            // Basic cron validation (5 or 6 fields)
            let parts: Vec<&str> = schedule.cron.split_whitespace().collect();
            if parts.len() < 5 || parts.len() > 6 {
                errors.push(ValidationError::InvalidCronExpression(
                    schedule.cron.clone(),
                ));
            }
        }

        // Validate route complexity
        for route in &schema.spec.routes {
            let depth = Self::condition_depth(&route.condition);
            if depth > 10 {
                warnings.push(ValidationWarning::DeeplyNestedConditions {
                    route: route.name.clone(),
                    depth,
                });
            }

            if route.actions.len() > 20 {
                warnings.push(ValidationWarning::TooManyActions {
                    route: route.name.clone(),
                    count: route.actions.len(),
                });
            }
        }

        ValidationResult {
            valid: errors.is_empty(),
            errors,
            warnings,
        }
    }

    fn name(&self) -> &str {
        "ResourceValidator"
    }
}

impl ResourceValidator {
    /// Calculate condition nesting depth
    fn condition_depth(condition: &ConditionSpec) -> usize {
        match condition {
            ConditionSpec::And { conditions } | ConditionSpec::Or { conditions } => {
                1 + conditions
                    .iter()
                    .map(Self::condition_depth)
                    .max()
                    .unwrap_or(0)
            }
            ConditionSpec::Not { condition } => 1 + Self::condition_depth(condition),
            _ => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_valid_schema() -> WorkflowSchema {
        WorkflowSchema {
            api_version: "graphica.io/v1".to_string(),
            kind: "Workflow".to_string(),
            metadata: WorkflowMetadata {
                name: "test-workflow".to_string(),
                version: Some("1.0.0".to_string()),
                description: None,
                owner: None,
                tags: vec![],
                annotations: HashMap::new(),
            },
            spec: WorkflowSpec {
                schedule: None,
                execution: ExecutionSpec {
                    mode: ExecutionMode::Batch,
                    timeout: 3600,
                    retries: 3,
                    retry_delay: 300,
                },
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

    // SchemaValidator tests
    #[test]
    fn test_schema_validator_valid() {
        let schema = create_valid_schema();
        let validator = SchemaValidator;
        let result = validator.validate(&schema);
        assert!(result.valid);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_schema_validator_invalid_api_version() {
        let mut schema = create_valid_schema();
        schema.api_version = "wrong/v1".to_string();
        let validator = SchemaValidator;
        let result = validator.validate(&schema);
        assert!(!result.valid);
        assert!(result
            .errors
            .iter()
            .any(|e| matches!(e, ValidationError::InvalidApiVersion(_, _))));
    }

    #[test]
    fn test_schema_validator_empty_name() {
        let mut schema = create_valid_schema();
        schema.metadata.name = String::new();
        let validator = SchemaValidator;
        let result = validator.validate(&schema);
        assert!(!result.valid);
        assert!(result
            .errors
            .iter()
            .any(|e| matches!(e, ValidationError::EmptyWorkflowName)));
    }

    #[test]
    fn test_schema_validator_duplicate_routes() {
        let mut schema = create_valid_schema();
        schema.spec.routes.push(RouteSpec {
            name: "default".to_string(),
            description: None,
            priority: 1,
            condition: ConditionSpec::Always,
            actions: vec![],
        });
        let validator = SchemaValidator;
        let result = validator.validate(&schema);
        assert!(!result.valid);
        assert!(result
            .errors
            .iter()
            .any(|e| matches!(e, ValidationError::DuplicateRouteName(_))));
    }

    #[test]
    fn test_schema_validator_no_routes() {
        let mut schema = create_valid_schema();
        schema.spec.routes.clear();
        let validator = SchemaValidator;
        let result = validator.validate(&schema);
        assert!(!result.valid);
        assert!(result
            .errors
            .iter()
            .any(|e| matches!(e, ValidationError::NoRoutes)));
    }

    #[test]
    fn test_schema_validator_invalid_version() {
        let mut schema = create_valid_schema();
        schema.metadata.version = Some("invalid".to_string());
        let validator = SchemaValidator;
        let result = validator.validate(&schema);
        assert!(result.valid); // Warning, not error
        assert!(!result.warnings.is_empty());
    }

    // SemanticValidator tests
    #[test]
    fn test_semantic_validator_valid() {
        let schema = create_valid_schema();
        let validator = SemanticValidator;
        let result = validator.validate(&schema);
        assert!(result.valid);
    }

    #[test]
    fn test_semantic_validator_unreachable_route() {
        let mut schema = create_valid_schema();
        schema.spec.routes.push(RouteSpec {
            name: "unreachable".to_string(),
            description: None,
            priority: 0,
            condition: ConditionSpec::Equals {
                field: "test".to_string(),
                value: serde_json::json!("value"),
            },
            actions: vec![],
        });
        let validator = SemanticValidator;
        let result = validator.validate(&schema);
        assert!(result.valid); // Warning, not error
        assert!(!result.warnings.is_empty());
    }

    #[test]
    fn test_semantic_validator_invalid_regex() {
        let mut schema = create_valid_schema();
        schema.spec.routes[0].condition = ConditionSpec::Regex {
            field: "test".to_string(),
            pattern: "[invalid".to_string(),
        };
        let validator = SemanticValidator;
        let result = validator.validate(&schema);
        assert!(!result.valid);
        assert!(result
            .errors
            .iter()
            .any(|e| matches!(e, ValidationError::InvalidRegexPattern(_))));
    }

    #[test]
    fn test_semantic_validator_empty_field() {
        let mut schema = create_valid_schema();
        schema.spec.routes[0].condition = ConditionSpec::Equals {
            field: String::new(),
            value: serde_json::json!("value"),
        };
        let validator = SemanticValidator;
        let result = validator.validate(&schema);
        assert!(!result.valid);
        assert!(result
            .errors
            .iter()
            .any(|e| matches!(e, ValidationError::EmptyFieldName { .. })));
    }

    // DependencyValidator tests
    #[test]
    fn test_dependency_validator_valid() {
        let schema = create_valid_schema();
        let validator = DependencyValidator;
        let result = validator.validate(&schema);
        assert!(result.valid);
    }

    #[test]
    fn test_dependency_validator_empty_kafka_topic() {
        let mut schema = create_valid_schema();
        schema.spec.routes[0].actions = vec![ActionSpec::SendToKafka {
            topic: String::new(),
            partition_key: None,
        }];
        let validator = DependencyValidator;
        let result = validator.validate(&schema);
        assert!(!result.valid);
        assert!(result
            .errors
            .iter()
            .any(|e| matches!(e, ValidationError::EmptyKafkaTopic { .. })));
    }

    #[test]
    fn test_dependency_validator_invalid_http_method() {
        let mut schema = create_valid_schema();
        schema.spec.routes[0].actions = vec![ActionSpec::SendToHttp {
            url: "http://example.com".to_string(),
            method: "INVALID".to_string(),
            headers: HashMap::new(),
        }];
        let validator = DependencyValidator;
        let result = validator.validate(&schema);
        assert!(!result.valid);
        assert!(result
            .errors
            .iter()
            .any(|e| matches!(e, ValidationError::InvalidHttpMethod { .. })));
    }

    // ResourceValidator tests
    #[test]
    fn test_resource_validator_valid() {
        let schema = create_valid_schema();
        let validator = ResourceValidator;
        let result = validator.validate(&schema);
        assert!(result.valid);
    }

    #[test]
    fn test_resource_validator_very_long_timeout() {
        let mut schema = create_valid_schema();
        schema.spec.execution.timeout = 100000;
        let validator = ResourceValidator;
        let result = validator.validate(&schema);
        assert!(result.valid); // Warning, not error
        assert!(!result.warnings.is_empty());
    }

    #[test]
    fn test_resource_validator_deeply_nested() {
        let mut schema = create_valid_schema();
        // Create deeply nested condition
        let mut condition = ConditionSpec::Always;
        for _ in 0..15 {
            condition = ConditionSpec::Not {
                condition: Box::new(condition),
            };
        }
        schema.spec.routes[0].condition = condition;
        let validator = ResourceValidator;
        let result = validator.validate(&schema);
        assert!(result.valid); // Warning, not error
        assert!(!result.warnings.is_empty());
    }
}
