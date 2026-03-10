//! Schema Versioning Module
//!
//! Track schema versions over time for:
//! - Historical schema queries
//! - Schema evolution tracking
//! - Lineage integration
//! - Audit and compliance

use crate::mapping::ddl::dialects::TableDefinition;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Schema version metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaVersion {
    /// Unique version ID
    pub version_id: String,

    /// Table name
    pub table_name: String,

    /// Table definition at this version
    pub table_definition: TableDefinition,

    /// Timestamp when this version was created
    pub created_at: DateTime<Utc>,

    /// User or system that created this version
    pub created_by: String,

    /// Previous version ID (None if first version)
    pub previous_version_id: Option<String>,

    /// Change description
    pub change_description: String,

    /// Schema hash for quick comparison
    pub schema_hash: String,

    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

impl SchemaVersion {
    /// Create a new schema version
    pub fn new(
        table_name: String,
        table_definition: TableDefinition,
        created_by: String,
        change_description: String,
        previous_version_id: Option<String>,
    ) -> Self {
        let version_id = Self::generate_version_id(&table_name);
        let schema_hash = Self::compute_schema_hash(&table_definition);

        Self {
            version_id,
            table_name,
            table_definition,
            created_at: Utc::now(),
            created_by,
            previous_version_id,
            change_description,
            schema_hash,
            metadata: HashMap::new(),
        }
    }

    /// Generate a unique version ID
    fn generate_version_id(table_name: &str) -> String {
        use uuid::Uuid;
        format!("schema_v_{}_{}", table_name, Uuid::new_v4())
    }

    /// Compute hash of schema for quick comparison
    fn compute_schema_hash(table_def: &TableDefinition) -> String {
        use sha2::{Digest, Sha256};

        // Create canonical representation for hashing
        let mut hasher = Sha256::new();

        // Hash table name
        hasher.update(table_def.name.as_bytes());

        // Hash columns in sorted order for consistency
        let mut sorted_columns: Vec<_> = table_def.columns.iter().collect();
        sorted_columns.sort_by_key(|c| &c.name);

        for col in sorted_columns {
            hasher.update(col.name.as_bytes());
            hasher.update(col.sql_type.as_bytes());
            hasher.update(&[col.nullable as u8]);
            hasher.update(&[col.primary_key as u8]);
        }

        // Hash primary key
        for pk_col in &table_def.primary_key {
            hasher.update(pk_col.as_bytes());
        }

        format!("{:x}", hasher.finalize())
    }

    /// Check if this version has the same schema as another (ignoring metadata)
    pub fn same_schema_as(&self, other: &SchemaVersion) -> bool {
        self.schema_hash == other.schema_hash
    }
}

/// Schema version history for a table
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaHistory {
    /// Table name
    pub table_name: String,

    /// All versions in chronological order
    pub versions: Vec<SchemaVersion>,

    /// Current (latest) version
    pub current_version: Option<SchemaVersion>,
}

impl SchemaHistory {
    /// Create new empty history
    pub fn new(table_name: String) -> Self {
        Self {
            table_name,
            versions: Vec::new(),
            current_version: None,
        }
    }

    /// Add a new version to the history
    pub fn add_version(&mut self, version: SchemaVersion) {
        self.versions.push(version.clone());
        self.current_version = Some(version);
    }

    /// Get schema version at a specific point in time
    pub fn get_version_at(&self, timestamp: DateTime<Utc>) -> Option<&SchemaVersion> {
        self.versions
            .iter()
            .rev() // Search from newest to oldest
            .find(|v| v.created_at <= timestamp)
    }

    /// Get version by version ID
    pub fn get_version_by_id(&self, version_id: &str) -> Option<&SchemaVersion> {
        self.versions.iter().find(|v| v.version_id == version_id)
    }

    /// Get all versions created between two timestamps
    pub fn get_versions_between(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Vec<&SchemaVersion> {
        self.versions
            .iter()
            .filter(|v| v.created_at >= start && v.created_at <= end)
            .collect()
    }

    /// Get the number of schema changes
    pub fn change_count(&self) -> usize {
        self.versions.len()
    }
}

/// Trait for schema version storage (RDF-backed in production)
#[async_trait::async_trait]
pub trait SchemaVersionStore: Send + Sync {
    /// Save a schema version
    async fn save_version(&self, version: &SchemaVersion) -> Result<()>;

    /// Get current version for a table
    async fn get_current_version(&self, table_name: &str) -> Result<Option<SchemaVersion>>;

    /// Get schema version at a specific time
    async fn get_version_at(
        &self,
        table_name: &str,
        timestamp: DateTime<Utc>,
    ) -> Result<Option<SchemaVersion>>;

    /// Get full history for a table
    async fn get_history(&self, table_name: &str) -> Result<SchemaHistory>;

    /// List all tables with schema versions
    async fn list_versioned_tables(&self) -> Result<Vec<String>>;
}

/// In-memory schema version store (for testing and development)
pub struct InMemorySchemaVersionStore {
    /// Table name -> Schema history
    histories: tokio::sync::RwLock<HashMap<String, SchemaHistory>>,
}

impl InMemorySchemaVersionStore {
    pub fn new() -> Self {
        Self {
            histories: tokio::sync::RwLock::new(HashMap::new()),
        }
    }
}

#[async_trait::async_trait]
impl SchemaVersionStore for InMemorySchemaVersionStore {
    async fn save_version(&self, version: &SchemaVersion) -> Result<()> {
        let mut histories = self.histories.write().await;

        let history = histories
            .entry(version.table_name.clone())
            .or_insert_with(|| SchemaHistory::new(version.table_name.clone()));

        history.add_version(version.clone());
        Ok(())
    }

    async fn get_current_version(&self, table_name: &str) -> Result<Option<SchemaVersion>> {
        let histories = self.histories.read().await;
        Ok(histories
            .get(table_name)
            .and_then(|h| h.current_version.clone()))
    }

    async fn get_version_at(
        &self,
        table_name: &str,
        timestamp: DateTime<Utc>,
    ) -> Result<Option<SchemaVersion>> {
        let histories = self.histories.read().await;
        Ok(histories
            .get(table_name)
            .and_then(|h| h.get_version_at(timestamp))
            .cloned())
    }

    async fn get_history(&self, table_name: &str) -> Result<SchemaHistory> {
        let histories = self.histories.read().await;
        histories
            .get(table_name)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("No schema history found for table: {}", table_name))
    }

    async fn list_versioned_tables(&self) -> Result<Vec<String>> {
        let histories = self.histories.read().await;
        Ok(histories.keys().cloned().collect())
    }
}

/// Helper function to record a new schema version after DDL execution
pub async fn record_schema_version(
    store: &dyn SchemaVersionStore,
    table_name: &str,
    table_definition: &TableDefinition,
    change_description: &str,
    created_by: &str,
) -> Result<SchemaVersion> {
    // Get previous version if exists
    let previous_version = store.get_current_version(table_name).await?;

    // Check if schema actually changed
    if let Some(ref prev) = previous_version {
        let new_hash = SchemaVersion::compute_schema_hash(table_definition);
        if prev.schema_hash == new_hash {
            tracing::debug!(
                "Schema for table {} unchanged, skipping version recording",
                table_name
            );
            return Ok(prev.clone());
        }
    }

    // Create new version
    let version = SchemaVersion::new(
        table_name.to_string(),
        table_definition.clone(),
        created_by.to_string(),
        change_description.to_string(),
        previous_version.as_ref().map(|v| v.version_id.clone()),
    );

    // Save version
    store
        .save_version(&version)
        .await
        .with_context(|| format!("Failed to save schema version for table: {}", table_name))?;

    // Record lineage for schema change
    record_schema_lineage(
        table_name,
        &version,
        previous_version.as_ref(),
        change_description,
        created_by,
    );

    tracing::info!(
        "Recorded new schema version {} for table {}",
        version.version_id,
        table_name
    );

    Ok(version)
}

/// Record lineage for schema changes
///
/// Logs comprehensive lineage information for schema changes that can be
/// integrated with RDF provenance tracking.
///
/// TODO: Integrate with WorkflowLineageGenerator to create PROV-based RDF triples:
/// - Activity: Schema creation/modification
/// - Entity: Table schema
/// - Agent: System/user that created the schema
/// - Source: CSV file or other source data
fn record_schema_lineage(
    table_name: &str,
    new_version: &SchemaVersion,
    previous_version: Option<&SchemaVersion>,
    change_description: &str,
    created_by: &str,
) {
    use serde_json::json;

    let lineage_event = json!({
        "event_type": "schema_change",
        "table_name": table_name,
        "version_id": new_version.version_id,
        "schema_hash": new_version.schema_hash,
        "created_at": new_version.created_at.to_rfc3339(),
        "created_by": created_by,
        "change_description": change_description,
        "previous_version_id": previous_version.map(|v| v.version_id.clone()),
        "columns": new_version.table_definition.columns.iter().map(|col| {
            json!({
                "name": col.name,
                "type": col.sql_type,
                "nullable": col.nullable,
                "primary_key": col.primary_key,
            })
        }).collect::<Vec<_>>(),
        "primary_key": new_version.table_definition.primary_key,
    });

    // Log lineage event (structured logging for later RDF conversion)
    tracing::info!(
        target: "graphica::lineage::schema",
        lineage_event = %lineage_event,
        "Schema change lineage recorded"
    );

    // TODO: Generate and store RDF triples following W3C PROV ontology:
    //
    // Subject: gph:schema/{version_id}
    // Triples:
    //   rdf:type -> gph:SchemaVersion
    //   prov:wasGeneratedBy -> activity:{change_id}
    //   prov:generatedAtTime -> {timestamp}
    //   gph:tableName -> {table_name}
    //   gph:schemaHash -> {schema_hash}
    //
    // Activity: activity:{change_id}
    // Triples:
    //   rdf:type -> prov:Activity
    //   prov:wasAssociatedWith -> agent:{created_by}
    //   prov:used -> source:{csv_file} (if applicable)
    //   prov:startedAtTime -> {timestamp}
    //   prov:endedAtTime -> {timestamp}
    //
    // If previous version exists:
    //   gph:schema/{version_id} prov:wasRevisionOf gph:schema/{previous_version_id}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::ddl::dialects::ColumnDefinition;

    fn create_test_table_def(name: &str) -> TableDefinition {
        TableDefinition {
            name: name.to_string(),
            columns: vec![
                ColumnDefinition {
                    name: "ID".to_string(),
                    sql_type: "INTEGER".to_string(),
                    nullable: false,
                    default_value: None,
                    primary_key: true,
                    unique: false,
                    check_constraint: None,
                    comment: None,
                },
                ColumnDefinition {
                    name: "NAME".to_string(),
                    sql_type: "VARCHAR(255)".to_string(),
                    nullable: false,
                    default_value: None,
                    primary_key: false,
                    unique: false,
                    check_constraint: None,
                    comment: None,
                },
            ],
            primary_key: vec!["ID".to_string()],
            foreign_keys: vec![],
            indexes: vec![],
            comment: None,
        }
    }

    #[test]
    fn test_schema_hash_consistency() {
        let table1 = create_test_table_def("CUSTOMERS");
        let table2 = create_test_table_def("CUSTOMERS");

        let hash1 = SchemaVersion::compute_schema_hash(&table1);
        let hash2 = SchemaVersion::compute_schema_hash(&table2);

        assert_eq!(hash1, hash2, "Same schema should produce same hash");
    }

    #[test]
    fn test_schema_hash_differs_on_change() {
        let mut table1 = create_test_table_def("CUSTOMERS");
        let mut table2 = create_test_table_def("CUSTOMERS");

        // Add column to table2
        table2.columns.push(ColumnDefinition {
            name: "EMAIL".to_string(),
            sql_type: "VARCHAR(255)".to_string(),
            nullable: true,
            default_value: None,
            primary_key: false,
            unique: false,
            check_constraint: None,
            comment: None,
        });

        let hash1 = SchemaVersion::compute_schema_hash(&table1);
        let hash2 = SchemaVersion::compute_schema_hash(&table2);

        assert_ne!(
            hash1, hash2,
            "Different schemas should produce different hashes"
        );
    }

    #[tokio::test]
    async fn test_in_memory_store() {
        let store = InMemorySchemaVersionStore::new();
        let table_def = create_test_table_def("CUSTOMERS");

        let version = record_schema_version(
            &store,
            "CUSTOMERS",
            &table_def,
            "Initial schema",
            "test_user",
        )
        .await
        .unwrap();

        // Retrieve current version
        let current = store.get_current_version("CUSTOMERS").await.unwrap();
        assert!(current.is_some());
        assert_eq!(current.unwrap().version_id, version.version_id);
    }

    #[tokio::test]
    async fn test_schema_history() {
        let store = InMemorySchemaVersionStore::new();
        let mut table_def = create_test_table_def("CUSTOMERS");

        // Record first version
        record_schema_version(&store, "CUSTOMERS", &table_def, "Initial schema", "user1")
            .await
            .unwrap();

        // Modify and record second version
        table_def.columns.push(ColumnDefinition {
            name: "EMAIL".to_string(),
            sql_type: "VARCHAR(255)".to_string(),
            nullable: true,
            default_value: None,
            primary_key: false,
            unique: false,
            check_constraint: None,
            comment: None,
        });

        record_schema_version(
            &store,
            "CUSTOMERS",
            &table_def,
            "Added email column",
            "user2",
        )
        .await
        .unwrap();

        // Get history
        let history = store.get_history("CUSTOMERS").await.unwrap();
        assert_eq!(history.change_count(), 2);
    }

    #[tokio::test]
    async fn test_no_duplicate_version_on_same_schema() {
        let store = InMemorySchemaVersionStore::new();
        let table_def = create_test_table_def("CUSTOMERS");

        // Record first version
        let v1 = record_schema_version(&store, "CUSTOMERS", &table_def, "Initial schema", "user1")
            .await
            .unwrap();

        // Try to record same schema again
        let v2 = record_schema_version(&store, "CUSTOMERS", &table_def, "Same schema", "user2")
            .await
            .unwrap();

        // Should return same version (no duplicate)
        assert_eq!(v1.version_id, v2.version_id);

        let history = store.get_history("CUSTOMERS").await.unwrap();
        assert_eq!(history.change_count(), 1, "Should only have one version");
    }
}
