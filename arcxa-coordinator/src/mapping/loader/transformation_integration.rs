//! Integration module for transformation engine with UnifiedSessionLoader
//!
//! This module bridges the new high-performance transformation engine
//! with the existing UnifiedSessionLoader workflow.

use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::mapping::loader::transformation::{TransformationEngine, Value};
use crate::mapping::multi_source::types::{
    ConflictResolution, SourceFieldRef, UnifiedFieldMapping,
};

/// Transformation processor for unified session loading
pub struct UnifiedTransformationProcessor {
    /// Transformation engine
    engine: Arc<TransformationEngine>,

    /// Enable parallel processing
    parallel_processing: bool,

    /// Batch size for parallel execution
    batch_size: usize,
}

impl UnifiedTransformationProcessor {
    /// Create a new transformation processor
    pub fn new(parallel_processing: bool) -> Self {
        Self {
            engine: Arc::new(TransformationEngine::new()),
            parallel_processing,
            batch_size: 1000,
        }
    }

    /// Apply transformations to a batch of source rows
    pub async fn transform_batch(
        &self,
        source_rows: Vec<HashMap<String, String>>,
        field_mappings: &[UnifiedFieldMapping],
    ) -> Result<Vec<HashMap<String, String>>> {
        if source_rows.is_empty() {
            return Ok(Vec::new());
        }

        let mut transformed_rows = Vec::with_capacity(source_rows.len());

        // Process in batches for better memory efficiency
        for chunk in source_rows.chunks(self.batch_size) {
            let batch_results = if self.parallel_processing && chunk.len() > 100 {
                self.transform_batch_parallel(chunk.to_vec(), field_mappings)
                    .await?
            } else {
                self.transform_batch_sequential(chunk, field_mappings)?
            };

            transformed_rows.extend(batch_results);
        }

        Ok(transformed_rows)
    }

    /// Transform a batch of rows in parallel
    async fn transform_batch_parallel(
        &self,
        rows: Vec<HashMap<String, String>>,
        field_mappings: &[UnifiedFieldMapping],
    ) -> Result<Vec<HashMap<String, String>>> {
        use rayon::prelude::*;

        let engine = self.engine.clone();
        let mappings = field_mappings.to_vec();

        tokio::task::spawn_blocking(move || {
            rows.into_par_iter()
                .map(|row| Self::transform_single_row(&engine, row, &mappings))
                .collect::<Result<Vec<_>>>()
        })
        .await?
    }

    /// Transform a batch of rows sequentially
    fn transform_batch_sequential(
        &self,
        rows: &[HashMap<String, String>],
        field_mappings: &[UnifiedFieldMapping],
    ) -> Result<Vec<HashMap<String, String>>> {
        rows.iter()
            .map(|row| Self::transform_single_row(&self.engine, row.clone(), field_mappings))
            .collect()
    }

    /// Transform a single row based on field mappings
    fn transform_single_row(
        engine: &TransformationEngine,
        mut row: HashMap<String, String>,
        field_mappings: &[UnifiedFieldMapping],
    ) -> Result<HashMap<String, String>> {
        let mut transformed = HashMap::new();

        for mapping in field_mappings {
            let value = Self::resolve_field_value(engine, &row, mapping)?;

            // Apply transformation if specified
            let final_value = if let Some(ref transform_expr) = mapping.transformation {
                // Create context with the resolved value
                let mut context = HashMap::new();
                context.insert("value".to_string(), value.clone());

                // Add all source fields to context for complex transformations
                for (key, val) in &row {
                    context.insert(key.clone(), val.clone());
                }

                // Execute transformation
                match engine.execute(transform_expr, &context) {
                    Ok(Value::String(s)) => s.into_owned(),
                    Ok(v) => v.as_string().into_owned(),
                    Err(e) => {
                        warn!(
                            "Transformation failed for field {}: {}, using original value",
                            mapping.target_column.column_name, e
                        );
                        value
                    }
                }
            } else {
                value
            };

            transformed.insert(mapping.target_column.column_name.clone(), final_value);
        }

        Ok(transformed)
    }

    /// Resolve field value based on conflict resolution strategy
    fn resolve_field_value(
        engine: &TransformationEngine,
        row: &HashMap<String, String>,
        mapping: &UnifiedFieldMapping,
    ) -> Result<String> {
        let source_values: Vec<String> = mapping
            .source_fields
            .iter()
            .filter_map(|source_ref| row.get(&source_ref.field_name).cloned())
            .collect();

        if source_values.is_empty() {
            return Ok(String::new());
        }

        match &mapping.conflict_resolution {
            ConflictResolution::NoConflict => source_values
                .first()
                .cloned()
                .ok_or_else(|| anyhow!("No source value found")),

            ConflictResolution::UsePrimary { primary_source } => {
                // Find the value from the primary source
                mapping
                    .source_fields
                    .iter()
                    .find(|sf| {
                        sf.datasource_id == *primary_source || sf.field_name == *primary_source
                    })
                    .and_then(|sf| row.get(&sf.field_name))
                    .cloned()
                    .ok_or_else(|| anyhow!("Primary source value not found"))
            }

            ConflictResolution::Merge { separator } => Ok(source_values.join(separator)),

            ConflictResolution::Coalesce => {
                // Use COALESCE function from transformation engine
                let mut context = HashMap::new();
                for (i, value) in source_values.iter().enumerate() {
                    context.insert(format!("value{}", i + 1), value.clone());
                }

                let coalesce_expr = format!(
                    "COALESCE({})",
                    (1..=source_values.len())
                        .map(|i| format!("{{value{}}}", i))
                        .collect::<Vec<_>>()
                        .join(", ")
                );

                match engine.execute(&coalesce_expr, &context) {
                    Ok(Value::String(s)) => Ok(s.into_owned()),
                    Ok(v) => Ok(v.as_string().into_owned()),
                    Err(e) => {
                        warn!("COALESCE failed: {}, using first value", e);
                        Ok(source_values[0].clone())
                    }
                }
            }

            ConflictResolution::CustomRule { rule } => {
                // Execute custom rule expression
                let mut context = HashMap::new();
                for (i, value) in source_values.iter().enumerate() {
                    context.insert(format!("source{}", i + 1), value.clone());
                }

                match engine.execute(rule, &context) {
                    Ok(Value::String(s)) => Ok(s.into_owned()),
                    Ok(v) => Ok(v.as_string().into_owned()),
                    Err(e) => {
                        warn!("Custom rule failed: {}, using first value", e);
                        Ok(source_values[0].clone())
                    }
                }
            }
        }
    }

    /// Apply transformations and resolve conflicts for unified data
    pub async fn process_unified_data(
        &self,
        source_data: HashMap<String, Vec<HashMap<String, String>>>, // session_id -> rows
        field_mappings: &[UnifiedFieldMapping],
    ) -> Result<Vec<HashMap<String, String>>> {
        info!(
            "Processing unified data from {} sources with {} field mappings",
            source_data.len(),
            field_mappings.len()
        );

        // Merge all source data into unified rows
        let mut unified_rows = Vec::new();

        // Find the maximum number of rows across all sources
        let max_rows = source_data
            .values()
            .map(|rows| rows.len())
            .max()
            .unwrap_or(0);

        for row_index in 0..max_rows {
            let mut merged_row = HashMap::new();

            // Collect data from all sources for this row index
            for (session_id, rows) in &source_data {
                if let Some(row) = rows.get(row_index) {
                    // Prefix field names with session ID to avoid conflicts
                    for (field_name, value) in row {
                        let qualified_name = format!("{}.{}", session_id, field_name);
                        merged_row.insert(qualified_name, value.clone());

                        // Also add without prefix for backward compatibility
                        merged_row.insert(field_name.clone(), value.clone());
                    }
                }
            }

            unified_rows.push(merged_row);
        }

        debug!("Merged {} rows from all sources", unified_rows.len());

        // Apply transformations to the unified data
        self.transform_batch(unified_rows, field_mappings).await
    }
}

/// Extension trait for the existing UnifiedSessionLoader
pub trait UnifiedSessionLoaderExt {
    /// Apply transformations using the new engine
    fn apply_transformations(
        &self,
        source_data: HashMap<String, Vec<HashMap<String, String>>>,
        field_mappings: &[UnifiedFieldMapping],
    ) -> impl std::future::Future<Output = Result<Vec<HashMap<String, String>>>> + Send;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::multi_source::types::{SourceFieldRef, TargetColumnRef};

    #[tokio::test]
    async fn test_transformation_processor() -> Result<()> {
        let processor = UnifiedTransformationProcessor::new(false);

        let mut source_row = HashMap::new();
        source_row.insert("name".to_string(), "  john doe  ".to_string());
        source_row.insert("email".to_string(), "JOHN@EXAMPLE.COM".to_string());

        let field_mappings = vec![
            UnifiedFieldMapping {
                id: "mapping_1".to_string(),
                source_fields: vec![SourceFieldRef {
                    session_id: "session_1".to_string(),
                    datasource_id: "csv_1".to_string(),
                    table_name: "users.csv".to_string(),
                    field_name: "name".to_string(),
                    source_data_type: "TEXT".to_string(),
                }],
                ontology_term_uri: "http://schema.org/name".to_string(),
                target_column: TargetColumnRef {
                    table_name: "customers".to_string(),
                    column_name: "full_name".to_string(),
                    data_type: "VARCHAR(255)".to_string(),
                },
                conflict_resolution: ConflictResolution::NoConflict,
                transformation: Some("UPPER(TRIM({name}))".to_string()),
                confidence: 0.95,
            },
            UnifiedFieldMapping {
                id: "mapping_2".to_string(),
                source_fields: vec![SourceFieldRef {
                    session_id: "session_1".to_string(),
                    datasource_id: "csv_1".to_string(),
                    table_name: "users.csv".to_string(),
                    field_name: "email".to_string(),
                    source_data_type: "TEXT".to_string(),
                }],
                ontology_term_uri: "http://schema.org/email".to_string(),
                target_column: TargetColumnRef {
                    table_name: "customers".to_string(),
                    column_name: "email_address".to_string(),
                    data_type: "VARCHAR(255)".to_string(),
                },
                conflict_resolution: ConflictResolution::NoConflict,
                transformation: Some("LOWER({email})".to_string()),
                confidence: 0.95,
            },
        ];

        let result = processor
            .transform_batch(vec![source_row], &field_mappings)
            .await?;

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].get("full_name"), Some(&"JOHN DOE".to_string()));
        assert_eq!(
            result[0].get("email_address"),
            Some(&"john@example.com".to_string())
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_conflict_resolution_coalesce() -> Result<()> {
        let processor = UnifiedTransformationProcessor::new(false);

        let mut source_row = HashMap::new();
        source_row.insert("email1".to_string(), "".to_string());
        source_row.insert("email2".to_string(), "john@example.com".to_string());

        let field_mappings = vec![UnifiedFieldMapping {
            id: "mapping_1".to_string(),
            source_fields: vec![
                SourceFieldRef {
                    session_id: "session_1".to_string(),
                    datasource_id: "csv_1".to_string(),
                    table_name: "users1.csv".to_string(),
                    field_name: "email1".to_string(),
                    source_data_type: "TEXT".to_string(),
                },
                SourceFieldRef {
                    session_id: "session_2".to_string(),
                    datasource_id: "csv_2".to_string(),
                    table_name: "users2.csv".to_string(),
                    field_name: "email2".to_string(),
                    source_data_type: "TEXT".to_string(),
                },
            ],
            ontology_term_uri: "http://schema.org/email".to_string(),
            target_column: TargetColumnRef {
                table_name: "customers".to_string(),
                column_name: "email".to_string(),
                data_type: "VARCHAR(255)".to_string(),
            },
            conflict_resolution: ConflictResolution::Coalesce,
            transformation: None,
            confidence: 0.95,
        }];

        let result = processor
            .transform_batch(vec![source_row], &field_mappings)
            .await?;

        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].get("email"),
            Some(&"john@example.com".to_string())
        );

        Ok(())
    }
}
