//! Rebalance Instruction Handler
//!
//! Handles hash range rebalancing instructions from the coordinator.

use anyhow::Result;
use graphica_core::distributed::proto::coordinator_service::RebalanceInstruction as ProtoRebalance;
use tracing::{info, warn};

/// Rebalance instruction containing new hash range assignment
#[derive(Debug, Clone)]
pub struct RebalanceInstruction {
    /// New hash range start
    pub new_hash_start: u64,

    /// New hash range end
    pub new_hash_end: u64,

    /// Target shards for data migration
    pub target_shards: Vec<String>,
}

impl From<ProtoRebalance> for RebalanceInstruction {
    fn from(proto: ProtoRebalance) -> Self {
        let (new_hash_start, new_hash_end) = if let Some(range) = proto.new_hash_range {
            (range.start, range.end)
        } else {
            (0, u64::MAX)
        };

        Self {
            new_hash_start,
            new_hash_end,
            target_shards: proto.target_shards,
        }
    }
}

/// Handler for rebalancing operations
pub struct RebalanceHandler {
    // Future: Add state for tracking rebalance progress
}

impl RebalanceHandler {
    /// Create a new rebalance handler
    pub fn new() -> Self {
        Self {}
    }

    /// Handle a rebalance instruction
    pub async fn handle(&self, proto: ProtoRebalance) -> Result<()> {
        let instruction = RebalanceInstruction::from(proto);

        warn!("Coordinator requested hash range rebalancing");
        info!(
            "  New hash range: {:016x}..{:016x}",
            instruction.new_hash_start, instruction.new_hash_end
        );
        info!("  Target shards: {:?}", instruction.target_shards);

        // TODO: Implement rebalancing logic
        // This would involve:
        // 1. Enter read-only mode for affected hash range
        // 2. Scan local data and identify triples to migrate
        // 3. Stream data to target shards
        // 4. Wait for acknowledgement
        // 5. Delete migrated data locally
        // 6. Update local hash range
        // 7. Resume normal operations

        warn!("Rebalancing not yet implemented - instruction logged");

        Ok(())
    }

    /// Calculate hash range size
    pub fn calculate_range_size(start: u64, end: u64) -> u64 {
        end.saturating_sub(start)
    }

    /// Check if a hash falls within a range
    pub fn hash_in_range(hash: u64, start: u64, end: u64) -> bool {
        hash >= start && hash <= end
    }
}

impl Default for RebalanceHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rebalance_instruction_from_proto() {
        let proto = ProtoRebalance {
            new_hash_range: Some(graphica_core::distributed::proto::coordinator_service::HashRange {
                start: 1000,
                end: 2000,
            }),
            target_shards: vec!["shard-1".to_string(), "shard-2".to_string(), "shard-3".to_string()],
            start_after: 0,
        };

        let instruction = RebalanceInstruction::from(proto);

        assert_eq!(instruction.new_hash_start, 1000);
        assert_eq!(instruction.new_hash_end, 2000);
        assert_eq!(instruction.target_shards, vec!["shard-1".to_string(), "shard-2".to_string(), "shard-3".to_string()]);
    }

    #[test]
    fn test_rebalance_instruction_no_hash_range() {
        let proto = ProtoRebalance {
            new_hash_range: None,
            target_shards: vec![],
            start_after: 0,
        };

        let instruction = RebalanceInstruction::from(proto);

        assert_eq!(instruction.new_hash_start, 0);
        assert_eq!(instruction.new_hash_end, u64::MAX);
        assert_eq!(instruction.target_shards.len(), 0);
    }

    #[tokio::test]
    async fn test_handle_rebalance() {
        let handler = RebalanceHandler::new();

        let proto = ProtoRebalance {
            new_hash_range: Some(graphica_core::distributed::proto::coordinator_service::HashRange {
                start: 1000,
                end: 2000,
            }),
            target_shards: vec!["shard-1".to_string(), "shard-2".to_string()],
            start_after: 0,
        };

        let result = handler.handle(proto).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_calculate_range_size() {
        assert_eq!(RebalanceHandler::calculate_range_size(0, 1000), 1000);
        assert_eq!(RebalanceHandler::calculate_range_size(1000, 2000), 1000);
        assert_eq!(RebalanceHandler::calculate_range_size(0, u64::MAX), u64::MAX);
    }

    #[test]
    fn test_calculate_range_size_overflow() {
        // Should handle underflow gracefully
        assert_eq!(RebalanceHandler::calculate_range_size(2000, 1000), 0);
    }

    #[test]
    fn test_hash_in_range() {
        assert!(RebalanceHandler::hash_in_range(500, 0, 1000));
        assert!(RebalanceHandler::hash_in_range(0, 0, 1000));
        assert!(RebalanceHandler::hash_in_range(1000, 0, 1000));
        assert!(!RebalanceHandler::hash_in_range(1001, 0, 1000));
        assert!(!RebalanceHandler::hash_in_range(1500, 0, 1000));
    }

    #[test]
    fn test_hash_in_range_full_range() {
        assert!(RebalanceHandler::hash_in_range(0, 0, u64::MAX));
        assert!(RebalanceHandler::hash_in_range(u64::MAX, 0, u64::MAX));
        assert!(RebalanceHandler::hash_in_range(u64::MAX / 2, 0, u64::MAX));
    }

    #[test]
    fn test_multiple_target_shards() {
        let proto = ProtoRebalance {
            new_hash_range: Some(graphica_core::distributed::proto::coordinator_service::HashRange {
                start: 0,
                end: 1000,
            }),
            target_shards: vec![
                "shard-5".to_string(),
                "shard-10".to_string(),
                "shard-15".to_string(),
                "shard-20".to_string(),
            ],
            start_after: 0,
        };

        let instruction = RebalanceInstruction::from(proto);
        assert_eq!(instruction.target_shards.len(), 4);
        assert_eq!(
            instruction.target_shards,
            vec![
                "shard-5".to_string(),
                "shard-10".to_string(),
                "shard-15".to_string(),
                "shard-20".to_string(),
            ]
        );
    }
}
