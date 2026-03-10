//! Unified Mapping Session
//!
//! Consolidates multiple source CSV mapping sessions into a unified target schema.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use super::types::*;
use crate::governance::rdf_store::GraphicaRdfStore;

/// Unified mapping session
///
/// Consolidates mappings from multiple CSV sources to a single target database schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedMappingSession {
    /// Unique session ID
    pub session_id: UnifiedSessionId,

    /// Session name
    pub name: String,

    /// Source mapping session IDs
    pub source_sessions: Vec<SourceSessionId>,

    /// Target database configuration
    pub target_database: TargetDatabase,

    /// Target table schemas
    pub target_schema: HashMap<String, TargetTableSchema>,

    /// Mapping rules (ontology term → target column)
    pub mapping_rules: Vec<TargetMappingRule>,

    /// Current status
    pub status: UnifiedMappingStatus,

    /// Detected conflicts
    pub conflicts: Vec<MappingConflictInfo>,

    /// Created timestamp
    pub created_at: chrono::DateTime<chrono::Utc>,

    /// Updated timestamp
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl UnifiedMappingSession {
    /// Create a new unified mapping session
    pub fn new(
        name: String,
        source_sessions: Vec<SourceSessionId>,
        target_database: TargetDatabase,
        target_schema: HashMap<String, TargetTableSchema>,
    ) -> Self {
        let now = chrono::Utc::now();
        Self {
            session_id: format!("unified_{}", Uuid::new_v4()),
            name,
            source_sessions,
            target_database,
            target_schema,
            mapping_rules: Vec::new(),
            status: UnifiedMappingStatus::PendingReview,
            conflicts: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }

    /// Add a mapping rule
    pub fn add_mapping_rule(&mut self, rule: TargetMappingRule) -> Result<()> {
        // Validate that target table exists
        if !self.target_schema.contains_key(&rule.target_table) {
            anyhow::bail!(
                "Target table {} does not exist in schema",
                rule.target_table
            );
        }

        // Validate that target column exists
        let table_schema = &self.target_schema[&rule.target_table];
        if !table_schema.columns.contains_key(&rule.target_column) {
            anyhow::bail!(
                "Target column {} does not exist in table {}",
                rule.target_column,
                rule.target_table
            );
        }

        self.mapping_rules.push(rule);
        self.updated_at = chrono::Utc::now();
        Ok(())
    }

    /// Detect mapping conflicts
    ///
    /// Identifies cases where multiple source fields map to the same ontology term
    /// but target different columns, or vice versa.
    pub fn detect_conflicts(&mut self) -> Vec<MappingConflictInfo> {
        let mut ontology_to_targets: HashMap<String, Vec<&TargetMappingRule>> = HashMap::new();

        // Group rules by ontology term
        for rule in &self.mapping_rules {
            ontology_to_targets
                .entry(rule.ontology_term.clone())
                .or_insert_with(Vec::new)
                .push(rule);
        }

        let mut conflicts = Vec::new();

        // Check for conflicts
        for (ontology_term, rules) in ontology_to_targets {
            if rules.len() > 1 {
                // Multiple rules for same ontology term
                let target_columns: Vec<String> = rules
                    .iter()
                    .map(|r| format!("{}.{}", r.target_table, r.target_column))
                    .collect();

                // Only a conflict if they map to different targets
                let unique_targets: std::collections::HashSet<_> = target_columns.iter().collect();
                if unique_targets.len() > 1 {
                    conflicts.push(MappingConflictInfo {
                        ontology_term: ontology_term.clone(),
                        conflicting_fields: Vec::new(), // Will be populated from source sessions
                        suggested_target_column: rules[0].target_column.clone(),
                        suggested_resolution: format!(
                            "Multiple mappings found: {}. Suggest merging into {}",
                            target_columns.join(", "),
                            target_columns[0]
                        ),
                    });
                }
            }
        }

        self.conflicts = conflicts.clone();
        self.updated_at = chrono::Utc::now();
        conflicts
    }

    /// Get mapping rule for an ontology term
    pub fn get_mapping_for_term(&self, ontology_term: &str) -> Option<&TargetMappingRule> {
        self.mapping_rules
            .iter()
            .find(|r| r.ontology_term == ontology_term)
    }

    /// Get all mapping rules for a target table
    pub fn get_mappings_for_table(&self, table_name: &str) -> Vec<&TargetMappingRule> {
        self.mapping_rules
            .iter()
            .filter(|r| r.target_table == table_name)
            .collect()
    }

    /// Mark session as reviewed
    pub fn mark_reviewed(&mut self) {
        self.status = UnifiedMappingStatus::Reviewed;
        self.updated_at = chrono::Utc::now();
    }

    /// Mark session as active
    pub fn mark_active(&mut self) {
        self.status = UnifiedMappingStatus::Active;
        self.updated_at = chrono::Utc::now();
    }

    /// Validate session is ready for load
    pub fn validate_for_load(&self) -> Result<()> {
        if self.status != UnifiedMappingStatus::Active {
            anyhow::bail!(
                "Session must be Active to load (current: {:?})",
                self.status
            );
        }

        if self.mapping_rules.is_empty() {
            anyhow::bail!("No mapping rules defined");
        }

        if !self.conflicts.is_empty() {
            anyhow::bail!("Unresolved conflicts exist: {}", self.conflicts.len());
        }

        Ok(())
    }
}

/// Mapping conflict resolution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictResolution {
    /// Ontology term with conflict
    pub ontology_term: String,

    /// Resolution action
    pub action: ResolutionAction,
}

/// Resolution action for conflicts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResolutionAction {
    /// Use first source field only
    UseFirst,

    /// Use last source field only
    UseLast,

    /// Merge using COALESCE
    Coalesce,

    /// Merge using CONCAT
    Concat { separator: String },

    /// Custom transformation
    Custom { transformation: String },
}

/// Unified mapping coordinator
///
/// Manages unified mapping sessions and stores them as RDF triples.
pub struct UnifiedMappingCoordinator {
    /// RDF store for persistence
    rdf_store: Option<Arc<GraphicaRdfStore>>,

    /// In-memory session cache
    sessions: HashMap<UnifiedSessionId, UnifiedMappingSession>,
}

impl UnifiedMappingCoordinator {
    /// Create new coordinator
    pub fn new() -> Self {
        Self {
            rdf_store: None,
            sessions: HashMap::new(),
        }
    }

    /// Create with RDF store
    pub fn with_rdf_store(rdf_store: Arc<GraphicaRdfStore>) -> Self {
        Self {
            rdf_store: Some(rdf_store),
            sessions: HashMap::new(),
        }
    }

    /// Create a new unified mapping session
    pub fn create_session(
        &mut self,
        name: String,
        source_sessions: Vec<SourceSessionId>,
        target_database: TargetDatabase,
        target_schema: HashMap<String, TargetTableSchema>,
    ) -> Result<UnifiedMappingSession> {
        let mut session =
            UnifiedMappingSession::new(name, source_sessions, target_database, target_schema);

        // Store in cache
        self.sessions
            .insert(session.session_id.clone(), session.clone());

        tracing::info!("Created unified mapping session: {}", session.session_id);

        Ok(session)
    }

    /// Get a session by ID
    pub fn get_session(&self, session_id: &str) -> Option<&UnifiedMappingSession> {
        self.sessions.get(session_id)
    }

    /// Get a mutable session by ID
    pub fn get_session_mut(&mut self, session_id: &str) -> Option<&mut UnifiedMappingSession> {
        self.sessions.get_mut(session_id)
    }

    /// Add mapping rule to session
    pub fn add_mapping_rule(&mut self, session_id: &str, rule: TargetMappingRule) -> Result<()> {
        let session = self
            .get_session_mut(session_id)
            .context("Session not found")?;

        session.add_mapping_rule(rule)?;

        tracing::debug!(
            "Added mapping rule to session {} (total: {})",
            session_id,
            session.mapping_rules.len()
        );

        Ok(())
    }

    /// Detect conflicts in session
    pub fn detect_conflicts(&mut self, session_id: &str) -> Result<Vec<MappingConflictInfo>> {
        let session = self
            .get_session_mut(session_id)
            .context("Session not found")?;

        let conflicts = session.detect_conflicts();

        tracing::info!(
            "Detected {} conflicts in session {}",
            conflicts.len(),
            session_id
        );

        Ok(conflicts)
    }

    /// Apply conflict resolutions
    pub fn apply_resolutions(
        &mut self,
        session_id: &str,
        resolutions: Vec<ConflictResolution>,
    ) -> Result<()> {
        let session = self
            .get_session_mut(session_id)
            .context("Session not found")?;

        let resolution_count = resolutions.len();

        for resolution in resolutions {
            // Update mapping rule with resolution transformation
            if let Some(rule) = session
                .mapping_rules
                .iter_mut()
                .find(|r| r.ontology_term == resolution.ontology_term)
            {
                match resolution.action {
                    ResolutionAction::Coalesce => {
                        rule.transformation = Some(format!("COALESCE({{value}})"));
                    }
                    ResolutionAction::Concat { separator } => {
                        rule.transformation = Some(format!("CONCAT({{value}}, '{}')", separator));
                    }
                    ResolutionAction::Custom { transformation } => {
                        rule.transformation = Some(transformation);
                    }
                    _ => {}
                }
            }
        }

        // Remove resolved conflicts
        session.conflicts.clear();
        session.updated_at = chrono::Utc::now();

        tracing::info!(
            "Applied {} resolutions to session {}",
            resolution_count,
            session_id
        );

        Ok(())
    }

    /// Mark session as reviewed and ready
    pub fn finalize_session(&mut self, session_id: &str) -> Result<()> {
        let session = self
            .get_session_mut(session_id)
            .context("Session not found")?;

        if !session.conflicts.is_empty() {
            anyhow::bail!("Cannot finalize session with unresolved conflicts");
        }

        session.mark_reviewed();
        session.mark_active();

        tracing::info!("Finalized session {} (status: Active)", session_id);

        Ok(())
    }

    /// List all sessions
    pub fn list_sessions(&self) -> Vec<&UnifiedMappingSession> {
        self.sessions.values().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_unified_mapping_session() {
        let target_db = TargetDatabase {
            database_type: "PostgreSQL".to_string(),
            connection: TargetConnection::ConnectionString {
                connection_string: "host=localhost".to_string(),
            },
            schema: Some("public".to_string()),
        };

        let mut target_schema = HashMap::new();
        target_schema.insert(
            "customers".to_string(),
            TargetTableSchema {
                table_name: "customers".to_string(),
                columns: {
                    let mut cols = HashMap::new();
                    cols.insert(
                        "email".to_string(),
                        ColumnDefinition {
                            data_type: "VARCHAR(255)".to_string(),
                            nullable: false,
                            unique: true,
                            default: None,
                        },
                    );
                    cols
                },
                primary_keys: vec!["customer_id".to_string()],
                foreign_keys: Vec::new(),
            },
        );

        let session = UnifiedMappingSession::new(
            "Test Session".to_string(),
            vec!["sess_001".to_string()],
            target_db,
            target_schema,
        );

        assert_eq!(session.name, "Test Session");
        assert_eq!(session.source_sessions.len(), 1);
        assert_eq!(session.status, UnifiedMappingStatus::PendingReview);
        assert!(session.session_id.starts_with("unified_"));
    }

    #[test]
    fn test_add_mapping_rule() {
        let target_db = TargetDatabase {
            database_type: "PostgreSQL".to_string(),
            connection: TargetConnection::ConnectionString {
                connection_string: "host=localhost".to_string(),
            },
            schema: Some("public".to_string()),
        };

        let mut target_schema = HashMap::new();
        target_schema.insert(
            "customers".to_string(),
            TargetTableSchema {
                table_name: "customers".to_string(),
                columns: {
                    let mut cols = HashMap::new();
                    cols.insert(
                        "email".to_string(),
                        ColumnDefinition {
                            data_type: "VARCHAR(255)".to_string(),
                            nullable: false,
                            unique: true,
                            default: None,
                        },
                    );
                    cols
                },
                primary_keys: vec!["customer_id".to_string()],
                foreign_keys: Vec::new(),
            },
        );

        let mut session = UnifiedMappingSession::new(
            "Test Session".to_string(),
            vec!["sess_001".to_string()],
            target_db,
            target_schema,
        );

        let rule = TargetMappingRule {
            ontology_term: "http://schema.org/email".to_string(),
            target_table: "customers".to_string(),
            target_column: "email".to_string(),
            transformation: Some("LOWER(TRIM({value}))".to_string()),
            required: true,
            source_fields: Vec::new(),
        };

        assert!(session.add_mapping_rule(rule).is_ok());
        assert_eq!(session.mapping_rules.len(), 1);
    }

    #[test]
    fn test_detect_conflicts() {
        let target_db = TargetDatabase {
            database_type: "PostgreSQL".to_string(),
            connection: TargetConnection::ConnectionString {
                connection_string: "host=localhost".to_string(),
            },
            schema: Some("public".to_string()),
        };

        let mut target_schema = HashMap::new();
        target_schema.insert(
            "customers".to_string(),
            TargetTableSchema {
                table_name: "customers".to_string(),
                columns: {
                    let mut cols = HashMap::new();
                    cols.insert(
                        "email".to_string(),
                        ColumnDefinition {
                            data_type: "VARCHAR(255)".to_string(),
                            nullable: false,
                            unique: true,
                            default: None,
                        },
                    );
                    cols.insert(
                        "contact_email".to_string(),
                        ColumnDefinition {
                            data_type: "VARCHAR(255)".to_string(),
                            nullable: true,
                            unique: false,
                            default: None,
                        },
                    );
                    cols
                },
                primary_keys: vec!["customer_id".to_string()],
                foreign_keys: Vec::new(),
            },
        );

        let mut session = UnifiedMappingSession::new(
            "Test Session".to_string(),
            vec!["sess_001".to_string(), "sess_002".to_string()],
            target_db,
            target_schema,
        );

        // Add two rules for same ontology term but different target columns
        session
            .add_mapping_rule(TargetMappingRule {
                ontology_term: "http://schema.org/email".to_string(),
                target_table: "customers".to_string(),
                target_column: "email".to_string(),
                transformation: None,
                required: true,
                source_fields: Vec::new(),
            })
            .unwrap();

        session
            .add_mapping_rule(TargetMappingRule {
                ontology_term: "http://schema.org/email".to_string(),
                target_table: "customers".to_string(),
                target_column: "contact_email".to_string(),
                transformation: None,
                required: false,
                source_fields: Vec::new(),
            })
            .unwrap();

        let conflicts = session.detect_conflicts();
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].ontology_term, "http://schema.org/email");
    }

    #[test]
    fn test_coordinator_create_session() {
        let mut coordinator = UnifiedMappingCoordinator::new();

        let target_db = TargetDatabase {
            database_type: "PostgreSQL".to_string(),
            connection: TargetConnection::ConnectionString {
                connection_string: "host=localhost".to_string(),
            },
            schema: Some("public".to_string()),
        };

        let session = coordinator
            .create_session(
                "Test Session".to_string(),
                vec!["sess_001".to_string()],
                target_db,
                HashMap::new(),
            )
            .unwrap();

        assert!(coordinator.get_session(&session.session_id).is_some());
    }

    #[test]
    fn test_validate_for_load() {
        let target_db = TargetDatabase {
            database_type: "PostgreSQL".to_string(),
            connection: TargetConnection::ConnectionString {
                connection_string: "host=localhost".to_string(),
            },
            schema: Some("public".to_string()),
        };

        let mut session = UnifiedMappingSession::new(
            "Test Session".to_string(),
            vec!["sess_001".to_string()],
            target_db,
            HashMap::new(),
        );

        // Should fail - not active
        assert!(session.validate_for_load().is_err());

        // Should fail - no mapping rules
        session.mark_active();
        assert!(session.validate_for_load().is_err());
    }
}
