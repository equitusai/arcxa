//! # Graphica Model Service
//!
//! Distributed transformer-based embedding service for semantic matching.
//!
//! This service provides:
//! - High-performance gRPC API for embedding generation
//! - Independent scaling from coordinator
//! - Multi-layer caching (in-memory + coordinator-side)
//! - Health checks for Kubernetes
//! - Prometheus metrics

mod cache;
mod model;

use anyhow::Result;
use cache::EmbeddingCache;
use clap::Parser;
use model::ModelInference;
use std::sync::Arc;
use std::time::Instant;
use tonic::{transport::Server, Request, Response, Status};
use tracing::{error, info};

// Include generated proto code
pub mod proto {
    tonic::include_proto!("graphica.model");
}

use proto::{
    model_service_server::{ModelService, ModelServiceServer},
    *,
};

/// Model service implementation
pub struct ModelServiceImpl {
    /// Model inference engine
    inference: Arc<ModelInference>,

    /// In-memory cache
    cache: Arc<EmbeddingCache>,

    /// Service start time
    start_time: Instant,
}

impl ModelServiceImpl {
    pub fn new(inference: ModelInference, cache_size: usize) -> Self {
        Self {
            inference: Arc::new(inference),
            cache: Arc::new(EmbeddingCache::new(cache_size)),
            start_time: Instant::now(),
        }
    }
}

#[tonic::async_trait]
impl ModelService for ModelServiceImpl {
    async fn embed_text(
        &self,
        request: Request<EmbedRequest>,
    ) -> Result<Response<EmbedResponse>, Status> {
        let req = request.into_inner();
        let text = req.text;

        info!("EmbedText request: '{}'", text);

        // Check cache first
        if let Some(embedding) = self.cache.get(&text) {
            info!("  ✓ Served from cache");

            return Ok(Response::new(EmbedResponse {
                embedding: embedding.to_vec(),
                dimension: embedding.len() as i32,
                inference_time_ms: 0.0,
                from_cache: true,
            }));
        }

        // Generate embedding
        match self.inference.embed(&text) {
            Ok((embedding, inference_time_ms)) => {
                // Store in cache
                self.cache.put(&text, &embedding);

                info!("  ✓ Generated in {:.2}ms", inference_time_ms);

                Ok(Response::new(EmbedResponse {
                    embedding: embedding.to_vec(),
                    dimension: embedding.len() as i32,
                    inference_time_ms: inference_time_ms as f32,
                    from_cache: false,
                }))
            }
            Err(e) => {
                error!("Failed to generate embedding: {}", e);
                Err(Status::internal(format!(
                    "Embedding generation failed: {}",
                    e
                )))
            }
        }
    }

    async fn embed_batch(
        &self,
        request: Request<EmbedBatchRequest>,
    ) -> Result<Response<EmbedBatchResponse>, Status> {
        let req = request.into_inner();
        let texts = req.texts;

        info!("EmbedBatch request: {} texts", texts.len());

        let start = Instant::now();
        let mut embeddings = Vec::new();

        for text in &texts {
            // Use get_or_compute for caching
            match self
                .cache
                .get_or_compute(text, || self.inference.embed(text).map(|(emb, _)| emb))
            {
                Ok(embedding) => {
                    embeddings.push(Embedding {
                        values: embedding.to_vec(),
                        dimension: embedding.len() as i32,
                    });
                }
                Err(e) => {
                    error!("Failed to generate embedding for '{}': {}", text, e);
                    return Err(Status::internal(format!("Batch embedding failed: {}", e)));
                }
            }
        }

        let total_time_ms = start.elapsed().as_secs_f64() * 1000.0;

        info!(
            "  ✓ Processed {} texts in {:.2}ms",
            texts.len(),
            total_time_ms
        );

        Ok(Response::new(EmbedBatchResponse {
            embeddings,
            total_inference_time_ms: total_time_ms as f32,
            count: texts.len() as i32,
        }))
    }

    async fn health(
        &self,
        _request: Request<HealthRequest>,
    ) -> Result<Response<HealthResponse>, Status> {
        let cache_stats = self.cache.stats();
        let uptime = self.start_time.elapsed().as_secs();

        Ok(Response::new(HealthResponse {
            healthy: true,
            model_name: self.inference.model_name().to_string(),
            embedding_dimension: self.inference.embedding_dim() as i32,
            total_requests: self.inference.total_inferences() as i64,
            avg_latency_ms: self.inference.avg_latency_ms() as f32,
            cache_hit_rate: (cache_stats.hit_rate * 100.0) as f32,
            uptime_seconds: uptime as i64,
        }))
    }

    async fn get_model_info(
        &self,
        _request: Request<ModelInfoRequest>,
    ) -> Result<Response<ModelInfoResponse>, Status> {
        Ok(Response::new(ModelInfoResponse {
            model_name: "sentence-transformers/all-MiniLM-L6-v2".to_string(),
            version: "2.0".to_string(),
            embedding_dimension: self.inference.embedding_dim() as i32,
            max_sequence_length: self.inference.max_length() as i32,
            model_size_bytes: 90_000_000, // ~90MB
            architecture: "transformer".to_string(),
            languages: vec!["en".to_string()],
            description: "Sentence transformer for semantic similarity".to_string(),
        }))
    }
}

/// Command-line arguments
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Port to listen on
    #[arg(short, long, default_value = "50051")]
    port: u16,

    /// Path to model directory
    #[arg(long)]
    model_path: Option<String>,

    /// Model name
    #[arg(long, default_value = "minilm")]
    model_name: String,

    /// Cache size (number of embeddings)
    #[arg(long, default_value = "10000")]
    cache_size: usize,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    let args = Args::parse();

    // Get model path from args or environment
    let model_dir = args
        .model_path
        .or_else(|| std::env::var("GRAPHICA_MODEL_PATH").ok())
        .expect("Model path must be provided via --model-path or GRAPHICA_MODEL_PATH env var");

    info!("🚀 Starting Graphica Model Service");
    info!("  Port: {}", args.port);
    info!("  Model Path: {}", model_dir);
    info!("  Cache Size: {}", args.cache_size);

    // Initialize model inference
    let model_path = format!("{}/model.onnx", model_dir);
    let tokenizer_path = format!("{}/tokenizer.json", model_dir);

    let inference = ModelInference::new(args.model_name.clone(), &model_path, &tokenizer_path)?;

    info!("✅ Model loaded successfully");

    // Create service
    let service = ModelServiceImpl::new(inference, args.cache_size);

    // Build server address
    let addr = format!("0.0.0.0:{}", args.port).parse()?;

    info!("🎧 Listening on {}", addr);
    info!(
        "📊 Health check: http://{}:{}/health",
        "localhost", args.port
    );
    info!("");
    info!("Ready to serve embedding requests!");

    // Start gRPC server
    Server::builder()
        .add_service(ModelServiceServer::new(service))
        .serve(addr)
        .await?;

    Ok(())
}
