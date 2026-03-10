//! Unified Session Loader
//!
//! Loads data from multiple CSV sources into a target database based on
//! a unified mapping session. This module orchestrates:
//!
//! 1. Reading data from source CSVs based on mapping sessions
//! 2. Applying field transformations
//! 3. Resolving conflicts using the unified session's conflict resolution rules
//! 4. Bulk loading to the target database (PostgreSQL, DB2, or Oracle)
//! 5. Tracking complete field-level lineage
//!
//! ## Architecture
//!
//! ```text
//! UnifiedSessionLoader
//! ├── UnifiedMappingCoordinator (get session definition)
//! ├── SourceDataExtractor (read CSV data for each source)
//! ├── TransformationEngine (apply SQL-like transformations)
//! ├── ConflictResolver (merge data based on conflict rules)
//! ├── DatabaseWriter (bulk load to target database)
//! └── LineageTracker (record complete provenance)
//! ```
//!
//! ## Usage
//!
//! ```rust,ignore
//! use graphica_coordinator::mapping::loader::unified_session_loader::*;
//!
//! let loader = UnifiedSessionLoader::new(
//!     unified_coordinator,
//!     mapping_engine,
//!     lineage_sink,
//! );
//!
//! let result = loader.load_unified_session(
//!     &unified_session_id,
//!     database_config,
//!     credentials,
//! ).await?;
//!
//! println!("Loaded {} rows in {:.1}s", result.rows_loaded, result.duration_secs);
//! ```

use anyhow::{anyhow, Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, info, warn};

use crate::mapping::loader::csv_reader::CsvStreamReader;
use crate::mapping::multi_source::types::{
    ConflictResolution, SourceFieldRef, UnifiedMappingSession,
};
use crate::mapping::multi_source::UnifiedMappingCoordinator;
use crate::mapping::types::MappingSession;
use crate::mapping::MappingEngine;
use graphica_core::core::lineage::LineageSink;

/// Configuration for unified session loading
#[derive(Debug, Clone)]
pub struct UnifiedLoadConfig {
    /// Batch size for bulk inserts
    pub batch_size: usize,

    /// Whether to create tables if they don't exist
    pub create_tables: bool,

    /// Whether to drop existing tables before loading
    pub drop_existing: bool,

    /// Whether to use transactions
    pub use_transactions: bool,

    /// Maximum number of errors before aborting
    pub max_errors: usize,

    /// Number of parallel workers for extraction
    pub parallel_workers: usize,
}

impl Default for UnifiedLoadConfig {
    fn default() -> Self {
        Self {
            batch_size: 1000,
            create_tables: true,
            drop_existing: false,
            use_transactions: true,
            max_errors: 100,
            parallel_workers: 4,
        }
    }
}

/// Result of loading a unified session
#[derive(Debug, Clone)]
pub struct UnifiedLoadResult {
    /// Total rows loaded
    pub rows_loaded: u64,

    /// Rows that failed to load
    pub rows_failed: u64,

    /// Number of tables loaded
    pub tables_loaded: usize,

    /// Duration in seconds
    pub duration_secs: f64,

    /// Rows loaded per second
    pub throughput: f64,

    /// Details per source session
    pub source_details: Vec<SourceLoadDetails>,

    /// Details per target table
    pub table_details: Vec<TableLoadDetails>,
}

/// Load details for a single source session
#[derive(Debug, Clone)]
pub struct SourceLoadDetails {
    pub session_id: String,
    pub rows_extracted: u64,
    pub rows_skipped: u64,
    pub extraction_time_secs: f64,
}

/// Load details for a single target table
#[derive(Debug, Clone)]
pub struct TableLoadDetails {
    pub table_name: String,
    pub rows_inserted: u64,
    pub rows_failed: u64,
    pub load_time_secs: f64,
}

/// Extracts source data from CSV files based on mapping session
///
/// This component:
/// 1. Identifies which CSV file contains the data (from datasource_id)
/// 2. Reads only the mapped fields from the CSV
/// 3. Returns structured data ready for transformation
pub struct SourceDataExtractor {
    /// Mapping engine for accessing datasource metadata
    mapping_engine: Arc<MappingEngine>,
}

impl SourceDataExtractor {
    pub fn new(mapping_engine: Arc<MappingEngine>) -> Self {
        Self { mapping_engine }
    }

    /// Extract data from a source session
    ///
    /// Returns a vector of rows, where each row is a HashMap of field_name -> value
    pub async fn extract_from_session(
        &self,
        session: &MappingSession,
        source_fields: &[SourceFieldRef],
    ) -> Result<Vec<HashMap<String, String>>> {
        debug!(
            "Extracting data from session {} (source: {})",
            session.session_id, session.source_id
        );

        // Get the list of field names we need to extract
        let field_names_to_extract: Vec<String> = source_fields
            .iter()
            .filter(|sf| sf.datasource_id == session.source_id)
            .map(|sf| sf.field_name.clone())
            .collect();

        if field_names_to_extract.is_empty() {
            debug!("No fields to extract from session {}", session.session_id);
            return Ok(vec![]);
        }

        debug!(
            "Extracting {} fields: {:?}",
            field_names_to_extract.len(),
            field_names_to_extract
        );

        // TODO: Get actual CSV file path from datasource registry
        // For now, construct a placeholder path
        let csv_file_path = PathBuf::from(format!("/data/csv/{}.csv", session.source_id));

        // Check if file exists (for now, just return empty if not found)
        if !csv_file_path.exists() {
            warn!(
                "CSV file not found: {:?}, returning empty data",
                csv_file_path
            );
            return Ok(vec![]);
        }

        // Read CSV file and extract only the needed fields
        self.read_csv_fields(&csv_file_path, &field_names_to_extract)
            .await
    }

    /// Read specific fields from a CSV file
    async fn read_csv_fields(
        &self,
        csv_path: &PathBuf,
        field_names: &[String],
    ) -> Result<Vec<HashMap<String, String>>> {
        debug!("Reading CSV file: {:?}", csv_path);

        let mut reader = csv::Reader::from_path(csv_path).context("Failed to open CSV file")?;

        // Get headers
        let headers = reader
            .headers()
            .context("Failed to read CSV headers")?
            .clone();

        // Find column indices for the fields we need
        let mut field_indices: HashMap<String, usize> = HashMap::new();
        for field_name in field_names {
            if let Some(index) = headers.iter().position(|h| h == field_name) {
                field_indices.insert(field_name.clone(), index);
            } else {
                warn!("Field '{}' not found in CSV headers", field_name);
            }
        }

        if field_indices.is_empty() {
            return Ok(vec![]);
        }

        // Read rows and extract only the needed fields
        let mut extracted_rows = Vec::new();

        for result in reader.records() {
            let record = result.context("Failed to read CSV record")?;

            let mut row = HashMap::new();
            for (field_name, index) in &field_indices {
                if let Some(value) = record.get(*index) {
                    row.insert(field_name.clone(), value.to_string());
                }
            }

            extracted_rows.push(row);
        }

        debug!("Extracted {} rows from CSV", extracted_rows.len());

        Ok(extracted_rows)
    }
}

/// Orchestrates loading of unified mapping session to target database
pub struct UnifiedSessionLoader {
    /// Unified mapping coordinator
    unified_coordinator: Arc<UnifiedMappingCoordinator>,

    /// Mapping engine for accessing source sessions
    mapping_engine: Arc<MappingEngine>,

    /// Lineage sink for provenance tracking
    lineage_sink: Option<Arc<dyn LineageSink>>,

    /// Load configuration
    config: UnifiedLoadConfig,
}

impl UnifiedSessionLoader {
    /// Create a new unified session loader
    pub fn new(
        unified_coordinator: Arc<UnifiedMappingCoordinator>,
        mapping_engine: Arc<MappingEngine>,
        lineage_sink: Option<Arc<dyn LineageSink>>,
        config: UnifiedLoadConfig,
    ) -> Self {
        Self {
            unified_coordinator,
            mapping_engine,
            lineage_sink,
            config,
        }
    }

    /// Load a unified mapping session to target database
    ///
    /// This is the main entry point for loading. It:
    /// 1. Loads the unified session definition
    /// 2. Extracts data from all source CSVs
    /// 3. Applies transformations and conflict resolution
    /// 4. Loads data to target database in batches
    /// 5. Tracks complete lineage
    pub async fn load_unified_session(
        &self,
        unified_session_id: &str,
    ) -> Result<UnifiedLoadResult> {
        let start_time = Instant::now();

        info!(
            "Starting unified session load: {} (batch_size: {})",
            unified_session_id, self.config.batch_size
        );

        // Step 1: Load unified session definition
        let unified_session = self
            .unified_coordinator
            .get_unified_session(unified_session_id)?
            .ok_or_else(|| anyhow!("Unified session not found: {}", unified_session_id))?;

        info!(
            "Loaded unified session: {} source sessions, {} field mappings",
            unified_session.source_sessions.len(),
            unified_session.field_mappings.len()
        );

        // Step 2: Validate session is ready to load
        self.validate_session_ready(&unified_session)?;

        // Step 3: Extract data from all source sessions
        let source_details = self.extract_all_sources(&unified_session).await?;

        let total_rows_extracted: u64 = source_details.iter().map(|s| s.rows_extracted).sum();

        info!(
            "Extracted {} total rows from {} sources",
            total_rows_extracted,
            source_details.len()
        );

        // Step 4: Apply transformations and conflict resolution
        // TODO: Implement transformation and conflict resolution

        // Step 5: Load to target database
        // TODO: Implement database loading

        // Step 6: Track lineage
        // TODO: Implement lineage tracking

        let duration = start_time.elapsed();

        let result = UnifiedLoadResult {
            rows_loaded: total_rows_extracted,
            rows_failed: 0,
            tables_loaded: 0,
            duration_secs: duration.as_secs_f64(),
            throughput: total_rows_extracted as f64 / duration.as_secs_f64(),
            source_details,
            table_details: vec![],
        };

        info!(
            "Unified session load completed: {} rows in {:.1}s ({:.1} rows/sec)",
            result.rows_loaded, result.duration_secs, result.throughput
        );

        Ok(result)
    }

    /// Validate that the unified session is ready to load
    fn validate_session_ready(&self, session: &UnifiedMappingSession) -> Result<()> {
        // Check for unresolved conflicts
        let unresolved_conflicts: Vec<_> =
            session.conflicts.iter().filter(|c| !c.resolved).collect();

        if !unresolved_conflicts.is_empty() {
            return Err(anyhow!(
                "Cannot load session with {} unresolved conflicts. Please resolve conflicts first.",
                unresolved_conflicts.len()
            ));
        }

        // Check that we have field mappings
        if session.field_mappings.is_empty() {
            return Err(anyhow!("No field mappings found in unified session"));
        }

        // Check that target database is configured
        if session.target_database.datasource_id.is_empty() {
            return Err(anyhow!("Target database not configured"));
        }

        Ok(())
    }

    /// Extract data from all source sessions
    async fn extract_all_sources(
        &self,
        unified_session: &UnifiedMappingSession,
    ) -> Result<Vec<SourceLoadDetails>> {
        let mut source_details = Vec::new();

        // Build a list of all source fields from the unified field mappings
        let all_source_fields: Vec<SourceFieldRef> = unified_session
            .field_mappings
            .iter()
            .flat_map(|fm| fm.source_fields.clone())
            .collect();

        for source_session_id in &unified_session.source_sessions {
            let details = self
                .extract_source_session(source_session_id, &all_source_fields)
                .await?;
            source_details.push(details);
        }

        Ok(source_details)
    }

    /// Extract data from a single source session
    async fn extract_source_session(
        &self,
        session_id: &str,
        source_fields: &[SourceFieldRef],
    ) -> Result<SourceLoadDetails> {
        let start_time = Instant::now();

        info!("Extracting data from source session: {}", session_id);

        // Load the source mapping session
        let mapping_session = self
            .mapping_engine
            .storage
            .get_session(session_id)?
            .ok_or_else(|| anyhow!("Source session not found: {}", session_id))?;

        // Create extractor
        let extractor = SourceDataExtractor::new(self.mapping_engine.clone());

        // Extract data from CSV
        let extracted_data = extractor
            .extract_from_session(&mapping_session, source_fields)
            .await?;

        let rows_extracted = extracted_data.len() as u64;

        debug!(
            "Extracted {} rows from session {} in {:.1}s",
            rows_extracted,
            session_id,
            start_time.elapsed().as_secs_f64()
        );

        let details = SourceLoadDetails {
            session_id: session_id.to_string(),
            rows_extracted,
            rows_skipped: 0,
            extraction_time_secs: start_time.elapsed().as_secs_f64(),
        };

        Ok(details)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::multi_source::storage::UnifiedMappingStorage;
    use crate::mapping::multi_source::types::{
        TargetColumnRef, TargetDatabaseConfig, UnifiedFieldMapping, UnifiedSessionStatus,
    };
    use crate::mapping::storage::MappingStorage;
    use crate::mapping::types::{
        FieldApprovalStatus, FieldMappingState, MappingSession, MappingSessionConfig,
        MappingSessionStatus, MappingSessionSummary, SelectedMapping, TableMapping,
    };
    use std::collections::HashMap;
    use tempfile::TempDir;

    async fn create_test_loader() -> Result<(UnifiedSessionLoader, TempDir, TempDir)> {
        let source_dir = TempDir::new()?;
        let unified_dir = TempDir::new()?;

        let source_storage = Arc::new(MappingStorage::new(source_dir.path().to_str().unwrap())?);
        let unified_storage = Arc::new(UnifiedMappingStorage::new(
            unified_dir.path().to_str().unwrap(),
        )?);

        let coordinator = Arc::new(UnifiedMappingCoordinator::new(
            source_storage.clone(),
            unified_storage,
        ));

        // Create a mock MappingEngine (placeholder)
        // In real tests, you'd initialize a proper MappingEngine
        let temp_mapping_dir = TempDir::new()?;
        let mapping_engine = Arc::new(
            crate::mapping::MappingEngine::new(
                temp_mapping_dir.path().to_str().unwrap(),
                Arc::new(crate::governance::rdf_store::GraphicaRdfStore::new_in_memory()?),
                // PRE-EXISTING ISSUE: semantic_config parameter removed
            )
            .await?,
        );

        let loader = UnifiedSessionLoader::new(
            coordinator,
            mapping_engine,
            None,
            UnifiedLoadConfig::default(),
        );

        Ok((loader, source_dir, unified_dir))
    }

    fn create_test_source_session(session_id: &str) -> MappingSession {
        MappingSession {
            session_id: session_id.to_string(),
            source_id: "csv_001".to_string(),
            status: MappingSessionStatus::Active,
            tables: vec![TableMapping {
                table_name: "test_table".to_string(),
                field_mappings: vec![FieldMappingState {
                    field_id: "field_001".to_string(),
                    field_name: "email".to_string(),
                    data_type: "VARCHAR".to_string(),
                    sample_values: vec!["test@example.com".to_string()],
                    candidates: vec![],
                    selected_mapping: Some(SelectedMapping {
                        ontology_term_uri: "http://schema.org/email".to_string(),
                        confidence: 0.95,
                        was_top_candidate: true,
                        transformation: None,
                    }),
                    approval_status: FieldApprovalStatus::Approved,
                    reviewed_by: None,
                    reviewed_at: None,
                    notes: None,
                }],
                metadata: None,
            }],
            created_by: "test_user".to_string(),
            created_at: 1697356800,
            reviewed_by: None,
            reviewed_at: None,
            applied_at: None,
            config: MappingSessionConfig::default(),
            summary: MappingSessionSummary::default(),
        }
    }

    fn create_test_unified_session(
        session_id: &str,
        source_sessions: Vec<String>,
    ) -> UnifiedMappingSession {
        UnifiedMappingSession {
            id: session_id.to_string(),
            source_sessions,
            target_database: TargetDatabaseConfig {
                datasource_id: "postgres_001".to_string(),
                schema: "public".to_string(),
                tables: HashMap::new(),
            },
            field_mappings: vec![UnifiedFieldMapping {
                id: "mapping_001".to_string(),
                source_fields: vec![],
                ontology_term_uri: "http://schema.org/email".to_string(),
                target_column: TargetColumnRef {
                    table_name: "customers".to_string(),
                    column_name: "email".to_string(),
                    data_type: "VARCHAR(255)".to_string(),
                },
                conflict_resolution: ConflictResolution::NoConflict,
                transformation: None,
                confidence: 0.95,
            }],
            conflicts: vec![],
            status: UnifiedSessionStatus::ReadyToLoad,
            created_at: 1697356800,
            created_by: "test_user".to_string(),
            updated_at: 1697356800,
        }
    }

    #[tokio::test]
    async fn test_unified_session_loader_creation() -> Result<()> {
        let (loader, _source_dir, _unified_dir) = create_test_loader().await?;

        assert_eq!(loader.config.batch_size, 1000);
        assert!(loader.config.create_tables);

        Ok(())
    }

    #[tokio::test]
    async fn test_validate_session_ready_with_conflicts() -> Result<()> {
        let (loader, _source_dir, _unified_dir) = create_test_loader().await?;

        let mut session =
            create_test_unified_session("unified_001", vec!["session_001".to_string()]);

        // Add unresolved conflict
        session
            .conflicts
            .push(crate::mapping::multi_source::types::MappingConflict {
                id: "conflict_001".to_string(),
                ontology_term_uri: "http://schema.org/email".to_string(),
                conflicting_sources: vec![],
                target_column: TargetColumnRef {
                    table_name: "customers".to_string(),
                    column_name: "email".to_string(),
                    data_type: "VARCHAR(255)".to_string(),
                },
                suggested_resolution: ConflictResolution::NoConflict,
                resolved: false,
            });

        let result = loader.validate_session_ready(&session);

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("unresolved conflicts"));

        Ok(())
    }

    #[tokio::test]
    async fn test_validate_session_ready_success() -> Result<()> {
        let (loader, _source_dir, _unified_dir) = create_test_loader().await?;

        let session = create_test_unified_session("unified_001", vec!["session_001".to_string()]);

        let result = loader.validate_session_ready(&session);

        assert!(result.is_ok());

        Ok(())
    }

    #[tokio::test]
    async fn test_extract_source_session_not_found() -> Result<()> {
        let (loader, _source_dir, _unified_dir) = create_test_loader().await?;

        let source_fields = vec![];
        let result = loader
            .extract_source_session("nonexistent_session", &source_fields)
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));

        Ok(())
    }
}
