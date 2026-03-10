//! RDF Storage Client
//!
//! Client for storing RDF triples in the governance brain (shard).
//! Provides a high-level interface for schema inference to persist RDF triples.

use anyhow::{Context, Result};
use graphica_core::catalog::schema_to_rdf::{RdfNode, RdfTriple};
use graphica_core::distributed::proto::shard_service::{
    shard_service_client::ShardServiceClient, InsertBatchRequest, Triple as ProtoTriple,
};
use once_cell::sync::Lazy;
use prometheus::{register_histogram, register_int_counter, Histogram, IntCounter};
use tonic::transport::Channel;
use tracing::{error, info, warn};

// Prometheus metrics
static RDF_TRIPLES_STORED_TOTAL: Lazy<IntCounter> = Lazy::new(|| {
    register_int_counter!(
        "rdf_triples_stored_total",
        "Total number of RDF triples stored in governance brain"
    )
    .unwrap()
});

static RDF_TRIPLES_FAILED_TOTAL: Lazy<IntCounter> = Lazy::new(|| {
    register_int_counter!(
        "rdf_triples_failed_total",
        "Total number of RDF triples that failed to store"
    )
    .unwrap()
});

static RDF_STORAGE_DURATION_SECONDS: Lazy<Histogram> = Lazy::new(|| {
    register_histogram!(
        "rdf_storage_duration_seconds",
        "Duration of RDF storage operations in seconds"
    )
    .unwrap()
});

static RDF_STORAGE_BATCH_COUNT: Lazy<IntCounter> = Lazy::new(|| {
    register_int_counter!(
        "rdf_storage_batch_count_total",
        "Total number of batches sent to RDF storage"
    )
    .unwrap()
});

/// RDF storage client for persisting triples to governance brain
pub struct RdfStorageClient {
    /// gRPC client to shard service
    shard_client: Option<ShardServiceClient<Channel>>,

    /// Shard endpoint (e.g., "http://localhost:9090")
    shard_endpoint: String,

    /// Default named graph for schema inferences
    default_graph: String,

    /// Maximum batch size for chunked uploads (default: 1000)
    max_batch_size: usize,
}

/// Statistics from batch storage operation
#[derive(Debug, Clone)]
pub struct StorageStats {
    /// Total triples submitted
    pub total_triples: u64,

    /// Successfully inserted triples
    pub inserted_count: u64,

    /// Failed triples
    pub failed_count: u64,

    /// Number of batches sent
    pub batch_count: usize,

    /// Total duration in milliseconds
    pub total_duration_ms: u64,

    /// Average batch latency
    pub avg_batch_latency_ms: f64,
}

impl RdfStorageClient {
    /// Create a new RDF storage client
    pub fn new(shard_endpoint: impl Into<String>) -> Self {
        Self {
            shard_client: None,
            shard_endpoint: shard_endpoint.into(),
            default_graph: "http://graphica.io/catalog/inferred".to_string(),
            max_batch_size: 1000, // Default: 1000 triples per batch
        }
    }

    /// Set the default named graph
    pub fn with_default_graph(mut self, graph: impl Into<String>) -> Self {
        self.default_graph = graph.into();
        self
    }

    /// Set the maximum batch size for chunked uploads
    pub fn with_max_batch_size(mut self, size: usize) -> Self {
        self.max_batch_size = size.max(1); // Ensure at least 1
        self
    }

    /// Connect to the shard (lazy initialization)
    async fn ensure_connected(&mut self) -> Result<()> {
        if self.shard_client.is_none() {
            info!("Connecting to RDF shard at {}", self.shard_endpoint);

            let channel = Channel::from_shared(self.shard_endpoint.clone())
                .context("Invalid shard endpoint")?
                .connect()
                .await
                .context("Failed to connect to shard")?;

            self.shard_client = Some(ShardServiceClient::new(channel));

            info!("Successfully connected to RDF shard");
        }

        Ok(())
    }

    /// Store RDF triples from schema inference
    /// Returns count of successfully inserted triples (for backward compatibility)
    pub async fn store_schema_triples(
        &mut self,
        source_id: &str,
        triples: Vec<RdfTriple>,
    ) -> Result<u64> {
        let stats = self
            .store_schema_triples_with_stats(source_id, triples)
            .await?;
        Ok(stats.inserted_count)
    }

    /// Store RDF triples with detailed statistics
    pub async fn store_schema_triples_with_stats(
        &mut self,
        source_id: &str,
        triples: Vec<RdfTriple>,
    ) -> Result<StorageStats> {
        // Start timer for duration metric
        let _timer = RDF_STORAGE_DURATION_SECONDS.start_timer();

        self.ensure_connected().await?;

        if triples.is_empty() {
            warn!("No triples to store for source {}", source_id);
            return Ok(StorageStats {
                total_triples: 0,
                inserted_count: 0,
                failed_count: 0,
                batch_count: 0,
                total_duration_ms: 0,
                avg_batch_latency_ms: 0.0,
            });
        }

        let total_triples = triples.len() as u64;

        info!(
            "Storing {} RDF triples for source {} in graph {} (batch size: {})",
            total_triples, source_id, self.default_graph, self.max_batch_size
        );

        // Convert all triples to proto format
        let proto_triples: Vec<ProtoTriple> =
            triples.iter().map(|t| self.convert_to_proto(t)).collect();

        // Split into chunks if needed
        let chunks: Vec<&[ProtoTriple]> = proto_triples.chunks(self.max_batch_size).collect();

        let batch_count = chunks.len();

        if batch_count > 1 {
            info!(
                "Splitting {} triples into {} batches of max {} triples",
                total_triples, batch_count, self.max_batch_size
            );
        }

        // Track statistics
        let mut total_inserted = 0u64;
        let mut total_failed = 0u64;
        let mut total_duration_ms = 0u64;

        // Process each chunk
        for (idx, chunk) in chunks.iter().enumerate() {
            let request = InsertBatchRequest {
                triples: chunk.to_vec(),
                transactional: false, // Best-effort for schema inference
                default_graph: self.default_graph.clone(),
                request_id: format!(
                    "schema-inference-{}-{}-batch-{}",
                    source_id,
                    chrono::Utc::now().timestamp(),
                    idx
                ),
            };

            let client = self
                .shard_client
                .as_mut()
                .context("Shard client not initialized")?;

            let response = client
                .insert_batch(request)
                .await
                .context(format!("Failed to insert batch {} into shard", idx))?
                .into_inner();

            total_inserted += response.inserted_count;
            total_failed += response.failed_count;
            total_duration_ms += response.duration_ms;

            if response.failed_count > 0 {
                warn!(
                    "Batch {}/{}: Failed to insert {} out of {} triples",
                    idx + 1,
                    batch_count,
                    response.failed_count,
                    chunk.len()
                );
            } else {
                info!(
                    "Batch {}/{}: Inserted {} triples in {}ms",
                    idx + 1,
                    batch_count,
                    response.inserted_count,
                    response.duration_ms
                );
            }
        }

        let avg_latency = if batch_count > 0 {
            total_duration_ms as f64 / batch_count as f64
        } else {
            0.0
        };

        let stats = StorageStats {
            total_triples,
            inserted_count: total_inserted,
            failed_count: total_failed,
            batch_count,
            total_duration_ms,
            avg_batch_latency_ms: avg_latency,
        };

        // Record metrics
        RDF_TRIPLES_STORED_TOTAL.inc_by(total_inserted);
        RDF_TRIPLES_FAILED_TOTAL.inc_by(total_failed);
        RDF_STORAGE_BATCH_COUNT.inc_by(batch_count as u64);

        if total_failed > 0 {
            warn!(
                "RDF storage completed with warnings for source {}: {} inserted, {} failed, {} batches, {}ms total",
                source_id,
                total_inserted,
                total_failed,
                batch_count,
                total_duration_ms
            );
        } else {
            info!(
                "RDF storage successful for source {}: {} triples in {} batches, {}ms total (avg {:.1}ms/batch)",
                source_id,
                total_inserted,
                batch_count,
                total_duration_ms,
                avg_latency
            );
        }

        Ok(stats)
    }

    /// Convert RdfTriple to proto Triple
    /// Public for testing
    pub fn convert_to_proto(&self, triple: &RdfTriple) -> ProtoTriple {
        let (object_value, object_datatype, object_language) = match &triple.object {
            RdfNode::Uri(uri) => (uri.clone(), String::new(), String::new()),
            RdfNode::Literal(lit) => (lit.clone(), String::new(), String::new()),
            RdfNode::TypedLiteral(value, datatype) => {
                (value.clone(), datatype.clone(), String::new())
            }
            RdfNode::LangLiteral(value, lang) => (value.clone(), String::new(), lang.clone()),
        };

        ProtoTriple {
            subject: triple.subject.clone(),
            predicate: triple.predicate.clone(),
            object: object_value,
            object_datatype,
            object_language,
            graph: self.default_graph.clone(), // Use default graph
        }
    }

    /// Check if shard is available (health check)
    pub async fn is_available(&mut self) -> bool {
        if let Err(e) = self.ensure_connected().await {
            error!("Failed to connect to shard: {}", e);
            return false;
        }

        // Try a simple health check if the client supports it
        // For now, just return true if we could connect
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphica_core::catalog::schema_to_rdf::RdfNode;

    #[test]
    fn test_triple_conversion() {
        let client = RdfStorageClient::new("http://localhost:9090");

        // Test URI object
        let triple_uri = RdfTriple {
            subject: "http://example.com/subject".to_string(),
            predicate: "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_string(),
            object: RdfNode::Uri("http://example.com/Type".to_string()),
        };

        let proto = client.convert_to_proto(&triple_uri);
        assert_eq!(proto.subject, "http://example.com/subject");
        assert_eq!(
            proto.predicate,
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#type"
        );
        assert_eq!(proto.object, "http://example.com/Type");
        assert_eq!(proto.object_datatype, "");

        // Test typed literal
        let triple_literal = RdfTriple {
            subject: "http://example.com/subject".to_string(),
            predicate: "http://example.com/property".to_string(),
            object: RdfNode::TypedLiteral(
                "42".to_string(),
                "http://www.w3.org/2001/XMLSchema#integer".to_string(),
            ),
        };

        let proto = client.convert_to_proto(&triple_literal);
        assert_eq!(proto.object, "42");
        assert_eq!(
            proto.object_datatype,
            "http://www.w3.org/2001/XMLSchema#integer"
        );

        // Test language literal
        let triple_lang = RdfTriple {
            subject: "http://example.com/subject".to_string(),
            predicate: "http://www.w3.org/2000/01/rdf-schema#label".to_string(),
            object: RdfNode::LangLiteral("Hello".to_string(), "en".to_string()),
        };

        let proto = client.convert_to_proto(&triple_lang);
        assert_eq!(proto.object, "Hello");
        assert_eq!(proto.object_language, "en");
    }

    #[test]
    fn test_client_creation() {
        let client = RdfStorageClient::new("http://localhost:9090");
        assert_eq!(client.shard_endpoint, "http://localhost:9090");
        assert_eq!(client.default_graph, "http://graphica.io/catalog/inferred");
        assert_eq!(client.max_batch_size, 1000);

        let custom_client = RdfStorageClient::new("http://shard:9090")
            .with_default_graph("http://my.graph/custom")
            .with_max_batch_size(500);
        assert_eq!(custom_client.default_graph, "http://my.graph/custom");
        assert_eq!(custom_client.max_batch_size, 500);
    }

    #[test]
    fn test_batch_size_configuration() {
        let client = RdfStorageClient::new("http://localhost:9090").with_max_batch_size(100);
        assert_eq!(client.max_batch_size, 100);

        // Test minimum enforcement
        let min_client = RdfStorageClient::new("http://localhost:9090").with_max_batch_size(0);
        assert_eq!(min_client.max_batch_size, 1); // Should be clamped to 1
    }
}
