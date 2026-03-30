//! # Mapping Storage
//!
//! RocksDB-based storage for field mappings, historical data, and indexes.
//!
//! ## Storage Layout
//!
//! - `fields` CF: field_id → SchemaField (JSON)
//! - `mappings` CF: (field_id, term_uri) → confidence
//! - `feedback` CF: feedback_id → MappingFeedback (JSON)
//! - `historical` CF: term_uri → List<HistoricalMapping> (JSON)
//! - `field_index` CF: normalized_name → List<field_id>

use anyhow::{Context, Result};
use rocksdb::{ColumnFamilyDescriptor, Options, DB};
use std::sync::Arc;

use crate::mapping::types::*;

/// Column family names
const CF_FIELDS: &str = "fields";
const CF_MAPPINGS: &str = "mappings";
const CF_FEEDBACK: &str = "feedback";
const CF_HISTORICAL: &str = "historical";
const CF_FIELD_INDEX: &str = "field_index";
const CF_SESSIONS: &str = "sessions";

/// Mapping storage using RocksDB
pub struct MappingStorage {
    db: Arc<DB>,
}

impl MappingStorage {
    /// Create a new mapping storage
    pub fn new(path: &str) -> Result<Self> {
        // Create directory if it doesn't exist
        if path != ":memory:" {
            std::fs::create_dir_all(path).context("Failed to create storage directory")?;
        }

        // Configure RocksDB options
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);

        // Define column families
        let cf_fields = ColumnFamilyDescriptor::new(CF_FIELDS, Options::default());
        let cf_mappings = ColumnFamilyDescriptor::new(CF_MAPPINGS, Options::default());
        let cf_feedback = ColumnFamilyDescriptor::new(CF_FEEDBACK, Options::default());
        let cf_historical = ColumnFamilyDescriptor::new(CF_HISTORICAL, Options::default());
        let cf_field_index = ColumnFamilyDescriptor::new(CF_FIELD_INDEX, Options::default());
        let cf_sessions = ColumnFamilyDescriptor::new(CF_SESSIONS, Options::default());

        // Open database
        let db = DB::open_cf_descriptors(
            &opts,
            path,
            vec![
                cf_fields,
                cf_mappings,
                cf_feedback,
                cf_historical,
                cf_field_index,
                cf_sessions,
            ],
        )
        .context("Failed to open RocksDB")?;

        Ok(Self { db: Arc::new(db) })
    }

    /// Store a schema field
    pub fn store_field(&self, field: &SchemaField) -> Result<()> {
        let cf = self
            .db
            .cf_handle(CF_FIELDS)
            .context("Fields CF not found")?;

        // Serialize field to JSON
        let json = serde_json::to_vec(field).context("Failed to serialize field")?;

        // Store field
        self.db
            .put_cf(cf, field.id.as_bytes(), json)
            .context("Failed to store field")?;

        // Update field index
        self.index_field(field)?;

        Ok(())
    }

    /// Get a schema field by ID
    pub fn get_field(&self, field_id: &str) -> Result<Option<SchemaField>> {
        let cf = self
            .db
            .cf_handle(CF_FIELDS)
            .context("Fields CF not found")?;

        match self.db.get_cf(cf, field_id.as_bytes())? {
            Some(bytes) => {
                let field =
                    serde_json::from_slice(&bytes).context("Failed to deserialize field")?;
                Ok(Some(field))
            }
            None => Ok(None),
        }
    }

    /// Index a field by normalized name
    fn index_field(&self, field: &SchemaField) -> Result<()> {
        let cf = self
            .db
            .cf_handle(CF_FIELD_INDEX)
            .context("Field index CF not found")?;

        // Get existing field IDs for this name
        let mut field_ids: Vec<String> =
            match self.db.get_cf(cf, field.normalized_name.as_bytes())? {
                Some(bytes) => {
                    serde_json::from_slice(&bytes).context("Failed to deserialize field index")?
                }
                None => vec![],
            };

        // Add this field ID if not already present
        if !field_ids.contains(&field.id) {
            field_ids.push(field.id.clone());
        }

        // Serialize and store
        let json = serde_json::to_vec(&field_ids).context("Failed to serialize field index")?;

        self.db
            .put_cf(cf, field.normalized_name.as_bytes(), json)
            .context("Failed to store field index")?;

        Ok(())
    }

    /// Find fields by normalized name
    pub fn find_fields_by_name(&self, normalized_name: &str) -> Result<Vec<SchemaField>> {
        let cf_index = self
            .db
            .cf_handle(CF_FIELD_INDEX)
            .context("Field index CF not found")?;

        let field_ids: Vec<String> = match self.db.get_cf(cf_index, normalized_name.as_bytes())? {
            Some(bytes) => {
                serde_json::from_slice(&bytes).context("Failed to deserialize field index")?
            }
            None => return Ok(vec![]),
        };

        let mut fields = Vec::new();
        for field_id in field_ids {
            if let Some(field) = self.get_field(&field_id)? {
                fields.push(field);
            }
        }

        Ok(fields)
    }

    /// Store mapping feedback
    pub fn store_feedback(&self, feedback: &MappingFeedback) -> Result<()> {
        let cf_feedback = self
            .db
            .cf_handle(CF_FEEDBACK)
            .context("Feedback CF not found")?;

        let feedback_id = format!("{}_{}", feedback.field_id, feedback.timestamp);

        // Serialize feedback
        let json = serde_json::to_vec(feedback).context("Failed to serialize feedback")?;

        // Store feedback
        self.db
            .put_cf(cf_feedback, feedback_id.as_bytes(), json)
            .context("Failed to store feedback")?;

        // Update historical mappings if a term was selected
        if let Some(term_uri) = &feedback.selected_term_uri {
            self.update_historical_mapping(feedback, term_uri)?;
        }

        Ok(())
    }

    /// Update historical mappings
    fn update_historical_mapping(&self, feedback: &MappingFeedback, term_uri: &str) -> Result<()> {
        let cf = self
            .db
            .cf_handle(CF_HISTORICAL)
            .context("Historical CF not found")?;

        // Get existing historical mappings for this term
        let mut historical: Vec<HistoricalMapping> =
            match self.db.get_cf(cf, term_uri.as_bytes())? {
                Some(bytes) => serde_json::from_slice(&bytes)
                    .context("Failed to deserialize historical mappings")?,
                None => vec![],
            };

        // Get field name from feedback
        let field = self
            .get_field(&feedback.field_id)?
            .ok_or_else(|| anyhow::anyhow!("Field not found"))?;

        // Add new historical mapping
        let mapping = HistoricalMapping {
            source_field_name: field.name.clone(),
            ontology_term_uri: term_uri.to_string(),
            approved_by: feedback.user_id.clone(),
            approved_at: feedback.timestamp,
            similarity: 1.0, // Placeholder, will be computed on retrieval
        };

        historical.push(mapping);

        // Keep only last 100 mappings
        if historical.len() > 100 {
            historical.drain(0..historical.len() - 100);
        }

        // Serialize and store
        let json =
            serde_json::to_vec(&historical).context("Failed to serialize historical mappings")?;

        self.db
            .put_cf(cf, term_uri.as_bytes(), json)
            .context("Failed to store historical mappings")?;

        Ok(())
    }

    /// Get historical mappings for an ontology term
    pub fn get_historical_mappings(&self, term_uri: &str) -> Result<Vec<HistoricalMapping>> {
        let cf = self
            .db
            .cf_handle(CF_HISTORICAL)
            .context("Historical CF not found")?;

        match self.db.get_cf(cf, term_uri.as_bytes())? {
            Some(bytes) => {
                let historical = serde_json::from_slice(&bytes)
                    .context("Failed to deserialize historical mappings")?;
                Ok(historical)
            }
            None => Ok(vec![]),
        }
    }

    /// Store a mapping (field → ontology term with confidence)
    pub fn store_mapping(&self, field_id: &str, term_uri: &str, confidence: f64) -> Result<()> {
        let cf = self
            .db
            .cf_handle(CF_MAPPINGS)
            .context("Mappings CF not found")?;

        let key = format!("{}:{}", field_id, term_uri);

        self.db
            .put_cf(cf, key.as_bytes(), confidence.to_be_bytes())
            .context("Failed to store mapping")?;

        Ok(())
    }

    /// Get mapping confidence
    pub fn get_mapping(&self, field_id: &str, term_uri: &str) -> Result<Option<f64>> {
        let cf = self
            .db
            .cf_handle(CF_MAPPINGS)
            .context("Mappings CF not found")?;

        let key = format!("{}:{}", field_id, term_uri);

        match self.db.get_cf(cf, key.as_bytes())? {
            Some(bytes) => {
                let confidence = f64::from_be_bytes(
                    bytes
                        .try_into()
                        .map_err(|_| anyhow::anyhow!("Invalid confidence bytes"))?,
                );
                Ok(Some(confidence))
            }
            None => Ok(None),
        }
    }

    /// Get all mappings for a field
    pub fn get_field_mappings(&self, field_id: &str) -> Result<Vec<(String, f64)>> {
        let cf = self
            .db
            .cf_handle(CF_MAPPINGS)
            .context("Mappings CF not found")?;

        let prefix = format!("{}:", field_id);
        let mut mappings = Vec::new();

        let iter = self.db.prefix_iterator_cf(cf, prefix.as_bytes());

        for item in iter {
            let (key, value) = item?;

            let key_str = String::from_utf8_lossy(&key);
            if let Some(term_uri) = key_str.strip_prefix(&prefix) {
                let confidence = f64::from_be_bytes(value.as_ref().try_into().unwrap_or([0u8; 8]));
                mappings.push((term_uri.to_string(), confidence));
            }
        }

        Ok(mappings)
    }

    // ========================================================================
    // Mapping Session Storage - Phase 1 Implementation
    // ========================================================================

    /// Store a mapping session
    pub fn store_session(&self, session: &MappingSession) -> Result<()> {
        let cf = self
            .db
            .cf_handle(CF_SESSIONS)
            .context("Sessions CF not found")?;

        // Serialize session to JSON
        let json = serde_json::to_vec(session).context("Failed to serialize session")?;

        // Store session
        self.db
            .put_cf(cf, session.session_id.as_bytes(), json)
            .context("Failed to store session")?;

        Ok(())
    }

    /// Get a mapping session by ID
    pub fn get_session(&self, session_id: &str) -> Result<Option<MappingSession>> {
        let cf = self
            .db
            .cf_handle(CF_SESSIONS)
            .context("Sessions CF not found")?;

        match self.db.get_cf(cf, session_id.as_bytes())? {
            Some(bytes) => {
                let session =
                    serde_json::from_slice(&bytes).context("Failed to deserialize session")?;
                Ok(Some(session))
            }
            None => Ok(None),
        }
    }

    /// List mapping sessions with optional filters
    pub fn list_sessions(
        &self,
        source_id: Option<&str>,
        status: Option<MappingSessionStatus>,
    ) -> Result<Vec<MappingSession>> {
        let cf = self
            .db
            .cf_handle(CF_SESSIONS)
            .context("Sessions CF not found")?;

        let mut sessions = Vec::new();

        // Iterate through all sessions
        let iter = self.db.iterator_cf(cf, rocksdb::IteratorMode::Start);

        for item in iter {
            let (_, value) = item?;

            let session: MappingSession =
                serde_json::from_slice(&value).context("Failed to deserialize session")?;

            // Apply filters
            if let Some(src_id) = source_id {
                if session.source_id != src_id {
                    continue;
                }
            }

            if let Some(target_status) = status {
                if session.status != target_status {
                    continue;
                }
            }

            sessions.push(session);
        }

        // Sort by creation time (newest first)
        sessions.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        Ok(sessions)
    }

    /// Update session status
    pub fn update_session_status(
        &self,
        session_id: &str,
        new_status: MappingSessionStatus,
    ) -> Result<()> {
        let mut session = self
            .get_session(session_id)?
            .ok_or_else(|| anyhow::anyhow!("Session not found: {}", session_id))?;

        // Validate state transition
        if !session.status.can_transition_to(new_status) {
            return Err(anyhow::anyhow!(
                "Invalid status transition: {:?} -> {:?}",
                session.status,
                new_status
            ));
        }

        session.status = new_status;

        // Update timestamps based on status
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        match new_status {
            MappingSessionStatus::Applied => {
                session.applied_at = Some(now);
            }
            _ => {}
        }

        self.store_session(&session)?;

        Ok(())
    }

    /// Update a field mapping within a session
    pub fn update_field_mapping(
        &self,
        session_id: &str,
        field_id: &str,
        approval_status: FieldApprovalStatus,
        selected_mapping: Option<SelectedMapping>,
        reviewed_by: Option<String>,
        notes: Option<String>,
    ) -> Result<()> {
        let mut session = self
            .get_session(session_id)?
            .ok_or_else(|| anyhow::anyhow!("Session not found: {}", session_id))?;

        // Find the field mapping
        let mut found = false;
        for table in &mut session.tables {
            for field_mapping in &mut table.field_mappings {
                if field_mapping.field_id == field_id {
                    field_mapping.approval_status = approval_status;
                    field_mapping.selected_mapping = selected_mapping.clone();
                    field_mapping.reviewed_by = reviewed_by.clone();
                    field_mapping.notes = notes.clone();

                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs() as i64;
                    field_mapping.reviewed_at = Some(now);

                    found = true;
                    break;
                }
            }
            if found {
                break;
            }
        }

        if !found {
            return Err(anyhow::anyhow!("Field not found in session: {}", field_id));
        }

        // Update summary statistics
        session.summary = Self::compute_summary(&session);

        // Store updated session
        self.store_session(&session)?;

        Ok(())
    }

    /// Compute summary statistics for a session
    pub fn compute_summary(session: &MappingSession) -> MappingSessionSummary {
        let mut summary = MappingSessionSummary::default();

        for table in &session.tables {
            for field_mapping in &table.field_mappings {
                summary.total_fields += 1;

                if !field_mapping.candidates.is_empty() {
                    summary.fields_with_candidates += 1;
                }

                match field_mapping.approval_status {
                    FieldApprovalStatus::Pending => summary.pending_review += 1,
                    FieldApprovalStatus::AutoApproved => summary.auto_approved += 1,
                    FieldApprovalStatus::Approved => summary.user_approved += 1,
                    FieldApprovalStatus::Rejected => summary.rejected += 1,
                    FieldApprovalStatus::Modified => summary.modified += 1,
                }
            }
        }

        summary
    }

    /// Delete a mapping session
    pub fn delete_session(&self, session_id: &str) -> Result<()> {
        let cf = self
            .db
            .cf_handle(CF_SESSIONS)
            .context("Sessions CF not found")?;

        self.db
            .delete_cf(cf, session_id.as_bytes())
            .context("Failed to delete session")?;

        Ok(())
    }

    /// Record transformation statistics for a session
    ///
    /// Updates the session summary with transformation execution stats:
    /// - Increments transformations_executed count
    /// - Updates fields_used_in_transformations count
    /// - Tracks successful/failed transformation counts
    pub fn record_session_transformation(
        &self,
        session_id: &str,
        fields_used: usize,
        success: bool,
    ) -> Result<()> {
        let mut session = self
            .get_session(session_id)?
            .ok_or_else(|| anyhow::anyhow!("Session not found: {}", session_id))?;

        // Update transformation statistics
        session.summary.transformations_executed += 1;
        session.summary.fields_used_in_transformations += fields_used;

        if success {
            session.summary.successful_transformations += 1;
        } else {
            session.summary.failed_transformations += 1;
        }

        // Store updated session
        self.store_session(&session)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_field() -> SchemaField {
        SchemaField {
            id: "test_001".to_string(),
            name: "customer_email".to_string(),
            normalized_name: "customeremail".to_string(),
            data_type: "VARCHAR".to_string(),
            nullable: false,
            sample_values: vec![],
            source_id: "test_source".to_string(),
            table_name: "customers".to_string(),
            description: None,
            features: None,
        }
    }

    #[test]
    fn test_store_and_retrieve_field() {
        let temp_dir = TempDir::new().unwrap();
        let storage = MappingStorage::new(temp_dir.path().to_str().unwrap()).unwrap();

        let field = create_test_field();

        storage.store_field(&field).unwrap();

        let retrieved = storage.get_field(&field.id).unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "customer_email");
    }

    #[test]
    fn test_field_index() {
        let temp_dir = TempDir::new().unwrap();
        let storage = MappingStorage::new(temp_dir.path().to_str().unwrap()).unwrap();

        let field1 = SchemaField {
            id: "field_001".to_string(),
            name: "customer_email".to_string(),
            normalized_name: "customeremail".to_string(),
            data_type: "VARCHAR".to_string(),
            nullable: false,
            sample_values: vec![],
            source_id: "source1".to_string(),
            table_name: "customers".to_string(),
            description: None,
            features: None,
        };

        let field2 = SchemaField {
            id: "field_002".to_string(),
            name: "CustomerEmail".to_string(),
            normalized_name: "customeremail".to_string(),
            data_type: "VARCHAR".to_string(),
            nullable: false,
            sample_values: vec![],
            source_id: "source2".to_string(),
            table_name: "users".to_string(),
            description: None,
            features: None,
        };

        storage.store_field(&field1).unwrap();
        storage.store_field(&field2).unwrap();

        let fields = storage.find_fields_by_name("customeremail").unwrap();
        assert_eq!(fields.len(), 2);
    }

    #[test]
    fn test_store_mapping() {
        let temp_dir = TempDir::new().unwrap();
        let storage = MappingStorage::new(temp_dir.path().to_str().unwrap()).unwrap();

        storage
            .store_mapping("field_001", "http://schema.org/email", 0.85)
            .unwrap();

        let confidence = storage
            .get_mapping("field_001", "http://schema.org/email")
            .unwrap();
        assert!(confidence.is_some());
        assert!((confidence.unwrap() - 0.85).abs() < 0.01);
    }

    #[test]
    fn test_feedback_and_historical() {
        let temp_dir = TempDir::new().unwrap();
        let storage = MappingStorage::new(temp_dir.path().to_str().unwrap()).unwrap();

        let field = create_test_field();
        storage.store_field(&field).unwrap();

        let feedback = MappingFeedback {
            field_id: field.id.clone(),
            selected_term_uri: Some("http://schema.org/email".to_string()),
            accepted_top_suggestion: true,
            user_id: "user123".to_string(),
            notes: None,
            timestamp: 1234567890,
        };

        storage.store_feedback(&feedback).unwrap();

        let historical = storage
            .get_historical_mappings("http://schema.org/email")
            .unwrap();
        assert_eq!(historical.len(), 1);
        assert_eq!(historical[0].source_field_name, "customer_email");
    }

    #[test]
    fn test_get_field_mappings() {
        let temp_dir = TempDir::new().unwrap();
        let storage = MappingStorage::new(temp_dir.path().to_str().unwrap()).unwrap();

        storage
            .store_mapping("field_001", "http://schema.org/email", 0.85)
            .unwrap();
        storage
            .store_mapping("field_001", "http://schema.org/name", 0.50)
            .unwrap();

        let mappings = storage.get_field_mappings("field_001").unwrap();
        assert_eq!(mappings.len(), 2);
    }

    // ========================================================================
    // Mapping Session Storage Tests
    // ========================================================================

    fn create_test_session() -> MappingSession {
        MappingSession {
            session_id: "session_001".to_string(),
            source_id: "pg_source".to_string(),
            status: MappingSessionStatus::Draft,
            tables: vec![TableMapping {
                table_name: "customers".to_string(),
                field_mappings: vec![
                    FieldMappingState {
                        field_id: "field_001".to_string(),
                        field_name: "customer_email".to_string(),
                        data_type: "VARCHAR".to_string(),
                        sample_values: vec!["john@example.com".to_string()],
                        candidates: vec![],
                        selected_mapping: None,
                        approval_status: FieldApprovalStatus::Pending,
                        reviewed_by: None,
                        reviewed_at: None,
                        notes: None,
                    },
                    FieldMappingState {
                        field_id: "field_002".to_string(),
                        field_name: "customer_id".to_string(),
                        data_type: "INTEGER".to_string(),
                        sample_values: vec!["12345".to_string()],
                        candidates: vec![],
                        selected_mapping: Some(SelectedMapping {
                            ontology_term_uri: "http://schema.org/identifier".to_string(),
                            confidence: 0.98,
                            was_top_candidate: true,
                            transformation: None,
                        }),
                        approval_status: FieldApprovalStatus::AutoApproved,
                        reviewed_by: None,
                        reviewed_at: None,
                        notes: None,
                    },
                ],
                metadata: None,
            }],
            created_by: "user123".to_string(),
            created_at: 1234567890,
            reviewed_by: None,
            reviewed_at: None,
            applied_at: None,
            config: MappingSessionConfig::default(),
            summary: MappingSessionSummary::default(),
        }
    }

    #[test]
    fn test_store_and_retrieve_session() {
        let temp_dir = TempDir::new().unwrap();
        let storage = MappingStorage::new(temp_dir.path().to_str().unwrap()).unwrap();

        let session = create_test_session();
        storage.store_session(&session).unwrap();

        let retrieved = storage.get_session(&session.session_id).unwrap();
        assert!(retrieved.is_some());

        let retrieved_session = retrieved.unwrap();
        assert_eq!(retrieved_session.session_id, "session_001");
        assert_eq!(retrieved_session.source_id, "pg_source");
        assert_eq!(retrieved_session.status, MappingSessionStatus::Draft);
        assert_eq!(retrieved_session.tables.len(), 1);
        assert_eq!(retrieved_session.tables[0].field_mappings.len(), 2);
    }

    #[test]
    fn test_list_sessions() {
        let temp_dir = TempDir::new().unwrap();
        let storage = MappingStorage::new(temp_dir.path().to_str().unwrap()).unwrap();

        let mut session1 = create_test_session();
        session1.session_id = "session_001".to_string();
        session1.status = MappingSessionStatus::Draft;

        let mut session2 = create_test_session();
        session2.session_id = "session_002".to_string();
        session2.status = MappingSessionStatus::Approved;

        let mut session3 = create_test_session();
        session3.session_id = "session_003".to_string();
        session3.source_id = "mysql_source".to_string();

        storage.store_session(&session1).unwrap();
        storage.store_session(&session2).unwrap();
        storage.store_session(&session3).unwrap();

        // List all sessions
        let all = storage.list_sessions(None, None).unwrap();
        assert_eq!(all.len(), 3);

        // Filter by source
        let pg_sessions = storage.list_sessions(Some("pg_source"), None).unwrap();
        assert_eq!(pg_sessions.len(), 2);

        // Filter by status
        let approved = storage
            .list_sessions(None, Some(MappingSessionStatus::Approved))
            .unwrap();
        assert_eq!(approved.len(), 1);
        assert_eq!(approved[0].session_id, "session_002");

        // Filter by both
        let filtered = storage
            .list_sessions(Some("pg_source"), Some(MappingSessionStatus::Draft))
            .unwrap();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].session_id, "session_001");
    }

    #[test]
    fn test_update_session_status() {
        let temp_dir = TempDir::new().unwrap();
        let storage = MappingStorage::new(temp_dir.path().to_str().unwrap()).unwrap();

        let session = create_test_session();
        storage.store_session(&session).unwrap();

        // Valid transition: Draft -> PendingReview
        storage
            .update_session_status(&session.session_id, MappingSessionStatus::PendingReview)
            .unwrap();

        let updated = storage.get_session(&session.session_id).unwrap().unwrap();
        assert_eq!(updated.status, MappingSessionStatus::PendingReview);

        // Invalid transition: PendingReview -> Active (should fail)
        let result =
            storage.update_session_status(&session.session_id, MappingSessionStatus::Active);
        assert!(result.is_err());
    }

    #[test]
    fn test_update_field_mapping() {
        let temp_dir = TempDir::new().unwrap();
        let storage = MappingStorage::new(temp_dir.path().to_str().unwrap()).unwrap();

        let session = create_test_session();
        storage.store_session(&session).unwrap();

        // Update a field mapping
        let selected = SelectedMapping {
            ontology_term_uri: "http://schema.org/email".to_string(),
            confidence: 0.92,
            was_top_candidate: true,
            transformation: None,
        };

        storage
            .update_field_mapping(
                &session.session_id,
                "field_001",
                FieldApprovalStatus::Approved,
                Some(selected),
                Some("user123".to_string()),
                Some("Looks good".to_string()),
            )
            .unwrap();

        // Verify update
        let updated_session = storage.get_session(&session.session_id).unwrap().unwrap();
        let field_mapping = &updated_session.tables[0].field_mappings[0];

        assert_eq!(field_mapping.approval_status, FieldApprovalStatus::Approved);
        assert!(field_mapping.selected_mapping.is_some());
        assert_eq!(
            field_mapping
                .selected_mapping
                .as_ref()
                .unwrap()
                .ontology_term_uri,
            "http://schema.org/email"
        );
        assert_eq!(field_mapping.reviewed_by, Some("user123".to_string()));
        assert!(field_mapping.reviewed_at.is_some());
        assert_eq!(field_mapping.notes, Some("Looks good".to_string()));
    }

    #[test]
    fn test_compute_summary() {
        let temp_dir = TempDir::new().unwrap();
        let storage = MappingStorage::new(temp_dir.path().to_str().unwrap()).unwrap();

        let mut session = create_test_session();

        // Add more fields with different statuses
        session.tables[0].field_mappings.push(FieldMappingState {
            field_id: "field_003".to_string(),
            field_name: "phone".to_string(),
            data_type: "VARCHAR".to_string(),
            sample_values: vec![],
            candidates: vec![],
            selected_mapping: None,
            approval_status: FieldApprovalStatus::Rejected,
            reviewed_by: None,
            reviewed_at: None,
            notes: None,
        });

        session.tables[0].field_mappings.push(FieldMappingState {
            field_id: "field_004".to_string(),
            field_name: "address".to_string(),
            data_type: "VARCHAR".to_string(),
            sample_values: vec![],
            candidates: vec![],
            selected_mapping: None,
            approval_status: FieldApprovalStatus::Modified,
            reviewed_by: None,
            reviewed_at: None,
            notes: None,
        });

        storage.store_session(&session).unwrap();

        // Update to recompute summary
        storage
            .update_field_mapping(
                &session.session_id,
                "field_001",
                FieldApprovalStatus::Approved,
                None,
                None,
                None,
            )
            .unwrap();

        let updated = storage.get_session(&session.session_id).unwrap().unwrap();

        assert_eq!(updated.summary.total_fields, 4);
        assert_eq!(updated.summary.auto_approved, 1); // field_002
        assert_eq!(updated.summary.user_approved, 1); // field_001
        assert_eq!(updated.summary.rejected, 1); // field_003
        assert_eq!(updated.summary.modified, 1); // field_004
    }

    #[test]
    fn test_delete_session() {
        let temp_dir = TempDir::new().unwrap();
        let storage = MappingStorage::new(temp_dir.path().to_str().unwrap()).unwrap();

        let session = create_test_session();
        storage.store_session(&session).unwrap();

        // Verify it exists
        assert!(storage.get_session(&session.session_id).unwrap().is_some());

        // Delete
        storage.delete_session(&session.session_id).unwrap();

        // Verify it's gone
        assert!(storage.get_session(&session.session_id).unwrap().is_none());
    }

    #[test]
    fn test_status_transitions() {
        use MappingSessionStatus::*;

        // Valid transitions
        assert!(Draft.can_transition_to(PendingReview));
        assert!(Draft.can_transition_to(Approved));
        assert!(Draft.can_transition_to(Cancelled));
        assert!(PendingReview.can_transition_to(Approved));
        assert!(PendingReview.can_transition_to(Draft));
        assert!(PendingReview.can_transition_to(Cancelled));
        assert!(Approved.can_transition_to(Applied));
        assert!(Applied.can_transition_to(Active));

        // Invalid transitions
        assert!(!Draft.can_transition_to(Applied));
        assert!(!PendingReview.can_transition_to(Active));
        assert!(!Approved.can_transition_to(Draft));
        assert!(!Applied.can_transition_to(Draft));
        assert!(!Active.can_transition_to(Draft));
    }
}
