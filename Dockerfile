FROM redhat/ubi9 AS builder

RUN dnf --disableplugin=subscription-manager install -y \
    gcc gcc-c++ cmake glibc-devel \
    openssl-devel \
    libcurl-devel \ 
    unixODBC-devel \
    pkg-config \
    unzip \
    clang \
    && dnf clean all
    
ARG TARGETPLATFORM

RUN if [ "$TARGETPLATFORM" = "linux/amd64" ]; then \
    PROTOC_VERSION="32.1" && \
    PB_REL="https://github.com/protocolbuffers/protobuf/releases" && \
    curl -LO $PB_REL/download/v$PROTOC_VERSION/protoc-$PROTOC_VERSION-linux-x86_64.zip && \
    unzip protoc-$PROTOC_VERSION-linux-x86_64.zip -d /usr/local; \
elif [ "$TARGETPLATFORM" = "linux/ppc64le" ]; then \
    PROTOC_VERSION="32.1" && \
    PB_REL="https://github.com/protocolbuffers/protobuf/releases" && \
    curl -LO $PB_REL/download/v$PROTOC_VERSION/protoc-$PROTOC_VERSION-linux-ppcle_64.zip && \
    unzip protoc-$PROTOC_VERSION-linux-ppcle_64.zip -d /usr/local; \
else \
    echo "Unsupported platform: $TARGETPLATFORM"; \
fi

ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y \
    --default-toolchain 1.91.1
RUN rustc --version

WORKDIR /usr/src/arcxa

# Copy root Cargo.toml first (needed for path dependencies)
COPY Cargo.toml ./
COPY Cargo.lock ./

# Copy proto files (needed by build.rs files)
COPY proto ./proto

# Copy all crate directories (workspace members only, no root src)
COPY arcxa-core ./arcxa-core
COPY arcxa-model-service ./arcxa-model-service
COPY arcxa-evidence-ingestion ./arcxa-evidence-ingestion
COPY arcxa-traceability ./arcxa-traceability
COPY arcxa-verification ./arcxa-verification
COPY arcxa-shard ./arcxa-shard
COPY arcxa-coordinator ./arcxa-coordinator
COPY arcxa-cli ./arcxa-cli
COPY arcxa-migrations ./arcxa-migrations

# Use shared target directory so proto files are accessible across builds
ENV CARGO_TARGET_DIR=/usr/src/arcxa/target

# Build all binaries (cd into each directory like build.sh does)
# Build arcxa-core first (shared library)
WORKDIR /usr/src/arcxa/arcxa-core
RUN cargo build --locked --release --lib

WORKDIR /usr/src/arcxa/arcxa-migrations
RUN cargo build --locked --release --lib


# Build arcxa-model-service (with clean environment for OpenSSL)
WORKDIR /usr/src/arcxa/arcxa-model-service
RUN env -u CC -u CFLAGS -u CPPFLAGS -u LDFLAGS \
        -u C_INCLUDE_PATH -u CPLUS_INCLUDE_PATH -u LIBRARY_PATH \
        PKG_CONFIG_PATH=/usr/lib64/pkgconfig:/usr/share/pkgconfig \
        RUSTFLAGS="" \
        cargo build --locked --release

# Build arcxa-evidence-ingestion
WORKDIR /usr/src/arcxa/arcxa-evidence-ingestion
RUN env -u CC -u CFLAGS -u CPPFLAGS -u LDFLAGS \
        -u C_INCLUDE_PATH -u CPLUS_INCLUDE_PATH -u LIBRARY_PATH \
        PKG_CONFIG_PATH=/usr/lib64/pkgconfig:/usr/share/pkgconfig \
        RUSTFLAGS="" \
        cargo build --locked --release

# Build arcxa-traceability
WORKDIR /usr/src/arcxa/arcxa-traceability
RUN env -u CC -u CFLAGS -u CPPFLAGS -u LDFLAGS \
        -u C_INCLUDE_PATH -u CPLUS_INCLUDE_PATH -u LIBRARY_PATH \
        PKG_CONFIG_PATH=/usr/lib64/pkgconfig:/usr/share/pkgconfig \
        RUSTFLAGS="" \
        cargo build --locked --release

# Build arcxa-verification
WORKDIR /usr/src/arcxa/arcxa-verification
RUN env -u CC -u CFLAGS -u CPPFLAGS -u LDFLAGS \
        -u C_INCLUDE_PATH -u CPLUS_INCLUDE_PATH -u LIBRARY_PATH \
        PKG_CONFIG_PATH=/usr/lib64/pkgconfig:/usr/share/pkgconfig \
        RUSTFLAGS="" \
        cargo build --locked --release

# Build arcxa-shard (depends on root proto files)
WORKDIR /usr/src/arcxa/arcxa-shard
RUN cargo build --locked --release

# Build arcxa-coordinator (depends on root proto files)
WORKDIR /usr/src/arcxa/arcxa-coordinator
RUN cargo build --locked --release


FROM redhat/ubi9-minimal AS packages

RUN mkdir /microdir
RUN microdnf install \
    --releasever 9\
    --noplugins \
    --installroot /microdir \
    --setopt=cachedir=/var/cache \
    --setopt=reposdir=/etc/yum.repos.d \
    --setopt=varsdir=/etc/dnf/vars \
    --config=/etc/dnf/dnf.conf \
    -y shadow-utils glibc libstdc++ unixODBC libaio && \
    microdnf clean all

# Download ONNX Runtime
FROM redhat/ubi9-minimal AS onnxruntime
ARG ORT_VERSION=1.22.0
RUN microdnf install -y wget tar gzip && \
    wget -q https://github.com/microsoft/onnxruntime/releases/download/v${ORT_VERSION}/onnxruntime-linux-x64-${ORT_VERSION}.tgz && \
    tar -xzf onnxruntime-linux-x64-${ORT_VERSION}.tgz && \
    mkdir -p /onnxruntime/lib && \
    cp onnxruntime-linux-x64-${ORT_VERSION}/lib/libonnxruntime.so* /onnxruntime/lib/

FROM redhat/ubi9-micro

COPY --from=packages /microdir /
COPY --from=onnxruntime /onnxruntime/lib /usr/local/lib/onnxruntime

RUN groupadd -g 1000 eqworker && useradd -u 1000 eqworker -g eqworker
USER eqworker
WORKDIR /home/eqworker/
RUN mkdir ./models/ ./data

# Set environment variables
ENV ORT_DYLIB_PATH=/usr/local/lib/onnxruntime/libonnxruntime.so
ENV ROW_LINEAGE_DB_PATH=/home/eqworker/data/row-lineage-db

# Copy binaries from shared target directory
COPY --from=builder --chown=eqworker:eqworker /usr/src/arcxa/target/release/arcxa-model-service ./
COPY --from=builder --chown=eqworker:eqworker /usr/src/arcxa/target/release/arcxa-evidence-ingestion ./
COPY --from=builder --chown=eqworker:eqworker /usr/src/arcxa/target/release/arcxa-traceability ./
COPY --from=builder --chown=eqworker:eqworker /usr/src/arcxa/target/release/arcxa-verification ./
COPY --from=builder --chown=eqworker:eqworker /usr/src/arcxa/target/release/arcxa-shard ./
COPY --from=builder --chown=eqworker:eqworker /usr/src/arcxa/target/release/arcxa-coordinator ./
# COPY ./models/ ./models/
