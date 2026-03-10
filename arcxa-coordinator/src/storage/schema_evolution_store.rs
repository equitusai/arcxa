//! Schema Evolution Store - RocksDB-based storage for schema change tracking
//!
//! Stores schema change events, schema versions, and provides drift analysis capabilities.
//!
//! Column families:
//! - `schema_events`: Schema change events keyed by event ID
//! - `schema_events_by_datasource`: Events grouped by datasource for efficient querying
//! - `schema_events_by_table`: Events grouped by table for table-specific history
//! - `schema_events_by_time`: Time-indexed events for temporal queries
//! - `schema_versions`: Complete schema snapshots keyed by version ID
//! - `schema_versions_by_datasource`: Versions grouped by datasource

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use graphica_core::core::lineage::schema_evolution::{
    DriftSeverity, MigrationImpactAnalysis, RiskLevel, SchemaChangeEvent, SchemaChangeType,
    SchemaDriftAnalysis, SchemaVersion,
};
use graphica_core::gdpr::{
    BackendErasureResult, DataErasure, DataSubjectId, ErasureRequest, ErasureResult,
    ErasureStrategy,
};
use rocksdb::{ColumnFamilyDescriptor, Options, WriteBatch, DB};
use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use super::serialization_version::VersionedData;

/// Column family names for schema evolution tracking
const CF_SCHEMA_EVENTS: &str = "schema_events";
const CF_EVENTS_BY_DATASOURCE: &str = "schema_events_by_datasource";
const CF_EVENTS_BY_TABLE: &str = "schema_events_by_table";
const CF_EVENTS_BY_TIME: &str = "schema_events_by_time";
const CF_SCHEMA_VERSIONS: &str = "schema_versions";
const CF_VERSIONS_BY_DATASOURCE: &str = "schema_versions_by_datasource";
const CF_EVENTS_BY_TENANT: &str = "schema_events_by_tenant"; // GDPR erasure

/// Schema Evolution Store using RocksDB
pub struct SchemaEvolutionStore {
    db: Arc<DB>,
}

impl SchemaEvolutionStore {
    /// Open or create a schema evolution store at the given path
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);

        // Define column families
        let cfs = vec![
            ColumnFamilyDescriptor::new(CF_SCHEMA_EVENTS, Options::default()),
            ColumnFamilyDescriptor::new(CF_EVENTS_BY_DATASOURCE, Options::default()),
            ColumnFamilyDescriptor::new(CF_EVENTS_BY_TABLE, Options::default()),
            ColumnFamilyDescriptor::new(CF_EVENTS_BY_TIME, Options::default()),
            ColumnFamilyDescriptor::new(CF_SCHEMA_VERSIONS, Options::default()),
            ColumnFamilyDescriptor::new(CF_VERSIONS_BY_DATASOURCE, Options::default()),
            ColumnFamilyDescriptor::new(CF_EVENTS_BY_TENANT, Options::default()),
        ];

        let db = DB::open_cf_descriptors(&opts, path, cfs)
            .context("Failed to open RocksDB for schema evolution store")?;

        Ok(Self { db: Arc::new(db) })
    }

    // =============================================================================
    // Schema Change Event Methods
    // =============================================================================

    /// Record a schema change event
    pub fn record_schema_change(&self, event: SchemaChangeEvent) -> Result<()> {
        let event_id = event.id.clone();
        let datasource_id = event.datasource_id.clone();
        let table_name = event.table_name.clone();
        let detected_at = event.detected_at.timestamp_millis();

        // Serialize the event
        let versioned = VersionedData::wrap(event.clone());
        let serialized = versioned.serialize()?;

        // Store in primary CF
        let cf_events = self
            .db
            .cf_handle(CF_SCHEMA_EVENTS)
            .ok_or_else(|| anyhow!("Column family {} not found", CF_SCHEMA_EVENTS))?;
        self.db
            .put_cf(cf_events, event_id.as_bytes(), &serialized)?;

        // Index by datasource
        let cf_by_datasource = self
            .db
            .cf_handle(CF_EVENTS_BY_DATASOURCE)
            .ok_or_else(|| anyhow!("Column family {} not found", CF_EVENTS_BY_DATASOURCE))?;

        let datasource_key = format!("{}:{}", datasource_id, event_id);
        self.db.put_cf(
            cf_by_datasource,
            datasource_key.as_bytes(),
            event_id.as_bytes(),
        )?;

        // Index by table
        let cf_by_table = self
            .db
            .cf_handle(CF_EVENTS_BY_TABLE)
            .ok_or_else(|| anyhow!("Column family {} not found", CF_EVENTS_BY_TABLE))?;

        let table_key = format!("{}:{}:{}", datasource_id, table_name, event_id);
        self.db
            .put_cf(cf_by_table, table_key.as_bytes(), event_id.as_bytes())?;

        // Index by time (timestamp:datasource:event_id for time-range queries)
        let cf_by_time = self
            .db
            .cf_handle(CF_EVENTS_BY_TIME)
            .ok_or_else(|| anyhow!("Column family {} not found", CF_EVENTS_BY_TIME))?;

        let time_key = format!("{}:{}:{}", detected_at, datasource_id, event_id);
        self.db
            .put_cf(cf_by_time, time_key.as_bytes(), event_id.as_bytes())?;

        // Index by tenant (for GDPR erasure)
        let cf_by_tenant = self
            .db
            .cf_handle(CF_EVENTS_BY_TENANT)
            .ok_or_else(|| anyhow!("Column family {} not found", CF_EVENTS_BY_TENANT))?;

        let tenant_key = format!("{}:{}", event.tenant_id, event_id);
        self.db
            .put_cf(cf_by_tenant, tenant_key.as_bytes(), event_id.as_bytes())?;

        Ok(())
    }

    /// Record multiple schema change events in batch
    pub fn record_schema_changes_batch(&self, events: Vec<SchemaChangeEvent>) -> Result<()> {
        for event in events {
            self.record_schema_change(event)?;
        }
        Ok(())
    }

    /// Get a specific schema change event by ID
    pub fn get_schema_change_event(&self, event_id: &str) -> Result<Option<SchemaChangeEvent>> {
        let cf = self
            .db
            .cf_handle(CF_SCHEMA_EVENTS)
            .ok_or_else(|| anyhow!("Column family {} not found", CF_SCHEMA_EVENTS))?;

        if let Some(bytes) = self.db.get_cf(cf, event_id.as_bytes())? {
            let versioned = VersionedData::<SchemaChangeEvent>::deserialize(&bytes)?;
            Ok(Some(versioned.unwrap_current()?))
        } else {
            Ok(None)
        }
    }

    /// Get all schema change events for a datasource
    pub fn get_datasource_schema_changes(
        &self,
        datasource_id: &str,
    ) -> Result<Vec<SchemaChangeEvent>> {
        let cf_by_datasource = self
            .db
            .cf_handle(CF_EVENTS_BY_DATASOURCE)
            .ok_or_else(|| anyhow!("Column family {} not found", CF_EVENTS_BY_DATASOURCE))?;

        let cf_events = self
            .db
            .cf_handle(CF_SCHEMA_EVENTS)
            .ok_or_else(|| anyhow!("Column family {} not found", CF_SCHEMA_EVENTS))?;

        let prefix = format!("{}:", datasource_id);
        let iter = self
            .db
            .prefix_iterator_cf(cf_by_datasource, prefix.as_bytes());

        let mut events = Vec::new();
        for item in iter {
            let (_key, event_id_bytes) = item?;
            if let Some(event_bytes) = self.db.get_cf(cf_events, &event_id_bytes)? {
                let versioned = VersionedData::<SchemaChangeEvent>::deserialize(&event_bytes)?;
                events.push(versioned.unwrap_current()?);
            }
        }

        Ok(events)
    }

    /// Get all schema change events for a specific table
    pub fn get_table_schema_changes(
        &self,
        datasource_id: &str,
        table_name: &str,
    ) -> Result<Vec<SchemaChangeEvent>> {
        let cf_by_table = self
            .db
            .cf_handle(CF_EVENTS_BY_TABLE)
            .ok_or_else(|| anyhow!("Column family {} not found", CF_EVENTS_BY_TABLE))?;

        let cf_events = self
            .db
            .cf_handle(CF_SCHEMA_EVENTS)
            .ok_or_else(|| anyhow!("Column family {} not found", CF_SCHEMA_EVENTS))?;

        let prefix = format!("{}:{}:", datasource_id, table_name);
        let prefix_bytes = prefix.as_bytes();
        let iter = self.db.prefix_iterator_cf(cf_by_table, prefix_bytes);

        let mut events = Vec::new();
        for item in iter {
            let (key_bytes, event_id_bytes) = item?;

            // Manually check if the key still matches the prefix
            if !key_bytes.starts_with(prefix_bytes) {
                break; // Stop when we've passed the prefix range
            }

            if let Some(event_bytes) = self.db.get_cf(cf_events, &event_id_bytes)? {
                let versioned = VersionedData::<SchemaChangeEvent>::deserialize(&event_bytes)?;
                events.push(versioned.unwrap_current()?);
            }
        }

        Ok(events)
    }

    /// Get schema change events within a time range
    pub fn get_schema_changes_by_time_range(
        &self,
        start_timestamp: i64,
        end_timestamp: i64,
    ) -> Result<Vec<SchemaChangeEvent>> {
        let cf_by_time = self
            .db
            .cf_handle(CF_EVENTS_BY_TIME)
            .ok_or_else(|| anyhow!("Column family {} not found", CF_EVENTS_BY_TIME))?;

        let cf_events = self
            .db
            .cf_handle(CF_SCHEMA_EVENTS)
            .ok_or_else(|| anyhow!("Column family {} not found", CF_SCHEMA_EVENTS))?;

        let start_key = format!("{}:", start_timestamp);
        let iter = self.db.prefix_iterator_cf(cf_by_time, start_key.as_bytes());

        let mut events = Vec::new();
        for item in iter {
            let (key_bytes, event_id_bytes) = item?;
            let key_str = String::from_utf8_lossy(&key_bytes);

            // Parse timestamp from key (format: "timestamp:datasource:event_id")
            if let Some(timestamp_str) = key_str.split(':').next() {
                if let Ok(timestamp) = timestamp_str.parse::<i64>() {
                    if timestamp > end_timestamp {
                        break; // Passed the end of the range
                    }

                    if let Some(event_bytes) = self.db.get_cf(cf_events, &event_id_bytes)? {
                        let versioned =
                            VersionedData::<SchemaChangeEvent>::deserialize(&event_bytes)?;
                        events.push(versioned.unwrap_current()?);
                    }
                }
            }
        }

        Ok(events)
    }

    /// Get breaking schema changes for a datasource
    pub fn get_breaking_changes(&self, datasource_id: &str) -> Result<Vec<SchemaChangeEvent>> {
        let all_changes = self.get_datasource_schema_changes(datasource_id)?;
        Ok(all_changes.into_iter().filter(|e| e.is_breaking).collect())
    }

    // =============================================================================
    // Schema Version Methods
    // =============================================================================

    /// Save a schema version snapshot
    pub fn save_schema_version(&self, version: SchemaVersion) -> Result<()> {
        let version_id = version.version_id.clone();
        let datasource_id = version.datasource_id.clone();
        let created_at = version.created_at.timestamp_millis();

        // Serialize the version
        let versioned = VersionedData::wrap(version);
        let serialized = versioned.serialize()?;

        // Store in primary CF
        let cf_versions = self
            .db
            .cf_handle(CF_SCHEMA_VERSIONS)
            .ok_or_else(|| anyhow!("Column family {} not found", CF_SCHEMA_VERSIONS))?;
        self.db
            .put_cf(cf_versions, version_id.as_bytes(), &serialized)?;

        // Index by datasource (timestamp:version_id for ordering)
        let cf_by_datasource = self
            .db
            .cf_handle(CF_VERSIONS_BY_DATASOURCE)
            .ok_or_else(|| anyhow!("Column family {} not found", CF_VERSIONS_BY_DATASOURCE))?;

        let datasource_key = format!("{}:{}:{}", datasource_id, created_at, version_id);
        self.db.put_cf(
            cf_by_datasource,
            datasource_key.as_bytes(),
            version_id.as_bytes(),
        )?;

        Ok(())
    }

    /// Get a specific schema version by ID
    pub fn get_schema_version(&self, version_id: &str) -> Result<Option<SchemaVersion>> {
        let cf = self
            .db
            .cf_handle(CF_SCHEMA_VERSIONS)
            .ok_or_else(|| anyhow!("Column family {} not found", CF_SCHEMA_VERSIONS))?;

        if let Some(bytes) = self.db.get_cf(cf, version_id.as_bytes())? {
            let versioned = VersionedData::<SchemaVersion>::deserialize(&bytes)?;
            Ok(Some(versioned.unwrap_current()?))
        } else {
            Ok(None)
        }
    }

    /// Get all schema versions for a datasource
    pub fn get_datasource_schema_versions(
        &self,
        datasource_id: &str,
    ) -> Result<Vec<SchemaVersion>> {
        let cf_by_datasource = self
            .db
            .cf_handle(CF_VERSIONS_BY_DATASOURCE)
            .ok_or_else(|| anyhow!("Column family {} not found", CF_VERSIONS_BY_DATASOURCE))?;

        let cf_versions = self
            .db
            .cf_handle(CF_SCHEMA_VERSIONS)
            .ok_or_else(|| anyhow!("Column family {} not found", CF_SCHEMA_VERSIONS))?;

        let prefix = format!("{}:", datasource_id);
        let iter = self
            .db
            .prefix_iterator_cf(cf_by_datasource, prefix.as_bytes());

        let mut versions = Vec::new();
        for item in iter {
            let (_key, version_id_bytes) = item?;
            if let Some(version_bytes) = self.db.get_cf(cf_versions, &version_id_bytes)? {
                let versioned = VersionedData::<SchemaVersion>::deserialize(&version_bytes)?;
                versions.push(versioned.unwrap_current()?);
            }
        }

        Ok(versions)
    }

    /// Get the latest schema version for a datasource
    pub fn get_latest_schema_version(&self, datasource_id: &str) -> Result<Option<SchemaVersion>> {
        let mut versions = self.get_datasource_schema_versions(datasource_id)?;

        // Sort by created_at descending
        versions.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        Ok(versions.into_iter().next())
    }

    // =============================================================================
    // Analysis Methods
    // =============================================================================

    /// Compare two schema versions and produce drift analysis
    pub fn analyze_schema_drift(
        &self,
        source_version_id: &str,
        target_version_id: &str,
    ) -> Result<SchemaDriftAnalysis> {
        let source_version = self
            .get_schema_version(source_version_id)?
            .ok_or_else(|| anyhow!("Source version {} not found", source_version_id))?;

        let target_version = self
            .get_schema_version(target_version_id)?
            .ok_or_else(|| anyhow!("Target version {} not found", target_version_id))?;

        // Get all changes between these versions
        let start_time = source_version.created_at.timestamp_millis();
        let end_time = target_version.created_at.timestamp_millis();

        let changes = self.get_schema_changes_by_time_range(start_time, end_time)?;

        // Filter changes for this datasource
        let datasource_changes: Vec<SchemaChangeEvent> = changes
            .into_iter()
            .filter(|e| e.datasource_id == source_version.datasource_id)
            .collect();

        // Analyze changes
        let breaking_changes_count = datasource_changes.iter().filter(|e| e.is_breaking).count();
        let non_breaking_changes_count = datasource_changes.len() - breaking_changes_count;

        let mut tables_added = HashSet::new();
        let mut tables_dropped = HashSet::new();
        let mut tables_modified = HashSet::new();

        for change in &datasource_changes {
            match &change.change_type {
                SchemaChangeType::TableAdded => {
                    tables_added.insert(change.table_name.clone());
                }
                SchemaChangeType::TableDropped => {
                    tables_dropped.insert(change.table_name.clone());
                }
                _ => {
                    tables_modified.insert(change.table_name.clone());
                }
            }
        }

        // Determine severity
        let severity = if breaking_changes_count == 0 {
            if non_breaking_changes_count == 0 {
                DriftSeverity::None
            } else if non_breaking_changes_count < 5 {
                DriftSeverity::Low
            } else {
                DriftSeverity::Medium
            }
        } else if breaking_changes_count < 3 {
            DriftSeverity::High
        } else {
            DriftSeverity::Critical
        };

        Ok(SchemaDriftAnalysis {
            source_version_id: source_version_id.to_string(),
            target_version_id: target_version_id.to_string(),
            changes: datasource_changes,
            breaking_changes_count,
            non_breaking_changes_count,
            tables_added: tables_added.into_iter().collect(),
            tables_dropped: tables_dropped.into_iter().collect(),
            tables_modified: tables_modified.into_iter().collect(),
            severity,
            analyzed_at: chrono::Utc::now(),
        })
    }

    /// Analyze the impact of a schema change on downstream systems
    pub fn analyze_migration_impact(
        &self,
        change: &SchemaChangeEvent,
    ) -> Result<MigrationImpactAnalysis> {
        // This is a placeholder implementation
        // In a real system, you would:
        // 1. Query workflow/job metadata to find affected pipelines
        // 2. Parse query logs to find affected queries
        // 3. Check dashboard/report metadata for affected visualizations

        let risk_level = if change.is_breaking {
            if matches!(
                change.change_type,
                SchemaChangeType::TableDropped | SchemaChangeType::ColumnDropped
            ) {
                RiskLevel::Critical
            } else {
                RiskLevel::High
            }
        } else {
            RiskLevel::Low
        };

        Ok(MigrationImpactAnalysis {
            change: change.clone(),
            affected_queries: Vec::new(), // Would be populated from query logs
            affected_jobs: Vec::new(),    // Would be populated from job metadata
            affected_workflows: Vec::new(), // Would be populated from workflow metadata
            affected_dashboards: Vec::new(), // Would be populated from dashboard metadata
            impact_score: 0.0,            // Would be calculated based on affected systems
            migration_steps: vec![
                "Review affected queries and update them".to_string(),
                "Update downstream ETL jobs".to_string(),
                "Test changes in staging environment".to_string(),
                "Deploy schema migration".to_string(),
            ],
            risk_level,
        })
    }

    /// Search for schema changes by table pattern (supports wildcards)
    pub fn search_schema_changes(&self, pattern: &str) -> Result<Vec<SchemaChangeEvent>> {
        // For simplicity, we'll scan all events and filter
        // In production, you'd want more efficient indexing
        let cf_events = self
            .db
            .cf_handle(CF_SCHEMA_EVENTS)
            .ok_or_else(|| anyhow!("Column family {} not found", CF_SCHEMA_EVENTS))?;

        let iter = self.db.iterator_cf(cf_events, rocksdb::IteratorMode::Start);

        let mut matching_events = Vec::new();
        for item in iter {
            let (_key, value) = item?;
            let versioned = VersionedData::<SchemaChangeEvent>::deserialize(&value)?;
            let event = versioned.unwrap_current()?;

            // Simple pattern matching (could be enhanced with regex)
            if event.table_name.contains(pattern)
                || event.datasource_id.contains(pattern)
                || event
                    .column_name
                    .as_ref()
                    .map_or(false, |c| c.contains(pattern))
            {
                matching_events.push(event);
            }
        }

        Ok(matching_events)
    }

    /// Get all event IDs for a specific tenant (for GDPR erasure)
    pub fn get_tenant_event_ids(&self, tenant_id: &str) -> Result<Vec<String>> {
        let cf_by_tenant = self
            .db
            .cf_handle(CF_EVENTS_BY_TENANT)
            .ok_or_else(|| anyhow!("Column family {} not found", CF_EVENTS_BY_TENANT))?;

        let prefix = format!("{}:", tenant_id);
        let iter = self.db.prefix_iterator_cf(cf_by_tenant, prefix.as_bytes());

        let mut event_ids = Vec::new();
        for item in iter {
            let (key, value) = item?;
            let key_str = String::from_utf8_lossy(&key);

            // Only process keys that exactly match our tenant prefix
            if !key_str.starts_with(&prefix) {
                break;
            }

            let event_id = String::from_utf8_lossy(&value).to_string();
            event_ids.push(event_id);
        }

        Ok(event_ids)
    }

    /// Count total events for a specific tenant (for GDPR reporting)
    pub fn count_tenant_events(&self, tenant_id: &str) -> Result<u64> {
        let event_ids = self.get_tenant_event_ids(tenant_id)?;
        Ok(event_ids.len() as u64)
    }

    /// Get all event IDs associated with a specific user.
    ///
    /// User affinity is determined by `initiated_by`, with `tenant_id` as a
    /// compatibility fallback for deployments that reused tenant identifiers.
    pub fn get_user_event_ids(&self, user_id: &str) -> Result<Vec<String>> {
        let cf_events = self
            .db
            .cf_handle(CF_SCHEMA_EVENTS)
            .ok_or_else(|| anyhow!("Column family {} not found", CF_SCHEMA_EVENTS))?;

        let iter = self.db.iterator_cf(cf_events, rocksdb::IteratorMode::Start);
        let mut event_ids = Vec::new();

        for item in iter {
            let (_key, value) = item?;
            let versioned = VersionedData::<SchemaChangeEvent>::deserialize(&value)?;
            let event = versioned.unwrap_current()?;

            if event.initiated_by == user_id || event.tenant_id == user_id {
                event_ids.push(event.id.clone());
            }
        }

        Ok(event_ids)
    }

    /// Count total events associated with a specific user.
    pub fn count_user_events(&self, user_id: &str) -> Result<u64> {
        let event_ids = self.get_user_event_ids(user_id)?;
        Ok(event_ids.len() as u64)
    }

    fn erase_user_events(&self, user_id: &str, dry_run: bool) -> Result<u64> {
        let event_ids = self.get_user_event_ids(user_id)?;
        let total_events = event_ids.len() as u64;

        if dry_run || total_events == 0 {
            return Ok(total_events);
        }

        let event_id_set: HashSet<String> = event_ids.into_iter().collect();

        let cf_events = self
            .db
            .cf_handle(CF_SCHEMA_EVENTS)
            .ok_or_else(|| anyhow!("Column family {} not found", CF_SCHEMA_EVENTS))?;
        let cf_by_tenant = self
            .db
            .cf_handle(CF_EVENTS_BY_TENANT)
            .ok_or_else(|| anyhow!("Column family {} not found", CF_EVENTS_BY_TENANT))?;

        let mut batch = WriteBatch::default();

        for event_id in &event_id_set {
            batch.delete_cf(cf_events, event_id.as_bytes());
        }

        // Remove tenant index entries that reference the removed event IDs.
        let iter = self
            .db
            .iterator_cf(cf_by_tenant, rocksdb::IteratorMode::Start);
        for item in iter {
            let (key, value) = item?;
            let indexed_event_id = String::from_utf8_lossy(&value).to_string();
            if event_id_set.contains(&indexed_event_id) {
                batch.delete_cf(cf_by_tenant, key);
            }
        }

        self.db.write(batch)?;
        Ok(total_events)
    }
}

/// GDPR Data Erasure Implementation
///
/// Implements GDPR Article 17 (Right to Erasure) for schema evolution data.
#[async_trait]
impl DataErasure for SchemaEvolutionStore {
    async fn erase_data_subject(&self, request: &ErasureRequest) -> Result<ErasureResult> {
        let mut result = ErasureResult::new(request.clone(), uuid::Uuid::new_v4().to_string());

        if request.data_subject.id_type == "user_id" {
            let user_id = &request.data_subject.id;

            if request.dry_run {
                let count = self.count_user_events(user_id)?;
                result.add_backend_result(
                    "schema_evolution".to_string(),
                    BackendErasureResult::success(
                        "schema_evolution",
                        0,
                        ErasureStrategy::HardDelete,
                    )
                    .with_detail("schema_change_events_would_be_deleted", count)
                    .with_warning(format!(
                        "Dry run - no data was actually deleted. Would have deleted {} records.",
                        count
                    )),
                );
                return Ok(result.finalize());
            }

            let deleted = self.erase_user_events(user_id, false)?;
            result.add_backend_result(
                "schema_evolution".to_string(),
                BackendErasureResult::success(
                    "schema_evolution",
                    deleted,
                    ErasureStrategy::HardDelete,
                )
                .with_detail("schema_change_events", deleted),
            );
            return Ok(result.finalize());
        }

        if request.data_subject.id_type != "tenant_id" {
            result.add_backend_result(
                "schema_evolution".to_string(),
                BackendErasureResult::failure(
                    "schema_evolution",
                    format!(
                        "Unsupported data subject type: {}. Supported types: 'tenant_id', 'user_id'.",
                        request.data_subject.id_type
                    ),
                    ErasureStrategy::HardDelete,
                ),
            );
            return Ok(result.finalize());
        }

        let tenant_id = &request.data_subject.id;

        // Dry run: just count records
        if request.dry_run {
            let count = self.count_tenant_events(tenant_id)?;
            result.add_backend_result(
                "schema_evolution".to_string(),
                BackendErasureResult::success("schema_evolution", 0, ErasureStrategy::HardDelete)
                    .with_detail("schema_change_events_would_be_deleted", count)
                    .with_warning(format!(
                        "Dry run - no data was actually deleted. Would have deleted {} records.",
                        count
                    )),
            );
            return Ok(result.finalize());
        }

        // Get all event IDs for this tenant
        let event_ids = self.get_tenant_event_ids(tenant_id)?;
        let total_events = event_ids.len() as u64;

        if total_events == 0 {
            result.add_backend_result(
                "schema_evolution".to_string(),
                BackendErasureResult::success("schema_evolution", 0, ErasureStrategy::HardDelete)
                    .with_warning("No data found for tenant"),
            );
            return Ok(result.finalize());
        }

        // Hard delete all data for this tenant
        let cf_events = self
            .db
            .cf_handle(CF_SCHEMA_EVENTS)
            .ok_or_else(|| anyhow!("Column family {} not found", CF_SCHEMA_EVENTS))?;
        let cf_by_tenant = self
            .db
            .cf_handle(CF_EVENTS_BY_TENANT)
            .ok_or_else(|| anyhow!("Column family {} not found", CF_EVENTS_BY_TENANT))?;

        let mut batch = WriteBatch::default();
        let mut records_deleted = 0u64;

        // Delete schema change events
        for event_id in &event_ids {
            batch.delete_cf(cf_events, event_id.as_bytes());
            records_deleted += 1;
        }

        // Delete tenant index entries
        let prefix = format!("{}:", tenant_id);
        let iter = self.db.prefix_iterator_cf(cf_by_tenant, prefix.as_bytes());
        for item in iter {
            let (key, _value) = item?;
            let key_str = String::from_utf8_lossy(&key);

            // Only delete keys that exactly match our tenant prefix
            if !key_str.starts_with(&prefix) {
                break;
            }

            batch.delete_cf(cf_by_tenant, &key);
        }

        // Write batch
        self.db.write(batch)?;

        result.add_backend_result(
            "schema_evolution".to_string(),
            BackendErasureResult::success(
                "schema_evolution",
                records_deleted,
                ErasureStrategy::HardDelete,
            )
            .with_detail("schema_change_events", records_deleted)
            .with_detail("tenant_index_entries", total_events),
        );

        Ok(result.finalize())
    }

    async fn count_data_subject_records(&self, data_subject: &DataSubjectId) -> Result<u64> {
        match data_subject.id_type.as_str() {
            "tenant_id" => self.count_tenant_events(&data_subject.id),
            "user_id" => self.count_user_events(&data_subject.id),
            _ => anyhow::bail!(
                "Unsupported data subject type: {}. Supported types: 'tenant_id', 'user_id'.",
                data_subject.id_type
            ),
        }
    }

    async fn get_data_breakdown(
        &self,
        data_subject: &DataSubjectId,
    ) -> Result<std::collections::HashMap<String, u64>> {
        let count = match data_subject.id_type.as_str() {
            "tenant_id" => self.count_tenant_events(&data_subject.id)?,
            "user_id" => self.count_user_events(&data_subject.id)?,
            _ => {
                anyhow::bail!(
                    "Unsupported data subject type: {}. Supported types: 'tenant_id', 'user_id'.",
                    data_subject.id_type
                )
            }
        };
        let mut breakdown = std::collections::HashMap::new();
        breakdown.insert("schema_change_events".to_string(), count);
        Ok(breakdown)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use graphica_core::core::lineage::schema_evolution::SchemaElement;
    use tempfile::TempDir;

    fn create_test_store() -> (SchemaEvolutionStore, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let store = SchemaEvolutionStore::open(temp_dir.path()).unwrap();
        (store, temp_dir)
    }

    #[test]
    fn test_record_and_retrieve_schema_change() {
        let (store, _temp_dir) = create_test_store();

        // Verify GDPR tenant index column family exists
        assert!(store.db.cf_handle(CF_EVENTS_BY_TENANT).is_some());

        let event = SchemaChangeEvent::new(
            "postgres-prod",
            "customers",
            SchemaChangeType::ColumnAdded,
            "migration-script",
            "tenant-1",
        )
        .with_column("email")
        .with_after_state(SchemaElement::column("email", "VARCHAR(255)", false));

        store.record_schema_change(event.clone()).unwrap();

        // Retrieve by event ID
        let retrieved = store.get_schema_change_event(&event.id).unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().id, event.id);
    }

    #[test]
    fn test_get_datasource_schema_changes() {
        let (store, _temp_dir) = create_test_store();

        // Record multiple events for the same datasource
        for i in 0..3 {
            let event = SchemaChangeEvent::new(
                "postgres-prod",
                format!("table_{}", i),
                SchemaChangeType::TableAdded,
                "system",
                "tenant-1",
            );
            store.record_schema_change(event).unwrap();
        }

        let events = store
            .get_datasource_schema_changes("postgres-prod")
            .unwrap();
        assert_eq!(events.len(), 3);
    }

    #[test]
    fn test_get_table_schema_changes() {
        let (store, _temp_dir) = create_test_store();

        // Record events for different tables
        let event1 = SchemaChangeEvent::new(
            "postgres-prod",
            "customers",
            SchemaChangeType::ColumnAdded,
            "system",
            "tenant-1",
        );
        let event2 = SchemaChangeEvent::new(
            "postgres-prod",
            "customers",
            SchemaChangeType::ColumnDropped,
            "system",
            "tenant-1",
        );
        let event3 = SchemaChangeEvent::new(
            "postgres-prod",
            "orders",
            SchemaChangeType::TableAdded,
            "system",
            "tenant-1",
        );

        store.record_schema_change(event1).unwrap();
        store.record_schema_change(event2).unwrap();
        store.record_schema_change(event3).unwrap();

        let customer_events = store
            .get_table_schema_changes("postgres-prod", "customers")
            .unwrap();
        assert_eq!(customer_events.len(), 2);

        let order_events = store
            .get_table_schema_changes("postgres-prod", "orders")
            .unwrap();
        assert_eq!(order_events.len(), 1);
    }

    #[test]
    fn test_schema_version_storage() {
        let (store, _temp_dir) = create_test_store();

        let version = SchemaVersion {
            version_id: "v1".to_string(),
            datasource_id: "postgres-prod".to_string(),
            schema_name: Some("public".to_string()),
            created_at: Utc::now(),
            migration_id: Some("migration-001".to_string()),
            tables: vec![],
            previous_version: None,
            git_commit: None,
            tags: vec!["production".to_string()],
        };

        store.save_schema_version(version.clone()).unwrap();

        let retrieved = store.get_schema_version("v1").unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().version_id, "v1");
    }

    #[test]
    fn test_get_latest_schema_version() {
        let (store, _temp_dir) = create_test_store();

        // Create versions with different timestamps
        for i in 1..=3 {
            let version = SchemaVersion {
                version_id: format!("v{}", i),
                datasource_id: "postgres-prod".to_string(),
                schema_name: None,
                created_at: Utc::now() + chrono::Duration::seconds(i),
                migration_id: None,
                tables: vec![],
                previous_version: if i > 1 {
                    Some(format!("v{}", i - 1))
                } else {
                    None
                },
                git_commit: None,
                tags: vec![],
            };
            store.save_schema_version(version).unwrap();
        }

        let latest = store.get_latest_schema_version("postgres-prod").unwrap();
        assert!(latest.is_some());
        assert_eq!(latest.unwrap().version_id, "v3");
    }

    #[test]
    fn test_get_breaking_changes() {
        let (store, _temp_dir) = create_test_store();

        // Breaking change
        let event1 = SchemaChangeEvent::new(
            "postgres-prod",
            "customers",
            SchemaChangeType::ColumnDropped,
            "system",
            "tenant-1",
        );

        // Non-breaking change
        let event2 = SchemaChangeEvent::new(
            "postgres-prod",
            "customers",
            SchemaChangeType::ColumnAdded,
            "system",
            "tenant-1",
        );

        store.record_schema_change(event1).unwrap();
        store.record_schema_change(event2).unwrap();

        let breaking = store.get_breaking_changes("postgres-prod").unwrap();
        assert_eq!(breaking.len(), 1);
        assert!(breaking[0].is_breaking);
    }

    #[test]
    fn test_search_schema_changes() {
        let (store, _temp_dir) = create_test_store();

        let event1 = SchemaChangeEvent::new(
            "postgres-prod",
            "user_profiles",
            SchemaChangeType::TableAdded,
            "system",
            "tenant-1",
        );

        let event2 = SchemaChangeEvent::new(
            "postgres-prod",
            "customer_data",
            SchemaChangeType::TableAdded,
            "system",
            "tenant-1",
        );

        store.record_schema_change(event1).unwrap();
        store.record_schema_change(event2).unwrap();

        let results = store.search_schema_changes("user").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].table_name, "user_profiles");
    }
}
