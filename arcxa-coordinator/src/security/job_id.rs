//! JobId Security Module
//!
//! Provides a type-safe, validated wrapper around job IDs to prevent path traversal
//! and other security vulnerabilities.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::{Path, PathBuf};

/// Maximum allowed length for job IDs
const MAX_JOB_ID_LENGTH: usize = 128;

/// Allowed characters in job IDs (alphanumeric, hyphen, underscore)
const ALLOWED_CHARS: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_";

/// Errors that can occur during job ID validation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobIdError {
    /// Job ID is empty
    Empty,
    /// Job ID exceeds maximum length
    TooLong { length: usize, max: usize },
    /// Job ID contains invalid characters
    InvalidCharacters { invalid_chars: Vec<char> },
    /// Job ID contains path traversal sequences
    PathTraversal { sequence: String },
    /// Job ID contains null bytes
    NullByte,
    /// Job ID contains control characters
    ControlCharacters { positions: Vec<usize> },
    /// Job ID resolves to a path outside the base directory
    PathEscapes { resolved: PathBuf, base: PathBuf },
}

impl fmt::Display for JobIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JobIdError::Empty => write!(f, "Job ID cannot be empty"),
            JobIdError::TooLong { length, max } => {
                write!(f, "Job ID too long: {} characters (max: {})", length, max)
            }
            JobIdError::InvalidCharacters { invalid_chars } => {
                write!(
                    f,
                    "Job ID contains invalid characters: {}",
                    invalid_chars
                        .iter()
                        .map(|c| format!("'{}'", c))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
            JobIdError::PathTraversal { sequence } => {
                write!(f, "Job ID contains path traversal sequence: {}", sequence)
            }
            JobIdError::NullByte => write!(f, "Job ID contains null byte"),
            JobIdError::ControlCharacters { positions } => {
                write!(
                    f,
                    "Job ID contains control characters at positions: {:?}",
                    positions
                )
            }
            JobIdError::PathEscapes { resolved, base } => {
                write!(
                    f,
                    "Job ID resolves to path outside base directory: {} (base: {})",
                    resolved.display(),
                    base.display()
                )
            }
        }
    }
}

impl std::error::Error for JobIdError {}

/// A validated, type-safe job identifier
///
/// JobId enforces security constraints at construction time:
/// - Non-empty
/// - Maximum length limit
/// - Only alphanumeric, hyphen, and underscore characters
/// - No path traversal sequences (.., ./, etc.)
/// - No null bytes or control characters
///
/// # Examples
///
/// ```
/// use graphica_coordinator::security::job_id::JobId;
///
/// // Valid job IDs
/// let job1 = JobId::new("job-123").unwrap();
/// let job2 = JobId::new("data_ingestion_2024").unwrap();
///
/// // Invalid job IDs
/// assert!(JobId::new("../etc/passwd").is_err());  // Path traversal
/// assert!(JobId::new("job@123").is_err());        // Invalid character
/// assert!(JobId::new("").is_err());               // Empty
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct JobId(String);

impl JobId {
    /// Create a new JobId with validation
    ///
    /// # Errors
    ///
    /// Returns `JobIdError` if validation fails
    pub fn new(id: impl AsRef<str>) -> Result<Self, JobIdError> {
        let id = id.as_ref();

        // Check for empty
        if id.is_empty() {
            return Err(JobIdError::Empty);
        }

        // Check length
        if id.len() > MAX_JOB_ID_LENGTH {
            return Err(JobIdError::TooLong {
                length: id.len(),
                max: MAX_JOB_ID_LENGTH,
            });
        }

        // Check for null bytes
        if id.contains('\0') {
            return Err(JobIdError::NullByte);
        }

        // Check for control characters
        let control_positions: Vec<usize> = id
            .chars()
            .enumerate()
            .filter(|(_, c)| c.is_control())
            .map(|(i, _)| i)
            .collect();

        if !control_positions.is_empty() {
            return Err(JobIdError::ControlCharacters {
                positions: control_positions,
            });
        }

        // Check for path traversal sequences
        if id.contains("..") {
            return Err(JobIdError::PathTraversal {
                sequence: "..".to_string(),
            });
        }

        if id.contains("./") || id.contains(".\\") {
            return Err(JobIdError::PathTraversal {
                sequence: if id.contains("./") { "./" } else { ".\\" }.to_string(),
            });
        }

        // Check for invalid characters
        let invalid_chars: Vec<char> = id.chars().filter(|c| !ALLOWED_CHARS.contains(*c)).collect();

        if !invalid_chars.is_empty() {
            return Err(JobIdError::InvalidCharacters { invalid_chars });
        }

        Ok(JobId(id.to_string()))
    }

    /// Create a JobId without validation (unsafe - use only for trusted sources)
    ///
    /// # Safety
    ///
    /// Caller must ensure the input is already validated or comes from a trusted source.
    /// This should only be used in testing or when loading from a validated store.
    #[doc(hidden)]
    pub unsafe fn new_unchecked(id: impl Into<String>) -> Self {
        JobId(id.into())
    }

    /// Get the underlying string value
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Convert to a safe filesystem path component
    ///
    /// This method ensures the job ID can be safely used as a directory name
    /// by verifying it doesn't escape the base directory.
    ///
    /// # Errors
    ///
    /// Returns `JobIdError::PathEscapes` if the resolved path is outside the base directory
    pub fn to_safe_path(&self, base_dir: impl AsRef<Path>) -> Result<PathBuf, JobIdError> {
        let base = base_dir.as_ref();
        let path = base.join(&self.0);

        // Canonicalize to resolve any remaining symbolic links or path components
        // Note: This requires the base directory to exist
        let canonical_base = base.canonicalize().map_err(|_| JobIdError::PathEscapes {
            resolved: path.clone(),
            base: base.to_path_buf(),
        })?;

        // Check if path starts with base (even if it doesn't exist yet)
        // We can't canonicalize the full path if it doesn't exist, so we check the parent
        let path_parent = path.parent().unwrap_or(base);
        if let Ok(canonical_parent) = path_parent.canonicalize() {
            if !canonical_parent.starts_with(&canonical_base) {
                return Err(JobIdError::PathEscapes {
                    resolved: path.clone(),
                    base: base.to_path_buf(),
                });
            }
        }

        Ok(path)
    }

    /// Consume the JobId and return the inner String
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl fmt::Display for JobId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for JobId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for JobId {
    type Error = JobIdError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        JobId::new(value)
    }
}

impl TryFrom<&str> for JobId {
    type Error = JobIdError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        JobId::new(value)
    }
}

impl From<JobId> for String {
    fn from(job_id: JobId) -> Self {
        job_id.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_job_ids() {
        assert!(JobId::new("job-123").is_ok());
        assert!(JobId::new("data_ingestion_2024").is_ok());
        assert!(JobId::new("ABC-DEF-123").is_ok());
        assert!(JobId::new("a1-b2_c3").is_ok());
    }

    #[test]
    fn test_empty_job_id() {
        assert_eq!(JobId::new(""), Err(JobIdError::Empty));
    }

    #[test]
    fn test_too_long_job_id() {
        let long_id = "a".repeat(MAX_JOB_ID_LENGTH + 1);
        match JobId::new(&long_id) {
            Err(JobIdError::TooLong { length, max }) => {
                assert_eq!(length, MAX_JOB_ID_LENGTH + 1);
                assert_eq!(max, MAX_JOB_ID_LENGTH);
            }
            _ => panic!("Expected TooLong error"),
        }
    }

    #[test]
    fn test_path_traversal_prevention() {
        assert!(matches!(
            JobId::new("../etc/passwd"),
            Err(JobIdError::PathTraversal { .. })
        ));
        assert!(matches!(
            JobId::new("./config"),
            Err(JobIdError::PathTraversal { .. })
        ));
        assert!(matches!(
            JobId::new("data/../etc"),
            Err(JobIdError::PathTraversal { .. })
        ));
    }

    #[test]
    fn test_invalid_characters() {
        assert!(matches!(
            JobId::new("job@123"),
            Err(JobIdError::InvalidCharacters { .. })
        ));
        assert!(matches!(
            JobId::new("job/123"),
            Err(JobIdError::InvalidCharacters { .. })
        ));
        assert!(matches!(
            JobId::new("job\\123"),
            Err(JobIdError::InvalidCharacters { .. })
        ));
        assert!(matches!(
            JobId::new("job:123"),
            Err(JobIdError::InvalidCharacters { .. })
        ));
    }

    #[test]
    fn test_null_byte() {
        assert_eq!(JobId::new("job\0123"), Err(JobIdError::NullByte));
    }

    #[test]
    fn test_control_characters() {
        assert!(matches!(
            JobId::new("job\n123"),
            Err(JobIdError::ControlCharacters { .. })
        ));
        assert!(matches!(
            JobId::new("job\r123"),
            Err(JobIdError::ControlCharacters { .. })
        ));
    }

    #[test]
    fn test_safe_path_creation() {
        let temp_dir = std::env::temp_dir();
        let job_id = JobId::new("test-job-123").unwrap();

        let safe_path = job_id.to_safe_path(&temp_dir).unwrap();
        assert!(safe_path.starts_with(&temp_dir));
        assert!(safe_path.ends_with("test-job-123"));
    }

    #[test]
    fn test_display() {
        let job_id = JobId::new("test-job").unwrap();
        assert_eq!(job_id.to_string(), "test-job");
    }

    #[test]
    fn test_serde() {
        let job_id = JobId::new("test-job-123").unwrap();
        let json = serde_json::to_string(&job_id).unwrap();
        assert_eq!(json, "\"test-job-123\"");

        let deserialized: JobId = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, job_id);
    }

    #[test]
    fn test_serde_invalid() {
        let result: Result<JobId, _> = serde_json::from_str("\"../invalid\"");
        assert!(result.is_err());
    }
}
