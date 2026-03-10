//! HTTP Client for Workflow Actions
//!
//! Provides reliable HTTP/HTTPS request execution for SendToHttp actions with:
//! - Timeout handling
//! - Retry logic with exponential backoff
//! - Custom headers support
//! - Request/response metrics
//! - Connection pooling

use anyhow::{Context, Result};
use reqwest::{Client, Method, StatusCode};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::time::Duration;
use tracing::{debug, error, info, warn};

/// HTTP client configuration
#[derive(Debug, Clone)]
pub struct HttpClientConfig {
    /// Request timeout in milliseconds
    pub timeout_ms: u64,

    /// Connect timeout in milliseconds
    pub connect_timeout_ms: u64,

    /// Maximum number of retries for transient failures
    pub max_retries: u32,

    /// Initial retry backoff in milliseconds
    pub retry_backoff_ms: u64,

    /// Maximum retry backoff in milliseconds
    pub max_retry_backoff_ms: u64,

    /// User agent string
    pub user_agent: String,

    /// Connection pool size
    pub pool_max_idle_per_host: usize,

    /// Follow redirects
    pub follow_redirects: bool,

    /// Max redirects to follow
    pub max_redirects: usize,
}

impl Default for HttpClientConfig {
    fn default() -> Self {
        Self {
            timeout_ms: 30000,
            connect_timeout_ms: 5000,
            max_retries: 3,
            retry_backoff_ms: 100,
            max_retry_backoff_ms: 5000,
            user_agent: "Graphica-Workflow/1.0".to_string(),
            pool_max_idle_per_host: 10,
            follow_redirects: true,
            max_redirects: 5,
        }
    }
}

/// HTTP client wrapper for workflow actions
pub struct HttpClient {
    /// reqwest client
    client: Client,

    /// Configuration
    config: HttpClientConfig,
}

/// HTTP request result
#[derive(Debug, Clone)]
pub struct HttpResponse {
    /// HTTP status code
    pub status: u16,

    /// Response body as string
    pub body: String,

    /// Response headers
    pub headers: HashMap<String, String>,

    /// Request latency in milliseconds
    pub latency_ms: u64,

    /// Number of retries performed
    pub retries: u32,
}

impl HttpClient {
    /// Create a new HTTP client
    pub fn new(config: HttpClientConfig) -> Result<Self> {
        info!(
            "Creating HTTP client: timeout={}ms, max_retries={}",
            config.timeout_ms, config.max_retries
        );

        let mut client_builder = Client::builder()
            .timeout(Duration::from_millis(config.timeout_ms))
            .connect_timeout(Duration::from_millis(config.connect_timeout_ms))
            .user_agent(&config.user_agent)
            .pool_max_idle_per_host(config.pool_max_idle_per_host)
            .use_rustls_tls(); // Use rustls for TLS

        // Configure redirects
        if config.follow_redirects {
            client_builder =
                client_builder.redirect(reqwest::redirect::Policy::limited(config.max_redirects));
        } else {
            client_builder = client_builder.redirect(reqwest::redirect::Policy::none());
        }

        let client = client_builder
            .build()
            .context("Failed to create HTTP client")?;

        info!("HTTP client created successfully");

        Ok(Self { client, config })
    }

    /// Send an HTTP request with JSON payload
    ///
    /// ## Arguments
    /// * `method` - HTTP method (GET, POST, PUT, DELETE, etc.)
    /// * `url` - Target URL
    /// * `payload` - Optional JSON payload for POST/PUT
    /// * `headers` - Optional custom headers
    ///
    /// ## Returns
    /// HTTP response with status, body, headers, and latency
    pub async fn send_json(
        &self,
        method: &str,
        url: &str,
        payload: Option<&JsonValue>,
        headers: Option<&HashMap<String, String>>,
    ) -> Result<HttpResponse> {
        let start = std::time::Instant::now();

        debug!(
            "Sending HTTP {} to '{}' with payload: {:?}",
            method, url, payload
        );

        let http_method = method
            .parse::<Method>()
            .with_context(|| format!("Invalid HTTP method: {}", method))?;

        let mut retries = 0;
        let mut backoff_ms = self.config.retry_backoff_ms;

        loop {
            // Build request
            let mut request = self.client.request(http_method.clone(), url);

            // Add headers
            if let Some(header_map) = headers {
                for (key, value) in header_map {
                    request = request.header(key, value);
                }
            }

            // Add JSON payload if present
            if let Some(json_payload) = payload {
                request = request.json(json_payload);
            }

            // Send request
            match request.send().await {
                Ok(response) => {
                    let status = response.status();
                    let status_code = status.as_u16();

                    // Extract headers
                    let mut response_headers = HashMap::new();
                    for (key, value) in response.headers() {
                        if let Ok(value_str) = value.to_str() {
                            response_headers.insert(key.to_string(), value_str.to_string());
                        }
                    }

                    // Read body
                    let body = response
                        .text()
                        .await
                        .context("Failed to read response body")?;

                    let latency_ms = start.elapsed().as_millis() as u64;

                    // Check if we should retry on 5xx errors
                    if status.is_server_error() && retries < self.config.max_retries {
                        warn!(
                            "HTTP {} returned {}, retrying ({}/{})",
                            url,
                            status_code,
                            retries + 1,
                            self.config.max_retries
                        );

                        retries += 1;
                        tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                        backoff_ms = (backoff_ms * 2).min(self.config.max_retry_backoff_ms);
                        continue;
                    }

                    debug!(
                        "HTTP {} completed: status={}, latency={}ms, retries={}",
                        url, status_code, latency_ms, retries
                    );

                    return Ok(HttpResponse {
                        status: status_code,
                        body,
                        headers: response_headers,
                        latency_ms,
                        retries,
                    });
                }
                Err(err) => {
                    // Retry on network errors
                    if retries < self.config.max_retries {
                        warn!(
                            "HTTP {} failed: {}, retrying ({}/{})",
                            url,
                            err,
                            retries + 1,
                            self.config.max_retries
                        );

                        retries += 1;
                        tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                        backoff_ms = (backoff_ms * 2).min(self.config.max_retry_backoff_ms);
                        continue;
                    } else {
                        error!("HTTP {} failed after {} retries: {}", url, retries, err);
                        return Err(anyhow::anyhow!(
                            "HTTP request to '{}' failed after {} retries: {}",
                            url,
                            retries,
                            err
                        ));
                    }
                }
            }
        }
    }

    /// Send a GET request
    pub async fn get(
        &self,
        url: &str,
        headers: Option<&HashMap<String, String>>,
    ) -> Result<HttpResponse> {
        self.send_json("GET", url, None, headers).await
    }

    /// Send a POST request with JSON
    pub async fn post(
        &self,
        url: &str,
        payload: &JsonValue,
        headers: Option<&HashMap<String, String>>,
    ) -> Result<HttpResponse> {
        self.send_json("POST", url, Some(payload), headers).await
    }

    /// Send a PUT request with JSON
    pub async fn put(
        &self,
        url: &str,
        payload: &JsonValue,
        headers: Option<&HashMap<String, String>>,
    ) -> Result<HttpResponse> {
        self.send_json("PUT", url, Some(payload), headers).await
    }

    /// Send a DELETE request
    pub async fn delete(
        &self,
        url: &str,
        headers: Option<&HashMap<String, String>>,
    ) -> Result<HttpResponse> {
        self.send_json("DELETE", url, None, headers).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_config_default() {
        let config = HttpClientConfig::default();
        assert_eq!(config.timeout_ms, 30000);
        assert_eq!(config.max_retries, 3);
        assert!(config.follow_redirects);
        assert_eq!(config.pool_max_idle_per_host, 10);
    }

    #[test]
    fn test_http_client_creation() {
        let config = HttpClientConfig::default();
        let result = HttpClient::new(config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_method_parsing() {
        assert!("GET".parse::<Method>().is_ok());
        assert!("POST".parse::<Method>().is_ok());
        assert!("PUT".parse::<Method>().is_ok());
        assert!("DELETE".parse::<Method>().is_ok());
        assert!("PATCH".parse::<Method>().is_ok());
        // Note: reqwest's Method type also accepts custom/extension methods
        // so "INVALID" would actually parse successfully
    }

    #[test]
    fn test_exponential_backoff() {
        let config = HttpClientConfig::default();
        let mut backoff = config.retry_backoff_ms;

        // First retry: 100ms
        assert_eq!(backoff, 100);

        // Second retry: 200ms
        backoff = (backoff * 2).min(config.max_retry_backoff_ms);
        assert_eq!(backoff, 200);

        // Third retry: 400ms
        backoff = (backoff * 2).min(config.max_retry_backoff_ms);
        assert_eq!(backoff, 400);

        // Eventually capped at max
        for _ in 0..10 {
            backoff = (backoff * 2).min(config.max_retry_backoff_ms);
        }
        assert_eq!(backoff, config.max_retry_backoff_ms);
    }
}
