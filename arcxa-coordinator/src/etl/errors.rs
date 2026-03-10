//! ETL-specific error types
//!
//! This module defines comprehensive error types for the ETL system,
//! providing detailed context for debugging and error recovery.

use std::fmt;
use thiserror::Error;

/// Main ETL error type
#[derive(Debug, Error)]
pub enum EtlError {
    // ========================================================================
    // Format Errors
    // ========================================================================
    /// Error reading or parsing a specific format
    #[error("Format error in {format}: {message}")]
    FormatError {
        format: String,
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// Parse error at specific location
    #[error("Parse error at line {line}: {message}")]
    ParseError { line: usize, message: String },

    /// Schema-related error
    #[error("Schema error: {message}")]
    SchemaError { message: String },

    /// Schema mismatch between source and destination
    #[error("Schema mismatch: expected {expected:?}, got {actual:?}")]
    SchemaMismatch {
        expected: Vec<String>,
        actual: Vec<String>,
    },

    // ========================================================================
    // I/O Errors
    // ========================================================================
    /// File I/O error
    #[error("I/O error for {path}: {error}")]
    IoError { path: String, error: String },

    /// Network I/O error
    #[error("Network error: {message}")]
    NetworkError {
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    // ========================================================================
    // Database Errors
    // ========================================================================
    /// Database connection error
    #[error("Database connection error for {database}: {message}")]
    ConnectionError { database: String, message: String },

    /// SQL execution error
    #[error("SQL error: {message}")]
    SqlError {
        message: String,
        query: Option<String>,
    },

    /// Transaction error
    #[error("Transaction error: {message}")]
    TransactionError { message: String },

    /// Bulk load error
    #[error("Bulk load error: loaded {loaded}/{total} records before error: {message}")]
    BulkLoadError {
        loaded: usize,
        total: usize,
        message: String,
    },

    // ========================================================================
    // Transformation Errors
    // ========================================================================
    /// Transformation failed
    #[error("Transform error in {transformer}: {message}")]
    TransformError {
        transformer: String,
        message: String,
        record_index: Option<usize>,
    },

    /// Type conversion error
    #[error("Type conversion error: cannot convert {value} from {from_type} to {to_type}")]
    TypeConversionError {
        value: String,
        from_type: String,
        to_type: String,
    },

    /// Validation failed
    #[error("Validation error: {message}")]
    ValidationError {
        message: String,
        field: Option<String>,
        record_index: Option<usize>,
    },

    /// Field not found
    #[error("Field not found: {field}")]
    FieldNotFound {
        field: String,
        available_fields: Vec<String>,
    },

    // ========================================================================
    // Pipeline Errors
    // ========================================================================
    /// Pipeline configuration error
    #[error("Configuration error: {message}")]
    ConfigurationError { message: String },

    /// Pipeline execution error
    #[error("Pipeline execution error at stage {stage}: {message}")]
    PipelineError { stage: String, message: String },

    /// Checkpoint error
    #[error("Checkpoint error: {message}")]
    CheckpointError { message: String },

    /// Resource exhaustion
    #[error("Resource exhausted: {resource} - {message}")]
    ResourceExhausted { resource: String, message: String },

    // ========================================================================
    // Load Errors
    // ========================================================================
    /// Load mode not supported
    #[error("Load mode {mode} not supported by {destination}")]
    UnsupportedLoadMode { mode: String, destination: String },

    /// Key fields missing for upsert
    #[error("Key fields required for upsert: {fields:?}")]
    MissingKeyFields { fields: Vec<String> },

    /// Duplicate key error
    #[error("Duplicate key error: {key}")]
    DuplicateKey {
        key: String,
        existing_record: Option<String>,
    },

    // ========================================================================
    // Streaming Errors
    // ========================================================================
    /// Stream ended unexpectedly
    #[error("Stream ended unexpectedly after {records} records")]
    UnexpectedEndOfStream { records: usize },

    /// Stream processing error
    #[error("Stream processing error: {message}")]
    StreamError { message: String },

    // ========================================================================
    // General Errors
    // ========================================================================
    /// Operation cancelled
    #[error("Operation cancelled: {reason}")]
    Cancelled { reason: String },

    /// Operation timeout
    #[error("Operation timed out after {seconds} seconds")]
    Timeout { seconds: u64 },

    /// Not implemented
    #[error("Feature not implemented: {feature}")]
    NotImplemented { feature: String },

    /// Internal error (catch-all)
    #[error("Internal error: {message}")]
    Internal {
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// Wrapped anyhow error for compatibility
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Result type alias for ETL operations
pub type EtlResult<T> = Result<T, EtlError>;

/// Error context for detailed reporting
#[derive(Debug, Clone)]
pub struct ErrorContext {
    pub operation: String,
    pub record_index: Option<usize>,
    pub field_name: Option<String>,
    pub file_path: Option<String>,
    pub line_number: Option<usize>,
    pub additional_info: std::collections::HashMap<String, String>,
}

impl ErrorContext {
    pub fn new(operation: impl Into<String>) -> Self {
        Self {
            operation: operation.into(),
            record_index: None,
            field_name: None,
            file_path: None,
            line_number: None,
            additional_info: std::collections::HashMap::new(),
        }
    }

    pub fn with_record(mut self, index: usize) -> Self {
        self.record_index = Some(index);
        self
    }

    pub fn with_field(mut self, field: impl Into<String>) -> Self {
        self.field_name = Some(field.into());
        self
    }

    pub fn with_location(mut self, file: impl Into<String>, line: usize) -> Self {
        self.file_path = Some(file.into());
        self.line_number = Some(line);
        self
    }

    pub fn add_info(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.additional_info.insert(key.into(), value.into());
        self
    }
}

/// Error accumulator for batch operations
#[derive(Debug, Default)]
pub struct ErrorAccumulator {
    errors: Vec<(EtlError, Option<ErrorContext>)>,
    max_errors: usize,
    fail_fast: bool,
}

impl ErrorAccumulator {
    pub fn new(max_errors: usize, fail_fast: bool) -> Self {
        Self {
            errors: Vec::new(),
            max_errors,
            fail_fast,
        }
    }

    pub fn add(&mut self, error: EtlError, context: Option<ErrorContext>) -> EtlResult<()> {
        let should_fail = self.fail_fast || self.errors.len() + 1 >= self.max_errors;

        if should_fail {
            // Store error and return it
            self.errors.push((
                EtlError::Internal {
                    message: error.to_string(),
                    source: None,
                },
                context,
            ));
            Err(error)
        } else {
            // Just store the error
            self.errors.push((error, context));
            Ok(())
        }
    }

    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    pub fn error_count(&self) -> usize {
        self.errors.len()
    }

    pub fn take_errors(self) -> Vec<(EtlError, Option<ErrorContext>)> {
        self.errors
    }

    pub fn to_summary(&self) -> String {
        if self.errors.is_empty() {
            return "No errors".to_string();
        }

        let mut summary = format!("Total errors: {}\n", self.errors.len());

        // Group errors by type
        let mut error_counts = std::collections::HashMap::new();
        for (error, _) in &self.errors {
            let error_type = match error {
                EtlError::ParseError { .. } => "Parse",
                EtlError::ValidationError { .. } => "Validation",
                EtlError::TransformError { .. } => "Transform",
                EtlError::SqlError { .. } => "SQL",
                EtlError::ConnectionError { .. } => "Connection",
                _ => "Other",
            };
            *error_counts.entry(error_type).or_insert(0) += 1;
        }

        for (error_type, count) in error_counts {
            summary.push_str(&format!("  {} errors: {}\n", error_type, count));
        }

        // Show first few errors
        summary.push_str("\nFirst errors:\n");
        for (i, (error, context)) in self.errors.iter().take(5).enumerate() {
            summary.push_str(&format!("  {}. {}", i + 1, error));
            if let Some(ctx) = context {
                if let Some(record) = ctx.record_index {
                    summary.push_str(&format!(" (record {})", record));
                }
                if let Some(field) = &ctx.field_name {
                    summary.push_str(&format!(" [field: {}]", field));
                }
            }
            summary.push('\n');
        }

        if self.errors.len() > 5 {
            summary.push_str(&format!(
                "  ... and {} more errors\n",
                self.errors.len() - 5
            ));
        }

        summary
    }
}

impl fmt::Display for ErrorAccumulator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_summary())
    }
}

/// Extension trait for adding context to errors
///
/// Note: This trait is intentionally simple. For more complex error handling,
/// use the ErrorAccumulator or wrap errors manually.
pub trait ErrorContextExt<T> {
    fn with_context(self, context: ErrorContext) -> EtlResult<T>;
}

impl<T> ErrorContextExt<T> for EtlResult<T> {
    fn with_context(self, context: ErrorContext) -> EtlResult<T> {
        self.map_err(|e| EtlError::Internal {
            message: format!(
                "{} during {}: {}",
                e,
                context.operation,
                context
                    .additional_info
                    .iter()
                    .map(|(k, v)| format!("{}={}", k, v))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            source: Some(Box::new(e)),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_accumulator() {
        let mut acc = ErrorAccumulator::new(3, false);

        // Add errors
        acc.add(
            EtlError::ParseError {
                line: 1,
                message: "Invalid CSV".to_string(),
            },
            Some(ErrorContext::new("parsing").with_record(1)),
        )
        .unwrap();

        acc.add(
            EtlError::ValidationError {
                message: "Email invalid".to_string(),
                field: Some("email".to_string()),
                record_index: Some(2),
            },
            None,
        )
        .unwrap();

        assert_eq!(acc.error_count(), 2);
        assert!(acc.has_errors());

        // Check summary
        let summary = acc.to_summary();
        assert!(summary.contains("Total errors: 2"));
        assert!(summary.contains("Parse errors: 1"));
        assert!(summary.contains("Validation errors: 1"));
    }

    #[test]
    fn test_error_context() {
        let context = ErrorContext::new("loading")
            .with_record(42)
            .with_field("email")
            .with_location("data.csv", 100)
            .add_info("batch", "5");

        assert_eq!(context.operation, "loading");
        assert_eq!(context.record_index, Some(42));
        assert_eq!(context.field_name.as_deref(), Some("email"));
        assert_eq!(context.file_path.as_deref(), Some("data.csv"));
        assert_eq!(context.line_number, Some(100));
        assert_eq!(context.additional_info.get("batch"), Some(&"5".to_string()));
    }
}
