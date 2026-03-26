#!/bin/bash
# Run ARCXA in single-node mode (coordinator only, no shards)
# Useful for development and testing without distributed complexity

set -e

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
RED='\033[0;31m'
NC='\033[0m' # No Color
RUST_TOOLCHAIN="stable"

run_cargo() {
    local cargo_args=("$@")
    if [ "${cargo_args[0]}" = "cargo" ]; then
        cargo_args=("${cargo_args[@]:1}")
    fi

    env -u CC \
        -u CFLAGS \
        -u CPPFLAGS \
        -u LDFLAGS \
        -u C_INCLUDE_PATH \
        -u CPLUS_INCLUDE_PATH \
        -u LIBRARY_PATH \
        PKG_CONFIG_PATH=/usr/lib/x86_64-linux-gnu/pkgconfig \
        RUSTFLAGS="" \
        cargo +${RUST_TOOLCHAIN} "${cargo_args[@]}"
}

echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}Running ARCXA (Single-Node Mode)${NC}"
echo -e "${GREEN}========================================${NC}"
echo ""

# Cleanup any existing processes and locks
echo -e "${YELLOW}Cleaning up any existing ARCXA processes...${NC}"
pkill -9 -f arcxa-coordinator 2>/dev/null || true
pkill -9 -f arcxa-model-service 2>/dev/null || true

echo -e "${YELLOW}Removing stale RocksDB locks...${NC}"
find ./data -name "LOCK" -type f -delete 2>/dev/null || true

echo -e "${YELLOW}Wiping file library database (schema changes)...${NC}"
rm -rf ./data/file-library-db 2>/dev/null || true

echo -e "${GREEN}✓ Cleanup complete${NC}"
echo ""

# Check if infrastructure is running
echo -e "${YELLOW}Checking infrastructure services...${NC}"

if ! docker compose ps | grep -q "arcxa-kafka.*healthy"; then
    echo -e "${YELLOW}Kafka not running. Starting infrastructure...${NC}"
    docker compose up -d zookeeper kafka schema-registry

    # Wait for services to be healthy
    echo -e "${YELLOW}Waiting for services to be healthy...${NC}"
    sleep 10
fi

echo -e "${GREEN}✓ Infrastructure is ready${NC}"
echo ""

# Set environment variables for coordinator
export RUST_LOG=${RUST_LOG:-info}
export KAFKA_BROKERS=${KAFKA_BROKERS:-localhost:9092}
export KAFKA_CONSUMER_GROUP=${KAFKA_CONSUMER_GROUP:-arcxa-coordinator}
export KAFKA_LINEAGE_TOPIC=${KAFKA_LINEAGE_TOPIC:-graphica.lineage}
export KAFKA_QUALITY_TOPIC=${KAFKA_QUALITY_TOPIC:-graphica.quality}
export ENABLE_AUTH=${ENABLE_AUTH:-false}
export ENVIRONMENT=${ENVIRONMENT:-development}

# IMPORTANT: No SHARD_URLS for single-node mode
unset SHARD_URLS

# Create data directories
mkdir -p ./data/coordinator/rocksdb ./data/parquet ./data/archive ./data/audit

echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}Building ARCXA Coordinator${NC}"
echo -e "${BLUE}========================================${NC}"
echo ""

# Build coordinator
echo -e "${YELLOW}Building arcxa-coordinator...${NC}"
(cd arcxa-coordinator && run_cargo cargo build --release 2>&1 | tail -3)

echo -e "${GREEN}✓ Coordinator built successfully${NC}"
echo ""

# Cleanup function
cleanup() {
    echo ""
    echo -e "${YELLOW}Shutting down ARCXA...${NC}"
    kill $COORDINATOR_PID 2>/dev/null || true
    wait $COORDINATOR_PID 2>/dev/null || true
    echo -e "${GREEN}✓ Shutdown complete${NC}"
    exit 0
}

trap cleanup SIGINT SIGTERM

echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}Starting Coordinator (Single-Node)${NC}"
echo -e "${BLUE}========================================${NC}"
echo ""

# Start Coordinator in single-node mode
echo -e "${YELLOW}Starting Coordinator in single-node mode...${NC}"
RUST_LOG=$RUST_LOG \
ENVIRONMENT=$ENVIRONMENT \
REST_PORT=8080 \
GRPC_PORT=9090 \
ROCKSDB_PATH=./data/coordinator/rocksdb \
PARQUET_PATH=./data/parquet \
ARCHIVE_PATH=./data/archive \
KAFKA_BROKERS=$KAFKA_BROKERS \
KAFKA_CONSUMER_GROUP=$KAFKA_CONSUMER_GROUP \
KAFKA_LINEAGE_TOPIC=$KAFKA_LINEAGE_TOPIC \
KAFKA_QUALITY_TOPIC=$KAFKA_QUALITY_TOPIC \
ENABLE_AUTH=$ENABLE_AUTH \
./arcxa-coordinator/target/release/arcxa-coordinator > ./data/coordinator/coordinator.log 2>&1 &
COORDINATOR_PID=$!
echo -e "${GREEN}✓ Coordinator started (PID: $COORDINATOR_PID)${NC}"

# Wait for coordinator to be ready
echo -e "${YELLOW}Waiting for coordinator to be ready...${NC}"
sleep 3

# Verify coordinator is responsive
for i in {1..10}; do
    if curl -s http://localhost:8080/health > /dev/null 2>&1; then
        echo -e "${GREEN}✓ Coordinator is responsive${NC}"
        break
    fi
    if [ $i -eq 10 ]; then
        echo -e "${RED}⚠ Coordinator not responding after 10 attempts - check ./data/coordinator/coordinator.log${NC}"
    else
        sleep 1
    fi
done

echo ""
echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}ARCXA is Running!${NC}"
echo -e "${GREEN}========================================${NC}"
echo ""
echo -e "${BLUE}Architecture:${NC}"
echo -e "  Mode:         ${GREEN}Single-Node (No Shards)${NC}"
echo -e "  Coordinator:  PID $COORDINATOR_PID"
echo ""
echo -e "${BLUE}Endpoints:${NC}"
echo -e "  ${GREEN}REST API:${NC}  http://localhost:8080"
echo -e "  ${GREEN}gRPC API:${NC}  http://localhost:9090"
echo -e "  ${GREEN}Health:${NC}    http://localhost:8080/health"
echo -e "  ${GREEN}Metrics:${NC}   http://localhost:8080/metrics"
echo -e "  ${GREEN}OpenAPI:${NC}   http://localhost:8080/openapi.yaml"
echo ""
echo -e "${BLUE}RDF Storage:${NC}"
echo -e "  ${GREEN}Mode:${NC}         Single-node (embedded Oxigraph)"
echo -e "  ${GREEN}Storage:${NC}      ./data/coordinator/rocksdb"
echo -e "  ${GREEN}Performance:${NC}  Good for development, limited by single machine"
echo ""
echo -e "${BLUE}Logs:${NC}"
echo -e "  Coordinator: ${YELLOW}tail -f ./data/coordinator/coordinator.log${NC}"
echo ""
echo -e "${YELLOW}Note: For production with horizontal scaling, use ./run-local.sh instead${NC}"
echo -e "${YELLOW}Press Ctrl+C to stop the service${NC}"
echo ""

# Wait for coordinator to exit
wait $COORDINATOR_PID
