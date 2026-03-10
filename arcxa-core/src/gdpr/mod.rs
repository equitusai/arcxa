//! GDPR Compliance Module
//!
//! This module provides trait-based abstractions for GDPR compliance operations including:
//! - Right to erasure (Article 17)
//! - Right to data portability (Article 20)
//! - Consent management (Article 7)
//! - Data subject access requests (Article 15)
//!
//! ## Design Principles
//!
//! 1. **Storage-agnostic**: Traits can be implemented by any storage backend
//! 2. **Audit-first**: All GDPR operations are logged with cryptographic proof
//! 3. **Cascading deletions**: Automatically handle related data across stores
//! 4. **Immutability-aware**: Support tombstones for immutable audit logs
//! 5. **Distributed-ready**: Coordinate operations across sharded RDF stores

pub mod anonymization;
pub mod consent;
pub mod data_erasure;
pub mod data_export;
pub mod retention;
pub mod types;

pub use anonymization::{
    AnonymizationStrategy, Anonymizer, DateLevel, GeneralizationRules, GeographicLevel,
};
pub use consent::{ConsentManager, ConsentPurpose, ConsentRecord, ConsentStatus};
pub use data_erasure::{
    BackendErasureResult, DataErasure, ErasureRequest, ErasureResult, ErasureStrategy,
};
pub use data_export::{DataExport, ExportFormat, ExportRequest, ExportResult};
pub use retention::{DataCategory, LegalHold, RetentionManager, RetentionPolicy};
pub use types::{DataSubjectId, GdprAuditEvent, GdprRight, ProcessingBasis};
