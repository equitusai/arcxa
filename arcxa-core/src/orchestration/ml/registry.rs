//! ML model registry for endpoint management

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::{ModelProtocol, ServingFramework};

/// Model registry for managing model endpoints
pub struct ModelRegistry {
    /// Registered models
    models: Arc<RwLock<HashMap<String, ModelMetadata>>>,
}

impl ModelRegistry {
    /// Create new model registry
    pub fn new() -> Self {
        Self {
            models: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a model
    pub async fn register(&self, metadata: ModelMetadata) -> Result<()> {
        let mut models = self.models.write().await;
        models.insert(metadata.id.clone(), metadata);
        Ok(())
    }

    /// Get model metadata
    pub async fn get_model(&self, model_id: &str) -> Option<ModelMetadata> {
        let models = self.models.read().await;
        models.get(model_id).cloned()
    }

    /// List all registered models
    pub async fn list_models(&self) -> Vec<ModelSummary> {
        let models = self.models.read().await;
        models
            .values()
            .map(|m| ModelSummary {
                id: m.id.clone(),
                name: m.name.clone(),
                version: m.version.clone(),
                protocol: m.endpoint.protocol.clone(),
            })
            .collect()
    }

    /// Unregister a model
    pub async fn unregister(&self, model_id: &str) -> Result<()> {
        let mut models = self.models.write().await;
        models.remove(model_id);
        Ok(())
    }

    /// Update model endpoint
    pub async fn update_endpoint(&self, model_id: &str, endpoint: ModelEndpoint) -> Result<()> {
        let mut models = self.models.write().await;
        if let Some(model) = models.get_mut(model_id) {
            model.endpoint = endpoint;
        }
        Ok(())
    }
}

impl Default for ModelRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Model metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelMetadata {
    pub id: String,
    pub name: String,
    pub version: String,
    pub endpoint: ModelEndpoint,
    pub framework: ServingFramework,
    pub input_schema: Vec<FeatureSchema>,
    pub output_schema: Vec<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Model endpoint configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelEndpoint {
    pub protocol: ModelProtocol,
    pub url: String,
    pub timeout_ms: u64,
    #[serde(default)]
    pub headers: HashMap<String, String>,
}

/// Feature schema definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureSchema {
    pub name: String,
    pub data_type: FeatureDataType,
    pub required: bool,
}

/// Feature data types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FeatureDataType {
    String,
    Integer,
    Float,
    Boolean,
    Array,
    Object,
}

/// Model summary for listings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSummary {
    pub id: String,
    pub name: String,
    pub version: String,
    pub protocol: ModelProtocol,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_model() -> ModelMetadata {
        ModelMetadata {
            id: "test_model_123".to_string(),
            name: "Address Similarity".to_string(),
            version: "2.1.0".to_string(),
            endpoint: ModelEndpoint {
                protocol: ModelProtocol::Http,
                url: "http://localhost:8501/v1/models/address:predict".to_string(),
                timeout_ms: 500,
                headers: HashMap::new(),
            },
            framework: ServingFramework::TensorFlowServing,
            input_schema: vec![
                FeatureSchema {
                    name: "street".to_string(),
                    data_type: FeatureDataType::String,
                    required: true,
                },
                FeatureSchema {
                    name: "city".to_string(),
                    data_type: FeatureDataType::String,
                    required: true,
                },
            ],
            output_schema: vec!["similarity_score".to_string()],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    #[tokio::test]
    async fn test_register_model() {
        let registry = ModelRegistry::new();
        let model = create_test_model();

        registry.register(model.clone()).await.unwrap();

        let retrieved = registry.get_model(&model.id).await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "Address Similarity");
    }

    #[tokio::test]
    async fn test_list_models() {
        let registry = ModelRegistry::new();

        let model1 = create_test_model();
        let mut model2 = create_test_model();
        model2.id = "test_model_456".to_string();
        model2.name = "Gender Predictor".to_string();

        registry.register(model1).await.unwrap();
        registry.register(model2).await.unwrap();

        let models = registry.list_models().await;
        assert_eq!(models.len(), 2);
    }

    #[tokio::test]
    async fn test_unregister_model() {
        let registry = ModelRegistry::new();
        let model = create_test_model();

        registry.register(model.clone()).await.unwrap();
        assert!(registry.get_model(&model.id).await.is_some());

        registry.unregister(&model.id).await.unwrap();
        assert!(registry.get_model(&model.id).await.is_none());
    }

    #[tokio::test]
    async fn test_update_endpoint() {
        let registry = ModelRegistry::new();
        let model = create_test_model();
        let model_id = model.id.clone();

        registry.register(model).await.unwrap();

        let new_endpoint = ModelEndpoint {
            protocol: ModelProtocol::Grpc,
            url: "grpc://localhost:9000".to_string(),
            timeout_ms: 1000,
            headers: HashMap::new(),
        };

        registry
            .update_endpoint(&model_id, new_endpoint)
            .await
            .unwrap();

        let updated = registry.get_model(&model_id).await.unwrap();
        assert_eq!(updated.endpoint.protocol, ModelProtocol::Grpc);
        assert_eq!(updated.endpoint.timeout_ms, 1000);
    }
}
