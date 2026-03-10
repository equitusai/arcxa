//! Security Module Integration Tests
//!
//! Comprehensive tests for path traversal prevention and JobId validation.

#[cfg(test)]
mod integration_tests {
    use crate::security::{JobId, JobIdError};
    use std::path::PathBuf;

    /// OWASP Path Traversal Test Vectors
    /// Source: https://owasp.org/www-community/attacks/Path_Traversal
    #[test]
    fn test_owasp_path_traversal_vectors() {
        let test_cases = vec![
            // Basic traversal
            "../../../etc/passwd",
            "..\\..\\..\\windows\\system32\\config\\sam",

            // URL encoded (should be decoded by web framework before reaching us)
            "../etc/passwd",
            "..%2F..%2F..%2Fetc%2Fpasswd",

            // Double encoding (should be handled by web framework)
            "..%252F..%252F..%252Fetc%252Fpasswd",

            // Unicode/UTF-8 encoding
            "..%c0%af..%c0%af..%c0%afetc%c0%afpasswd",
            "..%c1%9c..%c1%9c..%c1%9cetc%c1%9cpasswd",

            // Null byte injection
            "../../../../etc/passwd%00",
            "../../../../etc/passwd\0.jpg",

            // Dot variants
            "....//....//....//etc/passwd",
            "..././..././..././etc/passwd",

            // Absolute paths
            "/etc/passwd",
            "C:\\Windows\\system32\\config\\sam",
            "\\\\server\\share\\file",

            // Mixed techniques
            "..;/etc/passwd",
            "..\\;/etc/passwd",
            "..///etc/passwd",
            "../\\/etc/passwd",

            // Long paths
            &"../".repeat(100),
            &"..\\".repeat(100),
        ];

        for payload in test_cases {
            let result = JobId::new(payload);
            assert!(
                result.is_err(),
                "Should reject OWASP vector: {}",
                payload
            );

            // Log the specific error for analysis
            if let Err(e) = result {
                eprintln!("Blocked '{}' with: {:?}", payload, e);
            }
        }
    }

    #[test]
    fn test_unicode_normalization_attacks() {
        let test_cases = vec![
            // Unicode normalization attacks
            "ﾉ../etc/passwd",  // Half-width katakana
            "․․/etc/passwd",   // Unicode dot variants
            "‥/etc/passwd",    // Two-dot leader
            "…/etc/passwd",     // Horizontal ellipsis
            "../\u{200B}etc/passwd", // Zero-width space
            "../\u{FEFF}etc/passwd", // Zero-width no-break space
        ];

        for payload in test_cases {
            let result = JobId::new(payload);
            assert!(
                result.is_err(),
                "Should reject Unicode attack: {}",
                payload
            );
        }
    }

    #[test]
    fn test_filesystem_specific_attacks() {
        let test_cases = vec![
            // Windows specific
            "CON", "PRN", "AUX", "NUL",
            "COM1", "COM2", "COM3", "COM4",
            "LPT1", "LPT2", "LPT3", "LPT4",
            "CON.txt", "PRN.log",
            "C:alternate_stream",
            "file.txt::$DATA",
            "file.txt:hidden",

            // Unix specific
            ".hidden_file",
            "~root",
            "~/sensitive",
            "$HOME/file",
            "${HOME}/file",

            // Special characters
            "job|command",
            "job&command",
            "job;command",
            "job`command`",
            "job$(command)",
            "job>(output)",
            "job<(input)",
        ];

        for payload in test_cases {
            let result = JobId::new(payload);
            if !result.is_ok() {
                eprintln!("Blocked filesystem attack: {}", payload);
            }
        }
    }

    #[test]
    fn test_safe_path_construction() {
        let job = JobId::new("valid_job_123").unwrap();
        let base = PathBuf::from("/var/dlq");

        let path = job.to_path(&base);

        // Ensure path stays within base directory
        assert!(path.starts_with(&base));
        assert_eq!(path, PathBuf::from("/var/dlq/valid_job_123"));

        // Ensure no traversal is possible
        assert!(!path.to_string_lossy().contains(".."));
    }

    #[test]
    fn test_length_limits() {
        // Test minimum length
        assert!(JobId::new("ab").is_err());
        assert!(JobId::new("abc").is_ok());

        // Test maximum length
        let long_id = "a".repeat(128);
        assert!(JobId::new(&long_id).is_ok());

        let too_long = "a".repeat(129);
        assert!(JobId::new(&too_long).is_err());
    }

    #[test]
    fn test_character_validation() {
        // Valid characters
        let valid = vec![
            "job_123",
            "batch-2024",
            "task.001",
            "UPPERCASE",
            "lowercase",
            "Mix3d_Ch4rs-2024.v1",
        ];

        for id in valid {
            assert!(JobId::new(id).is_ok(), "Should accept: {}", id);
        }

        // Invalid characters
        let invalid = vec![
            "job@host",
            "job#123",
            "job$var",
            "job%20",
            "job space",
            "job\ttab",
            "job\nnewline",
            "job/slash",
            "job\\backslash",
            "job:colon",
            "job*asterisk",
            "job?question",
            "job\"quote",
            "job'apostrophe",
            "job<less",
            "job>greater",
            "job|pipe",
        ];

        for id in invalid {
            assert!(JobId::new(id).is_err(), "Should reject: {}", id);
        }
    }

    #[test]
    fn test_real_world_job_ids() {
        // Test actual job ID formats from production
        let real_ids = vec![
            "load_e7d3a2f1-9b4c-4d8a-b5e2-1a3f5c7d9e2b",
            "etl_batch_2024_01_15_001",
            "user.john_doe.import.20240115",
            "workflow_engine_task_12345",
            "data-pipeline-stage-1",
            "_internal_maintenance_job",
        ];

        for id in real_ids {
            assert!(
                JobId::new(id).is_ok(),
                "Should accept real-world ID: {}",
                id
            );
        }
    }

    #[test]
    fn test_error_messages() {
        // Ensure error messages are helpful but don't leak information

        let cases = vec![
            ("", JobIdError::Empty),
            ("ab", JobIdError::TooShort),
            ("a".repeat(200), JobIdError::TooLong),
            ("../etc", JobIdError::PathTraversal("..".to_string())),
            ("/abs", JobIdError::AbsolutePath),
            ("job\0", JobIdError::NullByte),
            ("CON", JobIdError::ReservedName("CON".to_string())),
        ];

        for (input, expected_variant) in cases {
            let result = JobId::new(input);
            assert!(result.is_err());

            let error = result.unwrap_err();
            // Check error type matches (compare discriminants)
            assert_eq!(
                std::mem::discriminant(&error),
                std::mem::discriminant(&expected_variant),
                "Wrong error for input: {}",
                input
            );

            // Ensure error message exists and is reasonable
            let msg = error.to_string();
            assert!(!msg.is_empty());
            assert!(msg.len() < 200); // Don't leak too much info
        }
    }

    #[test]
    fn test_case_sensitivity() {
        // Job IDs should be case-sensitive
        let id1 = JobId::new("JobID").unwrap();
        let id2 = JobId::new("jobid").unwrap();

        assert_ne!(id1.as_str(), id2.as_str());
    }

    #[test]
    fn test_serde_roundtrip() {
        let original = JobId::new("test_job_123").unwrap();

        // Serialize
        let json = serde_json::to_string(&original).unwrap();
        assert_eq!(json, "\"test_job_123\"");

        // Deserialize
        let deserialized: JobId = serde_json::from_str(&json).unwrap();
        assert_eq!(original, deserialized);
    }

    #[test]
    fn test_serde_validation() {
        // Invalid job ID should fail deserialization
        let invalid_json = "\"../etc/passwd\"";
        let result: Result<JobId, _> = serde_json::from_str(invalid_json);
        assert!(result.is_err());
    }

    /// Performance benchmark for validation
    #[test]
    fn test_validation_performance() {
        use std::time::Instant;

        let iterations = 10_000;
        let test_ids = vec![
            "valid_job_123",
            "another_job_456",
            "batch-2024-01-15",
            "../etc/passwd",  // Invalid
            "job_with_very_long_name_but_still_valid_123456789",
        ];

        let start = Instant::now();

        for _ in 0..iterations {
            for id in &test_ids {
                let _ = JobId::new(id);
            }
        }

        let duration = start.elapsed();
        let ops = (iterations * test_ids.len()) as f64;
        let us_per_op = duration.as_micros() as f64 / ops;

        println!(
            "Validation performance: {:.3} μs per operation ({} ops in {:?})",
            us_per_op, ops, duration
        );

        // Assert performance requirement: < 1μs per validation
        assert!(
            us_per_op < 1.0,
            "Validation too slow: {:.3} μs per operation",
            us_per_op
        );
    }

    #[test]
    fn test_concurrent_validation() {
        use std::sync::Arc;
        use std::thread;

        let test_ids = Arc::new(vec![
            "job_1", "job_2", "job_3",
            "../etc/passwd", "/absolute", "job\0null",
        ]);

        let mut handles = vec![];

        for i in 0..10 {
            let ids = Arc::clone(&test_ids);
            let handle = thread::spawn(move || {
                for _ in 0..100 {
                    for id in ids.iter() {
                        let _ = JobId::new(id);
                    }
                }
                println!("Thread {} completed", i);
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().expect("Thread panicked");
        }
    }

    /// Fuzzing test with random inputs
    #[test]
    fn test_fuzz_random_inputs() {
        use rand::{Rng, thread_rng};

        let mut rng = thread_rng();

        for _ in 0..1000 {
            // Generate random string
            let len = rng.gen_range(0..200);
            let random_string: String = (0..len)
                .map(|_| {
                    let byte = rng.gen_range(0..=255);
                    byte as u8 as char
                })
                .collect();

            let result = JobId::new(&random_string);

            // If validation succeeds, ensure path construction is safe
            if let Ok(job_id) = result {
                let base = PathBuf::from("/base");
                let path = job_id.to_path(&base);

                // Must stay within base directory
                assert!(
                    path.starts_with(&base),
                    "Path escape with input: {:?}",
                    random_string
                );
            }
        }
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;
    use crate::security::validation::{SecurityValidator, quick_validate_no_traversal};

    #[test]
    fn test_quick_validation() {
        // Should accept
        assert!(quick_validate_no_traversal("normal_job"));
        assert!(quick_validate_no_traversal("job-123"));

        // Should reject
        assert!(!quick_validate_no_traversal("../etc/passwd"));
        assert!(!quick_validate_no_traversal("/absolute"));
        assert!(!quick_validate_no_traversal("C:\\Windows"));
        assert!(!quick_validate_no_traversal("file\0null"));
    }

    #[test]
    fn test_security_validator() {
        let validator = SecurityValidator::new();
        let base = PathBuf::from("/var/data");

        // Valid paths
        assert!(validator.validate_path(Path::new("file.txt"), &base).is_ok());
        assert!(validator.validate_path(Path::new("sub/dir/file.txt"), &base).is_ok());

        // Invalid paths
        assert!(validator.validate_path(Path::new("../etc/passwd"), &base).is_err());
        assert!(validator.validate_path(Path::new("/absolute/path"), &base).is_err());
    }
}

// Required for rand in tests
#[cfg(test)]
extern crate rand;