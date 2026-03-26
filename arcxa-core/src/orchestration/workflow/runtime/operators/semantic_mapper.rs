use crate::orchestration::workflow::definition::SemanticMapperConfig;
use crate::orchestration::workflow::error::Result;
use crate::orchestration::workflow::runtime::frame::BatchFrame;

use super::RuntimeOperator;

/// Batch-native semantic mapper entry point for the runtime layer.
///
/// The current implementation is intentionally a pass-through: the optimized
/// workflow executor still relies on the existing mapping services, but this
/// lets small-dataset execution stay batch-aware end to end while the runtime
/// substrate is being phased in.
#[derive(Debug, Default)]
pub struct SemanticMapperBatchOperator;

impl RuntimeOperator for SemanticMapperBatchOperator {
    fn name(&self) -> &'static str {
        "semantic_mapper"
    }
}

impl SemanticMapperBatchOperator {
    pub fn execute(&self, frame: BatchFrame, _config: &SemanticMapperConfig) -> Result<BatchFrame> {
        Ok(frame)
    }
}
