#!/bin/bash
# Build and run arcxa-model-service in Docker (glibc 2.36 compatible)

set -e

echo "=== ARCXA Model Service - Docker Build ==="
echo ""

cd "$(dirname "$0")"

# Check Docker
if ! command -v docker &> /dev/null; then
    echo "❌ Docker not found. Please install Docker."
    exit 1
fi

echo "Building Docker image (this may take 5-10 minutes first time)..."
echo "  Base: debian:bookworm-slim (glibc 2.36)"
echo "  Build: Multi-stage with Rust 1.75"
echo ""

# Build from project root
docker build \
    -f arcxa-model-service/Dockerfile \
    -t arcxa-model-service:latest \
    .

echo ""
echo "✅ Build complete!"
echo ""
echo "To run the service:"
echo "  docker run -d --name arcxa-model-service -p 50051:50051 arcxa-model-service:latest"
echo ""
echo "Or use docker-compose:"
echo "  docker-compose up -d model-service"
