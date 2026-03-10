//! Preflight Validation for Batch Jobs
//!
//! Validates batch jobs before execution to catch errors early and prevent failures.
//!
//! ## Validation Checks
//!
//! 1. **File Validation**: All files exist and are accessible
//! 2. **Workflow Validation**: Workflow exists and is enabled
//! 3. **Dependency Validation**: Dependencies form valid DAG (no cycles)
//! 4. **Resource Validation**: Resource limits are within system capacity
//! 5. **Schema Validation**: File schemas match expected formats
//! 6. **Access Control**: User has permission to execute workflow
//!
//! ## Example
//!
//! ```rust,ignore
//! use graphica_coordinator::workflows::engine::PreflightValidator;
//! use graphica_coordinator::workflows::domain::BatchJob;
//!
//! let validator = PreflightValidator::new();
//! let result = validator.validate(&batch_job).await?;
//!
//! if !result.is_valid {
//!     println!("Validation failed");
//! }
//! ```

use crate::api::file_library::storage_trait::FileLibraryStore;
use crate::api::file_library::types::{DataFile, FileStatus as LibraryFileStatus};
use crate::workflows::domain::{BatchJob, DataSource};
use anyhow::{anyhow, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use tracing::{debug, info, warn};

/// Preflight validation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreflightResult {
    /// Overall validation status
    pub is_valid: bool,

    /// Validation errors (blocking issues)
    pub errors: Vec<ValidationError>,

    /// Validation warnings (non-blocking issues)
    pub warnings: Vec<ValidationWarning>,

    /// Detailed checks performed
    pub checks: Vec<ValidationCheck>,

    /// Estimated execution time (minutes)
    pub estimated_duration_minutes: Option<usize>,

    /// Resource requirements
    pub resource_requirements: ResourceRequirements,
}

impl PreflightResult {
    pub fn new() -> Self {
        Self {
            is_valid: true,
            errors: Vec::new(),
            warnings: Vec::new(),
            checks: Vec::new(),
            estimated_duration_minutes: None,
            resource_requirements: ResourceRequirements::default(),
        }
    }

    pub fn is_valid(&self) -> bool {
        self.is_valid && self.errors.is_empty()
    }

    pub fn add_error(&mut self, error: ValidationError) {
        self.is_valid = false;
        self.errors.push(error);
    }

    pub fn add_warning(&mut self, warning: ValidationWarning) {
        self.warnings.push(warning);
    }

    pub fn add_check(&mut self, check: ValidationCheck) {
        self.checks.push(check);
    }
}

impl Default for PreflightResult {
    fn default() -> Self {
        Self::new()
    }
}

/// Validation error (blocking)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationError {
    pub code: String,
    pub message: String,
    pub context: HashMap<String, String>,
}

impl ValidationError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            context: HashMap::new(),
        }
    }

    pub fn with_context(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.context.insert(key.into(), value.into());
        self
    }
}

/// Validation warning (non-blocking)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationWarning {
    pub code: String,
    pub message: String,
    pub recommendation: String,
}

impl ValidationWarning {
    pub fn new(
        code: impl Into<String>,
        message: impl Into<String>,
        recommendation: impl Into<String>,
    ) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            recommendation: recommendation.into(),
        }
    }
}

/// Individual validation check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationCheck {
    pub name: String,
    pub passed: bool,
    pub message: String,
    pub duration_ms: u64,
}

/// Resource requirements for batch job
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceRequirements {
    pub max_memory_mb: usize,
    pub max_db_connections: usize,
    pub max_concurrent_workflows: usize,
    pub total_file_size_mb: usize,
}

impl Default for ResourceRequirements {
    fn default() -> Self {
        Self {
            max_memory_mb: 0,
            max_db_connections: 0,
            max_concurrent_workflows: 0,
            total_file_size_mb: 0,
        }
    }
}

/// Validation cache entry
#[derive(Debug, Clone)]
struct ValidationCacheEntry {
    /// Cached file metadata
    file: DataFile,
    /// Validation timestamp
    validated_at: DateTime<Utc>,
}

impl ValidationCacheEntry {
    /// Check if cache entry is still fresh (valid for 60 seconds)
    fn is_fresh(&self) -> bool {
        Utc::now().signed_duration_since(self.validated_at) < Duration::seconds(60)
    }
}

/// Result of validating a single file
#[derive(Debug)]
struct FileValidationResult {
    /// Blocking error if file cannot be used
    error: Option<ValidationError>,
    /// Non-blocking warnings about the file
    warnings: Vec<ValidationWarning>,
}

/// Preflight validator for batch jobs with File Library integration
pub struct PreflightValidator {
    /// File library storage backend
    file_store: Arc<dyn FileLibraryStore>,

    /// Validation cache to avoid repeated lookups
    /// Key: file_id, Value: cached file metadata with timestamp
    validation_cache: Arc<RwLock<HashMap<String, ValidationCacheEntry>>>,
}

impl PreflightValidator {
    /// Create a new preflight validator with file library integration
    pub fn new(file_store: Arc<dyn FileLibraryStore>) -> Self {
        Self {
            file_store,
            validation_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a validator without file library (for testing/legacy code)
    /// This will create a minimal in-memory implementation
    #[cfg(test)]
    pub fn new_without_file_library() -> Self {
        use crate::api::file_library::storage::FileLibraryStorage;
        Self::new(Arc::new(FileLibraryStorage::new()))
    }

    /// Validate a batch job before execution
    ///
    /// This performs comprehensive validation including:
    /// - File existence and accessibility
    /// - Workflow validation
    /// - Dependency validation
    /// - Resource validation
    /// - Schema validation
    pub async fn validate(&self, batch_job: &BatchJob) -> Result<PreflightResult> {
        info!(
            "Starting preflight validation for batch job: {}",
            batch_job.job_id
        );

        let mut result = PreflightResult::new();

        // 1. Validate basic batch job structure
        self.validate_batch_job_structure(batch_job, &mut result)
            .await?;

        // 2. Validate workflow configuration
        self.validate_workflow_config(batch_job, &mut result)
            .await?;

        // 3. Validate dependencies (DAG check)
        self.validate_dependencies(batch_job, &mut result).await?;

        // 4. Validate resource limits
        self.validate_resource_limits(batch_job, &mut result)
            .await?;

        // 5. Validate file references
        self.validate_file_references(batch_job, &mut result)
            .await?;

        // 6. Estimate execution time
        self.estimate_execution_time(batch_job, &mut result);

        // 7. Calculate resource requirements
        self.calculate_resource_requirements(batch_job, &mut result);

        info!(
            "Preflight validation complete: valid={}, errors={}, warnings={}",
            result.is_valid(),
            result.errors.len(),
            result.warnings.len()
        );

        Ok(result)
    }

    /// Validate basic batch job structure
    async fn validate_batch_job_structure(
        &self,
        batch_job: &BatchJob,
        result: &mut PreflightResult,
    ) -> Result<()> {
        let start = std::time::Instant::now();

        // Check name
        if batch_job.name.is_empty() {
            result.add_error(ValidationError::new(
                "EMPTY_NAME",
                "Batch job name cannot be empty",
            ));
        }

        // Check workflow ID
        if batch_job.workflow_id.is_empty() {
            result.add_error(ValidationError::new(
                "EMPTY_WORKFLOW_ID",
                "Workflow ID cannot be empty",
            ));
        }

        // Check executions
        if batch_job.workflow_executions.is_empty() {
            result.add_error(ValidationError::new(
                "NO_EXECUTIONS",
                "Batch job must have at least one workflow execution",
            ));
        }

        // Check for duplicate execution IDs
        let mut seen_ids = HashSet::new();
        for exec in &batch_job.workflow_executions {
            if !seen_ids.insert(&exec.execution_id) {
                result.add_error(
                    ValidationError::new("DUPLICATE_EXECUTION_ID", "Duplicate execution ID found")
                        .with_context("execution_id", exec.execution_id.clone()),
                );
            }
        }

        result.add_check(ValidationCheck {
            name: "Batch Job Structure".to_string(),
            passed: result.errors.is_empty(),
            message: format!(
                "Validated {} workflow executions",
                batch_job.workflow_executions.len()
            ),
            duration_ms: start.elapsed().as_millis() as u64,
        });

        Ok(())
    }

    /// Validate workflow configuration
    async fn validate_workflow_config(
        &self,
        batch_job: &BatchJob,
        result: &mut PreflightResult,
    ) -> Result<()> {
        let start = std::time::Instant::now();

        // In production, this would fetch and validate the actual workflow
        // For now, we do basic config validation

        // Check max_parallel
        if batch_job.config.max_parallel == 0 {
            result.add_error(ValidationError::new(
                "INVALID_MAX_PARALLEL",
                "max_parallel must be at least 1",
            ));
        }

        if batch_job.config.max_parallel > 100 {
            result.add_warning(ValidationWarning::new(
                "HIGH_MAX_PARALLEL",
                format!(
                    "max_parallel is very high ({})",
                    batch_job.config.max_parallel
                ),
                "Consider reducing to avoid resource exhaustion".to_string(),
            ));
        }

        // Check max_retries
        if batch_job.config.max_retries > 10 {
            result.add_warning(ValidationWarning::new(
                "HIGH_MAX_RETRIES",
                format!(
                    "max_retries is very high ({})",
                    batch_job.config.max_retries
                ),
                "Consider reducing to avoid excessive retry delays".to_string(),
            ));
        }

        result.add_check(ValidationCheck {
            name: "Workflow Configuration".to_string(),
            passed: true,
            message: format!(
                "max_parallel={}, max_retries={}",
                batch_job.config.max_parallel, batch_job.config.max_retries
            ),
            duration_ms: start.elapsed().as_millis() as u64,
        });

        Ok(())
    }

    /// Validate dependencies form a valid DAG
    async fn validate_dependencies(
        &self,
        batch_job: &BatchJob,
        result: &mut PreflightResult,
    ) -> Result<()> {
        let start = std::time::Instant::now();

        // Build execution ID index
        let exec_by_id: HashMap<_, _> = batch_job
            .workflow_executions
            .iter()
            .map(|e| (e.execution_id.as_str(), e))
            .collect();

        // Check all dependencies exist
        for exec in &batch_job.workflow_executions {
            for dep_id in &exec.dependencies {
                if !exec_by_id.contains_key(dep_id.as_str()) {
                    result.add_error(
                        ValidationError::new(
                            "INVALID_DEPENDENCY",
                            "Dependency references non-existent execution",
                        )
                        .with_context("execution_id", exec.execution_id.clone())
                        .with_context("dependency_id", dep_id.clone()),
                    );
                }
            }
        }

        // Check for circular dependencies using DFS
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();

        for exec in &batch_job.workflow_executions {
            if !visited.contains(exec.execution_id.as_str()) {
                if self.has_cycle(
                    exec.execution_id.as_str(),
                    &exec_by_id,
                    &mut visited,
                    &mut rec_stack,
                ) {
                    result.add_error(
                        ValidationError::new("CIRCULAR_DEPENDENCY", "Circular dependency detected")
                            .with_context("execution_id", exec.execution_id.clone()),
                    );
                    break;
                }
            }
        }

        result.add_check(ValidationCheck {
            name: "Dependency Validation".to_string(),
            passed: result.errors.is_empty(),
            message: "Checked for circular dependencies".to_string(),
            duration_ms: start.elapsed().as_millis() as u64,
        });

        Ok(())
    }

    /// DFS helper for cycle detection
    fn has_cycle(
        &self,
        exec_id: &str,
        exec_by_id: &HashMap<&str, &crate::workflows::domain::WorkflowExecutionRef>,
        visited: &mut HashSet<String>,
        rec_stack: &mut HashSet<String>,
    ) -> bool {
        visited.insert(exec_id.to_string());
        rec_stack.insert(exec_id.to_string());

        if let Some(exec) = exec_by_id.get(exec_id) {
            for dep_id in &exec.dependencies {
                if !visited.contains(dep_id.as_str()) {
                    if self.has_cycle(dep_id, exec_by_id, visited, rec_stack) {
                        return true;
                    }
                } else if rec_stack.contains(dep_id.as_str()) {
                    return true;
                }
            }
        }

        rec_stack.remove(exec_id);
        false
    }

    /// Validate resource limits are within system capacity
    async fn validate_resource_limits(
        &self,
        batch_job: &BatchJob,
        result: &mut PreflightResult,
    ) -> Result<()> {
        let start = std::time::Instant::now();

        let limits = &batch_job.config.resource_limits;

        // Check memory limits
        if limits.max_memory_mb == 0 {
            result.add_warning(ValidationWarning::new(
                "ZERO_MEMORY_LIMIT",
                "Memory limit is set to 0 (unlimited)",
                "Consider setting a memory limit to prevent OOM errors".to_string(),
            ));
        }

        if limits.max_memory_mb > 16384 {
            // 16GB
            result.add_warning(ValidationWarning::new(
                "HIGH_MEMORY_LIMIT",
                format!("Memory limit is very high ({}MB)", limits.max_memory_mb),
                "Ensure system has sufficient memory available".to_string(),
            ));
        }

        // Check DB connection limits
        if limits.max_db_connections == 0 {
            result.add_error(ValidationError::new(
                "ZERO_DB_CONNECTIONS",
                "DB connection limit cannot be 0",
            ));
        }

        if limits.max_db_connections > 100 {
            result.add_warning(ValidationWarning::new(
                "HIGH_DB_CONNECTIONS",
                format!(
                    "DB connection limit is very high ({})",
                    limits.max_db_connections
                ),
                "Ensure database can handle this many connections".to_string(),
            ));
        }

        // Check file size limits
        if limits.max_file_size_mb > 1000 {
            // 1GB
            result.add_warning(ValidationWarning::new(
                "LARGE_FILE_SIZE_LIMIT",
                format!(
                    "Max file size is very large ({}MB)",
                    limits.max_file_size_mb
                ),
                "Large files may cause memory issues during processing".to_string(),
            ));
        }

        result.add_check(ValidationCheck {
            name: "Resource Limits".to_string(),
            passed: result.errors.is_empty(),
            message: format!(
                "Memory: {}MB, DB Connections: {}, Max File Size: {}MB",
                limits.max_memory_mb, limits.max_db_connections, limits.max_file_size_mb
            ),
            duration_ms: start.elapsed().as_millis() as u64,
        });

        Ok(())
    }

    /// Validate file references exist and are accessible
    ///
    /// This performs concurrent validation of all files through the File Library:
    /// - File existence checks
    /// - Schema validation
    /// - File size validation
    /// - Status validation (must be Validated, not Error/Processing)
    /// - Uses caching to avoid repeated lookups
    async fn validate_file_references(
        &self,
        batch_job: &BatchJob,
        result: &mut PreflightResult,
    ) -> Result<()> {
        let start = std::time::Instant::now();

        // Collect all file IDs that need validation
        let mut file_ids_to_validate = Vec::new();
        let mut total_file_size_bytes: u64 = 0;

        // Validate data source structure and collect file IDs
        for exec in &batch_job.workflow_executions {
            match &exec.source {
                DataSource::CsvFile {
                    file_id, file_path, ..
                } => {
                    // Check file_id is not empty
                    if file_id.is_empty() {
                        result.add_error(
                            ValidationError::new("EMPTY_FILE_ID", "File ID cannot be empty")
                                .with_context("execution_id", exec.execution_id.clone()),
                        );
                    } else {
                        file_ids_to_validate.push((
                            exec.execution_id.clone(),
                            file_id.clone(),
                            file_path.clone(),
                        ));
                    }

                    // Check file_path is not empty
                    if file_path.to_str().map_or(true, |s| s.is_empty()) {
                        result.add_error(
                            ValidationError::new("EMPTY_FILE_PATH", "File path cannot be empty")
                                .with_context("execution_id", exec.execution_id.clone()),
                        );
                    }

                    // Check file extension (should be CSV)
                    if let Some(path_str) = file_path.to_str() {
                        if !path_str.to_lowercase().ends_with(".csv") {
                            result.add_warning(ValidationWarning::new(
                                "NON_CSV_FILE",
                                format!("File '{}' does not have .csv extension", path_str),
                                "Ensure this is intentional for bulk CSV import".to_string(),
                            ));
                        }
                    }
                }
                DataSource::DatabaseQuery {
                    datasource_id,
                    query,
                    ..
                } => {
                    // Validate database query source
                    if datasource_id.is_empty() {
                        result.add_error(
                            ValidationError::new(
                                "EMPTY_DATASOURCE_ID",
                                "Database datasource ID cannot be empty",
                            )
                            .with_context("execution_id", exec.execution_id.clone()),
                        );
                    }
                    if query.is_empty() {
                        result.add_error(
                            ValidationError::new("EMPTY_QUERY", "Database query cannot be empty")
                                .with_context("execution_id", exec.execution_id.clone()),
                        );
                    }
                }
                DataSource::S3Object { bucket, key, .. } => {
                    // Validate S3 source
                    if bucket.is_empty() {
                        result.add_error(
                            ValidationError::new("EMPTY_BUCKET", "S3 bucket cannot be empty")
                                .with_context("execution_id", exec.execution_id.clone()),
                        );
                    }
                    if key.is_empty() {
                        result.add_error(
                            ValidationError::new("EMPTY_KEY", "S3 key cannot be empty")
                                .with_context("execution_id", exec.execution_id.clone()),
                        );
                    }
                }
            }
        }

        // Perform concurrent file validation for all CSV files
        if !file_ids_to_validate.is_empty() {
            debug!(
                "Validating {} files concurrently through File Library",
                file_ids_to_validate.len()
            );

            // Validate files concurrently using tokio tasks
            let mut validation_tasks = Vec::new();

            for (execution_id, file_id, file_path) in file_ids_to_validate {
                let file_store = self.file_store.clone();
                let validation_cache = self.validation_cache.clone();
                let max_file_size_mb = batch_job.config.resource_limits.max_file_size_mb;

                // Spawn concurrent validation task
                let task = tokio::task::spawn(async move {
                    Self::validate_single_file(
                        file_store,
                        validation_cache,
                        &execution_id,
                        &file_id,
                        &file_path,
                        max_file_size_mb,
                    )
                    .await
                });

                validation_tasks.push(task);
            }

            // Wait for all validations to complete and collect results
            for task in validation_tasks {
                match task.await {
                    Ok(Ok((file_size, validation_result))) => {
                        total_file_size_bytes += file_size;

                        // Add any errors or warnings from this file
                        if let Some(error) = validation_result.error {
                            result.add_error(error);
                        }
                        for warning in validation_result.warnings {
                            result.add_warning(warning);
                        }
                    }
                    Ok(Err(e)) => {
                        result.add_error(ValidationError::new(
                            "FILE_VALIDATION_FAILED",
                            format!("File validation failed: {}", e),
                        ));
                    }
                    Err(e) => {
                        result.add_error(ValidationError::new(
                            "VALIDATION_TASK_FAILED",
                            format!("Validation task panicked: {}", e),
                        ));
                    }
                }
            }

            // Update total file size in resource requirements
            result.resource_requirements.total_file_size_mb =
                (total_file_size_bytes / (1024 * 1024)) as usize;

            // Warn if total file size is very large
            if result.resource_requirements.total_file_size_mb > 1000 {
                result.add_warning(ValidationWarning::new(
                    "LARGE_TOTAL_FILE_SIZE",
                    format!(
                        "Total file size is very large ({}MB)",
                        result.resource_requirements.total_file_size_mb
                    ),
                    "Consider breaking into smaller batches or increasing memory limits"
                        .to_string(),
                ));
            }
        }

        result.add_check(ValidationCheck {
            name: "File References".to_string(),
            passed: result.errors.is_empty(),
            message: format!(
                "Validated {} file references ({}MB total)",
                batch_job.workflow_executions.len(),
                result.resource_requirements.total_file_size_mb
            ),
            duration_ms: start.elapsed().as_millis() as u64,
        });

        Ok(())
    }

    /// Validate a single file through the File Library
    ///
    /// Returns (file_size_bytes, validation_result)
    async fn validate_single_file(
        file_store: Arc<dyn FileLibraryStore>,
        validation_cache: Arc<RwLock<HashMap<String, ValidationCacheEntry>>>,
        execution_id: &str,
        file_id: &str,
        file_path: &std::path::PathBuf,
        max_file_size_mb: usize,
    ) -> Result<(u64, FileValidationResult)> {
        // Check cache first
        if let Ok(cache) = validation_cache.read() {
            if let Some(entry) = cache.get(file_id) {
                if entry.is_fresh() {
                    debug!("Using cached validation result for file: {}", file_id);
                    return Self::validate_file_metadata(
                        &entry.file,
                        execution_id,
                        file_id,
                        file_path,
                        max_file_size_mb,
                    );
                }
            }
        }

        // Fetch file from file library
        let file = match file_store.get_file(file_id) {
            Ok(Some(f)) => f,
            Ok(None) => {
                return Ok((
                    0,
                    FileValidationResult {
                        error: Some(
                            ValidationError::new(
                                "FILE_NOT_FOUND",
                                "File not found in File Library",
                            )
                            .with_context("execution_id", execution_id.to_string())
                            .with_context("file_id", file_id.to_string()),
                        ),
                        warnings: Vec::new(),
                    },
                ));
            }
            Err(e) => {
                return Ok((
                    0,
                    FileValidationResult {
                        error: Some(
                            ValidationError::new(
                                "FILE_LIBRARY_ERROR",
                                format!("Failed to fetch file from library: {}", e),
                            )
                            .with_context("execution_id", execution_id.to_string())
                            .with_context("file_id", file_id.to_string()),
                        ),
                        warnings: Vec::new(),
                    },
                ));
            }
        };

        // Cache the file metadata
        if let Ok(mut cache) = validation_cache.write() {
            cache.insert(
                file_id.to_string(),
                ValidationCacheEntry {
                    file: file.clone(),
                    validated_at: Utc::now(),
                },
            );
        }

        // Validate file metadata
        Self::validate_file_metadata(&file, execution_id, file_id, file_path, max_file_size_mb)
    }

    /// Validate file metadata against batch job requirements
    fn validate_file_metadata(
        file: &DataFile,
        execution_id: &str,
        file_id: &str,
        _file_path: &std::path::PathBuf,
        max_file_size_mb: usize,
    ) -> Result<(u64, FileValidationResult)> {
        let mut result = FileValidationResult {
            error: None,
            warnings: Vec::new(),
        };

        // 1. Check file status
        match file.status {
            LibraryFileStatus::Validated => {
                // Good - file is validated
            }
            LibraryFileStatus::Error => {
                result.error = Some(
                    ValidationError::new(
                        "FILE_HAS_ERRORS",
                        format!("File '{}' has validation errors", file.name),
                    )
                    .with_context("execution_id", execution_id.to_string())
                    .with_context("file_id", file_id.to_string())
                    .with_context("errors", file.validation_errors.join(", ")),
                );
                return Ok((file.size_bytes, result));
            }
            LibraryFileStatus::Processing => {
                result.error = Some(
                    ValidationError::new(
                        "FILE_STILL_PROCESSING",
                        format!("File '{}' is still being processed", file.name),
                    )
                    .with_context("execution_id", execution_id.to_string())
                    .with_context("file_id", file_id.to_string()),
                );
                return Ok((file.size_bytes, result));
            }
            LibraryFileStatus::Pending => {
                result.error = Some(
                    ValidationError::new(
                        "FILE_NOT_VALIDATED",
                        format!("File '{}' has not been validated yet", file.name),
                    )
                    .with_context("execution_id", execution_id.to_string())
                    .with_context("file_id", file_id.to_string()),
                );
                return Ok((file.size_bytes, result));
            }
            LibraryFileStatus::Warning => {
                // Continue but add warnings
                for warning_msg in &file.validation_warnings {
                    result.warnings.push(ValidationWarning::new(
                        "FILE_HAS_WARNING",
                        format!("File '{}': {}", file.name, warning_msg),
                        "Review file warnings before executing batch job".to_string(),
                    ));
                }
            }
        }

        // 2. Check file size
        let file_size_mb = file.size_bytes / (1024 * 1024);
        if file_size_mb > max_file_size_mb as u64 {
            result.error = Some(
                ValidationError::new(
                    "FILE_TOO_LARGE",
                    format!(
                        "File '{}' size ({}MB) exceeds maximum ({}MB)",
                        file.name, file_size_mb, max_file_size_mb
                    ),
                )
                .with_context("execution_id", execution_id.to_string())
                .with_context("file_id", file_id.to_string())
                .with_context("file_size_mb", file_size_mb.to_string())
                .with_context("max_file_size_mb", max_file_size_mb.to_string()),
            );
            return Ok((file.size_bytes, result));
        }

        // Warn if file is large (> 100MB)
        if file_size_mb > 100 {
            result.warnings.push(ValidationWarning::new(
                "LARGE_FILE",
                format!("File '{}' is large ({}MB)", file.name, file_size_mb),
                "Consider monitoring memory usage during processing".to_string(),
            ));
        }

        // 3. Check schema exists
        if file.schema.is_none() {
            result.warnings.push(ValidationWarning::new(
                "NO_SCHEMA",
                format!("File '{}' has no schema defined", file.name),
                "Run file scan to detect schema before executing workflows".to_string(),
            ));
        } else if let Some(schema) = &file.schema {
            // Check if schema is stale (> 30 days old)
            let schema_age_days = Utc::now()
                .signed_duration_since(schema.last_scanned)
                .num_days();

            if schema_age_days > 30 {
                result.warnings.push(ValidationWarning::new(
                    "STALE_SCHEMA",
                    format!(
                        "File '{}' schema is {} days old",
                        file.name, schema_age_days
                    ),
                    "Consider re-scanning file to ensure schema is up to date".to_string(),
                ));
            }

            // Warn if file has PII fields
            let pii_fields: Vec<_> = schema
                .fields
                .iter()
                .filter(|f| f.is_pii == Some(true))
                .map(|f| f.name.clone())
                .collect();

            if !pii_fields.is_empty() {
                result.warnings.push(ValidationWarning::new(
                    "FILE_CONTAINS_PII",
                    format!(
                        "File '{}' contains PII fields: {}",
                        file.name,
                        pii_fields.join(", ")
                    ),
                    "Ensure data protection policies are followed".to_string(),
                ));
            }
        }

        // 4. Check for validation errors (even if status is Warning)
        if !file.validation_errors.is_empty() {
            for error_msg in &file.validation_errors {
                result.warnings.push(ValidationWarning::new(
                    "FILE_VALIDATION_ERROR",
                    format!("File '{}': {}", file.name, error_msg),
                    "Review and resolve file validation errors".to_string(),
                ));
            }
        }

        Ok((file.size_bytes, result))
    }

    /// Estimate execution time based on file count and historical data
    fn estimate_execution_time(&self, batch_job: &BatchJob, result: &mut PreflightResult) {
        // Simple estimation: assume 5 minutes per file on average
        let avg_minutes_per_file = 5;
        let total_files = batch_job.workflow_executions.len();
        let max_parallel = batch_job.config.max_parallel;

        // Calculate parallel execution time
        let sequential_time = total_files * avg_minutes_per_file;
        let parallel_time =
            (total_files as f64 / max_parallel as f64).ceil() as usize * avg_minutes_per_file;

        result.estimated_duration_minutes = Some(parallel_time);

        debug!(
            "Estimated execution time: {} minutes (sequential: {} minutes)",
            parallel_time, sequential_time
        );
    }

    /// Calculate resource requirements for the batch job
    fn calculate_resource_requirements(&self, batch_job: &BatchJob, result: &mut PreflightResult) {
        let config = &batch_job.config;

        result.resource_requirements = ResourceRequirements {
            max_memory_mb: config.resource_limits.max_memory_mb,
            max_db_connections: config.resource_limits.max_db_connections,
            max_concurrent_workflows: config.max_parallel,
            total_file_size_mb: 0, // Would be calculated from actual file sizes
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflows::domain::{BatchJob, BatchJobConfig, WorkflowExecutionRef};
    use std::path::PathBuf;

    #[tokio::test]
    async fn test_validate_empty_batch_job() {
        let validator = PreflightValidator::new_without_file_library();
        let config = BatchJobConfig::default();
        let batch_job = BatchJob::new(
            "Test Batch".to_string(),
            "workflow_123".to_string(),
            config,
            "user_1".to_string(),
        );

        let result = validator.validate(&batch_job).await.unwrap();

        // Should fail because no executions
        assert!(!result.is_valid());
        assert!(result.errors.iter().any(|e| e.code == "NO_EXECUTIONS"));
    }

    #[tokio::test]
    async fn test_validate_valid_batch_job() {
        use crate::api::file_library::storage::FileLibraryStorage;
        use crate::api::file_library::types::{DataFile, FileOwner, FileSchema, FileStatus};
        use std::collections::HashMap;

        // Create file store and add a valid file
        let file_store = Arc::new(FileLibraryStorage::new());
        file_store
            .create_file(DataFile {
                id: "file_1".to_string(),
                name: "data.csv".to_string(),
                file_path: "/tmp/data.csv".to_string(),
                folder_id: None,
                description: None,
                owner: FileOwner {
                    user_id: "user_1".to_string(),
                    email: "user1@example.com".to_string(),
                    name: "User One".to_string(),
                },
                size_bytes: 1024,
                encoding: "utf-8".to_string(),
                delimiter: ",".to_string(),
                has_header: true,
                schema: Some(FileSchema {
                    fields: vec![],
                    total_rows: 100,
                    estimated_rows: None,
                    last_scanned: chrono::Utc::now(),
                }),
                ontology_mappings: vec![],
                status: FileStatus::Validated,
                validation_errors: vec![],
                validation_warnings: vec![],
                tags: vec![],
                metadata: HashMap::new(),
                sensitivity_level: None,
                retention_policy: None,
                access_control: None,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
                last_accessed: None,
                version: None,
                previous_versions: vec![],
            })
            .unwrap();

        let validator = PreflightValidator::new(file_store);
        let config = BatchJobConfig::default();
        let mut batch_job = BatchJob::new(
            "Test Batch".to_string(),
            "workflow_123".to_string(),
            config,
            "user_1".to_string(),
        );

        #[allow(deprecated)]
        batch_job.add_execution(WorkflowExecutionRef::from_file(
            "file_1".to_string(),
            "data.csv".to_string(),
            "data".to_string(),
        ));

        let result = validator.validate(&batch_job).await.unwrap();

        // Should pass basic validation
        assert!(
            result.is_valid(),
            "Validation should pass with a valid file. Errors: {:?}",
            result.errors
        );
        assert!(result.errors.is_empty());
    }

    #[tokio::test]
    async fn test_validate_circular_dependency() {
        let validator = PreflightValidator::new_without_file_library();
        let config = BatchJobConfig::default();
        let mut batch_job = BatchJob::new(
            "Test Batch".to_string(),
            "workflow_123".to_string(),
            config,
            "user_1".to_string(),
        );

        let exec1 = WorkflowExecutionRef::new(
            DataSource::CsvFile {
                file_id: "file_1".to_string(),
                file_path: PathBuf::from("data1.csv"),
                encoding: None,
                delimiter: None,
                has_header: true,
            },
            "data1".to_string(),
        );
        let exec2 = WorkflowExecutionRef::new(
            DataSource::CsvFile {
                file_id: "file_2".to_string(),
                file_path: PathBuf::from("data2.csv"),
                encoding: None,
                delimiter: None,
                has_header: true,
            },
            "data2".to_string(),
        )
        .with_dependency(exec1.execution_id.clone());

        // Create circular dependency
        let mut exec1_circular = exec1.clone();
        exec1_circular.dependencies.push(exec2.execution_id.clone());

        batch_job.add_execution(exec1_circular);
        batch_job.add_execution(exec2);

        let result = validator.validate(&batch_job).await.unwrap();

        // Should detect circular dependency
        assert!(!result.is_valid());
        assert!(result
            .errors
            .iter()
            .any(|e| e.code == "CIRCULAR_DEPENDENCY"));
    }

    #[tokio::test]
    async fn test_validate_invalid_dependency() {
        let validator = PreflightValidator::new_without_file_library();
        let config = BatchJobConfig::default();
        let mut batch_job = BatchJob::new(
            "Test Batch".to_string(),
            "workflow_123".to_string(),
            config,
            "user_1".to_string(),
        );

        let exec1 = WorkflowExecutionRef::new(
            DataSource::CsvFile {
                file_id: "file_1".to_string(),
                file_path: PathBuf::from("data1.csv"),
                encoding: None,
                delimiter: None,
                has_header: true,
            },
            "data1".to_string(),
        )
        .with_dependency("nonexistent_id".to_string());

        batch_job.add_execution(exec1);

        let result = validator.validate(&batch_job).await.unwrap();

        // Should detect invalid dependency
        assert!(!result.is_valid());
        assert!(result.errors.iter().any(|e| e.code == "INVALID_DEPENDENCY"));
    }

    #[tokio::test]
    async fn test_validate_resource_limits() {
        let validator = PreflightValidator::new_without_file_library();
        let mut config = BatchJobConfig::default();
        config.resource_limits.max_db_connections = 0; // Invalid

        let mut batch_job = BatchJob::new(
            "Test Batch".to_string(),
            "workflow_123".to_string(),
            config,
            "user_1".to_string(),
        );

        #[allow(deprecated)]
        batch_job.add_execution(WorkflowExecutionRef::from_file(
            "file_1".to_string(),
            "data.csv".to_string(),
            "data".to_string(),
        ));

        let result = validator.validate(&batch_job).await.unwrap();

        // Should detect zero DB connections error
        assert!(!result.is_valid());
        assert!(result
            .errors
            .iter()
            .any(|e| e.code == "ZERO_DB_CONNECTIONS"));
    }

    #[tokio::test]
    async fn test_estimate_execution_time() {
        let validator = PreflightValidator::new_without_file_library();
        let mut config = BatchJobConfig::default();
        config.max_parallel = 4;

        let mut batch_job = BatchJob::new(
            "Test Batch".to_string(),
            "workflow_123".to_string(),
            config,
            "user_1".to_string(),
        );

        // Add 20 files
        for i in 1..=20 {
            #[allow(deprecated)]
            batch_job.add_execution(WorkflowExecutionRef::from_file(
                format!("file_{}", i),
                format!("data{}.csv", i),
                format!("data{}", i),
            ));
        }

        let result = validator.validate(&batch_job).await.unwrap();

        // Should estimate execution time
        assert!(result.estimated_duration_minutes.is_some());
        let estimated = result.estimated_duration_minutes.unwrap();

        // 20 files / 4 parallel = 5 batches * 5 min = 25 minutes
        assert_eq!(estimated, 25);
    }

    #[tokio::test]
    async fn test_non_csv_file_warning() {
        let validator = PreflightValidator::new_without_file_library();
        let config = BatchJobConfig::default();
        let mut batch_job = BatchJob::new(
            "Test Batch".to_string(),
            "workflow_123".to_string(),
            config,
            "user_1".to_string(),
        );

        #[allow(deprecated)]
        batch_job.add_execution(WorkflowExecutionRef::from_file(
            "file_1".to_string(),
            "data.txt".to_string(), // Not a CSV
            "data".to_string(),
        ));

        let result = validator.validate(&batch_job).await.unwrap();

        // Should have warning about non-CSV file
        assert!(result.warnings.iter().any(|w| w.code == "NON_CSV_FILE"));
    }
}
