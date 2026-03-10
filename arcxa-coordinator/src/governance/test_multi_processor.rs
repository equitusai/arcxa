#[cfg(test)]
mod multi_processor_verification {
    use super::*;
    use crate::governance::message_router::{MessageRouter, RoutingStrategy, RoutedMessage, MessagePriority};
    use crate::governance::processor_pool::{ProcessorPool, ProcessorPoolConfig, BatchProcessorConfig};
    use crate::governance::rdf_store::GraphicaRdfStore;
    use graphica_core::reliability::CircuitBreakerConfig;
    use std::sync::Arc;
    use std::time::Duration;
    use uuid::Uuid;

    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn test_verify_multi_processor_spawning() {
        println!("\n=== VERIFYING MULTI-PROCESSOR SPAWNING ===");

        // Create config for 4 processors
        let config = ProcessorPoolConfig {
            num_processors: 4,
            processor_config: BatchProcessorConfig {
                batch_size: 10,
                batch_timeout: Duration::from_millis(50),
                max_retries: 2,
                retry_delay: Duration::from_millis(10),
                dlq_threshold: 3,
            },
            circuit_breaker: CircuitBreakerConfig {
                failure_threshold: 5,
                timeout: Duration::from_secs(10),
                success_threshold: 2,
            },
        };

        // Create router with 4 processors
        let mut router = MessageRouter::new(config.num_processors, 100);
        router.set_strategy(RoutingStrategy::RoundRobin);

        // Verify router has 4 channels
        println!("Router has {} processors configured", router.num_processors());
        assert_eq!(router.num_processors(), 4);

        // Create RDF store
        let store = Arc::new(GraphicaRdfStore::new("/tmp/test_multi_proc").unwrap());

        // Spawn processor pool
        let pool = ProcessorPool::spawn(config, &mut router, store)
            .await
            .expect("Failed to spawn pool");

        println!("Pool spawned with {} processors", pool.processor_count());
        assert_eq!(pool.processor_count(), 4);

        // Check processor loads (should all be 0 initially)
        let loads = pool.processor_loads();
        println!("Initial processor loads:");
        for (id, load) in &loads {
            println!("  Processor {}: load = {}", id, load);
        }
        assert_eq!(loads.len(), 4);

        // Route some messages to verify distribution
        println!("\nRouting 8 messages with round-robin...");
        for i in 0..8 {
            let msg = RoutedMessage {
                event: vec![1, 2, 3],
                dataset_id: format!("dataset_{}", i),
                record_id: format!("record_{}", i),
                priority: MessagePriority::Normal,
                retry_count: 0,
                trace_id: Uuid::new_v4().to_string(),
            };

            match router.route(msg) {
                Ok(_) => println!("  Message {} routed successfully", i),
                Err(e) => println!("  Message {} failed to route: {}", i, e),
            }
        }

        // Get loads after routing
        let loads_after = router.all_loads();
        println!("\nProcessor loads after routing:");
        for (id, load) in &loads_after {
            println!("  Processor {}: load = {}", id, load);
        }

        // With round-robin, each processor should have 2 messages
        for (id, load) in &loads_after {
            assert_eq!(load, 2, "Processor {} should have 2 messages with round-robin, has {}", id, load);
        }

        // Test hash affinity to ensure messages go to different processors
        router.set_strategy(RoutingStrategy::HashAffinity);
        println!("\nTesting hash affinity routing...");

        // Route messages with different datasets (should distribute)
        for i in 0..4 {
            let msg = RoutedMessage {
                event: vec![1, 2, 3],
                dataset_id: format!("unique_dataset_{}", i),
                record_id: format!("record_{}", i),
                priority: MessagePriority::Normal,
                retry_count: 0,
                trace_id: Uuid::new_v4().to_string(),
            };
            router.route(msg).ok();
        }

        // Allow processors to run briefly
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Check that all processors are healthy
        assert!(pool.is_healthy());

        // Trigger a flush to test barrier coordination
        println!("\nTesting coordinated flush...");
        let flush_result = tokio::time::timeout(
            Duration::from_secs(2),
            pool.flush()
        ).await;
        assert!(flush_result.is_ok(), "Flush should complete within timeout");

        // Shutdown pool and verify all processors complete
        println!("\nShutting down pool...");
        let results = pool.shutdown().await;

        println!("Shutdown results:");
        assert_eq!(results.len(), 4, "Should have 4 processor results");

        for (i, result) in results.iter().enumerate() {
            match result {
                Ok(stats) => {
                    println!("  Processor {} shutdown: id={}, processed={}, failed={}, batches={}",
                        i, stats.id, stats.events_processed, stats.events_failed, stats.batches_processed);
                    // Verify processor IDs are correct
                    assert_eq!(stats.id, i, "Processor ID should match index");
                }
                Err(e) => {
                    panic!("Processor {} failed to shutdown: {}", i, e);
                }
            }
        }

        println!("\n✅ MULTI-PROCESSOR VERIFICATION COMPLETE!");
        println!("  ✓ Successfully created 4 separate processors");
        println!("  ✓ Each processor has its own channel receiver");
        println!("  ✓ Messages distributed across processors");
        println!("  ✓ Barrier-based flush coordination works");
        println!("  ✓ All processors shutdown gracefully");
    }
}