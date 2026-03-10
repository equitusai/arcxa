//! Server-Sent Events (SSE) for Real-Time Progress Streaming
//!
//! Provides real-time updates for batch job execution progress via SSE.
//!
//! ## Example
//!
//! ```javascript
//! const eventSource = new EventSource('/api/v1/batch-jobs/batch_123/stream');
//!
//! eventSource.addEventListener('progress', (event) => {
//!     const data = JSON.parse(event.data);
//!     console.log(`Progress: ${data.progress_percent}%`);
//! });
//!
//! eventSource.addEventListener('completed', (event) => {
//!     console.log('Batch job completed!');
//!     eventSource.close();
//! });
//! ```

use crate::workflows::domain::{BatchJobStatus, WorkflowExecutionStatus};
use crate::workflows::storage::BatchJobStore;
use axum::{
    extract::{Path, State},
    response::{
        sse::{Event, KeepAlive},
        Sse,
    },
};
use futures::stream::{self, Stream};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tokio_stream::StreamExt as _;
use tracing::{debug, error, info};

use super::batch_handlers::BatchJobApiState;
use super::handlers::ApiError;

/// Batch job progress event
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProgressEvent {
    /// Initial connection established
    Connected {
        job_id: String,
        status: BatchJobStatus,
    },

    /// Progress update
    Progress {
        job_id: String,
        status: BatchJobStatus,
        total_files: usize,
        pending: usize,
        in_progress: usize,
        completed: usize,
        failed: usize,
        retrying: usize,
        progress_percent: f64,
        current_wave: Option<usize>,
        total_waves: Option<usize>,
    },

    /// Workflow execution update
    WorkflowUpdate {
        job_id: String,
        execution_id: String,
        file_name: String,
        status: WorkflowExecutionStatus,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        rows_processed: Option<usize>,
    },

    /// Batch job completed
    Completed {
        job_id: String,
        status: BatchJobStatus,
        total_completed: usize,
        total_failed: usize,
        duration_ms: Option<i64>,
    },

    /// Batch job failed
    Failed {
        job_id: String,
        error: String,
        failed_count: usize,
    },

    /// Batch job cancelled
    Cancelled {
        job_id: String,
        completed_count: usize,
        pending_count: usize,
    },

    /// Heartbeat to keep connection alive
    Heartbeat {
        timestamp: chrono::DateTime<chrono::Utc>,
    },
}

impl ProgressEvent {
    /// Convert to SSE event
    pub fn to_sse_event(&self) -> Result<Event, serde_json::Error> {
        let event_type = match self {
            ProgressEvent::Connected { .. } => "connected",
            ProgressEvent::Progress { .. } => "progress",
            ProgressEvent::WorkflowUpdate { .. } => "workflow_update",
            ProgressEvent::Completed { .. } => "completed",
            ProgressEvent::Failed { .. } => "failed",
            ProgressEvent::Cancelled { .. } => "cancelled",
            ProgressEvent::Heartbeat { .. } => "heartbeat",
        };

        let data = serde_json::to_string(self)?;

        Ok(Event::default().event(event_type).data(data))
    }
}

/// Stream batch job progress via SSE
///
/// GET /api/v1/batch-jobs/{id}/stream
pub async fn stream_batch_job_progress(
    State(state): State<Arc<BatchJobApiState>>,
    Path(job_id): Path<String>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    info!("Starting SSE stream for batch job: {}", job_id);

    // Verify batch job exists
    let batch_job = state
        .batch_store
        .get(&job_id)?
        .ok_or_else(|| ApiError::NotFound(format!("Batch job not found: {}", job_id)))?;

    // Create event stream
    let stream = create_progress_stream(state.batch_store.clone(), job_id.clone());

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

/// Create a progress event stream for a batch job
fn create_progress_stream(
    batch_store: Arc<BatchJobStore>,
    job_id: String,
) -> impl Stream<Item = Result<Event, Infallible>> {
    // Poll interval (1 second)
    let poll_interval = Duration::from_secs(1);

    // Heartbeat interval (30 seconds)
    let heartbeat_interval = Duration::from_secs(30);

    stream::unfold(
        (batch_store, job_id, 0, std::time::Instant::now()),
        move |(store, job_id, mut heartbeat_counter, last_heartbeat)| async move {
            // Check if we should send a heartbeat
            let now = std::time::Instant::now();
            let since_heartbeat = now.duration_since(last_heartbeat);

            if since_heartbeat >= heartbeat_interval {
                let heartbeat_event = ProgressEvent::Heartbeat {
                    timestamp: chrono::Utc::now(),
                };

                match heartbeat_event.to_sse_event() {
                    Ok(event) => {
                        return Some((Ok(event), (store, job_id, 0, now)));
                    }
                    Err(e) => {
                        error!("Failed to serialize heartbeat event: {}", e);
                    }
                }
            }

            // Poll batch job status
            match store.get(&job_id) {
                Ok(Some(batch_job)) => {
                    debug!(
                        "Batch job {} status: {:?}, progress: {:.1}%",
                        job_id, batch_job.status, batch_job.progress.progress_percent
                    );

                    // Create progress event
                    let event = if batch_job.status == BatchJobStatus::Completed {
                        ProgressEvent::Completed {
                            job_id: batch_job.job_id.clone(),
                            status: batch_job.status,
                            total_completed: batch_job.progress.completed,
                            total_failed: batch_job.progress.failed,
                            duration_ms: batch_job.duration_ms(),
                        }
                    } else if batch_job.status == BatchJobStatus::Failed {
                        ProgressEvent::Failed {
                            job_id: batch_job.job_id.clone(),
                            error: "Batch job execution failed".to_string(),
                            failed_count: batch_job.progress.failed,
                        }
                    } else if batch_job.status == BatchJobStatus::Cancelled {
                        ProgressEvent::Cancelled {
                            job_id: batch_job.job_id.clone(),
                            completed_count: batch_job.progress.completed,
                            pending_count: batch_job.progress.pending,
                        }
                    } else {
                        ProgressEvent::Progress {
                            job_id: batch_job.job_id.clone(),
                            status: batch_job.status,
                            total_files: batch_job.progress.total_files,
                            pending: batch_job.progress.pending,
                            in_progress: batch_job.progress.in_progress,
                            completed: batch_job.progress.completed,
                            failed: batch_job.progress.failed,
                            retrying: batch_job.progress.retrying,
                            progress_percent: batch_job.progress.progress_percent,
                            current_wave: None,
                            total_waves: None,
                        }
                    };

                    // Check if terminal state
                    let is_terminal = batch_job.is_terminal();

                    // Convert to SSE event
                    match event.to_sse_event() {
                        Ok(sse_event) => {
                            // If terminal state, end stream after this event
                            if is_terminal {
                                info!("Batch job {} reached terminal state, ending stream", job_id);
                                return Some((
                                    Ok(sse_event),
                                    (store, job_id, heartbeat_counter + 1, last_heartbeat),
                                ));
                            }

                            // Wait before next poll
                            sleep(poll_interval).await;

                            Some((
                                Ok(sse_event),
                                (store, job_id, heartbeat_counter + 1, last_heartbeat),
                            ))
                        }
                        Err(e) => {
                            error!("Failed to serialize progress event: {}", e);
                            sleep(poll_interval).await;
                            Some((
                                Ok(Event::default()
                                    .event("error")
                                    .data("Failed to serialize event")),
                                (store, job_id, heartbeat_counter + 1, last_heartbeat),
                            ))
                        }
                    }
                }
                Ok(None) => {
                    error!("Batch job {} not found, ending stream", job_id);
                    None // End stream
                }
                Err(e) => {
                    error!("Failed to fetch batch job {}: {}", job_id, e);
                    sleep(poll_interval).await;
                    Some((
                        Ok(Event::default()
                            .event("error")
                            .data(format!("Failed to fetch batch job: {}", e))),
                        (store, job_id, heartbeat_counter + 1, last_heartbeat),
                    ))
                }
            }
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_progress_event_serialization() {
        let event = ProgressEvent::Progress {
            job_id: "batch_123".to_string(),
            status: BatchJobStatus::Running,
            total_files: 20,
            pending: 5,
            in_progress: 3,
            completed: 10,
            failed: 2,
            retrying: 0,
            progress_percent: 60.0,
            current_wave: Some(2),
            total_waves: Some(4),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"progress\""));
        assert!(json.contains("\"job_id\":\"batch_123\""));
        assert!(json.contains("\"progress_percent\":60"));
    }

    #[test]
    fn test_completed_event_serialization() {
        let event = ProgressEvent::Completed {
            job_id: "batch_123".to_string(),
            status: BatchJobStatus::Completed,
            total_completed: 18,
            total_failed: 2,
            duration_ms: Some(120000),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"completed\""));
        assert!(json.contains("\"total_completed\":18"));
    }

    #[test]
    fn test_workflow_update_event_serialization() {
        let event = ProgressEvent::WorkflowUpdate {
            job_id: "batch_123".to_string(),
            execution_id: "exec_456".to_string(),
            file_name: "customers.csv".to_string(),
            status: WorkflowExecutionStatus::Completed,
            error: None,
            rows_processed: Some(10000),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"workflow_update\""));
        assert!(json.contains("\"file_name\":\"customers.csv\""));
    }

    #[test]
    fn test_to_sse_event() {
        let event = ProgressEvent::Heartbeat {
            timestamp: chrono::Utc::now(),
        };

        let sse_event = event.to_sse_event().unwrap();
        // SSE event should be created successfully
    }
}
