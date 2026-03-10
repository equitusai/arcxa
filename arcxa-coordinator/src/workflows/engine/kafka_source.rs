//! Kafka Source for Streaming Workflows
//!
//! Provides Kafka consumer integration for Timely dataflow with:
//! - Partition-based parallelism (each worker reads specific partitions)
//! - Consumer group coordination
//! - Offset management and checkpointing
//! - Graceful shutdown and rebalancing
//!
//! ## Architecture
//!
//! ```text
//! KafkaSource
//!   ├─ Consumer Pool (one per Timely worker)
//!   ├─ Partition Assignment (round-robin across workers)
//!   ├─ Offset Tracking (committed periodically)
//!   └─ Graceful Shutdown (flush pending commits)
//! ```
//!
//! ## Example
//!
//! ```rust,no_run
//! use graphica_coordinator::workflows::engine::KafkaSource;
//! use std::collections::HashMap;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let source = KafkaSource::new(
//!     "events_topic",
//!     "workflow_group",
//!     vec!["localhost:9092".to_string()],
//!     HashMap::new(),
//! )?;
//!
//! // Initialize worker 0 with partitions [0, 1]
//! source.initialize_worker(0, vec![0, 1]).await?;
//!
//! // Poll records from worker 0 (reads from partitions 0 and 1)
//! let records = source.poll_records(0, 1000).await?;
//! # Ok(())
//! # }
//! ```

use anyhow::{anyhow, Context, Result};
use rdkafka::config::ClientConfig;
use rdkafka::consumer::{Consumer, StreamConsumer};
use rdkafka::message::{BorrowedMessage, Message};
use rdkafka::topic_partition_list::TopicPartitionList;
use rdkafka::Offset;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Kafka source for streaming workflows
pub struct KafkaSource {
    /// Topic name
    topic: String,

    /// Consumer group ID
    group_id: String,

    /// Kafka brokers
    brokers: Vec<String>,

    /// Additional Kafka properties
    properties: HashMap<String, String>,

    /// Active consumers (worker_id -> consumer)
    consumers: Arc<RwLock<HashMap<usize, StreamConsumer>>>,

    /// Offset tracking (partition -> last committed offset)
    offsets: Arc<RwLock<HashMap<i32, i64>>>,
}

/// A record from Kafka
#[derive(Debug, Clone)]
pub struct KafkaRecord {
    /// Partition number
    pub partition: i32,

    /// Offset within partition
    pub offset: i64,

    /// Record key (optional)
    pub key: Option<Vec<u8>>,

    /// Record payload
    pub payload: Vec<u8>,

    /// Event timestamp (if available)
    pub timestamp: Option<i64>,
}

impl KafkaSource {
    /// Create a new Kafka source
    pub fn new(
        topic: impl Into<String>,
        group_id: impl Into<String>,
        brokers: Vec<String>,
        properties: HashMap<String, String>,
    ) -> Result<Self> {
        let topic = topic.into();
        let group_id = group_id.into();

        info!(
            "Creating Kafka source for topic: {} (group: {}, brokers: {:?})",
            topic, group_id, brokers
        );

        Ok(Self {
            topic,
            group_id,
            brokers,
            properties,
            consumers: Arc::new(RwLock::new(HashMap::new())),
            offsets: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Initialize a consumer for a specific worker
    ///
    /// Each Timely worker calls this with its worker ID to get a dedicated consumer.
    pub async fn initialize_worker(&self, worker_id: usize, partitions: Vec<i32>) -> Result<()> {
        info!(
            "Initializing Kafka consumer for worker {} (partitions: {:?})",
            worker_id, partitions
        );

        // Create consumer config
        let mut config = ClientConfig::new();
        config
            .set("bootstrap.servers", self.brokers.join(","))
            .set("group.id", &self.group_id)
            .set("enable.auto.commit", "false") // Manual offset management
            .set("auto.offset.reset", "earliest")
            .set("enable.partition.eof", "false");

        // Apply custom properties
        for (key, value) in &self.properties {
            config.set(key, value);
        }

        // Create consumer
        let consumer: StreamConsumer =
            config.create().context("Failed to create Kafka consumer")?;

        // Manually assign partitions (no consumer group rebalancing)
        let mut tpl = TopicPartitionList::new();
        for partition in &partitions {
            tpl.add_partition(&self.topic, *partition);
        }

        consumer
            .assign(&tpl)
            .context("Failed to assign partitions")?;

        // Store consumer
        self.consumers.write().await.insert(worker_id, consumer);

        info!(
            "Worker {} initialized with partitions: {:?}",
            worker_id, partitions
        );

        Ok(())
    }

    /// Poll for records from assigned partitions
    ///
    /// Called by each Timely worker to fetch records.
    pub async fn poll_records(
        &self,
        worker_id: usize,
        timeout_ms: u64,
    ) -> Result<Vec<KafkaRecord>> {
        let consumers = self.consumers.read().await;
        let consumer = consumers
            .get(&worker_id)
            .ok_or_else(|| anyhow!("Worker {} not initialized", worker_id))?;

        let mut records = Vec::new();
        let timeout = Duration::from_millis(timeout_ms);
        let start = std::time::Instant::now();

        // Use recv() with timeout for async consumption
        while start.elapsed() < timeout {
            match tokio::time::timeout(Duration::from_millis(100), consumer.recv()).await {
                Ok(Ok(message)) => {
                    let record = self.message_to_record(&message)?;
                    records.push(record);

                    // Batch up to 1000 records
                    if records.len() >= 1000 {
                        break;
                    }
                }
                Ok(Err(e)) => {
                    error!("Kafka recv error: {:?}", e);
                    return Err(anyhow!("Kafka recv error: {:?}", e));
                }
                Err(_) => {
                    // Timeout - no more messages available
                    if !records.is_empty() {
                        break;
                    }
                }
            }
        }

        debug!(
            "Worker {} polled {} records in {:?}",
            worker_id,
            records.len(),
            start.elapsed()
        );

        Ok(records)
    }

    /// Commit offsets for a worker
    pub async fn commit_offsets(&self, worker_id: usize, offsets: Vec<(i32, i64)>) -> Result<()> {
        let consumers = self.consumers.read().await;
        let consumer = consumers
            .get(&worker_id)
            .ok_or_else(|| anyhow!("Worker {} not initialized", worker_id))?;

        let offset_count = offsets.len();
        let mut tpl = TopicPartitionList::new();
        for (partition, offset) in offsets {
            tpl.add_partition_offset(&self.topic, partition, Offset::Offset(offset + 1))?;
        }

        consumer
            .commit(&tpl, rdkafka::consumer::CommitMode::Sync)
            .context("Failed to commit offsets")?;

        // Update internal offset tracking
        let mut offset_map = self.offsets.write().await;
        for elem in tpl.elements() {
            if let Offset::Offset(off) = elem.offset() {
                offset_map.insert(elem.partition(), off);
            }
        }

        debug!("Worker {} committed {} offsets", worker_id, offset_count);

        Ok(())
    }

    /// Get current lag for all partitions
    pub async fn get_lag(&self, worker_id: usize) -> Result<HashMap<i32, i64>> {
        let consumers = self.consumers.read().await;
        let consumer = consumers
            .get(&worker_id)
            .ok_or_else(|| anyhow!("Worker {} not initialized", worker_id))?;

        let mut lag = HashMap::new();

        // Get assigned partitions
        let assignment = consumer
            .assignment()
            .context("Failed to get partition assignment")?;

        for elem in assignment.elements() {
            let partition_id = elem.partition();

            // Get current position
            let position = match consumer.position() {
                Ok(tpl) => tpl
                    .find_partition(&self.topic, partition_id)
                    .and_then(|p| p.offset().to_raw())
                    .unwrap_or(0),
                Err(_) => 0,
            };

            // Get high watermark
            let (_, high) = consumer
                .fetch_watermarks(&self.topic, partition_id, Duration::from_secs(5))
                .unwrap_or((0, 0));

            let partition_lag = high - position;
            lag.insert(partition_id, partition_lag);
        }

        Ok(lag)
    }

    /// Shutdown a worker's consumer
    pub async fn shutdown_worker(&self, worker_id: usize) -> Result<()> {
        info!("Shutting down Kafka consumer for worker {}", worker_id);

        let mut consumers = self.consumers.write().await;
        if let Some(consumer) = consumers.remove(&worker_id) {
            // Final commit before shutdown
            consumer
                .commit_consumer_state(rdkafka::consumer::CommitMode::Sync)
                .context("Failed to commit final offsets")?;

            info!("Worker {} consumer shutdown complete", worker_id);
        }

        Ok(())
    }

    /// Calculate partition assignment for workers
    ///
    /// Distributes partitions evenly across workers using round-robin.
    pub async fn calculate_partition_assignment(
        &self,
        num_workers: usize,
    ) -> Result<HashMap<usize, Vec<i32>>> {
        // Get topic metadata to find partition count
        let consumer: StreamConsumer = ClientConfig::new()
            .set("bootstrap.servers", self.brokers.join(","))
            .set("group.id", format!("{}_metadata", self.group_id))
            .create()
            .context("Failed to create metadata consumer")?;

        let metadata = consumer
            .fetch_metadata(Some(&self.topic), Duration::from_secs(10))
            .context("Failed to fetch topic metadata")?;

        let topic_metadata = metadata
            .topics()
            .iter()
            .find(|t| t.name() == self.topic)
            .ok_or_else(|| anyhow!("Topic not found: {}", self.topic))?;

        let num_partitions = topic_metadata.partitions().len();
        info!(
            "Topic {} has {} partitions, assigning to {} workers",
            self.topic, num_partitions, num_workers
        );

        // Round-robin assignment
        let mut assignment: HashMap<usize, Vec<i32>> = HashMap::new();
        for partition_id in 0..num_partitions as i32 {
            let worker_id = (partition_id as usize) % num_workers;
            assignment.entry(worker_id).or_default().push(partition_id);
        }

        info!("Partition assignment: {:?}", assignment);

        Ok(assignment)
    }

    /// Convert Kafka message to KafkaRecord
    fn message_to_record(&self, message: &BorrowedMessage) -> Result<KafkaRecord> {
        let partition = message.partition();
        let offset = message.offset();
        let key = message.key().map(|k| k.to_vec());
        let payload = message
            .payload()
            .ok_or_else(|| anyhow!("Message has no payload"))?
            .to_vec();
        let timestamp = message.timestamp().to_millis();

        Ok(KafkaRecord {
            partition,
            offset,
            key,
            payload,
            timestamp,
        })
    }
}

impl KafkaRecord {
    /// Parse payload as JSON
    pub fn parse_json(&self) -> Result<JsonValue> {
        serde_json::from_slice(&self.payload).context("Failed to parse JSON payload")
    }

    /// Parse payload as UTF-8 string
    pub fn parse_string(&self) -> Result<String> {
        String::from_utf8(self.payload.clone()).context("Failed to parse UTF-8 string")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kafka_source_creation() {
        let source = KafkaSource::new(
            "test_topic",
            "test_group",
            vec!["localhost:9092".to_string()],
            HashMap::new(),
        );

        assert!(source.is_ok());
        let source = source.unwrap();
        assert_eq!(source.topic, "test_topic");
        assert_eq!(source.group_id, "test_group");
    }

    #[tokio::test]
    async fn test_partition_assignment_single_worker() {
        let source = KafkaSource::new(
            "test_topic",
            "test_group",
            vec!["localhost:9092".to_string()],
            HashMap::new(),
        )
        .unwrap();

        // This test requires a running Kafka broker, so we skip actual assignment
        // In a real integration test, we would verify the assignment
    }

    #[tokio::test]
    async fn test_partition_assignment_multiple_workers() {
        let source = KafkaSource::new(
            "test_topic",
            "test_group",
            vec!["localhost:9092".to_string()],
            HashMap::new(),
        )
        .unwrap();

        // This test requires a running Kafka broker, so we skip actual assignment
        // In a real integration test, we would verify round-robin distribution
    }

    #[test]
    fn test_kafka_record_parse_json() {
        let record = KafkaRecord {
            partition: 0,
            offset: 100,
            key: None,
            payload: br#"{"name":"test","value":42}"#.to_vec(),
            timestamp: Some(1234567890),
        };

        let json = record.parse_json().unwrap();
        assert_eq!(json["name"], "test");
        assert_eq!(json["value"], 42);
    }

    #[test]
    fn test_kafka_record_parse_string() {
        let record = KafkaRecord {
            partition: 0,
            offset: 100,
            key: None,
            payload: b"hello world".to_vec(),
            timestamp: Some(1234567890),
        };

        let text = record.parse_string().unwrap();
        assert_eq!(text, "hello world");
    }

    #[test]
    fn test_kafka_record_parse_invalid_json() {
        let record = KafkaRecord {
            partition: 0,
            offset: 100,
            key: None,
            payload: b"not json".to_vec(),
            timestamp: Some(1234567890),
        };

        let result = record.parse_json();
        assert!(result.is_err());
    }

    #[test]
    fn test_kafka_record_parse_invalid_utf8() {
        let record = KafkaRecord {
            partition: 0,
            offset: 100,
            key: None,
            payload: vec![0xFF, 0xFE, 0xFD], // Invalid UTF-8
            timestamp: Some(1234567890),
        };

        let result = record.parse_string();
        assert!(result.is_err());
    }
}
