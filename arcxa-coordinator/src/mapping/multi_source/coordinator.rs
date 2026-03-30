//! Unified Mapping Coordinator
//!
//! This module provides orchestration logic for consolidating multiple
//! source mapping sessions into a single unified mapping targeting a
//! normalized relational database schema.

use super::storage::UnifiedMappingStorage;
use super::types::{
    ConflictResolution, MappingConflict, SourceFieldRef, TargetColumnConfig, TargetColumnRef,
    TargetDatabaseConfig, TargetTableConfig, UnifiedFieldMapping, UnifiedLoadJob,
    UnifiedLoadJobStatus, UnifiedLoadProgress, UnifiedMappingSession, UnifiedSessionStatus,
};
use crate::mapping::storage::MappingStorage;
use crate::mapping::types::{FieldApprovalStatus, MappingSession};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Coordinator for creating unified mapping sessions
pub struct UnifiedMappingCoordinator {
    /// Storage for source mapping sessions
    source_storage: Arc<MappingStorage>,

    /// Storage for unified mapping sessions
    unified_storage: Arc<UnifiedMappingStorage>,
}

/// Request to create a unified mapping session
#[derive(Debug, Clone)]
pub struct CreateUnifiedSessionRequest {
    /// Source mapping session IDs to consolidate
    pub source_session_ids: Vec<String>,

    /// Target database configuration
    pub target_database: TargetDatabaseConfig,

    /// User creating this session
    pub created_by: String,
}

/// Response from creating a unified mapping session
#[derive(Debug, Clone)]
pub struct CreateUnifiedSessionResponse {
    /// Created unified session ID
    pub session_id: String,

    /// Number of field mappings created
    pub field_mappings_count: usize,

    /// Number of conflicts detected
    pub conflicts_detected: usize,

    /// Current session status
    pub status: UnifiedSessionStatus,
}

impl UnifiedMappingCoordinator {
    /// Create a new coordinator
    pub fn new(
        source_storage: Arc<MappingStorage>,
        unified_storage: Arc<UnifiedMappingStorage>,
    ) -> Self {
        Self {
            source_storage,
            unified_storage,
        }
    }

    /// Create a unified mapping session from multiple source sessions
    ///
    /// This is the main orchestration method that:
    /// 1. Loads source mapping sessions
    /// 2. Extracts approved field mappings
    /// 3. Groups mappings by ontology term
    /// 4. Detects conflicts
    /// 5. Creates unified field mappings
    /// 6. Stores the unified session
    pub async fn create_unified_session(
        &self,
        request: CreateUnifiedSessionRequest,
    ) -> Result<CreateUnifiedSessionResponse> {
        info!(
            "Creating unified session from {} source sessions",
            request.source_session_ids.len()
        );

        // Validate source session IDs
        if request.source_session_ids.is_empty() {
            return Err(anyhow::anyhow!("No source sessions provided"));
        }

        // Load source sessions
        let source_sessions = self.load_source_sessions(&request.source_session_ids)?;

        // Extract approved field mappings
        let field_mappings_by_term =
            self.extract_field_mappings_by_ontology_term(&source_sessions)?;

        debug!(
            "Extracted {} unique ontology terms",
            field_mappings_by_term.len()
        );

        // Detect conflicts and build unified mappings
        let (unified_mappings, conflicts) = self.build_unified_mappings(
            field_mappings_by_term,
            &request.target_database,
            &source_sessions,
        )?;

        // Generate session ID
        let session_id = format!(
            "unified_{}",
            uuid::Uuid::new_v4().to_string().replace('-', "")
        );

        // Determine initial status
        let status = if conflicts.is_empty() {
            UnifiedSessionStatus::ReadyToLoad
        } else {
            UnifiedSessionStatus::ConflictsDetected
        };

        // Create unified session
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let unified_session = UnifiedMappingSession {
            id: session_id.clone(),
            source_sessions: request.source_session_ids.clone(),
            target_database: request.target_database.clone(),
            field_mappings: unified_mappings.clone(),
            conflicts: conflicts.clone(),
            status: status.clone(),
            created_at: now,
            created_by: request.created_by,
            updated_at: now,
        };

        // Store unified session
        self.unified_storage.store_session(&unified_session)?;

        info!(
            "Created unified session {}: {} field mappings, {} conflicts",
            session_id,
            unified_mappings.len(),
            conflicts.len()
        );

        Ok(CreateUnifiedSessionResponse {
            session_id,
            field_mappings_count: unified_mappings.len(),
            conflicts_detected: conflicts.len(),
            status,
        })
    }

    /// Load source mapping sessions
    fn load_source_sessions(&self, session_ids: &[String]) -> Result<Vec<MappingSession>> {
        let mut sessions = Vec::new();

        for session_id in session_ids {
            let session = self
                .source_storage
                .get_session(session_id)?
                .ok_or_else(|| anyhow::anyhow!("Source session not found: {}", session_id))?;

            sessions.push(session);
        }

        Ok(sessions)
    }

    /// Extract field mappings grouped by ontology term
    ///
    /// Returns a HashMap where:
    /// - Key: Ontology term URI
    /// - Value: Vec of (source_session_id, field_mapping_state)
    fn extract_field_mappings_by_ontology_term(
        &self,
        sessions: &[MappingSession],
    ) -> Result<HashMap<String, Vec<(String, FieldInfo)>>> {
        let mut mappings_by_term: HashMap<String, Vec<(String, FieldInfo)>> = HashMap::new();

        for session in sessions {
            for table in &session.tables {
                for field_mapping in &table.field_mappings {
                    // Only include approved or auto-approved mappings
                    if !matches!(
                        field_mapping.approval_status,
                        FieldApprovalStatus::Approved | FieldApprovalStatus::AutoApproved
                    ) {
                        continue;
                    }

                    if let Some(selected) = &field_mapping.selected_mapping {
                        let field_info = FieldInfo {
                            session_id: session.session_id.clone(),
                            datasource_id: session.source_id.clone(),
                            table_name: table.table_name.clone(),
                            field_name: field_mapping.field_name.clone(),
                            data_type: field_mapping.data_type.clone(),
                            ontology_term_uri: selected.ontology_term_uri.clone(),
                            confidence: selected.confidence,
                            transformation: selected.transformation.clone(),
                        };

                        mappings_by_term
                            .entry(selected.ontology_term_uri.clone())
                            .or_insert_with(Vec::new)
                            .push((session.session_id.clone(), field_info));
                    }
                }
            }
        }

        Ok(mappings_by_term)
    }

    /// Build unified field mappings and detect conflicts
    fn build_unified_mappings(
        &self,
        mappings_by_term: HashMap<String, Vec<(String, FieldInfo)>>,
        target_database: &TargetDatabaseConfig,
        _source_sessions: &[MappingSession],
    ) -> Result<(Vec<UnifiedFieldMapping>, Vec<MappingConflict>)> {
        let mut unified_mappings = Vec::new();
        let mut conflicts = Vec::new();

        for (ontology_term_uri, field_infos) in mappings_by_term {
            // Determine target column from ontology term
            let target_column = self.map_ontology_term_to_target_column(
                &ontology_term_uri,
                target_database,
                &field_infos,
            )?;

            // Check for conflicts (multiple sources -> same ontology term)
            if field_infos.len() > 1 {
                // Conflict detected
                debug!(
                    "Conflict detected: {} sources map to ontology term {}",
                    field_infos.len(),
                    ontology_term_uri
                );

                let conflict =
                    self.create_conflict(&ontology_term_uri, &field_infos, &target_column)?;

                conflicts.push(conflict);

                // Create unified mapping with suggested resolution
                let unified_mapping = self.create_unified_mapping_with_conflict(
                    &ontology_term_uri,
                    &field_infos,
                    &target_column,
                )?;

                unified_mappings.push(unified_mapping);
            } else {
                // No conflict - single source
                let (_, field_info) = &field_infos[0];

                let source_ref = SourceFieldRef {
                    session_id: field_info.session_id.clone(),
                    datasource_id: field_info.datasource_id.clone(),
                    table_name: field_info.table_name.clone(),
                    field_name: field_info.field_name.clone(),
                    source_data_type: field_info.data_type.clone(),
                };

                let mapping_id = format!(
                    "mapping_{}",
                    uuid::Uuid::new_v4().to_string().replace('-', "")
                );

                let unified_mapping = UnifiedFieldMapping {
                    id: mapping_id,
                    source_fields: vec![source_ref],
                    ontology_term_uri: ontology_term_uri.clone(),
                    target_column: target_column.clone(),
                    conflict_resolution: ConflictResolution::NoConflict,
                    transformation: field_info.transformation.clone(),
                    confidence: field_info.confidence,
                };

                unified_mappings.push(unified_mapping);
            }
        }

        Ok((unified_mappings, conflicts))
    }

    /// Map an ontology term to a target database column
    ///
    /// This uses a simple heuristic: extract the local name from the URI
    /// and use it as the column name. In a production system, this would
    /// consult the target database schema configuration.
    fn map_ontology_term_to_target_column(
        &self,
        ontology_term_uri: &str,
        target_database: &TargetDatabaseConfig,
        field_infos: &[(String, FieldInfo)],
    ) -> Result<TargetColumnRef> {
        // Extract local name from ontology URI and build matching candidates.
        let ontology_local_name = self.extract_local_name_from_uri(ontology_term_uri);
        let mut column_candidates = Vec::new();
        self.push_identifier_candidate(&mut column_candidates, &ontology_local_name);
        for (_, field_info) in field_infos {
            self.push_identifier_candidate(&mut column_candidates, &field_info.field_name);
        }

        // Determine data type (use first field's data type)
        let data_type = if let Some((_, field_info)) = field_infos.first() {
            self.normalize_data_type(&field_info.data_type)
        } else {
            "VARCHAR(255)".to_string()
        };

        if let Some(target_column) =
            self.find_target_column_match(target_database, &column_candidates)
        {
            return Ok(target_column);
        }

        let table_name = self.preferred_target_table_name(target_database, field_infos);
        let column_name = column_candidates
            .first()
            .cloned()
            .unwrap_or_else(|| "default_column".to_string());

        Ok(TargetColumnRef {
            table_name,
            column_name,
            data_type,
        })
    }

    /// Extract local name from ontology URI
    ///
    /// Examples:
    /// - http://schema.org/email -> email
    /// - http://example.com/ontology#name -> name
    fn extract_local_name_from_uri(&self, uri: &str) -> String {
        crate::mapping::uri_utils::extract_local_name(uri).unwrap_or_else(|| uri.to_string())
    }

    fn push_identifier_candidate(&self, candidates: &mut Vec<String>, raw: &str) {
        for candidate in [raw.trim().to_string(), self.to_snake_case(raw)] {
            if !candidate.is_empty() && !candidates.iter().any(|existing| existing == &candidate) {
                candidates.push(candidate);
            }
        }
    }

    fn preferred_target_table_name(
        &self,
        target_database: &TargetDatabaseConfig,
        field_infos: &[(String, FieldInfo)],
    ) -> String {
        if target_database.tables.len() == 1 {
            return target_database
                .tables
                .keys()
                .next()
                .cloned()
                .unwrap_or_else(|| "default_table".to_string());
        }

        field_infos
            .first()
            .map(|(_, field_info)| field_info.table_name.clone())
            .unwrap_or_else(|| "default_table".to_string())
    }

    fn find_target_column_match(
        &self,
        target_database: &TargetDatabaseConfig,
        column_candidates: &[String],
    ) -> Option<TargetColumnRef> {
        for (table_name, table_config) in &target_database.tables {
            for candidate in column_candidates {
                if let Some(column_config) = table_config.columns.get(candidate) {
                    return Some(TargetColumnRef {
                        table_name: table_name.clone(),
                        column_name: column_config.name.clone(),
                        data_type: column_config.data_type.clone(),
                    });
                }
            }

            for (configured_name, column_config) in &table_config.columns {
                let configured_key = self.normalize_identifier_key(configured_name);
                if column_candidates
                    .iter()
                    .any(|candidate| self.normalize_identifier_key(candidate) == configured_key)
                {
                    return Some(TargetColumnRef {
                        table_name: table_name.clone(),
                        column_name: column_config.name.clone(),
                        data_type: column_config.data_type.clone(),
                    });
                }
            }
        }

        None
    }

    fn normalize_identifier_key(&self, value: &str) -> String {
        value
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .flat_map(|c| c.to_lowercase())
            .collect()
    }

    fn to_snake_case(&self, value: &str) -> String {
        let mut result = String::new();
        let mut previous_was_separator = false;

        for (index, ch) in value.trim().chars().enumerate() {
            if ch.is_ascii_alphanumeric() {
                if ch.is_ascii_uppercase() {
                    if index > 0 && !previous_was_separator && !result.ends_with('_') {
                        result.push('_');
                    }
                    result.push(ch.to_ascii_lowercase());
                } else {
                    result.push(ch.to_ascii_lowercase());
                }
                previous_was_separator = false;
            } else if !result.is_empty() && !result.ends_with('_') {
                result.push('_');
                previous_was_separator = true;
            }
        }

        result.trim_matches('_').to_string()
    }

    /// Normalize data type to SQL standard
    fn normalize_data_type(&self, source_type: &str) -> String {
        let upper = source_type.to_uppercase();
        match upper.as_str() {
            "TEXT" | "STRING" | "VARCHAR" => "VARCHAR(255)".to_string(),
            "INT" | "INTEGER" | "BIGINT" => "INTEGER".to_string(),
            "FLOAT" | "DOUBLE" | "REAL" => "DECIMAL".to_string(),
            "BOOL" | "BOOLEAN" => "BOOLEAN".to_string(),
            "DATE" => "DATE".to_string(),
            "TIMESTAMP" | "DATETIME" => "TIMESTAMP".to_string(),
            _ => source_type.to_string(),
        }
    }

    /// Create a conflict entry
    fn create_conflict(
        &self,
        ontology_term_uri: &str,
        field_infos: &[(String, FieldInfo)],
        target_column: &TargetColumnRef,
    ) -> Result<MappingConflict> {
        let conflict_id = format!(
            "conflict_{}",
            uuid::Uuid::new_v4().to_string().replace('-', "")
        );

        let conflicting_sources: Vec<SourceFieldRef> = field_infos
            .iter()
            .map(|(_, info)| SourceFieldRef {
                session_id: info.session_id.clone(),
                datasource_id: info.datasource_id.clone(),
                table_name: info.table_name.clone(),
                field_name: info.field_name.clone(),
                source_data_type: info.data_type.clone(),
            })
            .collect();

        // Suggest resolution: use the field with highest confidence as primary
        let primary_source = field_infos
            .iter()
            .max_by(|(_, a), (_, b)| a.confidence.partial_cmp(&b.confidence).unwrap())
            .map(|(_, info)| {
                format!(
                    "{}.{}.{}",
                    info.datasource_id, info.table_name, info.field_name
                )
            })
            .unwrap_or_else(|| "unknown".to_string());

        Ok(MappingConflict {
            id: conflict_id,
            ontology_term_uri: ontology_term_uri.to_string(),
            conflicting_sources,
            target_column: target_column.clone(),
            suggested_resolution: ConflictResolution::UsePrimary { primary_source },
            resolved: false,
        })
    }

    /// Create a unified mapping with conflict
    fn create_unified_mapping_with_conflict(
        &self,
        ontology_term_uri: &str,
        field_infos: &[(String, FieldInfo)],
        target_column: &TargetColumnRef,
    ) -> Result<UnifiedFieldMapping> {
        let mapping_id = format!(
            "mapping_{}",
            uuid::Uuid::new_v4().to_string().replace('-', "")
        );

        let source_fields: Vec<SourceFieldRef> = field_infos
            .iter()
            .map(|(_, info)| SourceFieldRef {
                session_id: info.session_id.clone(),
                datasource_id: info.datasource_id.clone(),
                table_name: info.table_name.clone(),
                field_name: info.field_name.clone(),
                source_data_type: info.data_type.clone(),
            })
            .collect();

        // Use highest confidence as the overall confidence
        let confidence = field_infos
            .iter()
            .map(|(_, info)| info.confidence)
            .fold(0.0_f64, f64::max);

        // Suggest UsePrimary resolution
        let primary_source = field_infos
            .iter()
            .max_by(|(_, a), (_, b)| a.confidence.partial_cmp(&b.confidence).unwrap())
            .map(|(_, info)| {
                format!(
                    "{}.{}.{}",
                    info.datasource_id, info.table_name, info.field_name
                )
            })
            .unwrap_or_else(|| "unknown".to_string());

        Ok(UnifiedFieldMapping {
            id: mapping_id,
            source_fields,
            ontology_term_uri: ontology_term_uri.to_string(),
            target_column: target_column.clone(),
            conflict_resolution: ConflictResolution::UsePrimary { primary_source },
            transformation: None,
            confidence,
        })
    }

    /// Get a unified session by ID
    pub fn get_unified_session(&self, session_id: &str) -> Result<Option<UnifiedMappingSession>> {
        self.unified_storage.get_session(session_id)
    }

    /// List all unified sessions
    pub fn list_unified_sessions(
        &self,
        limit: Option<usize>,
    ) -> Result<Vec<UnifiedMappingSession>> {
        self.unified_storage.list_sessions(limit)
    }

    /// Update a unified session status.
    pub fn update_unified_session_status(
        &self,
        session_id: &str,
        status: UnifiedSessionStatus,
    ) -> Result<()> {
        self.unified_storage
            .update_session_status(session_id, status)
    }

    /// Persist a fully updated unified session.
    pub fn update_unified_session(&self, session: &UnifiedMappingSession) -> Result<()> {
        self.unified_storage.update_session(session)
    }

    /// Create and persist a load job for a unified session.
    pub fn create_load_job(&self, session_id: &str, database_type: &str) -> Result<UnifiedLoadJob> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let job = UnifiedLoadJob {
            id: format!("loadjob_{}", uuid::Uuid::new_v4().simple()),
            session_id: session_id.to_string(),
            database_type: database_type.to_string(),
            status: UnifiedLoadJobStatus::Queued,
            progress: UnifiedLoadProgress::default(),
            started_at: None,
            completed_at: None,
            error_message: None,
            external_run_id: None,
            created_at: now,
            updated_at: now,
        };
        self.unified_storage.store_load_job(&job)?;
        Ok(job)
    }

    /// Fetch a load job.
    pub fn get_load_job(&self, job_id: &str) -> Result<Option<UnifiedLoadJob>> {
        self.unified_storage.get_load_job(job_id)
    }

    /// Update load job status and optional details.
    pub fn update_load_job_status(
        &self,
        job_id: &str,
        status: UnifiedLoadJobStatus,
        error_message: Option<String>,
        external_run_id: Option<String>,
    ) -> Result<()> {
        self.unified_storage
            .update_load_job_status(job_id, status, error_message, external_run_id)
    }

    /// Update load job progress.
    pub fn update_load_job_progress(
        &self,
        job_id: &str,
        progress: UnifiedLoadProgress,
    ) -> Result<()> {
        self.unified_storage
            .update_load_job_progress(job_id, progress)
    }

    /// Delete a unified session
    pub fn delete_unified_session(&self, session_id: &str) -> Result<()> {
        self.unified_storage.delete_session(session_id)
    }
}

/// Field information extracted from source session
#[derive(Debug, Clone)]
struct FieldInfo {
    session_id: String,
    datasource_id: String,
    table_name: String,
    field_name: String,
    data_type: String,
    ontology_term_uri: String,
    confidence: f64,
    transformation: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::types::FieldMappingState;
    use crate::mapping::types::{
        MappingSessionConfig, MappingSessionStatus, MappingSessionSummary, SelectedMapping,
        TableMapping,
    };
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn create_test_coordinator() -> Result<(UnifiedMappingCoordinator, TempDir, TempDir)> {
        let source_dir = TempDir::new()?;
        let unified_dir = TempDir::new()?;

        let source_storage = Arc::new(MappingStorage::new(source_dir.path().to_str().unwrap())?);
        let unified_storage = Arc::new(UnifiedMappingStorage::new(
            unified_dir.path().to_str().unwrap(),
        )?);

        let coordinator = UnifiedMappingCoordinator::new(source_storage, unified_storage);

        Ok((coordinator, source_dir, unified_dir))
    }

    fn create_test_mapping_session(
        session_id: &str,
        source_id: &str,
        field_name: &str,
        ontology_term: &str,
        confidence: f64,
    ) -> MappingSession {
        MappingSession {
            session_id: session_id.to_string(),
            source_id: source_id.to_string(),
            status: MappingSessionStatus::Active,
            tables: vec![TableMapping {
                table_name: "test_table".to_string(),
                field_mappings: vec![FieldMappingState {
                    field_id: format!("{}_field", source_id),
                    field_name: field_name.to_string(),
                    data_type: "VARCHAR".to_string(),
                    sample_values: vec!["test".to_string()],
                    candidates: vec![],
                    selected_mapping: Some(SelectedMapping {
                        ontology_term_uri: ontology_term.to_string(),
                        confidence,
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

    #[tokio::test]
    async fn test_create_unified_session_single_source() -> Result<()> {
        let (coordinator, _source_dir, _unified_dir) = create_test_coordinator()?;

        // Create and store a source session
        let source_session = create_test_mapping_session(
            "session_001",
            "csv_001",
            "email",
            "http://schema.org/email",
            0.95,
        );
        coordinator.source_storage.store_session(&source_session)?;

        // Create target database config
        let target_database = TargetDatabaseConfig {
            datasource_id: "target_postgres".to_string(),
            schema: "public".to_string(),
            tables: HashMap::new(),
        };

        // Create unified session
        let request = CreateUnifiedSessionRequest {
            source_session_ids: vec!["session_001".to_string()],
            target_database,
            created_by: "test_user".to_string(),
        };

        let response = coordinator.create_unified_session(request).await?;

        assert_eq!(response.field_mappings_count, 1);
        assert_eq!(response.conflicts_detected, 0);
        assert!(matches!(response.status, UnifiedSessionStatus::ReadyToLoad));

        Ok(())
    }

    #[tokio::test]
    async fn test_create_unified_session_no_conflicts() -> Result<()> {
        let (coordinator, _source_dir, _unified_dir) = create_test_coordinator()?;

        // Create two source sessions with different ontology terms
        let session1 = create_test_mapping_session(
            "session_001",
            "csv_001",
            "email",
            "http://schema.org/email",
            0.95,
        );
        let session2 = create_test_mapping_session(
            "session_002",
            "csv_002",
            "name",
            "http://schema.org/name",
            0.90,
        );

        coordinator.source_storage.store_session(&session1)?;
        coordinator.source_storage.store_session(&session2)?;

        let target_database = TargetDatabaseConfig {
            datasource_id: "target_postgres".to_string(),
            schema: "public".to_string(),
            tables: HashMap::new(),
        };

        let request = CreateUnifiedSessionRequest {
            source_session_ids: vec!["session_001".to_string(), "session_002".to_string()],
            target_database,
            created_by: "test_user".to_string(),
        };

        let response = coordinator.create_unified_session(request).await?;

        assert_eq!(response.field_mappings_count, 2);
        assert_eq!(response.conflicts_detected, 0);
        assert!(matches!(response.status, UnifiedSessionStatus::ReadyToLoad));

        Ok(())
    }

    #[tokio::test]
    async fn test_create_unified_session_with_conflict() -> Result<()> {
        let (coordinator, _source_dir, _unified_dir) = create_test_coordinator()?;

        // Create two source sessions mapping to SAME ontology term
        let session1 = create_test_mapping_session(
            "session_001",
            "csv_001",
            "email",
            "http://schema.org/email",
            0.95,
        );
        let session2 = create_test_mapping_session(
            "session_002",
            "csv_002",
            "customer_email",
            "http://schema.org/email", // SAME term
            0.90,
        );

        coordinator.source_storage.store_session(&session1)?;
        coordinator.source_storage.store_session(&session2)?;

        let target_database = TargetDatabaseConfig {
            datasource_id: "target_postgres".to_string(),
            schema: "public".to_string(),
            tables: HashMap::new(),
        };

        let request = CreateUnifiedSessionRequest {
            source_session_ids: vec!["session_001".to_string(), "session_002".to_string()],
            target_database,
            created_by: "test_user".to_string(),
        };

        let response = coordinator.create_unified_session(request).await?;

        // Should have 1 unified mapping (for the ontology term)
        assert_eq!(response.field_mappings_count, 1);

        // Should detect 1 conflict
        assert_eq!(response.conflicts_detected, 1);

        // Status should be ConflictsDetected
        assert!(matches!(
            response.status,
            UnifiedSessionStatus::ConflictsDetected
        ));

        // Verify unified session was stored
        let unified_session = coordinator.get_unified_session(&response.session_id)?;
        assert!(unified_session.is_some());

        let unified_session = unified_session.unwrap();
        assert_eq!(unified_session.field_mappings.len(), 1);
        assert_eq!(unified_session.conflicts.len(), 1);

        // Verify conflict details
        let conflict = &unified_session.conflicts[0];
        assert_eq!(conflict.conflicting_sources.len(), 2);
        assert!(!conflict.resolved);

        Ok(())
    }

    #[tokio::test]
    async fn test_create_unified_session_empty_sources() -> Result<()> {
        let (coordinator, _source_dir, _unified_dir) = create_test_coordinator()?;

        let target_database = TargetDatabaseConfig {
            datasource_id: "target_postgres".to_string(),
            schema: "public".to_string(),
            tables: HashMap::new(),
        };

        let request = CreateUnifiedSessionRequest {
            source_session_ids: vec![],
            target_database,
            created_by: "test_user".to_string(),
        };

        let result = coordinator.create_unified_session(request).await;
        assert!(result.is_err());

        Ok(())
    }

    #[tokio::test]
    async fn test_create_unified_session_nonexistent_source() -> Result<()> {
        let (coordinator, _source_dir, _unified_dir) = create_test_coordinator()?;

        let target_database = TargetDatabaseConfig {
            datasource_id: "target_postgres".to_string(),
            schema: "public".to_string(),
            tables: HashMap::new(),
        };

        let request = CreateUnifiedSessionRequest {
            source_session_ids: vec!["nonexistent".to_string()],
            target_database,
            created_by: "test_user".to_string(),
        };

        let result = coordinator.create_unified_session(request).await;
        assert!(result.is_err());

        Ok(())
    }

    #[tokio::test]
    async fn test_get_unified_session() -> Result<()> {
        let (coordinator, _source_dir, _unified_dir) = create_test_coordinator()?;

        let source_session = create_test_mapping_session(
            "session_001",
            "csv_001",
            "email",
            "http://schema.org/email",
            0.95,
        );
        coordinator.source_storage.store_session(&source_session)?;

        let target_database = TargetDatabaseConfig {
            datasource_id: "target_postgres".to_string(),
            schema: "public".to_string(),
            tables: HashMap::new(),
        };

        let request = CreateUnifiedSessionRequest {
            source_session_ids: vec!["session_001".to_string()],
            target_database,
            created_by: "test_user".to_string(),
        };

        let response = coordinator.create_unified_session(request).await?;

        // Retrieve the session
        let retrieved = coordinator.get_unified_session(&response.session_id)?;
        assert!(retrieved.is_some());

        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.id, response.session_id);
        assert_eq!(retrieved.source_sessions.len(), 1);

        Ok(())
    }

    #[tokio::test]
    async fn test_list_unified_sessions() -> Result<()> {
        let (coordinator, _source_dir, _unified_dir) = create_test_coordinator()?;

        // Create two unified sessions
        for i in 1..=2 {
            let source_session = create_test_mapping_session(
                &format!("session_{:03}", i),
                &format!("csv_{:03}", i),
                "email",
                "http://schema.org/email",
                0.95,
            );
            coordinator.source_storage.store_session(&source_session)?;

            let target_database = TargetDatabaseConfig {
                datasource_id: "target_postgres".to_string(),
                schema: "public".to_string(),
                tables: HashMap::new(),
            };

            let request = CreateUnifiedSessionRequest {
                source_session_ids: vec![format!("session_{:03}", i)],
                target_database,
                created_by: "test_user".to_string(),
            };

            coordinator.create_unified_session(request).await?;
        }

        // List all sessions
        let sessions = coordinator.list_unified_sessions(None)?;
        assert_eq!(sessions.len(), 2);

        Ok(())
    }

    #[tokio::test]
    async fn test_delete_unified_session() -> Result<()> {
        let (coordinator, _source_dir, _unified_dir) = create_test_coordinator()?;

        let source_session = create_test_mapping_session(
            "session_001",
            "csv_001",
            "email",
            "http://schema.org/email",
            0.95,
        );
        coordinator.source_storage.store_session(&source_session)?;

        let target_database = TargetDatabaseConfig {
            datasource_id: "target_postgres".to_string(),
            schema: "public".to_string(),
            tables: HashMap::new(),
        };

        let request = CreateUnifiedSessionRequest {
            source_session_ids: vec!["session_001".to_string()],
            target_database,
            created_by: "test_user".to_string(),
        };

        let response = coordinator.create_unified_session(request).await?;

        // Delete the session
        coordinator.delete_unified_session(&response.session_id)?;

        // Verify deletion
        let retrieved = coordinator.get_unified_session(&response.session_id)?;
        assert!(retrieved.is_none());

        Ok(())
    }

    #[test]
    fn test_extract_local_name_from_uri() -> Result<()> {
        let (coordinator, _source_dir, _unified_dir) = create_test_coordinator()?;

        assert_eq!(
            coordinator.extract_local_name_from_uri("http://schema.org/email"),
            "email"
        );
        assert_eq!(
            coordinator.extract_local_name_from_uri("http://example.com/ontology#name"),
            "name"
        );
        assert_eq!(
            coordinator.extract_local_name_from_uri("http://purl.org/dc/terms/creator"),
            "creator"
        );

        Ok(())
    }

    #[test]
    fn test_normalize_data_type() -> Result<()> {
        let (coordinator, _source_dir, _unified_dir) = create_test_coordinator()?;

        assert_eq!(coordinator.normalize_data_type("TEXT"), "VARCHAR(255)");
        assert_eq!(coordinator.normalize_data_type("STRING"), "VARCHAR(255)");
        assert_eq!(coordinator.normalize_data_type("INT"), "INTEGER");
        assert_eq!(coordinator.normalize_data_type("BIGINT"), "INTEGER");
        assert_eq!(coordinator.normalize_data_type("FLOAT"), "DECIMAL");
        assert_eq!(coordinator.normalize_data_type("DOUBLE"), "DECIMAL");
        assert_eq!(coordinator.normalize_data_type("BOOL"), "BOOLEAN");
        assert_eq!(coordinator.normalize_data_type("DATE"), "DATE");
        assert_eq!(coordinator.normalize_data_type("TIMESTAMP"), "TIMESTAMP");

        Ok(())
    }

    #[tokio::test]
    async fn test_create_unified_session_prefers_configured_target_table_and_column() -> Result<()>
    {
        let (coordinator, _source_dir, _unified_dir) = create_test_coordinator()?;

        let source_session = create_test_mapping_session(
            "session_001",
            "csv_001",
            "customer_id",
            "http://example.org/ontology#customerId",
            0.95,
        );
        coordinator.source_storage.store_session(&source_session)?;

        let mut target_columns = HashMap::new();
        target_columns.insert(
            "customer_id".to_string(),
            TargetColumnConfig {
                name: "customer_id".to_string(),
                data_type: "TEXT".to_string(),
                nullable: false,
                is_primary_key: true,
                default_value: None,
            },
        );

        let mut target_tables = HashMap::new();
        target_tables.insert(
            "arcxa_mcp_unified_target".to_string(),
            TargetTableConfig {
                name: "arcxa_mcp_unified_target".to_string(),
                columns: target_columns,
                primary_keys: vec!["customer_id".to_string()],
                foreign_keys: vec![],
            },
        );

        let target_database = TargetDatabaseConfig {
            datasource_id: "target_postgres".to_string(),
            schema: "public".to_string(),
            tables: target_tables,
        };

        let request = CreateUnifiedSessionRequest {
            source_session_ids: vec!["session_001".to_string()],
            target_database,
            created_by: "test_user".to_string(),
        };

        let response = coordinator.create_unified_session(request).await?;
        let unified_session = coordinator
            .get_unified_session(&response.session_id)?
            .expect("stored unified session");

        assert_eq!(unified_session.field_mappings.len(), 1);
        assert_eq!(
            unified_session.field_mappings[0].target_column.table_name,
            "arcxa_mcp_unified_target"
        );
        assert_eq!(
            unified_session.field_mappings[0].target_column.column_name,
            "customer_id"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_three_source_sessions_mixed_conflicts() -> Result<()> {
        let (coordinator, _source_dir, _unified_dir) = create_test_coordinator()?;

        // Session 1: email -> schema.org/email
        let mut session1 = create_test_mapping_session(
            "session_001",
            "csv_001",
            "email",
            "http://schema.org/email",
            0.95,
        );

        // Session 2: customer_email -> schema.org/email (CONFLICT)
        //            name -> schema.org/name (unique)
        let mut session2 = create_test_mapping_session(
            "session_002",
            "csv_002",
            "customer_email",
            "http://schema.org/email",
            0.90,
        );
        session2.tables[0].field_mappings.push(FieldMappingState {
            field_id: "csv_002_name_field".to_string(),
            field_name: "name".to_string(),
            data_type: "VARCHAR".to_string(),
            sample_values: vec!["John".to_string()],
            candidates: vec![],
            selected_mapping: Some(SelectedMapping {
                ontology_term_uri: "http://schema.org/name".to_string(),
                confidence: 0.92,
                was_top_candidate: true,
                transformation: None,
            }),
            approval_status: FieldApprovalStatus::Approved,
            reviewed_by: None,
            reviewed_at: None,
            notes: None,
        });

        // Session 3: price -> schema.org/price (unique)
        let session3 = create_test_mapping_session(
            "session_003",
            "csv_003",
            "price",
            "http://schema.org/price",
            0.88,
        );

        coordinator.source_storage.store_session(&session1)?;
        coordinator.source_storage.store_session(&session2)?;
        coordinator.source_storage.store_session(&session3)?;

        let target_database = TargetDatabaseConfig {
            datasource_id: "target_postgres".to_string(),
            schema: "public".to_string(),
            tables: HashMap::new(),
        };

        let request = CreateUnifiedSessionRequest {
            source_session_ids: vec![
                "session_001".to_string(),
                "session_002".to_string(),
                "session_003".to_string(),
            ],
            target_database,
            created_by: "test_user".to_string(),
        };

        let response = coordinator.create_unified_session(request).await?;

        // Should have 3 unified mappings (email, name, price)
        assert_eq!(response.field_mappings_count, 3);

        // Should have 1 conflict (email)
        assert_eq!(response.conflicts_detected, 1);

        // Verify unified session
        let unified_session = coordinator
            .get_unified_session(&response.session_id)?
            .unwrap();

        // Find the email mapping (has conflict)
        let email_mapping = unified_session
            .field_mappings
            .iter()
            .find(|m| m.ontology_term_uri == "http://schema.org/email")
            .expect("Email mapping should exist");

        assert_eq!(email_mapping.source_fields.len(), 2); // Two sources
        assert!(matches!(
            email_mapping.conflict_resolution,
            ConflictResolution::UsePrimary { .. }
        ));

        // Find the name mapping (no conflict)
        let name_mapping = unified_session
            .field_mappings
            .iter()
            .find(|m| m.ontology_term_uri == "http://schema.org/name")
            .expect("Name mapping should exist");

        assert_eq!(name_mapping.source_fields.len(), 1);
        assert!(matches!(
            name_mapping.conflict_resolution,
            ConflictResolution::NoConflict
        ));

        Ok(())
    }
}
