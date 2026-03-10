//! Row-level lineage context for ETL step execution
//!
//! Provides context for tracking individual rows through ETL pipeline steps.

use crate::core::lineage::row_level::{RowId, RowLineageEvent, RowTransformation};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Context for tracking row-level lineage through workflow execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RowLineageContext {
    /// Execution ID for this workflow run
    pub execution_id: String,

    /// Current job/workflow ID
    pub job_id: String,

    /// Current batch ID for grouping rows
    pub batch_id: String,

    /// Tenant ID for multi-tenancy
    pub tenant_id: String,

    /// Current step ID being executed
    pub current_step_id: Option<String>,

    /// Map of internal row index to RowId for tracking
    pub row_mappings: HashMap<usize, RowId>,

    /// Accumulated lineage events to be written
    pub pending_events: Vec<RowLineageEvent>,

    /// Track row transformations (dedup, merge, etc.)
    pub transformations: Vec<RowTransformationRecord>,
}

/// Record of a row transformation for detailed tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RowTransformationRecord {
    pub source_rows: Vec<RowId>,
    pub output_row: Option<RowId>,
    pub transformation: RowTransformation,
    pub timestamp: DateTime<Utc>,
    pub step_id: String,
}

impl RowLineageContext {
    /// Create a new row lineage context (backward compatible)
    pub fn new(execution_id: String, job_id: String, tenant_id: String) -> Self {
        Self::with_step(execution_id, job_id, tenant_id, None)
    }

    /// Create a new row lineage context with step tracking
    pub fn with_step(
        execution_id: String,
        job_id: String,
        tenant_id: String,
        step_id: Option<String>,
    ) -> Self {
        let batch_id = format!("batch_{}", uuid::Uuid::new_v4());
        Self {
            execution_id,
            job_id,
            batch_id,
            tenant_id,
            current_step_id: step_id,
            row_mappings: HashMap::new(),
            pending_events: Vec::new(),
            transformations: Vec::new(),
        }
    }

    /// Set the current step ID for subsequent lineage events
    pub fn set_current_step(&mut self, step_id: String) {
        self.current_step_id = Some(step_id);
    }

    /// Clear the current step ID
    pub fn clear_current_step(&mut self) {
        self.current_step_id = None;
    }

    /// Track a source row from CSV
    pub fn track_csv_row(&mut self, file_path: &str, row_number: u64, internal_idx: usize) {
        let row_id = RowId::csv(file_path, row_number);
        self.row_mappings.insert(internal_idx, row_id.clone());

        // Create initial lineage event with step tracking
        let event = RowLineageEvent::success_with_step(
            row_id,
            self.batch_id.clone(),
            self.job_id.clone(),
            self.current_step_id.clone(),
            file_path.to_string(),
            self.tenant_id.clone(),
        );
        self.pending_events.push(event);
    }

    /// Track a deduplicated row
    pub fn track_deduplication(
        &mut self,
        kept_idx: usize,
        removed_indices: Vec<usize>,
        strategy: &str,
    ) {
        if let Some(kept_row) = self.row_mappings.get(&kept_idx).cloned() {
            let removed_rows: Vec<RowId> = removed_indices
                .iter()
                .filter_map(|idx| self.row_mappings.get(idx).cloned())
                .collect();

            // Create transformation record
            let transformation = RowTransformation::new(
                format!("deduplication_{}", strategy),
                vec!["_row".to_string()],
            );

            let record = RowTransformationRecord {
                source_rows: {
                    let mut all = vec![kept_row.clone()];
                    all.extend(removed_rows.clone());
                    all
                },
                output_row: Some(kept_row.clone()),
                transformation,
                timestamp: Utc::now(),
                step_id: format!("dedup_{}", self.batch_id),
            };

            self.transformations.push(record);

            // Mark removed rows as filtered
            for removed_row in removed_rows {
                let mut event = RowLineageEvent::filtered(
                    removed_row,
                    self.batch_id.clone(),
                    self.job_id.clone(),
                    format!("Duplicate removed using {} strategy", strategy),
                    "deduplication".to_string(),
                    self.tenant_id.clone(),
                );
                event.correlation_id = Some(self.execution_id.clone());
                self.pending_events.push(event);
            }
        }
    }

    /// Track row export to CSV
    pub fn track_csv_export(&mut self, internal_idx: usize, output_path: &str, output_line: u64) {
        if let Some(source_row) = self.row_mappings.get(&internal_idx).cloned() {
            let output_row = RowId::csv(output_path, output_line);

            // Create event showing row was written
            let mut event = RowLineageEvent::success(
                source_row.clone(),
                self.batch_id.clone(),
                self.job_id.clone(),
                output_path.to_string(),
                self.tenant_id.clone(),
            );
            event.output_row_id = Some(output_row.clone());
            event.correlation_id = Some(self.execution_id.clone());

            // Add export transformation
            let mut transformation =
                RowTransformation::new("csv_export".to_string(), vec!["_row".to_string()]);
            transformation.after_values = Some({
                let mut map = HashMap::new();
                map.insert("output_path".to_string(), serde_json::json!(output_path));
                map.insert("output_line".to_string(), serde_json::json!(output_line));
                map
            });
            event.add_transformation(transformation);

            self.pending_events.push(event);

            // Update mapping for potential further processing
            self.row_mappings.insert(internal_idx, output_row);
        }
    }

    /// Track a filtered row
    pub fn track_filtered_row(&mut self, internal_idx: usize, reason: &str, rule_id: &str) {
        if let Some(row_id) = self.row_mappings.get(&internal_idx).cloned() {
            let mut event = RowLineageEvent::filtered(
                row_id,
                self.batch_id.clone(),
                self.job_id.clone(),
                reason.to_string(),
                rule_id.to_string(),
                self.tenant_id.clone(),
            );
            event.correlation_id = Some(self.execution_id.clone());
            self.pending_events.push(event);
        }
    }

    /// Track a transformation applied to a row
    pub fn track_transformation(&mut self, internal_idx: usize, transformation: RowTransformation) {
        if let Some(row_id) = self.row_mappings.get(&internal_idx).cloned() {
            // Find existing event or create new one
            if let Some(event) = self
                .pending_events
                .iter_mut()
                .find(|e| e.row_id == row_id && e.is_success())
            {
                event.add_transformation(transformation);
            } else {
                let mut event = RowLineageEvent::success(
                    row_id,
                    self.batch_id.clone(),
                    self.job_id.clone(),
                    String::new(), // Will be filled by exporter
                    self.tenant_id.clone(),
                );
                event.add_transformation(transformation);
                event.correlation_id = Some(self.execution_id.clone());
                self.pending_events.push(event);
            }
        }
    }

    /// Get all pending events and clear the buffer
    pub fn flush_events(&mut self) -> Vec<RowLineageEvent> {
        std::mem::take(&mut self.pending_events)
    }

    /// Create a new batch ID for the next stage
    pub fn new_batch(&mut self) {
        self.batch_id = format!("batch_{}", uuid::Uuid::new_v4());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_row_lineage_context_creation() {
        let ctx = RowLineageContext::new(
            "exec_123".to_string(),
            "job_456".to_string(),
            "tenant_abc".to_string(),
        );

        assert_eq!(ctx.execution_id, "exec_123");
        assert_eq!(ctx.job_id, "job_456");
        assert_eq!(ctx.tenant_id, "tenant_abc");
        assert!(ctx.batch_id.starts_with("batch_"));
        assert!(ctx.row_mappings.is_empty());
        assert!(ctx.pending_events.is_empty());
    }

    #[test]
    fn test_track_csv_row() {
        let mut ctx = RowLineageContext::new(
            "exec_123".to_string(),
            "job_456".to_string(),
            "tenant_abc".to_string(),
        );

        ctx.track_csv_row("/data/test.csv", 42, 0);

        assert_eq!(ctx.row_mappings.len(), 1);
        assert_eq!(ctx.pending_events.len(), 1);

        let row_id = ctx.row_mappings.get(&0).unwrap();
        assert_eq!(row_id.source_id, "/data/test.csv");

        let event = &ctx.pending_events[0];
        assert!(event.is_success());
        assert_eq!(event.row_id.source_id, "/data/test.csv");
    }

    #[test]
    fn test_track_deduplication() {
        let mut ctx = RowLineageContext::new(
            "exec_123".to_string(),
            "job_456".to_string(),
            "tenant_abc".to_string(),
        );

        // Track some rows
        ctx.track_csv_row("/data/test.csv", 1, 0);
        ctx.track_csv_row("/data/test.csv", 2, 1);
        ctx.track_csv_row("/data/test.csv", 3, 2);

        // Clear initial events
        ctx.pending_events.clear();

        // Track deduplication - keep first, remove others
        ctx.track_deduplication(0, vec![1, 2], "first");

        assert_eq!(ctx.transformations.len(), 1);
        assert_eq!(ctx.pending_events.len(), 2); // 2 filtered events for removed rows

        // Check that removed rows are marked as filtered
        for event in &ctx.pending_events {
            assert!(event.is_filtered());
            assert!(event.correlation_id.is_some());
        }
    }

    #[test]
    fn test_track_csv_export() {
        let mut ctx = RowLineageContext::new(
            "exec_123".to_string(),
            "job_456".to_string(),
            "tenant_abc".to_string(),
        );

        // Track source row
        ctx.track_csv_row("/data/input.csv", 1, 0);
        ctx.pending_events.clear();

        // Track export
        ctx.track_csv_export(0, "/data/output.csv", 1);

        assert_eq!(ctx.pending_events.len(), 1);
        let event = &ctx.pending_events[0];
        assert!(event.is_success());
        assert!(event.output_row_id.is_some());

        let output_row = event.output_row_id.as_ref().unwrap();
        assert_eq!(output_row.source_id, "/data/output.csv");

        // Check transformation was added
        assert_eq!(event.transformations.len(), 1);
        assert_eq!(event.transformations[0].transform_type, "csv_export");
    }
}
