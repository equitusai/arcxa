//! # Model Inference Module
//!
//! ONNX-based transformer inference for semantic embeddings.
//!
//! This module is isolated from the coordinator to enable:
//! - Independent scaling of inference workload
//! - GPU acceleration without affecting coordinator
//! - Model updates without coordinator restart
//! - Multi-model support with routing

use anyhow::{Context, Result};
use ndarray::{Array1, Array2};
use ort::{session::Session, value::Tensor};
use std::sync::{Arc, Mutex};
use tokenizers::Tokenizer;
use tracing::{debug, info};

/// Model inference engine
pub struct ModelInference {
    /// Model name (e.g., "minilm", "bge", "mpnet")
    model_name: String,

    /// ONNX session for transformer model (wrapped in Mutex for thread safety)
    session: Arc<Mutex<Session>>,

    /// HuggingFace tokenizer
    tokenizer: Arc<Tokenizer>,

    /// Embedding dimension (384 for MiniLM)
    embedding_dim: usize,

    /// Maximum sequence length
    max_length: usize,

    /// Total inference count (for metrics)
    total_inferences: Arc<Mutex<u64>>,

    /// Total inference time (for avg latency)
    total_inference_time_ms: Arc<Mutex<f64>>,
}

impl ModelInference {
    /// Create a new model inference engine
    pub fn new(model_name: String, model_path: &str, tokenizer_path: &str) -> Result<Self> {
        info!("🧠 Initializing model inference engine...");
        info!("  Model: {}", model_name);
        info!("  ONNX: {}", model_path);
        info!("  Tokenizer: {}", tokenizer_path);

        // Load tokenizer
        let tokenizer = Tokenizer::from_file(tokenizer_path)
            .map_err(|e| anyhow::anyhow!("Failed to load tokenizer: {}", e))?;
        info!("  ✓ Tokenizer loaded");

        // Initialize ONNX Runtime environment (v2.0 API)
        let _ = ort::init().with_name("graphica-model-service").commit();
        // Note: In v2.0, init() returns a unit type, not a Result

        // Load ONNX model (v2.0 API)
        let session = Session::builder()
            .map_err(|e| anyhow::anyhow!("Failed to create session builder: {:?}", e))?
            .with_intra_threads(4)
            .map_err(|e| anyhow::anyhow!("Failed to set intra threads: {:?}", e))?
            .commit_from_file(model_path)
            .map_err(|e| anyhow::anyhow!("Failed to load ONNX model: {:?}", e))?;

        info!("  ✓ ONNX model loaded");

        // Print input metadata for debugging
        info!("  Model inputs:");
        for (i, input) in session.inputs.iter().enumerate() {
            info!("    [{}] name='{}'", i, input.name);
        }

        Ok(Self {
            model_name,
            session: Arc::new(Mutex::new(session)),
            tokenizer: Arc::new(tokenizer),
            embedding_dim: 384, // MiniLM dimension
            max_length: 256,
            total_inferences: Arc::new(Mutex::new(0)),
            total_inference_time_ms: Arc::new(Mutex::new(0.0)),
        })
    }

    /// Generate embedding for a single text
    pub fn embed(&self, text: &str) -> Result<(Array1<f32>, f64)> {
        let start = std::time::Instant::now();

        debug!("Generating embedding for: {}", text);

        // Tokenize input
        let encoding = self
            .tokenizer
            .encode(text, true)
            .map_err(|e| anyhow::anyhow!("Failed to tokenize text: {}", e))?;

        let input_ids = encoding.get_ids();
        let attention_mask = encoding.get_attention_mask();

        // Truncate to max length
        let seq_len = input_ids.len().min(self.max_length);
        let input_ids = &input_ids[..seq_len];
        let attention_mask = &attention_mask[..seq_len];

        // Convert to ONNX format
        let input_ids_array: Array2<i64> = Array2::from_shape_vec(
            (1, seq_len),
            input_ids.iter().map(|&id| id as i64).collect(),
        )?;

        let attention_mask_array: Array2<i64> = Array2::from_shape_vec(
            (1, seq_len),
            attention_mask.iter().map(|&mask| mask as i64).collect(),
        )?;

        // Token type IDs (all zeros for single sequence)
        let token_type_ids_array: Array2<i64> = Array2::zeros((1, seq_len));

        // Run inference
        let (_seq_len, hidden_states_array) = self.run_inference(
            &input_ids_array,
            &attention_mask_array,
            &token_type_ids_array,
        )?;

        // Apply mean pooling
        let embedding = self.mean_pooling(&hidden_states_array, attention_mask)?;

        // L2 normalize
        let mut normalized = embedding;
        let norm: f32 = normalized.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            normalized.mapv_inplace(|x| x / norm);
        }

        let inference_time_ms = start.elapsed().as_secs_f64() * 1000.0;

        // Update metrics
        {
            let mut count = self.total_inferences.lock().unwrap();
            *count += 1;
        }
        {
            let mut total_time = self.total_inference_time_ms.lock().unwrap();
            *total_time += inference_time_ms;
        }

        debug!("✓ Generated embedding in {:.2}ms", inference_time_ms);

        Ok((normalized, inference_time_ms))
    }

    /// Generate embeddings for multiple texts (batch)
    pub fn embed_batch(&self, texts: &[String]) -> Result<(Vec<Array1<f32>>, f64)> {
        let start = std::time::Instant::now();

        // For now, process one at a time
        // TODO: Implement true batch processing with padding
        let embeddings: Result<Vec<Array1<f32>>> = texts
            .iter()
            .map(|text| self.embed(text).map(|(emb, _)| emb))
            .collect();

        let total_time_ms = start.elapsed().as_secs_f64() * 1000.0;

        Ok((embeddings?, total_time_ms))
    }

    /// Run ONNX inference and extract hidden states
    fn run_inference(
        &self,
        input_ids: &Array2<i64>,
        attention_mask: &Array2<i64>,
        token_type_ids: &Array2<i64>,
    ) -> Result<(usize, Array2<f32>)> {
        // Run inference using v2.0 API with inputs! macro
        // Convert ndarray arrays to ort Tensors
        // Clone the arrays to create owned data
        let input_ids_tensor = Tensor::from_array(input_ids.clone())
            .map_err(|e| anyhow::anyhow!("Failed to create input_ids tensor: {:?}", e))?;
        let attention_mask_tensor = Tensor::from_array(attention_mask.clone())
            .map_err(|e| anyhow::anyhow!("Failed to create attention_mask tensor: {:?}", e))?;
        let token_type_ids_tensor = Tensor::from_array(token_type_ids.clone())
            .map_err(|e| anyhow::anyhow!("Failed to create token_type_ids tensor: {:?}", e))?;

        // Lock the session for inference
        let mut session = self.session.lock().unwrap();
        let outputs = session
            .run(ort::inputs![
                input_ids_tensor,
                attention_mask_tensor,
                token_type_ids_tensor
            ])
            .map_err(|e| anyhow::anyhow!("Failed to run inference: {:?}", e))?;

        // Extract hidden states tensor using v2.0 API
        // try_extract_tensor returns (shape, data_slice)
        let (shape, data_slice) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| anyhow::anyhow!("Failed to extract f32 tensor: {:?}", e))?;

        // Get shape: [batch_size, seq_len, hidden_dim]
        // Shape implements Deref<Target = [i64]>
        let shape_dims: &[i64] = &*shape;

        if shape_dims.len() != 3 || shape_dims[0] != 1 || shape_dims[2] != self.embedding_dim as i64
        {
            anyhow::bail!(
                "Unexpected output shape: {:?}, expected [1, seq_len, {}]",
                shape_dims,
                self.embedding_dim
            );
        }

        let seq_len = shape_dims[1] as usize;

        // Convert to owned ndarray (reshape from [1, seq_len, 384] to [seq_len, 384])
        let data: Vec<f32> = data_slice.iter().copied().collect();
        let hidden_states_array = Array2::from_shape_vec((seq_len, self.embedding_dim), data)?;

        Ok((seq_len, hidden_states_array))
    }

    /// Apply mean pooling over sequence dimension
    fn mean_pooling(
        &self,
        hidden_states: &Array2<f32>,
        attention_mask: &[u32],
    ) -> Result<Array1<f32>> {
        let seq_len = hidden_states.shape()[0];
        let emb_dim = hidden_states.shape()[1];

        let mut pooled = Array1::<f32>::zeros(emb_dim);
        let mut mask_sum = 0.0f32;

        for (i, &mask_val) in attention_mask.iter().enumerate().take(seq_len) {
            if mask_val > 0 {
                let token_embedding = hidden_states.row(i);
                for (j, &val) in token_embedding.iter().enumerate() {
                    pooled[j] += val;
                }
                mask_sum += 1.0;
            }
        }

        if mask_sum > 0.0 {
            pooled.mapv_inplace(|x| x / mask_sum);
        }

        Ok(pooled)
    }

    /// Get model name
    pub fn model_name(&self) -> &str {
        &self.model_name
    }

    /// Get embedding dimension
    pub fn embedding_dim(&self) -> usize {
        self.embedding_dim
    }

    /// Get max sequence length
    pub fn max_length(&self) -> usize {
        self.max_length
    }

    /// Get total inference count
    pub fn total_inferences(&self) -> u64 {
        *self.total_inferences.lock().unwrap()
    }

    /// Get average inference latency
    pub fn avg_latency_ms(&self) -> f64 {
        let count = *self.total_inferences.lock().unwrap();
        let total_time = *self.total_inference_time_ms.lock().unwrap();

        if count > 0 {
            total_time / count as f64
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn get_model_path() -> Option<String> {
        env::var("GRAPHICA_MODEL_PATH").ok()
    }

    #[test]
    #[ignore] // Requires model files
    fn test_model_inference() {
        let model_path = get_model_path().expect("GRAPHICA_MODEL_PATH not set");

        let inference = ModelInference::new(
            "minilm".to_string(),
            &format!("{}/model.onnx", model_path),
            &format!("{}/tokenizer.json", model_path),
        )
        .expect("Failed to create inference engine");

        let (embedding, time_ms) = inference
            .embed("customer_email")
            .expect("Failed to generate embedding");

        assert_eq!(embedding.len(), 384);
        assert!(time_ms > 0.0 && time_ms < 100.0); // Should be < 100ms

        // Check normalized
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 0.01);
    }

    #[test]
    #[ignore]
    fn test_batch_inference() {
        let model_path = get_model_path().expect("GRAPHICA_MODEL_PATH not set");

        let inference = ModelInference::new(
            "minilm".to_string(),
            &format!("{}/model.onnx", model_path),
            &format!("{}/tokenizer.json", model_path),
        )
        .expect("Failed to create inference engine");

        let texts = vec![
            "customer_email".to_string(),
            "phone_number".to_string(),
            "address".to_string(),
        ];

        let (embeddings, total_time_ms) = inference
            .embed_batch(&texts)
            .expect("Failed to batch embed");

        assert_eq!(embeddings.len(), 3);
        assert!(total_time_ms > 0.0);

        for emb in &embeddings {
            assert_eq!(emb.len(), 384);
        }
    }
}
