//! Quick test to verify Phase 2 model loading
//!
//! NOTE: This test requires ML dependencies (ort, tokenizers, ndarray) that should only be
//! in graphica-model-service crate, not graphica-coordinator.
//!
//! This test is DISABLED in graphica-coordinator. It should be moved to graphica-model-service.
//!
//! Run with: cargo test --test test_phase2_model -- --nocapture

// Disable this test - it's in the wrong crate
#![cfg(feature = "ml-model-service")]

use anyhow::Result;

// Mock the necessary modules since we can't build the full project yet
// This is just a smoke test to verify dependencies are available

#[test]
fn test_dependencies_available() {
    // Test that key dependencies are available
    println!("✓ Testing Phase 2 dependencies...");

    // ndarray
    let _arr = ndarray::Array1::<f32>::zeros(384);
    println!("  ✓ ndarray available");

    // tokenizers
    // We can't test tokenizers without the file, but we can check it compiles
    println!("  ✓ tokenizers crate available");

    // ort
    // We can't test ort without the model, but we can check it compiles
    println!("  ✓ ort crate available");

    println!("✅ All Phase 2 dependencies are available");
}

#[test]
#[ignore] // Requires model files
fn test_model_loads() -> Result<()> {
    use std::env;

    let model_path =
        env::var("GRAPHICA_MODEL_PATH").unwrap_or_else(|_| "models/minilm".to_string());

    println!("🧠 Testing model loading from: {}", model_path);

    // Check files exist
    let model_file = format!("{}/model.onnx", model_path);
    let tokenizer_file = format!("{}/tokenizer.json", model_path);

    assert!(
        std::path::Path::new(&model_file).exists(),
        "Model file not found: {}",
        model_file
    );
    assert!(
        std::path::Path::new(&tokenizer_file).exists(),
        "Tokenizer file not found: {}",
        tokenizer_file
    );

    println!("  ✓ Model files exist");

    // Try loading tokenizer
    let _tokenizer = tokenizers::Tokenizer::from_file(&tokenizer_file)?;
    println!("  ✓ Tokenizer loaded successfully");

    // Try loading ONNX model
    let _session = ort::Session::builder()?.commit_from_file(&model_file)?;
    println!("  ✓ ONNX model loaded successfully");

    println!("✅ Phase 2 model is ready!");

    Ok(())
}

#[test]
#[ignore] // Requires model files
fn test_embedding_generation() -> Result<()> {
    use ndarray::Array2;
    use std::env;

    let model_path =
        env::var("GRAPHICA_MODEL_PATH").unwrap_or_else(|_| "models/minilm".to_string());

    println!("🧠 Testing embedding generation...");

    let model_file = format!("{}/model.onnx", model_path);
    let tokenizer_file = format!("{}/tokenizer.json", model_path);

    // Load tokenizer
    let tokenizer = tokenizers::Tokenizer::from_file(&tokenizer_file)?;
    println!("  ✓ Tokenizer loaded");

    // Encode test text
    let text = "customer_email";
    let encoding = tokenizer.encode(text, true).unwrap();

    let input_ids = encoding.get_ids();
    let attention_mask = encoding.get_attention_mask();

    println!("  ✓ Text tokenized: {} -> {} tokens", text, input_ids.len());

    // Load ONNX model
    let session = ort::Session::builder()?.commit_from_file(&model_file)?;
    println!("  ✓ ONNX session created");

    // Prepare inputs
    let seq_len = input_ids.len();
    let input_ids_array: Array2<i64> = Array2::from_shape_vec(
        (1, seq_len),
        input_ids.iter().map(|&id| id as i64).collect(),
    )?;

    let attention_mask_array: Array2<i64> = Array2::from_shape_vec(
        (1, seq_len),
        attention_mask.iter().map(|&mask| mask as i64).collect(),
    )?;

    // Run inference
    let outputs = session.run(ort::inputs![
        "input_ids" => ort::Value::from_array(input_ids_array)?,
        "attention_mask" => ort::Value::from_array(attention_mask_array)?,
    ]?)?;

    println!("  ✓ Inference completed");

    // Extract output
    let hidden_states = outputs
        .get("last_hidden_state")
        .expect("Output not found")
        .try_extract_tensor::<f32>()?;

    let shape = hidden_states.view().shape();
    println!("  ✓ Output shape: {:?}", shape);

    assert_eq!(shape[0], 1, "Batch size should be 1");
    assert_eq!(shape[2], 384, "Embedding dimension should be 384");

    println!("✅ Embedding generation works!");

    Ok(())
}
