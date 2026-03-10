//! Integration module for streaming deduplication with WorkflowExecutor
//!
//! This module shows how to integrate the StreamingDeduplicator with the
//! existing WorkflowExecutor, providing a migration path from the current
//! in-memory implementation.

use super::{
    executor::WorkflowExecutor,
    execution_context_v2::ExecutionContextV2,
    streaming_deduplicator::{StreamingDeduplicator, StreamingDedupConfig},
    definition::{DeduplicatorConfig, WorkflowStep},
    error::Result,
};
use std::sync::Arc;

impl WorkflowExecutor {
    /// Execute deduplicator step with automatic streaming for large datasets
    ///
    /// This method automatically chooses between in-memory and streaming
    /// deduplication based on dataset size.
    pub async fn execute_deduplicator_v2(
        &self,
        config: &DeduplicatorConfig,
        context: &ExecutionContextV2,
    ) -> Result<(bool, serde_json::Value, f64)> {
        // Check dataset size to determine strategy
        let row_count = context.row_storage
            .as_ref()
            .map(|s| s.len())
            .unwrap_or(0);

        let memory_estimate = context.row_storage
            .as_ref()
            .map(|s| s.memory_usage())
            .unwrap_or(0);

        tracing::info!(
            "Deduplicator v2: {} rows, ~{} MB estimated memory",
            row_count,
            memory_estimate / 1_000_000
        );

        // Decide on strategy based on size
        if row_count < 50_000 && memory_estimate < 50_000_000 {
            // Small dataset: use in-memory dedup for speed
            tracing::info!("Using in-memory deduplication for small dataset");
            self.execute_inmemory_deduplicator(config, context).await
        } else {
            // Large dataset: use streaming dedup
            tracing::info!("Using streaming deduplication for large dataset");
            self.execute_streaming_deduplicator(config, context).await
        }
    }

    /// Execute streaming deduplicator for large datasets
    async fn execute_streaming_deduplicator(
        &self,
        config: &DeduplicatorConfig,
        context: &ExecutionContextV2,
    ) -> Result<(bool, serde_json::Value, f64)> {
        let streaming_config = StreamingDedupConfig {
            base: config.clone(),
            batch_size: self.determine_batch_size(context),
            max_memory_bytes: context.resource_limits.max_memory_bytes / 10, // Use 10% of total memory
            cache_size: 100_000,
            bloom_expected_items: context.row_storage.as_ref().map(|s| s.len()).unwrap_or(1_000_000),
            bloom_false_positive_rate: 0.01,
            parallel_processing: true,
            num_workers: 4,
        };

        let mut deduplicator = StreamingDeduplicator::new(
            streaming_config,
            context,
            self.lineage_tracker.clone(),
        )?;

        deduplicator.execute(context).await
    }

    /// Execute in-memory deduplicator for small datasets (existing implementation)
    async fn execute_inmemory_deduplicator(
        &self,
        config: &DeduplicatorConfig,
        context: &ExecutionContextV2,
    ) -> Result<(bool, serde_json::Value, f64)> {
        // Convert from ExecutionContextV2 to legacy ExecutionContext
        let legacy_context = self.convert_to_legacy_context(context)?;

        // Call existing deduplicator
        self.execute_deduplicator(config, &legacy_context).await
    }

    /// Determine optimal batch size based on available resources
    fn determine_batch_size(&self, context: &ExecutionContextV2) -> usize {
        let available_memory = context.resource_limits.max_memory_bytes;
        let row_count = context.row_storage
            .as_ref()
            .map(|s| s.len())
            .unwrap_or(0);

        // Estimate bytes per row (rough average)
        let bytes_per_row = if row_count > 0 {
            context.row_storage
                .as_ref()
                .map(|s| s.memory_usage() / row_count)
                .unwrap_or(1000)
        } else {
            1000 // Default estimate
        };

        // Target 10MB per batch or 10K rows, whichever is smaller
        let target_batch_bytes = 10_000_000;
        let max_batch_size = 10_000;

        let calculated = target_batch_bytes / bytes_per_row.max(1);
        calculated.min(max_batch_size).max(100)
    }

    /// Convert ExecutionContextV2 to legacy context (temporary migration helper)
    fn convert_to_legacy_context(&self, v2: &ExecutionContextV2) -> Result<super::executor::ExecutionContext> {
        // This is a temporary bridge for backwards compatibility
        // In production, we'd properly migrate all code to use V2
        todo!("Implement V2 to V1 context conversion")
    }
}

/// Example of how to use the streaming deduplicator directly
pub async fn example_streaming_dedup() -> Result<()> {
    use super::row_storage::RowStorage;

    // Create a sample context
    let mut context = ExecutionContextV2::new(serde_json::json!({}));

    // Add some sample data
    let sample_rows = vec![
        serde_json::json!({"id": "1", "name": "Alice", "email": "alice@example.com"}),
        serde_json::json!({"id": "2", "name": "Bob", "email": "bob@example.com"}),
        serde_json::json!({"id": "1", "name": "Alice", "email": "alice@example.com"}), // Duplicate
        serde_json::json!({"id": "3", "name": "Charlie", "email": "charlie@example.com"}),
        serde_json::json!({"id": "2", "name": "Bob", "email": "bob@example.com"}), // Duplicate
    ];

    // Create row storage
    context.row_storage = Some(RowStorage::from_rows(sample_rows)?);

    // Configure deduplication
    let config = StreamingDedupConfig {
        base: DeduplicatorConfig {
            method: super::definition::DedupMethod::Exact,
            key_fields: vec!["id".to_string()],
            keep: super::definition::KeepStrategy::First,
        },
        batch_size: 2, // Small batch for demo
        max_memory_bytes: 1_000_000,
        cache_size: 10,
        bloom_expected_items: 100,
        bloom_false_positive_rate: 0.01,
        parallel_processing: false,
        num_workers: 1,
    };

    // Execute deduplication
    let mut deduplicator = StreamingDeduplicator::new(config, &context, None)?;
    let (success, output, confidence) = deduplicator.execute(&context).await?;

    println!("Success: {}", success);
    println!("Confidence: {}", confidence);
    println!("Output: {}", serde_json::to_string_pretty(&output)?);

    Ok(())
}

/// Benchmark comparing in-memory vs streaming dedup
#[cfg(test)]
mod benchmarks {
    use super::*;
    use std::time::Instant;

    async fn generate_test_data(size: usize, duplicate_rate: f64) -> Vec<serde_json::Value> {
        let mut rows = Vec::with_capacity(size);
        let unique_count = ((size as f64) * (1.0 - duplicate_rate)) as usize;

        for i in 0..size {
            let id = if i < unique_count {
                i.to_string()
            } else {
                // Create duplicate
                ((i - unique_count) % unique_count).to_string()
            };

            rows.push(serde_json::json!({
                "id": id,
                "name": format!("User_{}", i),
                "email": format!("user{}@example.com", i),
                "created_at": "2024-01-01T00:00:00Z",
                "metadata": {
                    "source": "test",
                    "version": 1
                }
            }));
        }

        // Shuffle to mix duplicates
        use rand::seq::SliceRandom;
        let mut rng = rand::thread_rng();
        rows.shuffle(&mut rng);

        rows
    }

    #[tokio::test]
    async fn benchmark_streaming_dedup() {
        let sizes = vec![1_000, 10_000, 100_000];
        let duplicate_rates = vec![0.1, 0.3, 0.5];

        println!("\n=== Streaming Deduplication Benchmarks ===\n");
        println!("{:<10} {:<10} {:<15} {:<15} {:<15}",
                 "Rows", "Dup%", "Time (ms)", "Throughput", "Memory (MB)");
        println!("{}", "-".repeat(70));

        for size in sizes {
            for dup_rate in &duplicate_rates {
                let data = generate_test_data(size, *dup_rate).await;
                let mut context = ExecutionContextV2::new(serde_json::json!({}));
                context.row_storage = Some(RowStorage::from_rows(data).unwrap());

                let config = StreamingDedupConfig {
                    base: DeduplicatorConfig {
                        method: super::definition::DedupMethod::Exact,
                        key_fields: vec!["id".to_string()],
                        keep: super::definition::KeepStrategy::First,
                    },
                    batch_size: 1000,
                    max_memory_bytes: 10_000_000,
                    cache_size: 10_000,
                    bloom_expected_items: size,
                    bloom_false_positive_rate: 0.01,
                    parallel_processing: false,
                    num_workers: 1,
                };

                let start = Instant::now();
                let mut deduplicator = StreamingDeduplicator::new(config, &context, None).unwrap();
                let (success, output, _) = deduplicator.execute(&context).await.unwrap();
                let duration = start.elapsed();

                assert!(success);

                let stats = deduplicator.state_manager.stats();
                let throughput = (size as f64) / duration.as_secs_f64();
                let memory_mb = context.row_storage.as_ref().unwrap().memory_usage() as f64 / 1_000_000.0;

                println!("{:<10} {:<10} {:<15} {:<15.0} {:<15.2}",
                         size,
                         format!("{}%", (dup_rate * 100.0) as u32),
                         duration.as_millis(),
                         throughput,
                         memory_mb);

                // Verify correctness
                let expected_unique = ((size as f64) * (1.0 - dup_rate)) as usize;
                let actual_unique = output["_row_count"].as_u64().unwrap() as usize;
                assert!(
                    (actual_unique as i32 - expected_unique as i32).abs() < 10,
                    "Expected ~{} unique rows, got {}",
                    expected_unique,
                    actual_unique
                );
            }
        }
    }

    #[tokio::test]
    async fn test_memory_pressure() {
        // Test that dedup correctly handles memory pressure
        let data = generate_test_data(100_000, 0.3).await;
        let mut context = ExecutionContextV2::new(serde_json::json!({}));
        context.row_storage = Some(RowStorage::from_rows(data).unwrap());

        // Set very low memory limit to force flushes
        let config = StreamingDedupConfig {
            base: DeduplicatorConfig {
                method: super::definition::DedupMethod::Exact,
                key_fields: vec!["id".to_string()],
                keep: super::definition::KeepStrategy::First,
            },
            batch_size: 1000,
            max_memory_bytes: 1_000_000, // 1MB - will force frequent flushes
            cache_size: 100, // Small cache
            bloom_expected_items: 100_000,
            bloom_false_positive_rate: 0.01,
            parallel_processing: false,
            num_workers: 1,
        };

        let mut deduplicator = StreamingDeduplicator::new(config, &context, None).unwrap();
        let (success, _, _) = deduplicator.execute(&context).await.unwrap();

        assert!(success);

        let stats = deduplicator.state_manager.stats();
        assert!(stats.memory_flushes > 0, "Should have triggered memory flushes");
        println!("Memory flushes: {}", stats.memory_flushes);
    }
}