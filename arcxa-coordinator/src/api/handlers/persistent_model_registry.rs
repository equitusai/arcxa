//! Persistent Model Registry with RDF Storage
//!
//! Wraps graphica-core's ModelRegistry with RDF persistence layer.
//! Models are stored in both in-memory cache and RDF store for durability.

use crate::governance::rdf_store::{GraphicaRdfStore, RdfStore};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use graphica_core::orchestration::ml::{
    registry::FeatureDataType, registry::FeatureSchema, ModelEndpoint, ModelMetadata,
    ModelProtocol, ModelRegistry, ServingFramework,
};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::sync::Arc;

/// Model summary (for list view)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSummary {
    pub id: String,
    pub name: String,
    pub version: String,
    pub endpoint_url: String,
    pub protocol: String,
    pub framework: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Persistent model registry with RDF backing
pub struct PersistentModelRegistry {
    /// In-memory registry (fast access)
    memory_registry: Arc<ModelRegistry>,
    /// RDF store (persistent storage)
    rdf_store: Option<Arc<GraphicaRdfStore>>,
}

impl PersistentModelRegistry {
    /// Create new persistent registry
    pub fn new(rdf_store: Option<Arc<GraphicaRdfStore>>) -> Self {
        Self {
            memory_registry: Arc::new(ModelRegistry::new()),
            rdf_store,
        }
    }

    /// Create new persistent registry and load existing models from RDF
    pub async fn new_and_load(rdf_store: Option<Arc<GraphicaRdfStore>>) -> Result<Self> {
        let registry = Self::new(rdf_store);
        registry.load_from_rdf().await?;
        Ok(registry)
    }

    /// Register a model (persists to RDF + in-memory)
    pub async fn register(&self, metadata: ModelMetadata) -> Result<()> {
        // Store in RDF first
        if let Some(ref rdf_store) = self.rdf_store {
            self.persist_to_rdf(&metadata, rdf_store)
                .await
                .context("Failed to persist model to RDF store")?;
        }

        // Then store in memory
        self.memory_registry
            .register(metadata)
            .await
            .context("Failed to register model in memory")
    }

    /// Get model metadata (from memory)
    pub async fn get_model(&self, model_id: &str) -> Option<ModelMetadata> {
        self.memory_registry.get_model(model_id).await
    }

    /// List all registered models (from memory)
    pub async fn list_models(&self) -> Vec<ModelSummary> {
        // Get core summaries from memory registry
        let core_summaries = self.memory_registry.list_models().await;

        // Enhance with additional fields by fetching full metadata
        let mut enhanced = Vec::new();
        for summary in core_summaries {
            if let Some(model) = self.memory_registry.get_model(&summary.id).await {
                enhanced.push(ModelSummary {
                    id: model.id,
                    name: model.name,
                    version: model.version,
                    endpoint_url: model.endpoint.url,
                    protocol: format!("{:?}", model.endpoint.protocol).to_lowercase(),
                    framework: format!("{:?}", model.framework),
                    created_at: model.created_at,
                    updated_at: model.updated_at,
                });
            }
        }
        enhanced
    }

    /// Unregister a model (removes from both RDF and memory)
    pub async fn unregister(&self, model_id: &str) -> Result<()> {
        // Remove from RDF first
        if let Some(ref rdf_store) = self.rdf_store {
            self.remove_from_rdf(model_id, rdf_store)
                .await
                .context("Failed to remove model from RDF store")?;
        }

        // Then remove from memory
        self.memory_registry
            .unregister(model_id)
            .await
            .context("Failed to unregister model from memory")
    }

    /// Update model endpoint (updates both RDF and memory)
    pub async fn update_endpoint(&self, model_id: &str, endpoint: ModelEndpoint) -> Result<()> {
        // Update in memory first
        self.memory_registry
            .update_endpoint(model_id, endpoint.clone())
            .await
            .context("Failed to update endpoint in memory")?;

        // Then update in RDF
        if let Some(ref rdf_store) = self.rdf_store {
            let model = self
                .memory_registry
                .get_model(model_id)
                .await
                .ok_or_else(|| anyhow::anyhow!("Model not found: {}", model_id))?;

            self.persist_to_rdf(&model, rdf_store)
                .await
                .context("Failed to update model in RDF store")?;
        }

        Ok(())
    }

    /// Load all models from RDF store into memory
    async fn load_from_rdf(&self) -> Result<()> {
        let rdf_store = match &self.rdf_store {
            Some(store) => store,
            None => return Ok(()), // No RDF store, skip loading
        };

        tracing::info!("Loading models from RDF store...");

        let sparql_query = r#"
PREFIX orch: <http://graphica.io/orchestration#>
PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>
PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>

SELECT ?model ?id ?name ?version ?url ?protocol ?framework ?timeout ?created ?updated
WHERE {
    ?model rdf:type orch:ExternalModel ;
           orch:modelId ?id ;
           orch:modelName ?name ;
           orch:modelVersion ?version ;
           orch:endpoint ?url ;
           orch:protocol ?protocol ;
           orch:framework ?framework ;
           orch:timeoutMs ?timeout ;
           orch:createdAt ?created ;
           orch:updatedAt ?updated .
}
"#;

        let results = rdf_store
            .query(sparql_query)
            .context("Failed to query models from RDF store")?;

        let mut loaded_count = 0;
        for row in results {
            match self.parse_model_from_sparql_row(&row) {
                Ok(metadata) => {
                    self.memory_registry.register(metadata).await?;
                    loaded_count += 1;
                }
                Err(e) => {
                    tracing::warn!("Failed to parse model from RDF: {}", e);
                }
            }
        }

        tracing::info!("Loaded {} models from RDF store", loaded_count);
        Ok(())
    }

    /// Persist model metadata to RDF store
    async fn persist_to_rdf(
        &self,
        metadata: &ModelMetadata,
        rdf_store: &GraphicaRdfStore,
    ) -> Result<()> {
        let model_uri = format!("http://graphica.io/orchestration#model/{}", metadata.id);

        // Build feature schema JSON
        let input_schema_json = serde_json::to_string(&metadata.input_schema)
            .context("Failed to serialize input schema")?;
        let output_schema_json = serde_json::to_string(&metadata.output_schema)
            .context("Failed to serialize output schema")?;
        let headers_json = serde_json::to_string(&metadata.endpoint.headers)
            .context("Failed to serialize headers")?;

        // Escape JSON for SPARQL
        let input_schema_escaped = input_schema_json.replace('\\', "\\\\").replace('"', "\\\"");
        let output_schema_escaped = output_schema_json
            .replace('\\', "\\\\")
            .replace('"', "\\\"");
        let headers_escaped = headers_json.replace('\\', "\\\\").replace('"', "\\\"");

        let sparql_insert = format!(
            r#"
PREFIX orch: <http://graphica.io/orchestration#>
PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>
PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>

DELETE {{
    <{model_uri}> ?p ?o .
}}
WHERE {{
    <{model_uri}> ?p ?o .
}} ;

INSERT DATA {{
    <{model_uri}> rdf:type orch:ExternalModel ;
                  orch:modelId "{model_id}" ;
                  orch:modelName "{model_name}" ;
                  orch:modelVersion "{model_version}" ;
                  orch:endpoint "{endpoint_url}" ;
                  orch:protocol "{protocol}" ;
                  orch:framework "{framework}" ;
                  orch:timeoutMs "{timeout}"^^xsd:integer ;
                  orch:inputSchema "{input_schema}" ;
                  orch:outputSchema "{output_schema}" ;
                  orch:headers "{headers}" ;
                  orch:createdAt "{created_at}"^^xsd:dateTime ;
                  orch:updatedAt "{updated_at}"^^xsd:dateTime .
}}
"#,
            model_uri = model_uri,
            model_id = metadata.id,
            model_name = metadata.name,
            model_version = metadata.version,
            endpoint_url = metadata.endpoint.url,
            protocol = format!("{:?}", metadata.endpoint.protocol).to_lowercase(),
            framework = format!("{:?}", metadata.framework),
            timeout = metadata.endpoint.timeout_ms,
            input_schema = input_schema_escaped,
            output_schema = output_schema_escaped,
            headers = headers_escaped,
            created_at = metadata.created_at.to_rfc3339(),
            updated_at = metadata.updated_at.to_rfc3339(),
        );

        rdf_store
            .update(&sparql_insert)
            .context("Failed to execute SPARQL INSERT for model")?;

        tracing::debug!("Persisted model to RDF: {}", metadata.id);
        Ok(())
    }

    /// Remove model from RDF store
    async fn remove_from_rdf(&self, model_id: &str, rdf_store: &GraphicaRdfStore) -> Result<()> {
        let model_uri = format!("http://graphica.io/orchestration#model/{}", model_id);

        let sparql_delete = format!(
            r#"
PREFIX orch: <http://graphica.io/orchestration#>

DELETE {{
    <{model_uri}> ?p ?o .
}}
WHERE {{
    <{model_uri}> ?p ?o .
}}
"#,
            model_uri = model_uri
        );

        rdf_store
            .update(&sparql_delete)
            .context("Failed to execute SPARQL DELETE for model")?;

        tracing::debug!("Removed model from RDF: {}", model_id);
        Ok(())
    }

    /// Parse model metadata from SPARQL query row
    fn parse_model_from_sparql_row(&self, row: &JsonValue) -> Result<ModelMetadata> {
        let id = row["id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'id' in SPARQL result"))?
            .trim_matches('"')
            .to_string();

        let name = row["name"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'name' in SPARQL result"))?
            .trim_matches('"')
            .to_string();

        let version = row["version"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'version' in SPARQL result"))?
            .trim_matches('"')
            .to_string();

        let url = row["url"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'url' in SPARQL result"))?
            .trim_matches('"')
            .to_string();

        let protocol_str = row["protocol"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'protocol' in SPARQL result"))?
            .trim_matches('"');

        let protocol = match protocol_str {
            "http" => ModelProtocol::Http,
            "grpc" => ModelProtocol::Grpc,
            "lambda" => ModelProtocol::Lambda,
            _ => anyhow::bail!("Unknown protocol: {}", protocol_str),
        };

        let framework_str = row["framework"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'framework' in SPARQL result"))?
            .trim_matches('"');

        let framework = match framework_str {
            "TensorFlowServing" => ServingFramework::TensorFlowServing,
            "TorchServe" => ServingFramework::TorchServe,
            "SageMaker" => ServingFramework::SageMaker,
            "Custom" => ServingFramework::Custom,
            _ => ServingFramework::Custom,
        };

        let timeout_ms = row["timeout"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'timeout' in SPARQL result"))?
            .trim_matches('"')
            .parse::<u64>()
            .context("Failed to parse timeout")?;

        let created_at = row["created"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'created' in SPARQL result"))?
            .trim_matches('"');
        let created_at = chrono::DateTime::parse_from_rfc3339(created_at)
            .context("Failed to parse created_at")?
            .with_timezone(&chrono::Utc);

        let updated_at = row["updated"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'updated' in SPARQL result"))?
            .trim_matches('"');
        let updated_at = chrono::DateTime::parse_from_rfc3339(updated_at)
            .context("Failed to parse updated_at")?
            .with_timezone(&chrono::Utc);

        // Parse input/output schemas from JSON (simplified for now)
        let input_schema = vec![]; // TODO: Parse from JSON
        let output_schema = vec![]; // TODO: Parse from JSON
        let headers = std::collections::HashMap::new(); // TODO: Parse from JSON

        Ok(ModelMetadata {
            id,
            name,
            version,
            endpoint: ModelEndpoint {
                protocol,
                url,
                timeout_ms,
                headers,
            },
            framework,
            input_schema,
            output_schema,
            created_at,
            updated_at,
        })
    }

    /// Register a new version of an existing model
    ///
    /// This creates a new version entry in RDF and updates the active version pointer.
    /// The old version remains in RDF for history/rollback purposes.
    pub async fn register_version(&self, metadata: ModelMetadata) -> Result<()> {
        let rdf_store = match &self.rdf_store {
            Some(store) => store,
            None => {
                // Without RDF, just update in-memory (no versioning)
                return self.register(metadata).await;
            }
        };

        // Generate version-specific URI
        let version_uri = format!(
            "http://graphica.io/orchestration#model/{}/version/{}",
            metadata.id, metadata.version
        );

        // Persist this version to RDF
        self.persist_version_to_rdf(&metadata, &version_uri, rdf_store)
            .await?;

        // Update active version pointer
        self.set_active_version(&metadata.id, &metadata.version, rdf_store)
            .await?;

        // Update in-memory registry
        self.memory_registry
            .register(metadata)
            .await
            .context("Failed to register model version in memory")
    }

    /// Get all versions of a model from RDF
    pub async fn list_model_versions(&self, model_id: &str) -> Result<Vec<String>> {
        let rdf_store = match &self.rdf_store {
            Some(store) => store,
            None => return Ok(vec![]),
        };

        let sparql_query = format!(
            r#"
PREFIX orch: <http://graphica.io/orchestration#>

SELECT DISTINCT ?version
WHERE {{
    ?versionUri orch:modelId "{model_id}" ;
                orch:modelVersion ?version .
}}
ORDER BY DESC(?version)
"#,
            model_id = model_id
        );

        let results = rdf_store
            .query(&sparql_query)
            .context("Failed to query model versions")?;

        let versions: Vec<String> = results
            .iter()
            .filter_map(|row| {
                row["version"]
                    .as_str()
                    .map(|s| s.trim_matches('"').to_string())
            })
            .collect();

        Ok(versions)
    }

    /// Get the active version of a model
    pub async fn get_active_version(&self, model_id: &str) -> Result<Option<String>> {
        let rdf_store = match &self.rdf_store {
            Some(store) => store,
            None => return Ok(None),
        };

        let sparql_query = format!(
            r#"
PREFIX orch: <http://graphica.io/orchestration#>

SELECT ?version
WHERE {{
    <http://graphica.io/orchestration#model/{}> orch:activeVersion ?version .
}}
"#,
            model_id
        );

        let results = rdf_store
            .query(&sparql_query)
            .context("Failed to query active version")?;

        Ok(results.first().and_then(|row| {
            row["version"]
                .as_str()
                .map(|s| s.trim_matches('"').to_string())
        }))
    }

    /// Activate a specific version of a model
    pub async fn activate_version(&self, model_id: &str, version: &str) -> Result<()> {
        let rdf_store = match &self.rdf_store {
            Some(store) => store,
            None => {
                anyhow::bail!("Cannot activate version without RDF store");
            }
        };

        // Load the specific version metadata
        let version_metadata = self
            .load_version_from_rdf(model_id, version, rdf_store)
            .await?;

        // Update active version pointer
        self.set_active_version(model_id, version, rdf_store)
            .await?;

        // Update in-memory registry
        self.memory_registry
            .register(version_metadata)
            .await
            .context("Failed to activate version in memory")
    }

    /// Set active version pointer in RDF
    async fn set_active_version(
        &self,
        model_id: &str,
        version: &str,
        rdf_store: &GraphicaRdfStore,
    ) -> Result<()> {
        let model_uri = format!("http://graphica.io/orchestration#model/{}", model_id);

        let sparql_update = format!(
            r#"
PREFIX orch: <http://graphica.io/orchestration#>

DELETE {{
    <{model_uri}> orch:activeVersion ?oldVersion .
}}
WHERE {{
    <{model_uri}> orch:activeVersion ?oldVersion .
}} ;

INSERT DATA {{
    <{model_uri}> orch:activeVersion "{version}" .
}}
"#,
            model_uri = model_uri,
            version = version
        );

        rdf_store
            .update(&sparql_update)
            .context("Failed to update active version")?;

        tracing::info!("Set active version for model {} to {}", model_id, version);
        Ok(())
    }

    /// Persist a specific version to RDF
    async fn persist_version_to_rdf(
        &self,
        metadata: &ModelMetadata,
        version_uri: &str,
        rdf_store: &GraphicaRdfStore,
    ) -> Result<()> {
        // Build schema JSON (same as persist_to_rdf)
        let input_schema_json = serde_json::to_string(&metadata.input_schema)
            .context("Failed to serialize input schema")?;
        let output_schema_json = serde_json::to_string(&metadata.output_schema)
            .context("Failed to serialize output schema")?;
        let headers_json = serde_json::to_string(&metadata.endpoint.headers)
            .context("Failed to serialize headers")?;

        let input_schema_escaped = input_schema_json.replace('\\', "\\\\").replace('"', "\\\"");
        let output_schema_escaped = output_schema_json
            .replace('\\', "\\\\")
            .replace('"', "\\\"");
        let headers_escaped = headers_json.replace('\\', "\\\\").replace('"', "\\\"");

        let sparql_insert = format!(
            r#"
PREFIX orch: <http://graphica.io/orchestration#>
PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>
PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>

DELETE {{
    <{version_uri}> ?p ?o .
}}
WHERE {{
    <{version_uri}> ?p ?o .
}} ;

INSERT DATA {{
    <{version_uri}> rdf:type orch:ModelVersion ;
                    orch:modelId "{model_id}" ;
                    orch:modelName "{model_name}" ;
                    orch:modelVersion "{model_version}" ;
                    orch:endpoint "{endpoint_url}" ;
                    orch:protocol "{protocol}" ;
                    orch:framework "{framework}" ;
                    orch:timeoutMs "{timeout}"^^xsd:integer ;
                    orch:inputSchema "{input_schema}" ;
                    orch:outputSchema "{output_schema}" ;
                    orch:headers "{headers}" ;
                    orch:createdAt "{created_at}"^^xsd:dateTime ;
                    orch:updatedAt "{updated_at}"^^xsd:dateTime .
}}
"#,
            version_uri = version_uri,
            model_id = metadata.id,
            model_name = metadata.name,
            model_version = metadata.version,
            endpoint_url = metadata.endpoint.url,
            protocol = format!("{:?}", metadata.endpoint.protocol).to_lowercase(),
            framework = format!("{:?}", metadata.framework),
            timeout = metadata.endpoint.timeout_ms,
            input_schema = input_schema_escaped,
            output_schema = output_schema_escaped,
            headers = headers_escaped,
            created_at = metadata.created_at.to_rfc3339(),
            updated_at = metadata.updated_at.to_rfc3339(),
        );

        rdf_store
            .update(&sparql_insert)
            .context("Failed to persist model version to RDF")?;

        tracing::debug!("Persisted model version {} to RDF", metadata.version);
        Ok(())
    }

    /// Load a specific version from RDF
    async fn load_version_from_rdf(
        &self,
        model_id: &str,
        version: &str,
        rdf_store: &GraphicaRdfStore,
    ) -> Result<ModelMetadata> {
        let version_uri = format!(
            "http://graphica.io/orchestration#model/{}/version/{}",
            model_id, version
        );

        let sparql_query = format!(
            r#"
PREFIX orch: <http://graphica.io/orchestration#>
PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>

SELECT ?id ?name ?version ?url ?protocol ?framework ?timeout ?created ?updated
WHERE {{
    <{version_uri}> orch:modelId ?id ;
                    orch:modelName ?name ;
                    orch:modelVersion ?version ;
                    orch:endpoint ?url ;
                    orch:protocol ?protocol ;
                    orch:framework ?framework ;
                    orch:timeoutMs ?timeout ;
                    orch:createdAt ?created ;
                    orch:updatedAt ?updated .
}}
"#,
            version_uri = version_uri
        );

        let results = rdf_store
            .query(&sparql_query)
            .context("Failed to query model version")?;

        let row = results
            .first()
            .ok_or_else(|| anyhow::anyhow!("Model version not found: {} v{}", model_id, version))?;

        self.parse_model_from_sparql_row(row)
    }

    /// Get underlying ModelRegistry (for compatibility)
    pub fn as_core_registry(&self) -> Arc<ModelRegistry> {
        self.memory_registry.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_persistent_registry_without_rdf() {
        // Create registry without RDF store (in-memory only)
        let registry = PersistentModelRegistry::new(None);

        let metadata = ModelMetadata {
            id: "test_model".to_string(),
            name: "Test Model".to_string(),
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

        // Should work without RDF store
        registry.register(metadata.clone()).await.unwrap();

        let retrieved = registry.get_model("test_model").await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "Test Model");
    }

    #[tokio::test]
    async fn test_list_models() {
        let registry = PersistentModelRegistry::new(None);

        let models = registry.list_models().await;
        assert_eq!(models.len(), 0);
    }

    #[tokio::test]
    async fn test_model_versioning_in_memory() {
        let registry = PersistentModelRegistry::new(None);

        // Register version 1.0.0
        let metadata_v1 = ModelMetadata {
            id: "versioned_model".to_string(),
            name: "Versioned Model".to_string(),
            version: "1.0.0".to_string(),
            endpoint: ModelEndpoint {
                protocol: ModelProtocol::Http,
                url: "http://localhost:8501/v1/predict".to_string(),
                timeout_ms: 5000,
                headers: std::collections::HashMap::new(),
            },
            framework: ServingFramework::TensorFlowServing,
            input_schema: vec![],
            output_schema: vec![],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        registry.register_version(metadata_v1).await.unwrap();

        // Verify v1.0.0 is active
        let retrieved = registry.get_model("versioned_model").await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().version, "1.0.0");

        // Register version 2.0.0
        let metadata_v2 = ModelMetadata {
            id: "versioned_model".to_string(),
            name: "Versioned Model".to_string(),
            version: "2.0.0".to_string(),
            endpoint: ModelEndpoint {
                protocol: ModelProtocol::Grpc,
                url: "http://localhost:9000".to_string(),
                timeout_ms: 3000,
                headers: std::collections::HashMap::new(),
            },
            framework: ServingFramework::TorchServe,
            input_schema: vec![],
            output_schema: vec![],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        registry.register_version(metadata_v2).await.unwrap();

        // Verify v2.0.0 is now active
        let retrieved = registry.get_model("versioned_model").await;
        assert!(retrieved.is_some());
        let model = retrieved.unwrap();
        assert_eq!(model.version, "2.0.0");
        assert_eq!(model.endpoint.protocol, ModelProtocol::Grpc);
    }

    #[tokio::test]
    async fn test_version_activation_in_memory() {
        let registry = PersistentModelRegistry::new(None);

        // Register two versions
        let metadata_v1 = ModelMetadata {
            id: "rollback_model".to_string(),
            name: "Rollback Model".to_string(),
            version: "1.0.0".to_string(),
            endpoint: ModelEndpoint {
                protocol: ModelProtocol::Http,
                url: "http://localhost:8501/v1".to_string(),
                timeout_ms: 5000,
                headers: std::collections::HashMap::new(),
            },
            framework: ServingFramework::Custom,
            input_schema: vec![],
            output_schema: vec![],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let metadata_v2 = ModelMetadata {
            id: "rollback_model".to_string(),
            name: "Rollback Model".to_string(),
            version: "2.0.0".to_string(),
            endpoint: ModelEndpoint {
                protocol: ModelProtocol::Http,
                url: "http://localhost:8501/v2".to_string(),
                timeout_ms: 3000,
                headers: std::collections::HashMap::new(),
            },
            framework: ServingFramework::Custom,
            input_schema: vec![],
            output_schema: vec![],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        registry.register_version(metadata_v1).await.unwrap();
        registry.register_version(metadata_v2).await.unwrap();

        // Verify v2.0.0 is active
        let active = registry.get_model("rollback_model").await;
        assert!(active.is_some());
        assert_eq!(active.unwrap().version, "2.0.0");

        // Attempting to activate version without RDF store should fail
        let result = registry.activate_version("rollback_model", "1.0.0").await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Cannot activate version without RDF store"));
    }

    #[tokio::test]
    async fn test_get_active_version_without_rdf() {
        let registry = PersistentModelRegistry::new(None);

        // Without RDF store, get_active_version returns None
        let version = registry.get_active_version("nonexistent").await;
        assert!(version.is_ok());
        assert_eq!(version.unwrap(), None);
    }

    #[tokio::test]
    async fn test_list_versions_without_rdf() {
        let registry = PersistentModelRegistry::new(None);

        // Without RDF store, list_model_versions returns empty
        let versions = registry.list_model_versions("nonexistent").await;
        assert!(versions.is_ok());
        assert_eq!(versions.unwrap().len(), 0);
    }
}
