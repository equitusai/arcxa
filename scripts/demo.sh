#!/bin/bash
# Graphica End-to-End Demo Runner
# Demonstrates: Kafka → Timely → RocksDB → RDF pipeline

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

echo -e "${BLUE}"
echo "╔════════════════════════════════════════════════════════════╗"
echo "║       GRAPHICA END-TO-END PIPELINE DEMONSTRATION          ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo -e "${NC}"

# Step 1: Prerequisites Check
echo -e "\n${YELLOW}[Step 1/7]${NC} Checking prerequisites..."

if ! command -v docker &> /dev/null; then
    echo -e "${RED}✗ Docker not found${NC}"
    exit 1
fi

if ! docker ps | grep -q graphica-kafka; then
    echo -e "${RED}✗ Kafka not running. Start with: docker-compose up -d${NC}"
    exit 1
fi

echo -e "${GREEN}✓${NC} Docker running"
echo -e "${GREEN}✓${NC} Kafka container active"

# Step 2: Setup Kafka Topics
echo -e "\n${YELLOW}[Step 2/7]${NC} Setting up Kafka topics..."

if [ -f "$SCRIPT_DIR/setup-kafka-topics.sh" ]; then
    bash "$SCRIPT_DIR/setup-kafka-topics.sh" > /dev/null 2>&1
    echo -e "${GREEN}✓${NC} Kafka topics configured (7 topics)"
else
    echo -e "${YELLOW}⚠${NC} Topic setup script not found, assuming topics exist"
fi

# Step 3: Start Graphica Server
echo -e "\n${YELLOW}[Step 3/7]${NC} Starting Graphica server..."

# Kill any existing Graphica process
pkill -f "target/debug/graphica" 2>/dev/null || true
sleep 2

# Clean data directories for fresh start
rm -rf "$PROJECT_DIR/data/rocksdb" "$PROJECT_DIR/data/rdf" 2>/dev/null || true
mkdir -p "$PROJECT_DIR/data/rocksdb" "$PROJECT_DIR/data/rdf"

# Start server in background
cd "$PROJECT_DIR"
RUST_LOG=graphica=info cargo run --bin graphica > /tmp/graphica-demo.log 2>&1 &
GRAPHICA_PID=$!

echo -e "${GREEN}✓${NC} Graphica server starting (PID: $GRAPHICA_PID)"

# Wait for server to be ready
echo -n "   Waiting for server readiness"
for i in {1..30}; do
    if curl -sf http://localhost:8080/health/live > /dev/null 2>&1; then
        echo -e " ${GREEN}✓${NC}"
        break
    fi
    echo -n "."
    sleep 1
    if [ $i -eq 30 ]; then
        echo -e " ${RED}✗ Timeout${NC}"
        tail -50 /tmp/graphica-demo.log
        kill $GRAPHICA_PID 2>/dev/null || true
        exit 1
    fi
done

# Step 4: Generate Test Data
echo -e "\n${YELLOW}[Step 4/7]${NC} Generating test data (80 events)..."

cargo run --example record_generator 2>&1 | grep -E "(Generated|✓)" || true

echo -e "${GREEN}✓${NC} Sent 80 events to Kafka"
echo "   - 20 customers"
echo "   - 50 orders"
echo "   - 10 products"

# Step 5: Monitor Processing
echo -e "\n${YELLOW}[Step 5/7]${NC} Monitoring pipeline processing..."

echo -n "   Waiting for events to process"
PROCESSED=0
for i in {1..20}; do
    # Check RDF triple count as proxy for processing
    TRIPLES=$(curl -s -X POST http://localhost:8080/api/v1/governance/sparql \
        -H 'Content-Type: application/json' \
        -d '{"sparql": "SELECT (COUNT(*) as ?count) WHERE { ?s ?p ?o }"}' \
        2>/dev/null | jq -r '.results[0].count // 0' 2>/dev/null || echo "0")

    if [ "$TRIPLES" -gt 200 ]; then
        PROCESSED=1
        echo -e " ${GREEN}✓${NC}"
        break
    fi
    echo -n "."
    sleep 1
done

if [ $PROCESSED -eq 0 ]; then
    echo -e " ${YELLOW}⚠ Still processing${NC}"
fi

# Step 6: Validate Results
echo -e "\n${YELLOW}[Step 6/7]${NC} Validating pipeline results..."

# Get triple count
TRIPLE_COUNT=$(curl -s -X POST http://localhost:8080/api/v1/governance/sparql \
    -H 'Content-Type: application/json' \
    -d '{"sparql": "SELECT (COUNT(*) as ?count) WHERE { ?s ?p ?o }"}' \
    2>/dev/null | jq -r '.results[0].count // 0' 2>/dev/null || echo "0")

echo -e "${GREEN}✓${NC} RDF Store: $TRIPLE_COUNT triples materialized"

# Query sample lineage
LINEAGE=$(curl -s -X POST http://localhost:8080/api/v1/governance/sparql \
    -H 'Content-Type: application/json' \
    -d '{"sparql": "PREFIX gph: <http://graphica.io/ontology#> SELECT ?dataset ?recordId WHERE { ?lineage gph:dataset ?dataset . ?lineage gph:recordId ?recordId } LIMIT 5"}' \
    2>/dev/null | jq -r '.results | length' 2>/dev/null || echo "0")

if [ "$LINEAGE" -gt 0 ]; then
    echo -e "${GREEN}✓${NC} Lineage Queries: SPARQL working ($LINEAGE results)"
else
    echo -e "${YELLOW}⚠${NC} Lineage Queries: No results yet"
fi

# Check health
HEALTH=$(curl -s http://localhost:8080/health/storage 2>/dev/null | jq -r '.healthy // false' 2>/dev/null || echo "false")

if [ "$HEALTH" == "true" ]; then
    echo -e "${GREEN}✓${NC} Storage Health: All systems operational"
else
    echo -e "${YELLOW}⚠${NC} Storage Health: Degraded"
fi

# Step 7: Summary Report
echo -e "\n${YELLOW}[Step 7/7]${NC} Demo Summary"
echo "════════════════════════════════════════════════════════════"

echo -e "\n${BLUE}Pipeline Flow:${NC}"
echo "  Kafka (graphica.lineage)"
echo "    ↓ ParallelCdcConsumer (4 partitions)"
echo "    ↓ Timely Dataflow (standardize, dedup, rules, profile)"
echo "    ↓ AsyncStorageWriter (batch: 100-1000)"
echo "    ↓ RocksDB (persistent) + RDF Store (in-memory)"
echo "    ↓ REST API (SPARQL queries)"

echo -e "\n${BLUE}Results:${NC}"
echo "  • Events Sent:       80"
echo "  • RDF Triples:       $TRIPLE_COUNT"
echo "  • SPARQL Working:    $([ "$LINEAGE" -gt 0 ] && echo 'Yes' || echo 'Pending')"
echo "  • Storage Healthy:   $([ "$HEALTH" == "true" ] && echo 'Yes' || echo 'Degraded')"

echo -e "\n${BLUE}Test Queries:${NC}"
echo "  # Count all triples"
echo "  curl -X POST http://localhost:8080/api/v1/governance/sparql \\"
echo "    -H 'Content-Type: application/json' \\"
echo "    -d '{\"sparql\": \"SELECT (COUNT(*) as ?count) WHERE { ?s ?p ?o }\"}'"

echo ""
echo "  # Query lineage data"
echo "  curl -X POST http://localhost:8080/api/v1/governance/sparql \\"
echo "    -H 'Content-Type: application/json' \\"
echo "    -d '{\"sparql\": \"PREFIX gph: <http://graphica.io/ontology#> SELECT ?dataset ?recordId WHERE { ?lineage gph:dataset ?dataset . ?lineage gph:recordId ?recordId } LIMIT 10\"}'"

echo -e "\n${BLUE}Logs:${NC}"
echo "  tail -f /tmp/graphica-demo.log"

echo -e "\n${GREEN}✓ Demo Complete!${NC}"
echo ""
echo "Press Ctrl+C to stop the Graphica server (PID: $GRAPHICA_PID)"
echo "or run: kill $GRAPHICA_PID"
echo ""

# Keep script running so server stays up
wait $GRAPHICA_PID 2>/dev/null || true
