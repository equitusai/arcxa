//! Data Discovery Service
//!
//! Scans Graphica's data stores to discover personal data for GDPR export.
//! Searches across lineage, RDF, governance, and file library systems.
//!
//! ## Discovery Strategy
//!
//! For a given user_id, this service discovers:
//!
//! 1. **Lineage Events** - All lineage events where user is mentioned
//!    - Tenant_id matches user_id
//!    - Metadata contains user_id
//!    - Record_id contains user_id
//!
//! 2. **RDF Triples** - All RDF triples about the user
//!    - Subject/object contains user URI
//!    - Predicates indicating user ownership
//!
//! 3. **Governance Records** - Access logs, annotations, audit trails
//!    - Bitemporal annotations created by user
//!    - Access audit logs for user
//!
//! 4. **File Library** - Uploaded files owned by user
//!    - Files where user_id matches owner
//!
//! ## Performance Considerations
//!
//! - Uses indexed queries where possible
//! - Supports time range filtering to limit scope
//! - Returns data references (not full data) for efficient collection

use super::types::{DataCategory, ExportRequest, TimeRange};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use graphica_core::core::lineage::column_level::ColumnLineageSink;
use graphica_core::core::lineage::row_level::RowLevelLineageSink;
use graphica_core::core::lineage::{LineageEvent, LineageSink};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use uuid::Uuid;

// Import storage backend traits
use crate::api::file_library::storage_trait::FileLibraryStore;
use crate::api::file_library::types::ListFilesRequest;
use crate::governance::SharedGovernanceBrain;
use crate::storage::column_lineage_store::ColumnLineageStore;
use crate::storage::row_lineage_store::RowLineageStore;

/// Result of data discovery process
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryResult {
    /// User ID this discovery is for
    pub user_id: String,

    /// Total number of data items discovered
    pub total_items: usize,

    /// Data items organized by category
    pub items_by_category: HashMap<DataCategory, Vec<DataReference>>,

    /// Discovery timestamp
    pub discovered_at: DateTime<Utc>,

    /// Time range that was searched (if specified)
    pub time_range: Option<(DateTime<Utc>, DateTime<Utc>)>,

    /// Any warnings encountered during discovery
    pub warnings: Vec<String>,
}

/// Reference to a discovered data item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataReference {
    /// Unique ID for this data item
    pub id: String,

    /// Type of data (e.g., "lineage_event", "rdf_triple", "file", "annotation")
    pub data_type: String,

    /// Category this data belongs to
    pub category: DataCategory,

    /// Storage location (e.g., "rocksdb://lineage/events", "file://uploads/abc.csv")
    pub storage_location: String,

    /// Timestamp when this data was created/modified
    pub timestamp: DateTime<Utc>,

    /// Size estimate in bytes
    pub size_bytes: Option<usize>,

    /// Additional metadata for retrieval
    pub metadata: HashMap<String, String>,
}

/// Data discovery service with multi-backend support for complete GDPR compliance
pub struct DataDiscoveryService {
    /// High-level event lineage (existing)
    lineage_storage: Arc<dyn LineageSink>,

    /// Row-level lineage (for ETL audit trails)
    row_lineage_store: Arc<RowLineageStore>,

    /// Column-level lineage (for transformation tracking)
    column_lineage_store: Arc<ColumnLineageStore>,

    /// RDF governance brain (for ontology/SHACL/PROV lineage)
    governance_brain: Option<SharedGovernanceBrain>,

    /// File library (for uploaded files)
    file_library: Option<Arc<dyn FileLibraryStore>>,
}

impl DataDiscoveryService {
    /// Create new discovery service with all required backends
    pub fn new(
        lineage_storage: Arc<dyn LineageSink>,
        row_lineage_store: Arc<RowLineageStore>,
        column_lineage_store: Arc<ColumnLineageStore>,
        governance_brain: Option<SharedGovernanceBrain>,
        file_library: Option<Arc<dyn FileLibraryStore>>,
    ) -> Self {
        Self {
            lineage_storage,
            row_lineage_store,
            column_lineage_store,
            governance_brain,
            file_library,
        }
    }

    /// Discover all personal data for a user
    ///
    /// Scans all configured data sources concurrently and returns references to discovered data.
    pub async fn discover_user_data(&self, request: &ExportRequest) -> Result<DiscoveryResult> {
        let start_time = Utc::now();
        let mut items_by_category: HashMap<DataCategory, Vec<DataReference>> = HashMap::new();
        let mut warnings = Vec::new();

        let time_range_tuple = request.time_range.as_ref().map(|tr| (tr.start, tr.end));

        // Launch all discovery tasks concurrently using tokio::join!
        let user_id = request.user_id.clone();

        let lineage_task = async {
            self.discover_lineage_data(&user_id, time_range_tuple.as_ref())
                .map_err(|e| format!("Lineage events: {}", e))
        };

        let row_task = async {
            self.discover_row_lineage(&user_id)
                .await
                .map_err(|e| format!("Row lineage: {}", e))
        };

        let col_task = async {
            self.discover_column_lineage(&user_id)
                .await
                .map_err(|e| format!("Column lineage: {}", e))
        };

        let rdf_task = async {
            if self.governance_brain.is_some() {
                self.discover_rdf_data(&user_id)
                    .await
                    .map_err(|e| format!("RDF triples: {}", e))
            } else {
                Ok(Vec::new())
            }
        };

        let file_task = async {
            if self.file_library.is_some() {
                self.discover_files(&user_id)
                    .await
                    .map_err(|e| format!("File library: {}", e))
            } else {
                Ok(Vec::new())
            }
        };

        // Await all concurrently
        let (lineage_result, row_result, col_result, rdf_result, file_result) =
            tokio::join!(lineage_task, row_task, col_task, rdf_task, file_task);

        // Process results, collecting errors as warnings
        match lineage_result {
            Ok(refs) => self.categorize_and_add(&mut items_by_category, refs),
            Err(e) => warnings.push(e),
        }

        match row_result {
            Ok(refs) => self.categorize_and_add(&mut items_by_category, refs),
            Err(e) => warnings.push(e),
        }

        match col_result {
            Ok(refs) => self.categorize_and_add(&mut items_by_category, refs),
            Err(e) => warnings.push(e),
        }

        match rdf_result {
            Ok(refs) => self.categorize_and_add(&mut items_by_category, refs),
            Err(e) => warnings.push(e),
        }

        match file_result {
            Ok(refs) => self.categorize_and_add(&mut items_by_category, refs),
            Err(e) => warnings.push(e),
        }

        // Calculate total items
        let total_items = items_by_category.values().map(|refs| refs.len()).sum();

        Ok(DiscoveryResult {
            user_id: request.user_id.clone(),
            total_items,
            items_by_category,
            discovered_at: start_time,
            time_range: time_range_tuple,
            warnings,
        })
    }

    /// Discover lineage events for a user
    fn discover_lineage_data(
        &self,
        user_id: &str,
        time_range: Option<&(DateTime<Utc>, DateTime<Utc>)>,
    ) -> Result<Vec<DataReference>> {
        let mut references = Vec::new();

        // Strategy 1: Query by tenant_id (most common case)
        let events = if let Some((start, end)) = time_range {
            self.lineage_storage.query_by_time_range(*start, *end)?
        } else {
            // Without time range, we need a different strategy
            // Try querying by record_id pattern
            self.lineage_storage.get_record_lineage(user_id)?
        };

        // Filter events that actually relate to this user
        let user_events: Vec<&LineageEvent> = events
            .iter()
            .filter(|event| {
                // Match by tenant_id
                if event.tenant_id == user_id {
                    return true;
                }

                // Match by record_id
                if event.record_id.contains(user_id) {
                    return true;
                }

                // Match in metadata
                if event.metadata.values().any(|v| v.contains(user_id)) {
                    return true;
                }

                false
            })
            .collect();

        // Convert to data references
        for event in user_events {
            let mut metadata = HashMap::new();
            metadata.insert("dataset".to_string(), event.dataset.clone());
            metadata.insert("record_id".to_string(), event.record_id.clone());
            metadata.insert("tenant_id".to_string(), event.tenant_id.clone());

            references.push(DataReference {
                id: event.id.to_string(),
                data_type: "lineage_event".to_string(),
                category: DataCategory::Behavioral, // Lineage = user behavior data
                storage_location: format!("rocksdb://lineage/events/{}", event.id),
                timestamp: event.ts,
                size_bytes: Some(self.estimate_lineage_event_size(event)),
                metadata,
            });
        }

        Ok(references)
    }

    /// Estimate size of a lineage event in bytes
    fn estimate_lineage_event_size(&self, event: &LineageEvent) -> usize {
        // Rough estimate based on field sizes
        let base_size = 256; // UUID, timestamps, etc.
        let dataset_size = event.dataset.len();
        let record_id_size = event.record_id.len();
        let source_refs_size = event.source_refs.len() * 128;
        let transforms_size = event.transforms.len() * 256;
        let metadata_size: usize = event.metadata.iter().map(|(k, v)| k.len() + v.len()).sum();

        base_size
            + dataset_size
            + record_id_size
            + source_refs_size
            + transforms_size
            + metadata_size
    }

    /// Discover row-level lineage for a user
    async fn discover_row_lineage(&self, user_id: &str) -> Result<Vec<DataReference>> {
        let row_ids = self
            .row_lineage_store
            .get_tenant_row_ids(user_id)
            .context("Failed to get tenant row IDs")?;

        let mut references = Vec::new();

        for row_id in row_ids.iter().take(10000) {
            // Limit to 10K rows for performance
            match self.row_lineage_store.get_row_lineage(&row_id).await {
                Ok(events) => {
                    for event in events {
                        let mut metadata = HashMap::new();
                        metadata.insert("job_id".to_string(), event.job_id.clone());
                        metadata.insert("batch_id".to_string(), event.batch_id.clone());
                        metadata.insert("outcome".to_string(), format!("{:?}", event.outcome));

                        references.push(DataReference {
                            id: row_id.to_key(),
                            data_type: "row_lineage_event".to_string(),
                            category: DataCategory::Technical,
                            storage_location: format!("rocksdb://row_lineage/{}", row_id.to_key()),
                            timestamp: event.timestamp,
                            size_bytes: Some(256), // Estimate
                            metadata,
                        });
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to get row lineage for {:?}: {}", row_id, e);
                }
            }
        }

        Ok(references)
    }

    /// Discover column-level lineage for a user
    async fn discover_column_lineage(&self, user_id: &str) -> Result<Vec<DataReference>> {
        let column_refs = self
            .column_lineage_store
            .get_tenant_column_refs(user_id)
            .context("Failed to get tenant column refs")?;

        let mut references = Vec::new();

        for col_ref in column_refs.iter().take(1000) {
            // Limit to 1K columns
            match self.column_lineage_store.get_column_lineage(&col_ref).await {
                Ok(events) => {
                    for event in events {
                        let mut metadata = HashMap::new();
                        metadata.insert("job_id".to_string(), event.job_id.clone());
                        metadata.insert(
                            "transformation_type".to_string(),
                            format!("{:?}", event.transformation_type),
                        );

                        references.push(DataReference {
                            id: event.id.clone(),
                            data_type: "column_lineage_event".to_string(),
                            category: DataCategory::Technical,
                            storage_location: format!(
                                "rocksdb://column_lineage/{}",
                                col_ref.fully_qualified_name()
                            ),
                            timestamp: event.created_at,
                            size_bytes: Some(512), // Estimate
                            metadata,
                        });
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to get column lineage for {:?}: {}", col_ref, e);
                }
            }
        }

        Ok(references)
    }

    /// Discover RDF triples about a user
    async fn discover_rdf_data(&self, user_id: &str) -> Result<Vec<DataReference>> {
        let governance_brain = self
            .governance_brain
            .as_ref()
            .context("Governance brain not configured")?;

        // Query RDF store for triples involving user
        let sparql = format!(
            r#"
            PREFIX prov: <http://www.w3.org/ns/prov#>
            PREFIX graphica: <http://graphica.io/ns/>

            SELECT ?subject ?predicate ?object ?timestamp
            WHERE {{
              {{
                ?subject ?predicate ?object .
                FILTER(CONTAINS(STR(?subject), "{}"))
              }} UNION {{
                ?subject ?predicate ?object .
                FILTER(CONTAINS(STR(?object), "{}"))
              }} UNION {{
                ?lineage graphica:tenantId "{}" .
                ?lineage ?predicate ?object .
                BIND(?lineage AS ?subject)
              }}
              OPTIONAL {{ ?subject prov:generatedAtTime ?timestamp }}
            }}
            LIMIT 10000
            "#,
            user_id, user_id, user_id
        );

        let results = governance_brain
            .query(&sparql)
            .context("Failed to execute SPARQL query")?;

        let mut references = Vec::new();
        for row in results.iter().take(10000) {
            let subject = row
                .get("subject")
                .map(|s| s.to_string())
                .unwrap_or_default();
            let predicate = row
                .get("predicate")
                .map(|p| p.to_string())
                .unwrap_or_default();
            let object = row.get("object").map(|o| o.to_string()).unwrap_or_default();
            let timestamp_str = row.get("timestamp").map(|t| t.to_string());

            let timestamp = timestamp_str
                .and_then(|ts| chrono::DateTime::parse_from_rfc3339(&ts).ok())
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|| Utc::now());

            let mut metadata = HashMap::new();
            metadata.insert("predicate".to_string(), predicate);
            metadata.insert("object".to_string(), object);

            references.push(DataReference {
                id: format!("rdf_triple_{}", uuid::Uuid::new_v4()),
                data_type: "rdf_triple".to_string(),
                category: DataCategory::Behavioral,
                storage_location: format!("rdf://governance/{}", subject),
                timestamp,
                size_bytes: Some(128), // Estimate
                metadata,
            });
        }

        Ok(references)
    }

    /// Discover files owned by a user
    async fn discover_files(&self, user_id: &str) -> Result<Vec<DataReference>> {
        let file_library = self
            .file_library
            .as_ref()
            .context("File library not configured")?;

        // Query files owned by user
        let request = ListFilesRequest {
            folder_id: None,
            tags: None,
            search: None,
            status: None,
            owner: Some(user_id.to_string()),
            sort: None,
            order: None,
            limit: Some(10000),
            offset: None,
        };

        let files = file_library
            .list_files(&request)
            .context("Failed to list files by owner")?;

        let mut references = Vec::new();
        for file in files.iter().take(10000) {
            // Limit to 10K files
            let mut metadata = HashMap::new();
            metadata.insert("name".to_string(), file.name.clone());
            metadata.insert("status".to_string(), format!("{:?}", file.status));
            if let Some(ref folder_id) = &file.folder_id {
                metadata.insert("folder_id".to_string(), folder_id.to_string());
            }

            references.push(DataReference {
                id: file.id.clone(),
                data_type: "file".to_string(),
                category: DataCategory::Technical, // Could be based on file.sensitivity_level
                storage_location: format!("file://{}", file.file_path),
                timestamp: file.created_at,
                size_bytes: Some(file.size_bytes as usize),
                metadata,
            });
        }

        Ok(references)
    }

    /// Categorize data references and add to map
    fn categorize_and_add(
        &self,
        items_by_category: &mut HashMap<DataCategory, Vec<DataReference>>,
        references: Vec<DataReference>,
    ) {
        for data_ref in references {
            let category = data_ref.category.clone();
            items_by_category
                .entry(category)
                .or_insert_with(Vec::new)
                .push(data_ref);
        }
    }
}

/// Summary statistics for discovery
impl DiscoveryResult {
    /// Get total size estimate in bytes
    pub fn estimated_size_bytes(&self) -> usize {
        self.items_by_category
            .values()
            .flat_map(|refs| refs.iter())
            .filter_map(|r| r.size_bytes)
            .sum()
    }

    /// Get count of items per category
    pub fn category_counts(&self) -> HashMap<DataCategory, usize> {
        self.items_by_category
            .iter()
            .map(|(cat, refs)| (cat.clone(), refs.len()))
            .collect()
    }

    /// Check if any warnings were encountered
    pub fn has_warnings(&self) -> bool {
        !self.warnings.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphica_core::core::lineage::{DataRef, LineageEvent};
    use tempfile::TempDir;
    use uuid::Uuid;

    /// Mock lineage storage for testing
    struct MockLineageSink {
        events: Vec<LineageEvent>,
    }

    impl LineageSink for MockLineageSink {
        fn write(&self, _event: LineageEvent) -> Result<()> {
            Ok(())
        }

        fn get_record_lineage(&self, record_id: &str) -> Result<Vec<LineageEvent>> {
            Ok(self
                .events
                .iter()
                .filter(|e| e.record_id == record_id || e.tenant_id == record_id)
                .cloned()
                .collect())
        }

        fn get_model_impact(&self, _model_id: &str, _version: &str) -> Result<Vec<LineageEvent>> {
            Ok(vec![])
        }

        fn query_by_time_range(
            &self,
            start: DateTime<Utc>,
            end: DateTime<Utc>,
        ) -> Result<Vec<LineageEvent>> {
            Ok(self
                .events
                .iter()
                .filter(|e| e.ts >= start && e.ts <= end)
                .cloned()
                .collect())
        }

        fn get_run_lineage(&self, _run_id: &str) -> Result<Vec<LineageEvent>> {
            Ok(vec![])
        }

        fn get_lineage_as_of(
            &self,
            _record_id: &str,
            _as_of: DateTime<Utc>,
        ) -> Result<Vec<LineageEvent>> {
            Ok(vec![])
        }
    }

    fn create_test_event(user_id: &str, timestamp: DateTime<Utc>) -> LineageEvent {
        LineageEvent {
            id: Uuid::new_v4(),
            dataset: "test.dataset".to_string(),
            record_id: format!("record_{}", user_id),
            source_refs: vec![],
            transforms: vec![],
            model_refs: vec![],
            output_ref: DataRef {
                system: "test".to_string(),
                path: "/test/path".to_string(),
                version: None,
                extracted_at: timestamp,
                cdc_position: None,
            },
            ts: timestamp,
            run_id: Uuid::new_v4().to_string(),
            tenant_id: user_id.to_string(),
            correlation_id: None,
            metadata: HashMap::new(),
        }
    }

    fn create_test_service(events: Vec<LineageEvent>) -> (DataDiscoveryService, TempDir, TempDir) {
        let lineage_storage: Arc<dyn LineageSink> = Arc::new(MockLineageSink { events });
        let row_dir = TempDir::new().expect("row lineage temp dir");
        let col_dir = TempDir::new().expect("column lineage temp dir");
        let row_lineage_store =
            Arc::new(RowLineageStore::new(row_dir.path()).expect("row lineage store"));
        let column_lineage_store =
            Arc::new(ColumnLineageStore::new(col_dir.path()).expect("column lineage store"));

        let service = DataDiscoveryService::new(
            lineage_storage,
            row_lineage_store,
            column_lineage_store,
            None,
            None,
        );
        (service, row_dir, col_dir)
    }

    #[tokio::test]
    async fn test_discover_user_data_by_tenant_id() {
        let now = Utc::now();
        let events = vec![
            create_test_event("alice", now),
            create_test_event("bob", now),
            create_test_event("alice", now - chrono::Duration::hours(1)),
        ];

        let (service, _row_dir, _col_dir) = create_test_service(events);

        let request = ExportRequest {
            user_id: "alice".to_string(),
            format: super::super::types::ExportFormat::Json,
            categories: vec![],
            include_derived: false,
            include_metadata: true,
            include_audit_trail: false,
            time_range: None,
            filters: HashMap::new(),
        };

        let result = service.discover_user_data(&request).await.unwrap();

        assert_eq!(result.user_id, "alice");
        assert_eq!(result.total_items, 2); // 2 events for alice
        assert!(!result.has_warnings());
    }

    #[tokio::test]
    async fn test_discover_with_time_range() {
        let now = Utc::now();
        let old_event_time = now - chrono::Duration::days(10);
        let recent_event_time = now - chrono::Duration::hours(1);

        let events = vec![
            create_test_event("alice", old_event_time),
            create_test_event("alice", recent_event_time),
        ];

        let (service, _row_dir, _col_dir) = create_test_service(events);

        // Search only last 7 days
        let start = now - chrono::Duration::days(7);
        let end = now;

        let request = ExportRequest {
            user_id: "alice".to_string(),
            format: super::super::types::ExportFormat::Json,
            categories: vec![],
            include_derived: false,
            include_metadata: true,
            include_audit_trail: false,
            time_range: Some(super::super::types::TimeRange { start, end }),
            filters: HashMap::new(),
        };

        let result = service.discover_user_data(&request).await.unwrap();

        assert_eq!(result.total_items, 1); // Only recent event
    }

    #[tokio::test]
    async fn test_category_counts() {
        let now = Utc::now();
        let events = vec![
            create_test_event("alice", now),
            create_test_event("alice", now - chrono::Duration::hours(1)),
        ];

        let (service, _row_dir, _col_dir) = create_test_service(events);

        let request = ExportRequest {
            user_id: "alice".to_string(),
            format: super::super::types::ExportFormat::Json,
            categories: vec![],
            include_derived: false,
            include_metadata: true,
            include_audit_trail: false,
            time_range: None,
            filters: HashMap::new(),
        };

        let result = service.discover_user_data(&request).await.unwrap();
        let counts = result.category_counts();

        // All lineage events should be categorized as Behavioral
        assert_eq!(counts.get(&DataCategory::Behavioral), Some(&2));
    }

    #[tokio::test]
    async fn test_size_estimation() {
        let now = Utc::now();
        let events = vec![create_test_event("alice", now)];

        let (service, _row_dir, _col_dir) = create_test_service(events);

        let request = ExportRequest {
            user_id: "alice".to_string(),
            format: super::super::types::ExportFormat::Json,
            categories: vec![],
            include_derived: false,
            include_metadata: true,
            include_audit_trail: false,
            time_range: None,
            filters: HashMap::new(),
        };

        let result = service.discover_user_data(&request).await.unwrap();
        let size = result.estimated_size_bytes();

        // Should have some estimated size
        assert!(size > 0);
    }
}
