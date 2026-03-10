#!/bin/bash

# Generate JWT secret if not already set
if [ -z "$JWT_SECRET" ]; then
    export JWT_SECRET=$(openssl rand -base64 32)
    echo "Generated JWT_SECRET: $JWT_SECRET"
    echo "Save this for future use!"
    echo ""
fi

# Change to coordinator directory
cd /root/graphica/graphica/arcxa-coordinator

# Check if shards are running
echo "Checking shard connectivity..."
if ! nc -z localhost 50051 2>/dev/null; then
    echo "⚠️  Warning: Shard 1 (port 50051) not reachable"
fi
if ! nc -z localhost 50052 2>/dev/null; then
    echo "⚠️  Warning: Shard 2 (port 50052) not reachable"
fi

echo ""
echo "Starting coordinator..."
echo "REST API:    http://localhost:8080"
echo "gRPC API:    http://localhost:9090"
echo "Metrics:     http://localhost:8080/metrics"
echo "Prometheus:  http://localhost:9090 (if running)"
echo "Grafana:     http://localhost:3000 (if running)"
echo ""

# Start coordinator
./target/release/arcxa-coordinator \
  --rest-port 8080 \
  --grpc-port 9090 \
  --shard-urls "localhost:50051,localhost:50052" \
  --shard-count 2 \
  --rocksdb-path ./data/coordinator/rocksdb

echo ""
echo "⚠️  Look for the SETUP TOKEN in the output above"
echo "You'll need it to create the first admin user"
