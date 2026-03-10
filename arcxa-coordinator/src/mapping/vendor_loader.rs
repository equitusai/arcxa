//! Vendor Ontology Bulk Loader
//!
//! Loads pre-built vendor ontologies (Oracle, SAP, etc.) into the OntologyRegistry
//! and registers vendor→semantic mappings in the ManualMappingStore.
//!
//! ## Architecture Decision: Why extend workflow engine vs new mediation engine?
//!
//! **User Insight**: "we kind of already had this 'mediator' as part of our workflow designer"
//!
//! **Analysis**:
//! - OntologyMapperTransformer already maps source → ontology URI → target
//! - WorkflowLineageGenerator already tracks multi-step transformations
//! - ManualMappingStore already provides priority: manual (1.0) > session > automatic
//! - Workflow routes already support composable transformations
//!
//! **Decision**: Don't build parallel SemanticMediationEngine. Instead:
//! 1. Load vendor ontologies into existing PersistedOntologyRegistry
//! 2. Load vendor→semantic mappings into existing ManualMappingStore
//! 3. Create workflow templates for Oracle→SAP migrations
//! 4. Let OntologyMapperTransformer handle semantic mediation (already does!)
//!
//! ## Benefits:
//! - Reuses mature infrastructure (OntologyMapper, lineage, manual mappings)
//! - Minimal new code (~200 lines vs 2000+ for new engine)
//! - End-to-end lineage already works
//! - Composable via workflow routes
//!
//! ## File Structure:
//!
//! ```
//! vendors/
//! ├── oracle_ebs_r12.2/
//! │   ├── metadata.json           # Display name, table count, etc.
//! │   ├── ontology.ttl            # Vendor schema (RDF/Turtle)
//! │   └── mappings_to_semantic/
//! │       ├── accounting.json     # Oracle GL → accounting:* URIs
//! │       └── supply_chain.json   # Oracle PO → supply_chain:* URIs
//! ├── sap_s4hana_2023/
//! │   ├── metadata.json
//! │   ├── ontology.ttl
//! │   └── mappings_from_semantic/
//! │       └── accounting.json     # accounting:* URIs → SAP FI tables
//! ```

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::governance::rdf_store::RdfStore;
use crate::mapping::manual::{ManualFieldMapping, ManualMappingStore, SourceContext, UsageStats};
use crate::mapping::ontology_registry::PersistedOntologyRegistry;

/// Vendor ontology metadata (from metadata.json)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VendorOntologyMetadata {
    /// Vendor identifier (must match directory name)
    pub vendor_id: String,

    /// Human-readable display name
    pub display_name: String,

    /// Version string (e.g., "R12.2.11", "S/4HANA 2023")
    pub version: String,

    /// Modules/schemas covered
    pub modules: Vec<String>,

    /// Number of tables in this ontology
    pub table_count: usize,

    /// Number of fields/columns
    pub field_count: usize,

    /// RDF namespace URI
    pub namespace: String,

    /// Optional documentation URL
    #[serde(default)]
    pub documentation_url: Option<String>,
}

/// Batch mapping file (vendor→semantic or semantic→vendor)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchMappingFile {
    /// Source vendor ID (e.g., "oracle_ebs_r12.2")
    pub source_vendor: String,

    /// Target semantic domain (e.g., "accounting", "supply_chain")
    pub target_semantic: String,

    /// Individual field mappings
    pub mappings: Vec<BatchFieldMapping>,
}

/// Single field mapping in batch file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchFieldMapping {
    /// Source table name
    pub source_table: String,

    /// Source field name
    pub source_field: String,

    /// Target ontology URI
    ///
    /// Examples:
    /// - `http://graphica.io/ontology/accounting#documentIdentifier`
    /// - `http://schema.org/givenName`
    pub target_uri: String,

    /// Mapping confidence (0.0 - 1.0)
    ///
    /// Pre-built vendor mappings should be high confidence (0.9-1.0)
    /// as they're validated against vendor documentation
    pub confidence: f64,

    /// Optional SQL transformation expression
    ///
    /// Examples:
    /// - `null` - Direct mapping
    /// - `"UPPER(field_name)"` - Transform to uppercase
    /// - `"COALESCE(field_dr, 0) - COALESCE(field_cr, 0)"` - DR/CR to signed amount
    #[serde(default)]
    pub transformation: Option<String>,

    /// Optional notes explaining the mapping
    #[serde(default)]
    pub notes: Option<String>,
}

/// Vendor ontology bulk loader
///
/// Loads pre-built vendor ontologies and mappings for ERP migrations
pub struct VendorOntologyLoader {
    /// Ontology registry (for vendor schemas)
    registry: Arc<PersistedOntologyRegistry>,

    /// Manual mapping store (for vendor→semantic mappings)
    manual_store: Option<Arc<ManualMappingStore>>,
}

impl VendorOntologyLoader {
    /// Create vendor ontology loader
    ///
    /// # Arguments
    ///
    /// * `registry` - Ontology registry for storing vendor schemas
    pub fn new(registry: Arc<PersistedOntologyRegistry>) -> Self {
        Self {
            registry,
            manual_store: None,
        }
    }

    /// Set manual mapping store (optional, for loading vendor→semantic mappings)
    pub fn with_manual_store(mut self, store: Arc<ManualMappingStore>) -> Self {
        self.manual_store = Some(store);
        self
    }

    /// Load all vendor ontologies from directory
    ///
    /// Scans directory for vendor subdirectories and loads:
    /// 1. Vendor schema (ontology.ttl) into OntologyRegistry
    /// 2. Vendor→semantic mappings into ManualMappingStore (if configured)
    ///
    /// # Arguments
    ///
    /// * `vendor_dir` - Directory containing vendor subdirectories
    ///
    /// # Returns
    ///
    /// Number of vendor ontologies loaded
    ///
    /// # Example
    ///
    /// ```ignore
    /// let loader = VendorOntologyLoader::new(registry)
    ///     .with_manual_store(manual_store);
    ///
    /// let count = loader.load_all_vendors("./vendors").await?;
    /// println!("Loaded {} vendor ontologies", count);
    /// ```
    pub async fn load_all_vendors(&self, vendor_dir: impl AsRef<Path>) -> Result<usize> {
        let vendor_dir = vendor_dir.as_ref();
        info!("Loading vendor ontologies from: {}", vendor_dir.display());

        if !vendor_dir.exists() {
            warn!("Vendor directory does not exist: {}", vendor_dir.display());
            return Ok(0);
        }

        let mut loaded_count = 0;

        // Iterate over vendor subdirectories
        for entry in std::fs::read_dir(vendor_dir).context(format!(
            "Failed to read vendor directory: {}",
            vendor_dir.display()
        ))? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                match self.load_vendor(&path).await {
                    Ok(metadata) => {
                        info!(
                            "✓ Loaded vendor ontology: {} ({} tables, {} fields)",
                            metadata.display_name, metadata.table_count, metadata.field_count
                        );
                        loaded_count += 1;
                    }
                    Err(e) => {
                        warn!("✗ Failed to load vendor from {}: {}", path.display(), e);
                    }
                }
            }
        }

        info!("Loaded {} vendor ontologies", loaded_count);
        Ok(loaded_count)
    }

    /// Load single vendor ontology from directory
    ///
    /// # Arguments
    ///
    /// * `vendor_path` - Path to vendor directory (e.g., `vendors/oracle_ebs_r12.2`)
    ///
    /// # Returns
    ///
    /// Vendor metadata
    async fn load_vendor(&self, vendor_path: impl AsRef<Path>) -> Result<VendorOntologyMetadata> {
        let vendor_path = vendor_path.as_ref();
        let vendor_id = vendor_path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| anyhow::anyhow!("Invalid vendor directory name"))?;

        debug!("Loading vendor: {}", vendor_id);

        // 1. Load metadata.json
        let metadata = self.load_metadata(vendor_path)?;

        // Validate vendor_id matches directory name
        if metadata.vendor_id != vendor_id {
            return Err(anyhow::anyhow!(
                "Vendor ID mismatch: metadata.vendor_id='{}' != directory name '{}'",
                metadata.vendor_id,
                vendor_id
            ));
        }

        // 2. Load ontology.ttl
        self.load_ontology(vendor_path, &metadata).await?;

        // 3. Load mappings (if manual store configured)
        if self.manual_store.is_some() {
            self.load_vendor_mappings(vendor_path, &metadata.vendor_id)
                .await?;
        }

        Ok(metadata)
    }

    /// Load metadata.json from vendor directory
    fn load_metadata(&self, vendor_path: &Path) -> Result<VendorOntologyMetadata> {
        let metadata_path = vendor_path.join("metadata.json");

        if !metadata_path.exists() {
            return Err(anyhow::anyhow!(
                "Missing metadata.json in {}",
                vendor_path.display()
            ));
        }

        let file = std::fs::File::open(&metadata_path).context(format!(
            "Failed to open metadata.json: {}",
            metadata_path.display()
        ))?;

        let metadata: VendorOntologyMetadata = serde_json::from_reader(file).context(format!(
            "Failed to parse metadata.json: {}",
            metadata_path.display()
        ))?;

        Ok(metadata)
    }

    /// Load ontology.ttl into OntologyRegistry
    async fn load_ontology(
        &self,
        vendor_path: &Path,
        metadata: &VendorOntologyMetadata,
    ) -> Result<()> {
        let ontology_path = vendor_path.join("ontology.ttl");

        if !ontology_path.exists() {
            return Err(anyhow::anyhow!(
                "Missing ontology.ttl in {}",
                vendor_path.display()
            ));
        }

        let ontology_content = std::fs::read_to_string(&ontology_path).context(format!(
            "Failed to read ontology.ttl: {}",
            ontology_path.display()
        ))?;

        // Register in persisted ontology registry
        self.registry
            .register_custom_ontology(
                &metadata.vendor_id,
                ontology_content,
                Some(metadata.namespace.clone()),
            )
            .await
            .context(format!(
                "Failed to register ontology for vendor: {}",
                metadata.vendor_id
            ))?;

        debug!(
            "Registered ontology: {} (namespace: {})",
            metadata.vendor_id, metadata.namespace
        );

        Ok(())
    }

    /// Load vendor→semantic mappings from mappings_to_semantic/ directory
    async fn load_vendor_mappings(&self, vendor_path: &Path, vendor_id: &str) -> Result<()> {
        let manual_store = self
            .manual_store
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Manual mapping store not configured"))?;

        let mappings_dir = vendor_path.join("mappings_to_semantic");

        if !mappings_dir.exists() {
            debug!(
                "No mappings_to_semantic directory for vendor: {}",
                vendor_id
            );
            return Ok(());
        }

        let mut total_mappings = 0;

        // Load all mapping files (accounting.json, supply_chain.json, etc.)
        for entry in std::fs::read_dir(&mappings_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                let count = self
                    .load_mapping_file(manual_store.clone(), &path, "system")
                    .await?;

                debug!(
                    "Loaded {} mappings from {}",
                    count,
                    path.file_name().unwrap().to_str().unwrap()
                );

                total_mappings += count;
            }
        }

        info!(
            "Loaded {} manual mappings for vendor: {}",
            total_mappings, vendor_id
        );

        Ok(())
    }

    /// Load mappings from single JSON file into ManualMappingStore
    ///
    /// # Arguments
    ///
    /// * `manual_store` - Manual mapping store
    /// * `mappings_file` - Path to mapping file (e.g., `accounting.json`)
    /// * `created_by` - User/system identifier for audit trail
    ///
    /// # Returns
    ///
    /// Number of mappings loaded
    pub async fn load_mapping_file(
        &self,
        manual_store: Arc<ManualMappingStore>,
        mappings_file: impl AsRef<Path>,
        created_by: &str,
    ) -> Result<usize> {
        let mappings_file = mappings_file.as_ref();

        let content = std::fs::read_to_string(mappings_file).context(format!(
            "Failed to read mapping file: {}",
            mappings_file.display()
        ))?;

        let batch_file: BatchMappingFile = serde_json::from_str(&content).context(format!(
            "Failed to parse mapping file: {}",
            mappings_file.display()
        ))?;

        let mut loaded_count = 0;

        for mapping in &batch_file.mappings {
            // Validate confidence
            if mapping.confidence < 0.0 || mapping.confidence > 1.0 {
                warn!(
                    "Skipping mapping with invalid confidence: {}.{} -> {} (confidence: {})",
                    mapping.source_table,
                    mapping.source_field,
                    mapping.target_uri,
                    mapping.confidence
                );
                continue;
            }

            // Create manual mapping
            let manual_mapping = ManualFieldMapping {
                id: uuid::Uuid::new_v4().to_string(),
                source_context: SourceContext {
                    source_id: Some(batch_file.source_vendor.clone()),
                    table_name: mapping.source_table.clone(),
                    field_name: mapping.source_field.clone(),
                    field_metadata: None,
                },
                target_field_uri: mapping.target_uri.clone(),
                confidence: mapping.confidence,
                notes: mapping.notes.clone(),
                created_by: created_by.to_string(),
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
                usage_stats: UsageStats::default(),
            };

            // Save to store
            manual_store
                .store_mapping(manual_mapping)
                .await
                .context(format!(
                    "Failed to save manual mapping: {}.{} -> {}",
                    mapping.source_table, mapping.source_field, mapping.target_uri
                ))?;

            loaded_count += 1;
        }

        Ok(loaded_count)
    }

    /// List all registered vendors
    pub async fn list_vendors(&self) -> Result<Vec<VendorOntologyMetadata>> {
        // TODO: Query OntologyRegistry for all vendor ontologies
        // For now, return empty list
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::governance::rdf_store::GraphicaRdfStore;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_vendor_ontology_loader_basic() -> Result<()> {
        let temp_dir = TempDir::new()?;

        // Create vendor directory structure
        let vendor_dir = temp_dir.path().join("vendors");
        let oracle_dir = vendor_dir.join("oracle_test");
        std::fs::create_dir_all(&oracle_dir)?;

        // Write metadata.json
        let metadata = VendorOntologyMetadata {
            vendor_id: "oracle_test".to_string(),
            display_name: "Oracle Test".to_string(),
            version: "1.0".to_string(),
            modules: vec!["TEST".to_string()],
            table_count: 1,
            field_count: 2,
            namespace: "http://test.example.com/oracle#".to_string(),
            documentation_url: None,
        };

        std::fs::write(
            oracle_dir.join("metadata.json"),
            serde_json::to_string_pretty(&metadata)?,
        )?;

        // Write ontology.ttl
        let ontology = r#"
@prefix test: <http://test.example.com/oracle#> .
@prefix sh: <http://www.w3.org/ns/shacl#> .

test:TestTable a sh:NodeShape ;
    sh:property [
        sh:path test:field1 ;
        sh:datatype xsd:string ;
    ] .
"#;
        std::fs::write(oracle_dir.join("ontology.ttl"), ontology)?;

        // Setup ontology registry
        let registry_path = temp_dir.path().join("ontology_registry");
        let registry = Arc::new(PersistedOntologyRegistry::open(&registry_path).await?);

        // Setup manual mapping store
        let rdf_store: Arc<dyn RdfStore> = Arc::new(GraphicaRdfStore::new_in_memory()?);
        let manual_store = Arc::new(ManualMappingStore::new(
            rdf_store,
            temp_dir.path().to_str().unwrap(),
        )?);

        // Load vendor
        let loader = VendorOntologyLoader::new(registry).with_manual_store(manual_store);

        let count = loader.load_all_vendors(&vendor_dir).await?;

        assert_eq!(count, 1, "Should load 1 vendor");

        Ok(())
    }

    #[tokio::test]
    async fn test_batch_mapping_file_parsing() -> Result<()> {
        let json = r#"
{
  "source_vendor": "oracle_ebs_r12.2",
  "target_semantic": "accounting",
  "mappings": [
    {
      "source_table": "GL_JE_HEADERS",
      "source_field": "JE_HEADER_ID",
      "target_uri": "http://graphica.io/ontology/accounting#documentIdentifier",
      "confidence": 1.0
    },
    {
      "source_table": "GL_JE_HEADERS",
      "source_field": "STATUS",
      "target_uri": "http://graphica.io/ontology/accounting#documentStatus",
      "confidence": 0.95,
      "transformation": "map_status_codes",
      "notes": "Maps Oracle status codes to semantic values"
    }
  ]
}
"#;

        let batch_file: BatchMappingFile = serde_json::from_str(json)?;

        assert_eq!(batch_file.source_vendor, "oracle_ebs_r12.2");
        assert_eq!(batch_file.target_semantic, "accounting");
        assert_eq!(batch_file.mappings.len(), 2);

        let first_mapping = &batch_file.mappings[0];
        assert_eq!(first_mapping.source_table, "GL_JE_HEADERS");
        assert_eq!(first_mapping.confidence, 1.0);

        let second_mapping = &batch_file.mappings[1];
        assert_eq!(
            second_mapping.transformation,
            Some("map_status_codes".to_string())
        );

        Ok(())
    }
}
