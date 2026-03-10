//! # Batch Processor
//!
//! Background processor that handles batching and flushing of lineage events.
//! Accumulates events, flushes based on size/timeout, and processes queries.

use crate::governance::async_config::AsyncGovernanceConfig;
use crate::governance::async_core::{AsyncBrainState, EventBatch, GovernanceMessage};
use crate::governance::converters::ToRdfTriples;
use crate::governance::rdf_store::{GraphicaRdfStore, RdfStore};
use anyhow::{Context, Result};
use graphica_core::core::lineage::LineageEvent;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// Background processor for batching and materializing lineage events
pub struct BatchProcessor {
    /// Configuration for batch processing
    config: AsyncGovernanceConfig,

    /// RDF store for materialization
    store: Arc<GraphicaRdfStore>,

    /// Shared state with metrics
    state: Arc<AsyncBrainState>,

    /// Message receiver
    rx: mpsc::Receiver<GovernanceMessage>,
}

impl BatchProcessor {
    /// Create new batch processor
    pub fn new(
        config: AsyncGovernanceConfig,
        store: Arc<GraphicaRdfStore>,
        state: Arc<AsyncBrainState>,
        rx: mpsc::Receiver<GovernanceMessage>,
    ) -> Self {
        Self {
            config,
            store,
            state,
            rx,
        }
    }

    /// Main processing loop
    ///
    /// Receives messages from channel, accumulates events into batches,
    /// and flushes based on size/timeout. Handles queries and metrics requests.
    pub async fn run(mut self) -> Result<()> {
        info!(
            "Batch processor started (batch_size: {}, timeout: {:?})",
            self.config.batch_size, self.config.batch_timeout
        );

        let mut batch = EventBatch::new();
        let mut shutdown = false;

        while !shutdown {
            // Calculate remaining time until timeout
            let elapsed = batch.age();
            let remaining = if elapsed >= self.config.batch_timeout {
                tokio::time::Duration::from_millis(0)
            } else {
                let remaining_std = self.config.batch_timeout - elapsed;
                tokio::time::Duration::from_secs(remaining_std.as_secs())
                    + tokio::time::Duration::from_nanos(remaining_std.subsec_nanos() as u64)
            };

            // Wait for message or timeout
            tokio::select! {
                // Receive message from channel
                msg = self.rx.recv() => {
                    match msg {
                        Some(GovernanceMessage::MaterializeEvent(event)) => {
                            debug!("Received MaterializeEvent for record {}", event.record_id);
                            batch.add(event);

                            // Flush if batch size reached
                            if batch.len() >= self.config.batch_size {
                                self.flush_batch(&mut batch).await?;
                            }
                        }

                        Some(GovernanceMessage::ProcessBatch(events)) => {
                            debug!("Received ProcessBatch with {} events", events.len());

                            // Add all events to batch
                            for event in events {
                                batch.add(event);
                            }

                            // Flush if batch size reached
                            if batch.len() >= self.config.batch_size {
                                self.flush_batch(&mut batch).await?;
                            }
                        }

                        Some(GovernanceMessage::Query { sparql, response }) => {
                            debug!("Received Query request");

                            // Flush batch first to ensure consistency
                            if !batch.is_empty() {
                                self.flush_batch(&mut batch).await?;
                            }

                            // Execute query on the store
                            let result = self.store.as_ref().query(&sparql)
                                .context("Failed to execute SPARQL query");

                            // Send response (ignore send errors - receiver may have dropped)
                            let _ = response.send(result);
                        }

                        Some(GovernanceMessage::GetMetrics { response }) => {
                            debug!("Received GetMetrics request");

                            // Get current metrics snapshot
                            let metrics = self.state.get_metrics().await;

                            // Send response (ignore send errors)
                            let _ = response.send(metrics);
                        }

                        Some(GovernanceMessage::Shutdown) => {
                            info!("Received shutdown signal");
                            shutdown = true;

                            // Flush remaining events
                            if !batch.is_empty() {
                                self.flush_batch(&mut batch).await?;
                            }
                        }

                        None => {
                            warn!("Channel closed, shutting down processor");
                            shutdown = true;

                            // Flush remaining events
                            if !batch.is_empty() {
                                self.flush_batch(&mut batch).await?;
                            }
                        }
                    }
                }

                // Timeout - flush batch
                _ = tokio::time::sleep(remaining), if !batch.is_empty() => {
                    debug!("Batch timeout reached, flushing {} events", batch.len());
                    self.flush_batch(&mut batch).await?;
                }
            }
        }

        info!("Batch processor shut down gracefully");
        Ok(())
    }

    /// Flush current batch to RDF store
    ///
    /// Drains events from batch, converts to RDF triples, inserts into store,
    /// and updates metrics. Implements retry logic for failed events.
    async fn flush_batch(&self, batch: &mut EventBatch) -> Result<()> {
        if batch.is_empty() {
            return Ok(());
        }

        let flush_start = Instant::now();
        let events = batch.drain();
        let batch_size = events.len();

        debug!("Flushing batch of {} events", batch_size);

        let mut failed_count = 0;
        let mut retry_events = Vec::new();

        // Process each event
        for event in events {
            match self.materialize_event(&event).await {
                Ok(_) => {
                    debug!("Successfully materialized event {}", event.id);
                }
                Err(e) => {
                    error!("Failed to materialize event {}: {}", event.id, e);
                    failed_count += 1;
                    retry_events.push(event);
                }
            }
        }

        // Retry failed events
        if !retry_events.is_empty() && self.config.max_retries > 0 {
            for retry in 1..=self.config.max_retries {
                if retry_events.is_empty() {
                    break;
                }

                warn!(
                    "Retrying {} failed events (attempt {}/{})",
                    retry_events.len(),
                    retry,
                    self.config.max_retries
                );

                let mut still_failing = Vec::new();

                for event in retry_events.drain(..) {
                    match self.materialize_event(&event).await {
                        Ok(_) => {
                            debug!("Event {} succeeded on retry {}", event.id, retry);
                            failed_count -= 1;
                        }
                        Err(e) => {
                            error!("Event {} failed on retry {}: {}", event.id, retry, e);
                            still_failing.push(event);
                        }
                    }
                }

                retry_events = still_failing;
            }
        }

        // Final failed count after retries
        if !retry_events.is_empty() {
            error!(
                "Permanently failed to materialize {} events after {} retries",
                retry_events.len(),
                self.config.max_retries
            );
        }

        let flush_duration = flush_start.elapsed();

        // Update metrics
        self.state
            .record_batch(batch_size, flush_duration, failed_count)
            .await;

        info!(
            "Flushed batch: {} events, {} failed, {} ms",
            batch_size,
            failed_count,
            flush_duration.as_millis()
        );

        Ok(())
    }

    /// Materialize a single lineage event to RDF store
    async fn materialize_event(&self, event: &LineageEvent) -> Result<()> {
        // Convert event to RDF triples
        let triples = event
            .to_rdf_triples()
            .context("Failed to convert lineage event to RDF triples")?;

        // Insert each triple into store
        for (subject, predicate, object) in triples {
            self.store
                .as_ref()
                .insert_triple(&subject, &predicate, &object, None)
                .with_context(|| {
                    format!(
                        "Failed to insert triple: {} {} {}",
                        subject, predicate, object
                    )
                })?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use graphica_core::core::lineage::DataRef;
    use std::collections::HashMap;
    use std::time::Duration;
    use tokio::sync::oneshot;
    use uuid::Uuid;

    fn create_test_event(record_id: &str) -> LineageEvent {
        LineageEvent {
            id: Uuid::new_v4(),
            dataset: "test_dataset".to_string(),
            record_id: record_id.to_string(),
            source_refs: vec![],
            transforms: vec![],
            model_refs: vec![],
            output_ref: DataRef {
                system: "test_system".to_string(),
                path: "test/path".to_string(),
                version: Some("v1".to_string()),
                extracted_at: Utc::now(),
                cdc_position: None,
            },
            ts: Utc::now(),
            run_id: "test_run".to_string(),
            tenant_id: "test_tenant".to_string(),
            correlation_id: None,
            metadata: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn test_batch_processor_single_event() {
        let config = AsyncGovernanceConfig {
            batch_size: 10,
            batch_timeout: Duration::from_millis(100),
            ..Default::default()
        };

        let store = Arc::new(GraphicaRdfStore::new_in_memory().unwrap());
        let state = Arc::new(AsyncBrainState::new(config.clone()));
        let (tx, rx) = mpsc::channel(100);

        let processor = BatchProcessor::new(config, store.clone(), state.clone(), rx);

        // Spawn processor
        let processor_handle = tokio::spawn(async move { processor.run().await });

        // Send single event
        let event = create_test_event("rec1");
        tx.send(GovernanceMessage::MaterializeEvent(event))
            .await
            .unwrap();

        // Shutdown
        tx.send(GovernanceMessage::Shutdown).await.unwrap();
        drop(tx);

        // Wait for processor to finish
        processor_handle.await.unwrap().unwrap();

        // Check metrics
        let metrics = state.get_metrics().await;
        assert_eq!(metrics.processed_events, 1);
        assert_eq!(metrics.batches_processed, 1);
    }

    #[tokio::test]
    async fn test_batch_processor_size_flush() {
        let config = AsyncGovernanceConfig {
            batch_size: 3,
            batch_timeout: Duration::from_secs(10),
            ..Default::default()
        };

        let store = Arc::new(GraphicaRdfStore::new_in_memory().unwrap());
        let state = Arc::new(AsyncBrainState::new(config.clone()));
        let (tx, rx) = mpsc::channel(100);

        let processor = BatchProcessor::new(config, store.clone(), state.clone(), rx);

        let processor_handle = tokio::spawn(async move { processor.run().await });

        // Send 3 events - should trigger flush
        for i in 1..=3 {
            tx.send(GovernanceMessage::MaterializeEvent(create_test_event(
                &format!("rec{}", i),
            )))
            .await
            .unwrap();
        }

        // Give it time to flush
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Check metrics - batch should be flushed
        let metrics = state.get_metrics().await;
        assert_eq!(metrics.batches_processed, 1);
        assert_eq!(metrics.processed_events, 3);

        // Shutdown
        tx.send(GovernanceMessage::Shutdown).await.unwrap();
        drop(tx);
        processor_handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn test_batch_processor_timeout_flush() {
        let config = AsyncGovernanceConfig {
            batch_size: 100,
            batch_timeout: Duration::from_millis(50),
            ..Default::default()
        };

        let store = Arc::new(GraphicaRdfStore::new_in_memory().unwrap());
        let state = Arc::new(AsyncBrainState::new(config.clone()));
        let (tx, rx) = mpsc::channel(100);

        let processor = BatchProcessor::new(config, store.clone(), state.clone(), rx);

        let processor_handle = tokio::spawn(async move { processor.run().await });

        // Send 2 events (below batch_size)
        tx.send(GovernanceMessage::MaterializeEvent(create_test_event(
            "rec1",
        )))
        .await
        .unwrap();
        tx.send(GovernanceMessage::MaterializeEvent(create_test_event(
            "rec2",
        )))
        .await
        .unwrap();

        // Wait for timeout
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Check metrics - should be flushed by timeout
        let metrics = state.get_metrics().await;
        assert_eq!(metrics.batches_processed, 1);
        assert_eq!(metrics.processed_events, 2);

        // Shutdown
        tx.send(GovernanceMessage::Shutdown).await.unwrap();
        drop(tx);
        processor_handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn test_batch_processor_query() {
        let config = AsyncGovernanceConfig::default();
        let store = Arc::new(GraphicaRdfStore::new_in_memory().unwrap());
        let state = Arc::new(AsyncBrainState::new(config.clone()));
        let (tx, rx) = mpsc::channel(100);

        let processor = BatchProcessor::new(config, store.clone(), state.clone(), rx);

        let processor_handle = tokio::spawn(async move { processor.run().await });

        // Send events
        tx.send(GovernanceMessage::MaterializeEvent(create_test_event(
            "rec1",
        )))
        .await
        .unwrap();
        tx.send(GovernanceMessage::MaterializeEvent(create_test_event(
            "rec2",
        )))
        .await
        .unwrap();

        // Query
        let (query_tx, query_rx) = oneshot::channel();
        tx.send(GovernanceMessage::Query {
            sparql: "SELECT * WHERE { ?s ?p ?o } LIMIT 10".to_string(),
            response: query_tx,
        })
        .await
        .unwrap();

        let result = query_rx.await.unwrap();
        assert!(result.is_ok());

        // Shutdown
        tx.send(GovernanceMessage::Shutdown).await.unwrap();
        drop(tx);
        processor_handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn test_batch_processor_metrics() {
        let config = AsyncGovernanceConfig::default();
        let store = Arc::new(GraphicaRdfStore::new_in_memory().unwrap());
        let state = Arc::new(AsyncBrainState::new(config.clone()));
        let (tx, rx) = mpsc::channel(100);

        let processor = BatchProcessor::new(config, store.clone(), state.clone(), rx);

        let processor_handle = tokio::spawn(async move { processor.run().await });

        // Send events
        tx.send(GovernanceMessage::MaterializeEvent(create_test_event(
            "rec1",
        )))
        .await
        .unwrap();

        // Request metrics
        let (metrics_tx, metrics_rx) = oneshot::channel();
        tx.send(GovernanceMessage::GetMetrics {
            response: metrics_tx,
        })
        .await
        .unwrap();

        let metrics = metrics_rx.await.unwrap();
        assert!(metrics.total_events() <= 1); // May or may not be flushed yet

        // Shutdown
        tx.send(GovernanceMessage::Shutdown).await.unwrap();
        drop(tx);
        processor_handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn test_batch_processor_process_batch_message() {
        let config = AsyncGovernanceConfig {
            batch_size: 10,
            ..Default::default()
        };

        let store = Arc::new(GraphicaRdfStore::new_in_memory().unwrap());
        let state = Arc::new(AsyncBrainState::new(config.clone()));
        let (tx, rx) = mpsc::channel(100);

        let processor = BatchProcessor::new(config, store.clone(), state.clone(), rx);

        let processor_handle = tokio::spawn(async move { processor.run().await });

        // Send batch of events
        let events = vec![
            create_test_event("rec1"),
            create_test_event("rec2"),
            create_test_event("rec3"),
        ];

        tx.send(GovernanceMessage::ProcessBatch(events))
            .await
            .unwrap();

        // Wait a bit
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Shutdown and check
        tx.send(GovernanceMessage::Shutdown).await.unwrap();
        drop(tx);
        processor_handle.await.unwrap().unwrap();

        let metrics = state.get_metrics().await;
        assert_eq!(metrics.processed_events, 3);
    }
}
