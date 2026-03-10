//! OpenLineage HTTP Client
//!
//! This module provides an HTTP client for sending OpenLineage events to
//! OpenLineage-compatible backends like Marquez, DataHub, Egeria, etc.
//!
//! ## Example
//!
//! ```no_run
//! use graphica_core::openlineage::{OpenLineageClient, OpenLineageEvent, EventType};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let client = OpenLineageClient::new("http://marquez:5000".to_string())?;
//!
//!     let event = OpenLineageEvent::new(
//!         EventType::Complete,
//!         "run-123".to_string(),
//!         "graphica".to_string(),
//!         "etl.process_orders".to_string(),
//!         "https://github.com/graphica/graphica".to_string(),
//!     );
//!
//!     client.emit_event(&event).await?;
//!     Ok(())
//! }
//! ```

use super::event::OpenLineageEvent;
use anyhow::{Context, Result};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, error, info, warn};

/// OpenLineage HTTP client configuration
#[derive(Debug, Clone)]
pub struct OpenLineageClientConfig {
    /// Base URL of the OpenLineage backend (e.g., "http://marquez:5000")
    pub base_url: String,

    /// Optional API key for authentication
    pub api_key: Option<String>,

    /// Request timeout in seconds (default: 30)
    pub timeout_secs: u64,

    /// Number of retries on failure (default: 3)
    pub max_retries: u32,

    /// Retry delay in milliseconds (default: 1000)
    pub retry_delay_ms: u64,

    /// Enable compression for requests (default: true)
    pub enable_compression: bool,
}

impl Default for OpenLineageClientConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:5000".to_string(),
            api_key: None,
            timeout_secs: 30,
            max_retries: 3,
            retry_delay_ms: 1000,
            enable_compression: true,
        }
    }
}

/// OpenLineage HTTP client
///
/// Sends OpenLineage events to compatible backends via HTTP POST requests.
pub struct OpenLineageClient {
    config: OpenLineageClientConfig,
    http_client: Client,
}

impl OpenLineageClient {
    /// Create a new OpenLineage client with default configuration
    pub fn new(base_url: String) -> Result<Self> {
        let config = OpenLineageClientConfig {
            base_url,
            ..Default::default()
        };
        Self::with_config(config)
    }

    /// Create a new OpenLineage client with custom configuration
    pub fn with_config(config: OpenLineageClientConfig) -> Result<Self> {
        let http_client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .user_agent("graphica-openlineage-client/0.1.0")
            .build()
            .context("Failed to build HTTP client")?;

        Ok(Self {
            config,
            http_client,
        })
    }

    /// Emit a single OpenLineage event
    ///
    /// Sends the event to the configured backend endpoint.
    /// Automatically retries on transient failures.
    pub async fn emit_event(&self, event: &OpenLineageEvent) -> Result<()> {
        let url = format!("{}/api/v1/lineage", self.config.base_url);

        debug!(
            "Sending OpenLineage event: job={}.{}, run_id={}, event_type={:?}",
            event.job.namespace, event.job.name, event.run.run_id, event.event_type
        );

        let mut attempt = 0;
        loop {
            attempt += 1;

            match self.send_event_once(&url, event).await {
                Ok(_) => {
                    info!(
                        "Successfully sent OpenLineage event for run_id={}",
                        event.run.run_id
                    );
                    return Ok(());
                }
                Err(e) if attempt >= self.config.max_retries => {
                    error!(
                        "Failed to send OpenLineage event after {} attempts: {}",
                        attempt, e
                    );
                    return Err(e);
                }
                Err(e) => {
                    warn!(
                        "Failed to send OpenLineage event (attempt {}/{}): {}. Retrying...",
                        attempt, self.config.max_retries, e
                    );
                    tokio::time::sleep(Duration::from_millis(self.config.retry_delay_ms)).await;
                }
            }
        }
    }

    /// Send event once (internal method for retry logic)
    async fn send_event_once(&self, url: &str, event: &OpenLineageEvent) -> Result<()> {
        let mut request = self
            .http_client
            .post(url)
            .json(event)
            .header("Content-Type", "application/json");

        // Add API key if configured
        if let Some(api_key) = &self.config.api_key {
            request = request.header("Authorization", format!("Bearer {}", api_key));
        }

        let response = request
            .send()
            .await
            .context("Failed to send HTTP request")?;

        let status = response.status();

        match status {
            StatusCode::OK | StatusCode::CREATED | StatusCode::NO_CONTENT => {
                debug!("Event successfully accepted by backend: {}", status);
                Ok(())
            }
            StatusCode::BAD_REQUEST => {
                let error_body = response.text().await.unwrap_or_default();
                Err(anyhow::anyhow!(
                    "Bad request (400): Event rejected by backend. Details: {}",
                    error_body
                ))
            }
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Err(anyhow::anyhow!(
                "Authentication failed ({}): Check API key configuration",
                status
            )),
            StatusCode::SERVICE_UNAVAILABLE | StatusCode::GATEWAY_TIMEOUT => Err(anyhow::anyhow!(
                "Backend unavailable ({}): Service may be down",
                status
            )),
            _ => {
                let error_body = response.text().await.unwrap_or_default();
                Err(anyhow::anyhow!(
                    "Unexpected response ({}): {}",
                    status,
                    error_body
                ))
            }
        }
    }

    /// Emit multiple events in batch
    ///
    /// Sends events sequentially with retry logic for each.
    /// Returns the number of successfully sent events.
    pub async fn emit_batch(&self, events: &[OpenLineageEvent]) -> Result<usize> {
        let mut success_count = 0;

        for (idx, event) in events.iter().enumerate() {
            match self.emit_event(event).await {
                Ok(_) => success_count += 1,
                Err(e) => {
                    error!(
                        "Failed to send event {}/{} (run_id={}): {}",
                        idx + 1,
                        events.len(),
                        event.run.run_id,
                        e
                    );
                    // Continue with next events even if one fails
                }
            }
        }

        info!(
            "Batch emission complete: {}/{} events sent successfully",
            success_count,
            events.len()
        );

        Ok(success_count)
    }

    /// Check if the OpenLineage backend is reachable
    pub async fn health_check(&self) -> Result<bool> {
        // Try to access the root endpoint or a known health endpoint
        let url = format!("{}/api/v1/namespaces", self.config.base_url);

        match self.http_client.get(&url).send().await {
            Ok(response) if response.status().is_success() => {
                info!(
                    "OpenLineage backend is reachable at {}",
                    self.config.base_url
                );
                Ok(true)
            }
            Ok(response) => {
                warn!(
                    "OpenLineage backend returned non-success status: {}",
                    response.status()
                );
                Ok(false)
            }
            Err(e) => {
                error!("Failed to reach OpenLineage backend: {}", e);
                Ok(false)
            }
        }
    }

    /// Get the configured base URL
    pub fn base_url(&self) -> &str {
        &self.config.base_url
    }
}

/// Response from OpenLineage backend (for future use)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenLineageResponse {
    /// Response message
    pub message: Option<String>,
    /// Response status
    pub status: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openlineage::EventType;

    #[test]
    fn test_client_creation() {
        let client = OpenLineageClient::new("http://marquez:5000".to_string()).unwrap();
        assert_eq!(client.base_url(), "http://marquez:5000");
    }

    #[test]
    fn test_client_with_custom_config() {
        let config = OpenLineageClientConfig {
            base_url: "http://datahub:8080".to_string(),
            api_key: Some("secret-key".to_string()),
            timeout_secs: 60,
            max_retries: 5,
            retry_delay_ms: 2000,
            enable_compression: false,
        };

        let client = OpenLineageClient::with_config(config.clone()).unwrap();
        assert_eq!(client.base_url(), "http://datahub:8080");
        assert_eq!(client.config.timeout_secs, 60);
        assert_eq!(client.config.max_retries, 5);
    }

    #[test]
    fn test_default_config() {
        let config = OpenLineageClientConfig::default();
        assert_eq!(config.base_url, "http://localhost:5000");
        assert_eq!(config.timeout_secs, 30);
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.retry_delay_ms, 1000);
        assert!(config.enable_compression);
        assert!(config.api_key.is_none());
    }

    // Note: Integration tests with actual HTTP calls should be in separate test file
    // These are just unit tests for configuration and client setup
}
