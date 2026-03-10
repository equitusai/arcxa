#!/bin/bash
# Setup script for Phase 2 Semantic Matcher
# Downloads the MiniLM ONNX model and tokenizer from HuggingFace

set -e

BLUE='\033[0;34m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${BLUE}╔════════════════════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║  Phase 2 Model Setup                                       ║${NC}"
echo -e "${BLUE}║  Downloading sentence-transformers/all-MiniLM-L6-v2        ║${NC}"
echo -e "${BLUE}╚════════════════════════════════════════════════════════════╝${NC}"
echo ""

# Create models directory
MODEL_DIR="models/minilm"
mkdir -p "$MODEL_DIR"

echo -e "${YELLOW}[1/3] Downloading ONNX model (90MB)...${NC}"
if [ -f "$MODEL_DIR/model.onnx" ]; then
    echo -e "${GREEN}✓ Model already exists, skipping download${NC}"
else
    curl -L "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/onnx/model.onnx" \
        -o "$MODEL_DIR/model.onnx"
    echo -e "${GREEN}✓ Model downloaded${NC}"
fi

echo ""
echo -e "${YELLOW}[2/3] Downloading tokenizer...${NC}"
if [ -f "$MODEL_DIR/tokenizer.json" ]; then
    echo -e "${GREEN}✓ Tokenizer already exists, skipping download${NC}"
else
    curl -L "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/tokenizer.json" \
        -o "$MODEL_DIR/tokenizer.json"
    echo -e "${GREEN}✓ Tokenizer downloaded${NC}"
fi

echo ""
echo -e "${YELLOW}[3/3] Downloading config...${NC}"
if [ -f "$MODEL_DIR/config.json" ]; then
    echo -e "${GREEN}✓ Config already exists, skipping download${NC}"
else
    curl -L "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/config.json" \
        -o "$MODEL_DIR/config.json"
    echo -e "${GREEN}✓ Config downloaded${NC}"
fi

echo ""
echo -e "${BLUE}╔════════════════════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║  Setup Complete!                                           ║${NC}"
echo -e "${BLUE}╚════════════════════════════════════════════════════════════╝${NC}"
echo ""
echo "Model information:"
echo "  Location: $MODEL_DIR"
echo "  Model size: $(du -h "$MODEL_DIR/model.onnx" | cut -f1)"
echo "  Architecture: MiniLM-L6 (6 layers, 22.7M parameters)"
echo "  Embedding dimension: 384"
echo ""
echo "Next steps:"
echo "  1. Set environment variable: export GRAPHICA_MODEL_PATH=\"$(pwd)/$MODEL_DIR\""
echo "  2. Build the coordinator: cargo build --release"
echo "  3. Run with Phase 2 enabled: ./target/release/arcxa-coordinator"
echo ""
