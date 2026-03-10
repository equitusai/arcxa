#!/bin/bash
# Setup Kafka topics for Graphica demo pipeline

set -e

KAFKA_CONTAINER="graphica-kafka"
BOOTSTRAP_SERVER="localhost:9092"

echo "🚀 Setting up Kafka topics for Graphica..."

# Function to create topic
create_topic() {
    local topic=$1
    local partitions=$2
    local retention_ms=$3
    local compression=$4

    echo "Creating topic: $topic (partitions=$partitions)"

    docker exec $KAFKA_CONTAINER kafka-topics \
        --create \
        --topic "$topic" \
        --bootstrap-server $BOOTSTRAP_SERVER \
        --partitions "$partitions" \
        --replication-factor 1 \
        --config retention.ms="$retention_ms" \
        --config compression.type="$compression" \
        --if-not-exists
}

# Main lineage topic (what Graphica consumes)
create_topic "graphica.lineage" 4 604800000 "snappy"  # 7 days retention

# CDC source topics (simulating Debezium)
create_topic "cdc.retail.customers" 4 86400000 "snappy"  # 1 day
create_topic "cdc.retail.orders" 4 86400000 "snappy"
create_topic "cdc.retail.products" 4 86400000 "snappy"
create_topic "cdc.retail.inventory" 4 86400000 "snappy"

# Dead letter queue
create_topic "graphica.dlq" 1 2592000000 "gzip"  # 30 days

# Quality violations
create_topic "graphica.quality.violations" 4 604800000 "snappy"  # 7 days

echo ""
echo "✅ Topic creation complete!"
echo ""
echo "📊 Listing all Graphica topics:"
docker exec $KAFKA_CONTAINER kafka-topics \
    --list \
    --bootstrap-server $BOOTSTRAP_SERVER \
    | grep -E "(graphica|cdc\.retail)"

echo ""
echo "🔍 Topic details:"
for topic in "graphica.lineage" "cdc.retail.customers" "cdc.retail.orders"; do
    echo ""
    echo "Topic: $topic"
    docker exec $KAFKA_CONTAINER kafka-topics \
        --describe \
        --topic "$topic" \
        --bootstrap-server $BOOTSTRAP_SERVER
done

echo ""
echo "✨ Kafka topics ready for demo pipeline!"
