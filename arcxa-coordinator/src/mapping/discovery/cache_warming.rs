//! # Background Cache Warming
//!
//! Event-driven cache warming system that pre-populates the discovery cache
//! when datasources are registered or updated.
//!
//! ## Architecture
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────┐
//! │           CatalogEvent (datasource registered)            │
//! └─────────────────────┬────────────────────────────────────┘
//!                       │
//!                       ▼
//! ┌──────────────────────────────────────────────────────────┐
//! │           CacheWarmingCoordinator                         │
//! │  - Receives datasource events                             │
//! │  - Spawns background warming tasks                        │
//! │  - Manages warming queue                                  │
//! └─────────────────────┬────────────────────────────────────┘
//!                       │
//!                       ▼
//! ┌──────────────────────────────────────────────────────────┐
//! │           Background Tokio Tasks                          │
//! │  - Asynchronous discovery                                 │
//! │  - Populates cache                                        │
//! │  - Fire-and-forget (errors logged)                        │
//! └──────────────────────────────────────────────────────────┘
//! ```

use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use super::service::DiscoveryService;

/// Datasource event types
#[derive(Debug, Clone)]
pub enum DataSourceEvent {
    /// Datasource was registered
    Registered { source_id: String },
    /// Datasource was updated
    Updated { source_id: String },
    /// Datasource was deleted
    Deleted { source_id: String },
}

/// Cache warming coordinator
///
/// Manages background cache warming tasks triggered by datasource events.
pub struct CacheWarmingCoordinator {
    /// Discovery service for cache warming
    discovery_service: Arc<dyn DiscoveryService>,
    /// Event channel sender
    event_tx: mpsc::UnboundedSender<DataSourceEvent>,
}

impl CacheWarmingCoordinator {
    /// Create a new cache warming coordinator
    ///
    /// Spawns a background task that processes datasource events
    /// and triggers cache warming operations.
    pub fn new(discovery_service: Arc<dyn DiscoveryService>) -> Self {
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();

        let service = discovery_service.clone();

        // Spawn background event processing task
        tokio::spawn(async move {
            info!("🔥 Cache warming coordinator started");

            while let Some(event) = event_rx.recv().await {
                match event {
                    DataSourceEvent::Registered { source_id } => {
                        info!("Datasource registered: {}, warming cache...", source_id);
                        Self::warm_cache_async(service.clone(), source_id).await;
                    }
                    DataSourceEvent::Updated { source_id } => {
                        info!("Datasource updated: {}, refreshing cache...", source_id);
                        Self::warm_cache_async(service.clone(), source_id).await;
                    }
                    DataSourceEvent::Deleted { source_id } => {
                        debug!(
                            "Datasource deleted: {} (cache will expire naturally)",
                            source_id
                        );
                        // Cache will expire based on TTL, no need to invalidate explicitly
                    }
                }
            }

            warn!("Cache warming coordinator stopped (channel closed)");
        });

        Self {
            discovery_service,
            event_tx,
        }
    }

    /// Notify the coordinator of a datasource event
    ///
    /// This is a fire-and-forget operation - events are queued
    /// and processed asynchronously in the background.
    pub fn notify(&self, event: DataSourceEvent) {
        if let Err(e) = self.event_tx.send(event) {
            warn!("Failed to send cache warming event: {}", e);
        }
    }

    /// Warm cache for a datasource (fire-and-forget)
    async fn warm_cache_async(service: Arc<dyn DiscoveryService>, source_id: String) {
        // Spawn a separate task so warming doesn't block event processing
        tokio::spawn(async move {
            service.warm_cache_for_source(&source_id).await;
        });
    }

    /// Trigger immediate cache warming (synchronous wait)
    ///
    /// Unlike notify(), this method waits for the warming to complete
    /// and is suitable for testing or manual cache warming.
    pub async fn warm_now(&self, source_id: &str) {
        info!("Manual cache warming triggered for: {}", source_id);
        self.discovery_service
            .warm_cache_for_source(source_id)
            .await;
    }
}

/// Extension trait for catalog to enable automatic cache warming
///
/// Implement this trait on your catalog to automatically trigger
/// cache warming when datasources are registered or updated.
pub trait CacheWarmingCatalog {
    /// Set the cache warming coordinator
    fn set_cache_warming(&mut self, coordinator: Arc<CacheWarmingCoordinator>);

    /// Notify cache warming on datasource events
    fn notify_cache_warming(&self, event: DataSourceEvent);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::discovery::MockDiscoveryService;

    #[tokio::test]
    async fn test_cache_warming_coordinator() {
        let service = Arc::new(MockDiscoveryService::new());
        let coordinator = CacheWarmingCoordinator::new(service);

        // Trigger cache warming event
        coordinator.notify(DataSourceEvent::Registered {
            source_id: "test_source".to_string(),
        });

        // Give background task time to process
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // If we get here without panicking, the coordinator is working
    }

    #[tokio::test]
    async fn test_warm_now() {
        let service = Arc::new(MockDiscoveryService::new());
        let coordinator = CacheWarmingCoordinator::new(service);

        // Trigger synchronous warming
        coordinator.warm_now("test_source").await;

        // Should complete without error
    }
}
