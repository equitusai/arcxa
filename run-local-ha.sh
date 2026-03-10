#!/bin/bash
# Run ARCXA with HA Coordinator Cluster (3 coordinators with Raft consensus)
# This script demonstrates the high-availability architecture with automatic failover

set -e

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
RED='\033[0;31m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

echo -e "${CYAN}========================================${NC}"
echo -e "${CYAN}ARCXA HA Cluster (3 Coordinators)${NC}"
echo -e "${CYAN}========================================${NC}"
echo ""

# Check if coordinator was built with HA support
if ! ./arcxa-coordinator/target/release/arcxa-coordinator --version 2>&1 | grep -q "coordinator"; then
    echo -e "${RED}Error: Coordinator binary not found!${NC}"
    echo -e "${YELLOW}Build with HA support: ${GREEN}ENABLE_HA=true ./build.sh${NC}"
    exit 1
fi

# Cleanup any existing processes and locks
echo -e "${YELLOW}Cleaning up any existing ARCXA processes...${NC}"
pkill -9 -f arcxa-coordinator 2>/dev/null || true
pkill -9 -f arcxa-shard 2>/dev/null || true
pkill -9 -f arcxa-model-service 2>/dev/null || true

echo -e "${YELLOW}Removing stale RocksDB locks...${NC}"
find ./data -name "LOCK" -type f -delete 2>/dev/null || true

echo -e "${YELLOW}Wiping file library database (schema changes)...${NC}"
rm -rf ./data/file-library-db 2>/dev/null || true

echo -e "${YELLOW}Wiping workflow execution databases (fresh start)...${NC}"
rm -rf ./data/coordinator-1/workflow-executions 2>/dev/null || true
rm -rf ./data/coordinator-2/workflow-executions 2>/dev/null || true
rm -rf ./data/coordinator-3/workflow-executions 2>/dev/null || true

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

# Set environment variables
export RUST_LOG=${RUST_LOG:-info}
export KAFKA_BROKERS=${KAFKA_BROKERS:-localhost:9092}
export ENABLE_AUTH=${ENABLE_AUTH:-false}
export ENVIRONMENT=${ENVIRONMENT:-development}

# Create data directories
mkdir -p ./data/shard-0 ./data/shard-1 ./data/shard-2
mkdir -p ./data/coordinator-1/rocksdb ./data/coordinator-1/raft ./data/coordinator-1/workflow-executions
mkdir -p ./data/coordinator-2/rocksdb ./data/coordinator-2/raft ./data/coordinator-2/workflow-executions
mkdir -p ./data/coordinator-3/rocksdb ./data/coordinator-3/raft ./data/coordinator-3/workflow-executions
mkdir -p ./data/parquet ./data/archive ./data/audit
mkdir -p ./data/model-service ./data/file-library

echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}Starting Shards${NC}"
echo -e "${BLUE}========================================${NC}"
echo ""

# Note: Shards will automatically register with coordinator via gRPC
# - Coordinator assigns shard IDs dynamically
# - Hash ranges distributed evenly (3 shards: 0-33%, 33-66%, 66-100%)
# - Persistent identity survives restarts

# Start Shard 0 (will auto-register and receive shard_id=0)
echo -e "${YELLOW}Starting Shard 0 on port 9100 (auto-registration)...${NC}"
RUST_LOG=$RUST_LOG \
./arcxa-shard/target/release/arcxa-shard \
  --data-path ./data/shard-0 \
  --port 9100 \
  --coordinator-url http://localhost:9091 \
  --heartbeat-interval 30 \
  > ./data/shard-0/shard.log 2>&1 &
SHARD_0_PID=$!
echo -e "${GREEN}✓ Shard 0 starting (PID: $SHARD_0_PID) - will auto-register with coordinator${NC}"

# Start Shard 1 (will auto-register and receive shard_id=1)
echo -e "${YELLOW}Starting Shard 1 on port 9101 (auto-registration)...${NC}"
RUST_LOG=$RUST_LOG \
./arcxa-shard/target/release/arcxa-shard \
  --data-path ./data/shard-1 \
  --port 9101 \
  --coordinator-url http://localhost:9091 \
  --heartbeat-interval 30 \
  > ./data/shard-1/shard.log 2>&1 &
SHARD_1_PID=$!
echo -e "${GREEN}✓ Shard 1 starting (PID: $SHARD_1_PID) - will auto-register with coordinator${NC}"

# Start Shard 2 (will auto-register and receive shard_id=2)
echo -e "${YELLOW}Starting Shard 2 on port 9102 (auto-registration)...${NC}"
RUST_LOG=$RUST_LOG \
./arcxa-shard/target/release/arcxa-shard \
  --data-path ./data/shard-2 \
  --port 9102 \
  --coordinator-url http://localhost:9091 \
  --heartbeat-interval 30 \
  > ./data/shard-2/shard.log 2>&1 &
SHARD_2_PID=$!
echo -e "${GREEN}✓ Shard 2 starting (PID: $SHARD_2_PID) - will auto-register with coordinator${NC}"

# Wait for shards to be ready
echo -e "${YELLOW}Waiting for shards to be ready...${NC}"
sleep 3

echo ""
echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}Starting Model Service${NC}"
echo -e "${BLUE}========================================${NC}"
echo ""

# Start Model Service
echo -e "${YELLOW}Starting Model Service on port 50051...${NC}"
RUST_LOG=$RUST_LOG \
GRAPHICA_MODEL_PATH=./models/minilm \
GRPC_PORT=50051 \
LD_LIBRARY_PATH=./arcxa-model-service/target/release:$LD_LIBRARY_PATH \
./arcxa-model-service/target/release/arcxa-model-service \
  > ./data/model-service/service.log 2>&1 &
MODEL_SERVICE_PID=$!
echo -e "${GREEN}✓ Model Service started (PID: $MODEL_SERVICE_PID)${NC}"

# Wait for model service to be ready
echo -e "${YELLOW}Waiting for model service to load...${NC}"
sleep 2

echo ""
echo -e "${CYAN}========================================${NC}"
echo -e "${CYAN}Starting HA Coordinator Cluster${NC}"
echo -e "${CYAN}========================================${NC}"
echo ""

# Coordinator 1 (Node ID 1) - port 8081
echo -e "${YELLOW}Starting Coordinator 1 (Leader candidate) on port 8081...${NC}"
RUST_LOG=$RUST_LOG \
ENVIRONMENT=$ENVIRONMENT \
SHARD_URLS="localhost:9100,localhost:9101,localhost:9102" \
GRAPHICA_MODEL_SERVICE_URL="http://localhost:50051" \
REST_PORT=8081 \
GRPC_PORT=9091 \
ROCKSDB_PATH=./data/coordinator-1/rocksdb \
PARQUET_PATH=./data/parquet \
ARCHIVE_PATH=./data/archive \
ENABLE_CRYPTOGRAPHIC_AUDIT=true \
AUDIT_CHAIN_PATH=./data/audit/coordinator-1 \
AUDIT_USER_ID="coordinator-1" \
KAFKA_BROKERS=$KAFKA_BROKERS \
ENABLE_AUTH=$ENABLE_AUTH \
WORKFLOW_EXECUTION_DB_PATH=./data/coordinator-1/workflow-executions \
FILE_LIBRARY_DB_PATH=./data/file-library-db \
CHECKPOINT_ENABLED=true \
CHECKPOINT_INTERVAL_SECS=30 \
RAFT_COORDINATOR_ID="coordinator-1" \
RAFT_PEER_URLS="http://localhost:8081,http://localhost:8082,http://localhost:8083" \
RAFT_NODE_ID=1 \
RAFT_STORAGE_PATH=./data/coordinator-1/raft \
RAFT_PEER_2="localhost:9092" \
RAFT_PEER_3="localhost:9093" \
./arcxa-coordinator/target/release/arcxa-coordinator > ./data/coordinator-1/coordinator.log 2>&1 &
COORDINATOR_1_PID=$!
echo -e "${GREEN}✓ Coordinator 1 started (PID: $COORDINATOR_1_PID, Node ID: 1)${NC}"

# Coordinator 2 (Node ID 2) - port 8082
echo -e "${YELLOW}Starting Coordinator 2 (Follower) on port 8082...${NC}"
RUST_LOG=$RUST_LOG \
ENVIRONMENT=$ENVIRONMENT \
SHARD_URLS="localhost:9100,localhost:9101,localhost:9102" \
GRAPHICA_MODEL_SERVICE_URL="http://localhost:50051" \
REST_PORT=8082 \
GRPC_PORT=9092 \
ROCKSDB_PATH=./data/coordinator-2/rocksdb \
PARQUET_PATH=./data/parquet \
ARCHIVE_PATH=./data/archive \
ENABLE_CRYPTOGRAPHIC_AUDIT=true \
AUDIT_CHAIN_PATH=./data/audit/coordinator-2 \
AUDIT_USER_ID="coordinator-2" \
KAFKA_BROKERS=$KAFKA_BROKERS \
ENABLE_AUTH=$ENABLE_AUTH \
WORKFLOW_EXECUTION_DB_PATH=./data/coordinator-2/workflow-executions \
FILE_LIBRARY_DB_PATH=./data/file-library-db \
CHECKPOINT_ENABLED=true \
CHECKPOINT_INTERVAL_SECS=30 \
RAFT_COORDINATOR_ID="coordinator-2" \
RAFT_PEER_URLS="http://localhost:8081,http://localhost:8082,http://localhost:8083" \
RAFT_NODE_ID=2 \
RAFT_STORAGE_PATH=./data/coordinator-2/raft \
RAFT_PEER_1="localhost:9091" \
RAFT_PEER_3="localhost:9093" \
./arcxa-coordinator/target/release/arcxa-coordinator > ./data/coordinator-2/coordinator.log 2>&1 &
COORDINATOR_2_PID=$!
echo -e "${GREEN}✓ Coordinator 2 started (PID: $COORDINATOR_2_PID, Node ID: 2)${NC}"

# Coordinator 3 (Node ID 3) - port 8083
echo -e "${YELLOW}Starting Coordinator 3 (Follower) on port 8083...${NC}"
RUST_LOG=$RUST_LOG \
ENVIRONMENT=$ENVIRONMENT \
SHARD_URLS="localhost:9100,localhost:9101,localhost:9102" \
GRAPHICA_MODEL_SERVICE_URL="http://localhost:50051" \
REST_PORT=8083 \
GRPC_PORT=9093 \
ROCKSDB_PATH=./data/coordinator-3/rocksdb \
PARQUET_PATH=./data/parquet \
ARCHIVE_PATH=./data/archive \
ENABLE_CRYPTOGRAPHIC_AUDIT=true \
AUDIT_CHAIN_PATH=./data/audit/coordinator-3 \
AUDIT_USER_ID="coordinator-3" \
KAFKA_BROKERS=$KAFKA_BROKERS \
ENABLE_AUTH=$ENABLE_AUTH \
WORKFLOW_EXECUTION_DB_PATH=./data/coordinator-3/workflow-executions \
FILE_LIBRARY_DB_PATH=./data/file-library-db \
CHECKPOINT_ENABLED=true \
CHECKPOINT_INTERVAL_SECS=30 \
RAFT_COORDINATOR_ID="coordinator-3" \
RAFT_PEER_URLS="http://localhost:8081,http://localhost:8082,http://localhost:8083" \
RAFT_NODE_ID=3 \
RAFT_STORAGE_PATH=./data/coordinator-3/raft \
RAFT_PEER_1="localhost:9091" \
RAFT_PEER_2="localhost:9092" \
./arcxa-coordinator/target/release/arcxa-coordinator > ./data/coordinator-3/coordinator.log 2>&1 &
COORDINATOR_3_PID=$!
echo -e "${GREEN}✓ Coordinator 3 started (PID: $COORDINATOR_3_PID, Node ID: 3)${NC}"

echo ""
echo -e "${YELLOW}Waiting for Raft cluster to elect leader...${NC}"
sleep 5

echo ""
echo -e "${CYAN}========================================${NC}"
echo -e "${CYAN}ARCXA HA Cluster is Running!${NC}"
echo -e "${CYAN}========================================${NC}"
echo ""
echo -e "${BLUE}Architecture:${NC}"
echo -e "  ${CYAN}Coordinator Cluster (Raft):${NC}"
echo -e "    Coordinator 1: PID $COORDINATOR_1_PID ${GREEN}(Node ID: 1, port 8081)${NC}"
echo -e "    Coordinator 2: PID $COORDINATOR_2_PID ${GREEN}(Node ID: 2, port 8082)${NC}"
echo -e "    Coordinator 3: PID $COORDINATOR_3_PID ${GREEN}(Node ID: 3, port 8083)${NC}"
echo -e "  ${BLUE}Shards (Auto-Registered):${NC}"
echo -e "    Shard 0: PID $SHARD_0_PID (port 9100) ${GREEN}[auto-assigned shard_id=0]${NC}"
echo -e "    Shard 1: PID $SHARD_1_PID (port 9101) ${GREEN}[auto-assigned shard_id=1]${NC}"
echo -e "    Shard 2: PID $SHARD_2_PID (port 9102) ${GREEN}[auto-assigned shard_id=2]${NC}"
echo -e "  ${BLUE}Model Service:${NC}"
echo -e "    PID $MODEL_SERVICE_PID (port 50051)"
echo ""
echo -e "${BLUE}Coordinator Endpoints:${NC}"
echo -e "  ${GREEN}Coordinator 1:${NC} http://localhost:8081 ${CYAN}(Health: /health, Metrics: /metrics)${NC}"
echo -e "  ${GREEN}Coordinator 2:${NC} http://localhost:8082 ${CYAN}(Health: /health, Metrics: /metrics)${NC}"
echo -e "  ${GREEN}Coordinator 3:${NC} http://localhost:8083 ${CYAN}(Health: /health, Metrics: /metrics)${NC}"
echo ""
echo -e "${BLUE}HA Features:${NC}"
echo -e "  ${GREEN}Leader Election:${NC}        Automatic (Raft consensus)"
echo -e "  ${GREEN}Kafka Replay Coord:${NC}     Distributed with Raft leader election"
echo -e "  ${GREEN}Failover Time:${NC}          < 1 second"
echo -e "  ${GREEN}Consistent Hashing:${NC}     150 virtual nodes per shard"
echo -e "  ${GREEN}Circuit Breakers:${NC}       Enabled (prevents cascading failures)"
echo -e "  ${GREEN}Shard Rebalancing:${NC}      Automatic (20% imbalance threshold)"
echo -e "  ${GREEN}Workflow Persistence:${NC}   RocksDB + Checkpointing (30s interval)"
echo -e "  ${GREEN}Cryptographic Audit:${NC}    Enabled (SOX/HIPAA/GDPR compliance)"
echo ""
echo -e "${BLUE}Testing HA:${NC}"
echo -e "  ${YELLOW}1. Send requests to any coordinator (they auto-forward to leader)${NC}"
echo -e "  ${YELLOW}2. Kill the leader: kill \$COORDINATOR_1_PID${NC}"
echo -e "  ${YELLOW}3. Watch automatic failover (< 1 second)${NC}"
echo -e "  ${YELLOW}4. Requests continue working on remaining coordinators${NC}"
echo ""
echo -e "${BLUE}Check Raft Status:${NC}"
echo -e "  ${YELLOW}# Check Kafka replay coordination Raft state${NC}"
echo -e "  ${YELLOW}curl http://localhost:8081/kafka/raft/state | jq${NC}"
echo -e "  ${YELLOW}curl http://localhost:8082/kafka/raft/state | jq${NC}"
echo -e "  ${YELLOW}curl http://localhost:8083/kafka/raft/state | jq${NC}"
echo -e "  ${YELLOW}# Check Raft log entries${NC}"
echo -e "  ${YELLOW}curl http://localhost:8081/kafka/raft/log | jq${NC}"
echo ""
echo -e "${BLUE}Logs:${NC}"
echo -e "  Coordinator 1: ${YELLOW}tail -f ./data/coordinator-1/coordinator.log${NC}"
echo -e "  Coordinator 2: ${YELLOW}tail -f ./data/coordinator-2/coordinator.log${NC}"
echo -e "  Coordinator 3: ${YELLOW}tail -f ./data/coordinator-3/coordinator.log${NC}"
echo -e "  Model Service: ${YELLOW}tail -f ./data/model-service/service.log${NC}"
echo -e "  Shard 0:       ${YELLOW}tail -f ./data/shard-0/shard.log${NC}"
echo -e "  Shard 1:       ${YELLOW}tail -f ./data/shard-1/shard.log${NC}"
echo -e "  Shard 2:       ${YELLOW}tail -f ./data/shard-2/shard.log${NC}"
echo ""
echo -e "${YELLOW}Press Ctrl+C to stop all services${NC}"
echo ""

# Cleanup function
cleanup() {
    echo ""
    echo -e "${YELLOW}Shutting down HA cluster...${NC}"
    kill $COORDINATOR_1_PID $COORDINATOR_2_PID $COORDINATOR_3_PID 2>/dev/null || true
    kill $SHARD_0_PID $SHARD_1_PID $SHARD_2_PID 2>/dev/null || true
    kill $MODEL_SERVICE_PID 2>/dev/null || true
    wait $COORDINATOR_1_PID $COORDINATOR_2_PID $COORDINATOR_3_PID 2>/dev/null || true
    wait $SHARD_0_PID $SHARD_1_PID $SHARD_2_PID 2>/dev/null || true
    wait $MODEL_SERVICE_PID 2>/dev/null || true
    echo -e "${GREEN}✓ Shutdown complete${NC}"
    exit 0
}

trap cleanup SIGINT SIGTERM

# Wait for any process to exit
wait $COORDINATOR_1_PID $COORDINATOR_2_PID $COORDINATOR_3_PID $MODEL_SERVICE_PID $SHARD_0_PID $SHARD_1_PID $SHARD_2_PID
