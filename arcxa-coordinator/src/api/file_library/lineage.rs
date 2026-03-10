//! File Library Lineage Integration
//!
//! Tracks file usage in workflows, transforms, and queries for complete provenance.

use anyhow::Result;
use chrono::{DateTime, Utc};
use graphica_core::core::lineage::{DataRef, LineageEvent, LineageSink, TransformRef};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use uuid::Uuid;

use crate::storage::LineageStorage;

/// File usage tracking for lineage
#[derive(Clone)]
pub struct FileLineageTracker {
    lineage_storage: Arc<LineageStorage>,
}

/// Represents different ways a file can be used
#[derive(Debug, Clone)]
pub enum FileUsageType {
    /// File read by a workflow
    WorkflowRead {
        workflow_id: String,
        step_id: String,
    },
    /// File used as input to a transformation
    TransformInput { transform_id: String },
    /// File queried via API
    ApiQuery { endpoint: String, user_id: String },
    /// File downloaded
    Download { user_id: String },
    /// File previewed
    Preview { user_id: String },
}

/// Impact analysis result for a file
#[derive(Debug, Clone)]
pub struct FileImpactReport {
    pub file_id: String,
    pub dependent_workflows: Vec<WorkflowDependency>,
    pub dependent_transforms: Vec<TransformDependency>,
    pub recent_usage_count: usize,
    pub last_used_at: Option<DateTime<Utc>>,
    pub can_safely_delete: bool,
    pub can_safely_modify: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct WorkflowDependency {
    pub workflow_id: String,
    pub workflow_name: Option<String>,
    pub step_id: String,
    pub last_execution: DateTime<Utc>,
    pub is_active: bool,
}

#[derive(Debug, Clone)]
pub struct TransformDependency {
    pub transform_id: String,
    pub transform_name: Option<String>,
    pub last_used: DateTime<Utc>,
}

impl FileLineageTracker {
    pub fn new(lineage_storage: Arc<LineageStorage>) -> Self {
        Self { lineage_storage }
    }

    /// Helper to create a TransformRef for file operations
    fn create_transform_ref(
        transform_id_string: String,
        transform_type: String,
        now: DateTime<Utc>,
    ) -> TransformRef {
        TransformRef {
            id: Uuid::new_v4(),
            transform_type,
            rule_id: transform_id_string,
            version: "1.0.0".to_string(),
            parameters: HashMap::new(),
            applied_at: now,
            fields_modified: Vec::new(),
        }
    }

    /// Track file usage by recording a lineage event
    pub async fn track_file_usage(
        &self,
        file_id: &str,
        file_path: &str,
        usage_type: FileUsageType,
    ) -> Result<()> {
        let now = Utc::now();

        // Create a DataRef for the file
        let file_ref = DataRef {
            system: "file-library".to_string(),
            path: format!("{}::{}", file_id, file_path),
            version: None,
            extracted_at: now,
            cdc_position: None,
        };

        // Create appropriate LineageEvent based on usage type
        let event = match usage_type {
            FileUsageType::WorkflowRead {
                workflow_id,
                step_id,
            } => {
                let transform_id_string = format!("workflow:{}:step:{}", workflow_id, step_id);
                let transform_ref = Self::create_transform_ref(
                    transform_id_string,
                    "workflow_step".to_string(),
                    now,
                );

                LineageEvent {
                    id: Uuid::new_v4(),
                    dataset: file_id.to_string(),
                    record_id: file_id.to_string(),
                    source_refs: vec![file_ref.clone()],
                    transforms: vec![transform_ref],
                    model_refs: Vec::new(),
                    output_ref: DataRef {
                        system: "file-library".to_string(),
                        path: "".to_string(),
                        version: None,
                        extracted_at: now,
                        cdc_position: None,
                    },
                    ts: now,
                    run_id: format!("workflow_run_{}", Utc::now().timestamp()),
                    tenant_id: "default".to_string(),
                    correlation_id: None,
                    metadata: HashMap::new(),
                }
            }

            FileUsageType::TransformInput { transform_id } => {
                let transform_ref = Self::create_transform_ref(
                    transform_id.clone(),
                    "data_transform".to_string(),
                    now,
                );

                LineageEvent {
                    id: Uuid::new_v4(),
                    dataset: file_id.to_string(),
                    record_id: file_id.to_string(),
                    source_refs: vec![file_ref.clone()],
                    transforms: vec![transform_ref],
                    model_refs: Vec::new(),
                    output_ref: DataRef {
                        system: "file-library".to_string(),
                        path: "".to_string(),
                        version: None,
                        extracted_at: now,
                        cdc_position: None,
                    },
                    ts: now,
                    run_id: format!("transform_run_{}", Utc::now().timestamp()),
                    tenant_id: "default".to_string(),
                    correlation_id: None,
                    metadata: HashMap::new(),
                }
            }

            FileUsageType::ApiQuery { endpoint, user_id } => {
                let transform_id_string = format!("api:{}:user:{}", endpoint, user_id);
                let transform_ref =
                    Self::create_transform_ref(transform_id_string, "api_query".to_string(), now);

                LineageEvent {
                    id: Uuid::new_v4(),
                    dataset: file_id.to_string(),
                    record_id: file_id.to_string(),
                    source_refs: vec![file_ref.clone()],
                    transforms: vec![transform_ref],
                    model_refs: Vec::new(),
                    output_ref: DataRef {
                        system: "file-library".to_string(),
                        path: "".to_string(),
                        version: None,
                        extracted_at: now,
                        cdc_position: None,
                    },
                    ts: now,
                    run_id: format!("api_query_{}", Utc::now().timestamp()),
                    tenant_id: "default".to_string(),
                    correlation_id: None,
                    metadata: HashMap::new(),
                }
            }

            FileUsageType::Download { user_id } => {
                let transform_id_string = format!("download:user:{}", user_id);
                let transform_ref = Self::create_transform_ref(
                    transform_id_string,
                    "file_download".to_string(),
                    now,
                );

                LineageEvent {
                    id: Uuid::new_v4(),
                    dataset: file_id.to_string(),
                    record_id: file_id.to_string(),
                    source_refs: vec![file_ref.clone()],
                    transforms: vec![transform_ref],
                    model_refs: Vec::new(),
                    output_ref: DataRef {
                        system: "file-library".to_string(),
                        path: "".to_string(),
                        version: None,
                        extracted_at: now,
                        cdc_position: None,
                    },
                    ts: now,
                    run_id: format!("download_{}", Utc::now().timestamp()),
                    tenant_id: "default".to_string(),
                    correlation_id: None,
                    metadata: HashMap::new(),
                }
            }

            FileUsageType::Preview { user_id } => {
                let transform_id_string = format!("preview:user:{}", user_id);
                let transform_ref = Self::create_transform_ref(
                    transform_id_string,
                    "file_preview".to_string(),
                    now,
                );

                LineageEvent {
                    id: Uuid::new_v4(),
                    dataset: file_id.to_string(),
                    record_id: file_id.to_string(),
                    source_refs: vec![file_ref.clone()],
                    transforms: vec![transform_ref],
                    model_refs: Vec::new(),
                    output_ref: DataRef {
                        system: "file-library".to_string(),
                        path: "".to_string(),
                        version: None,
                        extracted_at: now,
                        cdc_position: None,
                    },
                    ts: now,
                    run_id: format!("preview_{}", Utc::now().timestamp()),
                    tenant_id: "default".to_string(),
                    correlation_id: None,
                    metadata: HashMap::new(),
                }
            }
        };

        // Write lineage event
        self.lineage_storage.write(event)?;

        Ok(())
    }

    /// Get impact analysis for a file
    pub async fn get_file_impact(&self, file_id: &str) -> Result<FileImpactReport> {
        // Get all lineage events for this file
        let events = self.lineage_storage.get_record_lineage(file_id)?;

        let mut dependent_workflows = Vec::new();
        let mut dependent_transforms = Vec::new();
        let mut workflow_ids_seen = HashSet::new();
        let mut transform_ids_seen = HashSet::new();
        let mut last_used: Option<DateTime<Utc>> = None;

        for event in &events {
            // Track the most recent usage
            if last_used.is_none() || event.ts > last_used.unwrap() {
                last_used = Some(event.ts);
            }

            // Extract workflow dependencies
            for transform_ref in &event.transforms {
                if transform_ref.rule_id.starts_with("workflow:") {
                    // Parse workflow ID from rule_id (format: "workflow:id:step:step_id")
                    let parts: Vec<&str> = transform_ref.rule_id.split(':').collect();
                    if parts.len() >= 4 {
                        let workflow_id = parts[1].to_string();
                        let step_id = parts[3].to_string();

                        if workflow_ids_seen.insert(workflow_id.clone()) {
                            dependent_workflows.push(WorkflowDependency {
                                workflow_id: workflow_id.clone(),
                                workflow_name: None, // Would query workflow service for name
                                step_id,
                                last_execution: event.ts,
                                is_active: true, // Would query workflow status
                            });
                        }
                    }
                } else if transform_ref.transform_type == "data_transform" {
                    let transform_id = transform_ref.rule_id.clone();
                    if transform_ids_seen.insert(transform_id.clone()) {
                        dependent_transforms.push(TransformDependency {
                            transform_id: transform_id.clone(),
                            transform_name: None, // Would query transform registry
                            last_used: event.ts,
                        });
                    }
                }
            }
        }

        // Determine if file can be safely deleted or modified
        let recent_usage_count = events.len();
        let has_active_workflows = !dependent_workflows.is_empty();
        let has_recent_usage = last_used
            .map(|ts| (Utc::now() - ts).num_days() < 7)
            .unwrap_or(false);

        let can_safely_delete = !has_active_workflows && !has_recent_usage;
        let can_safely_modify = !has_active_workflows;

        let mut warnings = Vec::new();
        if has_active_workflows {
            warnings.push(format!(
                "File is used by {} active workflow(s)",
                dependent_workflows.len()
            ));
        }
        if has_recent_usage {
            warnings.push("File has been used in the last 7 days".to_string());
        }
        if !dependent_transforms.is_empty() {
            warnings.push(format!(
                "File is referenced by {} transform(s)",
                dependent_transforms.len()
            ));
        }

        Ok(FileImpactReport {
            file_id: file_id.to_string(),
            dependent_workflows,
            dependent_transforms,
            recent_usage_count,
            last_used_at: last_used,
            can_safely_delete,
            can_safely_modify,
            warnings,
        })
    }

    /// Get lineage graph for a file (upstream and downstream)
    pub async fn get_file_lineage(
        &self,
        file_id: &str,
    ) -> Result<(Vec<LineageEvent>, Vec<LineageEvent>)> {
        // Get upstream lineage (what created this file)
        let upstream_events = self.lineage_storage.get_record_lineage(file_id)?;

        // Get downstream lineage (what uses this file)
        // This would need to query by source_refs, which may require additional index
        let downstream_events = Vec::new(); // Placeholder

        Ok((upstream_events, downstream_events))
    }

    /// Get file usage statistics
    pub async fn get_usage_stats(&self, file_id: &str, days: i64) -> Result<FileUsageStats> {
        let cutoff_time = Utc::now() - chrono::Duration::days(days);

        // Get all lineage events for this file
        let events = self.lineage_storage.get_record_lineage(file_id)?;

        // Filter to events within time window
        let recent_events: Vec<_> = events.into_iter().filter(|e| e.ts >= cutoff_time).collect();

        let total_accesses = recent_events.len();
        let mut unique_workflows = HashSet::new();
        let mut unique_users = HashSet::new();

        for event in &recent_events {
            for transform_ref in &event.transforms {
                // Extract workflow IDs
                if transform_ref.rule_id.starts_with("workflow:") {
                    if let Some(wf_id) = transform_ref.rule_id.split(':').nth(1) {
                        unique_workflows.insert(wf_id.to_string());
                    }
                }

                // Extract user IDs
                if transform_ref.rule_id.contains(":user:") {
                    if let Some(user_id) = transform_ref.rule_id.split(":user:").nth(1) {
                        unique_users.insert(user_id.to_string());
                    }
                }
            }
        }

        let last_access = recent_events.iter().map(|e| e.ts).max();

        Ok(FileUsageStats {
            file_id: file_id.to_string(),
            total_accesses,
            unique_workflows: unique_workflows.len(),
            unique_users: unique_users.len(),
            last_accessed: last_access,
            time_window_days: days,
        })
    }
}

#[derive(Debug, Clone)]
pub struct FileUsageStats {
    pub file_id: String,
    pub total_accesses: usize,
    pub unique_workflows: usize,
    pub unique_users: usize,
    pub last_accessed: Option<DateTime<Utc>>,
    pub time_window_days: i64,
}
