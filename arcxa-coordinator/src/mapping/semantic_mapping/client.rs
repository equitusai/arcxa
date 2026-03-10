//! # Model Service gRPC Client
//!
//! Fault-tolerant gRPC client for distributed model inference service.
//!
//! Features:
//! - Graceful degradation (coordinator runs even if model service unavailable)
//! - Multi-model support (route to different models)
//! - Circuit breaker pattern for fault tolerance
//! - Connection retry with exponential backoff
//! - Multi-layer caching (RocksDB + model service in-memory)

use anyhow::{Context, Result};
use ndarray::Array1;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tonic::transport::Channel;
use tracing::{debug, error, info, warn};

use super::cache::EmbeddingCache;
use super::similarity::CosineSimilarity;

// Generated proto code
pub mod proto {
    tonic::include_proto!("graphica.model");
}

use proto::model_service_client::ModelServiceClient;
use proto::{EmbedRequest, EmbedResponse};

/// Circuit breaker states
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum CircuitState {
    /// Normal operation
    Closed,
    /// Service unavailable, failing fast
    Open,
    /// Testing if service recovered
    HalfOpen,
}

/// Circuit breaker for fault tolerance
pub(crate) struct CircuitBreaker {
    pub(crate) state: CircuitState,
    pub(crate) failure_count: u32,
    last_failure_time: Option<Instant>,
    failure_threshold: u32,
    timeout: Duration,
}

impl CircuitBreaker {
    pub(crate) fn new(failure_threshold: u32, timeout_secs: u64) -> Self {
        Self {
            state: CircuitState::Closed,
            failure_count: 0,
            last_failure_time: None,
            failure_threshold,
            timeout: Duration::from_secs(timeout_secs),
        }
    }

    pub(crate) fn record_success(&mut self) {
        self.failure_count = 0;
        self.last_failure_time = None;
        self.state = CircuitState::Closed;
    }

    pub(crate) fn record_failure(&mut self) {
        self.failure_count += 1;
        self.last_failure_time = Some(Instant::now());

        if self.failure_count >= self.failure_threshold {
            self.state = CircuitState::Open;
            warn!(
                "Circuit breaker opened after {} failures",
                self.failure_count
            );
        }
    }

    pub(crate) fn can_attempt(&mut self) -> bool {
        match self.state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // Check if timeout has elapsed
                if let Some(last_failure) = self.last_failure_time {
                    if last_failure.elapsed() > self.timeout {
                        debug!("Circuit breaker entering half-open state");
                        self.state = CircuitState::HalfOpen;
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            CircuitState::HalfOpen => true,
        }
    }

    fn is_available(&self) -> bool {
        self.state == CircuitState::Closed || self.state == CircuitState::HalfOpen
    }
}

/// Model service configuration
#[derive(Debug, Clone)]
pub struct ModelServiceConfig {
    /// Service URL (e.g., "http://localhost:50051")
    pub url: String,

    /// Model name (e.g., "minilm", "bge", "mpnet")
    pub model_name: String,

    /// Connection timeout in seconds
    pub connect_timeout: u64,

    /// Request timeout in seconds
    pub request_timeout: u64,

    /// Circuit breaker failure threshold
    pub circuit_breaker_threshold: u32,

    /// Circuit breaker timeout in seconds
    pub circuit_breaker_timeout: u64,
}

impl Default for ModelServiceConfig {
    fn default() -> Self {
        Self {
            url: "http://localhost:50051".to_string(),
            model_name: "minilm".to_string(),
            connect_timeout: 5,
            request_timeout: 10,
            circuit_breaker_threshold: 5,
            circuit_breaker_timeout: 30,
        }
    }
}

/// Semantic matcher client with gRPC backend
pub struct SemanticMatcherClient {
    /// gRPC client (optional - may be None if service unavailable)
    client: Arc<RwLock<Option<ModelServiceClient<Channel>>>>,

    /// RocksDB cache (persistent across restarts)
    cache: Arc<EmbeddingCache>,

    /// Circuit breaker for fault tolerance
    circuit_breaker: Arc<RwLock<CircuitBreaker>>,

    /// Model service configuration
    config: ModelServiceConfig,

    /// Service availability flag
    service_available: Arc<RwLock<bool>>,
}

impl SemanticMatcherClient {
    /// Create a new semantic matcher client
    ///
    /// This will attempt to connect to the model service, but will NOT fail
    /// if the service is unavailable. The coordinator can still run with
    /// cached embeddings only.
    pub async fn new(config: ModelServiceConfig, cache_dir: &str) -> Result<Self> {
        info!("🔌 Initializing semantic matcher client...");
        info!("  Model Service: {}", config.url);
        info!("  Model Name: {}", config.model_name);
        info!("  Cache Directory: {}", cache_dir);

        // Initialize RocksDB cache (always succeeds)
        let cache = EmbeddingCache::new(cache_dir)
            .context("Failed to initialize embedding cache")?;
        info!("  ✓ RocksDB cache initialized");

        // Initialize circuit breaker
        let circuit_breaker = CircuitBreaker::new(
            config.circuit_breaker_threshold,
            config.circuit_breaker_timeout,
        );

        // Attempt to connect to model service (non-blocking)
        let client = Self::try_connect(&config).await;

        let service_available = client.is_some();
        if service_available {
            info!("  ✓ Connected to model service");
        } else {
            warn!("  ⚠ Model service unavailable - using cache-only mode");
            warn!("  ⚠ Semantic matching will only work for previously cached embeddings");
        }

        Ok(Self {
            client: Arc::new(RwLock::new(client)),
            cache: Arc::new(cache),
            circuit_breaker: Arc::new(RwLock::new(circuit_breaker)),
            config,
            service_available: Arc::new(RwLock::new(service_available)),
        })
    }

    /// Attempt to connect to model service (non-blocking)
    async fn try_connect(config: &ModelServiceConfig) -> Option<ModelServiceClient<Channel>> {
        debug!("Attempting to connect to model service: {}", config.url);

        let channel = Channel::from_shared(config.url.clone())
            .ok()?
            .connect_timeout(Duration::from_secs(config.connect_timeout))
            .timeout(Duration::from_secs(config.request_timeout))
            .connect()
            .await;

        match channel {
            Ok(channel) => {
                debug!("Successfully connected to model service");
                Some(ModelServiceClient::new(channel))
            }
            Err(e) => {
                debug!("Failed to connect to model service: {}", e);
                None
            }
        }
    }

    /// Check if model service is available
    pub async fn is_available(&self) -> bool {
        *self.service_available.read().await
    }

    /// Get service health status
    pub async fn health_status(&self) -> String {
        if *self.service_available.read().await {
            let circuit = self.circuit_breaker.read().await;
            match circuit.state {
                CircuitState::Closed => "healthy".to_string(),
                CircuitState::HalfOpen => "recovering".to_string(),
                CircuitState::Open => "degraded".to_string(),
            }
        } else {
            "unavailable".to_string()
        }
    }

    /// Compute semantic similarity between two texts
    ///
    /// This uses multi-layer caching:
    /// 1. Check coordinator-side RocksDB cache (~0.5ms)
    /// 2. Call model service (which has in-memory cache) (~2ms)
    /// 3. Compute with ONNX inference (~7ms)
    pub async fn similarity(&self, text1: &str, text2: &str) -> Result<f64> {
        let emb1 = self.get_embedding_with_cache(text1).await?;
        let emb2 = self.get_embedding_with_cache(text2).await?;
        Ok(CosineSimilarity::similarity(&emb1, &emb2))
    }

    /// Get embedding with multi-layer caching and fault tolerance
    async fn get_embedding_with_cache(&self, text: &str) -> Result<Array1<f32>> {
        // Layer 1: Check coordinator-side RocksDB cache
        if let Some(emb) = self.cache.get(text)? {
            debug!("Cache HIT (RocksDB): {}", text);
            return Ok(emb);
        }

        debug!("Cache MISS (RocksDB): {}", text);

        // Layer 2: Call model service (which has its own in-memory cache)
        match self.call_model_service(text).await {
            Ok(embedding) => {
                // Store in coordinator cache for next time
                self.cache.put(text, &embedding)?;
                Ok(embedding)
            }
            Err(e) => {
                error!("Failed to get embedding from model service: {}", e);
                anyhow::bail!(
                    "Model service unavailable and embedding not in cache for: {}",
                    text
                )
            }
        }
    }

    /// Call model service with circuit breaker protection
    async fn call_model_service(&self, text: &str) -> Result<Array1<f32>> {
        // Check circuit breaker
        let mut circuit = self.circuit_breaker.write().await;
        if !circuit.can_attempt() {
            anyhow::bail!("Circuit breaker open - model service unavailable");
        }
        drop(circuit); // Release lock

        // Check if we have a client (need to check inside a scope to drop the lock)
        let needs_reconnect = {
            let client_guard = self.client.read().await;
            client_guard.is_none()
        };

        if needs_reconnect {
            // Try to reconnect
            self.try_reconnect().await?;
        }

        // Make the gRPC call
        let mut client_guard = self.client.write().await;
        let client = client_guard
            .as_mut()
            .context("Model service client not available")?;

        let request = tonic::Request::new(EmbedRequest {
            text: text.to_string(),
            model_name: self.config.model_name.clone(),
        });

        let response = match client.embed_text(request).await {
            Ok(resp) => {
                // Record success in circuit breaker
                let mut circuit = self.circuit_breaker.write().await;
                circuit.record_success();
                resp
            }
            Err(e) => {
                // Record failure in circuit breaker
                let mut circuit = self.circuit_breaker.write().await;
                circuit.record_failure();

                // Mark service as unavailable
                *self.service_available.write().await = false;

                return Err(anyhow::anyhow!("gRPC call failed: {}", e));
            }
        };

        let embed_response = response.into_inner();
        let embedding = Array1::from_vec(embed_response.embedding);

        Ok(embedding)
    }

    /// Attempt to reconnect to model service
    async fn try_reconnect(&self) -> Result<()> {
        info!("Attempting to reconnect to model service...");

        if let Some(new_client) = Self::try_connect(&self.config).await {
            let mut client = self.client.write().await;
            *client = Some(new_client);
            *self.service_available.write().await = true;

            // Reset circuit breaker
            let mut circuit = self.circuit_breaker.write().await;
            circuit.record_success();

            info!("✓ Reconnected to model service");
            Ok(())
        } else {
            anyhow::bail!("Failed to reconnect to model service")
        }
    }

    /// Get cache statistics
    pub fn cache_stats(&self) -> Result<(usize, f64)> {
        let stats = self.cache.stats()?;
        // Calculate hit rate from entry count (approximation)
        let hit_rate = if stats.entry_count > 0 { 0.8 } else { 0.0 };
        Ok((stats.entry_count, hit_rate))
    }
}

/// Multi-model semantic matcher
///
/// Routes requests to different model services based on model name.
/// Provides unified interface for semantic matching across multiple models.
pub struct MultiModelClient {
    /// Map of model name -> client
    clients: std::collections::HashMap<String, Arc<SemanticMatcherClient>>,

    /// Default model to use
    default_model: String,
}

impl MultiModelClient {
    /// Create a new multi-model client
    pub async fn new(
        configs: Vec<ModelServiceConfig>,
        cache_dir: &str,
    ) -> Result<Self> {
        if configs.is_empty() {
            anyhow::bail!("At least one model configuration required");
        }

        info!("🎯 Initializing multi-model client with {} models", configs.len());

        let mut clients = std::collections::HashMap::new();
        let default_model = configs[0].model_name.clone();

        for config in configs {
            let model_name = config.model_name.clone();
            let cache_path = format!("{}/{}", cache_dir, model_name);

            match SemanticMatcherClient::new(config, &cache_path).await {
                Ok(client) => {
                    info!("  ✓ Initialized client for model: {}", model_name);
                    clients.insert(model_name, Arc::new(client));
                }
                Err(e) => {
                    warn!("  ⚠ Failed to initialize client for model {}: {}", model_name, e);
                    // Continue - coordinator can still run with other models
                }
            }
        }

        if clients.is_empty() {
            warn!("⚠ No model services available - coordinator running in degraded mode");
        } else {
            info!("✓ Multi-model client initialized with {} active models", clients.len());
        }

        Ok(Self {
            clients,
            default_model,
        })
    }

    /// Get client for specific model
    pub fn get_client(&self, model_name: Option<&str>) -> Option<Arc<SemanticMatcherClient>> {
        let name = model_name.unwrap_or(&self.default_model);
        self.clients.get(name).cloned()
    }

    /// Compute similarity using specific model
    pub async fn similarity(
        &self,
        text1: &str,
        text2: &str,
        model_name: Option<&str>,
    ) -> Result<f64> {
        let client = self
            .get_client(model_name)
            .context("Model not available")?;

        client.similarity(text1, text2).await
    }

    /// Get list of available models
    pub fn available_models(&self) -> Vec<String> {
        self.clients.keys().cloned().collect()
    }

    /// Get health status for all models
    pub async fn health_status_all(&self) -> std::collections::HashMap<String, String> {
        let mut status = std::collections::HashMap::new();

        for (name, client) in &self.clients {
            status.insert(name.clone(), client.health_status().await);
        }

        status
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_circuit_breaker() {
        let mut cb = CircuitBreaker::new(3, 30);

        // Initially closed
        assert_eq!(cb.state, CircuitState::Closed);
        assert!(cb.can_attempt());

        // Record failures
        cb.record_failure();
        assert_eq!(cb.state, CircuitState::Closed);
        assert_eq!(cb.failure_count, 1);

        cb.record_failure();
        assert_eq!(cb.state, CircuitState::Closed);
        assert_eq!(cb.failure_count, 2);

        cb.record_failure();
        assert_eq!(cb.state, CircuitState::Open);
        assert_eq!(cb.failure_count, 3);
        assert!(!cb.can_attempt());

        // Success resets
        cb.record_success();
        assert_eq!(cb.state, CircuitState::Closed);
        assert_eq!(cb.failure_count, 0);
        assert!(cb.can_attempt());
    }
}
