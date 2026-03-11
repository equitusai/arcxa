# Dockerfile for ML embeddings development and testing
# Requires Ubuntu 22.04+ for glibc 2.32+ compatibility with ONNX Runtime
FROM ubuntu:22.04

# Install Rust and dependencies
RUN apt-get update && apt-get install -y \
    curl \
    build-essential \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Install Rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

# Set working directory
WORKDIR /workspace

# Copy workspace Cargo.toml and Cargo files
COPY Cargo.toml /workspace/Cargo.toml
COPY arcxa-core/Cargo.toml /workspace/arcxa-core/
COPY arcxa-core/Cargo.lock /workspace/arcxa-core/ 2>/dev/null || true

# Build dependencies (cache layer)
WORKDIR /workspace/arcxa-core
RUN mkdir -p src && \
    echo "fn main() {}" > src/lib.rs && \
    cargo build && \
    rm -rf src

# Copy source code
COPY arcxa-core/src /workspace/arcxa-core/src
COPY arcxa-core/build.rs /workspace/arcxa-core/ 2>/dev/null || true

# Build
RUN cargo build

# Default command: run tests
CMD ["cargo", "test", "--lib", "ml::embeddings", "--", "--nocapture", "--ignored"]
