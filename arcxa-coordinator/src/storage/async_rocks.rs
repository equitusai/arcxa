//! # Async RocksDB Wrapper
//!
//! Async wrapper around RocksLineageStore for non-blocking I/O.
//!
//! Enables concurrent read/write operations using Tokio runtime.

use crate::storage::rocks::RocksLineageStore;
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use graphica_core::core::lineage::{AsyncLineageSink, LineageEvent};
use std::sync::Arc;

/// Async wrapper around RocksLineageStore
///
/// Uses `tokio::task::spawn_blocking` to execute blocking RocksDB operations
/// in a thread pool, allowing concurrent operations without blocking the main
/// async runtime.
///
/// # Performance Benefits
///
/// - **Concurrent Reads:** Multiple read queries can run in parallel
/// - **Non-blocking Writes:** Write operations don't block read queries
/// - **Better Throughput:** Expected 2× improvement on read-heavy workloads
/// - **Resource Efficiency:** Tokio manages thread pool automatically
///
/// # Example
///
/// ```ignore
/// use graphica::storage::RocksLineageStore;
/// use graphica::storage::AsyncRocksLineageStore;
/// use std::sync::Arc;
/// use anyhow::Result;
///
/// #[tokio::main]
/// async fn main() -> Result<()> {
///     // Create sync store
///     let sync_store = Arc::new(RocksLineageStore::new("/data/lineage")?);
///
///     // Wrap in async interface
///     let async_store = AsyncRocksLineageStore::new(sync_store);
///
///     // Now use async API
///     let events = async_store.get_record_lineage("rec-123").await?;
///
///     Ok(())
/// }
/// ```
pub struct AsyncRocksLineageStore {
    inner: Arc<RocksLineageStore>,
}

impl AsyncRocksLineageStore {
    /// Create new async wrapper around RocksLineageStore
    pub fn new(store: Arc<RocksLineageStore>) -> Self {
        Self { inner: store }
    }

    /// Get reference to underlying sync store
    pub fn inner(&self) -> &Arc<RocksLineageStore> {
        &self.inner
    }
}

#[async_trait]
impl AsyncLineageSink for AsyncRocksLineageStore {
    async fn write(&self, event: LineageEvent) -> Result<()> {
        let inner = self.inner.clone();
        tokio::task::spawn_blocking(move || {
            use graphica_core::core::lineage::LineageSink;
            inner.write(event)
        })
        .await
        .map_err(|e| anyhow::anyhow!("Spawn blocking failed: {}", e))?
    }

    async fn write_batch(&self, events: Vec<LineageEvent>) -> Result<()> {
        let inner = self.inner.clone();
        tokio::task::spawn_blocking(move || inner.write_batch(events))
            .await
            .map_err(|e| anyhow::anyhow!("Spawn blocking failed: {}", e))?
    }

    async fn get_record_lineage(&self, record_id: &str) -> Result<Vec<LineageEvent>> {
        let record_id = record_id.to_string();
        let inner = self.inner.clone();
        tokio::task::spawn_blocking(move || {
            use graphica_core::core::lineage::LineageSink;
            inner.get_record_lineage(&record_id)
        })
        .await
        .map_err(|e| anyhow::anyhow!("Spawn blocking failed: {}", e))?
    }

    async fn get_model_impact(&self, model_id: &str, version: &str) -> Result<Vec<LineageEvent>> {
        let model_id = model_id.to_string();
        let version = version.to_string();
        let inner = self.inner.clone();
        tokio::task::spawn_blocking(move || {
            use graphica_core::core::lineage::LineageSink;
            inner.get_model_impact(&model_id, &version)
        })
        .await
        .map_err(|e| anyhow::anyhow!("Spawn blocking failed: {}", e))?
    }

    async fn query_by_time_range(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<LineageEvent>> {
        let inner = self.inner.clone();
        tokio::task::spawn_blocking(move || {
            use graphica_core::core::lineage::LineageSink;
            inner.query_by_time_range(start, end)
        })
        .await
        .map_err(|e| anyhow::anyhow!("Spawn blocking failed: {}", e))?
    }

    async fn get_run_lineage(&self, run_id: &str) -> Result<Vec<LineageEvent>> {
        let run_id = run_id.to_string();
        let inner = self.inner.clone();
        tokio::task::spawn_blocking(move || {
            use graphica_core::core::lineage::LineageSink;
            inner.get_run_lineage(&run_id)
        })
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
        tokio::task::spawn_blocking(move || {
            use graphica_core::core::lineage::LineageSink;
            inner.get_lineage_as_of(&record_id, as_of)
        })
        .await
        .map_err(|e| anyhow::anyhow!("Spawn blocking failed: {}", e))?
    }

    async fn flush(&self) -> Result<()> {
        // RocksDB writes are immediately durable by default
        // Writer pool auto-flushes based on batch_timeout
        // This is a no-op for compatibility with AsyncLineageSink trait
        if self.inner.is_writer_pool_enabled() {
            tracing::debug!("Flush called - writer pool auto-flushes based on timeout");
        }
        Ok(())
    }

    async fn health_check(&self) -> Result<()> {
        // Try a dummy query to verify RocksDB is responsive
        let inner = self.inner.clone();
        tokio::task::spawn_blocking(move || {
            use graphica_core::core::lineage::LineageSink;
            // Query for non-existent record - should succeed quickly
            inner.get_record_lineage("__health_check__").map(|_| ())
        })
        .await
        .map_err(|e| anyhow::anyhow!("Spawn blocking failed: {}", e))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphica_core::core::lineage::{DataRef, LineageSink};
    use std::collections::HashMap;
    use uuid::Uuid;

    fn create_test_event(record_id: &str) -> LineageEvent {
        LineageEvent {
            id: Uuid::new_v4(),
            dataset: "test".to_string(),
            record_id: record_id.to_string(),
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
            run_id: "test-run".to_string(),
            tenant_id: "test-tenant".to_string(),
            correlation_id: None,
            metadata: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn test_async_write_and_read() {
        let temp_dir = tempfile::tempdir().unwrap();
        let sync_store =
            Arc::new(RocksLineageStore::new(temp_dir.path().to_str().unwrap()).unwrap());
        let async_store = AsyncRocksLineageStore::new(sync_store);

        let event = create_test_event("rec-async-123");
        let record_id = event.record_id.clone();

        // Async write
        async_store.write(event).await.unwrap();

        // Async read
        let events = async_store.get_record_lineage(&record_id).await.unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].record_id, record_id);
    }

    #[tokio::test]
    async fn test_async_write_batch() {
        let temp_dir = tempfile::tempdir().unwrap();
        let sync_store =
            Arc::new(RocksLineageStore::new(temp_dir.path().to_str().unwrap()).unwrap());
        let async_store = AsyncRocksLineageStore::new(sync_store);

        let events: Vec<LineageEvent> = (0..10)
            .map(|i| create_test_event(&format!("rec-batch-{}", i)))
            .collect();

        // Async batch write
        async_store.write_batch(events).await.unwrap();

        // Verify all written
        for i in 0..10 {
            let result = async_store
                .get_record_lineage(&format!("rec-batch-{}", i))
                .await
                .unwrap();
            assert_eq!(result.len(), 1);
        }
    }

    #[tokio::test]
    async fn test_concurrent_reads() {
        let temp_dir = tempfile::tempdir().unwrap();
        let sync_store =
            Arc::new(RocksLineageStore::new(temp_dir.path().to_str().unwrap()).unwrap());
        let async_store = Arc::new(AsyncRocksLineageStore::new(sync_store));

        // Write test data
        for i in 0..5 {
            let event = create_test_event(&format!("rec-concurrent-{}", i));
            async_store.write(event).await.unwrap();
        }

        // Spawn concurrent reads
        let mut handles = vec![];
        for i in 0..5 {
            let store = async_store.clone();
            let handle = tokio::spawn(async move {
                store
                    .get_record_lineage(&format!("rec-concurrent-{}", i))
                    .await
                    .unwrap()
            });
            handles.push(handle);
        }

        // Wait for all reads to complete
        for handle in handles {
            let events = handle.await.unwrap();
            assert_eq!(events.len(), 1);
        }
    }

    #[tokio::test]
    async fn test_health_check() {
        let temp_dir = tempfile::tempdir().unwrap();
        let sync_store =
            Arc::new(RocksLineageStore::new(temp_dir.path().to_str().unwrap()).unwrap());
        let async_store = AsyncRocksLineageStore::new(sync_store);

        // Should not error
        async_store.health_check().await.unwrap();
    }

    #[tokio::test]
    async fn test_time_range_query() {
        let temp_dir = tempfile::tempdir().unwrap();
        let sync_store =
            Arc::new(RocksLineageStore::new(temp_dir.path().to_str().unwrap()).unwrap());
        let async_store = AsyncRocksLineageStore::new(sync_store);

        let now = Utc::now();
        let event = create_test_event("rec-time-range");
        async_store.write(event).await.unwrap();

        // Query time range
        let start = now - chrono::Duration::hours(1);
        let end = now + chrono::Duration::hours(1);
        let events = async_store.query_by_time_range(start, end).await.unwrap();

        assert_eq!(events.len(), 1);
    }
}
