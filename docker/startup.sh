#!/usr/bin/env bash

###############################################################################
# Graphica Infrastructure Startup Script
#
# Production-ready script to start all external dependencies for Graphica
#
# Usage:
#   ./docker/startup.sh [PROFILE]
#
# Profiles:
#   minimal   - Core services only (Kafka, Zookeeper, Schema Registry)
#   dev       - Development (+ Kafka UI)
#   monitoring - With Prometheus/Grafana
#   fusion    - With PostgreSQL for fusion operations
#   timeseries - With TimescaleDB for time-series
#   cache     - With Redis for caching
#   full      - All services
#
# Environment Variables:
#   GRAPHICA_ENV - Environment (dev|staging|prod), default: dev
#   COMPOSE_PROJECT_NAME - Docker Compose project name, default: graphica
###############################################################################

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

# Configuration
GRAPHICA_ENV="${GRAPHICA_ENV:-dev}"
COMPOSE_PROJECT_NAME="${COMPOSE_PROJECT_NAME:-graphica}"
PROFILE="${1:-minimal}"
HEALTH_CHECK_TIMEOUT=300 # 5 minutes
HEALTH_CHECK_INTERVAL=5

# Logging functions
log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Error handler
error_handler() {
    log_error "Startup failed at line $1"
    log_info "Rolling back infrastructure..."
    cd "${PROJECT_ROOT}" && ${DOCKER_COMPOSE_CMD:-docker compose} -f "${PROJECT_ROOT}/docker-compose.yml" down --remove-orphans 2>/dev/null || true
    exit 1
}

trap 'error_handler $LINENO' ERR

# Check prerequisites
check_prerequisites() {
    log_info "Checking prerequisites..."

    # Check Docker
    if ! command -v docker &> /dev/null; then
        log_error "Docker is not installed. Please install Docker first."
        exit 1
    fi

    # Check Docker Compose (prefer v2)
    if docker compose version &> /dev/null; then
        DOCKER_COMPOSE_CMD="docker compose"
    elif command -v docker-compose &> /dev/null; then
        DOCKER_COMPOSE_CMD="docker-compose"
    else
        log_error "Docker Compose is not installed. Please install Docker Compose first."
        exit 1
    fi

    log_info "Using: ${DOCKER_COMPOSE_CMD}"

    # Check Docker daemon
    if ! docker info &> /dev/null; then
        log_error "Docker daemon is not running. Please start Docker."
        exit 1
    fi

    # Check disk space
    local available_space
    available_space=$(df -BG "${PROJECT_ROOT}" | tail -1 | awk '{print $4}' | sed 's/G//')
    if [[ ${available_space} -lt 10 ]]; then
        log_warning "Low disk space: ${available_space}GB available. At least 10GB recommended."
    fi

    log_success "Prerequisites check passed"
}

# Create required directories and config files
setup_config_files() {
    log_info "Setting up configuration files..."

    # Create directories
    mkdir -p "${PROJECT_ROOT}/docker/prometheus"
    mkdir -p "${PROJECT_ROOT}/docker/grafana/provisioning/dashboards"
    mkdir -p "${PROJECT_ROOT}/docker/grafana/provisioning/datasources"
    mkdir -p "${PROJECT_ROOT}/docker/postgres"
    mkdir -p "${PROJECT_ROOT}/docker/timescaledb"

    # Prometheus config
    if [[ ! -f "${PROJECT_ROOT}/docker/prometheus/prometheus.yml" ]]; then
        cat > "${PROJECT_ROOT}/docker/prometheus/prometheus.yml" << 'EOF'
global:
  scrape_interval: 15s
  evaluation_interval: 15s
  external_labels:
    cluster: 'graphica'
    environment: '${GRAPHICA_ENV}'

scrape_configs:
  - job_name: 'prometheus'
    static_configs:
      - targets: ['localhost:9090']

  - job_name: 'graphica'
    static_configs:
      - targets: ['host.docker.internal:8080']  # Graphica metrics endpoint
    scrape_interval: 10s

  - job_name: 'kafka'
    static_configs:
      - targets: ['kafka:9092']
    scrape_interval: 30s
EOF
    fi

    # Grafana datasource
    if [[ ! -f "${PROJECT_ROOT}/docker/grafana/provisioning/datasources/prometheus.yml" ]]; then
        cat > "${PROJECT_ROOT}/docker/grafana/provisioning/datasources/prometheus.yml" << 'EOF'
apiVersion: 1

datasources:
  - name: Prometheus
    type: prometheus
    access: proxy
    url: http://prometheus:9090
    isDefault: true
    editable: true
EOF
    fi

    # PostgreSQL init script
    if [[ ! -f "${PROJECT_ROOT}/docker/postgres/init.sql" ]]; then
        cat > "${PROJECT_ROOT}/docker/postgres/init.sql" << 'EOF'
-- Graphica PostgreSQL initialization
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- Fusion operations table
CREATE TABLE IF NOT EXISTS fusion_operations (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    fusion_id VARCHAR(255) UNIQUE NOT NULL,
    operation_type VARCHAR(50) NOT NULL,
    merged_entity_id VARCHAR(255) NOT NULL,
    confidence DOUBLE PRECISION,
    executed_at TIMESTAMPTZ DEFAULT NOW(),
    reversed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX idx_fusion_merged_entity ON fusion_operations(merged_entity_id);
CREATE INDEX idx_fusion_executed_at ON fusion_operations(executed_at);

-- Fusion participants table
CREATE TABLE IF NOT EXISTS fusion_participants (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    fusion_id UUID REFERENCES fusion_operations(id) ON DELETE CASCADE,
    entity_id VARCHAR(255) NOT NULL,
    role VARCHAR(50) NOT NULL, -- 'source' or 'merged'
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX idx_fusion_participants_entity ON fusion_participants(entity_id);
CREATE INDEX idx_fusion_participants_fusion ON fusion_participants(fusion_id);

-- Fusion rules table
CREATE TABLE IF NOT EXISTS fusion_rules (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    rule_id VARCHAR(255) UNIQUE NOT NULL,
    name VARCHAR(255) NOT NULL,
    priority INTEGER DEFAULT 0,
    match_criteria JSONB NOT NULL,
    merge_strategy JSONB NOT NULL,
    min_confidence DOUBLE PRECISION DEFAULT 0.0,
    require_human_review BOOLEAN DEFAULT false,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX idx_fusion_rules_priority ON fusion_rules(priority DESC);
EOF
    fi

    # TimescaleDB init script
    if [[ ! -f "${PROJECT_ROOT}/docker/timescaledb/init.sql" ]]; then
        cat > "${PROJECT_ROOT}/docker/timescaledb/init.sql" << 'EOF'
-- Graphica TimescaleDB initialization
CREATE EXTENSION IF NOT EXISTS timescaledb;

-- Attribute predictions time-series
CREATE TABLE IF NOT EXISTS attribute_predictions (
    time TIMESTAMPTZ NOT NULL,
    entity_id VARCHAR(255) NOT NULL,
    attribute_name VARCHAR(255) NOT NULL,
    value TEXT,
    confidence DOUBLE PRECISION,
    model_id VARCHAR(255),
    model_version VARCHAR(50),
    input_features JSONB,
    PRIMARY KEY (time, entity_id, attribute_name)
);

SELECT create_hypertable('attribute_predictions', 'time', if_not_exists => TRUE);

-- Retention policy: keep 2 years
SELECT add_retention_policy('attribute_predictions', INTERVAL '2 years', if_not_exists => TRUE);

-- Compression policy: compress data older than 30 days
SELECT add_compression_policy('attribute_predictions', INTERVAL '30 days', if_not_exists => TRUE);

-- Continuous aggregate: hourly summaries
CREATE MATERIALIZED VIEW IF NOT EXISTS attribute_predictions_hourly
WITH (timescaledb.continuous) AS
SELECT time_bucket('1 hour', time) AS bucket,
       entity_id,
       attribute_name,
       AVG(confidence) as avg_confidence,
       COUNT(*) as prediction_count
FROM attribute_predictions
GROUP BY bucket, entity_id, attribute_name;

-- Refresh policy for continuous aggregate
SELECT add_continuous_aggregate_policy('attribute_predictions_hourly',
    start_offset => INTERVAL '3 hours',
    end_offset => INTERVAL '1 hour',
    schedule_interval => INTERVAL '1 hour',
    if_not_exists => TRUE
);

-- Indexes
CREATE INDEX IF NOT EXISTS idx_attr_predictions_entity ON attribute_predictions(entity_id, time DESC);
CREATE INDEX IF NOT EXISTS idx_attr_predictions_model ON attribute_predictions(model_id, time DESC);
CREATE INDEX IF NOT EXISTS idx_attr_predictions_confidence ON attribute_predictions(confidence);
EOF
    fi

    log_success "Configuration files created"
}

# Determine Docker Compose profiles
get_compose_profiles() {
    local profile=$1
    local profiles=""

    case ${profile} in
        minimal)
            profiles=""
            ;;
        dev)
            profiles="--profile dev"
            ;;
        monitoring)
            profiles="--profile monitoring"
            ;;
        fusion)
            profiles="--profile fusion"
            ;;
        timeseries)
            profiles="--profile timeseries"
            ;;
        cache)
            profiles="--profile cache"
            ;;
        full)
            profiles="--profile dev --profile monitoring --profile full"
            ;;
        *)
            log_error "Unknown profile: ${profile}"
            log_info "Available profiles: minimal, dev, monitoring, fusion, timeseries, cache, full"
            exit 1
            ;;
    esac

    echo "${profiles}"
}

# Start services
start_services() {
    local profiles
    profiles=$(get_compose_profiles "${PROFILE}")

    log_info "Starting Graphica infrastructure with profile: ${PROFILE}"
    log_info "Environment: ${GRAPHICA_ENV}"

    cd "${PROJECT_ROOT}"

    # Pull latest images
    log_info "Pulling Docker images..."
    ${DOCKER_COMPOSE_CMD} pull ${profiles}

    # Start services
    log_info "Starting services..."
    GRAPHICA_ENV=${GRAPHICA_ENV} ${DOCKER_COMPOSE_CMD} up -d ${profiles}

    log_success "Services started"
}

# Wait for service health
wait_for_service() {
    local service=$1
    local port=$2
    local max_attempts=$((HEALTH_CHECK_TIMEOUT / HEALTH_CHECK_INTERVAL))
    local attempt=0

    log_info "Waiting for ${service} to be healthy..."

    while [[ ${attempt} -lt ${max_attempts} ]]; do
        if ${DOCKER_COMPOSE_CMD} ps ${service} 2>/dev/null | grep -q "healthy"; then
            log_success "${service} is healthy"
            return 0
        fi

        attempt=$((attempt + 1))
        echo -n "."
        sleep ${HEALTH_CHECK_INTERVAL}
    done

    echo ""
    log_error "${service} failed to become healthy within ${HEALTH_CHECK_TIMEOUT} seconds"
    ${DOCKER_COMPOSE_CMD} logs ${service} --tail=50
    return 1
}

# Health check all services
health_check() {
    log_info "Running health checks..."

    # Core services (always present)
    wait_for_service zookeeper 2181
    wait_for_service kafka 9092
    wait_for_service schema-registry 8081

    # Profile-specific services
    case ${PROFILE} in
        dev|full)
            wait_for_service kafka-ui 8080 || log_warning "Kafka UI not healthy (non-critical)"
            ;;
    esac

    case ${PROFILE} in
        monitoring|full)
            wait_for_service prometheus 9090 || log_warning "Prometheus not healthy"
            wait_for_service grafana 3000 || log_warning "Grafana not healthy"
            ;;
    esac

    case ${PROFILE} in
        fusion|full)
            wait_for_service postgres 5432 || log_warning "PostgreSQL not healthy"
            ;;
    esac

    case ${PROFILE} in
        timeseries|full)
            wait_for_service timescaledb 5432 || log_warning "TimescaleDB not healthy"
            ;;
    esac

    case ${PROFILE} in
        cache|full)
            wait_for_service redis 6379 || log_warning "Redis not healthy"
            ;;
    esac

    log_success "Health checks completed"
}

# Create Kafka topics
create_kafka_topics() {
    log_info "Creating Kafka topics..."

    local topics=(
        "graphica.lineage.events:3:1"
        "graphica.quality.violations:3:1"
        "graphica.model.predictions:3:1"
        "graphica.fusion.operations:3:1"
        "graphica.cdc.events:3:1"
        "graphica.dlq:3:1"
    )

    for topic_config in "${topics[@]}"; do
        IFS=':' read -r topic partitions replication <<< "${topic_config}"

        docker exec graphica-kafka kafka-topics --create \
            --if-not-exists \
            --bootstrap-server localhost:9092 \
            --topic "${topic}" \
            --partitions "${partitions}" \
            --replication-factor "${replication}" \
            --config retention.ms=604800000 \
            --config segment.ms=86400000 \
            2>/dev/null || log_warning "Topic ${topic} may already exist"
    done

    log_success "Kafka topics created"
}

# Show service endpoints
show_endpoints() {
    log_info "=== Graphica Infrastructure Endpoints ==="
    echo ""
    echo "Core Services:"
    echo "  Kafka Bootstrap:     localhost:9092"
    echo "  Schema Registry:     http://localhost:8081"
    echo "  Zookeeper:           localhost:2181"
    echo ""

    case ${PROFILE} in
        dev|full)
            echo "Development:"
            echo "  Kafka UI:            http://localhost:8080"
            echo ""
            ;;
    esac

    case ${PROFILE} in
        monitoring|full)
            echo "Monitoring:"
            echo "  Prometheus:          http://localhost:9090"
            echo "  Grafana:             http://localhost:3000 (admin/graphica-admin)"
            echo ""
            ;;
    esac

    case ${PROFILE} in
        fusion|full)
            echo "Fusion Storage:"
            echo "  PostgreSQL:          postgresql://graphica:graphica-secret@localhost:5432/graphica"
            echo ""
            ;;
    esac

    case ${PROFILE} in
        timeseries|full)
            echo "Time-Series Storage:"
            echo "  TimescaleDB:         postgresql://graphica:graphica-secret@localhost:5433/graphica_timeseries"
            echo ""
            ;;
    esac

    case ${PROFILE} in
        cache|full)
            echo "Cache:"
            echo "  Redis:               redis://localhost:6379"
            echo ""
            ;;
    esac

    echo "Environment Variables (add to .env or export):"
    echo "  export KAFKA_BROKERS=localhost:9092"
    echo "  export SCHEMA_REGISTRY_URL=http://localhost:8081"

    case ${PROFILE} in
        fusion|full)
            echo "  export FUSION_DB_URL=postgresql://graphica:graphica-secret@localhost:5432/graphica"
            ;;
    esac

    case ${PROFILE} in
        timeseries|full)
            echo "  export TIMESERIES_DB_URL=postgresql://graphica:graphica-secret@localhost:5433/graphica_timeseries"
            ;;
    esac

    case ${PROFILE} in
        cache|full)
            echo "  export REDIS_URL=redis://localhost:6379"
            ;;
    esac

    echo ""
}

# Main execution
main() {
    log_info "Graphica Infrastructure Startup"
    log_info "================================"
    echo ""

    check_prerequisites
    setup_config_files
    start_services
    health_check
    create_kafka_topics

    echo ""
    log_success "Graphica infrastructure is ready!"
    echo ""
    show_endpoints

    log_info "To view logs: docker-compose logs -f"
    log_info "To stop: ./docker/teardown.sh"
}

# Run main
main "$@"
