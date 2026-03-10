//! Migration example: Updating existing executor with minimal changes
//!
//! This file demonstrates how to modify the existing executor.rs to use
//! the new row storage system with minimal disruption to existing code.

use std::collections::HashMap;
use serde_json;

// =============================================================================
// STEP 1: Update the execute_step_internal function (lines 232-274)
// =============================================================================

/// Original problematic code:
fn execute_step_internal_OLD(
    context: &mut ExecutionContext,
    step_result: StepResult,
) {
    // PROBLEM: This clones entire dataset!
    if let serde_json::Value::Object(ref mut working_obj) = context.working_data {
        if let serde_json::Value::Object(output_obj) = &step_result.output {
            for (key, value) in output_obj {
                working_obj.insert(key.clone(), value.clone()); // EXPENSIVE!
            }
        }
    }
}

/// New optimized version:
fn execute_step_internal_NEW(
    context: &mut ExecutionContext,
    step_result: StepResult,
) {
    // Extract rows separately from metadata
    let (metadata, rows) = if let serde_json::Value::Object(mut obj) = step_result.output {
        let rows = obj.remove("_rows");
        (serde_json::Value::Object(obj), rows)
    } else {
        (step_result.output, None)
    };

    // Store rows in row_storage instead of working_data
    if let Some(rows_value) = rows {
        if let Some(rows_array) = rows_value.as_array() {
            // Use new set_rows method that handles tiering
            context.set_rows(rows_array.clone()).ok();
        }
    }

    // Only merge metadata (no _rows field)
    if let serde_json::Value::Object(ref mut working_obj) = context.working_data {
        if let serde_json::Value::Object(metadata_obj) = metadata {
            for (key, value) in metadata_obj {
                if key != "_rows" {  // Skip _rows
                    working_obj.insert(key, value);
                }
            }
        }
    }
}

// =============================================================================
// STEP 2: Update ExecutionContext struct (add new fields)
// =============================================================================

/// Add these fields to ExecutionContext:
pub struct ExecutionContext {
    // ... existing fields ...

    /// NEW: Row storage reference (replaces _rows in working_data)
    pub row_storage: Option<RowStorage>,

    /// NEW: Storage manager for lifecycle management (optional)
    pub storage_manager: Option<Arc<StorageManager>>,

    // ... rest of existing fields ...
}

impl ExecutionContext {
    /// NEW METHOD: Get rows with automatic backend selection
    pub fn get_rows(&self) -> Result<RowAccessor> {
        if let Some(ref storage) = self.row_storage {
            Ok(RowAccessor::new(storage.clone()))
        } else {
            // Fallback for backwards compatibility
            if let Some(rows) = self.working_data.get("_rows") {
                Ok(RowAccessor::from_json(rows.clone()))
            } else {
                Err(WorkflowError::DataNotFound("No rows found".into()))
            }
        }
    }

    /// NEW METHOD: Set rows with automatic tiering
    pub fn set_rows(&mut self, rows: Vec<serde_json::Value>) -> Result<()> {
        self.row_storage = Some(RowStorage::from_rows(rows)?);

        // Update metadata in working_data
        if let serde_json::Value::Object(ref mut obj) = self.working_data {
            obj.insert("_row_count".into(), json!(self.row_storage.as_ref().unwrap().len()));
            obj.remove("_rows");  // Remove old _rows to save memory
        }
        Ok(())
    }
}

// =============================================================================
// STEP 3: Update get_rows_from_context to use new accessor
// =============================================================================

/// Original implementation:
fn get_rows_from_context_OLD(&self, context: &ExecutionContext) -> Result<Vec<serde_json::Value>> {
    if let Some(rows) = context.working_data.get("_rows").and_then(|v| v.as_array()) {
        return Ok(rows.clone());  // EXPENSIVE CLONE!
    }
    // ... fallback logic ...
}

/// New implementation (backwards compatible):
fn get_rows_from_context_NEW(&self, context: &ExecutionContext) -> Result<Vec<serde_json::Value>> {
    // Use new accessor which handles storage tiering
    let accessor = context.get_rows()?;

    // For backwards compatibility, materialize to Vec
    // Steps can be gradually migrated to use accessor directly
    accessor.to_vec()
}

/// Even better - return accessor for zero-copy access:
fn get_row_accessor(&self, context: &ExecutionContext) -> Result<RowAccessor> {
    context.get_rows()
}

// =============================================================================
// STEP 4: Gradually migrate steps to use streaming
// =============================================================================

/// Example: Migrate deduplicator step
impl WorkflowExecutor {
    /// Phase 1: Minimal change - just avoid cloning in get_rows
    async fn execute_deduplicator_phase1(
        &self,
        config: &DeduplicatorConfig,
        context: &mut ExecutionContext,
    ) -> Result<(bool, serde_json::Value)> {
        // Old: let rows = self.get_rows_from_context(context)?;
        // New: Use accessor to avoid clone
        let rows = context.get_rows()?.to_vec()?;  // Still materializes but no clone in storage

        // ... rest of logic unchanged ...
        let deduped = self.deduplicate_rows(rows, config)?;

        // Store using new method
        context.set_rows(deduped)?;

        Ok((true, json!({"_row_count": context.row_storage.as_ref().unwrap().len()})))
    }

    /// Phase 2: Full streaming implementation
    async fn execute_deduplicator_phase2(
        &self,
        config: &DeduplicatorConfig,
        context: &mut ExecutionContext,
    ) -> Result<(bool, serde_json::Value)> {
        let accessor = context.get_rows()?;
        let mut seen = HashSet::new();
        let mut deduped = Vec::new();

        // Stream through rows without materializing all at once
        for row_result in accessor.iter() {
            let row = row_result?;
            let key = self.build_dedup_key(&row, &config.key_fields)?;

            if seen.insert(key) {
                deduped.push(row);
            }
        }

        context.set_rows(deduped)?;
        Ok((true, json!({"_row_count": context.row_storage.as_ref().unwrap().len()})))
    }
}

// =============================================================================
// STEP 5: Add storage manager initialization (optional but recommended)
// =============================================================================

/// In main executor initialization:
pub fn create_optimized_executor() -> WorkflowExecutor {
    // Create storage manager for optimal performance
    let storage_manager = Arc::new(
        StorageManager::new(
            Path::new("/tmp/graphica/rocksdb"),
            Path::new("/tmp/graphica/temp"),
        ).expect("Failed to create storage manager")
    );

    let mut executor = WorkflowExecutor::new();
    executor.storage_manager = Some(storage_manager);
    executor
}

// =============================================================================
// MIGRATION CHECKLIST
// =============================================================================

// 1. [ ] Add row_storage field to ExecutionContext
// 2. [ ] Add get_rows() and set_rows() methods
// 3. [ ] Update execute loop to not clone _rows (lines 232-274)
// 4. [ ] Update get_rows_from_context to use accessor
// 5. [ ] Test with existing workflows to ensure compatibility
// 6. [ ] Gradually migrate individual steps to streaming
// 7. [ ] Add storage manager for large dataset support
// 8. [ ] Monitor performance improvements

// =============================================================================
// BACKWARDS COMPATIBILITY WRAPPER
// =============================================================================

/// Wrapper to make old steps work with new context
pub struct BackwardsCompatibleStep<T> {
    inner: T,
}

impl<T: OldStepTrait> BackwardsCompatibleStep<T> {
    pub fn execute(&self, context: &mut ExecutionContext) -> Result<StepResult> {
        // Ensure _rows is in working_data for old steps
        if context.row_storage.is_some() && !context.working_data.get("_rows").is_some() {
            // Temporarily materialize rows for old step
            let rows = context.get_rows()?.to_vec()?;
            if let serde_json::Value::Object(ref mut obj) = context.working_data {
                obj.insert("_rows".into(), json!(rows));
            }
        }

        // Execute old step
        let result = self.inner.execute_old(context)?;

        // Extract rows back to storage
        if let serde_json::Value::Object(ref mut obj) = context.working_data {
            if let Some(rows_value) = obj.remove("_rows") {
                if let Some(rows) = rows_value.as_array() {
                    context.set_rows(rows.clone())?;
                }
            }
        }

        Ok(result)
    }
}

// =============================================================================
// PERFORMANCE COMPARISON
// =============================================================================

#[cfg(test)]
mod benchmarks {
    use super::*;
    use std::time::Instant;

    #[test]
    fn benchmark_clone_vs_reference() {
        let rows = vec![json!({"id": 1, "data": "x".repeat(1000)}); 100_000];

        // Old approach with cloning
        let start = Instant::now();
        let cloned = rows.clone();
        let clone_time = start.elapsed();

        // New approach with Arc reference
        let start = Instant::now();
        let storage = RowStorage::from_rows(rows).unwrap();
        let _accessor = RowAccessor::new(storage);
        let ref_time = start.elapsed();

        println!("Clone time: {:?}", clone_time);
        println!("Reference time: {:?}", ref_time);
        println!("Speedup: {:.2}x", clone_time.as_secs_f64() / ref_time.as_secs_f64());

        // Expected: 100x+ speedup for large datasets
        assert!(ref_time < clone_time / 10);
    }
}