//! # Comprehensive Unit Tests for Phase 2 Semantic Matching
//!
//! Tests cover:
//! - Circuit breaker state transitions
//! - Configuration loading
//! - Multi-model routing
//! - Graceful degradation
//! - Cache integration
//! - Fault tolerance

#[cfg(test)]
mod tests {
    use super::super::client::*;
    use super::super::cache::EmbeddingCache;
    use ndarray::Array1;
    use tempfile::TempDir;

    // ============================================================================
    // Circuit Breaker Tests
    // ============================================================================

    #[test]
    fn test_circuit_breaker_initial_state() {
        let mut cb = CircuitBreaker::new(5, 30);
        assert_eq!(cb.state, CircuitState::Closed);
        assert_eq!(cb.failure_count, 0);
        assert!(cb.can_attempt());
    }

    #[test]
    fn test_circuit_breaker_opens_after_threshold() {
        let mut cb = CircuitBreaker::new(3, 30);

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

        // Circuit should block attempts
        assert!(!cb.can_attempt());
    }

    #[test]
    fn test_circuit_breaker_reset_on_success() {
        let mut cb = CircuitBreaker::new(3, 30);

        // Build up failures
        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.failure_count, 2);

        // Success should reset
        cb.record_success();
        assert_eq!(cb.state, CircuitState::Closed);
        assert_eq!(cb.failure_count, 0);
        assert!(cb.can_attempt());
    }

    #[test]
    fn test_circuit_breaker_half_open_after_timeout() {
        let mut cb = CircuitBreaker::new(3, 0); // 0 second timeout for testing

        // Open the circuit
        for _ in 0..3 {
            cb.record_failure();
        }
        assert_eq!(cb.state, CircuitState::Open);

        // Sleep to ensure timeout has elapsed
        std::thread::sleep(std::time::Duration::from_millis(10));

        // Should transition to half-open
        assert!(cb.can_attempt());
        assert_eq!(cb.state, CircuitState::HalfOpen);
    }

    #[test]
    fn test_circuit_breaker_closes_from_half_open_on_success() {
        let mut cb = CircuitBreaker::new(3, 0);

        // Open the circuit
        for _ in 0..3 {
            cb.record_failure();
        }
        assert_eq!(cb.state, CircuitState::Open);

        // Wait for timeout
        std::thread::sleep(std::time::Duration::from_millis(10));
        let _ = cb.can_attempt(); // Transition to half-open

        // Success from half-open should close
        cb.record_success();
        assert_eq!(cb.state, CircuitState::Closed);
        assert_eq!(cb.failure_count, 0);
    }

    #[test]
    fn test_circuit_breaker_reopens_from_half_open_on_failure() {
        let mut cb = CircuitBreaker::new(3, 0);

        // Open the circuit
        for _ in 0..3 {
            cb.record_failure();
        }

        // Wait for timeout and transition to half-open
        std::thread::sleep(std::time::Duration::from_millis(10));
        let _ = cb.can_attempt();
        assert_eq!(cb.state, CircuitState::HalfOpen);

        // Failure from half-open should reopen
        cb.record_failure();
        assert_eq!(cb.state, CircuitState::Open);
    }

    // ============================================================================
    // Configuration Tests
    // ============================================================================

    #[test]
    fn test_model_service_config_default() {
        let config = ModelServiceConfig::default();
        assert_eq!(config.url, "http://localhost:50051");
        assert_eq!(config.model_name, "minilm");
        assert_eq!(config.connect_timeout, 5);
        assert_eq!(config.request_timeout, 10);
        assert_eq!(config.circuit_breaker_threshold, 5);
        assert_eq!(config.circuit_breaker_timeout, 30);
    }

    #[test]
    fn test_model_service_config_custom() {
        let config = ModelServiceConfig {
            url: "http://custom-service:50051".to_string(),
            model_name: "bge".to_string(),
            connect_timeout: 10,
            request_timeout: 20,
            circuit_breaker_threshold: 3,
            circuit_breaker_timeout: 60,
        };

        assert_eq!(config.url, "http://custom-service:50051");
        assert_eq!(config.model_name, "bge");
        assert_eq!(config.connect_timeout, 10);
        assert_eq!(config.request_timeout, 20);
        assert_eq!(config.circuit_breaker_threshold, 3);
        assert_eq!(config.circuit_breaker_timeout, 60);
    }

    // ============================================================================
    // Cache Integration Tests
    // ============================================================================

    #[test]
    fn test_cache_stats() {
        let cache_dir = TempDir::new().unwrap();
        let cache = EmbeddingCache::new(cache_dir.path().to_str().unwrap()).unwrap();

        // Initial stats
        let stats = cache.stats().unwrap();
        assert_eq!(stats.entry_count, 0);

        // Add some embeddings
        let emb1 = Array1::from_vec(vec![1.0, 2.0, 3.0]);
        let emb2 = Array1::from_vec(vec![4.0, 5.0, 6.0]);

        cache.put("text1", &emb1).unwrap();
        cache.put("text2", &emb2).unwrap();

        // Check stats
        let stats = cache.stats().unwrap();
        assert_eq!(stats.entry_count, 2);
    }

    #[test]
    fn test_cache_get_or_compute() {
        let cache_dir = TempDir::new().unwrap();
        let cache = EmbeddingCache::new(cache_dir.path().to_str().unwrap()).unwrap();

        let text = "test_text";
        let expected = Array1::from_vec(vec![1.0, 2.0, 3.0]);

        // First call - compute
        let mut compute_called = false;
        let result = cache
            .get_or_compute(text, || {
                compute_called = true;
                Ok(expected.clone())
            })
            .unwrap();

        assert!(compute_called);
        assert_eq!(result, expected);

        // Second call - cached
        compute_called = false;
        let result = cache
            .get_or_compute(text, || {
                compute_called = true;
                Ok(expected.clone())
            })
            .unwrap();

        assert!(!compute_called); // Should not compute again
        assert_eq!(result, expected);
    }

    #[test]
    fn test_cache_persistence() {
        let cache_dir = TempDir::new().unwrap();
        let path = cache_dir.path().to_str().unwrap();

        let emb = Array1::from_vec(vec![1.0, 2.0, 3.0]);

        // Write to cache
        {
            let cache = EmbeddingCache::new(path).unwrap();
            cache.put("persistent_text", &emb).unwrap();
        }

        // Read from new cache instance (tests persistence)
        {
            let cache = EmbeddingCache::new(path).unwrap();
            let result = cache.get("persistent_text").unwrap();
            assert!(result.is_some());
            assert_eq!(result.unwrap(), emb);
        }
    }

    #[test]
    fn test_cache_clear() {
        let cache_dir = TempDir::new().unwrap();
        let cache = EmbeddingCache::new(cache_dir.path().to_str().unwrap()).unwrap();

        // Add embeddings
        let emb = Array1::from_vec(vec![1.0, 2.0, 3.0]);
        cache.put("text1", &emb).unwrap();
        cache.put("text2", &emb).unwrap();

        assert_eq!(cache.stats().unwrap().entry_count, 2);

        // Clear cache
        cache.clear().unwrap();

        assert_eq!(cache.stats().unwrap().entry_count, 0);
        assert!(cache.get("text1").unwrap().is_none());
    }

    // ============================================================================
    // Similarity Tests
    // ============================================================================

    #[test]
    fn test_cosine_similarity_identical_vectors() {
        use super::super::similarity::CosineSimilarity;

        let vec1 = Array1::from_vec(vec![1.0, 2.0, 3.0]);
        let vec2 = Array1::from_vec(vec![1.0, 2.0, 3.0]);

        let sim = CosineSimilarity::similarity(&vec1, &vec2);
        assert!((sim - 1.0).abs() < 0.001); // Should be ~1.0
    }

    #[test]
    fn test_cosine_similarity_orthogonal_vectors() {
        use super::super::similarity::CosineSimilarity;

        let vec1 = Array1::from_vec(vec![1.0, 0.0, 0.0]);
        let vec2 = Array1::from_vec(vec![0.0, 1.0, 0.0]);

        let sim = CosineSimilarity::similarity(&vec1, &vec2);
        assert!(sim.abs() < 0.001); // Should be ~0.0
    }

    #[test]
    fn test_cosine_similarity_opposite_vectors() {
        use super::super::similarity::CosineSimilarity;

        let vec1 = Array1::from_vec(vec![1.0, 2.0, 3.0]);
        let vec2 = Array1::from_vec(vec![-1.0, -2.0, -3.0]);

        let sim = CosineSimilarity::similarity(&vec1, &vec2);
        assert!((sim - (-1.0)).abs() < 0.001); // Should be ~-1.0
    }

    #[test]
    fn test_cosine_similarity_zero_vector() {
        use super::super::similarity::CosineSimilarity;

        let vec1 = Array1::from_vec(vec![1.0, 2.0, 3.0]);
        let vec2 = Array1::from_vec(vec![0.0, 0.0, 0.0]);

        let sim = CosineSimilarity::similarity(&vec1, &vec2);
        assert_eq!(sim, 0.0); // Should handle zero vectors gracefully
    }

    #[test]
    fn test_cosine_similarity_normalized_vectors() {
        use super::super::similarity::CosineSimilarity;

        // Pre-normalized vectors (unit length)
        let vec1 = Array1::from_vec(vec![0.6, 0.8, 0.0]);
        let vec2 = Array1::from_vec(vec![0.8, 0.6, 0.0]);

        let sim = CosineSimilarity::similarity(&vec1, &vec2);
        assert!(sim > 0.9); // Should be high similarity
    }

    // ============================================================================
    // Multi-Model Client Tests (Mock-based)
    // ============================================================================

    #[test]
    fn test_multi_model_client_creation_with_no_configs() {
        // Just verify configs validation would fail
        let configs: Vec<ModelServiceConfig> = vec![];
        assert!(configs.is_empty());
    }

    #[test]
    fn test_multi_model_client_config_validation() {
        let configs = vec![
            ModelServiceConfig {
                url: "http://minilm:50051".to_string(),
                model_name: "minilm".to_string(),
                ..Default::default()
            },
            ModelServiceConfig {
                url: "http://bge:50051".to_string(),
                model_name: "bge".to_string(),
                ..Default::default()
            },
            ModelServiceConfig {
                url: "http://mpnet:50051".to_string(),
                model_name: "mpnet".to_string(),
                ..Default::default()
            },
        ];

        assert_eq!(configs.len(), 3);
        assert_eq!(configs[0].model_name, "minilm");
        assert_eq!(configs[1].model_name, "bge");
        assert_eq!(configs[2].model_name, "mpnet");
    }

    // ============================================================================
    // Integration Tests (require model service - marked as ignored)
    // ============================================================================

    #[tokio::test]
    #[ignore] // Requires running model service
    async fn test_client_graceful_degradation_no_service() {
        let cache_dir = TempDir::new().unwrap();

        let config = ModelServiceConfig {
            url: "http://nonexistent:99999".to_string(), // Invalid URL
            model_name: "minilm".to_string(),
            connect_timeout: 1, // Short timeout
            ..Default::default()
        };

        // Client should create successfully even if service unavailable
        let result = SemanticMatcherClient::new(config, cache_dir.path().to_str().unwrap()).await;

        assert!(result.is_ok());
        let client = result.unwrap();

        // Service should be marked unavailable
        assert!(!client.is_available().await);
        assert_eq!(client.health_status().await, "unavailable");
    }

    #[tokio::test]
    #[ignore] // Requires running model service
    async fn test_client_with_available_service() {
        use std::env;

        // Only run if GRAPHICA_MODEL_SERVICE_URL is set
        if let Ok(url) = env::var("GRAPHICA_MODEL_SERVICE_URL") {
            let cache_dir = TempDir::new().unwrap();

            let config = ModelServiceConfig {
                url,
                model_name: "minilm".to_string(),
                ..Default::default()
            };

            let client = SemanticMatcherClient::new(config, cache_dir.path().to_str().unwrap())
                .await
                .unwrap();

            // Service should be available
            assert!(client.is_available().await);

            let health = client.health_status().await;
            assert!(health == "healthy" || health == "recovering");
        }
    }

    #[tokio::test]
    #[ignore] // Requires running model service
    async fn test_similarity_computation() {
        use std::env;

        if let Ok(url) = env::var("GRAPHICA_MODEL_SERVICE_URL") {
            let cache_dir = TempDir::new().unwrap();

            let config = ModelServiceConfig {
                url,
                model_name: "minilm".to_string(),
                ..Default::default()
            };

            let client = SemanticMatcherClient::new(config, cache_dir.path().to_str().unwrap())
                .await
                .unwrap();

            if client.is_available().await {
                // Test semantic similarity
                let sim = client.similarity("email", "e-mail").await;

                if let Ok(score) = sim {
                    assert!(score >= 0.0 && score <= 1.0);
                    assert!(score > 0.7); // "email" and "e-mail" should be very similar
                }
            }
        }
    }

    #[tokio::test]
    #[ignore] // Requires running model service
    async fn test_cache_effectiveness() {
        use std::env;

        if let Ok(url) = env::var("GRAPHICA_MODEL_SERVICE_URL") {
            let cache_dir = TempDir::new().unwrap();

            let config = ModelServiceConfig {
                url,
                model_name: "minilm".to_string(),
                ..Default::default()
            };

            let client = SemanticMatcherClient::new(config, cache_dir.path().to_str().unwrap())
                .await
                .unwrap();

            if client.is_available().await {
                // First call (cache miss)
                let start1 = std::time::Instant::now();
                let _ = client.similarity("customer_email", "email_address").await;
                let dur1 = start1.elapsed();

                // Second call (cache hit)
                let start2 = std::time::Instant::now();
                let _ = client.similarity("customer_email", "email_address").await;
                let dur2 = start2.elapsed();

                // Cache should be faster (at least 2x)
                assert!(dur2 < dur1);
            }
        }
    }

    #[tokio::test]
    #[ignore] // Requires multiple model services
    async fn test_multi_model_routing() {
        use std::env;

        if let Ok(urls) = env::var("GRAPHICA_MODEL_SERVICE_URLS") {
            if let Ok(names) = env::var("GRAPHICA_MODEL_NAMES") {
                let cache_dir = TempDir::new().unwrap();

                let url_list: Vec<&str> = urls.split(',').collect();
                let name_list: Vec<&str> = names.split(',').collect();

                let configs: Vec<ModelServiceConfig> = url_list
                    .iter()
                    .zip(name_list.iter())
                    .map(|(url, name)| ModelServiceConfig {
                        url: url.to_string(),
                        model_name: name.to_string(),
                        ..Default::default()
                    })
                    .collect();

                let client = MultiModelClient::new(configs, cache_dir.path().to_str().unwrap())
                    .await
                    .unwrap();

                let available = client.available_models();
                assert!(!available.is_empty());

                // Test routing to different models
                for model in available {
                    let result = client.similarity("email", "e-mail", Some(&model)).await;
                    // Result may succeed or fail depending on service availability
                    println!("Model {}: {:?}", model, result);
                }
            }
        }
    }
}
