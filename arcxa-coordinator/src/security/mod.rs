//! Security Module
//!
//! Provides security utilities and validation for the Graphica platform.
//!
//! # Job ID Security
//!
//! The primary component is the `JobId` type, which provides compile-time
//! guarantees that job identifiers are safe to use in filesystem operations
//! and API calls.
//!
//! # Examples
//!
//! ```
//! use graphica_coordinator::security::job_id::JobId;
//!
//! // Validate a job ID at API boundaries
//! fn handle_request(job_id_str: &str) -> Result<(), Box<dyn std::error::Error>> {
//!     let job_id = JobId::new(job_id_str)?;
//!
//!     // Now safe to use in filesystem operations
//!     let path = job_id.to_safe_path("/data/jobs")?;
//!
//!     Ok(())
//! }
//! ```

pub mod job_id;
pub mod validation;

// Re-export main types
pub use job_id::{JobId, JobIdError};
pub use validation::SecurityValidator;
