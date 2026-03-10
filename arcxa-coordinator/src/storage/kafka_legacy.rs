//! # Kafka Lineage Sink
//!
//! Publish lineage events to Kafka for streaming consumers.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use graphica_core::core::lineage::{LineageEvent, LineageSink};
use rdkafka::config::ClientConfig;
use rdkafka::producer::{FutureProducer, FutureRecord};
use std::time::Duration;

pub struct KafkaLineageSink {
    producer: FutureProducer,
    topic: String,
}

impl KafkaLineageSink {
    pub fn new(brokers: &str) -> Result<Self> {
        let producer: FutureProducer = ClientConfig::new()
            .set("bootstrap.servers", brokers)
            .set("message.timeout.ms", "5000")
            .set("compression.type", "snappy")
            .create()
            .context("Failed to create Kafka producer")?;

        Ok(Self {
            producer,
            topic: "graphica.lineage.events".to_string(),
        })
    }

    pub fn with_topic(mut self, topic: String) -> Self {
        self.topic = topic;
        self
    }
}

impl LineageSink for KafkaLineageSink {
    fn write(&self, event: LineageEvent) -> Result<()> {
        let key = event.record_id.clone();
        let value = serde_json::to_vec(&event)?;

        let record = FutureRecord::to(&self.topic).key(&key).payload(&value);

        // Blocking send with timeout (ensures message is actually sent)
        // Using futures::executor::block_on or checking if we're in a tokio runtime
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            // We're in a tokio runtime - spawn task to avoid blocking
            let producer = self.producer.clone();
            let topic = self.topic.clone();
            let key_owned = key.clone();
            let value_owned = value.clone();

            handle.spawn(async move {
                let record = FutureRecord::to(&topic)
                    .key(&key_owned)
                    .payload(&value_owned);

                if let Err((e, _)) = producer.send(record, Duration::from_secs(5)).await {
                    eprintln!("Failed to send lineage event to Kafka: {:?}", e);
                }
            });
            Ok(())
        } else {
            // Not in tokio runtime - use blocking send
            futures::executor::block_on(async {
                self.producer
                    .send(record, Duration::from_secs(5))
                    .await
                    .map(|_| ())
                    .map_err(|(e, _)| anyhow::anyhow!("Kafka send failed: {:?}", e))
            })
        }
    }

    fn get_record_lineage(&self, _record_id: &str) -> Result<Vec<LineageEvent>> {
        anyhow::bail!("Kafka sink does not support queries")
    }

    fn get_model_impact(&self, _model_id: &str, _version: &str) -> Result<Vec<LineageEvent>> {
        anyhow::bail!("Kafka sink does not support queries")
    }

    fn query_by_time_range(
        &self,
        _start: DateTime<Utc>,
        _end: DateTime<Utc>,
    ) -> Result<Vec<LineageEvent>> {
        anyhow::bail!("Kafka sink does not support queries")
    }

    fn get_run_lineage(&self, _run_id: &str) -> Result<Vec<LineageEvent>> {
        anyhow::bail!("Kafka sink does not support queries")
    }

    fn get_lineage_as_of(
        &self,
        _record_id: &str,
        _as_of: DateTime<Utc>,
    ) -> Result<Vec<LineageEvent>> {
        anyhow::bail!("Kafka sink does not support queries")
    }
}
