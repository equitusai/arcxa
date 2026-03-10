# Dockerfile for ARCXA Model Service
# Multi-stage build for optimal image size

# Stage 1: Build
FROM rust:1.85-slim as builder

# Install build dependencies including ONNX Runtime requirements
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    protobuf-compiler \
    cmake \
    clang \
    libclang-dev \
    wget \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Copy proto files first (needed by build.rs)
COPY proto ./proto

# Copy model service
COPY arcxa-model-service ./arcxa-model-service

# Build model service (release mode)
WORKDIR /build/arcxa-model-service
RUN cargo build --release --bin arcxa-model-service

# Stage 2: Runtime
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    libssl3 \
    ca-certificates \
    wget \
    && rm -rf /var/lib/apt/lists/*

# Download and install ONNX Runtime
ARG ORT_VERSION=1.22.0
RUN wget -q https://github.com/microsoft/onnxruntime/releases/download/v${ORT_VERSION}/onnxruntime-linux-x64-${ORT_VERSION}.tgz && \
    tar -xzf onnxruntime-linux-x64-${ORT_VERSION}.tgz && \
    mkdir -p /usr/local/lib/onnxruntime && \
    cp onnxruntime-linux-x64-${ORT_VERSION}/lib/libonnxruntime.so* /usr/local/lib/onnxruntime/ && \
    rm -rf onnxruntime-linux-x64-${ORT_VERSION}.tgz onnxruntime-linux-x64-${ORT_VERSION} && \
    ldconfig

# Install grpcurl for health checks
RUN wget -qO- https://github.com/fullstorydev/grpcurl/releases/download/v1.8.9/grpcurl_1.8.9_linux_x86_64.tar.gz | \
    tar -xz -C /usr/local/bin grpcurl && \
    chmod +x /usr/local/bin/grpcurl

# Create app directory and user
RUN useradd -m -u 1000 graphica && \
    mkdir -p /app/models /app/cache && \
    chown -R graphica:graphica /app

WORKDIR /app

# Copy binary from builder
COPY --from=builder /build/arcxa-model-service/target/release/arcxa-model-service /app/arcxa-model-service

# Create models directory for volume mounts
RUN mkdir -p /app/models/minilm && chown -R graphica:graphica /app/models

USER graphica

# Expose gRPC port
EXPOSE 50051

# Health check via gRPC
HEALTHCHECK --interval=15s --timeout=5s --start-period=30s --retries=5 \
    CMD grpcurl -plaintext localhost:50051 list || exit 1

# Environment defaults
ENV GRPC_PORT=50051 \
    GRAPHICA_MODEL_PATH=/app/models/minilm \
    RUST_LOG=info \
    ORT_DYLIB_PATH=/usr/local/lib/onnxruntime/libonnxruntime.so

# Run model service
ENTRYPOINT ["/app/arcxa-model-service"]
