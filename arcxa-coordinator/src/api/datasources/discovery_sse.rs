//! # Server-Sent Events (SSE) for Real-time Discovery Progress
//!
//! Streams discovery progress updates to clients in real-time.
//!
//! ## Usage
//!
//! ```javascript
//! const eventSource = new EventSource(
//!   '/api/v1/datasources/ds-123/discovery/stream?discovery_id=uuid-here'
//! );
//!
//! eventSource.onmessage = (event) => {
//!   const progress = JSON.parse(event.data);
//!   console.log(`Progress: ${progress.percent_complete}%`);
//!   console.log(`Step: ${progress.current_step}`);
//!
//!   if (progress.status === 'completed' || progress.status === 'failed') {
//!     eventSource.close();
//!   }
//! };
//!
//! eventSource.onerror = (error) => {
//!   console.error('SSE error:', error);
//!   eventSource.close();
//! };
//! ```

use axum::{
    extract::{Path, Query, State},
    response::sse::{Event, KeepAlive, Sse},
};
use futures::stream::{self, Stream, StreamExt};
use serde::Deserialize;
use std::convert::Infallible;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tokio_stream::StreamExt as _;
use tracing::{debug, info, warn};

use crate::api::ApiState;
use crate::mapping::discovery::{DiscoveryStateManager, DiscoveryStatus};

/// Query parameters for SSE stream
#[derive(Debug, Deserialize)]
pub struct StreamQueryParams {
    /// Discovery ID to stream
    pub discovery_id: String,

    /// Update interval in milliseconds (default: 500ms)
    #[serde(default = "default_update_interval")]
    pub update_interval_ms: u64,
}

fn default_update_interval() -> u64 {
    500
}

type EventStream = Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>>;

/// GET /api/v1/datasources/:id/discovery/stream
///
/// Server-Sent Events stream for real-time discovery progress updates.
/// Streams progress every 500ms (configurable) until completion/failure.
pub async fn stream_discovery_progress(
    State(state): State<Arc<ApiState>>,
    Path(datasource_id): Path<String>,
    Query(params): Query<StreamQueryParams>,
) -> Sse<EventStream> {
    info!(
        datasource_id = %datasource_id,
        discovery_id = %params.discovery_id,
        update_interval_ms = params.update_interval_ms,
        "Starting SSE stream for discovery progress"
    );

    let update_interval = Duration::from_millis(params.update_interval_ms);

    // Get state manager or return error stream
    let state_manager = match state.discovery_state.as_ref() {
        Some(sm) => sm.clone(),
        None => {
            warn!("Discovery state manager not initialized");
            return create_error_stream("Discovery service not available");
        }
    };

    let discovery_id = params.discovery_id.clone();

    // Create async stream that polls progress at intervals
    let progress_stream = stream::unfold(
        (state_manager, discovery_id, false),
        move |(state_manager, discovery_id, mut done)| async move {
            if done {
                return None;
            }

            // Get current progress
            let progress = match state_manager.get_progress(&discovery_id) {
                Some(p) => p,
                None => {
                    warn!(discovery_id = %discovery_id, "Discovery not found in stream");
                    let error_event = Event::default().event("error").data("Discovery not found");
                    done = true;
                    return Some((Ok(error_event), (state_manager, discovery_id, done)));
                }
            };

            debug!(
                discovery_id = %discovery_id,
                status = ?progress.status,
                percent = %progress.percent_complete,
                "Streaming progress update"
            );

            // Serialize progress to JSON
            let progress_json = match serde_json::to_string(&progress) {
                Ok(json) => json,
                Err(e) => {
                    warn!(discovery_id = %discovery_id, error = %e, "Failed to serialize progress");
                    let error_event = Event::default()
                        .event("error")
                        .data("Failed to serialize progress");
                    done = true;
                    return Some((Ok(error_event), (state_manager, discovery_id, done)));
                }
            };

            // Create SSE event
            let event = Event::default().event("progress").data(progress_json);

            // Check if discovery is complete
            match progress.status {
                DiscoveryStatus::Completed => {
                    info!(discovery_id = %discovery_id, "Discovery completed, closing stream");
                    done = true;
                }
                DiscoveryStatus::Failed => {
                    warn!(discovery_id = %discovery_id, "Discovery failed, closing stream");
                    done = true;
                }
                DiscoveryStatus::Cancelled => {
                    info!(discovery_id = %discovery_id, "Discovery cancelled, closing stream");
                    done = true;
                }
                _ => {
                    // Still running, wait before next update
                    sleep(update_interval).await;
                }
            }

            Some((Ok(event), (state_manager, discovery_id, done)))
        },
    );

    let boxed_stream: EventStream = Box::pin(progress_stream);

    Sse::new(boxed_stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}

/// Helper function to create an error stream with proper type
fn create_error_stream(error_message: &'static str) -> Sse<EventStream> {
    let error_stream = stream::unfold(
        (Some(error_message), false),
        move |(msg, sent)| async move {
            if sent {
                return None;
            }

            let msg = msg?;
            let error_event = Event::default().event("error").data(msg);

            Some((Ok(error_event), (None, true)))
        },
    );

    let boxed_stream: EventStream = Box::pin(error_stream);

    Sse::new(boxed_stream).keep_alive(KeepAlive::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::discovery::DiscoveryProgress;

    #[tokio::test]
    async fn test_stream_query_params_default() {
        let params: StreamQueryParams =
            serde_json::from_str(r#"{"discovery_id":"test-123"}"#).unwrap();
        assert_eq!(params.discovery_id, "test-123");
        assert_eq!(params.update_interval_ms, 500);
    }

    #[tokio::test]
    async fn test_stream_query_params_custom() {
        let params: StreamQueryParams =
            serde_json::from_str(r#"{"discovery_id":"test-123","update_interval_ms":1000}"#)
                .unwrap();
        assert_eq!(params.discovery_id, "test-123");
        assert_eq!(params.update_interval_ms, 1000);
    }
}
