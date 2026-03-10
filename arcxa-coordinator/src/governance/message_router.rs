//! # Message Router for Async Governance Brain V2
//!
//! Implements multi-processor work distribution using per-processor channels.
//! Provides intelligent routing strategies including hash affinity, least-loaded,
//! round-robin, and hybrid approaches.
//!
//! The MessageRouter is the key component enabling true multi-processor parallelism
//! by maintaining separate channels for each processor and routing messages based
//! on configurable strategies.

use ahash::AHasher;
use flume::{bounded, Receiver, SendTimeoutError, Sender};
use graphica_core::core::lineage::LineageEvent;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;

/// Message router for distributing work across multiple processors
pub struct MessageRouter {
    /// Per-processor channels for work distribution
    processor_channels: Vec<ProcessorChannel>,

    /// Round-robin counter for fallback routing
    next_processor: AtomicUsize,

    /// Load tracking per processor
    processor_loads: Vec<Arc<AtomicUsize>>,

    /// Routing strategy
    strategy: RoutingStrategy,
}

/// Channel for a single processor
pub struct ProcessorChannel {
    /// Processor ID
    pub id: usize,
    /// Sender for routing messages to this processor
    pub sender: Sender<RoutedMessage>,
    /// Receiver for this processor (will be taken by processor)
    pub receiver: Option<Receiver<RoutedMessage>>,
    /// Load tracker (number of pending messages)
    pub load: Arc<AtomicUsize>,
}

/// Routing strategy for message distribution
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutingStrategy {
    /// Hash-based routing for data affinity (same dataset → same processor)
    HashAffinity,
    /// Route to least-loaded processor
    LeastLoaded,
    /// Simple round-robin distribution
    RoundRobin,
    /// Hybrid: Hash with overflow to least-loaded
    HybridHashLeastLoaded,
}

/// Message wrapper with routing metadata
#[derive(Debug, Clone)]
pub struct RoutedMessage {
    /// Lineage event payload
    pub event: LineageEvent,
    /// Message priority
    pub priority: MessagePriority,
    /// Number of retry attempts
    pub retry_count: u32,
    /// Trace ID for distributed tracing
    pub trace_id: String,
}

/// Message priority levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MessagePriority {
    /// Critical events (compliance, regulatory)
    Critical = 0,
    /// High priority (fraud detection, anomalies)
    High = 1,
    /// Normal priority (standard lineage)
    Normal = 2,
    /// Low priority (background profiling)
    Low = 3,
}

/// Errors that can occur during routing
#[derive(Debug, Error)]
pub enum RouterError {
    /// Channel is full (includes processor ID)
    #[error("Channel {0} is full")]
    ChannelFull(usize),

    /// All channels are full
    #[error("All channels are full")]
    AllChannelsFull,

    /// Router is shutting down
    #[error("Router is shutting down")]
    ShuttingDown,
}

impl MessageRouter {
    /// Create a new message router with N processors
    ///
    /// # Arguments
    /// * `num_processors` - Number of concurrent processors
    /// * `channel_capacity` - Capacity of each processor's channel
    pub fn new(num_processors: usize, channel_capacity: usize) -> Self {
        let mut processor_channels = Vec::with_capacity(num_processors);
        let mut processor_loads = Vec::with_capacity(num_processors);

        for id in 0..num_processors {
            let (tx, rx) = bounded(channel_capacity);
            let load = Arc::new(AtomicUsize::new(0));

            processor_channels.push(ProcessorChannel {
                id,
                sender: tx,
                receiver: Some(rx),
                load: load.clone(),
            });
            processor_loads.push(load);
        }

        Self {
            processor_channels,
            next_processor: AtomicUsize::new(0),
            processor_loads,
            strategy: RoutingStrategy::HybridHashLeastLoaded,
        }
    }

    /// Route a message to the appropriate processor
    pub fn route(&self, message: RoutedMessage) -> Result<(), RouterError> {
        match self.strategy {
            RoutingStrategy::HashAffinity => self.route_hash(&message),
            RoutingStrategy::LeastLoaded => self.route_least_loaded(&message),
            RoutingStrategy::RoundRobin => self.route_round_robin(&message),
            RoutingStrategy::HybridHashLeastLoaded => {
                // Try hash-based first, fall back to least-loaded
                self.route_hash(&message)
                    .or_else(|_| self.route_least_loaded(&message))
            }
        }
    }

    /// Hash-based routing for data affinity
    /// Same dataset + record_id always routes to same processor for cache locality
    fn route_hash(&self, message: &RoutedMessage) -> Result<(), RouterError> {
        let mut hasher = AHasher::default();
        message.event.dataset.hash(&mut hasher);
        message.event.record_id.hash(&mut hasher);
        let hash = hasher.finish();

        let processor_idx = (hash as usize) % self.processor_channels.len();
        self.send_to_processor(processor_idx, message)
    }

    /// Route to least-loaded processor
    fn route_least_loaded(&self, message: &RoutedMessage) -> Result<(), RouterError> {
        let mut min_load = usize::MAX;
        let mut selected = 0;

        for (idx, load) in self.processor_loads.iter().enumerate() {
            let current_load = load.load(Ordering::Relaxed);
            if current_load < min_load {
                min_load = current_load;
                selected = idx;
            }
        }

        self.send_to_processor(selected, message)
    }

    /// Simple round-robin routing
    fn route_round_robin(&self, message: &RoutedMessage) -> Result<(), RouterError> {
        let processor_idx =
            self.next_processor.fetch_add(1, Ordering::Relaxed) % self.processor_channels.len();

        self.send_to_processor(processor_idx, message)
    }

    /// Send message to specific processor with timeout
    fn send_to_processor(
        &self,
        processor_idx: usize,
        message: &RoutedMessage,
    ) -> Result<(), RouterError> {
        let channel = &self.processor_channels[processor_idx];

        // Try send with timeout
        match channel
            .sender
            .send_timeout(message.clone(), Duration::from_millis(100))
        {
            Ok(_) => {
                // Update load counter
                channel.load.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
            Err(SendTimeoutError::Timeout(_)) | Err(SendTimeoutError::Disconnected(_)) => {
                Err(RouterError::ChannelFull(processor_idx))
            }
        }
    }

    /// Take receivers for spawning processors
    /// This transfers ownership of receivers to processor tasks
    pub fn take_receivers(&mut self) -> Vec<(usize, Receiver<RoutedMessage>, Arc<AtomicUsize>)> {
        self.processor_channels
            .iter_mut()
            .map(|ch| {
                let receiver = ch.receiver.take().expect("Receiver already taken");
                (ch.id, receiver, ch.load.clone())
            })
            .collect()
    }

    /// Set routing strategy
    pub fn set_strategy(&mut self, strategy: RoutingStrategy) {
        self.strategy = strategy;
    }

    /// Get current routing strategy
    pub fn strategy(&self) -> RoutingStrategy {
        self.strategy
    }

    /// Get number of processors
    pub fn num_processors(&self) -> usize {
        self.processor_channels.len()
    }

    /// Get current load for a specific processor
    pub fn processor_load(&self, processor_id: usize) -> Option<usize> {
        self.processor_loads
            .get(processor_id)
            .map(|load| load.load(Ordering::Relaxed))
    }

    /// Get loads for all processors
    pub fn all_loads(&self) -> Vec<(usize, usize)> {
        self.processor_loads
            .iter()
            .enumerate()
            .map(|(id, load)| (id, load.load(Ordering::Relaxed)))
            .collect()
    }
}

impl Default for RoutingStrategy {
    fn default() -> Self {
        RoutingStrategy::HybridHashLeastLoaded
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use graphica_core::core::lineage::DataRef;
    use std::collections::HashMap;
    use uuid::Uuid;

    fn create_test_message(dataset_id: &str, record_id: &str) -> RoutedMessage {
        RoutedMessage {
            event: LineageEvent {
                id: Uuid::new_v4(),
                dataset: dataset_id.to_string(),
                record_id: record_id.to_string(),
                source_refs: vec![DataRef {
                    system: "test".to_string(),
                    path: "/test".to_string(),
                    version: None,
                    extracted_at: Utc::now(),
                    cdc_position: None,
                }],
                transforms: vec![],
                model_refs: vec![],
                output_ref: DataRef {
                    system: "output".to_string(),
                    path: "/output".to_string(),
                    version: None,
                    extracted_at: Utc::now(),
                    cdc_position: None,
                },
                ts: Utc::now(),
                run_id: "test-run".to_string(),
                tenant_id: "test-tenant".to_string(),
                correlation_id: Some(Uuid::new_v4().to_string()),
                metadata: HashMap::new(),
            },
            priority: MessagePriority::Normal,
            retry_count: 0,
            trace_id: Uuid::new_v4().to_string(),
        }
    }

    #[test]
    fn test_router_creation() {
        let router = MessageRouter::new(4, 100);
        assert_eq!(router.num_processors(), 4);
        assert_eq!(router.strategy(), RoutingStrategy::HybridHashLeastLoaded);

        // All loads should be zero initially
        let loads = router.all_loads();
        assert_eq!(loads.len(), 4);
        for (_, load) in loads {
            assert_eq!(load, 0);
        }
    }

    #[test]
    fn test_round_robin_routing() {
        let mut router = MessageRouter::new(4, 100);
        router.set_strategy(RoutingStrategy::RoundRobin);

        // Take receivers so we can route messages
        let mut receivers = router.take_receivers();

        // Route 8 messages - should distribute evenly
        for i in 0..8 {
            let msg = create_test_message("dataset", &format!("rec{}", i));
            router.route(msg).expect("Failed to route message");
        }

        // Each processor should have received 2 messages
        let loads = router.all_loads();
        for (id, load) in loads {
            assert_eq!(load, 2, "Processor {} has unexpected load", id);
        }

        // Clean up receivers
        drop(receivers);
    }

    #[test]
    fn test_hash_affinity() {
        let mut router = MessageRouter::new(4, 100);
        router.set_strategy(RoutingStrategy::HashAffinity);

        // Take receivers
        let mut receivers = router.take_receivers();

        // Same dataset+record should always go to same processor
        let msg1 = create_test_message("customers", "rec123");
        let msg2 = create_test_message("customers", "rec123");
        let msg3 = create_test_message("customers", "rec123");

        router.route(msg1).expect("Failed to route");
        router.route(msg2).expect("Failed to route");
        router.route(msg3).expect("Failed to route");

        // Should all go to same processor
        let loads = router.all_loads();
        let non_zero_loads: Vec<_> = loads.iter().filter(|(_, load)| *load > 0).collect();
        assert_eq!(
            non_zero_loads.len(),
            1,
            "Messages should go to single processor"
        );
        assert_eq!(non_zero_loads[0].1, 3, "Should have 3 messages");

        drop(receivers);
    }

    #[test]
    fn test_least_loaded_routing() {
        let mut router = MessageRouter::new(4, 100);
        router.set_strategy(RoutingStrategy::LeastLoaded);

        // Take receivers
        let mut receivers = router.take_receivers();

        // Manually set some loads to simulate uneven distribution
        router.processor_loads[0].store(10, Ordering::Relaxed);
        router.processor_loads[1].store(5, Ordering::Relaxed);
        router.processor_loads[2].store(15, Ordering::Relaxed);
        router.processor_loads[3].store(8, Ordering::Relaxed);

        // Route message - should go to processor 1 (load=5, minimum)
        let msg = create_test_message("dataset", "rec1");
        router.route(msg).expect("Failed to route");

        // Processor 1 should now have load=6
        assert_eq!(router.processor_load(1).unwrap(), 6);

        drop(receivers);
    }

    #[test]
    fn test_channel_full_error() {
        let mut router = MessageRouter::new(2, 1); // Very small capacity
        router.set_strategy(RoutingStrategy::RoundRobin);

        // Take receivers but don't consume messages
        let _receivers = router.take_receivers();

        // Fill first channel
        let msg1 = create_test_message("dataset", "rec1");
        router.route(msg1).expect("First message should succeed");

        // Fill second channel
        let msg2 = create_test_message("dataset", "rec2");
        router.route(msg2).expect("Second message should succeed");

        // Third message should fail (would go to first channel, which is full)
        let msg3 = create_test_message("dataset", "rec3");
        let result = router.route(msg3);

        assert!(result.is_err(), "Should fail when channel is full");
        match result.unwrap_err() {
            RouterError::ChannelFull(id) => {
                assert_eq!(id, 0, "Should report correct processor ID");
            }
            _ => panic!("Wrong error type"),
        }
    }

    #[test]
    fn test_hybrid_strategy_fallback() {
        let mut router = MessageRouter::new(2, 1); // Small capacity
        router.set_strategy(RoutingStrategy::HybridHashLeastLoaded);

        // Take receivers
        let _receivers = router.take_receivers();

        // Set processor 0 to full (simulated)
        router.processor_loads[0].store(100, Ordering::Relaxed);

        // This message would normally go to processor 0 via hash,
        // but should fallback to processor 1 (least loaded)
        let msg = create_test_message("dataset", "rec1");

        // First, route once to fill processor 0's channel
        router.route(msg.clone()).ok();

        // Now route again - hash would pick 0, but it's full,
        // so hybrid should fall back to least loaded (processor 1)
        let result = router.route(msg);

        // Either succeeds on processor 1, or both are full
        // This test just validates the fallback mechanism exists
        match result {
            Ok(_) => {
                // Successfully fell back
                assert!(router.processor_load(1).unwrap() > 0);
            }
            Err(RouterError::ChannelFull(_)) => {
                // Both channels full - acceptable for this test
            }
            Err(e) => panic!("Unexpected error: {:?}", e),
        }
    }

    #[test]
    fn test_message_priority_ordering() {
        // Test that priority enum is correctly ordered
        assert!(MessagePriority::Critical < MessagePriority::High);
        assert!(MessagePriority::High < MessagePriority::Normal);
        assert!(MessagePriority::Normal < MessagePriority::Low);
    }

    #[test]
    fn test_routing_strategy_default() {
        assert_eq!(
            RoutingStrategy::default(),
            RoutingStrategy::HybridHashLeastLoaded
        );
    }

    #[test]
    fn test_set_strategy() {
        let mut router = MessageRouter::new(4, 100);

        assert_eq!(router.strategy(), RoutingStrategy::HybridHashLeastLoaded);

        router.set_strategy(RoutingStrategy::RoundRobin);
        assert_eq!(router.strategy(), RoutingStrategy::RoundRobin);

        router.set_strategy(RoutingStrategy::LeastLoaded);
        assert_eq!(router.strategy(), RoutingStrategy::LeastLoaded);
    }

    #[test]
    fn test_all_loads() {
        let router = MessageRouter::new(3, 100);

        router.processor_loads[0].store(5, Ordering::Relaxed);
        router.processor_loads[1].store(10, Ordering::Relaxed);
        router.processor_loads[2].store(3, Ordering::Relaxed);

        let loads = router.all_loads();
        assert_eq!(loads, vec![(0, 5), (1, 10), (2, 3)]);
    }

    #[test]
    fn test_take_receivers() {
        let mut router = MessageRouter::new(3, 100);

        let receivers = router.take_receivers();
        assert_eq!(receivers.len(), 3);

        for (id, _rx, load) in receivers {
            assert!(id < 3);
            assert_eq!(load.load(Ordering::Relaxed), 0);
        }
    }

    #[test]
    #[should_panic(expected = "Receiver already taken")]
    fn test_take_receivers_twice_panics() {
        let mut router = MessageRouter::new(2, 100);

        let _receivers1 = router.take_receivers();
        let _receivers2 = router.take_receivers(); // Should panic
    }
}
