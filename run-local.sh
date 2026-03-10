#!/bin/bash
# Run ARCXA locally with Docker infrastructure and local binaries
# This script runs the sharded architecture: coordinator + multiple shards
#
# Usage: ./run-local.sh [OPTIONS]
#
# OPTIONS:
#   --help      Show this help message
#   --inspect   Show binary paths and versions without running
#   --dev       Build in debug mode (faster builds, slower runtime)
#
# NOTE: This is for single-coordinator development mode (no HA, no Raft coordination)
# For high-availability with Raft leader election, use: ./run-local-ha.sh

set -e

VERSION="0.4.1"  # Memory optimization + GDPR refactor

run_cargo() {
    env -u CC \
        -u CFLAGS \
        -u CPPFLAGS \
        -u LDFLAGS \
        -u C_INCLUDE_PATH \
        -u CPLUS_INCLUDE_PATH \
        -u LIBRARY_PATH \
        PKG_CONFIG_PATH=/usr/lib/x86_64-linux-gnu/pkgconfig \
        RUSTFLAGS="" \
        "$@"
}

# Parse command line arguments
BUILD_MODE="release"
BUILD_FLAG="--release"
TARGET_DIR="release"
INSPECT_ONLY=false

show_help() {
    echo "ARCXA Local Runner v${VERSION}"
    echo ""
    echo "Usage: ./run-local.sh [OPTIONS]"
    echo ""
    echo "OPTIONS:"
    echo "  --help      Show this help message"
    echo "  --inspect   Show binary paths and versions without running"
    echo "  --dev       Build in debug mode (faster builds, slower runtime)"
    echo ""
    echo "EXAMPLES:"
    echo "  ./run-local.sh              # Run in release mode"
    echo "  ./run-local.sh --dev        # Run in debug mode"
    echo "  ./run-local.sh --inspect    # Check binaries and versions"
    echo ""
    echo "NOTES:"
    echo "  - This runs single-coordinator mode (development)"
    echo "  - For HA with Raft coordination: ./run-local-ha.sh"
    echo "  - Logs: ./data/coordinator/coordinator.log"
    exit 0
}

inspect_binaries() {
    echo "========================================="
    echo "ARCXA Binary Inspection v${VERSION}"
    echo "========================================="
    echo ""

    echo "Build Configuration:"
    echo "  Build Mode: ${BUILD_MODE}"
    echo "  Target Dir: ./target/${TARGET_DIR}"
    echo ""

    echo "Binary Paths:"
    COORD_BIN="./target/${TARGET_DIR}/arcxa-coordinator"
    MODEL_BIN="./target/${TARGET_DIR}/arcxa-model-service"
    SHARD_BIN="./arcxa-shard/target/${TARGET_DIR}/arcxa-shard"

    if [ -f "$COORD_BIN" ]; then
        COORD_SIZE=$(ls -lh "$COORD_BIN" | awk '{print $5}')
        COORD_DATE=$(ls -lh "$COORD_BIN" | awk '{print $6, $7, $8}')
        echo "  ✓ Coordinator:   $COORD_BIN"
        echo "                   Size: $COORD_SIZE, Modified: $COORD_DATE"
    else
        echo "  ✗ Coordinator:   NOT FOUND at $COORD_BIN"
    fi

    if [ -f "$MODEL_BIN" ]; then
        MODEL_SIZE=$(ls -lh "$MODEL_BIN" | awk '{print $5}')
        MODEL_DATE=$(ls -lh "$MODEL_BIN" | awk '{print $6, $7, $8}')
        echo "  ✓ Model Service: $MODEL_BIN"
        echo "                   Size: $MODEL_SIZE, Modified: $MODEL_DATE"
    else
        echo "  ✗ Model Service: NOT FOUND at $MODEL_BIN"
    fi

    if [ -f "$SHARD_BIN" ]; then
        SHARD_SIZE=$(ls -lh "$SHARD_BIN" | awk '{print $5}')
        SHARD_DATE=$(ls -lh "$SHARD_BIN" | awk '{print $6, $7, $8}')
        echo "  ✓ Shard:         $SHARD_BIN"
        echo "                   Size: $SHARD_SIZE, Modified: $SHARD_DATE"
    else
        echo "  ✗ Shard:         NOT FOUND at $SHARD_BIN"
    fi

    echo ""
    echo "Running Processes:"
    if pgrep -f arcxa-coordinator > /dev/null; then
        COORD_PID=$(pgrep -f arcxa-coordinator)
        COORD_EXE=$(readlink -f /proc/$COORD_PID/exe 2>/dev/null || echo "unknown")
        echo "  ⚠ Coordinator:   RUNNING (PID $COORD_PID)"
        echo "                   Exe: $COORD_EXE"
    else
        echo "  ✓ Coordinator:   Not running"
    fi

    if pgrep -f arcxa-shard > /dev/null; then
        echo "  ⚠ Shards:        RUNNING ($(pgrep -f arcxa-shard | wc -l) processes)"
    else
        echo "  ✓ Shards:        Not running"
    fi

    if pgrep -f arcxa-model-service > /dev/null; then
        echo "  ⚠ Model Service: RUNNING"
    else
        echo "  ✓ Model Service: Not running"
    fi

    echo ""
    echo "Version Info:"
    echo "  Script Version: ${VERSION}"
    echo "  Workspace Ver:  $(grep '^version' Cargo.toml | head -1 | cut -d'"' -f2)"
    echo ""
    exit 0
}

for arg in "$@"; do
    case $arg in
        --help)
            show_help
            ;;
        --inspect)
            INSPECT_ONLY=true
            ;;
        --dev)
            BUILD_MODE="debug"
            BUILD_FLAG=""
            TARGET_DIR="debug"
            ;;
        *)
            echo "Unknown option: $arg"
            echo "Use --help for usage information"
            exit 1
            ;;
    esac
done

if [ "$INSPECT_ONLY" = true ]; then
    inspect_binaries
fi

if [ "$BUILD_MODE" == "debug" ]; then
    echo "🚀 Using DEBUG mode for faster builds"
fi

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
RED='\033[0;31m'
NC='\033[0m' # No Color

echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}Running ARCXA Locally (Sharded)${NC}"
echo -e "${GREEN}Single Coordinator - Development Mode${NC}"
echo -e "${GREEN}Build Mode: ${BUILD_MODE}${NC}"
echo -e "${GREEN}========================================${NC}"
echo ""
echo -e "${YELLOW}NOTE: For HA with Raft coordination, use: ${GREEN}./run-local-ha.sh${NC}"
echo ""

# Cleanup any existing processes and locks
echo -e "${YELLOW}Cleaning up any existing ARCXA processes...${NC}"
pkill -9 -f arcxa-coordinator 2>/dev/null || true
pkill -9 -f arcxa-shard 2>/dev/null || true
pkill -9 -f arcxa-model-service 2>/dev/null || true

echo -e "${YELLOW}Removing stale RocksDB locks...${NC}"
find ./data -name "LOCK" -type f -delete 2>/dev/null || true

echo -e "${YELLOW}Wiping file library database (schema changes)...${NC}"
rm -rf ./data/file-library-db 2>/dev/null || true

echo -e "${YELLOW}Wiping workflow execution database (fresh start)...${NC}"
rm -rf ./data/workflow-executions-db 2>/dev/null || true

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
export ROW_LINEAGE_DB_PATH=${ROW_LINEAGE_DB_PATH:-./data/row-lineage-db}

# Create data directories
mkdir -p ./data/shard-0 ./data/shard-1 ./data/shard-2 ./data/coordinator/rocksdb
mkdir -p ./data/parquet ./data/archive ./data/audit
mkdir -p ./data/workflow-executions ./data/file-library ./data/row-lineage-db

echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}Building ARCXA Components${NC}"
echo -e "${BLUE}========================================${NC}"
echo ""

# Build all components
# Note: Workspace binaries go to ./target/release/, shard builds separately

echo -e "${YELLOW}Building workspace components (coordinator, model-service, core) in ${BUILD_MODE} mode...${NC}"
echo -e "${YELLOW}Note: Clearing conda environment variables to avoid OpenSSL conflicts${NC}"
run_cargo cargo build $BUILD_FLAG --features cryptographic-audit,odbc 2>&1 | tail -1

echo -e "${YELLOW}Building arcxa-shard (separate build) in ${BUILD_MODE} mode...${NC}"
echo -e "${YELLOW}Note: Clearing conda environment variables to avoid OpenSSL conflicts${NC}"
(cd arcxa-shard && run_cargo cargo build $BUILD_FLAG 2>&1 | tail -1)

echo -e "${GREEN}✓ All components built successfully${NC}"
echo ""

# Cleanup function
cleanup() {
    echo ""
    echo -e "${YELLOW}Shutting down ARCXA...${NC}"
    # Kill all processes (ignore errors for processes that weren't started)
    kill $COORDINATOR_PID 2>/dev/null || true
    kill $MODEL_SERVICE_PID 2>/dev/null || true
    kill $SHARD_0_PID 2>/dev/null || true
    kill $SHARD_1_PID 2>/dev/null || true
    [ -n "$SHARD_2_PID" ] && kill $SHARD_2_PID 2>/dev/null || true

    # Wait for processes to exit
    wait $COORDINATOR_PID 2>/dev/null || true
    wait $MODEL_SERVICE_PID 2>/dev/null || true
    wait $SHARD_0_PID 2>/dev/null || true
    wait $SHARD_1_PID 2>/dev/null || true
    [ -n "$SHARD_2_PID" ] && wait $SHARD_2_PID 2>/dev/null || true

    echo -e "${GREEN}✓ Shutdown complete${NC}"
    exit 0
}

trap cleanup SIGINT SIGTERM

echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}Starting Coordinator (gRPC Service)${NC}"
echo -e "${BLUE}========================================${NC}"
echo ""

# Start Coordinator FIRST (shards need to register with it)
echo -e "${YELLOW}Starting Coordinator with Cryptographic Audit & Workflow Persistence...${NC}"
RUST_LOG=$RUST_LOG \
ENVIRONMENT=$ENVIRONMENT \
GRAPHICA_MODEL_SERVICE_URL="http://localhost:50051" \
REST_PORT=8082 \
GRPC_PORT=50051 \
ROCKSDB_PATH=./data/coordinator/rocksdb \
PARQUET_PATH=./data/parquet \
ARCHIVE_PATH=./data/archive \
ENABLE_CRYPTOGRAPHIC_AUDIT=true \
AUDIT_CHAIN_PATH=./data/audit \
AUDIT_USER_ID="local-coordinator" \
KAFKA_BROKERS=$KAFKA_BROKERS \
KAFKA_CONSUMER_GROUP=$KAFKA_CONSUMER_GROUP \
KAFKA_LINEAGE_TOPIC=$KAFKA_LINEAGE_TOPIC \
KAFKA_QUALITY_TOPIC=$KAFKA_QUALITY_TOPIC \
ENABLE_AUTH=$ENABLE_AUTH \
WORKFLOW_EXECUTION_DB_PATH=./data/workflow-executions-db \
FILE_LIBRARY_DB_PATH=./data/file-library-db \
ROW_LINEAGE_DB_PATH=$ROW_LINEAGE_DB_PATH \
CHECKPOINT_ENABLED=true \
CHECKPOINT_INTERVAL_SECS=30 \
./target/${TARGET_DIR}/arcxa-coordinator > ./data/coordinator/coordinator.log 2>&1 &
COORDINATOR_PID=$!
echo -e "${GREEN}✓ Coordinator started with cryptographic audit & workflow persistence (PID: $COORDINATOR_PID)${NC}"

# Wait for coordinator to be ready
echo -e "${YELLOW}Waiting for coordinator gRPC service to be ready...${NC}"
sleep 3

echo ""
echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}Starting Shards (Auto-Registration)${NC}"
echo -e "${BLUE}========================================${NC}"
echo ""

# Note: Shards will automatically register with coordinator via gRPC
# - Coordinator assigns shard IDs dynamically
# - Hash ranges distributed evenly (2 shards: 0-50%, 50-100%)
# - Persistent identity survives restarts

# Start Shard 0 (will register and receive shard_id=0)
# Note: Shard binary is in separate target dir
echo -e "${YELLOW}Starting Shard 0 on port 9100 (auto-registration)...${NC}"
RUST_LOG=$RUST_LOG \
./arcxa-shard/target/${TARGET_DIR}/arcxa-shard \
  --data-path ./data/shard-0 \
  --port 9100 \
  --coordinator-url http://localhost:50051 \
  --heartbeat-interval 30 \
  > ./data/shard-0/shard.log 2>&1 &
SHARD_0_PID=$!
echo -e "${GREEN}✓ Shard 0 starting (PID: $SHARD_0_PID) - will auto-register with coordinator${NC}"

# Start Shard 1 (will register and receive shard_id=1)
echo -e "${YELLOW}Starting Shard 1 on port 9101 (auto-registration)...${NC}"
RUST_LOG=$RUST_LOG \
./arcxa-shard/target/${TARGET_DIR}/arcxa-shard \
  --data-path ./data/shard-1 \
  --port 9101 \
  --coordinator-url http://localhost:50051 \
  --heartbeat-interval 30 \
  > ./data/shard-1/shard.log 2>&1 &
SHARD_1_PID=$!
echo -e "${GREEN}✓ Shard 1 starting (PID: $SHARD_1_PID) - will auto-register with coordinator${NC}"

# Optional: Start Shard 2 for 3-shard configuration
# Uncomment to enable third shard
# echo -e "${YELLOW}Starting Shard 2 on port 9102 (auto-registration)...${NC}"
# RUST_LOG=$RUST_LOG \
# ./arcxa-shard/target/${TARGET_DIR}/arcxa-shard \
#   --data-path ./data/shard-2 \
#   --port 9102 \
#   --coordinator-url http://localhost:50051 \
#   --heartbeat-interval 30 \
#   > ./data/shard-2/shard.log 2>&1 &
# SHARD_2_PID=$!
# echo -e "${GREEN}✓ Shard 2 starting (PID: $SHARD_2_PID) - will auto-register with coordinator${NC}"

# Wait for shards to be ready
echo -e "${YELLOW}Waiting for shards to be ready...${NC}"
sleep 3

echo ""
echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}Starting Model Service${NC}"
echo -e "${BLUE}========================================${NC}"
echo ""

# Start Model Service
# Note: Model service binary is in workspace target dir
# Check if ONNX Runtime is available
if [ ! -f "./onnxruntime/lib/libonnxruntime.so" ]; then
    echo -e "${YELLOW}ONNX Runtime not found. Downloading ONNX Runtime v1.22.0...${NC}"
    mkdir -p ./onnxruntime
    wget -q https://github.com/microsoft/onnxruntime/releases/download/v1.22.0/onnxruntime-linux-x64-1.22.0.tgz
    tar -xzf onnxruntime-linux-x64-1.22.0.tgz
    mv onnxruntime-linux-x64-1.22.0/lib ./onnxruntime/
    rm -rf onnxruntime-linux-x64-1.22.0.tgz onnxruntime-linux-x64-1.22.0
    echo -e "${GREEN}✓ ONNX Runtime downloaded to ./onnxruntime/lib${NC}"
fi

echo -e "${YELLOW}Starting Model Service on port 50051...${NC}"
RUST_LOG=$RUST_LOG \
GRAPHICA_MODEL_PATH=./models/minilm \
GRPC_PORT=50051 \
ORT_DYLIB_PATH=./onnxruntime/lib/libonnxruntime.so \
./target/${TARGET_DIR}/arcxa-model-service \
  > ./data/model-service.log 2>&1 &
MODEL_SERVICE_PID=$!
echo -e "${GREEN}✓ Model Service started (PID: $MODEL_SERVICE_PID)${NC}"

# Wait for model service to be ready
echo -e "${YELLOW}Waiting for model service to load...${NC}"
sleep 2

# Verify shards have registered
echo ""
echo -e "${YELLOW}Verifying shard registration...${NC}"
sleep 2  # Give shards time to register

# Check shard logs for registration confirmation
if grep -q "Successfully registered" ./data/shard-0/shard.log 2>/dev/null || \
   grep -q "Successfully reconnected" ./data/shard-0/shard.log 2>/dev/null; then
    echo -e "${GREEN}✓ Shard 0 registered with coordinator${NC}"
else
    echo -e "${RED}⚠ Shard 0 registration pending - check ./data/shard-0/shard.log${NC}"
fi

if grep -q "Successfully registered" ./data/shard-1/shard.log 2>/dev/null || \
   grep -q "Successfully reconnected" ./data/shard-1/shard.log 2>/dev/null; then
    echo -e "${GREEN}✓ Shard 1 registered with coordinator${NC}"
else
    echo -e "${RED}⚠ Shard 1 registration pending - check ./data/shard-1/shard.log${NC}"
fi

# Verify coordinator REST API is responsive
echo ""
echo -e "${YELLOW}Verifying coordinator REST API...${NC}"
for i in {1..10}; do
    if curl -s http://localhost:8082/health > /dev/null 2>&1; then
        echo -e "${GREEN}✓ Coordinator REST API is responsive${NC}"
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
echo -e "  Coordinator:   PID $COORDINATOR_PID ${GREEN}(audit + workflow persistence)${NC}"
echo -e "  Model Service: PID $MODEL_SERVICE_PID (port 50051)"
echo -e "  Shard 0:       PID $SHARD_0_PID (port 9100)"
echo -e "  Shard 1:       PID $SHARD_1_PID (port 9101)"
echo ""
echo -e "${BLUE}Endpoints:${NC}"
echo -e "  ${GREEN}REST API:${NC}  http://localhost:8082"
echo -e "  ${GREEN}gRPC API:${NC}  http://localhost:9090"
echo -e "  ${GREEN}Health:${NC}    http://localhost:8082/health"
echo -e "  ${GREEN}Metrics:${NC}   http://localhost:8082/metrics"
echo -e "  ${GREEN}OpenAPI:${NC}   http://localhost:8082/openapi.yaml"
echo ""
echo -e "${BLUE}Observability (start with docker compose):${NC}"
echo -e "  ${GREEN}Prometheus:${NC} http://localhost:9090"
echo -e "  ${GREEN}Grafana:${NC}    http://localhost:3000 (admin/graphica-admin)"
echo -e "  ${YELLOW}Start:${NC}      docker compose up -d prometheus grafana"
echo ""
echo -e "${BLUE}Distributed RDF Store (Auto-Registration):${NC}"
echo -e "  ${GREEN}Mode:${NC}                Production (2 shards, automatic registration)"
echo -e "  ${GREEN}Coordinator gRPC:${NC}   localhost:50051"
echo -e "  ${GREEN}Hash Distribution:${NC}  Automatic (0-50%, 50-100%)"
echo -e "  ${GREEN}Shard 0:${NC}            localhost:9100 (auto-assigned shard_id=0)"
echo -e "  ${GREEN}Shard 1:${NC}            localhost:9101 (auto-assigned shard_id=1)"
echo -e "  ${GREEN}Heartbeat:${NC}          Every 30 seconds with statistics"
echo -e "  ${GREEN}Identity Files:${NC}     ./data/shard-{0,1}/.graphica/shard_identity.json"
echo ""
echo -e "${BLUE}Cryptographic Audit Chain:${NC}"
echo -e "  ${GREEN}Status:${NC}     Enabled (SOX/HIPAA/GDPR compliance)"
echo -e "  ${GREEN}Storage:${NC}    ./data/audit (RocksDB)"
echo -e "  ${GREEN}Algorithm:${NC}  Ed25519 + SHA-256 + Merkle Tree"
echo -e "  ${GREEN}User ID:${NC}    local-coordinator"
echo ""
echo -e "${BLUE}Workflow Execution Persistence:${NC}"
echo -e "  ${GREEN}Status:${NC}              Enabled (RocksDB + Checkpointing)"
echo -e "  ${GREEN}Storage:${NC}             ./data/workflow-executions-db"
echo -e "  ${GREEN}Checkpoints:${NC}         Every 30 seconds"
echo -e "  ${GREEN}Recovery:${NC}            Automatic on restart"
echo -e "  ${GREEN}File Library:${NC}        ./data/file-library-db"
echo ""
echo -e "${BLUE}Logs:${NC}"
echo -e "  Coordinator:   ${YELLOW}tail -f ./data/coordinator/coordinator.log${NC}"
echo -e "  Model Service: ${YELLOW}tail -f ./data/model-service.log${NC}"
echo -e "  Shard 0:       ${YELLOW}tail -f ./data/shard-0/shard.log${NC}"
echo -e "  Shard 1:       ${YELLOW}tail -f ./data/shard-1/shard.log${NC}"
echo ""
echo -e "${BLUE}For HA with Raft Coordination:${NC}"
echo -e "  ${GREEN}./run-local-ha.sh${NC}  - 3 coordinators with Raft leader election"
echo ""
echo -e "${YELLOW}Press Ctrl+C to stop all services${NC}"
echo ""

# Wait for any process to exit
if [ -n "$SHARD_2_PID" ]; then
    wait $COORDINATOR_PID $MODEL_SERVICE_PID $SHARD_0_PID $SHARD_1_PID $SHARD_2_PID
else
    wait $COORDINATOR_PID $MODEL_SERVICE_PID $SHARD_0_PID $SHARD_1_PID
fi
