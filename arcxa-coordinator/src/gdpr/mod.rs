//! GDPR Compliance Module for Graphica Coordinator
//!
//! Implements core GDPR compliance features:
//! - Article 17: Right to Erasure (Right to be Forgotten)
//! - Article 20: Right to Data Portability

pub mod coordinator;
pub mod export;

pub use coordinator::GdprCoordinator;
