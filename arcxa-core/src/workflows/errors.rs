// Error types for declarative workflow validation

use std::fmt;
use thiserror::Error;

/// Validation error - prevents workflow deployment
#[derive(Debug, Clone, Error, PartialEq)]
pub enum ValidationError {
    /// Workflow name is empty or missing
    #[error("Workflow name cannot be empty")]
    EmptyWorkflowName,

    /// No routes defined
    #[error("Workflow must have at least one route")]
    NoRoutes,

    /// Invalid API version
    #[error("Invalid API version: {0}, expected {1}")]
    InvalidApiVersion(String, String),

    /// Invalid kind
    #[error("Invalid kind: {0}, expected 'Workflow'")]
    InvalidKind(String),

    /// Invalid cron expression
    #[error("Invalid cron expression: {0}")]
    InvalidCronExpression(String),

    /// Invalid timezone
    #[error("Invalid timezone: {0}")]
    InvalidTimezone(String),

    /// Route name is empty
    #[error("Route name cannot be empty")]
    EmptyRouteName,

    /// Duplicate route name
    #[error("Duplicate route name: {0}")]
    DuplicateRouteName(String),

    /// Action type not recognized
    #[error("Invalid action type: {0}")]
    InvalidActionType(String),

    /// Missing required field in action
    #[error("Missing required field '{field}' in action '{action}'")]
    MissingActionField { action: String, field: String },

    /// Invalid field reference
    #[error("Field '{0}' not found in input schema")]
    InvalidFieldReference(String),

    /// Reference data not found
    #[error("Reference data '{0}' not found")]
    ReferenceDataNotFound(String),

    /// Model not found
    #[error("Model '{0}' not found")]
    ModelNotFound(String),

    /// Invalid regex pattern
    #[error("Invalid regex pattern: {0}")]
    InvalidRegexPattern(String),

    /// Circular dependency detected
    #[error("Circular dependency detected in routes")]
    CircularDependency,

    /// Default route not found
    #[error("Default route '{0}' not defined in routes")]
    DefaultRouteNotFound(String),

    /// Invalid resource specification
    #[error("Invalid resource specification: {0}")]
    InvalidResourceSpec(String),

    /// Schema validation failed
    #[error("Schema validation failed: {0}")]
    SchemaValidationFailed(String),

    /// Invalid condition specification
    #[error("Invalid condition: {0}")]
    InvalidCondition(String),

    /// Invalid quality threshold
    #[error("Quality threshold must be between 0.0 and 1.0, got {0}")]
    InvalidQualityThreshold(f64),

    /// Invalid timeout value
    #[error("Timeout must be positive, got {0}")]
    InvalidTimeout(i64),

    /// Invalid retry configuration
    #[error("Invalid retry configuration: {0}")]
    InvalidRetryConfig(String),

    /// Alert configuration invalid
    #[error("Invalid alert configuration: {0}")]
    InvalidAlertConfig(String),

    /// Workflow name too long
    #[error("Workflow name too long: {length} characters (max 253)")]
    WorkflowNameTooLong { length: usize },

    /// Invalid retries
    #[error("Retries must be non-negative, got {retries}")]
    InvalidRetries { retries: i32 },

    /// Empty field name in condition
    #[error("Field name cannot be empty in route '{route}'")]
    EmptyFieldName { route: String },

    /// Empty logical operator
    #[error("Logical operator '{operator}' in route '{route}' must have at least one condition")]
    EmptyLogicalOperator { route: String, operator: String },

    /// Empty reference data
    #[error("Reference data cannot be empty in route '{route}'")]
    EmptyReferenceData { route: String },

    /// Empty join key
    #[error("Join key cannot be empty in route '{route}'")]
    EmptyJoinKey { route: String },

    /// Empty rule ID
    #[error("Rule ID cannot be empty in route '{route}'")]
    EmptyRuleId { route: String },

    /// Empty Kafka topic
    #[error("Kafka topic cannot be empty in route '{route}'")]
    EmptyKafkaTopic { route: String },

    /// Empty HTTP URL
    #[error("HTTP URL cannot be empty in route '{route}'")]
    EmptyHttpUrl { route: String },

    /// Invalid HTTP method
    #[error("Invalid HTTP method '{method}' in route '{route}'")]
    InvalidHttpMethod { route: String, method: String },

    /// Empty code
    #[error("Code cannot be empty in route '{route}'")]
    EmptyCode { route: String },

    /// Empty model ID
    #[error("Model ID cannot be empty in route '{route}'")]
    EmptyModelId { route: String },

    /// Empty transformer
    #[error("Transformer name cannot be empty in route '{route}'")]
    EmptyTransformer { route: String },

    /// Invalid memory limit
    #[error("Memory limit must be positive, got {limit} MB")]
    InvalidMemoryLimit { limit: i32 },

    /// Invalid CPU limit
    #[error("CPU limit must be positive, got {limit} cores")]
    InvalidCpuLimit { limit: f64 },

    /// Empty cron expression
    #[error("Cron expression cannot be empty")]
    EmptyCronExpression,

    /// Custom validation error
    #[error("{0}")]
    Custom(String),
}

/// Validation warning - doesn't prevent deployment but should be addressed
#[derive(Debug, Clone, PartialEq)]
pub enum ValidationWarning {
    /// No default route specified
    NoDefaultRoute,

    /// Route priority may cause unexpected behavior
    RoutePriorityConflict { route1: String, route2: String },

    /// Resource limits not specified
    NoResourceLimits,

    /// No monitoring configured
    NoMonitoring,

    /// SLA not specified
    NoSlaSpecified,

    /// Schedule timezone is not UTC (may cause confusion)
    NonUtcTimezone { timezone: String },

    /// Large number of routes may impact performance
    TooManyRoutes { count: usize },

    /// Action may have performance implications
    PerformanceConcern { action: String, concern: String },

    /// Unused reference data
    UnusedReferenceData { name: String },

    /// Deprecated feature used
    DeprecatedFeature {
        feature: String,
        alternative: String,
    },

    /// Missing documentation
    MissingDescription {
        item_type: String,
        item_name: String,
    },

    /// Invalid version format
    InvalidVersionFormat { version: String },

    /// Negative priority
    NegativePriority { route: String, priority: i32 },

    /// Extremely high priority
    ExtremelyHighPriority { route: String, priority: i32 },

    /// Route without actions
    RouteWithoutActions { route: String },

    /// Multiple Always conditions
    MultipleAlwaysConditions { route: String },

    /// Unreachable route
    UnreachableRoute { route: String },

    /// Single condition in logical operator
    SingleConditionInLogicalOperator { route: String },

    /// Double negation
    DoubleNegation { route: String },

    /// Contradictory conditions
    ContradictoryConditions { route: String, field: String },

    /// Invalid HTTP URL
    InvalidHttpUrl { route: String, url: String },

    /// Unsupported language
    UnsupportedLanguage { route: String, language: String },

    /// Empty log message
    EmptyLogMessage { route: String },

    /// Invalid log level
    InvalidLogLevel { route: String, level: String },

    /// Expensive operation not last
    ExpensiveOperationNotLast { route: String },

    /// Very long timeout
    VeryLongTimeout { timeout: i64 },

    /// Excessive retries
    ExcessiveRetries { retries: i32 },

    /// Excessive memory request
    ExcessiveMemoryRequest { memory_mb: i32 },

    /// Excessive CPU request
    ExcessiveCpuRequest { cpu_cores: f64 },

    /// Deeply nested conditions
    DeeplyNestedConditions { route: String, depth: usize },

    /// Too many actions
    TooManyActions { route: String, count: usize },

    /// Custom warning
    Custom(String),
}

impl fmt::Display for ValidationWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ValidationWarning::NoDefaultRoute => {
                write!(f, "No default route specified")
            }
            ValidationWarning::RoutePriorityConflict { route1, route2 } => {
                write!(
                    f,
                    "Routes '{}' and '{}' have conflicting priorities",
                    route1, route2
                )
            }
            ValidationWarning::NoResourceLimits => {
                write!(f, "No resource limits specified")
            }
            ValidationWarning::NoMonitoring => {
                write!(f, "No monitoring configuration")
            }
            ValidationWarning::NoSlaSpecified => {
                write!(f, "No SLA specified")
            }
            ValidationWarning::NonUtcTimezone { timezone } => {
                write!(
                    f,
                    "Schedule uses non-UTC timezone: {}. Consider using UTC to avoid confusion.",
                    timezone
                )
            }
            ValidationWarning::TooManyRoutes { count } => {
                write!(
                    f,
                    "Workflow has {} routes, which may impact performance",
                    count
                )
            }
            ValidationWarning::PerformanceConcern { action, concern } => {
                write!(f, "Action '{}': {}", action, concern)
            }
            ValidationWarning::UnusedReferenceData { name } => {
                write!(f, "Reference data '{}' is defined but not used", name)
            }
            ValidationWarning::DeprecatedFeature {
                feature,
                alternative,
            } => {
                write!(
                    f,
                    "Feature '{}' is deprecated, use '{}' instead",
                    feature, alternative
                )
            }
            ValidationWarning::MissingDescription {
                item_type,
                item_name,
            } => {
                write!(f, "{} '{}' has no description", item_type, item_name)
            }
            ValidationWarning::InvalidVersionFormat { version } => {
                write!(
                    f,
                    "Version '{}' does not follow semver format (e.g., 1.0.0)",
                    version
                )
            }
            ValidationWarning::NegativePriority { route, priority } => {
                write!(f, "Route '{}' has negative priority {}", route, priority)
            }
            ValidationWarning::ExtremelyHighPriority { route, priority } => {
                write!(
                    f,
                    "Route '{}' has very high priority {} (>10000)",
                    route, priority
                )
            }
            ValidationWarning::RouteWithoutActions { route } => {
                write!(f, "Route '{}' has no actions", route)
            }
            ValidationWarning::MultipleAlwaysConditions { route } => {
                write!(
                    f,
                    "Multiple routes with Always condition, route '{}' may be unreachable",
                    route
                )
            }
            ValidationWarning::UnreachableRoute { route } => {
                write!(f, "Route '{}' is unreachable", route)
            }
            ValidationWarning::SingleConditionInLogicalOperator { route } => {
                write!(
                    f,
                    "Logical operator in route '{}' has only one condition",
                    route
                )
            }
            ValidationWarning::DoubleNegation { route } => {
                write!(
                    f,
                    "Route '{}' has double negation (NOT NOT), simplify condition",
                    route
                )
            }
            ValidationWarning::ContradictoryConditions { route, field } => {
                write!(
                    f,
                    "Route '{}' has contradictory conditions for field '{}'",
                    route, field
                )
            }
            ValidationWarning::InvalidHttpUrl { route, url } => {
                write!(f, "Route '{}' has invalid HTTP URL: {}", route, url)
            }
            ValidationWarning::UnsupportedLanguage { route, language } => {
                write!(
                    f,
                    "Route '{}' uses unsupported language: {}",
                    route, language
                )
            }
            ValidationWarning::EmptyLogMessage { route } => {
                write!(f, "Route '{}' has log action with empty message", route)
            }
            ValidationWarning::InvalidLogLevel { route, level } => {
                write!(f, "Route '{}' has invalid log level: {}", route, level)
            }
            ValidationWarning::ExpensiveOperationNotLast { route } => {
                write!(
                    f,
                    "Route '{}' has expensive operation not at end of action list",
                    route
                )
            }
            ValidationWarning::VeryLongTimeout { timeout } => {
                write!(f, "Timeout is very long: {} seconds (>24 hours)", timeout)
            }
            ValidationWarning::ExcessiveRetries { retries } => {
                write!(f, "Excessive retry count: {} (>10)", retries)
            }
            ValidationWarning::ExcessiveMemoryRequest { memory_mb } => {
                write!(f, "Excessive memory request: {} MB (>32GB)", memory_mb)
            }
            ValidationWarning::ExcessiveCpuRequest { cpu_cores } => {
                write!(f, "Excessive CPU request: {} cores (>32)", cpu_cores)
            }
            ValidationWarning::DeeplyNestedConditions { route, depth } => {
                write!(
                    f,
                    "Route '{}' has deeply nested conditions (depth {})",
                    route, depth
                )
            }
            ValidationWarning::TooManyActions { route, count } => {
                write!(f, "Route '{}' has many actions ({} actions)", route, count)
            }
            ValidationWarning::Custom(msg) => write!(f, "{}", msg),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validation_error_display() {
        let error = ValidationError::EmptyWorkflowName;
        assert_eq!(error.to_string(), "Workflow name cannot be empty");

        let error = ValidationError::DuplicateRouteName("test".to_string());
        assert_eq!(error.to_string(), "Duplicate route name: test");
    }

    #[test]
    fn test_validation_warning_display() {
        let warning = ValidationWarning::UnreachableRoute {
            route: "test".to_string(),
        };
        assert_eq!(warning.to_string(), "Route 'test' is unreachable");

        let warning = ValidationWarning::NoDefaultRoute;
        assert_eq!(warning.to_string(), "No default route specified");
    }

    #[test]
    fn test_error_equality() {
        let error1 = ValidationError::EmptyWorkflowName;
        let error2 = ValidationError::EmptyWorkflowName;
        assert_eq!(error1, error2);

        let error3 = ValidationError::NoRoutes;
        assert_ne!(error1, error3);
    }
}
