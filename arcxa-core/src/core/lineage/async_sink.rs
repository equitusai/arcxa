//! # Async LineageSink Trait
//!
//! Async version of LineageSink for non-blocking I/O operations.
//!
//! Enables full async architecture with Tokio for maximum concurrency.

use super::LineageEvent;
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};

/// Async trait for storing and querying lineage
///
/// All operations are non-blocking, enabling high concurrency with Tokio.
#[async_trait]
pub trait AsyncLineageSink: Send + Sync {
    /// Persist lineage event (async)
    async fn write(&self, event: LineageEvent) -> Result<()>;

    /// Persist multiple lineage events in a batch (async)
    async fn write_batch(&self, events: Vec<LineageEvent>) -> Result<()> {
        // Default implementation: sequential writes
        // Implementations should override for true batch optimization
        for event in events {
            self.write(event).await?;
        }
        Ok(())
    }

    /// Query lineage for a specific record (async)
    async fn get_record_lineage(&self, record_id: &str) -> Result<Vec<LineageEvent>>;

    /// Query all data affected by a model (async)
    async fn get_model_impact(&self, model_id: &str, version: &str) -> Result<Vec<LineageEvent>>;

    /// Query lineage by time range (async)
    async fn query_by_time_range(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<LineageEvent>>;

    /// Get lineage for a specific run (async)
    async fn get_run_lineage(&self, run_id: &str) -> Result<Vec<LineageEvent>>;

    /// Time-travel query: Get lineage as it existed at a specific timestamp (async)
    async fn get_lineage_as_of(
        &self,
        record_id: &str,
        as_of: DateTime<Utc>,
    ) -> Result<Vec<LineageEvent>>;

    /// Flush any pending writes (async)
    async fn flush(&self) -> Result<()> {
        // Default: no-op
        // Implementations with buffering should override
        Ok(())
    }

    /// Health check (async)
    async fn health_check(&self) -> Result<()> {
        // Default implementation: try a dummy query
        let _ = self.get_record_lineage("__health_check__").await?;
        Ok(())
    }
}

/// Adapter to wrap sync LineageSink in async
pub struct SyncToAsyncAdapter<T> {
    inner: std::sync::Arc<T>,
}

impl<T> SyncToAsyncAdapter<T> {
    pub fn new(inner: T) -> Self {
        Self {
            inner: std::sync::Arc::new(inner),
        }
    }

    pub fn from_arc(inner: std::sync::Arc<T>) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl<T> AsyncLineageSink for SyncToAsyncAdapter<T>
where
    T: super::LineageSink + Send + Sync + 'static,
{
    async fn write(&self, event: LineageEvent) -> Result<()> {
        // Clone Arc to move into closure
        let inner = self.inner.clone();
        tokio::task::spawn_blocking(move || inner.write(event))
            .await
            .map_err(|e| anyhow::anyhow!("Spawn blocking failed: {}", e))?
    }

    async fn get_record_lineage(&self, record_id: &str) -> Result<Vec<LineageEvent>> {
        let record_id = record_id.to_string();
        let inner = self.inner.clone();
        tokio::task::spawn_blocking(move || inner.get_record_lineage(&record_id))
            .await
            .map_err(|e| anyhow::anyhow!("Spawn blocking failed: {}", e))?
    }

    async fn get_model_impact(&self, model_id: &str, version: &str) -> Result<Vec<LineageEvent>> {
        let model_id = model_id.to_string();
        let version = version.to_string();
        let inner = self.inner.clone();
        tokio::task::spawn_blocking(move || inner.get_model_impact(&model_id, &version))
            .await
            .map_err(|e| anyhow::anyhow!("Spawn blocking failed: {}", e))?
    }

    async fn query_by_time_range(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<LineageEvent>> {
        let inner = self.inner.clone();
        tokio::task::spawn_blocking(move || inner.query_by_time_range(start, end))
            .await
            .map_err(|e| anyhow::anyhow!("Spawn blocking failed: {}", e))?
    }

    async fn get_run_lineage(&self, run_id: &str) -> Result<Vec<LineageEvent>> {
        let run_id = run_id.to_string();
        let inner = self.inner.clone();
        tokio::task::spawn_blocking(move || inner.get_run_lineage(&run_id))
            .await
            .map_err(|e| anyhow::anyhow!("Spawn blocking failed: {}", e))?
    }

    async fn get_lineage_as_of(
        &self,
        record_id: &str,
        as_of: DateTime<Utc>,
    ) -> Result<Vec<LineageEvent>> {
        let record_id = record_id.to_string();
        let inner = self.inner.clone();
        tokio::task::spawn_blocking(move || inner.get_lineage_as_of(&record_id, as_of))
            .await
            .map_err(|e| anyhow::anyhow!("Spawn blocking failed: {}", e))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::lineage::{DataRef, LineageSink};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    // Mock sync implementation for testing
    struct MockSyncSink {
        events: Arc<Mutex<Vec<LineageEvent>>>,
    }

    impl MockSyncSink {
        fn new() -> Self {
            Self {
                events: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    impl LineageSink for MockSyncSink {
        fn write(&self, event: LineageEvent) -> Result<()> {
            self.events.lock().unwrap().push(event);
            Ok(())
        }

        fn get_record_lineage(&self, record_id: &str) -> Result<Vec<LineageEvent>> {
            let events = self.events.lock().unwrap();
            Ok(events
                .iter()
                .filter(|e| e.record_id == record_id)
                .cloned()
                .collect())
        }

        fn get_model_impact(&self, _model_id: &str, _version: &str) -> Result<Vec<LineageEvent>> {
            Ok(vec![])
        }

        fn query_by_time_range(
            &self,
            _start: DateTime<Utc>,
            _end: DateTime<Utc>,
        ) -> Result<Vec<LineageEvent>> {
            Ok(vec![])
        }

        fn get_run_lineage(&self, _run_id: &str) -> Result<Vec<LineageEvent>> {
            Ok(vec![])
        }

        fn get_lineage_as_of(
            &self,
            _record_id: &str,
            _as_of: DateTime<Utc>,
        ) -> Result<Vec<LineageEvent>> {
            Ok(vec![])
        }
    }

    #[tokio::test]
    async fn test_sync_to_async_adapter() {
        let sync_sink = MockSyncSink::new();
        let async_sink = SyncToAsyncAdapter::new(sync_sink);

        let event = LineageEvent {
            id: uuid::Uuid::new_v4(),
            dataset: "test".to_string(),
            record_id: "rec123".to_string(),
            source_refs: vec![],
            transforms: vec![],
            model_refs: vec![],
            output_ref: DataRef {
                system: "test".to_string(),
                path: "test".to_string(),
                version: None,
                extracted_at: Utc::now(),
                cdc_position: None,
            },
            ts: Utc::now(),
            run_id: "run1".to_string(),
            tenant_id: "tenant1".to_string(),
            correlation_id: None,
            metadata: HashMap::new(),
        };

        // Write event
        async_sink.write(event.clone()).await.unwrap();

        // Query event
        let events = async_sink.get_record_lineage("rec123").await.unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].record_id, "rec123");
    }

    #[tokio::test]
    async fn test_async_write_batch() {
        let sync_sink = MockSyncSink::new();
        let async_sink = SyncToAsyncAdapter::new(sync_sink);

        let events: Vec<LineageEvent> = (0..10)
            .map(|i| LineageEvent {
                id: uuid::Uuid::new_v4(),
                dataset: "test".to_string(),
                record_id: format!("rec{}", i),
                source_refs: vec![],
                transforms: vec![],
                model_refs: vec![],
                output_ref: DataRef {
                    system: "test".to_string(),
                    path: "test".to_string(),
                    version: None,
                    extracted_at: Utc::now(),
                    cdc_position: None,
                },
                ts: Utc::now(),
                run_id: "run1".to_string(),
                tenant_id: "tenant1".to_string(),
                correlation_id: None,
                metadata: HashMap::new(),
            })
            .collect();

        // Write batch
        async_sink.write_batch(events).await.unwrap();

        // Verify all written
        for i in 0..10 {
            let result = async_sink
                .get_record_lineage(&format!("rec{}", i))
                .await
                .unwrap();
            assert_eq!(result.len(), 1);
        }
    }

    #[tokio::test]
    async fn test_health_check() {
        let sync_sink = MockSyncSink::new();
        let async_sink = SyncToAsyncAdapter::new(sync_sink);

        // Should not error
        async_sink.health_check().await.unwrap();
    }
}
