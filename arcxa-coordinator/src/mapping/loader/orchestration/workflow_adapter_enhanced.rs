//! Enhanced Workflow Adapter with Row-Level Lineage
//!
//! Extends the existing WorkflowCsvAdapter to support comprehensive row-level
//! lineage tracking for regulatory compliance and audit requirements.

use anyhow::{Context, Result};
use crate::common::csv_utils::{CsvReaderConfig, CsvStreamReader};
use crate::workflows::domain::{Condition, Workflow};
use crate::workflows::engine::ConditionEvaluator;
use graphica::core::lineage::row_level::{
    ProcessingOutcome, RowId, RowLevelLineageSink, RowLineageEvent, RowTransformation,
    QualityViolation,
};
use chrono::Utc;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use uuid::Uuid;

/// Enhanced CSV adapter with row-level lineage tracking
pub struct EnhancedWorkflowCsvAdapter {
    workflow: Workflow,
    lineage_sink: Arc<dyn RowLevelLineageSink>,
    tenant_id: String,
}

impl EnhancedWorkflowCsvAdapter {
    /// Create a new enhanced adapter
    pub fn new(
        workflow: Workflow,
        lineage_sink: Arc<dyn RowLevelLineageSink>,
        tenant_id: String,
    ) -> Self {
        Self {
            workflow,
            lineage_sink,
            tenant_id,
        }
    }

    /// Process CSV file with comprehensive row-level lineage tracking
    pub async fn process_csv_with_lineage(
        &self,
        input_path: &Path,
        output_path: &Path,
        source_table_name: &str,
    ) -> Result<DetailedProcessingResult> {
        let start_time = Utc::now();
        let job_id = format!("csv_{}_{}", source_table_name, Uuid::new_v4());
        let batch_id = format!("batch_{}", Uuid::new_v4());

        // Open input CSV
        let mut reader = CsvStreamReader::new(
            input_path,
            CsvReaderConfig {
                delimiter: Some(b','),
                has_header: true,
                ..Default::default()
            },
        )?;
        reader.init()?;

        let headers: Vec<String> = reader
            .headers()
            .context("Failed to read CSV headers")?
            .iter()
            .map(|s| s.to_string())
            .collect();

        // Create output CSV
        let mut writer = csv::Writer::from_path(output_path)?;
        writer.write_record(headers.iter())?;

        // Processing counters
        let mut total_rows = 0u64;
        let mut written_rows = 0u64;
        let mut filtered_rows = 0u64;
        let mut failed_rows = 0u64;
        let mut filter_reasons: HashMap<String, u64> = HashMap::new();
        let mut transformation_counts: HashMap<String, u64> = HashMap::new();

        // Lineage event buffer
        let mut lineage_batch = Vec::new();
        const BATCH_SIZE: usize = 1000;

        // Process each row
        while let Some(record) = reader.read_record()? {
            total_rows += 1;

            // Create unique row identifier
            let row_id = RowId::csv(
                input_path.to_string_lossy().to_string(),
                total_rows,
            );

            // Convert to JSON for evaluation
            let json_row = self.csv_row_to_json(&headers, &record.fields, source_table_name)?;

            // Evaluate workflow conditions
            let (should_process, filter_reason, matched_route) =
                self.evaluate_row_detailed(&json_row)?;

            // Track lineage event
            let mut lineage_event = RowLineageEvent {
                row_id: row_id.clone(),
                batch_id: batch_id.clone(),
                job_id: job_id.clone(),
                timestamp: Utc::now(),
                outcome: ProcessingOutcome::Failed {
                    error: "Not processed yet".to_string(),
                },
                transformations: Vec::new(),
                output_row_id: None,
                tenant_id: self.tenant_id.clone(),
                correlation_id: Some(format!("csv_{}", total_rows)),
            };

            if should_process {
                // Apply transformations and track them
                match self.apply_transformations_tracked(
                    &headers,
                    record.fields.clone(),
                    matched_route,
                ) {
                    Ok((transformed_row, transformations)) => {
                        // Write to output
                        writer.write_record(transformed_row.iter())?;
                        written_rows += 1;

                        // Track transformations
                        for transform in &transformations {
                            *transformation_counts
                                .entry(transform.transform_type.clone())
                                .or_default() += 1;
                        }

                        // Update lineage event
                        lineage_event.outcome = ProcessingOutcome::Processed {
                            output_location: output_path.to_string_lossy().to_string(),
                        };
                        lineage_event.transformations = transformations;
                        lineage_event.output_row_id = Some(RowId::csv(
                            output_path.to_string_lossy().to_string(),
                            written_rows,
                        ));
                    }
                    Err(e) => {
                        failed_rows += 1;
                        lineage_event.outcome = ProcessingOutcome::Failed {
                            error: e.to_string(),
                        };
                    }
                }
            } else {
                // Row was filtered
                filtered_rows += 1;
                *filter_reasons.entry(filter_reason.clone()).or_default() += 1;

                lineage_event.outcome = ProcessingOutcome::Filtered {
                    reason: filter_reason.clone(),
                    rule_id: matched_route
                        .map(|r| r.name.clone())
                        .unwrap_or_else(|| "no_match".to_string()),
                };
            }

            // Add to batch
            lineage_batch.push(lineage_event);

            // Flush batch if full
            if lineage_batch.len() >= BATCH_SIZE {
                self.lineage_sink
                    .write_rows_batch(lineage_batch.clone())
                    .await
                    .context("Failed to write lineage batch")?;
                lineage_batch.clear();
            }

            // Progress logging
            if total_rows % 10000 == 0 {
                tracing::info!(
                    "Progress: {} rows processed, {} written, {} filtered, {} failed",
                    total_rows,
                    written_rows,
                    filtered_rows,
                    failed_rows
                );
            }
        }

        // Flush remaining lineage events
        if !lineage_batch.is_empty() {
            self.lineage_sink
                .write_rows_batch(lineage_batch)
                .await
                .context("Failed to write final lineage batch")?;
        }

        writer.flush()?;

        // Calculate statistics
        let end_time = Utc::now();
        let duration_ms = (end_time - start_time).num_milliseconds() as u64;
        let avg_time_per_row = if total_rows > 0 {
            duration_ms as f64 / total_rows as f64
        } else {
            0.0
        };

        Ok(DetailedProcessingResult {
            job_id,
            batch_id,
            total_rows,
            written_rows,
            filtered_rows,
            failed_rows,
            filter_reasons,
            transformation_counts,
            duration_ms,
            avg_time_per_row,
            input_file: input_path.to_string_lossy().to_string(),
            output_file: output_path.to_string_lossy().to_string(),
        })
    }

    /// Convert CSV row to JSON for condition evaluation
    fn csv_row_to_json(
        &self,
        headers: &[String],
        row: &[String],
        source_table_name: &str,
    ) -> Result<serde_json::Value> {
        let mut json_obj = serde_json::Map::new();

        json_obj.insert(
            "source_table".to_string(),
            serde_json::Value::String(source_table_name.to_string()),
        );

        for (header, value) in headers.iter().zip(row.iter()) {
            let json_value = if let Ok(num) = value.parse::<f64>() {
                serde_json::Value::Number(
                    serde_json::Number::from_f64(num)
                        .unwrap_or_else(|| serde_json::Number::from(0)),
                )
            } else if value.eq_ignore_ascii_case("true") {
                serde_json::Value::Bool(true)
            } else if value.eq_ignore_ascii_case("false") {
                serde_json::Value::Bool(false)
            } else if value.is_empty() {
                serde_json::Value::Null
            } else {
                serde_json::Value::String(value.to_string())
            };

            json_obj.insert(header.clone(), json_value);
        }

        Ok(serde_json::Value::Object(json_obj))
    }

    /// Evaluate row with detailed tracking
    fn evaluate_row_detailed(
        &self,
        json_row: &serde_json::Value,
    ) -> Result<(bool, String, Option<&crate::workflows::domain::Route>)> {
        for route in self.workflow.routes_by_priority() {
            match ConditionEvaluator::evaluate(&route.condition, json_row) {
                Ok(true) => {
                    return Ok((true, format!("Matched: {}", route.name), Some(route)));
                }
                Ok(false) => continue,
                Err(e) => {
                    tracing::debug!("Error evaluating route {}: {}", route.name, e);
                    continue;
                }
            }
        }

        Ok((false, "No matching routes".to_string(), None))
    }

    /// Apply transformations and track them
    fn apply_transformations_tracked(
        &self,
        headers: &[String],
        mut row: Vec<String>,
        route: Option<&crate::workflows::domain::Route>,
    ) -> Result<(Vec<String>, Vec<RowTransformation>)> {
        let mut transformations = Vec::new();

        if let Some(route) = route {
            for action in &route.actions {
                if let crate::workflows::domain::Action::Transform { transformer, config } = action
                {
                    // Capture before state
                    let mut before_values = HashMap::new();
                    for (i, header) in headers.iter().enumerate() {
                        if let Some(value) = row.get(i) {
                            before_values.insert(
                                header.clone(),
                                serde_json::Value::String(value.clone()),
                            );
                        }
                    }

                    // Apply transformation
                    self.apply_single_transformation(&mut row, headers, transformer, config)?;

                    // Capture after state
                    let mut after_values = HashMap::new();
                    let mut changed_fields = Vec::new();
                    for (i, header) in headers.iter().enumerate() {
                        if let Some(value) = row.get(i) {
                            let new_val = serde_json::Value::String(value.clone());
                            after_values.insert(header.clone(), new_val.clone());

                            // Check if field changed
                            if let Some(old_val) = before_values.get(header) {
                                if old_val != &new_val {
                                    changed_fields.push(header.clone());
                                }
                            }
                        }
                    }

                    // Record transformation if fields actually changed
                    if !changed_fields.is_empty() {
                        let mut transform = RowTransformation::new(
                            transformer.clone(),
                            changed_fields,
                        );
                        transform = transform.with_values(before_values, after_values);
                        transformations.push(transform);
                    }
                }
            }
        }

        Ok((row, transformations))
    }

    /// Apply a single transformation to a row
    fn apply_single_transformation(
        &self,
        row: &mut Vec<String>,
        headers: &[String],
        transformer: &str,
        config: &serde_json::Value,
    ) -> Result<()> {
        match transformer {
            "proper_case" => {
                let fields = self.get_transform_fields(config, headers);
                for field in fields {
                    if let Some(idx) = headers.iter().position(|h| h == &field) {
                        if let Some(value) = row.get_mut(idx) {
                            *value = to_proper_case(value);
                        }
                    }
                }
            }
            "normalize_email" => {
                let field = config
                    .get("field")
                    .and_then(|v| v.as_str())
                    .unwrap_or("email");
                if let Some(idx) = headers.iter().position(|h| h == field) {
                    if let Some(value) = row.get_mut(idx) {
                        *value = value.trim().to_lowercase();
                    }
                }
            }
            "trim_whitespace" => {
                let fields = self.get_transform_fields(config, headers);
                for field in fields {
                    if let Some(idx) = headers.iter().position(|h| h == &field) {
                        if let Some(value) = row.get_mut(idx) {
                            *value = value.trim().to_string();
                        }
                    }
                }
            }
            _ => {
                tracing::warn!("Unknown transformer: {}", transformer);
            }
        }

        Ok(())
    }

    /// Get fields for transformation from config
    fn get_transform_fields(&self, config: &serde_json::Value, headers: &[String]) -> Vec<String> {
        if let Some(fields_arr) = config.get("fields").and_then(|v| v.as_array()) {
            fields_arr
                .iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.to_string())
                .collect()
        } else {
            headers.to_vec()
        }
    }
}

/// Convert string to proper case
fn to_proper_case(s: &str) -> String {
    s.split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => {
                    first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase()
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Detailed result of CSV processing with lineage
#[derive(Debug, Clone)]
pub struct DetailedProcessingResult {
    pub job_id: String,
    pub batch_id: String,
    pub total_rows: u64,
    pub written_rows: u64,
    pub filtered_rows: u64,
    pub failed_rows: u64,
    pub filter_reasons: HashMap<String, u64>,
    pub transformation_counts: HashMap<String, u64>,
    pub duration_ms: u64,
    pub avg_time_per_row: f64,
    pub input_file: String,
    pub output_file: String,
}

impl DetailedProcessingResult {
    /// Get success rate as percentage
    pub fn success_rate(&self) -> f64 {
        if self.total_rows == 0 {
            0.0
        } else {
            (self.written_rows as f64 / self.total_rows as f64) * 100.0
        }
    }

    /// Get filter rate as percentage
    pub fn filter_rate(&self) -> f64 {
        if self.total_rows == 0 {
            0.0
        } else {
            (self.filtered_rows as f64 / self.total_rows as f64) * 100.0
        }
    }

    /// Generate summary report
    pub fn summary(&self) -> String {
        format!(
            r#"
CSV Processing Summary
======================
Job ID: {}
Input: {}
Output: {}

Rows Processed: {}
  - Written: {} ({:.1}%)
  - Filtered: {} ({:.1}%)
  - Failed: {}

Processing Time: {} ms
Avg per Row: {:.2} ms

Filter Reasons:
{}

Transformations Applied:
{}
"#,
            self.job_id,
            self.input_file,
            self.output_file,
            self.total_rows,
            self.written_rows,
            self.success_rate(),
            self.filtered_rows,
            self.filter_rate(),
            self.failed_rows,
            self.duration_ms,
            self.avg_time_per_row,
            self.filter_reasons
                .iter()
                .map(|(reason, count)| format!("  - {}: {}", reason, count))
                .collect::<Vec<_>>()
                .join("\n"),
            self.transformation_counts
                .iter()
                .map(|(transform, count)| format!("  - {}: {}", transform, count))
                .collect::<Vec<_>>()
                .join("\n"),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflows::domain::{Action, Route, Condition};
    use graphica::storage::row_lineage_store::RowLineageStore;
    use tempfile::{tempdir, NamedTempFile};
    use std::io::Write;
    use serde_json::json;

    #[tokio::test]
    async fn test_enhanced_csv_processing() -> Result<()> {
        // Create test workflow
        let route = Route::new(
            "filter_active",
            "Filter Active Records",
            Condition::Equals {
                field: "status".to_string(),
                value: json!("active"),
            },
            vec![
                Action::Transform {
                    transformer: "proper_case".to_string(),
                    config: json!({"fields": ["name"]}),
                },
                Action::Transform {
                    transformer: "trim_whitespace".to_string(),
                    config: json!({"fields": ["city"]}),
                },
            ],
        );
        let workflow = Workflow::new("test", "Test Workflow", vec![route]);

        // Create lineage store
        let lineage_dir = tempdir()?;
        let lineage_store = Arc::new(RowLineageStore::new(lineage_dir.path())?);

        // Create adapter
        let adapter = EnhancedWorkflowCsvAdapter::new(
            workflow,
            lineage_store.clone(),
            "test_tenant".to_string(),
        );

        // Create test CSV
        let mut input_file = NamedTempFile::new()?;
        writeln!(input_file, "id,name,city,status")?;
        writeln!(input_file, "1,john doe,  new york  ,active")?;
        writeln!(input_file, "2,JANE SMITH,  los angeles  ,inactive")?;
        writeln!(input_file, "3,bob johnson,chicago,active")?;
        input_file.flush()?;

        let output_file = NamedTempFile::new()?;

        // Process CSV
        let result = adapter
            .process_csv_with_lineage(
                input_file.path(),
                output_file.path(),
                "customers",
            )
            .await?;

        // Verify results
        assert_eq!(result.total_rows, 3);
        assert_eq!(result.written_rows, 2); // Only active records
        assert_eq!(result.filtered_rows, 1); // Inactive record

        // Check lineage was recorded
        let row1_id = RowId::csv(input_file.path().to_string_lossy().to_string(), 1);
        let lineage = lineage_store.get_row_lineage(&row1_id).await?;
        assert!(!lineage.is_empty());
        assert!(lineage[0].is_success());

        // Check transformations were tracked
        let transformations = lineage_store.get_row_transformations(&row1_id).await?;
        assert!(transformations.iter().any(|t| t.transform_type == "proper_case"));
        assert!(transformations.iter().any(|t| t.transform_type == "trim_whitespace"));

        // Verify filtered row
        let row2_id = RowId::csv(input_file.path().to_string_lossy().to_string(), 2);
        let lineage2 = lineage_store.get_row_lineage(&row2_id).await?;
        assert!(!lineage2.is_empty());
        assert!(lineage2[0].is_filtered());

        // Print summary
        println!("{}", result.summary());

        Ok(())
    }
}