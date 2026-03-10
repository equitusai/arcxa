//! GDPR Coordinator Service
//!
//! Orchestrates GDPR compliance operations across multiple storage backends.
//! Implements Article 17 (Right to Erasure) by coordinating erasure across
//! row lineage, column lineage, and schema evolution stores.
//!
//! ## Enhanced Features
//!
//! - **User-level erasure**: Erase data for individual users, not just tenants
//! - **Retention policy enforcement**: Prevent premature deletion of legally required data
//! - **Legal hold support**: Block deletion when data is under litigation hold
//! - **Anonymization options**: Support for anonymizing instead of hard deleting

use anyhow::Result;
use chrono::Utc;
use graphica_core::gdpr::{
    BackendErasureResult, DataCategory, DataErasure, DataSubjectId, ErasureRequest, ErasureResult,
    ErasureStrategy, RetentionManager,
};
use std::sync::Arc;
use tracing::{error, info, warn};

use crate::storage::{ColumnLineageStore, RowLineageStore, SchemaEvolutionStore};

/// GDPR Coordinator
///
/// Coordinates GDPR compliance operations across multiple storage backends.
/// Ensures all tenant and user data is erased from all relevant stores while
/// respecting retention policies and legal holds.
pub struct GdprCoordinator {
    row_lineage_store: Option<Arc<RowLineageStore>>,
    column_lineage_store: Option<Arc<ColumnLineageStore>>,
    schema_evolution_store: Option<Arc<SchemaEvolutionStore>>,
    retention_manager: Arc<RetentionManager>,
}

impl GdprCoordinator {
    /// Create a new GDPR coordinator with custom retention manager
    pub fn new(
        row_lineage_store: Option<Arc<RowLineageStore>>,
        column_lineage_store: Option<Arc<ColumnLineageStore>>,
        schema_evolution_store: Option<Arc<SchemaEvolutionStore>>,
        retention_manager: Arc<RetentionManager>,
    ) -> Self {
        Self {
            row_lineage_store,
            column_lineage_store,
            schema_evolution_store,
            retention_manager,
        }
    }

    /// Create a new GDPR coordinator with default retention policies
    pub fn with_default_retention(
        row_lineage_store: Option<Arc<RowLineageStore>>,
        column_lineage_store: Option<Arc<ColumnLineageStore>>,
        schema_evolution_store: Option<Arc<SchemaEvolutionStore>>,
    ) -> Self {
        Self::new(
            row_lineage_store,
            column_lineage_store,
            schema_evolution_store,
            Arc::new(RetentionManager::default()),
        )
    }

    /// Get reference to retention manager
    pub fn retention_manager(&self) -> &RetentionManager {
        &self.retention_manager
    }

    /// Erase all data for a tenant across all storage backends
    ///
    /// This method coordinates erasure across multiple backends, ensuring that
    /// all tenant data is removed from the system. It aggregates results from
    /// each backend into a comprehensive report.
    pub async fn erase_tenant_data(&self, tenant_id: &str, dry_run: bool) -> Result<ErasureResult> {
        let data_subject = DataSubjectId::tenant(tenant_id);
        let mut request = ErasureRequest::new(data_subject, "gdpr_coordinator");

        if dry_run {
            request = request.as_dry_run();
        }

        info!(
            tenant_id = %tenant_id,
            dry_run = %dry_run,
            "Starting GDPR data erasure"
        );

        let mut aggregated_result =
            ErasureResult::new(request.clone(), uuid::Uuid::new_v4().to_string());

        // Erase from row lineage store
        if let Some(ref store) = self.row_lineage_store {
            info!("Erasing tenant data from row lineage store");
            match store.erase_data_subject(&request).await {
                Ok(result) => {
                    for (backend_name, backend_result) in result.backend_results {
                        aggregated_result.add_backend_result(backend_name, backend_result);
                    }
                }
                Err(e) => {
                    error!(error = %e, "Failed to erase from row lineage store");
                    aggregated_result.add_backend_result(
                        "row_lineage".to_string(),
                        BackendErasureResult::failure(
                            "row_lineage",
                            format!("Erasure failed: {}", e),
                            ErasureStrategy::HardDelete,
                        ),
                    );
                }
            }
        } else {
            warn!("Row lineage store not available, skipping");
        }

        // Erase from column lineage store
        if let Some(ref store) = self.column_lineage_store {
            info!("Erasing tenant data from column lineage store");
            match store.erase_data_subject(&request).await {
                Ok(result) => {
                    for (backend_name, backend_result) in result.backend_results {
                        aggregated_result.add_backend_result(backend_name, backend_result);
                    }
                }
                Err(e) => {
                    error!(error = %e, "Failed to erase from column lineage store");
                    aggregated_result.add_backend_result(
                        "column_lineage".to_string(),
                        BackendErasureResult::failure(
                            "column_lineage",
                            format!("Erasure failed: {}", e),
                            ErasureStrategy::HardDelete,
                        ),
                    );
                }
            }
        } else {
            warn!("Column lineage store not available, skipping");
        }

        // Erase from schema evolution store
        if let Some(ref store) = self.schema_evolution_store {
            info!("Erasing tenant data from schema evolution store");
            match store.erase_data_subject(&request).await {
                Ok(result) => {
                    for (backend_name, backend_result) in result.backend_results {
                        aggregated_result.add_backend_result(backend_name, backend_result);
                    }
                }
                Err(e) => {
                    error!(error = %e, "Failed to erase from schema evolution store");
                    aggregated_result.add_backend_result(
                        "schema_evolution".to_string(),
                        BackendErasureResult::failure(
                            "schema_evolution",
                            format!("Erasure failed: {}", e),
                            ErasureStrategy::HardDelete,
                        ),
                    );
                }
            }
        } else {
            warn!("Schema evolution store not available, skipping");
        }

        let final_result = aggregated_result.finalize();

        if final_result.success {
            info!(
                tenant_id = %tenant_id,
                records_erased = %final_result.total_records_erased,
                backends = %final_result.backends_succeeded,
                "GDPR data erasure completed successfully"
            );
        } else {
            warn!(
                tenant_id = %tenant_id,
                records_erased = %final_result.total_records_erased,
                succeeded = %final_result.backends_succeeded,
                failed = %final_result.backends_failed,
                "GDPR data erasure completed with failures"
            );
        }

        Ok(final_result)
    }

    /// Erase all data for a user across all storage backends
    ///
    /// This method is similar to tenant erasure but:
    /// 1. Checks retention policies before deletion
    /// 2. Checks legal holds
    /// 3. Supports anonymization strategies
    /// 4. Provides detailed data category breakdown
    ///
    /// ## Arguments
    ///
    /// * `user_id` - The user identifier to erase
    /// * `dry_run` - If true, only simulates erasure without actual deletion
    /// * `strategy` - Erasure strategy (HardDelete, Anonymize, etc.)
    ///
    /// ## Returns
    ///
    /// Returns an ErasureResult containing:
    /// - Total records affected
    /// - Per-backend success/failure status
    /// - Retention policy violations (if any)
    /// - Legal hold warnings (if any)
    pub async fn erase_user_data(
        &self,
        user_id: &str,
        dry_run: bool,
        strategy: ErasureStrategy,
    ) -> Result<ErasureResult> {
        let data_subject = DataSubjectId::user(user_id);

        // Check legal holds first
        if self.retention_manager.is_subject_under_hold(user_id) {
            let holds = self.retention_manager.get_active_holds_for_subject(user_id);
            let hold_names: Vec<_> = holds.iter().map(|h| h.name.as_str()).collect();

            return Err(anyhow::anyhow!(
                "Cannot erase data for user '{}': under {} active legal hold(s): {}. \
                 Legal holds must be released before erasure can proceed.",
                user_id,
                holds.len(),
                hold_names.join(", ")
            ));
        }

        info!(
            user_id = %user_id,
            dry_run = %dry_run,
            strategy = ?strategy,
            "Starting GDPR user data erasure"
        );

        let mut request = ErasureRequest::new(data_subject.clone(), "gdpr_coordinator");
        request = request.with_strategy(strategy.clone());

        if dry_run {
            request = request.as_dry_run();
        }

        // Check retention policies for different data categories
        // Note: Lineage data is typically categorized as AuditLogs
        let data_created_at = Utc::now(); // TODO: Get actual data creation time from metadata

        if let Err(violation) =
            self.retention_manager
                .can_delete(&DataCategory::AuditLogs, user_id, data_created_at)
        {
            warn!(
                user_id = %user_id,
                violation = %violation,
                "Retention policy violation detected"
            );

            // For audit logs, we might want to use anonymization instead of hard delete
            if matches!(strategy, ErasureStrategy::HardDelete) {
                warn!(
                    "Switching from HardDelete to Anonymize for audit logs due to retention policy"
                );
                request = request.with_strategy(ErasureStrategy::Anonymize);
            }
        }

        let mut aggregated_result =
            ErasureResult::new(request.clone(), uuid::Uuid::new_v4().to_string());

        // Erase from row lineage store
        if let Some(ref store) = self.row_lineage_store {
            info!("Erasing user data from row lineage store");
            match store.erase_data_subject(&request).await {
                Ok(result) => {
                    for (backend_name, backend_result) in result.backend_results {
                        aggregated_result.add_backend_result(backend_name, backend_result);
                    }
                }
                Err(e) => {
                    error!(error = %e, "Failed to erase from row lineage store");
                    aggregated_result.add_backend_result(
                        "row_lineage".to_string(),
                        BackendErasureResult::failure(
                            "row_lineage",
                            format!("Erasure failed: {}", e),
                            strategy.clone(),
                        ),
                    );
                }
            }
        } else {
            warn!("Row lineage store not available, skipping");
        }

        // Erase from column lineage store
        if let Some(ref store) = self.column_lineage_store {
            info!("Erasing user data from column lineage store");
            match store.erase_data_subject(&request).await {
                Ok(result) => {
                    for (backend_name, backend_result) in result.backend_results {
                        aggregated_result.add_backend_result(backend_name, backend_result);
                    }
                }
                Err(e) => {
                    error!(error = %e, "Failed to erase from column lineage store");
                    aggregated_result.add_backend_result(
                        "column_lineage".to_string(),
                        BackendErasureResult::failure(
                            "column_lineage",
                            format!("Erasure failed: {}", e),
                            strategy.clone(),
                        ),
                    );
                }
            }
        } else {
            warn!("Column lineage store not available, skipping");
        }

        // Erase from schema evolution store
        if let Some(ref store) = self.schema_evolution_store {
            info!("Erasing user data from schema evolution store");
            match store.erase_data_subject(&request).await {
                Ok(result) => {
                    for (backend_name, backend_result) in result.backend_results {
                        aggregated_result.add_backend_result(backend_name, backend_result);
                    }
                }
                Err(e) => {
                    error!(error = %e, "Failed to erase from schema evolution store");
                    aggregated_result.add_backend_result(
                        "schema_evolution".to_string(),
                        BackendErasureResult::failure(
                            "schema_evolution",
                            format!("Erasure failed: {}", e),
                            strategy.clone(),
                        ),
                    );
                }
            }
        } else {
            warn!("Schema evolution store not available, skipping");
        }

        let final_result = aggregated_result.finalize();

        if final_result.success {
            info!(
                user_id = %user_id,
                records_erased = %final_result.total_records_erased,
                backends = %final_result.backends_succeeded,
                strategy = ?strategy,
                "GDPR user data erasure completed successfully"
            );
        } else {
            warn!(
                user_id = %user_id,
                records_erased = %final_result.total_records_erased,
                succeeded = %final_result.backends_succeeded,
                failed = %final_result.backends_failed,
                "GDPR user data erasure completed with failures"
            );
        }

        Ok(final_result)
    }

    /// Count total records for a user across all backends (for transparency)
    pub async fn count_user_data(&self, user_id: &str) -> Result<u64> {
        let data_subject = DataSubjectId::user(user_id);
        let mut total = 0u64;

        if let Some(ref store) = self.row_lineage_store {
            total += store.count_data_subject_records(&data_subject).await?;
        }

        if let Some(ref store) = self.column_lineage_store {
            total += store.count_data_subject_records(&data_subject).await?;
        }

        if let Some(ref store) = self.schema_evolution_store {
            total += store.count_data_subject_records(&data_subject).await?;
        }

        Ok(total)
    }

    /// Get detailed breakdown of user data across all backends
    pub async fn get_user_data_breakdown(
        &self,
        user_id: &str,
    ) -> Result<std::collections::HashMap<String, u64>> {
        let data_subject = DataSubjectId::user(user_id);
        let mut breakdown = std::collections::HashMap::new();

        if let Some(ref store) = self.row_lineage_store {
            let store_breakdown = store.get_data_breakdown(&data_subject).await?;
            breakdown.extend(store_breakdown);
        }

        if let Some(ref store) = self.column_lineage_store {
            let store_breakdown = store.get_data_breakdown(&data_subject).await?;
            breakdown.extend(store_breakdown);
        }

        if let Some(ref store) = self.schema_evolution_store {
            let store_breakdown = store.get_data_breakdown(&data_subject).await?;
            breakdown.extend(store_breakdown);
        }

        Ok(breakdown)
    }

    /// Count total records for a tenant across all backends (for transparency)
    pub async fn count_tenant_data(&self, tenant_id: &str) -> Result<u64> {
        let data_subject = DataSubjectId::tenant(tenant_id);
        let mut total = 0u64;

        if let Some(ref store) = self.row_lineage_store {
            total += store.count_data_subject_records(&data_subject).await?;
        }

        if let Some(ref store) = self.column_lineage_store {
            total += store.count_data_subject_records(&data_subject).await?;
        }

        if let Some(ref store) = self.schema_evolution_store {
            total += store.count_data_subject_records(&data_subject).await?;
        }

        Ok(total)
    }

    /// Get detailed breakdown of tenant data across all backends
    pub async fn get_tenant_data_breakdown(
        &self,
        tenant_id: &str,
    ) -> Result<std::collections::HashMap<String, u64>> {
        let data_subject = DataSubjectId::tenant(tenant_id);
        let mut breakdown = std::collections::HashMap::new();

        if let Some(ref store) = self.row_lineage_store {
            let store_breakdown = store.get_data_breakdown(&data_subject).await?;
            breakdown.extend(store_breakdown);
        }

        if let Some(ref store) = self.column_lineage_store {
            let store_breakdown = store.get_data_breakdown(&data_subject).await?;
            breakdown.extend(store_breakdown);
        }

        if let Some(ref store) = self.schema_evolution_store {
            let store_breakdown = store.get_data_breakdown(&data_subject).await?;
            breakdown.extend(store_breakdown);
        }

        Ok(breakdown)
    }

    /// Verify that all tenant data has been erased
    pub async fn verify_erasure(&self, tenant_id: &str) -> Result<bool> {
        let count = self.count_tenant_data(tenant_id).await?;
        Ok(count == 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn create_test_coordinator() -> (GdprCoordinator, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path();

        let row_store = RowLineageStore::new(temp_path.join("row_lineage")).unwrap();

        let column_store = ColumnLineageStore::new(temp_path.join("column_lineage")).unwrap();

        let schema_store = SchemaEvolutionStore::open(temp_path.join("schema_evolution")).unwrap();

        let coordinator = GdprCoordinator::with_default_retention(
            Some(Arc::new(row_store)),
            Some(Arc::new(column_store)),
            Some(Arc::new(schema_store)),
        );

        (coordinator, temp_dir)
    }

    #[tokio::test]
    async fn test_dry_run_erasure() {
        let (coordinator, _temp_dir) = create_test_coordinator().await;

        let result = coordinator
            .erase_tenant_data("tenant-test", true)
            .await
            .unwrap();

        assert!(result.request.dry_run);
        assert_eq!(result.total_records_erased, 0);
    }

    #[tokio::test]
    async fn test_count_tenant_data() {
        let (coordinator, _temp_dir) = create_test_coordinator().await;

        let count = coordinator.count_tenant_data("tenant-test").await.unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_verify_erasure() {
        let (coordinator, _temp_dir) = create_test_coordinator().await;

        let verified = coordinator.verify_erasure("tenant-test").await.unwrap();
        assert!(verified);
    }
}
