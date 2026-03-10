#!/bin/bash

# Coordinator startup script with test lineage API enabled

export ENABLE_TEST_LINEAGE_API=true
export RUST_LOG=info
export ENVIRONMENT=development
export GRAPHICA_MODEL_SERVICE_URL="http://localhost:50051"
export REST_PORT=8080
export GRPC_PORT=50051
export ROCKSDB_PATH=./data/coordinator/rocksdb
export PARQUET_PATH=./data/parquet
export ARCHIVE_PATH=./data/archive
export ENABLE_CRYPTOGRAPHIC_AUDIT=true
export AUDIT_CHAIN_PATH=./data/audit
export AUDIT_USER_ID="local-coordinator"
export KAFKA_BROKERS=localhost:9092
export KAFKA_CONSUMER_GROUP=arcxa-coordinator
export KAFKA_LINEAGE_TOPIC=graphica.lineage
export KAFKA_QUALITY_TOPIC=graphica.quality
export ENABLE_AUTH=false
export WORKFLOW_EXECUTION_DB_PATH=./data/workflow-executions-db
export FILE_LIBRARY_DB_PATH=./data/file-library-db
export CHECKPOINT_ENABLED=true
export CHECKPOINT_INTERVAL_SECS=30

exec ./target/release/arcxa-coordinator
