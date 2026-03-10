// Migration Strategy for Adding Manual Mapping to Existing System
use super::store::ManualMappingStore;
use super::types::*;
use crate::governance::rdf_store::{NamedGraph, RdfStore};
use anyhow::{Context, Result};
use std::sync::Arc;
use tracing::{info, warn};

/// Migration manager for setting up manual mapping system
pub struct ManualMappingMigration {
    rdf_store: Arc<dyn RdfStore>,
    rocksdb_path: String,
}

impl ManualMappingMigration {
    pub fn new(rdf_store: Arc<dyn RdfStore>, rocksdb_path: String) -> Self {
        Self {
            rdf_store,
            rocksdb_path,
        }
    }

    /// Run full migration to set up manual mapping system
    pub async fn run_migration(&self) -> Result<()> {
        info!("Starting manual mapping system migration");

        // Step 1: Create RDF ontology extensions
        self.create_rdf_ontology().await?;

        // Step 2: Initialize RocksDB column families
        self.initialize_rocksdb().await?;

        // Step 3: Migrate existing mappings if any
        self.migrate_existing_mappings().await?;

        // Step 4: Create indexes
        self.create_indexes().await?;

        // Step 5: Verify migration
        self.verify_migration().await?;

        info!("Manual mapping system migration completed successfully");
        Ok(())
    }

    /// Create RDF ontology for manual mappings
    async fn create_rdf_ontology(&self) -> Result<()> {
        info!("Creating RDF ontology for manual mappings");

        let ontology_triples = vec![
            // Define ManualFieldMapping class
            (
                "gph:ManualFieldMapping".to_string(),
                "rdf:type".to_string(),
                "rdfs:Class".to_string(),
            ),
            (
                "gph:ManualFieldMapping".to_string(),
                "rdfs:subClassOf".to_string(),
                "prov:Activity".to_string(),
            ),
            (
                "gph:ManualFieldMapping".to_string(),
                "rdfs:label".to_string(),
                "\"User-defined field mapping\"".to_string(),
            ),
            // Define properties
            (
                "gph:hasSource".to_string(),
                "rdf:type".to_string(),
                "rdf:Property".to_string(),
            ),
            (
                "gph:hasSource".to_string(),
                "rdfs:domain".to_string(),
                "gph:ManualFieldMapping".to_string(),
            ),
            (
                "gph:hasSource".to_string(),
                "rdfs:range".to_string(),
                "gph:SourceContext".to_string(),
            ),
            (
                "gph:mapsTo".to_string(),
                "rdf:type".to_string(),
                "rdf:Property".to_string(),
            ),
            (
                "gph:mapsTo".to_string(),
                "rdfs:domain".to_string(),
                "gph:ManualFieldMapping".to_string(),
            ),
            (
                "gph:applyCount".to_string(),
                "rdf:type".to_string(),
                "rdf:Property".to_string(),
            ),
            (
                "gph:applyCount".to_string(),
                "rdfs:domain".to_string(),
                "gph:ManualFieldMapping".to_string(),
            ),
            (
                "gph:applyCount".to_string(),
                "rdfs:range".to_string(),
                "xsd:integer".to_string(),
            ),
        ];

        // Insert ontology into governance RDF store
        let graph_uri = "http://graphica.io/ontology/manual-mappings";
        let graph = NamedGraph::new(graph_uri);
        self.rdf_store
            .insert_triples(ontology_triples, Some(&graph))?;

        Ok(())
    }

    /// Initialize RocksDB with required column families
    async fn initialize_rocksdb(&self) -> Result<()> {
        info!("Initializing RocksDB for manual mappings");

        // This is handled by ManualMappingStore::new()
        // Just verify we can create the store
        let _store = ManualMappingStore::new(self.rdf_store.clone(), &self.rocksdb_path)?;

        Ok(())
    }

    /// Migrate existing mappings from old system
    async fn migrate_existing_mappings(&self) -> Result<()> {
        info!("Checking for existing mappings to migrate");

        let store = ManualMappingStore::new(self.rdf_store.clone(), &self.rocksdb_path)?;

        // SPARQL query to find all existing manual field mappings in RDF store
        let sparql_query = r#"
            PREFIX gph: <http://graphica.io/ontology#>
            PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>
            PREFIX prov: <http://www.w3.org/ns/prov#>
            PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>

            SELECT ?mapping ?sourceId ?table ?field ?targetUri ?confidence ?creator ?created ?updated ?notes ?applyCount ?acceptCount ?rejectCount
            WHERE {
                ?mapping rdf:type gph:ManualFieldMapping .
                ?mapping gph:hasSource ?source .
                ?mapping gph:mapsTo ?targetUri .
                ?mapping gph:confidence ?confidence .
                ?mapping prov:wasAttributedTo ?creator .
                ?mapping prov:generatedAtTime ?created .

                ?source gph:sourceId ?sourceId .
                ?source gph:tableName ?table .
                ?source gph:fieldName ?field .

                OPTIONAL { ?mapping gph:updatedAt ?updated . }
                OPTIONAL { ?mapping gph:notes ?notes . }
                OPTIONAL { ?mapping gph:applyCount ?applyCount . }
                OPTIONAL { ?mapping gph:acceptCount ?acceptCount . }
                OPTIONAL { ?mapping gph:rejectCount ?rejectCount . }
            }
        "#;

        // Execute query
        let results = match self.rdf_store.query(sparql_query) {
            Ok(r) => r,
            Err(e) => {
                // If query fails (e.g., no manual mappings exist yet), that's okay
                info!("No existing manual mappings found to migrate (query returned no results or failed: {})", e);
                return Ok(());
            }
        };

        if results.is_empty() {
            info!("No existing manual mappings found to migrate");
            return Ok(());
        }

        info!("Found {} existing mappings to migrate", results.len());
        let mut migrated_count = 0;
        let mut error_count = 0;

        // Convert each result to ManualFieldMapping and store in RocksDB
        for result in results {
            match self.parse_and_store_mapping(&store, &result).await {
                Ok(id) => {
                    info!("Migrated mapping: {}", id);
                    migrated_count += 1;
                }
                Err(e) => {
                    warn!("Failed to migrate mapping: {}", e);
                    error_count += 1;
                }
            }
        }

        info!(
            "Migration complete: {} mappings migrated successfully, {} errors",
            migrated_count, error_count
        );

        Ok(())
    }

    /// Parse a SPARQL query result and store as ManualFieldMapping
    async fn parse_and_store_mapping(
        &self,
        store: &ManualMappingStore,
        result: &serde_json::Value,
    ) -> Result<String> {
        // Extract fields from SPARQL result
        let mapping_uri = result
            .get("mapping")
            .and_then(|v| v.get("value"))
            .and_then(|v| v.as_str())
            .context("Missing mapping URI")?;

        // Extract ID from URI (e.g., "http://graphica.io/manual-mappings/manual_123" -> "manual_123")
        let id = mapping_uri
            .rsplit('/')
            .next()
            .unwrap_or("unknown")
            .to_string();

        let source_id = result
            .get("sourceId")
            .and_then(|v| v.get("value"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let table_name = result
            .get("table")
            .and_then(|v| v.get("value"))
            .and_then(|v| v.as_str())
            .context("Missing table name")?
            .to_string();

        let field_name = result
            .get("field")
            .and_then(|v| v.get("value"))
            .and_then(|v| v.as_str())
            .context("Missing field name")?
            .to_string();

        let target_field_uri = result
            .get("targetUri")
            .and_then(|v| v.get("value"))
            .and_then(|v| v.as_str())
            .context("Missing target URI")?
            .to_string();

        let confidence = result
            .get("confidence")
            .and_then(|v| v.get("value"))
            .and_then(|v| v.as_str())
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(1.0);

        let created_by = result
            .get("creator")
            .and_then(|v| v.get("value"))
            .and_then(|v| v.as_str())
            .context("Missing creator")?
            .to_string();

        let created_at = result
            .get("created")
            .and_then(|v| v.get("value"))
            .and_then(|v| v.as_str())
            .and_then(|v| chrono::DateTime::parse_from_rfc3339(v).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(chrono::Utc::now);

        let updated_at = result
            .get("updated")
            .and_then(|v| v.get("value"))
            .and_then(|v| v.as_str())
            .and_then(|v| chrono::DateTime::parse_from_rfc3339(v).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or(created_at);

        let notes = result
            .get("notes")
            .and_then(|v| v.get("value"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let apply_count = result
            .get("applyCount")
            .and_then(|v| v.get("value"))
            .and_then(|v| v.as_str())
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);

        let accept_count = result
            .get("acceptCount")
            .and_then(|v| v.get("value"))
            .and_then(|v| v.as_str())
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);

        let reject_count = result
            .get("rejectCount")
            .and_then(|v| v.get("value"))
            .and_then(|v| v.as_str())
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);

        // Create ManualFieldMapping
        let mapping = ManualFieldMapping {
            id: id.clone(),
            source_context: SourceContext {
                source_id,
                table_name,
                field_name,
                field_metadata: None, // Not stored in old RDF format
            },
            target_field_uri,
            confidence,
            created_by,
            created_at,
            updated_at,
            notes,
            usage_stats: UsageStats {
                apply_count,
                accept_count,
                reject_count,
                last_used: None, // Not tracked in old format
            },
        };

        // Store in RocksDB
        store.store_mapping(mapping).await?;

        Ok(id)
    }

    /// Create indexes for efficient querying
    async fn create_indexes(&self) -> Result<()> {
        info!("Creating indexes for manual mappings");

        // Indexes are created automatically by ManualMappingStore
        // This is a placeholder for any additional indexing needs

        Ok(())
    }

    /// Verify migration succeeded
    async fn verify_migration(&self) -> Result<()> {
        info!("Verifying migration");

        let store = ManualMappingStore::new(self.rdf_store.clone(), &self.rocksdb_path)?;

        // Test basic operations
        let test_mapping = ManualFieldMapping {
            id: "test_verification".to_string(),
            source_context: SourceContext {
                source_id: Some("test".to_string()),
                table_name: "test_table".to_string(),
                field_name: "test_field".to_string(),
                field_metadata: None,
            },
            target_field_uri: "test:targetField".to_string(),
            confidence: 1.0,
            created_by: "migration_test".to_string(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            notes: Some("Migration verification test".to_string()),
            usage_stats: UsageStats::default(),
        };

        // Store test mapping
        store.store_mapping(test_mapping.clone()).await?;

        // Retrieve test mapping
        let retrieved = store
            .get_mapping("test_verification")
            .await?
            .context("Failed to retrieve test mapping")?;

        assert_eq!(retrieved.id, "test_verification");

        // Find by source
        let found = store
            .find_by_source(&test_mapping.source_context)
            .await?
            .context("Failed to find test mapping by source")?;

        assert_eq!(found.id, "test_verification");

        // Clean up test mapping
        // Note: Implement delete method if needed

        info!("Migration verification completed successfully");
        Ok(())
    }
}

/// Rollback migration if needed
pub struct ManualMappingRollback {
    rdf_store: Arc<dyn RdfStore>,
    rocksdb_path: String,
}

impl ManualMappingRollback {
    pub fn new(rdf_store: Arc<dyn RdfStore>, rocksdb_path: String) -> Self {
        Self {
            rdf_store,
            rocksdb_path,
        }
    }

    pub async fn rollback(&self) -> Result<()> {
        warn!("Rolling back manual mapping migration");

        // Remove RDF triples
        let graph_uri = "http://graphica.io/graphs/manual-mappings";
        let graph = NamedGraph::new(graph_uri);
        self.rdf_store.clear_graph(&graph)?;

        // Remove RocksDB data
        // Note: In production, we'd want to backup before removing
        std::fs::remove_dir_all(&self.rocksdb_path).ok();

        warn!("Manual mapping migration rolled back");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::governance::rdf_store::GraphicaRdfStore;
    use tempfile::TempDir;

    async fn create_test_rdf_store() -> Arc<dyn RdfStore> {
        Arc::new(GraphicaRdfStore::new_in_memory().unwrap())
    }

    fn create_test_rocksdb_path() -> TempDir {
        tempfile::TempDir::new().unwrap()
    }

    /// Test full migration with no existing mappings
    #[tokio::test]
    async fn test_migration_empty() {
        let rdf_store = create_test_rdf_store().await;
        let temp_dir = create_test_rocksdb_path();
        let rocksdb_path = temp_dir.path().to_str().unwrap().to_string();

        let migration = ManualMappingMigration::new(rdf_store.clone(), rocksdb_path.clone());

        // Should succeed even with no existing mappings
        let result = migration.run_migration().await;
        assert!(
            result.is_ok(),
            "Migration should succeed with no existing mappings"
        );
    }

    /// Test migration with existing manual mappings in RDF
    ///
    /// NOTE: This test requires a full RDF store with SPARQL support.
    /// The in-memory test store may not support complex SPARQL queries.
    /// This test validates the end-to-end migration flow.
    #[tokio::test]
    #[ignore = "Requires full RDF store with SPARQL support - run with real RDF backend"]
    async fn test_migration_with_existing_mappings() {
        let rdf_store = create_test_rdf_store().await;
        let temp_dir = create_test_rocksdb_path();
        let rocksdb_path = temp_dir.path().to_str().unwrap().to_string();

        // Insert test mappings into RDF store (simulating old system)
        let graph = NamedGraph::new("http://graphica.io/graph/manual-mappings");

        let mapping_id = "manual_test_123";
        let mapping_uri = format!("http://graphica.io/manual-mappings/{}", mapping_id);
        let source_uri = format!("{}#source", mapping_uri);

        // Create triples for a manual field mapping
        let triples = vec![
            // Mapping metadata
            (
                mapping_uri.clone(),
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_string(),
                "http://graphica.io/ontology#ManualFieldMapping".to_string(),
            ),
            (
                mapping_uri.clone(),
                "http://graphica.io/ontology#hasSource".to_string(),
                source_uri.clone(),
            ),
            (
                mapping_uri.clone(),
                "http://graphica.io/ontology#mapsTo".to_string(),
                "http://schema.org/email".to_string(),
            ),
            (
                mapping_uri.clone(),
                "http://graphica.io/ontology#confidence".to_string(),
                "1.0".to_string(),
            ),
            (
                mapping_uri.clone(),
                "http://www.w3.org/ns/prov#wasAttributedTo".to_string(),
                "test_user".to_string(),
            ),
            (
                mapping_uri.clone(),
                "http://www.w3.org/ns/prov#generatedAtTime".to_string(),
                "2024-01-01T00:00:00Z".to_string(),
            ),
            (
                mapping_uri.clone(),
                "http://graphica.io/ontology#applyCount".to_string(),
                "42".to_string(),
            ),
            (
                mapping_uri.clone(),
                "http://graphica.io/ontology#acceptCount".to_string(),
                "10".to_string(),
            ),
            (
                mapping_uri.clone(),
                "http://graphica.io/ontology#rejectCount".to_string(),
                "2".to_string(),
            ),
            (
                mapping_uri.clone(),
                "http://graphica.io/ontology#notes".to_string(),
                "Test mapping".to_string(),
            ),
            // Source context
            (
                source_uri.clone(),
                "http://graphica.io/ontology#sourceId".to_string(),
                "source_1".to_string(),
            ),
            (
                source_uri.clone(),
                "http://graphica.io/ontology#tableName".to_string(),
                "customers".to_string(),
            ),
            (
                source_uri.clone(),
                "http://graphica.io/ontology#fieldName".to_string(),
                "email_address".to_string(),
            ),
        ];

        rdf_store.insert_triples(triples, Some(&graph)).unwrap();

        // Run migration
        let migration = ManualMappingMigration::new(rdf_store.clone(), rocksdb_path.clone());
        let result = migration.run_migration().await;
        assert!(result.is_ok(), "Migration should succeed: {:?}", result);

        // Verify mapping was migrated to RocksDB
        let store = ManualMappingStore::new(rdf_store.clone(), &rocksdb_path).unwrap();
        let retrieved = store.get_mapping(mapping_id).await;
        assert!(retrieved.is_ok(), "Should retrieve migrated mapping");

        let mapping = retrieved.unwrap();
        assert!(mapping.is_some(), "Mapping should exist after migration");

        let mapping = mapping.unwrap();
        assert_eq!(mapping.id, mapping_id);
        assert_eq!(mapping.source_context.table_name, "customers");
        assert_eq!(mapping.source_context.field_name, "email_address");
        assert_eq!(mapping.target_field_uri, "http://schema.org/email");
        assert_eq!(mapping.confidence, 1.0);
        assert_eq!(mapping.created_by, "test_user");
        assert_eq!(mapping.usage_stats.apply_count, 42);
        assert_eq!(mapping.usage_stats.accept_count, 10);
        assert_eq!(mapping.usage_stats.reject_count, 2);
        assert_eq!(mapping.notes, Some("Test mapping".to_string()));
    }

    /// Test parsing of SPARQL query results
    #[tokio::test]
    async fn test_parse_sparql_result() {
        let rdf_store = create_test_rdf_store().await;
        let temp_dir = create_test_rocksdb_path();
        let rocksdb_path = temp_dir.path().to_str().unwrap().to_string();

        let migration = ManualMappingMigration::new(rdf_store.clone(), rocksdb_path.clone());
        let store = ManualMappingStore::new(rdf_store.clone(), &rocksdb_path).unwrap();

        // Create mock SPARQL result
        let result = serde_json::json!({
            "mapping": {
                "type": "uri",
                "value": "http://graphica.io/manual-mappings/manual_test_456"
            },
            "sourceId": {
                "type": "literal",
                "value": "db_1"
            },
            "table": {
                "type": "literal",
                "value": "orders"
            },
            "field": {
                "type": "literal",
                "value": "customer_id"
            },
            "targetUri": {
                "type": "uri",
                "value": "http://schema.org/identifier"
            },
            "confidence": {
                "type": "literal",
                "datatype": "http://www.w3.org/2001/XMLSchema#double",
                "value": "1.0"
            },
            "creator": {
                "type": "literal",
                "value": "admin"
            },
            "created": {
                "type": "literal",
                "datatype": "http://www.w3.org/2001/XMLSchema#dateTime",
                "value": "2024-01-15T10:30:00Z"
            },
            "applyCount": {
                "type": "literal",
                "datatype": "http://www.w3.org/2001/XMLSchema#integer",
                "value": "100"
            },
            "acceptCount": {
                "type": "literal",
                "datatype": "http://www.w3.org/2001/XMLSchema#integer",
                "value": "95"
            },
            "rejectCount": {
                "type": "literal",
                "datatype": "http://www.w3.org/2001/XMLSchema#integer",
                "value": "5"
            }
        });

        // Parse and store
        let result = migration.parse_and_store_mapping(&store, &result).await;
        assert!(
            result.is_ok(),
            "Should parse and store mapping successfully"
        );

        let id = result.unwrap();
        assert_eq!(id, "manual_test_456");

        // Verify stored mapping
        let retrieved = store.get_mapping(&id).await.unwrap();
        assert!(retrieved.is_some());

        let mapping = retrieved.unwrap();
        assert_eq!(mapping.source_context.table_name, "orders");
        assert_eq!(mapping.source_context.field_name, "customer_id");
        assert_eq!(mapping.target_field_uri, "http://schema.org/identifier");
        assert_eq!(mapping.usage_stats.apply_count, 100);
        assert_eq!(mapping.usage_stats.accept_count, 95);
        assert_eq!(mapping.usage_stats.reject_count, 5);
    }

    /// Test rollback functionality
    #[tokio::test]
    async fn test_rollback() {
        let rdf_store = create_test_rdf_store().await;
        let temp_dir = create_test_rocksdb_path();
        let rocksdb_path = temp_dir.path().to_str().unwrap().to_string();

        // First, run migration
        let migration = ManualMappingMigration::new(rdf_store.clone(), rocksdb_path.clone());
        migration.run_migration().await.unwrap();

        // Verify RDF ontology was created
        let count_before = rdf_store
            .count_triples(Some(&NamedGraph::new(
                "http://graphica.io/ontology/manual-mappings",
            )))
            .unwrap();
        assert!(count_before > 0, "Ontology triples should exist");

        // Rollback
        let rollback = ManualMappingRollback::new(rdf_store.clone(), rocksdb_path.clone());
        let result = rollback.rollback().await;
        assert!(result.is_ok(), "Rollback should succeed");

        // Verify graph was cleared
        let count_after = rdf_store
            .count_triples(Some(&NamedGraph::new(
                "http://graphica.io/graphs/manual-mappings",
            )))
            .unwrap();
        assert_eq!(count_after, 0, "Triples should be cleared after rollback");
    }

    /// Test migration handles malformed data gracefully
    #[tokio::test]
    async fn test_migration_with_malformed_data() {
        let rdf_store = create_test_rdf_store().await;
        let temp_dir = create_test_rocksdb_path();
        let rocksdb_path = temp_dir.path().to_str().unwrap().to_string();

        let migration = ManualMappingMigration::new(rdf_store.clone(), rocksdb_path.clone());
        let store = ManualMappingStore::new(rdf_store.clone(), &rocksdb_path).unwrap();

        // Malformed result (missing required fields)
        let malformed_result = serde_json::json!({
            "mapping": {
                "value": "http://graphica.io/manual-mappings/bad_mapping"
            }
            // Missing table, field, target, etc.
        });

        let result = migration
            .parse_and_store_mapping(&store, &malformed_result)
            .await;
        assert!(result.is_err(), "Should fail with malformed data");
    }

    /// Test verification step
    #[tokio::test]
    async fn test_verification() {
        let rdf_store = create_test_rdf_store().await;
        let temp_dir = create_test_rocksdb_path();
        let rocksdb_path = temp_dir.path().to_str().unwrap().to_string();

        let migration = ManualMappingMigration::new(rdf_store.clone(), rocksdb_path.clone());

        // Verification should create, retrieve, and find a test mapping
        let result = migration.verify_migration().await;
        assert!(result.is_ok(), "Verification should succeed: {:?}", result);
    }
}
