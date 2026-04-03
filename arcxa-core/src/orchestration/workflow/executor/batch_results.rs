use anyhow::Result;

use crate::orchestration::workflow::runtime::frame::BatchFrame;

#[derive(Debug)]
pub(super) struct BatchStepExecutionResult {
    pub(super) success: bool,
    pub(super) output: serde_json::Value,
    pub(super) confidence: f64,
    pub(super) batch_frame: Option<BatchFrame>,
}

impl BatchStepExecutionResult {
    pub(super) fn new(
        success: bool,
        output: serde_json::Value,
        confidence: f64,
        batch_frame: Option<BatchFrame>,
    ) -> Self {
        Self {
            success,
            output,
            confidence,
            batch_frame,
        }
    }

    pub(super) fn without_frame(success: bool, output: serde_json::Value, confidence: f64) -> Self {
        Self::new(success, output, confidence, None)
    }

    pub(super) fn with_frame(
        success: bool,
        output: serde_json::Value,
        confidence: f64,
        batch_frame: BatchFrame,
    ) -> Self {
        Self::new(success, output, confidence, Some(batch_frame))
    }

    pub(super) fn success(output: serde_json::Value) -> Self {
        Self::without_frame(true, output, 1.0)
    }
}

pub(super) fn build_batch_rows_step_result(
    frame: BatchFrame,
    extra_fields: Vec<(String, serde_json::Value)>,
    success: bool,
    confidence: f64,
) -> Result<BatchStepExecutionResult> {
    let row_count = frame.row_count();
    let rows = frame.to_json_values()?;

    Ok(BatchStepExecutionResult::with_frame(
        success,
        build_rows_output(rows, row_count, extra_fields),
        confidence,
        frame,
    ))
}

pub(super) fn build_batch_rows_success_result(
    frame: BatchFrame,
    extra_fields: Vec<(String, serde_json::Value)>,
) -> Result<BatchStepExecutionResult> {
    build_batch_rows_step_result(frame, extra_fields, true, 1.0)
}

pub(super) fn build_materialized_rows_step_result(
    rows: Vec<serde_json::Value>,
    extra_fields: Vec<(String, serde_json::Value)>,
    success: bool,
    confidence: f64,
) -> BatchStepExecutionResult {
    let row_count = rows.len();
    let batch_frame = BatchFrame::from_json_values(&rows).ok();
    let output = build_rows_output(rows, row_count, extra_fields);

    match batch_frame {
        Some(frame) => BatchStepExecutionResult::with_frame(success, output, confidence, frame),
        None => BatchStepExecutionResult::without_frame(success, output, confidence),
    }
}

pub(super) fn build_rows_output(
    rows: Vec<serde_json::Value>,
    row_count: usize,
    extra_fields: Vec<(String, serde_json::Value)>,
) -> serde_json::Value {
    let mut output = serde_json::Map::with_capacity(extra_fields.len() + 2);
    output.insert("_rows".to_string(), serde_json::Value::Array(rows));
    output.insert("_row_count".to_string(), serde_json::json!(row_count));
    for (key, value) in extra_fields {
        output.insert(key, value);
    }
    serde_json::Value::Object(output)
}
