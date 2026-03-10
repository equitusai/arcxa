#!/bin/bash
set -e

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}Graphica Incremental Deployment Script${NC}"
echo -e "${GREEN}Phase 1.5 - All Fixes Deployed${NC}"
echo -e "${GREEN}========================================${NC}"
echo ""

# Function to wait for service health
wait_for_service() {
    local service=$1
    local max_attempts=60
    local attempt=0

    echo -e "${YELLOW}Waiting for $service to be healthy...${NC}"

    while [ $attempt -lt $max_attempts ]; do
        if docker compose ps $service | grep -q "(healthy)"; then
            echo -e "${GREEN}✓ $service is healthy${NC}"
            return 0
        fi

        attempt=$((attempt + 1))
        echo -n "."
        sleep 2
    done

    echo -e "${RED}✗ $service failed to become healthy${NC}"
    return 1
}

# Function to check HTTP endpoint
check_http() {
    local url=$1
    local name=$2

    echo -e "${YELLOW}Checking $name at $url...${NC}"

    if curl -sf $url > /dev/null; then
        echo -e "${GREEN}✓ $name is responding${NC}"
        return 0
    else
        echo -e "${RED}✗ $name is not responding${NC}"
        return 1
    fi
}

# Step 1: Start infrastructure services
echo -e "${YELLOW}Step 1: Starting infrastructure services (Kafka, Zookeeper, Schema Registry)...${NC}"
docker compose up -d zookeeper kafka schema-registry

wait_for_service zookeeper
wait_for_service kafka
wait_for_service schema-registry

echo ""
echo -e "${GREEN}✓ Infrastructure services are ready${NC}"
echo ""

# Step 2: Create Kafka topics
echo -e "${YELLOW}Step 2: Creating Kafka topics...${NC}"
docker exec graphica-kafka kafka-topics --create \
    --if-not-exists \
    --bootstrap-server localhost:9092 \
    --topic graphica.lineage \
    --partitions 4 \
    --replication-factor 1 || true

echo -e "${GREEN}✓ Kafka topics created${NC}"
echo ""

# Step 3: Build Graphica application
echo -e "${YELLOW}Step 3: Building Graphica application (this may take a few minutes)...${NC}"
docker compose build graphica

echo -e "${GREEN}✓ Graphica built successfully${NC}"
echo ""

# Step 4: Start Graphica
echo -e "${YELLOW}Step 4: Starting Graphica application...${NC}"
docker compose up -d graphica

wait_for_service graphica

echo -e "${GREEN}✓ Graphica is running${NC}"
echo ""

# Step 5: Verification tests
echo -e "${YELLOW}Step 5: Running verification tests...${NC}"
echo ""

# Wait a bit for full startup
sleep 5

# Test 1: Health check
check_http "http://localhost:8888/health" "Health endpoint"

# Test 2: API root
check_http "http://localhost:8888/api/v1/" "API root"

# Test 3: Metrics
check_http "http://localhost:9091/metrics" "Metrics endpoint"

echo ""

# Step 6: Show logs
echo -e "${YELLOW}Step 6: Checking Graphica logs for startup confirmation...${NC}"
echo ""
docker compose logs --tail=50 graphica | grep -E "(Starting|started|healthy|Ready)" || true

echo ""
echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}Deployment Summary${NC}"
echo -e "${GREEN}========================================${NC}"
echo ""
echo -e "Graphica REST API:    ${GREEN}http://localhost:8888${NC}"
echo -e "Graphica gRPC API:    ${GREEN}http://localhost:9090${NC}"
echo -e "Metrics:              ${GREEN}http://localhost:9091/metrics${NC}"
echo -e "Kafka UI:             ${GREEN}http://localhost:8080${NC} (run with --profile dev)"
echo ""
echo -e "${GREEN}Next steps:${NC}"
echo -e "1. Test the API: curl http://localhost:8888/health"
echo -e "2. View logs: docker-compose logs -f graphica"
echo -e "3. Send test event: scripts/send-test-event.sh"
echo ""
echo -e "${GREEN}✓ Deployment complete!${NC}"
