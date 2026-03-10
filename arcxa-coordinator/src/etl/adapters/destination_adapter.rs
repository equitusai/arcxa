//! DataDestination Adapter
//!
//! **STATUS**: Stub implementation - will be completed in Phase 2
//!
//! This adapter will wrap DataDestination (new abstraction) as a workflow Transformer.
//!
//! ## Design Challenge
//!
//! DataDestination trait requires `&mut self` (for prepare/load/finalize sequence),
//! but Transformer trait only provides `&self`. Solutions:
//!
//! 1. Use `Arc<Mutex<Box<dyn DataDestination>>>` for interior mutability
//! 2. Or redesign DataDestination to not require mut (less ideal)
//! 3. Or make Transformer trait methods take `&mut self` (breaking change)
//!
//! **Decision**: We'll implement this properly in Phase 2 when we have actual
//! DataDestination implementations (Db2Destination) to test with.

use crate::workflows::engine::transformers::Transformer;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

/// Stub adapter for DataDestination
///
/// This will be properly implemented in Phase 2, Task 2.1-2.3
pub struct DataDestinationAdapter {
    _placeholder: String,
}

impl DataDestinationAdapter {
    /// Create a new DataDestinationAdapter (stub)
    pub fn new_stub(name: String) -> Self {
        Self { _placeholder: name }
    }
}

#[async_trait]
impl Transformer for DataDestinationAdapter {
    async fn transform(
        &self,
        _config: &Value,
        _data: &mut Value,
        _context: Option<&crate::workflows::engine::ExecutionContext>,
    ) -> Result<()> {
        Err(anyhow::anyhow!(
            "DataDestinationAdapter not yet implemented - see Phase 2 of ETL redesign"
        ))
    }

    fn name(&self) -> &'static str {
        "data_destination_adapter_stub"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stub_compiles() {
        let _adapter = DataDestinationAdapter::new_stub("test".to_string());
        // Stub exists and compiles - implementation in Phase 2
    }
}
