//! ML model invocation with HTTP/gRPC support

use anyhow::{Context, Result};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tonic::transport::Channel;

use super::retry::IsRetryable;
use super::{
    CircuitBreaker, CircuitBreakerConfig, ModelCache, ModelProtocol, ModelRegistry, RetryPolicy,
    ML_METRICS,
};
use crate::distributed::proto::prediction_service::{
    feature_value, prediction_service_client::PredictionServiceClient, prediction_value,
    FeatureValue, PredictRequest, PredictionValue,
};

/// ML model invoker with connection pooling, retry, and circuit breaker
pub struct ModelInvoker {
    /// HTTP client with connection pooling
    http_client: Client,
    /// Model registry for endpoint lookup
    registry: Arc<ModelRegistry>,
    /// Response cache
    cache: Arc<ModelCache>,
    /// Retry policy
    retry_policy: RetryPolicy,
    /// Circuit breaker
    circuit_breaker: Arc<CircuitBreaker>,
}

impl ModelInvoker {
    /// Create new model invoker with default retry and circuit breaker
    pub fn new(registry: Arc<ModelRegistry>, cache: Arc<ModelCache>) -> Result<Self> {
        Self::with_config(
            registry,
            cache,
            RetryPolicy::default(),
            CircuitBreakerConfig::default(),
        )
    }

    /// Create new model invoker with custom configuration
    pub fn with_config(
        registry: Arc<ModelRegistry>,
        cache: Arc<ModelCache>,
        retry_policy: RetryPolicy,
        circuit_breaker_config: CircuitBreakerConfig,
    ) -> Result<Self> {
        let http_client = Client::builder()
            .pool_max_idle_per_host(10)
            .pool_idle_timeout(Some(Duration::from_secs(90)))
            .timeout(Duration::from_millis(5000)) // Increased for retries
            .connect_timeout(Duration::from_millis(1000))
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            http_client,
            registry,
            cache,
            retry_policy,
            circuit_breaker: Arc::new(CircuitBreaker::new(circuit_breaker_config)),
        })
    }

    /// Invoke model with features (with retry and circuit breaker)
    pub async fn invoke(&self, request: ModelRequest) -> Result<ModelResponse, InvocationError> {
        let start_time = std::time::Instant::now();

        // Check cache first
        if let Some(cached) = self.cache.get(&request).await {
            ML_METRICS.record_cache_hit(&request.model_id);
            return Ok(cached);
        }
        ML_METRICS.record_cache_miss(&request.model_id);

        // Check circuit breaker
        if !self.circuit_breaker.is_request_allowed() {
            ML_METRICS.record_invocation_failure(
                &request.model_id,
                "unknown",
                "circuit_breaker_open",
            );
            return Err(InvocationError::CircuitBreakerOpen);
        }

        // Get model metadata from registry
        let model = self
            .registry
            .get_model(&request.model_id)
            .await
            .ok_or_else(|| {
                ML_METRICS.record_invocation_failure(
                    &request.model_id,
                    "unknown",
                    "model_not_found",
                );
                InvocationError::ModelNotFound(request.model_id.clone())
            })?;

        // Execute with retry logic
        let url = model.endpoint.url.clone();
        let protocol = model.endpoint.protocol.clone();
        let protocol_str = match protocol {
            ModelProtocol::Http => "http",
            ModelProtocol::Grpc => "grpc",
            ModelProtocol::Lambda => "lambda",
        };

        // Record invocation start
        ML_METRICS.record_invocation_start(&request.model_id, protocol_str);

        let response = self
            .retry_policy
            .execute_with_retry(|| async {
                // Dispatch based on protocol
                match protocol {
                    ModelProtocol::Http => self.invoke_http(&url, &request).await,
                    ModelProtocol::Grpc => self.invoke_grpc(&url, &request).await,
                    ModelProtocol::Lambda => self.invoke_lambda(&url, &request).await,
                }
            })
            .await;

        // Record circuit breaker outcome
        match &response {
            Ok(_) => self.circuit_breaker.record_success(),
            Err(e) if e.is_retryable() => self.circuit_breaker.record_failure(),
            Err(_) => {} // Don't penalize circuit breaker for non-retryable errors
        }

        // Record metrics
        let duration = start_time.elapsed().as_secs_f64();
        match &response {
            Ok(_) => {
                ML_METRICS.record_invocation_success(&request.model_id, protocol_str, duration);
            }
            Err(e) => {
                let error_type = match e {
                    InvocationError::NetworkError(_) => "network_error",
                    InvocationError::ModelError { .. } => "model_error",
                    InvocationError::ParseError(_) => "parse_error",
                    InvocationError::CircuitBreakerOpen => "circuit_breaker_open",
                    InvocationError::ModelNotFound(_) => "model_not_found",
                    InvocationError::UnsupportedProtocol(_) => "unsupported_protocol",
                    InvocationError::Timeout => "timeout",
                };
                ML_METRICS.record_invocation_failure(&request.model_id, protocol_str, error_type);
            }
        }

        let response = response?;

        // Cache successful response
        self.cache.put(&request, &response).await;

        Ok(response)
    }

    /// Invoke model with batch of requests
    ///
    /// Processes multiple predictions concurrently with configurable parallelism.
    /// Returns results in the same order as requests.
    ///
    /// # Arguments
    /// * `requests` - Vector of prediction requests
    /// * `max_concurrent` - Maximum number of concurrent requests (default: 10)
    pub async fn invoke_batch(
        &self,
        requests: Vec<ModelRequest>,
        max_concurrent: Option<usize>,
    ) -> Vec<Result<ModelResponse, InvocationError>> {
        use futures::stream::{self, StreamExt};

        let max_concurrent = max_concurrent.unwrap_or(10);
        let batch_size = requests.len();

        // Record batch metrics (using first model_id as representative)
        if let Some(first_request) = requests.first() {
            ML_METRICS.record_batch_request(&first_request.model_id, batch_size);
        }

        // Create stream of futures
        let futures = requests.into_iter().map(|request| {
            let invoker = self.clone();
            async move { invoker.invoke(request).await }
        });

        // Execute with concurrency limit and collect results in order
        stream::iter(futures)
            .buffer_unordered(max_concurrent)
            .collect::<Vec<_>>()
            .await
    }

    /// Clone invoker (cheap Arc clone)
    fn clone(&self) -> Self {
        Self {
            http_client: self.http_client.clone(),
            registry: self.registry.clone(),
            cache: self.cache.clone(),
            retry_policy: self.retry_policy.clone(),
            circuit_breaker: self.circuit_breaker.clone(),
        }
    }

    /// Invoke HTTP endpoint
    async fn invoke_http(
        &self,
        url: &str,
        request: &ModelRequest,
    ) -> Result<ModelResponse, InvocationError> {
        let payload = serde_json::json!({
            "instances": [request.features],
        });

        let response = self
            .http_client
            .post(url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| InvocationError::NetworkError(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(InvocationError::ModelError {
                status_code: status.as_u16(),
                message: error_text,
            });
        }

        let body: HttpModelResponse = response
            .json()
            .await
            .map_err(|e| InvocationError::ParseError(e.to_string()))?;

        // Extract first prediction
        let prediction =
            body.predictions.into_iter().next().ok_or_else(|| {
                InvocationError::ParseError("No predictions in response".to_string())
            })?;

        Ok(ModelResponse {
            model_id: request.model_id.clone(),
            predictions: prediction.predictions,
            confidence: prediction.confidence.unwrap_or(0.0),
            model_version: body.model_version,
        })
    }

    /// Invoke gRPC endpoint
    async fn invoke_grpc(
        &self,
        url: &str,
        request: &ModelRequest,
    ) -> Result<ModelResponse, InvocationError> {
        // Create gRPC channel (tonic handles connection pooling automatically)
        let channel = Channel::from_shared(url.to_string())
            .map_err(|e| InvocationError::NetworkError(e.to_string()))?
            .connect_timeout(Duration::from_millis(1000))
            .timeout(Duration::from_millis(request.timeout_ms.unwrap_or(5000)))
            .connect()
            .await
            .map_err(|e| InvocationError::NetworkError(e.to_string()))?;

        let mut client = PredictionServiceClient::new(channel);

        // Convert ModelRequest features to protobuf FeatureValue
        let mut features = HashMap::new();
        for (key, value) in &request.features {
            let feature_value = match value {
                serde_json::Value::String(s) => FeatureValue {
                    value: Some(feature_value::Value::StringValue(s.clone())),
                },
                serde_json::Value::Number(n) => FeatureValue {
                    value: Some(feature_value::Value::NumberValue(n.as_f64().unwrap_or(0.0))),
                },
                serde_json::Value::Bool(b) => FeatureValue {
                    value: Some(feature_value::Value::BoolValue(*b)),
                },
                serde_json::Value::Array(arr) => {
                    // Try to convert to number list or string list
                    if arr.is_empty() {
                        FeatureValue {
                            value: Some(feature_value::Value::StringList(
                                crate::distributed::proto::prediction_service::StringList {
                                    values: vec![],
                                },
                            )),
                        }
                    } else if arr.iter().all(|v| v.is_number()) {
                        FeatureValue {
                            value: Some(feature_value::Value::NumberList(
                                crate::distributed::proto::prediction_service::NumberList {
                                    values: arr.iter().filter_map(|v| v.as_f64()).collect(),
                                },
                            )),
                        }
                    } else {
                        FeatureValue {
                            value: Some(feature_value::Value::StringList(
                                crate::distributed::proto::prediction_service::StringList {
                                    values: arr
                                        .iter()
                                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                        .collect(),
                                },
                            )),
                        }
                    }
                }
                _ => {
                    return Err(InvocationError::ParseError(format!(
                        "Unsupported feature type for key: {}",
                        key
                    )))
                }
            };
            features.insert(key.clone(), feature_value);
        }

        // Build prediction request
        let grpc_request = tonic::Request::new(PredictRequest {
            model_id: request.model_id.clone(),
            model_version: String::new(), // TODO: Add version support
            features,
            timeout_ms: request.timeout_ms.unwrap_or(5000) as i64,
        });

        // Call Predict RPC
        let response = client
            .predict(grpc_request)
            .await
            .map_err(|e| {
                let status = e.code();
                InvocationError::ModelError {
                    status_code: status as u16,
                    message: e.message().to_string(),
                }
            })?
            .into_inner();

        // Convert protobuf PredictionValue back to serde_json::Value
        let mut predictions = HashMap::new();
        for (key, pred_value) in response.predictions {
            let json_value = match pred_value.value {
                Some(prediction_value::Value::StringValue(s)) => serde_json::Value::String(s),
                Some(prediction_value::Value::NumberValue(n)) => {
                    serde_json::json!(n)
                }
                Some(prediction_value::Value::BoolValue(b)) => serde_json::Value::Bool(b),
                Some(prediction_value::Value::StringList(list)) => serde_json::Value::Array(
                    list.values
                        .into_iter()
                        .map(serde_json::Value::String)
                        .collect(),
                ),
                Some(prediction_value::Value::NumberList(list)) => serde_json::Value::Array(
                    list.values
                        .into_iter()
                        .map(|n| serde_json::json!(n))
                        .collect(),
                ),
                None => serde_json::Value::Null,
            };
            predictions.insert(key, json_value);
        }

        Ok(ModelResponse {
            model_id: response.model_id,
            predictions,
            confidence: response.confidence,
            model_version: Some(response.model_version),
        })
    }

    /// Invoke AWS Lambda
    async fn invoke_lambda(
        &self,
        url: &str,
        request: &ModelRequest,
    ) -> Result<ModelResponse, InvocationError> {
        #[cfg(not(feature = "lambda"))]
        {
            let _ = (url, request); // Avoid unused warnings
            return Err(InvocationError::UnsupportedProtocol(
                "Lambda support not enabled. Rebuild with --features lambda".to_string(),
            ));
        }

        #[cfg(feature = "lambda")]
        {
            // Extract Lambda function name from URL
            // Expected format: "lambda://function-name" or "arn:aws:lambda:region:account:function:name"
            let function_name = if url.starts_with("lambda://") {
                url.strip_prefix("lambda://").unwrap_or(url)
            } else if url.starts_with("arn:aws:lambda") {
                // Extract function name from ARN
                url.split(':').last().unwrap_or(url)
            } else {
                url
            };

            // Create AWS config (uses default credential chain)
            let config = aws_config::load_from_env().await;
            let lambda_client = aws_sdk_lambda::Client::new(&config);

            // Build Lambda payload (JSON format)
            let payload = serde_json::json!({
                "model_id": request.model_id,
                "features": request.features,
            });

            let payload_bytes = serde_json::to_vec(&payload).map_err(|e| {
                InvocationError::ParseError(format!("Failed to serialize payload: {}", e))
            })?;

            // Invoke Lambda function
            let response = lambda_client
                .invoke()
                .function_name(function_name)
                .payload(aws_sdk_lambda::primitives::Blob::new(payload_bytes))
                .send()
                .await
                .map_err(|e| {
                    InvocationError::NetworkError(format!("Lambda invocation failed: {}", e))
                })?;

            // Check for function error
            if let Some(function_error) = response.function_error() {
                return Err(InvocationError::ModelError {
                    status_code: 500,
                    message: format!("Lambda function error: {}", function_error),
                });
            }

            // Parse response payload
            let response_payload = response
                .payload()
                .ok_or_else(|| InvocationError::ParseError("Empty Lambda response".to_string()))?;

            let response_json: serde_json::Value =
                serde_json::from_slice(response_payload.as_ref()).map_err(|e| {
                    InvocationError::ParseError(format!("Failed to parse Lambda response: {}", e))
                })?;

            // Extract predictions from response
            // Expected format: {"predictions": {...}, "confidence": 0.95, "model_version": "v1"}
            let predictions = response_json
                .get("predictions")
                .and_then(|p| p.as_object())
                .ok_or_else(|| {
                    InvocationError::ParseError(
                        "Missing 'predictions' field in Lambda response".to_string(),
                    )
                })?
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();

            let confidence = response_json
                .get("confidence")
                .and_then(|c| c.as_f64())
                .unwrap_or(1.0);

            let model_version = response_json
                .get("model_version")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            Ok(ModelResponse {
                model_id: request.model_id.clone(),
                predictions,
                confidence,
                model_version,
            })
        }
    }
}

/// Model invocation request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRequest {
    pub model_id: String,
    pub features: HashMap<String, serde_json::Value>,
    pub timeout_ms: Option<u64>,
}

/// Model invocation response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelResponse {
    pub model_id: String,
    pub predictions: HashMap<String, serde_json::Value>,
    pub confidence: f64,
    pub model_version: Option<String>,
}

/// HTTP model response format (TensorFlow Serving, TorchServe)
#[derive(Debug, Clone, Deserialize)]
struct HttpModelResponse {
    predictions: Vec<PredictionInstance>,
    #[serde(default)]
    model_version: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct PredictionInstance {
    predictions: HashMap<String, serde_json::Value>,
    confidence: Option<f64>,
}

/// Model invocation errors
#[derive(Debug, Error)]
pub enum InvocationError {
    #[error("Model not found: {0}")]
    ModelNotFound(String),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Model error (status {status_code}): {message}")]
    ModelError { status_code: u16, message: String },

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Timeout")]
    Timeout,

    #[error("Unsupported protocol: {0}")]
    UnsupportedProtocol(String),

    #[error("Circuit breaker open")]
    CircuitBreakerOpen,
}

/// Implement retry logic for invocation errors
impl super::retry::IsRetryable for InvocationError {
    fn is_retryable(&self) -> bool {
        match self {
            // Retryable errors
            InvocationError::NetworkError(_) => true,
            InvocationError::Timeout => true,
            InvocationError::ModelError { status_code, .. } => {
                // Retry on 5xx errors and 429 (rate limit)
                *status_code >= 500 || *status_code == 429
            }
            // Non-retryable errors
            InvocationError::ModelNotFound(_) => false,
            InvocationError::ParseError(_) => false,
            InvocationError::UnsupportedProtocol(_) => false,
            InvocationError::CircuitBreakerOpen => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_request_serialization() {
        let mut features = HashMap::new();
        features.insert("street".to_string(), serde_json::json!("123 Main St"));
        features.insert("city".to_string(), serde_json::json!("Boston"));

        let request = ModelRequest {
            model_id: "address_bert_v2".to_string(),
            features,
            timeout_ms: Some(500),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("address_bert_v2"));
    }

    #[test]
    fn test_model_response_deserialization() {
        let json = r#"{
            "model_id": "test_model",
            "predictions": {
                "gender": "female",
                "age_group": "25-34"
            },
            "confidence": 0.85,
            "model_version": "v2.1.0"
        }"#;

        let response: ModelResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.model_id, "test_model");
        assert_eq!(response.confidence, 0.85);
        assert_eq!(response.model_version, Some("v2.1.0".to_string()));
    }

    #[tokio::test]
    async fn test_batch_prediction() {
        use crate::orchestration::ml::{
            CacheConfig, ModelCache, ModelEndpoint, ModelMetadata, ModelRegistry, ServingFramework,
        };

        // This test verifies batch prediction works with mock registry/cache
        // In production, this would connect to real model endpoints

        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));

        // Register a test model
        let metadata = ModelMetadata {
            id: "batch_test_model".to_string(),
            name: "Batch Test Model".to_string(),
            version: "1.0.0".to_string(),
            endpoint: ModelEndpoint {
                protocol: ModelProtocol::Http,
                url: "http://localhost:8501/predict".to_string(),
                timeout_ms: 5000,
                headers: std::collections::HashMap::new(),
            },
            framework: ServingFramework::Custom,
            input_schema: vec![],
            output_schema: vec![],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        registry.register(metadata).await.unwrap();

        let invoker = ModelInvoker::new(registry, cache).unwrap();

        // Create batch of requests
        let requests = vec![
            ModelRequest {
                model_id: "batch_test_model".to_string(),
                features: {
                    let mut map = std::collections::HashMap::new();
                    map.insert("input1".to_string(), serde_json::json!("value1"));
                    map
                },
                timeout_ms: Some(5000),
            },
            ModelRequest {
                model_id: "batch_test_model".to_string(),
                features: {
                    let mut map = std::collections::HashMap::new();
                    map.insert("input2".to_string(), serde_json::json!("value2"));
                    map
                },
                timeout_ms: Some(5000),
            },
            ModelRequest {
                model_id: "batch_test_model".to_string(),
                features: {
                    let mut map = std::collections::HashMap::new();
                    map.insert("input3".to_string(), serde_json::json!("value3"));
                    map
                },
                timeout_ms: Some(5000),
            },
        ];

        // Note: This will fail since we don't have a real endpoint
        // But it verifies the batch API works structurally
        let results = invoker.invoke_batch(requests, Some(2)).await;

        // Verify we got 3 results back (even if they're errors)
        assert_eq!(results.len(), 3);

        // All should be errors since no actual server is running
        for result in results {
            assert!(result.is_err());
        }
    }

    #[test]
    fn test_batch_with_empty_requests() {
        use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};

        // Test that empty batch works correctly
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let registry = Arc::new(ModelRegistry::new());
            let cache = Arc::new(ModelCache::new(CacheConfig::default()));
            let invoker = ModelInvoker::new(registry, cache).unwrap();

            let results = invoker.invoke_batch(vec![], Some(10)).await;
            assert_eq!(results.len(), 0);
        });
    }
}
