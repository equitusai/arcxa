//! # Kafka Integration
//!
//! CDC event ingestion from Kafka/Debezium with parallel partition consumption.

use crate::core::lineage::{CdcPosition, DataRef};
use crate::ingestion::metrics;
use crate::ingestion::Record;
use anyhow::{Context, Result};
use chrono::Utc;
use dashmap::DashMap;
use rdkafka::config::ClientConfig;
use rdkafka::consumer::{Consumer, StreamConsumer};
use rdkafka::message::Message;
use rdkafka::TopicPartitionList;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::task::JoinHandle;

/// Offset tracker for checkpointing
///
/// Thread-safe tracking of current offsets per partition.
/// Used by checkpoint manager to snapshot Kafka positions.
#[derive(Clone)]
pub struct OffsetTracker {
    offsets: Arc<DashMap<i32, i64>>,
}

impl OffsetTracker {
    pub fn new() -> Self {
        Self {
            offsets: Arc::new(DashMap::new()),
        }
    }

    /// Update offset for a partition
    pub fn update(&self, partition: i32, offset: i64) {
        self.offsets.insert(partition, offset);
    }

    /// Get current offsets for checkpointing
    pub fn get_offsets(&self) -> HashMap<i32, i64> {
        self.offsets
            .iter()
            .map(|entry| (*entry.key(), *entry.value()))
            .collect()
    }

    /// Seek to checkpointed offsets on recovery
    ///
    /// Returns error if Kafka seek fails, but continues startup anyway
    /// (graceful degradation per Architecture Decision #3).
    pub async fn seek_to_offsets(
        &self,
        consumers: &[StreamConsumer],
        offsets: HashMap<i32, i64>,
        topic: &str,
    ) -> Result<()> {
        tracing::info!("Seeking to checkpointed offsets: {:?}", offsets);

        for (partition, offset) in offsets {
            if let Some(consumer) = consumers.get(partition as usize) {
                let mut tpl = TopicPartitionList::new();
                tpl.add_partition_offset(topic, partition, rdkafka::Offset::Offset(offset))
                    .context("Failed to add partition offset")?;

                if let Err(e) = consumer.seek_partitions(tpl, std::time::Duration::from_secs(5)) {
                    tracing::warn!(
                        "Failed to seek partition {} to offset {}: {}. Starting from current position.",
                        partition, offset, e
                    );
                    // Continue anyway - graceful degradation
                }
            }
        }

        Ok(())
    }
}

impl Default for OffsetTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Parallel Kafka consumer - one consumer per partition for maximum throughput
pub struct ParallelCdcConsumer {
    consumers: Vec<StreamConsumer>,
    output_channel: flume::Sender<Record>,
    offset_tracker: OffsetTracker,
    topic: String,
}

impl ParallelCdcConsumer {
    /// Create parallel consumers (one per partition)
    pub fn new(
        brokers: &str,
        group_id: &str,
        topic: String,
        num_partitions: usize,
    ) -> Result<(Self, flume::Receiver<Record>)> {
        let (tx, rx) = flume::bounded(1000);
        let offset_tracker = OffsetTracker::new();

        let mut consumers = Vec::new();
        for partition_id in 0..num_partitions {
            let mut config = ClientConfig::new();
            config
                .set("bootstrap.servers", brokers)
                .set("group.id", format!("{}-p{}", group_id, partition_id))
                .set("enable.auto.commit", "false")
                .set("auto.offset.reset", "earliest");

            // Optional: Set security protocol from environment
            if let Ok(security_protocol) = std::env::var("KAFKA_SECURITY_PROTOCOL") {
                config.set("security.protocol", security_protocol);
            }

            let consumer: StreamConsumer =
                config.create().context("Failed to create Kafka consumer")?;

            // Manually assign this partition
            let mut tpl = TopicPartitionList::new();
            tpl.add_partition(&topic, partition_id as i32);
            consumer
                .assign(&tpl)
                .context("Failed to assign partition")?;

            consumers.push(consumer);
            tracing::info!("Created consumer for partition {}", partition_id);
        }

        Ok((
            Self {
                consumers,
                output_channel: tx,
                offset_tracker,
                topic,
            },
            rx,
        ))
    }

    /// Get offset tracker (for checkpointing)
    pub fn offset_tracker(&self) -> OffsetTracker {
        self.offset_tracker.clone()
    }

    /// Get consumers reference (for seek operations)
    pub fn consumers(&self) -> &[StreamConsumer] {
        &self.consumers
    }

    /// Get topic name
    pub fn topic(&self) -> &str {
        &self.topic
    }

    /// Spawn parallel consumer tasks (one task per partition)
    pub fn start(self) -> Vec<JoinHandle<()>> {
        let offset_tracker = self.offset_tracker.clone();

        self.consumers
            .into_iter()
            .enumerate()
            .map(move |(partition_id, consumer)| {
                let tx = self.output_channel.clone();
                let tracker = offset_tracker.clone();

                tokio::spawn(async move {
                    tracing::info!("Consumer task {} started", partition_id);
                    let mut processed_count = 0u64;
                    const COMMIT_INTERVAL: u64 = 100; // Commit every 100 messages for performance

                    loop {
                        match consumer.recv().await {
                            Ok(msg) => {
                                // Track offset for checkpointing
                                tracker.update(partition_id as i32, msg.offset());

                                match Self::parse_cdc_message(&msg) {
                                    Ok(record) => {
                                        if let Err(e) = tx.send(record) {
                                            tracing::error!("Channel send failed: {}", e);
                                            break;
                                        }

                                        // FIX: Manual offset commit to prevent data loss
                                        processed_count += 1;
                                        if processed_count % COMMIT_INTERVAL == 0 {
                                            if let Err(e) = consumer.commit_message(
                                                &msg,
                                                rdkafka::consumer::CommitMode::Async,
                                            ) {
                                                tracing::error!("Failed to commit offset: {}", e);
                                            } else {
                                                tracing::debug!(
                                                    "Committed offset for partition {}, count: {}",
                                                    partition_id,
                                                    processed_count
                                                );
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        tracing::warn!("Failed to parse CDC message: {}", e);
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::error!("Kafka recv error: {}", e);
                                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                            }
                        }
                    }

                    // Final commit on exit
                    if let Err(e) =
                        consumer.commit_consumer_state(rdkafka::consumer::CommitMode::Sync)
                    {
                        tracing::error!(
                            "Failed final commit for partition {}: {}",
                            partition_id,
                            e
                        );
                    }

                    tracing::warn!("Consumer task {} exiting", partition_id);
                })
            })
            .collect()
    }

    fn parse_cdc_message(msg: &impl Message) -> Result<Record> {
        let payload = msg.payload().context("Message payload is empty")?;

        // Try parsing as Graphica lineage format first (our own format)
        if let Ok(graphica_event) = serde_json::from_slice::<GraphicaLineageEvent>(payload) {
            tracing::debug!(
                "Parsed as Graphica lineage event: dataset={}",
                graphica_event.dataset
            );

            let cdc_position = CdcPosition {
                topic: msg.topic().to_string(),
                partition: msg.partition(),
                offset: msg.offset(),
                lsn: graphica_event
                    .source
                    .cdc_position
                    .as_ref()
                    .and_then(|p| p.lsn)
                    .map(|l| l.to_string()),
            };

            let data_ref = DataRef {
                system: graphica_event.source.system.clone(),
                path: graphica_event.source.path.clone(),
                version: graphica_event
                    .source
                    .cdc_position
                    .as_ref()
                    .and_then(|p| p.lsn)
                    .map(|l| l.to_string()),
                extracted_at: Utc::now(),
                cdc_position: Some(cdc_position),
            };

            return Ok(Record {
                id: graphica_event.id,
                dataset: graphica_event.dataset,
                data: graphica_event.data,
                source: data_ref,
                timestamp: graphica_event.timestamp,
                tenant_id: graphica_event.tenant_id,
                semantic_metadata: None,
            });
        }

        // Try parsing as Debezium CDC format (external CDC events)
        match serde_json::from_slice::<DebeziumEvent>(payload) {
            Ok(cdc_event) => {
                tracing::debug!(
                    "Parsed as Debezium CDC event: table={}",
                    cdc_event.source.table
                );

                let cdc_position = CdcPosition {
                    topic: msg.topic().to_string(),
                    partition: msg.partition(),
                    offset: msg.offset(),
                    lsn: cdc_event.source.lsn.clone(),
                };

                let data_ref = DataRef {
                    system: cdc_event.source.name.clone(),
                    path: format!("{}.{}", cdc_event.source.schema, cdc_event.source.table),
                    version: cdc_event.source.lsn.clone(),
                    extracted_at: Utc::now(),
                    cdc_position: Some(cdc_position),
                };

                Ok(Record {
                    id: format!(
                        "{}:{}",
                        cdc_event.source.table,
                        cdc_event
                            .after
                            .get("id")
                            .unwrap_or(&serde_json::Value::Null)
                    ),
                    dataset: cdc_event.source.table.clone(),
                    data: cdc_event.after,
                    source: data_ref,
                    timestamp: cdc_event.ts_ms,
                    tenant_id: "default".to_string(),
                    semantic_metadata: None,
                })
            }
            Err(e) => {
                // Enhanced error logging with payload sample for debugging
                let payload_preview = String::from_utf8_lossy(payload);
                let preview_len = payload_preview.len().min(200);
                tracing::error!(
                    "Failed to parse message (neither Graphica nor Debezium format). \
                    Topic: {}, Partition: {}, Offset: {}, \
                    Error: {}, \
                    Payload preview (first 200 chars): {}...",
                    msg.topic(),
                    msg.partition(),
                    msg.offset(),
                    e,
                    &payload_preview[..preview_len]
                );
                anyhow::bail!("Failed to deserialize as Graphica or Debezium event: {}", e)
            }
        }
    }
}

/// Legacy single consumer (kept for backwards compatibility)
pub struct CdcConsumer {
    consumer: StreamConsumer,
    topics: Vec<String>,
}

impl CdcConsumer {
    pub fn new(brokers: &str, group_id: &str, topics: Vec<String>) -> Result<Self> {
        let mut config = ClientConfig::new();
        config
            .set("bootstrap.servers", brokers)
            .set("group.id", group_id)
            .set("enable.auto.commit", "false")
            .set("auto.offset.reset", "earliest");

        // Optional: Set security protocol from environment (for production)
        if let Ok(security_protocol) = std::env::var("KAFKA_SECURITY_PROTOCOL") {
            tracing::info!("Using Kafka security protocol: {}", security_protocol);
            config.set("security.protocol", security_protocol);
        } else {
            tracing::info!("Using plaintext Kafka connection (local dev)");
        }

        let consumer: StreamConsumer =
            config.create().context("Failed to create Kafka consumer")?;

        consumer
            .subscribe(&topics.iter().map(|s| s.as_str()).collect::<Vec<_>>())
            .context("Failed to subscribe to topics")?;

        Ok(Self { consumer, topics })
    }

    /// Poll for CDC events and convert to Records
    pub async fn poll_records(&self) -> Result<Vec<Record>> {
        let msg = self
            .consumer
            .recv()
            .await
            .context("Failed to receive Kafka message")?;

        let payload = msg.payload().context("Message payload is empty")?;

        let cdc_event: DebeziumEvent =
            serde_json::from_slice(payload).context("Failed to deserialize Debezium event")?;

        let record = self.convert_cdc_to_record(cdc_event, &msg)?;

        // FIX: Manual offset commit to prevent data loss
        self.consumer
            .commit_message(&msg, rdkafka::consumer::CommitMode::Async)
            .context("Failed to commit offset")?;

        Ok(vec![record])
    }

    fn convert_cdc_to_record(&self, event: DebeziumEvent, msg: &impl Message) -> Result<Record> {
        let cdc_position = CdcPosition {
            topic: msg.topic().to_string(),
            partition: msg.partition(),
            offset: msg.offset(),
            lsn: event.source.lsn.clone(),
        };

        let data_ref = DataRef {
            system: event.source.name,
            path: format!("{}.{}", event.source.schema, event.source.table),
            version: event.source.lsn.clone(),
            extracted_at: Utc::now(),
            cdc_position: Some(cdc_position),
        };

        Ok(Record {
            id: format!(
                "{}:{}",
                event.source.table,
                event.after.get("id").unwrap_or(&serde_json::Value::Null)
            ),
            dataset: event.source.table.clone(),
            data: event.after,
            source: data_ref,
            timestamp: event.ts_ms,
            tenant_id: "default".to_string(), // Extract from metadata
            semantic_metadata: None,
        })
    }
}

/// Debezium CDC event structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebeziumEvent {
    pub before: Option<serde_json::Value>,
    pub after: serde_json::Value,
    pub source: DebeziumSource,
    pub op: String, // c=create, u=update, d=delete, r=read
    pub ts_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebeziumSource {
    pub name: String,
    pub db: String,
    pub schema: String,
    pub table: String,
    pub lsn: Option<String>,
    pub txId: Option<i64>,
}

/// Graphica lineage event structure (produced by our own services)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphicaLineageEvent {
    pub data: serde_json::Value,
    pub dataset: String,
    pub id: String,
    pub source: GraphicaSource,
    pub tenant_id: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphicaSource {
    pub cdc_position: Option<GraphicaCdcPosition>,
    pub path: String,
    pub system: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphicaCdcPosition {
    pub lsn: Option<i64>,
    pub txid: Option<i64>,
}

/// Polymorphic message envelope for handling multiple formats
#[derive(Debug, Clone)]
pub enum KafkaMessage {
    Debezium(DebeziumEvent),
    Graphica(GraphicaLineageEvent),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_debezium_event_parsing() {
        let json = r#"{
            "before": null,
            "after": {"id": 123, "name": "test"},
            "source": {
                "name": "postgres",
                "db": "mydb",
                "schema": "public",
                "table": "customers",
                "lsn": "12345",
                "txId": 67890
            },
            "op": "c",
            "ts_ms": 1234567890
        }"#;

        let event: DebeziumEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.op, "c");
        assert_eq!(event.source.table, "customers");
    }

    #[test]
    fn test_graphica_lineage_event_parsing() {
        let json = r#"{
            "data": {
                "customer_id": 1,
                "email": "customer1@example.com",
                "first_name": "First1"
            },
            "dataset": "customers",
            "id": "CUST-000001",
            "source": {
                "cdc_position": {
                    "lsn": 100001,
                    "txid": 501
                },
                "path": "postgres.retail.customers",
                "system": "debezium"
            },
            "tenant_id": "retail_tenant",
            "timestamp": 1759441465528
        }"#;

        let event: GraphicaLineageEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.dataset, "customers");
        assert_eq!(event.id, "CUST-000001");
        assert_eq!(event.tenant_id, "retail_tenant");
        assert_eq!(event.source.system, "debezium");
    }
}
