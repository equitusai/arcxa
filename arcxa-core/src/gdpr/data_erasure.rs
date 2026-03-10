//! Data Erasure Trait and Types
//!
//! Implements GDPR Article 17 (Right to Erasure / "Right to be Forgotten").
//!
//! ## Design Principles
//!
//! 1. **Storage Backend Abstraction**: Trait-based design allows any storage system to implement erasure
//! 2. **Immutability Support**: Some systems (audit logs) use tombstones instead of hard deletion
//! 3. **Cascading Deletions**: Automatically handle related data across multiple column families/stores
//! 4. **Audit Trail**: Every erasure operation is logged for GDPR compliance auditing
//! 5. **Partial Failure Handling**: Gracefully handle scenarios where some deletions succeed and others fail
//!
//! ## Implementation Strategy by Storage Type
//!
//! ### RocksDB Stores
//! - **Mutable stores** (row lineage, column lineage, schema evolution): Hard delete via `delete()`
//! - **Immutable stores** (audit chain): Insert tombstone markers, preserve integrity
//! - **Indexed stores**: Clean up secondary indexes to prevent orphaned entries
//!
//! ### RDF Stores
//! - Use SPARQL DELETE WHERE to remove triples containing data subject identifiers
//! - Coordinate across sharded stores using distributed transaction patterns
//! - Preserve graph structure while removing personal data
//!
//! ### Multi-Tier Storage
//! - **Hot tier**: Delete from active RocksDB
//! - **Warm tier**: Delete from compressed RocksDB
//! - **Cold tier**: Delete from S3/object storage
//! - **Stream tier**: Tombstone in Kafka (log compaction will handle cleanup)

use super::types::{DataSubjectId, GdprAuditEvent};
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Erasure Strategy
///
/// Determines how data should be erased from a particular storage backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErasureStrategy {
    /// Hard delete - permanently remove data from storage
    ///
    /// Used for mutable stores where data can be safely deleted without
    /// compromising system integrity (e.g., row lineage, file metadata).
    HardDelete,

    /// Soft delete - mark data as deleted but keep physical record
    ///
    /// Used for immutable audit logs where deletion would break cryptographic chains.
    /// Instead, insert a tombstone marker that indicates the data has been erased.
    Tombstone,

    /// Anonymize - replace personal data with anonymized values
    ///
    /// Used when statistical or aggregate data must be preserved for analytics,
    /// but personal identifiers can be replaced with random/hashed values.
    Anonymize,

    /// Archive - move data to secure archive before deletion
    ///
    /// Used when legal hold or regulatory requirements mandate retention of
    /// deletion records for a specific period.
    ArchiveThenDelete,
}

/// Erasure Request
///
/// Represents a request to erase all data associated with a data subject.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErasureRequest {
    /// Unique identifier for this erasure request
    pub request_id: String,

    /// The data subject whose data should be erased
    pub data_subject: DataSubjectId,

    /// When the erasure was requested
    pub requested_at: DateTime<Utc>,

    /// Who requested the erasure (data subject, admin, automated process)
    pub requested_by: String,

    /// Optional justification for the erasure
    pub reason: Option<String>,

    /// Whether to perform a dry-run (report what would be deleted without actually deleting)
    pub dry_run: bool,

    /// Storage backends to target (if None, erase from all backends)
    pub target_backends: Option<Vec<String>>,

    /// Strategy to use for erasure (if None, use backend-specific default)
    pub strategy: Option<ErasureStrategy>,
}

impl ErasureRequest {
    /// Create a new erasure request
    pub fn new(data_subject: DataSubjectId, requested_by: impl Into<String>) -> Self {
        Self {
            request_id: uuid::Uuid::new_v4().to_string(),
            data_subject,
            requested_at: Utc::now(),
            requested_by: requested_by.into(),
            reason: None,
            dry_run: false,
            target_backends: None,
            strategy: None,
        }
    }

    /// Set this as a dry-run request
    pub fn as_dry_run(mut self) -> Self {
        self.dry_run = true;
        self
    }

    /// Set a specific erasure strategy
    pub fn with_strategy(mut self, strategy: ErasureStrategy) -> Self {
        self.strategy = Some(strategy);
        self
    }

    /// Set a reason for the erasure
    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }

    /// Target specific storage backends
    pub fn targeting_backends(mut self, backends: Vec<String>) -> Self {
        self.target_backends = Some(backends);
        self
    }
}

/// Erasure Result
///
/// Detailed report of what was erased from each storage backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErasureResult {
    /// The original request
    pub request: ErasureRequest,

    /// When the erasure operation started
    pub started_at: DateTime<Utc>,

    /// When the erasure operation completed
    pub completed_at: DateTime<Utc>,

    /// Overall success status
    pub success: bool,

    /// Results from each storage backend
    pub backend_results: HashMap<String, BackendErasureResult>,

    /// Total number of records erased across all backends
    pub total_records_erased: u64,

    /// Total number of storage backends processed
    pub total_backends_processed: usize,

    /// Number of backends that succeeded
    pub backends_succeeded: usize,

    /// Number of backends that failed
    pub backends_failed: usize,

    /// Audit event generated for this erasure
    pub audit_event_id: String,
}

impl ErasureResult {
    /// Create a new erasure result
    pub fn new(request: ErasureRequest, audit_event_id: String) -> Self {
        Self {
            request,
            started_at: Utc::now(),
            completed_at: Utc::now(),
            success: true,
            backend_results: HashMap::new(),
            total_records_erased: 0,
            total_backends_processed: 0,
            backends_succeeded: 0,
            backends_failed: 0,
            audit_event_id,
        }
    }

    /// Add a backend result
    pub fn add_backend_result(&mut self, backend_name: String, result: BackendErasureResult) {
        self.total_backends_processed += 1;
        self.total_records_erased += result.records_erased;

        if result.success {
            self.backends_succeeded += 1;
        } else {
            self.backends_failed += 1;
            self.success = false;
        }

        self.backend_results.insert(backend_name, result);
    }

    /// Finalize the result (set completion time)
    pub fn finalize(mut self) -> Self {
        self.completed_at = Utc::now();
        self
    }

    /// Get a summary message
    pub fn summary(&self) -> String {
        if self.success {
            format!(
                "Successfully erased {} records from {} backends",
                self.total_records_erased, self.backends_succeeded
            )
        } else {
            format!(
                "Partial erasure: {} records from {}/{} backends ({} failed)",
                self.total_records_erased,
                self.backends_succeeded,
                self.total_backends_processed,
                self.backends_failed
            )
        }
    }
}

/// Result from erasing data from a single storage backend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendErasureResult {
    /// Name of the storage backend
    pub backend_name: String,

    /// Whether the erasure succeeded
    pub success: bool,

    /// Number of records erased
    pub records_erased: u64,

    /// Strategy used for erasure
    pub strategy_used: ErasureStrategy,

    /// Detailed breakdown by data category
    pub details: HashMap<String, u64>,

    /// Error message if erasure failed
    pub error_message: Option<String>,

    /// Warnings (non-fatal issues encountered)
    pub warnings: Vec<String>,
}

impl BackendErasureResult {
    /// Create a successful backend result
    pub fn success(
        backend_name: impl Into<String>,
        records_erased: u64,
        strategy: ErasureStrategy,
    ) -> Self {
        Self {
            backend_name: backend_name.into(),
            success: true,
            records_erased,
            strategy_used: strategy,
            details: HashMap::new(),
            error_message: None,
            warnings: Vec::new(),
        }
    }

    /// Create a failed backend result
    pub fn failure(
        backend_name: impl Into<String>,
        error: impl Into<String>,
        strategy: ErasureStrategy,
    ) -> Self {
        Self {
            backend_name: backend_name.into(),
            success: false,
            records_erased: 0,
            strategy_used: strategy,
            details: HashMap::new(),
            error_message: Some(error.into()),
            warnings: Vec::new(),
        }
    }

    /// Add a detail breakdown
    pub fn with_detail(mut self, category: impl Into<String>, count: u64) -> Self {
        self.details.insert(category.into(), count);
        self
    }

    /// Add a warning
    pub fn with_warning(mut self, warning: impl Into<String>) -> Self {
        self.warnings.push(warning.into());
        self
    }
}

/// Data Erasure Trait
///
/// Storage backends implement this trait to provide GDPR-compliant data erasure.
///
/// ## Example Implementation
///
/// ```rust,ignore
/// use graphica_core::gdpr::{DataErasure, ErasureRequest, ErasureResult};
/// use async_trait::async_trait;
///
/// struct MyStorage {
///     // ... storage fields
/// }
///
/// #[async_trait]
/// impl DataErasure for MyStorage {
///     async fn erase_data_subject(&self, request: &ErasureRequest) -> Result<ErasureResult> {
///         let mut result = ErasureResult::new(request.clone(), "audit-123".to_string());
///
///         // Erase data from storage
///         let records_deleted = self.delete_by_user_id(&request.data_subject.id)?;
///
///         result.add_backend_result(
///             "my_storage".to_string(),
///             BackendErasureResult::success("my_storage", records_deleted, ErasureStrategy::HardDelete)
///         );
///
///         Ok(result.finalize())
///     }
///
///     async fn count_data_subject_records(&self, data_subject: &DataSubjectId) -> Result<u64> {
///         // Count records without actually deleting
///         Ok(self.count_by_user_id(&data_subject.id)?)
///     }
/// }
/// ```
#[async_trait]
pub trait DataErasure: Send + Sync {
    /// Erase all data associated with a data subject
    ///
    /// This is the primary method for implementing GDPR Article 17 (Right to Erasure).
    /// Implementations must:
    /// 1. Find all data associated with the data subject identifier
    /// 2. Delete or anonymize the data according to the erasure strategy
    /// 3. Clean up any secondary indexes or related data
    /// 4. Return a detailed result indicating what was erased
    ///
    /// # Arguments
    /// * `request` - The erasure request containing data subject ID and parameters
    ///
    /// # Returns
    /// * `Ok(ErasureResult)` - Detailed report of what was erased
    /// * `Err(...)` - If the erasure operation failed catastrophically
    async fn erase_data_subject(&self, request: &ErasureRequest) -> Result<ErasureResult>;

    /// Count how many records exist for a data subject (without erasing)
    ///
    /// Useful for dry-run operations and providing transparency to data subjects
    /// about how much data is stored about them.
    ///
    /// # Arguments
    /// * `data_subject` - The data subject identifier to count records for
    ///
    /// # Returns
    /// * `Ok(u64)` - Number of records found
    /// * `Err(...)` - If the count operation failed
    async fn count_data_subject_records(&self, data_subject: &DataSubjectId) -> Result<u64>;

    /// Get detailed breakdown of data by category
    ///
    /// Optional method that provides more granular insight into what types of data
    /// are stored for a data subject.
    ///
    /// # Arguments
    /// * `data_subject` - The data subject identifier
    ///
    /// # Returns
    /// * `HashMap<String, u64>` - Map of data category to record count
    ///   Example: {"lineage_events": 1234, "audit_logs": 56, "file_metadata": 12}
    async fn get_data_breakdown(
        &self,
        data_subject: &DataSubjectId,
    ) -> Result<HashMap<String, u64>> {
        // Default implementation: just return total count as "total"
        let total = self.count_data_subject_records(data_subject).await?;
        let mut breakdown = HashMap::new();
        breakdown.insert("total".to_string(), total);
        Ok(breakdown)
    }

    /// Verify that erasure was successful
    ///
    /// After an erasure operation, this method can be called to confirm that
    /// no data remains for the data subject.
    ///
    /// # Arguments
    /// * `data_subject` - The data subject identifier to verify
    ///
    /// # Returns
    /// * `Ok(true)` - No data found (erasure successful)
    /// * `Ok(false)` - Data still exists (erasure incomplete)
    /// * `Err(...)` - If verification failed
    async fn verify_erasure(&self, data_subject: &DataSubjectId) -> Result<bool> {
        let count = self.count_data_subject_records(data_subject).await?;
        Ok(count == 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_erasure_request_creation() {
        let data_subject = DataSubjectId::user("user123");
        let request = ErasureRequest::new(data_subject.clone(), "admin@example.com");

        assert_eq!(request.data_subject, data_subject);
        assert_eq!(request.requested_by, "admin@example.com");
        assert!(!request.dry_run);
        assert!(request.strategy.is_none());
        assert!(request.target_backends.is_none());
    }

    #[test]
    fn test_erasure_request_builder() {
        let request = ErasureRequest::new(DataSubjectId::user("user123"), "admin@example.com")
            .as_dry_run()
            .with_strategy(ErasureStrategy::Anonymize)
            .with_reason("User requested account deletion")
            .targeting_backends(vec!["row_lineage".to_string(), "audit_log".to_string()]);

        assert!(request.dry_run);
        assert_eq!(request.strategy, Some(ErasureStrategy::Anonymize));
        assert_eq!(request.reason.unwrap(), "User requested account deletion");
        assert_eq!(request.target_backends.unwrap().len(), 2);
    }

    #[test]
    fn test_erasure_result_aggregation() {
        let request = ErasureRequest::new(DataSubjectId::user("user123"), "admin@example.com");
        let mut result = ErasureResult::new(request, "audit-event-123".to_string());

        // Add successful backend result
        result.add_backend_result(
            "row_lineage".to_string(),
            BackendErasureResult::success("row_lineage", 1234, ErasureStrategy::HardDelete)
                .with_detail("lineage_events", 1200)
                .with_detail("batch_metadata", 34),
        );

        // Add another successful backend result
        result.add_backend_result(
            "column_lineage".to_string(),
            BackendErasureResult::success("column_lineage", 567, ErasureStrategy::HardDelete),
        );

        // Add failed backend result
        result.add_backend_result(
            "audit_log".to_string(),
            BackendErasureResult::failure(
                "audit_log",
                "RocksDB connection timeout",
                ErasureStrategy::Tombstone,
            ),
        );

        let result = result.finalize();

        assert_eq!(result.total_backends_processed, 3);
        assert_eq!(result.backends_succeeded, 2);
        assert_eq!(result.backends_failed, 1);
        assert_eq!(result.total_records_erased, 1801);
        assert!(!result.success); // Overall failure because one backend failed

        let summary = result.summary();
        assert!(summary.contains("1801 records"));
        assert!(summary.contains("2/3 backends"));
        assert!(summary.contains("1 failed"));
    }

    #[test]
    fn test_backend_erasure_result() {
        let result =
            BackendErasureResult::success("row_lineage", 1000, ErasureStrategy::HardDelete)
                .with_detail("csv_rows", 800)
                .with_detail("db2_rows", 200)
                .with_warning("Some batch metadata could not be verified");

        assert!(result.success);
        assert_eq!(result.records_erased, 1000);
        assert_eq!(result.details.get("csv_rows"), Some(&800));
        assert_eq!(result.warnings.len(), 1);

        let failed_result = BackendErasureResult::failure(
            "rdf_store",
            "SPARQL DELETE timeout after 30s",
            ErasureStrategy::HardDelete,
        );

        assert!(!failed_result.success);
        assert_eq!(failed_result.records_erased, 0);
        assert!(failed_result.error_message.is_some());
    }

    #[test]
    fn test_erasure_strategy_serialization() {
        let strategy = ErasureStrategy::HardDelete;
        let json = serde_json::to_string(&strategy).unwrap();
        assert_eq!(json, "\"hard_delete\"");

        let deserialized: ErasureStrategy = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, ErasureStrategy::HardDelete);
    }
}
