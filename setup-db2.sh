#!/bin/bash
# Setup DB2 Community Edition for Graphica Healthcare Demo

set -e

echo "============================================================"
echo "Setting up DB2 Community Edition for Graphica"
echo "============================================================"

# Stop and remove existing container and volumes
echo "Cleaning up any existing DB2 resources..."
docker-compose -f docker-compose-db2.yml down -v 2>/dev/null || true

# Remove any orphaned volumes
docker volume rm db2-data 2>/dev/null || true
docker volume rm graphica_db2-data 2>/dev/null || true

echo ""
echo "Starting DB2 container..."
docker-compose -f docker-compose-db2.yml up -d

echo ""
echo "Waiting for DB2 to initialize (this may take 2-3 minutes)..."
echo "DB2 Community Edition needs time to:"
echo "  1. Initialize instance"
echo "  2. Create database"
echo "  3. Configure settings"

# Wait for container to be healthy
MAX_WAIT=300  # 5 minutes
WAIT_TIME=0
SLEEP_INTERVAL=10

while [ $WAIT_TIME -lt $MAX_WAIT ]; do
    if docker exec graphica-db2 su - db2inst1 -c "db2 connect to GRAPHICA user db2inst1 using graphica-db2-pass" 2>&1 | grep -q "Database Connection Information"; then
        echo ""
        echo "DB2 is ready!"
        docker exec graphica-db2 su - db2inst1 -c "db2 connect reset" >/dev/null 2>&1
        break
    fi

    echo "Waiting... ($WAIT_TIME/${MAX_WAIT}s)"
    sleep $SLEEP_INTERVAL
    WAIT_TIME=$((WAIT_TIME + SLEEP_INTERVAL))
done

if [ $WAIT_TIME -ge $MAX_WAIT ]; then
    echo ""
    echo "ERROR: DB2 failed to start within $MAX_WAIT seconds"
    echo "Check logs with: docker logs graphica-db2"
    exit 1
fi

echo ""
echo "============================================================"
echo "DB2 Setup Complete!"
echo "============================================================"
echo ""
echo "Connection Details:"
echo "  Host:     localhost"
echo "  Port:     50000"
echo "  Database: GRAPHICA"
echo "  User:     db2inst1"
echo "  Password: graphica-db2-pass"
echo ""
echo "Test connection:"
echo "  docker exec graphica-db2 su - db2inst1 -c \"db2 connect to GRAPHICA && db2 'select 1 from sysibm.sysdummy1'\""
echo ""
echo "View logs:"
echo "  docker logs graphica-db2"
echo ""
echo "Stop DB2:"
echo "  docker-compose -f docker-compose-db2.yml down"
echo ""
