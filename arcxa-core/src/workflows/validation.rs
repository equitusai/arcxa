// Validation traits and result types for declarative workflows

use super::errors::{ValidationError, ValidationWarning};
use super::schema::WorkflowSchema;

/// Result of workflow validation
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// Whether the workflow is valid
    pub valid: bool,

    /// List of validation errors (must be empty if valid)
    pub errors: Vec<ValidationError>,

    /// List of warnings (doesn't prevent deployment but should be addressed)
    pub warnings: Vec<ValidationWarning>,
}

impl ValidationResult {
    /// Create a successful validation result
    pub fn success() -> Self {
        Self {
            valid: true,
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    /// Create a failed validation result with errors
    pub fn with_errors(errors: Vec<ValidationError>) -> Self {
        Self {
            valid: false,
            errors,
            warnings: Vec::new(),
        }
    }

    /// Create a successful validation result with warnings
    pub fn with_warnings(warnings: Vec<ValidationWarning>) -> Self {
        Self {
            valid: true,
            errors: Vec::new(),
            warnings,
        }
    }

    /// Add an error to the result
    pub fn add_error(&mut self, error: ValidationError) {
        self.errors.push(error);
        self.valid = false;
    }

    /// Add a warning to the result
    pub fn add_warning(&mut self, warning: ValidationWarning) {
        self.warnings.push(warning);
    }

    /// Merge another validation result into this one
    pub fn merge(&mut self, other: ValidationResult) {
        self.errors.extend(other.errors);
        self.warnings.extend(other.warnings);
        if !other.valid {
            self.valid = false;
        }
    }

    /// Check if there are any errors
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Check if there are any warnings
    pub fn has_warnings(&self) -> bool {
        !self.warnings.is_empty()
    }

    /// Get a summary string
    pub fn summary(&self) -> String {
        if self.valid && !self.has_warnings() {
            "Validation passed".to_string()
        } else if self.valid {
            format!("Validation passed with {} warnings", self.warnings.len())
        } else {
            format!(
                "Validation failed: {} errors, {} warnings",
                self.errors.len(),
                self.warnings.len()
            )
        }
    }
}

/// Trait for workflow validators
///
/// Validators check different aspects of workflow correctness:
/// - Schema validation: Structure and types
/// - Semantic validation: Business logic rules
/// - Dependency validation: References to external resources
/// - Performance validation: Resource estimates
pub trait Validator: Send + Sync {
    /// Validate a workflow schema
    fn validate(&self, schema: &WorkflowSchema) -> ValidationResult;

    /// Get the validator name (for debugging)
    fn name(&self) -> &str;
}

/// Composite validator that runs multiple validators in sequence
pub struct CompositeValidator {
    validators: Vec<Box<dyn Validator>>,
}

impl CompositeValidator {
    /// Create a new composite validator
    pub fn new() -> Self {
        Self {
            validators: Vec::new(),
        }
    }

    /// Add a validator to the chain
    pub fn add_validator(&mut self, validator: Box<dyn Validator>) {
        self.validators.push(validator);
    }

    /// Create with a list of validators
    pub fn with_validators(validators: Vec<Box<dyn Validator>>) -> Self {
        Self { validators }
    }
}

impl Default for CompositeValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl Validator for CompositeValidator {
    fn validate(&self, schema: &WorkflowSchema) -> ValidationResult {
        let mut result = ValidationResult::success();

        for validator in &self.validators {
            let validator_result = validator.validate(schema);
            result.merge(validator_result);
        }

        result
    }

    fn name(&self) -> &str {
        "CompositeValidator"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflows::errors::{ValidationError, ValidationWarning};

    struct MockValidator {
        should_fail: bool,
        should_warn: bool,
    }

    impl Validator for MockValidator {
        fn validate(&self, _schema: &WorkflowSchema) -> ValidationResult {
            let mut result = ValidationResult::success();

            if self.should_fail {
                result.add_error(ValidationError::EmptyWorkflowName);
            }

            if self.should_warn {
                result.add_warning(ValidationWarning::UnreachableRoute {
                    route: "test".to_string(),
                });
            }

            result
        }

        fn name(&self) -> &str {
            "MockValidator"
        }
    }

    #[test]
    fn test_validation_result_success() {
        let result = ValidationResult::success();
        assert!(result.valid);
        assert!(!result.has_errors());
        assert!(!result.has_warnings());
    }

    #[test]
    fn test_validation_result_with_errors() {
        let errors = vec![ValidationError::EmptyWorkflowName];
        let result = ValidationResult::with_errors(errors);
        assert!(!result.valid);
        assert!(result.has_errors());
        assert_eq!(result.errors.len(), 1);
    }

    #[test]
    fn test_validation_result_merge() {
        let mut result1 = ValidationResult::success();
        let mut result2 = ValidationResult::success();

        result2.add_error(ValidationError::EmptyWorkflowName);
        result1.merge(result2);

        assert!(!result1.valid);
        assert_eq!(result1.errors.len(), 1);
    }

    #[test]
    fn test_composite_validator() {
        use crate::workflows::schema::*;
        use std::collections::HashMap;

        let schema = WorkflowSchema {
            api_version: "graphica.io/v1".to_string(),
            kind: "Workflow".to_string(),
            metadata: WorkflowMetadata {
                name: "test".to_string(),
                version: None,
                description: None,
                owner: None,
                tags: Vec::new(),
                annotations: HashMap::new(),
            },
            spec: WorkflowSpec {
                schedule: None,
                execution: ExecutionSpec::default(),
                routes: Vec::new(),
                default_route: None,
                monitoring: None,
                resources: None,
            },
        };

        let mut composite = CompositeValidator::new();
        composite.add_validator(Box::new(MockValidator {
            should_fail: false,
            should_warn: true,
        }));
        composite.add_validator(Box::new(MockValidator {
            should_fail: true,
            should_warn: false,
        }));

        let result = composite.validate(&schema);
        assert!(!result.valid);
        assert_eq!(result.errors.len(), 1);
        assert_eq!(result.warnings.len(), 1);
    }
}
